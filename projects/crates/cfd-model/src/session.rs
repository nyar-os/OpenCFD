//! Document session API shared by the CLI and the Node-API surface.
//!
//! [`OpenCfd`] owns one `.ocfd` document and its file path. Every edit runs the
//! same validation and atomic write-back the CLI used to perform inline, so the
//! command line, the native addon and the TypeScript SDK cannot drift apart.
//!
//! The API is intentionally small and orthogonal: one method performs one
//! operation on the current document and returns a serialisable report. Argument
//! parsing, help text and process exit codes stay in the CLI layer.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Serialize;

use crate::checkpoint::Checkpoint;
use crate::export::{
    export_coefficients, export_contours, export_csv_with_layout, export_forces, export_probes,
    export_profile as write_profile, export_residuals, export_statistics as write_statistics,
    export_streamlines, export_svg, export_vtk_with_layout, ExportField, ExportLayout, ProfileLine,
};
use crate::mesh::Mesh;
use crate::model::{
    Boundaries, Boundary, BoundaryKind, Buoyancy, ConvectionScheme, Fluid, InitialField,
    InletProfile, MeshConfig, Model, Obstacle, PorousRegion, Probe, Reference, ScalarConfig,
    ScalarSource, Side, ThermalConfig, TimeScheme, TimeSettings, TransientInlet, Turbulence,
    TurbulenceModel, ViscosityModel, WallFunction,
};
use crate::particles::{
    export_particles as write_particles, ParticleConfig, DEFAULT_PARTICLE_STEPS,
};
use crate::quality::{mesh_quality as measure_quality, MeshQuality};
use crate::solver::{
    reynolds_number, solve, solve_with, solve_with_checkpoint as solve_resuming, stability_limits,
    Report, ReynoldsNumber, SolveOptions, StabilityLimits,
};
use crate::{CfdError, CAPABILITIES_JSON};

/// Exchange formats supported by [`OpenCfd::export`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Cell-centred CSV rows.
    Csv,
    /// Legacy ASCII `UNSTRUCTURED_GRID` VTK over the cell centres.
    Vtk,
    /// Self-contained SVG preview of one cell-centred field.
    Svg,
    /// Self-contained SVG of filled contours and iso-lines.
    Contours,
    /// Self-contained SVG of traced streamlines.
    Streamlines,
    /// Sampled residual-history CSV rows.
    Residuals,
    /// Sampled obstacle force-history CSV rows.
    Forces,
    /// Final per-obstacle drag, lift and moment summary CSV rows.
    Coefficients,
    /// Sampled point-monitor time-series CSV rows.
    Probes,
    /// Traced particle trajectories as CSV rows.
    Particles,
    /// One cell-centred field sampled along a straight segment as CSV rows.
    Profile,
    /// Per-field statistics over the fluid cells as a JSON document.
    Statistics,
}

impl ExportFormat {
    /// Returns the lowercase identifier used by the CLI and the SDK.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Vtk => "vtk",
            Self::Svg => "svg",
            Self::Contours => "contours",
            Self::Streamlines => "streamlines",
            Self::Residuals => "residuals",
            Self::Forces => "forces",
            Self::Coefficients => "coefficients",
            Self::Probes => "probes",
            Self::Particles => "particles",
            Self::Profile => "profile",
            Self::Statistics => "statistics",
        }
    }
}

impl FromStr for ExportFormat {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "csv" => Ok(Self::Csv),
            "vtk" => Ok(Self::Vtk),
            "svg" => Ok(Self::Svg),
            "contours" | "contour" => Ok(Self::Contours),
            "streamlines" | "streamline" => Ok(Self::Streamlines),
            "residuals" | "residual" => Ok(Self::Residuals),
            "forces" | "force" => Ok(Self::Forces),
            "coefficients" | "coefficient" | "cdcl" => Ok(Self::Coefficients),
            "probes" | "probe" => Ok(Self::Probes),
            "particles" | "particle" | "tracers" => Ok(Self::Particles),
            "profile" | "profiles" => Ok(Self::Profile),
            "statistics" | "stats" => Ok(Self::Statistics),
            "inp" | "openfoam" => Err(CfdError::new(
                "capability/not-implemented",
                format!(
                    "export format {value} is not implemented; only csv, vtk, svg, contours, streamlines, residuals, forces, coefficients, probes, particles, profile or statistics are supported"
                ),
            )),
            other => Err(CfdError::new(
                "cli/unknown-format",
                format!(
                    "unknown export format `{other}`; only csv, vtk, svg, contours, streamlines, residuals, forces, coefficients, probes, particles, profile or statistics are supported"
                ),
            )),
        }
    }
}

