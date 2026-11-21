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

use crate::export::{
    export_csv, export_forces, export_residuals, export_streamlines, export_svg, export_vtk,
    ExportField,
};
use crate::model::{
    Boundaries, BoundaryKind, ConvectionScheme, Fluid, InitialField, InletProfile, MeshConfig,
    Model, Obstacle, Probe, Reference, ScalarConfig, Side, ThermalConfig, TimeScheme, TimeSettings,
    Turbulence, TurbulenceModel,
};
use crate::solver::{solve, solve_with, Report, SolveOptions};
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
    /// Self-contained SVG of traced streamlines.
    Streamlines,
    /// Sampled residual-history CSV rows.
    Residuals,
    /// Sampled obstacle force-history CSV rows.
    Forces,
}

impl ExportFormat {
    /// Returns the lowercase identifier used by the CLI and the SDK.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Vtk => "vtk",
            Self::Svg => "svg",
            Self::Streamlines => "streamlines",
            Self::Residuals => "residuals",
            Self::Forces => "forces",
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
            "streamlines" | "streamline" => Ok(Self::Streamlines),
            "residuals" | "residual" => Ok(Self::Residuals),
            "forces" | "force" => Ok(Self::Forces),
            "inp" | "openfoam" => Err(CfdError::new(
                "capability/not-implemented",
                format!(
                    "export format {value} is not implemented; only csv, vtk, svg, streamlines, residuals or forces are supported"
                ),
            )),
            other => Err(CfdError::new(
                "cli/unknown-format",
                format!(
                    "unknown export format `{other}`; only csv, vtk, svg, streamlines, residuals or forces are supported"
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
    /// Time integration settings.
    pub time: TimeSettings,
    /// Boundary conditions.
    pub boundaries: Boundaries,
    /// Optional uniform initial velocity; missing means fluid at rest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial: Option<InitialField>,
    /// Solid obstacles in document order.
    pub obstacles: Vec<Obstacle>,
    /// Point monitors in document order.
    pub probes: Vec<Probe>,
    /// Optional passive scalar transport settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scalar: Option<ScalarConfig>,
    /// Optional eddy-viscosity turbulence settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turbulence: Option<Turbulence>,
    /// Optional thermal (energy-equation) settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thermal: Option<ThermalConfig>,
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
            time: model.time.clone(),
            boundaries: model.boundaries.clone(),
            initial: model.initial.clone(),
            obstacles: model.obstacles.clone(),
            probes: model.probes.clone(),
            scalar: model.scalar.clone(),
            turbulence: model.turbulence.clone(),
            thermal: model.thermal.clone(),
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

    /// Sets the convective discretisation and writes the document back.
    pub fn convection_set(&mut self, scheme: ConvectionScheme) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.convection = scheme;
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
    /// Passing [`TurbulenceModel::None`] removes the turbulence section.
    pub fn turbulence_set(
        &mut self,
        model_kind: TurbulenceModel,
        nu_t: f64,
        mixing_length: f64,
    ) -> Result<CaseSummary, CfdError> {
        self.edit(|model| {
            model.turbulence = if model_kind == TurbulenceModel::None {
                None
            } else {
                Some(Turbulence {
                    model: model_kind,
                    nu_t,
                    mixing_length,
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

    /// Solves the current document and returns the report.
    pub fn solve(&self) -> Result<Report, CfdError> {
        Ok(solve(&self.model)?.report)
    }

    /// Solves with solve-time overrides and returns the report.
    pub fn solve_with(&self, options: &SolveOptions) -> Result<Report, CfdError> {
        Ok(solve_with(&self.model, options)?.report)
    }

    /// Solves the current document and writes a CSV, VTK or pressure SVG field.
    pub fn export(&self, format: ExportFormat, out: &Path) -> Result<ExportReport, CfdError> {
        self.export_field(format, out, ExportField::Pressure)
    }

    /// Solves and exports one field; `field` only affects SVG output.
    pub fn export_field(
        &self,
        format: ExportFormat,
        out: &Path,
        field: ExportField,
    ) -> Result<ExportReport, CfdError> {
        let solution = solve(&self.model)?;
        let written = match format {
            ExportFormat::Csv => export_csv(&solution, out)?,
            ExportFormat::Vtk => export_vtk(&solution, out)?,
            ExportFormat::Svg => export_svg(&solution, out, field)?,
            ExportFormat::Streamlines => export_streamlines(&solution, out)?,
            ExportFormat::Residuals => export_residuals(&solution, out)?,
            ExportFormat::Forces => export_forces(&solution, out)?,
        };
        Ok(ExportReport {
            format: format.as_str().to_string(),
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
}