/// Options of [`OpenCfd::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCfdOptions {
    /// BCP-47 tag used for diagnostics and default copy. Defaults to `en-US`.
    pub locale: String,
}

impl Default for OpenCfdOptions {
    fn default() -> Self {
        Self {
            locale: "en-US".to_string(),
        }
    }
}

/// Snapshot of one open case document.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseSummary {
    /// Stable document identity; currently the absolute or relative file path.
    pub id: String,
    /// File the document is bound to.
    pub path: String,
    /// Document format tag.
    pub format: String,
    /// Document version.
    pub version: u32,
    /// Structured mesh.
    pub mesh: MeshConfig,
    /// Fluid properties.
    pub fluid: Fluid,
    /// Optional force and moment reference quantities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    /// Convective discretisation of the momentum and scalar equations.
    pub convection: ConvectionScheme,
    /// Uniform imposed pressure gradient `[dp/dx, dp/dy]` (Pa/m), when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pressure_gradient: Option<[f64; 2]>,
    /// Time integration settings.
    pub time: TimeSettings,
    /// Boundary conditions.
    pub boundaries: Boundaries,
    /// Optional uniform initial velocity; missing means fluid at rest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<InitialField>,
    /// Solid obstacles in document order.
    pub obstacles: Vec<Obstacle>,
    /// Porous regions in document order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub porous: Vec<PorousRegion>,
    /// Point monitors in document order.
    pub probes: Vec<Probe>,
    /// Volumetric scalar sources in document order.
    pub scalar_sources: Vec<ScalarSource>,
    /// Time-series boundary drives in document order.
    pub transient_inlets: Vec<TransientInlet>,
    /// Optional passive scalar transport settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scalar: Option<ScalarConfig>,
    /// Optional eddy-viscosity turbulence settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turbulence: Option<Turbulence>,
    /// Optional thermal (energy-equation) settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thermal: Option<ThermalConfig>,
    /// Optional Boussinesq buoyancy settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buoyancy: Option<Buoyancy>,
    /// Optional log-law wall function for the wall-shear report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_function: Option<WallFunction>,
}

impl CaseSummary {
    fn from_model(path: &Path, model: &Model) -> Self {
        Self {
            id: path.display().to_string(),
            path: path.display().to_string(),
            format: model.format.clone(),
            version: model.version,
            mesh: model.mesh.clone(),
            fluid: model.fluid.clone(),
            reference: model.reference.clone(),
            convection: model.convection,
            pressure_gradient: model.pressure_gradient,
            time: model.time.clone(),
            boundaries: model.boundaries.clone(),
            initial: model.initial.clone(),
            obstacles: model.obstacles.clone(),
            porous: model.porous.clone(),
            probes: model.probes.clone(),
            scalar_sources: model.scalar_sources.clone(),
            transient_inlets: model.transient_inlets.clone(),
            scalar: model.scalar.clone(),
            turbulence: model.turbulence.clone(),
            thermal: model.thermal.clone(),
            buoyancy: model.buoyancy.clone(),
            wall_function: model.wall_function.clone(),
        }
    }
}

/// Result of writing a document back to disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveReport {
    /// Target file.
    pub path: String,
    /// Number of UTF-8 bytes written.
    pub bytes: usize,
}

/// Result of exporting a solved field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    /// Format that was written.
    pub format: String,
    /// Sampling layout of a CSV or VTK field (`cell` or `node`).
    pub layout: String,
    /// File that received the export.
    pub path: String,
}

/// One `.ocfd` document and the file it is bound to.
///
/// Constructing a session opens no file. [`OpenCfd::create`] initialises the
/// default lid-driven cavity and [`OpenCfd::open`] loads an existing document;
/// both bind the session to a path so later edits can be written back
/// atomically.
#[derive(Debug, Clone)]
pub struct OpenCfd {
    locale: String,
    path: Option<PathBuf>,
    model: Model,
}

impl Default for OpenCfd {
    fn default() -> Self {
        Self::new(OpenCfdOptions::default())
    }
}

impl OpenCfd {
    /// Creates an unbound session. No file, window or network is touched.
    pub fn new(options: OpenCfdOptions) -> Self {
        Self {
            locale: options.locale,
            path: None,
            model: Model::new(),
        }
    }

    /// BCP-47 locale tag of this session.
    pub fn locale(&self) -> &str {
        &self.locale
    }

    /// Machine-readable capability matrix of this build.
    pub fn capabilities(&self) -> &'static str {
        CAPABILITIES_JSON
    }

    /// Creates the default cavity case at `path` and binds the session to it.
    ///
    /// Fails with `file/exists` when the target already exists; use
    /// [`OpenCfd::open`] for existing documents.
    pub fn create(&mut self, path: &Path) -> Result<CaseSummary, CfdError> {
        if path.exists() {
            return Err(CfdError::new(
                "file/exists",
                format!(
                    "file `{}` already exists; delete it or choose another path",
                    path.display()
                ),
            ));
        }
        let model = Model::new();
        model.validate()?;
        model.save_atomic(path)?;
        self.model = model;
        self.path = Some(path.to_path_buf());
        self.summary()
    }

    /// Loads an existing `.ocfd` document and binds the session to its path.
    pub fn open(&mut self, path: &Path) -> Result<CaseSummary, CfdError> {
        let model = Model::load(path)?;
        self.model = model;
        self.path = Some(path.to_path_buf());
        self.summary()
    }

    /// Snapshot of the current document.
    ///
    /// Fails with `document/no-path` while the session is not bound to a file.
    pub fn summary(&self) -> Result<CaseSummary, CfdError> {
        let path = self.bound_path()?;
        Ok(CaseSummary::from_model(path, &self.model))
    }

    /// Validates and atomically writes the document.
    ///
    /// `target` writes a copy (or moves the binding) to another path; `None`
    /// rewrites the path returned by [`OpenCfd::create`] or [`OpenCfd::open`].
    pub fn save(&mut self, target: Option<&Path>) -> Result<SaveReport, CfdError> {
        let path = match target {
            Some(path) => path.to_path_buf(),
            None => self.bound_path()?.to_path_buf(),
        };
        self.model.validate()?;
        let bytes = self.model.to_json_pretty()?.len();
        self.model.save_atomic(&path)?;
        self.path = Some(path.clone());
        Ok(SaveReport {
            path: path.display().to_string(),
            bytes,
        })
    }

    /// Sets the structured mesh and writes the document back.
    pub fn mesh_set(
        &mut self,
        nx: usize,
        ny: usize,
        length_x: f64,
        length_y: f64,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.mesh.nx = nx;
            model.mesh.ny = ny;
            model.mesh.length_x = length_x;
            model.mesh.length_y = length_y;
        })
    }

    /// Sets density and dynamic viscosity and writes the document back.
    pub fn fluid_set(&mut self, density: f64, viscosity: f64) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.fluid.density = density;
            model.fluid.viscosity = viscosity;
        })
    }

    /// Sets density, dynamic viscosity and the apparent-viscosity model and
    /// writes the document back.
    ///
    /// `consistency`, `power`, `min_viscosity` and `max_viscosity` are stored as
    /// given; validation rejects missing or illegal power-law parameters with
    /// `fluid/invalid-viscosity` and an unknown model with
    /// `fluid/invalid-viscosity-model`, leaving the file untouched.
    #[allow(clippy::too_many_arguments)]
    pub fn fluid_set_with_model(
        &mut self,
        density: f64,
        viscosity: f64,
        viscosity_model: ViscosityModel,
        consistency: Option<f64>,
        power: Option<f64>,
        min_viscosity: Option<f64>,
        max_viscosity: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.fluid.density = density;
            model.fluid.viscosity = viscosity;
            model.fluid.viscosity_model = viscosity_model;
            model.fluid.consistency = consistency;
            model.fluid.power = power;
            model.fluid.min_viscosity = min_viscosity;
            model.fluid.max_viscosity = max_viscosity;
        })
    }

    /// Sets the convective discretisation and writes the document back.
    pub fn convection_set(&mut self, scheme: ConvectionScheme) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.convection = scheme;
        })
    }

    /// Sets the uniform imposed pressure gradient and writes the document back.
    ///
    /// The gradient is stored in Pa/m and enters the momentum equation as the
    /// body force `-∇p/ρ`. Setting both components to zero removes the field.
    pub fn pressure_gradient_set(&mut self, dpdx: f64, dpdy: f64) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.pressure_gradient = if dpdx == 0.0 && dpdy == 0.0 {
                None
            } else {
                Some([dpdx, dpdy])
            };
        })
    }

    /// Sets force and moment reference quantities and writes the document.
    ///
    /// Omitted components keep their current values (or stay automatic when no
    /// reference section exists yet).
    pub fn reference_set(
        &mut self,
        velocity: Option<f64>,
        area: Option<f64>,
        length: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let reference = model.reference.get_or_insert_with(Reference::default);
            if let Some(velocity) = velocity {
                reference.velocity = Some(velocity);
            }
            if let Some(area) = area {
                reference.area = Some(area);
            }
            if let Some(length) = length {
                reference.length = Some(length);
            }
        })
    }

    /// Sets the time step, step count, pressure sweeps and optional tolerance.
    pub fn time_set(
        &mut self,
        dt: f64,
        steps: usize,
        pressure_iterations: usize,
        tolerance: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.time.dt = dt;
            model.time.steps = steps;
            model.time.pressure_iterations = pressure_iterations;
            if let Some(tolerance) = tolerance {
                model.time.tolerance = tolerance;
            }
        })
    }

    /// Sets the explicit time integration scheme and writes the document back.
    pub fn time_scheme_set(&mut self, scheme: TimeScheme) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.time.scheme = scheme;
        })
    }

    /// Enables or disables adaptive time stepping.
    ///
    /// When enabled, every step is reduced below `time.dt` as needed to satisfy
    /// the convective CFL target (and the explicit diffusion bound). A
    /// `target_cfl` outside `(0, 1]` is rejected by `validate`.
    pub fn time_adaptive_set(
        &mut self,
        enabled: bool,
        target_cfl: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.time.adaptive = enabled;
            if let Some(target) = target_cfl {
                model.time.target_cfl = target;
            }
        })
    }

    /// Sets one side's boundary kind, optional velocity components and outlet
    /// reference pressure.
    pub fn boundary_set(
        &mut self,
        side: Side,
        kind: BoundaryKind,
        u: Option<f64>,
        v: Option<f64>,
        pressure: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let boundary = model.boundaries.get_mut(side);
            boundary.kind = kind;
            if let Some(u) = u {
                boundary.velocity[0] = u;
            }
            if let Some(v) = v {
                boundary.velocity[1] = v;
            }
            if let Some(pressure) = pressure {
                boundary.pressure = pressure;
            }
        })
    }

    /// Returns the four side boundary conditions and the inlet profiles.
    pub fn boundary_list(&self) -> Boundaries {
        self.model.boundaries.clone()
    }

    /// Resets one side, or every side, to a stationary no-slip wall.
    ///
    /// The reset clears the side velocity and outlet pressure, restores the
    /// uniform inlet profile and removes any transient drive for that side, so
    /// a time series cannot keep overriding the wall during a transient solve.
    pub fn boundary_reset(&mut self, side: Option<Side>) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            for current in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
                if side.is_some_and(|target| target != current) {
                    continue;
                }
                *model.boundaries.get_mut(current) = Boundary::wall();
                *model.boundaries.inlet_profiles.get_mut(current) = InletProfile::Uniform;
                model.transient_inlets.retain(|inlet| inlet.side != current);
            }
        })
    }

    /// Reports the derived Reynolds number of the current document.
    ///
    /// The reference speed is the configured `reference.velocity`, otherwise
    /// the fastest inlet speed, otherwise `1 m/s`. The reference length is
    /// `reference.length`, otherwise the first obstacle's characteristic
    /// length, otherwise the domain height. No time stepping runs.
    pub fn reynolds(&self) -> Result<ReynoldsNumber, CfdError> {
        self.model.validate()?;
        Ok(reynolds_number(&self.model))
    }

    /// Validates the current document and returns its snapshot.
    pub fn validate(&self) -> Result<CaseSummary, CfdError> {
        self.model.validate()?;
        self.summary()
    }

    /// Sets the normal-velocity profile of one inlet side and writes the
    /// document back.
    ///
    /// `profile` applies only when the side is an `inlet`; `exponent` updates
    /// the shared power-law exponent and is ignored by other profiles.
    pub fn boundary_profile_set(
        &mut self,
        side: Side,
        profile: InletProfile,
        exponent: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            *model.boundaries.inlet_profiles.get_mut(side) = profile;
            if let Some(exponent) = exponent {
                model.boundaries.inlet_profiles.exponent = exponent;
            }
        })
    }

    /// Appends a solid obstacle and writes the document back.
    ///
    /// An empty id is replaced by `obstacle-N`, where `N` is the one-based
    /// position in the obstacle list.
    pub fn obstacle_add(&mut self, obstacle: Obstacle) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let id = if obstacle.id().is_empty() {
                format!("obstacle-{}", model.obstacles.len() + 1)
            } else {
                obstacle.id().to_string()
            };
            model.obstacles.push(obstacle.with_id(id));
        })
    }

    /// Returns the solid obstacles in document order.
    pub fn obstacle_list(&self) -> Vec<Obstacle> {
        self.model.obstacles.clone()
    }

    /// Removes the first solid obstacle with `id` and writes the document back.
    ///
    /// Fails with `obstacle/not-found` when no obstacle carries `id`, leaving
    /// the document unchanged. Duplicate ids cannot be created by the model, so
    /// only the first match is ever removed.
    pub fn obstacle_remove(&mut self, id: &str) -> Result<CaseSummary, CfdError> {
        self.bound_path()?;
        let index = self.obstacle_index(id)?;
        self.edit(|model| {
            model.obstacles.remove(index);
        })
    }

    /// Replaces the first solid obstacle with `id` in place and writes the
    /// document back.
    ///
    /// `obstacle` supplies the new geometry and keeps the stored `id`. Fails
    /// with `obstacle/not-found` when no obstacle carries `id`, leaving the
    /// document unchanged. The replacement goes through the same validation as
    /// [`OpenCfd::obstacle_add`], and obstacles after it keep their position.
    pub fn obstacle_update(
        &mut self,
        id: &str,
        obstacle: Obstacle,
    ) -> Result<CaseSummary, CfdError> {
        self.bound_path()?;
        let index = self.obstacle_index(id)?;
        self.edit(|model| {
            model.obstacles[index] = obstacle.with_id(id.to_string());
        })
    }

    /// Appends a porous region and writes the document back.
    ///
    /// An empty id is replaced by `porous-N`, where `N` is the one-based
    /// position in the region list.
    pub fn porous_add(&mut self, region: PorousRegion) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let id = if region.id().is_empty() {
                format!("porous-{}", model.porous.len() + 1)
            } else {
                region.id().to_string()
            };
            model.porous.push(region.with_id(id));
        })
    }

    /// Returns the porous regions in document order.
    pub fn porous_list(&self) -> Vec<PorousRegion> {
        self.model.porous.clone()
    }

    /// Removes the first porous region with `id` and writes the document back.
    ///
    /// Fails with `porous/not-found` when no region carries `id`, leaving the
    /// document unchanged. Duplicate ids cannot be created by the model, so
    /// only the first match is ever removed.
    pub fn porous_remove(&mut self, id: &str) -> Result<CaseSummary, CfdError> {
        self.bound_path()?;
        let index = self
            .model
            .porous
            .iter()
            .position(|region| region.id() == id)
            .ok_or_else(|| {
                CfdError::new(
                    "porous/not-found",
                    format!(
                        "no porous region has id `{id}`; available ids: {}",
                        available_ids(self.model.porous.iter().map(PorousRegion::id))
                    ),
                )
            })?;
        self.edit(|model| {
            model.porous.remove(index);
        })
    }

    /// Appends a point monitor and writes the document back.
    ///
    /// An empty id is replaced by `probe-N`, where `N` is the one-based
    /// position in the probe list.
    pub fn probe_add(&mut self, probe: Probe) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let id = if probe.id().is_empty() {
                format!("probe-{}", model.probes.len() + 1)
            } else {
                probe.id().to_string()
            };
            model.probes.push(probe.with_id(id));
        })
    }

    /// Returns the point monitors in document order.
    pub fn probe_list(&self) -> Vec<Probe> {
        self.model.probes.clone()
    }

    /// Removes every point monitor and writes the document back.
    pub fn probe_clear(&mut self) -> Result<CaseSummary, CfdError> {
        self.edit(|model| model.probes.clear())
    }

    /// Moves the first point monitor with `id` to `(x, y)` and writes the
    /// document back.
    ///
    /// Fails with `probe/not-found` when no probe carries `id`, leaving the
    /// document unchanged. The new position goes through the same domain and
    /// obstacle validation as [`OpenCfd::probe_add`].
    pub fn probe_update(&mut self, id: &str, x: f64, y: f64) -> Result<CaseSummary, CfdError> {
        self.bound_path()?;
        let index = self.probe_index(id)?;
        self.edit(|model| {
            model.probes[index] = Probe {
                id: id.to_string(),
                x,
                y,
            };
        })
    }

    /// Removes the first point monitor with `id` and writes the document back.
    ///
    /// Fails with `probe/not-found` when no probe carries `id`, leaving the
    /// document unchanged.
    pub fn probe_remove(&mut self, id: &str) -> Result<CaseSummary, CfdError> {
        self.bound_path()?;
        let index = self.probe_index(id)?;
        self.edit(|model| {
            model.probes.remove(index);
        })
    }

    /// Appends a volumetric scalar source or sink and writes the document back.
    ///
    /// An empty id is replaced by `source-N`, where `N` is the one-based
    /// position in the source list. Scalar transport must already be configured.
    pub fn scalar_source_add(&mut self, source: ScalarSource) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let id = if source.id().is_empty() {
                format!("source-{}", model.scalar_sources.len() + 1)
            } else {
                source.id().to_string()
            };
            model.scalar_sources.push(source.with_id(id));
        })
    }

    /// Returns the volumetric scalar sources in document order.
    pub fn scalar_source_list(&self) -> Vec<ScalarSource> {
        self.model.scalar_sources.clone()
    }

    /// Removes every volumetric scalar source and writes the document back.
    pub fn scalar_source_clear(&mut self) -> Result<CaseSummary, CfdError> {
        self.edit(|model| model.scalar_sources.clear())
    }

    /// Adds or replaces the transient drive of one side and writes the document
    /// back. A second entry for the same side replaces the first.
    pub fn transient_inlet_set(&mut self, inlet: TransientInlet) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model
                .transient_inlets
                .retain(|existing| existing.side != inlet.side);
            model.transient_inlets.push(inlet);
        })
    }

    /// Returns the transient inlet drives in document order.
    pub fn transient_inlet_list(&self) -> Vec<TransientInlet> {
        self.model.transient_inlets.clone()
    }

    /// Removes every transient inlet drive and writes the document back.
    pub fn transient_inlet_clear(&mut self) -> Result<CaseSummary, CfdError> {
        self.edit(|model| model.transient_inlets.clear())
    }

    /// Configures passive scalar transport and writes the document back.
    ///
    /// `initial` and `inlet` update the stored values; omitted components keep
    /// their current values (or the defaults when no scalar exists yet).
    pub fn scalar_set(
        &mut self,
        diffusivity: f64,
        initial: Option<f64>,
        inlet: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            let config = model.scalar.get_or_insert_with(ScalarConfig::default);
            config.diffusivity = diffusivity;
            if let Some(initial) = initial {
                config.initial = initial;
            }
            if let Some(inlet) = inlet {
                config.inlet_value = inlet;
            }
        })
    }

    /// Configures the eddy-viscosity turbulence model and writes the document.
    ///
    /// Passing [`TurbulenceModel::None`] removes the turbulence section. `k` and
    /// `epsilon` are the initial `k`–`ε` values used by
    /// [`TurbulenceModel::KEpsilon`]; omitted values keep the model defaults.
    pub fn turbulence_set(
        &mut self,
        model_kind: TurbulenceModel,
        nu_t: f64,
        mixing_length: f64,
        k: Option<f64>,
        epsilon: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.turbulence = if model_kind == TurbulenceModel::None {
                None
            } else {
                let defaults = Turbulence::default();
                Some(Turbulence {
                    model: model_kind,
                    nu_t,
                    mixing_length,
                    k: k.unwrap_or(defaults.k),
                    epsilon: epsilon.unwrap_or(defaults.epsilon),
                })
            };
        })
    }

    /// Configures thermal boundaries/source and writes the document back.
    ///
    /// `diffusivity` creates or updates the scalar transport coefficient when
    /// supplied; a thermal section requires an existing scalar field.
    pub fn thermal_set(
        &mut self,
        thermal: ThermalConfig,
        diffusivity: Option<f64>,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            if let Some(diffusivity) = diffusivity {
                model
                    .scalar
                    .get_or_insert_with(ScalarConfig::default)
                    .diffusivity = diffusivity;
            }
            model.thermal = Some(thermal);
        })
    }

    /// Configures Boussinesq buoyancy and writes the document back.
    ///
    /// `beta` is the thermal expansion coefficient (1/K), `t_ref` the
    /// reference temperature (K) and `[gx, gy]` the gravity vector (m/s²). A
    /// non-positive `beta` removes the buoyancy section. Buoyancy requires an
    /// existing scalar (temperature) field, so configure `scalar_set` first.
    pub fn buoyancy_set(
        &mut self,
        beta: f64,
        t_ref: f64,
        gx: f64,
        gy: f64,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.buoyancy = if beta > 0.0 {
                Some(Buoyancy {
                    beta,
                    t_ref,
                    gravity: [gx, gy],
                })
            } else {
                None
            };
        })
    }

    /// Configures the log-law wall function and writes the document back.
    ///
    /// `kappa` and `intercept` define the log law and `min_y_plus` the
    /// viscous-sublayer crossover. The wall function only changes the reported
    /// wall shear stress and friction coefficient.
    pub fn wall_function_set(
        &mut self,
        kappa: f64,
        intercept: f64,
        min_y_plus: f64,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.wall_function = Some(WallFunction {
                kappa,
                intercept,
                min_y_plus,
            });
        })
    }

    /// Removes the wall function and writes the document back.
    pub fn wall_function_clear(&mut self) -> Result<CaseSummary, CfdError> {
        self.edit(|model| model.wall_function = None)
    }

    /// Solves the current document and returns the report.
    pub fn solve(&self) -> Result<Report, CfdError> {
        Ok(solve(&self.model)?.report)
    }

    /// Measures the quality of the current document's mesh.
    ///
    /// The solid mask is applied first, so `fluidCells` and the aspect-ratio
    /// extremes match the mesh the solver would build. No time stepping runs.
    pub fn mesh_quality(&self) -> Result<MeshQuality, CfdError> {
        self.model.validate()?;
        let mut mesh = Mesh::new(&self.model.mesh)?;
        mesh.apply_obstacles(&self.model.obstacles)?;
        Ok(measure_quality(&mesh))
    }

    /// Reports the explicit time-step stability limits of the current document.
    ///
    /// The convective CFL number, the diffusion numbers of the configured
    /// `time.dt` and the largest stable step are derived from the mesh, the
    /// fluid and the fastest imposed speed. No time stepping runs.
    pub fn stability(&self) -> Result<StabilityLimits, CfdError> {
        self.model.validate()?;
        stability_limits(&self.model)
    }

    /// Solves with solve-time overrides and returns the report.
    pub fn solve_with(&self, options: &SolveOptions) -> Result<Report, CfdError> {
        Ok(solve_with(&self.model, options)?.report)
    }

    /// Solves, optionally resuming from `restart`, and optionally writes the
    /// final state to `checkpoint`.
    pub fn solve_with_checkpoint(
        &self,
        options: &SolveOptions,
        restart: Option<&Path>,
        checkpoint: Option<&Path>,
    ) -> Result<Report, CfdError> {
        let state = restart.map(Checkpoint::load).transpose()?;
        let solution = solve_resuming(&self.model, options, state.as_ref())?;
        if let Some(path) = checkpoint {
            Checkpoint::capture(&self.model, &solution).save(path)?;
        }
        Ok(solution.report)
    }

    /// Solves the current document and writes a CSV, VTK, contour or pressure
    /// SVG field.
    pub fn export(&self, format: ExportFormat, out: &Path) -> Result<ExportReport, CfdError> {
        self.export_field(format, out, ExportField::Pressure)
    }

    /// Solves and exports one field; `field` affects SVG and contour output.
    pub fn export_field(
        &self,
        format: ExportFormat,
        out: &Path,
        field: ExportField,
    ) -> Result<ExportReport, CfdError> {
        self.export_field_with_layout(format, out, field, ExportLayout::Cell)
    }

    /// Solves and exports one field with an explicit CSV/VTK sampling layout.
    ///
    /// `layout` selects cell centres or mesh nodes for `Csv` and `Vtk` and is
    /// reported back; SVG, contour, streamline and history exports always use
    /// cell-centred data.
    pub fn export_field_with_layout(
        &self,
        format: ExportFormat,
        out: &Path,
        field: ExportField,
        layout: ExportLayout,
    ) -> Result<ExportReport, CfdError> {
        let solution = solve(&self.model)?;
        let written = match format {
            ExportFormat::Csv => export_csv_with_layout(&solution, out, layout)?,
            ExportFormat::Vtk => export_vtk_with_layout(&solution, out, layout)?,
            ExportFormat::Svg => export_svg(&solution, out, field)?,
            ExportFormat::Contours => export_contours(&solution, out, field)?,
            ExportFormat::Streamlines => export_streamlines(&solution, out)?,
            ExportFormat::Residuals => export_residuals(&solution, out)?,
            ExportFormat::Forces => export_forces(&solution, out)?,
            ExportFormat::Coefficients => export_coefficients(&solution, out)?,
            ExportFormat::Probes => export_probes(&solution, out)?,
            ExportFormat::Profile => {
                return Err(CfdError::new(
                    "export/needs-line",
                    "profile export needs a line and sample count; use OpenCfd::export_profile",
                ))
            }
            ExportFormat::Statistics => write_statistics(&solution, out)?,
            ExportFormat::Particles => {
                let config = ParticleConfig {
                    seeds: Vec::new(),
                    dt: solution.report.dt,
                    max_steps: DEFAULT_PARTICLE_STEPS,
                };
                write_particles(&solution, out, &config)?
            }
        };
        Ok(ExportReport {
            format: format.as_str().to_string(),
            layout: layout.as_str().to_string(),
            path: written.display().to_string(),
        })
    }

    /// Solves and samples one cell-centred field along a straight segment.
    ///
    /// The line is sampled at `samples` equally spaced points; values come from
    /// the fluid cell containing each point, and a sample outside the domain or
    /// inside a solid cell is written as an empty CSV value. The line is
    /// validated before the solve, so invalid arguments never write a file.
    pub fn export_profile(
        &self,
        out: &Path,
        line: ProfileLine,
        samples: usize,
        field: ExportField,
    ) -> Result<ExportReport, CfdError> {
        line.length()?;
        if samples < 2 {
            return Err(CfdError::new(
                "export/invalid-samples",
                "profile samples must be at least 2",
            ));
        }
        let solution = solve(&self.model)?;
        let written = write_profile(&solution, out, line, samples, field)?;
        Ok(ExportReport {
            format: ExportFormat::Profile.as_str().to_string(),
            layout: ExportLayout::Cell.as_str().to_string(),
            path: written.display().to_string(),
        })
    }

    /// Solves and writes deterministic per-field statistics as JSON.
    ///
    /// Only fluid cells contribute, and non-finite values are skipped. See
    /// `crate::export::export_statistics` for the document shape.
    pub fn export_statistics(&self, out: &Path) -> Result<ExportReport, CfdError> {
        let solution = solve(&self.model)?;
        let written = write_statistics(&solution, out)?;
        Ok(ExportReport {
            format: ExportFormat::Statistics.as_str().to_string(),
            layout: ExportLayout::Cell.as_str().to_string(),
            path: written.display().to_string(),
        })
    }

    /// Solves and writes Lagrangian particle trajectories as CSV rows.
    ///
    /// `config.dt` must be positive; a non-positive or non-finite value is
    /// replaced by the last time step of the solve. An empty `seeds` list uses
    /// the default regular seed grid. See [`write_particles`] for the header.
    pub fn export_particles(
        &self,
        out: &Path,
        config: &ParticleConfig,
    ) -> Result<ExportReport, CfdError> {
        let solution = solve(&self.model)?;
        let mut config = config.clone();
        if !config.dt.is_finite() || config.dt <= 0.0 {
            config.dt = solution.report.dt;
        }
        let written = write_particles(&solution, out, &config)?;
        Ok(ExportReport {
            format: ExportFormat::Particles.as_str().to_string(),
            layout: ExportLayout::Cell.as_str().to_string(),
            path: written.display().to_string(),
        })
    }

    /// Applies one mutation to a copy, validates, then commits file and memory.
    fn edit(&mut self, apply: impl FnOnce(&mut Model)) -> Result<CaseSummary, CfdError> {
        let path = self.bound_path()?.to_path_buf();
        let mut model = self.model.clone();
        apply(&mut model);
        model.validate()?;
        model.save_atomic(&path)?;
        self.model = model;
        self.summary()
    }

    fn bound_path(&self) -> Result<&Path, CfdError> {
        self.path.as_deref().ok_or_else(|| {
            CfdError::new(
                "document/no-path",
                "the session is not bound to a file; create or open an .ocfd document first",
            )
        })
    }

    /// Position of the first obstacle carrying `id`.
    fn obstacle_index(&self, id: &str) -> Result<usize, CfdError> {
        self.model
            .obstacles
            .iter()
            .position(|obstacle| obstacle.id() == id)
            .ok_or_else(|| {
                CfdError::new(
                    "obstacle/not-found",
                    format!(
                        "no obstacle has id `{id}`; available ids: {}",
                        available_ids(self.model.obstacles.iter().map(Obstacle::id))
                    ),
                )
            })
    }

    /// Position of the first probe carrying `id`.
    fn probe_index(&self, id: &str) -> Result<usize, CfdError> {
        self.model
            .probes
            .iter()
            .position(|probe| probe.id() == id)
            .ok_or_else(|| {
                CfdError::new(
                    "probe/not-found",
                    format!(
                        "no probe has id `{id}`; available ids: {}",
                        available_ids(self.model.probes.iter().map(Probe::id))
                    ),
                )
            })
    }
}

/// Formats the ids of a document list for a not-found diagnostic.
fn available_ids<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let ids: Vec<String> = ids.map(|id| format!("`{id}`")).collect();
    if ids.is_empty() {
        "none".to_string()
    } else {
        ids.join(", ")
    }
}
