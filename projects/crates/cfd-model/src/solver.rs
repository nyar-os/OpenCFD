use boost_check::{CheckReport, Issue};
use serde::{Deserialize, Serialize};

use crate::checkpoint::Checkpoint;
use crate::fields::{stream_function, vorticity};
use crate::mesh::Mesh;
use crate::model::{
    Boundaries, BoundaryKind, Buoyancy, ConvectionScheme, Model, ScalarConfig, Side, ThermalConfig,
    TimeScheme, Turbulence, TurbulenceModel, WallFunction,
};
use crate::quality::{mesh_quality, MeshQuality};
use crate::CfdError;

/// Identifier stored in the `format` field of a solve report.
pub const REPORT_FORMAT: &str = "open-cfd-report";

/// Version of the solve report understood by this build.
pub const REPORT_VERSION: u32 = 1;

/// Upper bound on the number of samples kept in a residual or force history.
const MAX_HISTORY: usize = 1000;

/// Default turbulence identifier for reports written before the field existed.
fn default_turbulence_model_name() -> String {
    TurbulenceModel::None.as_str().to_string()
}

/// Default convection identifier for reports written before the field existed.
fn default_convection_name() -> String {
    ConvectionScheme::Upwind.as_str().to_string()
}

/// Default termination reason for reports written before the field existed.
fn default_termination_reason() -> String {
    "maxSteps".to_string()
}

/// Default time-scheme identifier for reports written before the field existed.
fn default_time_scheme_name() -> String {
    TimeScheme::Euler.as_str().to_string()
}

/// Per-region porous resistance summary.
///
/// `meanSpeed` is the area mean cell speed inside the region and `meanDrag` the
/// mean Darcy–Forchheimer acceleration magnitude `(ν/K)|U| + (C_F/√K)|U|²` in
/// m/s², both over the fluid cells the region owns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PorousSummary {
    /// Stable region identifier.
    pub id: String,
    /// Number of fluid cells owned by the region.
    pub cells: usize,
    /// Mean cell speed inside the region (m/s).
    pub mean_speed: f64,
    /// Mean drag acceleration magnitude inside the region (m/s²).
    pub mean_drag: f64,
}

/// One sampled velocity residual.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResidualSample {
    /// One-based time step of the sample.
    pub step: usize,
    /// L2 norm of the velocity change at this step.
    pub residual: f64,
    /// RMS of the discrete divergence after the step (1/s).
    #[serde(default)]
    pub continuity: f64,
}

/// Net mass flow through the domain boundary.
///
/// All values are mass flow rates in kg/s. `inflow` and `outflow` are the
/// magnitudes of the inward and outward fluxes; `net` is `outflow - inflow`.
/// `relativeError` is `|net| / max(inflow, outflow)` and is zero for a closed
/// domain or for a balanced channel.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MassBalance {
    /// Total inward mass flow (kg/s).
    pub inflow: f64,
    /// Total outward mass flow (kg/s).
    pub outflow: f64,
    /// Signed net outward mass flow `outflow - inflow` (kg/s).
    pub net: f64,
    /// `|net| / max(inflow, outflow)`; zero when no flow crosses the boundary.
    pub relative_error: f64,
}

/// Outward mass flow through one outlet side.
///
/// Only `outlet` and `convective` sides are listed. `flow` is signed: it is
/// positive when the side discharges fluid and negative when recirculation
/// brings fluid back in. `share` divides that signed flow by the sum of the
/// positive outlet flows, so the shares of pure outlets add up to one and a
/// recirculating outlet contributes a negative share. `meanVelocity` is the
/// signed normal velocity averaged over the side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutletFlow {
    /// Side identifier (`left`, `right`, `top` or `bottom`).
    pub side: String,
    /// Signed outward mass flow (kg/s).
    pub flow: f64,
    /// Share of the total positive outlet flow; zero when nothing discharges.
    pub share: f64,
    /// Signed normal velocity averaged over the side (m/s).
    pub mean_velocity: f64,
    /// Side length (m).
    pub length: f64,
}

/// One sampled force coefficient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForceSample {
    /// One-based time step of the sample.
    pub step: usize,
    /// Total force in x (N).
    pub fx: f64,
    /// Total force in y (N).
    pub fy: f64,
    /// Total moment about the obstacle centroid, positive counter-clockwise
    /// (N·m).
    #[serde(default)]
    pub mz: f64,
    /// Drag coefficient `2 Fx / (rho U² A)`.
    #[serde(default)]
    pub cd: f64,
    /// Lift coefficient `2 Fy / (rho U² A)`.
    #[serde(default)]
    pub cl: f64,
    /// Moment coefficient `2 Mz / (rho U² A L)`, positive counter-clockwise.
    #[serde(default)]
    pub cm: f64,
}

/// Force report of one obstacle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObstacleForce {
    /// Stable obstacle identifier.
    pub id: String,
    /// Obstacle kind (`rect` or `circle`).
    pub kind: String,
    /// Total pressure plus viscous force in x (N).
    pub fx: f64,
    /// Total pressure plus viscous force in y (N).
    pub fy: f64,
    /// Total moment about the obstacle centroid (N·m), positive
    /// counter-clockwise.
    #[serde(default)]
    pub mz: f64,
    /// Drag coefficient.
    pub cd: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Moment coefficient, positive counter-clockwise.
    #[serde(default)]
    pub cm: f64,
    /// Reference speed used by the coefficients (m/s).
    #[serde(default)]
    pub reference_velocity: f64,
    /// Reference area used by the force coefficients (m²).
    #[serde(default)]
    pub reference_area: f64,
    /// Reference length used by the moment coefficient (m).
    #[serde(default)]
    pub reference_length: f64,
    /// Force history sampled every `checkEvery` steps (bounded).
    pub history: Vec<ForceSample>,
}

/// Surface heat-transfer summary of one obstacle.
///
/// `meanHeatFlux` is the area-averaged flux from the obstacle surface into the
/// fluid (W/m²). `nusselt` is `q̅ · D / (k · ΔT)` with `ΔT` measured from
/// `referenceTemperature` (or the mean fluid temperature when omitted).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObstacleNusselt {
    /// Stable obstacle identifier.
    pub id: String,
    /// Obstacle kind (`rect` or `circle`).
    pub kind: String,
    /// Area-averaged surface heat flux into the fluid (W/m²).
    pub mean_heat_flux: f64,
    /// Area-averaged surface temperature (K).
    pub wall_temperature: f64,
    /// Reference temperature used for `ΔT` (K).
    pub reference_temperature: f64,
    /// Nusselt number `q̅·D/(k·ΔT)`.
    pub nusselt: f64,
}

/// One sampled probe reading.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSample {
    /// One-based time step of the sample.
    pub step: usize,
    /// Physical time of the sample, `step × dt` (s).
    pub time: f64,
    /// Cell-centred x velocity (m/s).
    pub u: f64,
    /// Cell-centred y velocity (m/s).
    pub v: f64,
    /// Speed `√(u² + v²)` (m/s).
    pub speed: f64,
    /// Cell-centred gauge pressure (Pa).
    pub pressure: f64,
    /// Cell-centred temperature (K), or `null` when no scalar is configured.
    pub temperature: Option<f64>,
}

/// Time series recorded at one point monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeSeries {
    /// Stable probe identifier.
    pub id: String,
    /// Probe x coordinate (m).
    pub x: f64,
    /// Probe y coordinate (m).
    pub y: f64,
    /// Cell `[i, j]` whose centre is nearest to the probe.
    pub cell: [usize; 2],
    /// Samples in time order (bounded).
    pub history: Vec<ProbeSample>,
}

/// Wall shear and friction summary of one domain side.
///
/// Only sides whose boundary kind is `wall` are reported. `meanShear` is the
/// length-averaged tangential stress the wall exerts on the fluid, positive
/// along the side's positive tangent (`+y` on the left and right sides, `+x` on
/// the top and bottom sides) in pascals. `frictionCoefficient` is
/// `meanShear / (0.5 ρ U²)` with `U` the same reference speed used by the force
/// coefficients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallShear {
    /// Side identifier (`left`, `right`, `top` or `bottom`).
    pub side: String,
    /// Length-averaged wall shear stress (Pa).
    pub mean_shear: f64,
    /// Largest absolute wall shear stress along the side (Pa).
    pub max_shear: f64,
    /// Length-averaged friction velocity `u_τ` estimate in `y⁺`, i.e. the mean
    /// of `u_τ·y/ν` over the fluid-adjacent boundary cells. Zero at rest.
    #[serde(default)]
    pub mean_y_plus: f64,
    /// Friction coefficient `meanShear / (0.5 ρ U²)`.
    pub friction_coefficient: f64,
}

/// Summary of a completed simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Report format tag; always [`REPORT_FORMAT`].
    pub format: String,
    /// Report version; always [`REPORT_VERSION`].
    pub version: u32,
    /// Number of time steps advanced since `t = 0`, including steps restored
    /// from a checkpoint.
    pub steps: usize,
    /// Time step of the last advanced step (s).
    pub dt: f64,
    /// Whether adaptive time stepping reduced the configured upper bound.
    #[serde(default)]
    pub adaptive: bool,
    /// Mean time step over the advanced steps (s).
    #[serde(default)]
    pub average_dt: f64,
    /// Smallest time step used (s).
    #[serde(default)]
    pub min_dt: f64,
    /// Largest time step used (s).
    #[serde(default)]
    pub max_dt: f64,
    /// Simulated physical time since `t = 0`, including time restored from a
    /// checkpoint (s).
    pub time: f64,
    /// Maximum velocity magnitude on all faces (m/s).
    pub max_velocity: f64,
    /// Minimum cell-centred pressure of fluid cells (Pa).
    pub min_pressure: f64,
    /// Maximum cell-centred pressure of fluid cells (Pa).
    pub max_pressure: f64,
    /// RMS of the discrete divergence after the final projection (1/s).
    pub divergence_l2: f64,
    /// Quality metrics of the mesh the case was solved on.
    #[serde(default)]
    pub mesh_quality: MeshQuality,
    /// Net mass flow through the domain boundary.
    #[serde(default)]
    pub mass_balance: MassBalance,
    /// Per-outlet signed mass flow and share, in side order.
    #[serde(default)]
    pub outlets: Vec<OutletFlow>,
    /// Minimum cell-centred vorticity of fluid cells (1/s).
    #[serde(default)]
    pub vorticity_min: f64,
    /// Maximum cell-centred vorticity of fluid cells (1/s).
    #[serde(default)]
    pub vorticity_max: f64,
    /// Minimum cell-centred stream function of fluid cells (m²/s).
    #[serde(default)]
    pub stream_function_min: f64,
    /// Maximum cell-centred stream function of fluid cells (m²/s).
    #[serde(default)]
    pub stream_function_max: f64,
    /// Run convergence. With a steady-state tolerance it reports whether the
    /// velocity stopped changing; without one it reports the pressure solve.
    pub converged: bool,
    /// Whether the velocity change fell below the requested tolerance.
    pub steady_state: bool,
    /// Final L2 norm of the per-step velocity change.
    pub residual: f64,
    /// Velocity residuals sampled every `checkEvery` steps (bounded).
    pub residual_history: Vec<ResidualSample>,
    /// Why the run stopped: `"maxSteps"` when every requested step was
    /// advanced, `"steadyState"` when the velocity residual met the tolerance.
    #[serde(default = "default_termination_reason")]
    pub termination_reason: String,
    /// Whether the final pressure solve reduced its residual below
    /// `tolerance × max(initial residual, 1e-12)`.
    pub pressure_converged: bool,
    /// SOR sweeps used by the final pressure solve.
    pub pressure_iterations: usize,
    /// Final maximum absolute pressure-Poisson residual (Pa/m²).
    pub pressure_residual: f64,
    /// Initial residual of the final pressure solve (Pa/m²).
    pub pressure_residual_initial: f64,
    /// Per-obstacle forces and coefficients, in document order.
    pub forces: Vec<ObstacleForce>,
    /// Point monitor time series, in document order.
    #[serde(default)]
    pub probes: Vec<ProbeSeries>,
    /// Wall shear and friction summary of every wall side, in side order.
    #[serde(default)]
    pub wall_shear: Vec<WallShear>,
    /// Convection scheme used by the run (`upwind`, `central` or `quick`).
    #[serde(default = "default_convection_name")]
    pub convection: String,
    /// Time integration scheme used by the run (`euler` or `rk2`).
    #[serde(default = "default_time_scheme_name")]
    pub time_scheme: String,
    /// Turbulence model identifier (`none`, `constant` or `mixingLength`).
    #[serde(default = "default_turbulence_model_name")]
    pub turbulence_model: String,
    /// Minimum eddy viscosity over fluid cells (m²/s).
    #[serde(default)]
    pub eddy_viscosity_min: f64,
    /// Maximum eddy viscosity over fluid cells (m²/s).
    #[serde(default)]
    pub eddy_viscosity_max: f64,
    /// Mean eddy viscosity over fluid cells (m²/s).
    #[serde(default)]
    pub eddy_viscosity_mean: f64,
    /// Per-obstacle Nusselt numbers, when a thermal configuration is present.
    #[serde(default)]
    pub nusselt: Vec<ObstacleNusselt>,
    /// Per-region porous resistance summary, in document order.
    #[serde(default)]
    pub porous: Vec<PorousSummary>,
    /// Whether Boussinesq buoyancy coupled the temperature field to momentum.
    #[serde(default)]
    pub buoyancy: bool,
    /// Whether a log-law wall function evaluated the reported wall shear.
    #[serde(default)]
    pub wall_function: bool,
}

/// A solved velocity, pressure and optional scalar field with its report.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// Mesh the field belongs to.
    pub mesh: Mesh,
    /// X-face velocity values.
    pub u: Vec<f64>,
    /// Y-face velocity values.
    pub v: Vec<f64>,
    /// Cell-centred pressure values.
    pub p: Vec<f64>,
    /// Cell-centred passive scalar, when the case configures one.
    pub temperature: Option<Vec<f64>>,
    /// Cell-centred vorticity `ω = ∂v/∂x − ∂u/∂y` (1/s).
    pub vorticity: Vec<f64>,
    /// Cell-centred stream function `ψ` (m²/s) with zero mean over fluid cells.
    pub stream_function: Vec<f64>,
    /// Cell-centred eddy viscosity, when a turbulence model is active.
    pub eddy_viscosity: Option<Vec<f64>>,
    /// Simulation report.
    pub report: Report,
}

/// Solve-time overrides.
///
/// The document still owns the defaults. `steps` and `tolerance` are optional
/// one-shot overrides for the CLI and SDK; `check_every` controls how often the
/// steady-state residual is sampled and tested. A `tolerance` enables early
/// stopping on the velocity residual; without it the run always uses `steps`
/// and reports pressure convergence in `Report::converged`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SolveOptions {
    /// Number of steps to advance; overrides `time.steps`.
    pub steps: Option<usize>,
    /// Steady-state tolerance on the L2 velocity change.
    pub tolerance: Option<f64>,
    /// Resume the steady-state check every this many steps.
    pub check_every: Option<usize>,
}

impl SolveOptions {
    fn validate(&self) -> Result<(), CfdError> {
        if self.steps == Some(0) {
            return Err(CfdError::new(
                "solver/invalid",
                "steps must be greater than 0",
            ));
        }
        if let Some(tolerance) = self.tolerance {
            if !tolerance.is_finite() || tolerance <= 0.0 {
                return Err(CfdError::new(
                    "solver/invalid",
                    "convergence tolerance must be a finite positive number",
                ));
            }
        }
        if self.check_every == Some(0) {
            return Err(CfdError::new(
                "solver/invalid",
                "checkEvery must be an integer greater than 0",
            ));
        }
        Ok(())
    }
}

/// Iteration controls for the pressure Poisson solver.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureSettings {
    /// Maximum number of SOR sweeps.
    pub iterations: usize,
    /// Residual tolerance relative to the initial residual of the solve.
    pub tolerance: f64,
}

/// Diagnostics returned by [`solve_pressure`].
#[derive(Debug, Clone, PartialEq)]
pub struct PressureStats {
    /// Number of sweeps actually performed.
    pub iterations: usize,
    /// Residual before the first sweep.
    pub initial_residual: f64,
    /// Residual after the last sweep.
    pub residual: f64,
    /// Residual after every sweep, in order.
    pub history: Vec<f64>,
    /// Whether `residual <= tolerance`.
    pub converged: bool,
}

/// Advances an incompressible case and returns the final field.
///
/// The scheme is a first-order Chorin projection on a staggered mesh: explicit
/// convection (first-order upwind, second-order central or QUICK according to
/// `model.convection`) plus explicit central diffusion produce a tentative
/// velocity, an SOR pressure Poisson solve enforces continuity, and the
/// pressure gradient corrects the velocity. Solid cells block all adjacent
/// faces, contribute zero divergence and are excluded from the pressure
/// unknowns.
pub fn solve(model: &Model) -> Result<Solution, CfdError> {
    solve_with(model, &SolveOptions::default())
}

/// Advances a case with solve-time overrides and returns the final field.
///
/// See [`solve`] for the numerical scheme. When `options.tolerance` is set the
/// run stops as soon as the L2 norm of the velocity change at a checked step is
/// at most that tolerance.
pub fn solve_with(model: &Model, options: &SolveOptions) -> Result<Solution, CfdError> {
    solve_with_checkpoint(model, options, None)
}

/// Solves a case, optionally resuming from a checkpoint and continuing its
/// step and sample numbering.
///
/// When `checkpoint` is present the mesh and scalar configuration must match
/// the case; the stored `u`, `v`, `p` and optional scalar replace the initial
/// field, the first advanced step is `checkpoint.steps + 1`, and the reported
/// `steps` and `time` are cumulative totals since `t = 0`.
pub fn solve_with_checkpoint(
    model: &Model,
    options: &SolveOptions,
    checkpoint: Option<&Checkpoint>,
) -> Result<Solution, CfdError> {
    model.validate()?;
    options.validate()?;
    let mut mesh = Mesh::new(&model.mesh)?;
    mesh.apply_obstacles(&model.obstacles)?;
    let steps = options.steps.unwrap_or(model.time.steps);

    let (mut u, mut v, mut p, mut temperature) = if let Some(state) = checkpoint {
        state.validate_against(model)?;
        (
            state.u.clone(),
            state.v.clone(),
            state.p.clone(),
            state.temperature.clone(),
        )
    } else {
        let mut u = vec![0.0; mesh.u_count()];
        let mut v = vec![0.0; mesh.v_count()];
        let p = vec![0.0; mesh.pressure_count()];
        if let Some(initial) = &model.initial {
            u.fill(initial.velocity[0]);
            v.fill(initial.velocity[1]);
        }
        mesh.apply_velocity_boundaries(&model.boundaries, &mut u, &mut v);
        let temperature = model
            .scalar
            .as_ref()
            .map(|config| initial_temperature(&mesh, config));
        (u, v, p, temperature)
    };
    let start_step = checkpoint.map_or(0, |state| state.steps);
    let mut simulated_time = checkpoint.map_or(0.0, |state| state.time);

    let settings = PressureSettings {
        iterations: model.time.pressure_iterations,
        tolerance: model.time.tolerance,
    };
    let mut eddy = eddy_viscosity_field(&mesh, model, &u, &v);
    let porous = porous_cells(&mesh, model);
    if !model.time.adaptive {
        check_explicit_stability(&mesh, model, &u, &v, eddy.as_deref())?;
    }

    let mut u_star = u.clone();
    let mut v_star = v.clone();
    let mut du = vec![0.0; mesh.u_count()];
    let mut dv = vec![0.0; mesh.v_count()];
    let mut u_previous = u.clone();
    let mut v_previous = v.clone();
    let mut last_stats = PressureStats {
        iterations: 0,
        initial_residual: 0.0,
        residual: 0.0,
        history: Vec::new(),
        converged: true,
    };

    let check_every = options.check_every.unwrap_or(1);
    let sample_stride = check_every.max(steps.div_ceil(MAX_HISTORY));
    let mut advanced = steps;
    let mut residual = 0.0;
    let mut steady_state = false;
    let mut residual_history = Vec::new();
    let mut force_samples: Vec<Vec<ForceSample>> = vec![Vec::new(); model.obstacles.len()];
    let probe_cells = locate_probes(&mesh, model)?;
    let mut probe_samples: Vec<Vec<ProbeSample>> = vec![Vec::new(); model.probes.len()];
    let mut dt_sum = 0.0;
    let mut dt_min = f64::INFINITY;
    let mut dt_max = 0.0_f64;
    let mut last_dt = checkpoint.map_or(model.time.dt, |state| {
        if state.dt > 0.0 {
            state.dt
        } else {
            model.time.dt
        }
    });

    for local_step in 1..=steps {
        let step = start_step + local_step;
        u_previous.copy_from_slice(&u);
        v_previous.copy_from_slice(&v);
        eddy = eddy_viscosity_field(&mesh, model, &u, &v);
        let dt = if model.time.adaptive {
            adaptive_dt(&mesh, model, &u, &v, eddy.as_deref())
        } else {
            model.time.dt
        };

        match model.time.scheme {
            TimeScheme::Euler => momentum_euler(
                &mesh,
                model,
                &u,
                &v,
                &p,
                temperature.as_deref(),
                eddy.as_deref(),
                &porous,
                dt,
                &mut u_star,
                &mut v_star,
                &mut du,
                &mut dv,
            ),
            TimeScheme::Rk2 => momentum_heun(
                &mesh,
                model,
                &u,
                &v,
                &p,
                temperature.as_deref(),
                eddy.as_deref(),
                &porous,
                dt,
                &mut u_star,
                &mut v_star,
            ),
        }
        mesh.apply_velocity_boundaries(&model.boundaries, &mut u_star, &mut v_star);

        let divergence = mesh.divergence(&u_star, &v_star);
        last_stats = solve_pressure(
            &mesh,
            &model.boundaries,
            &mut p,
            &divergence,
            model.fluid.density,
            dt,
            &settings,
        )?;

        u.copy_from_slice(&u_star);
        v.copy_from_slice(&v_star);
        project_velocity(
            &mesh,
            &model.boundaries,
            model.fluid.density,
            dt,
            &p,
            &mut u,
            &mut v,
        );
        mesh.apply_velocity_boundaries(&model.boundaries, &mut u, &mut v);
        mesh.apply_convective_outlets(
            &model.boundaries,
            &u_previous,
            &v_previous,
            &mut u,
            &mut v,
            dt,
        );

        if let (Some(config), Some(field)) = (model.scalar.as_ref(), temperature.as_mut()) {
            transport_thermal(
                &mesh,
                &model.boundaries,
                config,
                model.thermal.as_ref(),
                model.fluid.density,
                model.convection,
                dt,
                &u,
                &v,
                field,
            )?;
        }

        if !u
            .iter()
            .chain(v.iter())
            .chain(p.iter())
            .chain(temperature.iter().flat_map(|field| field.iter()))
            .all(|value| value.is_finite())
        {
            return Err(CfdError::new(
                "solver/non-finite",
                "the solve produced NaN or Inf; reduce dt, refine the mesh or lower the boundary velocity",
            ));
        }

        simulated_time += dt;
        dt_sum += dt;
        dt_min = dt_min.min(dt);
        dt_max = dt_max.max(dt);
        last_dt = dt;

        residual = velocity_residual(&u_previous, &v_previous, &u, &v);
        let sample = local_step % sample_stride == 0 || local_step == steps;
        if sample {
            residual_history.push(ResidualSample {
                step,
                residual,
                continuity: rms(&mesh.divergence(&u, &v)),
            });
            append_force_samples(
                &mesh,
                model,
                &u,
                &v,
                &p,
                eddy.as_deref(),
                step,
                &mut force_samples,
            );
            append_probe_samples(
                &mesh,
                &u,
                &v,
                &p,
                temperature.as_deref(),
                step,
                simulated_time,
                &probe_cells,
                &mut probe_samples,
            );
        }

        if let Some(tolerance) = options.tolerance {
            if local_step % check_every == 0 && residual <= tolerance {
                steady_state = true;
                advanced = local_step;
                break;
            }
        }
    }

    let divergence = mesh.divergence(&u, &v);
    let divergence_l2 = rms(&divergence);
    let mass_balance = mass_balance(&mesh, &model.boundaries, model.fluid.density, &u, &v);
    let outlets = outlet_flows(&mesh, &model.boundaries, model.fluid.density, &u, &v);
    let max_velocity = max_velocity(&u, &v);
    let quality = mesh_quality(&mesh);
    let (min_pressure, max_pressure) = pressure_range(&mesh, &p);
    let vorticity_field = vorticity(&mesh, &u, &v);
    let stream_function_field = stream_function(&mesh, &u, &v);
    let (vorticity_min, vorticity_max) = pressure_range(&mesh, &vorticity_field);
    let (stream_function_min, stream_function_max) = pressure_range(&mesh, &stream_function_field);
    let forces = build_forces(&mesh, model, &u, &v, &p, eddy.as_deref(), &force_samples);
    let probes = build_probes(model, &probe_cells, &probe_samples);
    let wall_shear_summary = wall_shear(&mesh, model, &u, &v);
    let (eddy_min, eddy_max, eddy_mean) = eddy_stats(&mesh, eddy.as_deref());
    let nusselt = compute_nusselt(&mesh, model, &u, &v, temperature.as_deref());
    let porous_summary = porous_summary(&mesh, model, &porous, &u, &v);

    let report = Report {
        format: REPORT_FORMAT.to_string(),
        version: REPORT_VERSION,
        steps: start_step + advanced,
        dt: last_dt,
        adaptive: model.time.adaptive,
        average_dt: if advanced > 0 {
            dt_sum / advanced as f64
        } else {
            last_dt
        },
        min_dt: if dt_min.is_finite() { dt_min } else { last_dt },
        max_dt: dt_max,
        time: simulated_time,
        max_velocity,
        min_pressure,
        max_pressure,
        divergence_l2,
        mesh_quality: quality,
        mass_balance,
        outlets,
        vorticity_min,
        vorticity_max,
        stream_function_min,
        stream_function_max,
        converged: if options.tolerance.is_some() {
            steady_state
        } else {
            last_stats.converged
        },
        steady_state,
        residual,
        residual_history,
        termination_reason: if steady_state {
            "steadyState".to_string()
        } else {
            "maxSteps".to_string()
        },
        pressure_converged: last_stats.converged,
        pressure_iterations: last_stats.iterations,
        pressure_residual: last_stats.residual,
        pressure_residual_initial: last_stats.initial_residual,
        forces,
        probes,
        wall_shear: wall_shear_summary,
        convection: model.convection.as_str().to_string(),
        time_scheme: model.time.scheme.as_str().to_string(),
        turbulence_model: model
            .turbulence
            .as_ref()
            .map_or("none", |turbulence| turbulence.model.as_str())
            .to_string(),
        eddy_viscosity_min: eddy_min,
        eddy_viscosity_max: eddy_max,
        eddy_viscosity_mean: eddy_mean,
        nusselt,
        porous: porous_summary,
        buoyancy: model
            .buoyancy
            .as_ref()
            .is_some_and(|buoyancy| buoyancy.beta > 0.0),
        wall_function: model.wall_function.is_some(),
    };
    Ok(Solution {
        mesh,
        u,
        v,
        p,
        temperature,
        vorticity: vorticity_field,
        stream_function: stream_function_field,
        eddy_viscosity: eddy,
        report,
    })
}

/// Solves the pressure Poisson equation `∇²p = (rho / dt) · div(u*)` in place.
///
/// Successive over-relaxation is used, with the relaxation factor derived from
/// the largest grid side. Wall and inlet sides are homogeneous Neumann
/// boundaries (the ghost value copies the adjacent cell); outlet sides are
/// homogeneous Dirichlet boundaries. Solid cells are not unknowns and are held
/// at zero pressure; a solid neighbour acts as a zero-flux face. In a fully
/// closed domain the null-space mean is removed after the sweeps, which does
/// not change the pressure gradient used for the velocity correction.
///
/// `settings.tolerance` is relative to the initial residual of this solve:
/// convergence requires `residual <= tolerance × max(initial, 1e-12)`.
pub fn solve_pressure(
    mesh: &Mesh,
    boundaries: &Boundaries,
    pressure: &mut [f64],
    divergence: &[f64],
    density: f64,
    dt: f64,
    settings: &PressureSettings,
) -> Result<PressureStats, CfdError> {
    if !density.is_finite() || density <= 0.0 {
        return Err(CfdError::new(
            "solver/invalid",
            "density must be a finite positive number",
        ));
    }
    if !dt.is_finite() || dt <= 0.0 {
        return Err(CfdError::new(
            "solver/invalid",
            "dt must be a finite positive number",
        ));
    }
    if settings.iterations == 0 {
        return Err(CfdError::new(
            "solver/invalid",
            "pressure iteration count must be greater than 0",
        ));
    }
    if !settings.tolerance.is_finite() || settings.tolerance <= 0.0 {
        return Err(CfdError::new(
            "solver/invalid",
            "pressure residual tolerance must be a finite positive number",
        ));
    }
    if pressure.len() != mesh.pressure_count() || divergence.len() != mesh.pressure_count() {
        return Err(CfdError::new(
            "solver/invalid",
            "pressure or divergence array length does not match the mesh",
        ));
    }

    for (index, value) in pressure.iter_mut().enumerate() {
        if mesh.solid[index] {
            *value = 0.0;
        }
    }

    let largest_side = mesh.nx.max(mesh.ny) as f64;
    let omega = 2.0 / (1.0 + (std::f64::consts::PI / largest_side).sin());
    let rhs_scale = density / dt;
    let denominator = 2.0 / (mesh.dx * mesh.dx) + 2.0 / (mesh.dy * mesh.dy);
    let periodic_x = boundaries.periodic_x();
    let periodic_y = boundaries.periodic_y();
    let initial_residual = pressure_residual(mesh, boundaries, pressure, divergence, rhs_scale);
    let threshold = settings.tolerance * initial_residual.max(1e-12);
    let mut history = Vec::with_capacity(settings.iterations);
    let mut residual = initial_residual;

    for _ in 0..settings.iterations {
        for j in 0..mesh.ny {
            for i in 0..mesh.nx {
                let index = mesh.pressure_index(i, j);
                if mesh.solid[index] {
                    continue;
                }
                let centre = pressure[index];
                let west = if i == 0 {
                    if periodic_x {
                        pressure[mesh.pressure_index(mesh.nx - 1, j)]
                    } else if boundaries.left.kind.is_outlet() {
                        boundaries.left.pressure
                    } else {
                        centre
                    }
                } else if mesh.solid[index - 1] {
                    centre
                } else {
                    pressure[index - 1]
                };
                let east = if i + 1 == mesh.nx {
                    if periodic_x {
                        pressure[mesh.pressure_index(0, j)]
                    } else if boundaries.right.kind.is_outlet() {
                        boundaries.right.pressure
                    } else {
                        centre
                    }
                } else if mesh.solid[index + 1] {
                    centre
                } else {
                    pressure[index + 1]
                };
                let south = if j == 0 {
                    if periodic_y {
                        pressure[mesh.pressure_index(i, mesh.ny - 1)]
                    } else if boundaries.bottom.kind.is_outlet() {
                        boundaries.bottom.pressure
                    } else {
                        centre
                    }
                } else if mesh.solid[index - mesh.nx] {
                    centre
                } else {
                    pressure[index - mesh.nx]
                };
                let north = if j + 1 == mesh.ny {
                    if periodic_y {
                        pressure[mesh.pressure_index(i, 0)]
                    } else if boundaries.top.kind.is_outlet() {
                        boundaries.top.pressure
                    } else {
                        centre
                    }
                } else if mesh.solid[index + mesh.nx] {
                    centre
                } else {
                    pressure[index + mesh.nx]
                };
                let target = ((west + east) / (mesh.dx * mesh.dx)
                    + (south + north) / (mesh.dy * mesh.dy)
                    - rhs_scale * divergence[index])
                    / denominator;
                pressure[index] += omega * (target - centre);
            }
        }
        residual = pressure_residual(mesh, boundaries, pressure, divergence, rhs_scale);
        history.push(residual);
        if residual <= threshold {
            break;
        }
    }

    if !has_dirichlet_pressure(boundaries) {
        let mut sum = 0.0;
        let mut count = 0;
        for (index, value) in pressure.iter().enumerate() {
            if !mesh.solid[index] {
                sum += value;
                count += 1;
            }
        }
        if count > 0 {
            let mean = sum / count as f64;
            for (index, value) in pressure.iter_mut().enumerate() {
                if !mesh.solid[index] {
                    *value -= mean;
                }
            }
        }
    }

    Ok(PressureStats {
        iterations: history.len(),
        initial_residual,
        residual,
        history,
        converged: residual <= threshold,
    })
}

fn has_dirichlet_pressure(boundaries: &Boundaries) -> bool {
    boundaries.left.kind.is_outlet()
        || boundaries.right.kind.is_outlet()
        || boundaries.top.kind.is_outlet()
        || boundaries.bottom.kind.is_outlet()
}

fn pressure_residual(
    mesh: &Mesh,
    boundaries: &Boundaries,
    pressure: &[f64],
    divergence: &[f64],
    rhs_scale: f64,
) -> f64 {
    let mut maximum: f64 = 0.0;
    let periodic_x = boundaries.periodic_x();
    let periodic_y = boundaries.periodic_y();
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            if mesh.solid[index] {
                continue;
            }
            let centre = pressure[index];
            let west = if i == 0 {
                if periodic_x {
                    pressure[mesh.pressure_index(mesh.nx - 1, j)]
                } else if boundaries.left.kind.is_outlet() {
                    boundaries.left.pressure
                } else {
                    centre
                }
            } else if mesh.solid[index - 1] {
                centre
            } else {
                pressure[index - 1]
            };
            let east = if i + 1 == mesh.nx {
                if periodic_x {
                    pressure[mesh.pressure_index(0, j)]
                } else if boundaries.right.kind.is_outlet() {
                    boundaries.right.pressure
                } else {
                    centre
                }
            } else if mesh.solid[index + 1] {
                centre
            } else {
                pressure[index + 1]
            };
            let south = if j == 0 {
                if periodic_y {
                    pressure[mesh.pressure_index(i, mesh.ny - 1)]
                } else if boundaries.bottom.kind.is_outlet() {
                    boundaries.bottom.pressure
                } else {
                    centre
                }
            } else if mesh.solid[index - mesh.nx] {
                centre
            } else {
                pressure[index - mesh.nx]
            };
            let north = if j + 1 == mesh.ny {
                if periodic_y {
                    pressure[mesh.pressure_index(i, 0)]
                } else if boundaries.top.kind.is_outlet() {
                    boundaries.top.pressure
                } else {
                    centre
                }
            } else if mesh.solid[index + mesh.nx] {
                centre
            } else {
                pressure[index + mesh.nx]
            };
            let laplacian = (west - 2.0 * centre + east) / (mesh.dx * mesh.dx)
                + (south - 2.0 * centre + north) / (mesh.dy * mesh.dy);
            let residual = (laplacian - rhs_scale * divergence[index]).abs();
            maximum = maximum.max(residual);
        }
    }
    maximum
}

/// Boussinesq acceleration at one face, in m/s².
///
/// The face temperature is the mean of its two adjacent fluid cells; solid
/// cells are skipped and an all-solid face contributes nothing. The returned
/// value is `−β·(T − T_ref)·g`, where `component` selects `gx` or `gy`, so a
/// hot cell under downward gravity accelerates upwards.
fn face_buoyancy(
    mesh: &Mesh,
    buoyancy: Option<&Buoyancy>,
    temperature: Option<&[f64]>,
    first: (usize, usize),
    second: (usize, usize),
    component: usize,
) -> f64 {
    let (Some(buoyancy), Some(field)) = (buoyancy, temperature) else {
        return 0.0;
    };
    if buoyancy.beta == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut count = 0_usize;
    for (i, j) in [first, second] {
        if mesh.is_solid(i, j) {
            continue;
        }
        sum += field[mesh.pressure_index(i, j)];
        count += 1;
    }
    if count == 0 {
        return 0.0;
    }
    let value = sum / count as f64;
    -buoyancy.beta * (value - buoyancy.t_ref) * buoyancy.gravity[component]
}

/// Index of the porous region that owns each cell, or `-1` for free fluid.
///
/// The first region in document order owns a cell when regions overlap, matching
/// the obstacle rule. Solid cells stay free of porous ownership because they are
/// already impermeable.
fn porous_cells(mesh: &Mesh, model: &Model) -> Vec<i32> {
    let mut owner = vec![-1; mesh.pressure_count()];
    if model.porous.is_empty() {
        return owner;
    }
    for (index, region) in model.porous.iter().enumerate() {
        for j in 0..mesh.ny {
            for i in 0..mesh.nx {
                let cell = mesh.pressure_index(i, j);
                if owner[cell] >= 0 || mesh.solid[cell] {
                    continue;
                }
                let (x, y) = mesh.cell_center(i, j);
                if region.contains(x, y) {
                    owner[cell] = index as i32;
                }
            }
        }
    }
    owner
}

/// Region that owns a velocity face, taken from its first owned adjacent cell.
fn face_porous_owner(
    mesh: &Mesh,
    porous: &[i32],
    first: Option<(usize, usize)>,
    second: Option<(usize, usize)>,
) -> i32 {
    if porous.is_empty() {
        return -1;
    }
    for (i, j) in [first, second].into_iter().flatten() {
        let owner = porous[mesh.pressure_index(i, j)];
        if owner >= 0 {
            return owner;
        }
    }
    -1
}

/// Darcy linear and Forchheimer quadratic drag coefficients of one region.
///
/// Returns `(ν/K, C_F/√K)`; both are zero for a free-fluid face.
fn porous_coefficient(model: &Model, owner: i32) -> (f64, f64) {
    let Ok(index) = usize::try_from(owner) else {
        return (0.0, 0.0);
    };
    let Some(region) = model.porous.get(index) else {
        return (0.0, 0.0);
    };
    (
        model.fluid.viscosity / region.permeability,
        region.inertial / region.permeability.sqrt(),
    )
}

/// Porous acceleration `−(ν/K)·u − (C_F/√K)·|U|·u` at one velocity face.
fn porous_drag(model: &Model, owner: i32, velocity: f64, transverse: f64) -> f64 {
    let (linear, quadratic) = porous_coefficient(model, owner);
    if linear == 0.0 && quadratic == 0.0 {
        return 0.0;
    }
    let speed = velocity.hypot(transverse);
    -(linear + quadratic * speed) * velocity
}

/// Largest explicit damping rate `ν/K + C_F·|U|/√K` over all regions (1/s).
fn porous_rate(model: &Model, max_speed: f64) -> f64 {
    model
        .porous
        .iter()
        .map(|region| {
            model.fluid.viscosity / region.permeability
                + region.inertial / region.permeability.sqrt() * max_speed
        })
        .fold(0.0_f64, f64::max)
}

#[allow(clippy::too_many_arguments)]
fn momentum_derivative(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    temperature: Option<&[f64]>,
    eddy: Option<&[f64]>,
    porous: &[i32],
    du: &mut [f64],
    dv: &mut [f64],
) {
    let nu = model.fluid.viscosity;
    let rho = model.fluid.density;
    let dx = mesh.dx;
    let dy = mesh.dy;
    let boundaries = &model.boundaries;
    let periodic_x = boundaries.periodic_x();
    let periodic_y = boundaries.periodic_y();
    let (source_x, source_y) = model.pressure_gradient.map_or((0.0, 0.0), |gradient| {
        (-gradient[0] / rho, -gradient[1] / rho)
    });

    for j in 0..mesh.ny {
        let start = if periodic_x { 0 } else { 1 };
        for i in start..mesh.nx {
            let index = mesh.u_index(i, j);
            if !mesh.u_face_open(i, j) {
                du[index] = 0.0;
                continue;
            }
            let centre = u[index];
            let du_dx =
                u_convective_derivative(mesh, u, i, j, centre, model.convection, periodic_x);
            let west_face = if i == 0 { mesh.nx - 1 } else { i - 1 };
            let v_face = 0.25
                * (v[mesh.v_index(west_face, j)]
                    + v[mesh.v_index(west_face, j + 1)]
                    + v[mesh.v_index(i, j)]
                    + v[mesh.v_index(i, j + 1)]);
            let dv_dy = 0.5
                * ((v[mesh.v_index(west_face, j + 1)] - v[mesh.v_index(west_face, j)]) / dy
                    + (v[mesh.v_index(i, j + 1)] - v[mesh.v_index(i, j)]) / dy);
            let convection = centre * du_dx + v_face * dv_dy;

            let below = if periodic_y {
                u[mesh.u_index(i, if j == 0 { mesh.ny - 1 } else { j - 1 })]
            } else if j == 0 {
                match boundaries.bottom.kind {
                    BoundaryKind::Outlet | BoundaryKind::Convective | BoundaryKind::Periodic => {
                        u[mesh.u_index(i, 1)]
                    }
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.bottom.velocity[0] - centre
                    }
                }
            } else {
                u[mesh.u_index(i, j - 1)]
            };
            let above = if periodic_y {
                u[mesh.u_index(i, if j + 1 == mesh.ny { 0 } else { j + 1 })]
            } else if j + 1 == mesh.ny {
                match boundaries.top.kind {
                    BoundaryKind::Outlet | BoundaryKind::Convective | BoundaryKind::Periodic => {
                        u[mesh.u_index(i, mesh.ny - 2)]
                    }
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.top.velocity[0] - centre
                    }
                }
            } else {
                u[mesh.u_index(i, j + 1)]
            };
            let west = if i == 0 {
                u[mesh.u_index(mesh.nx - 1, j)]
            } else {
                u[mesh.u_index(i - 1, j)]
            };
            let diffusion = (u[mesh.u_index(i + 1, j)] - 2.0 * centre + west) / (dx * dx)
                + (above - 2.0 * centre + below) / (dy * dy);
            let pressure_west = if i == 0 {
                mesh.pressure_index(mesh.nx - 1, j)
            } else {
                mesh.pressure_index(i - 1, j)
            };
            let pressure_gradient = (p[mesh.pressure_index(i, j)] - p[pressure_west]) / dx;
            let effective = nu + u_face_eddy(mesh, eddy, i, j);
            let porous_owner = face_porous_owner(
                mesh,
                porous,
                (i > 0).then(|| (i - 1, j)),
                (i < mesh.nx).then_some((i, j)),
            );
            let buoyancy_x = if i == 0 {
                if periodic_x {
                    face_buoyancy(
                        mesh,
                        model.buoyancy.as_ref(),
                        temperature,
                        (mesh.nx - 1, j),
                        (0, j),
                        0,
                    )
                } else {
                    0.0
                }
            } else {
                face_buoyancy(
                    mesh,
                    model.buoyancy.as_ref(),
                    temperature,
                    (i - 1, j),
                    (i, j),
                    0,
                )
            };
            du[index] = -convection + effective * diffusion - pressure_gradient / rho
                + source_x
                + buoyancy_x
                + porous_drag(model, porous_owner, centre, v_face);
        }
        if periodic_x {
            du[mesh.u_index(mesh.nx, j)] = du[mesh.u_index(0, j)];
        }
    }

    let start = if periodic_y { 0 } else { 1 };
    for j in start..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.v_index(i, j);
            if !mesh.v_face_open(i, j) {
                dv[index] = 0.0;
                continue;
            }
            let centre = v[index];
            let dv_dy =
                v_convective_derivative(mesh, v, i, j, centre, model.convection, periodic_y);
            let south_face = if j == 0 { mesh.ny - 1 } else { j - 1 };
            let u_face = 0.25
                * (u[mesh.u_index(i, j)]
                    + u[mesh.u_index(i + 1, j)]
                    + u[mesh.u_index(i, south_face)]
                    + u[mesh.u_index(i + 1, south_face)]);
            let du_dx = 0.5
                * ((u[mesh.u_index(i + 1, j)] - u[mesh.u_index(i, j)]) / dx
                    + (u[mesh.u_index(i + 1, south_face)] - u[mesh.u_index(i, south_face)]) / dx);
            let convection = u_face * du_dx + centre * dv_dy;

            let left = if periodic_x {
                v[mesh.v_index(if i == 0 { mesh.nx - 1 } else { i - 1 }, j)]
            } else if i == 0 {
                match boundaries.left.kind {
                    BoundaryKind::Outlet | BoundaryKind::Convective | BoundaryKind::Periodic => {
                        v[mesh.v_index(1, j)]
                    }
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.left.velocity[1] - centre
                    }
                }
            } else {
                v[mesh.v_index(i - 1, j)]
            };
            let right = if periodic_x {
                v[mesh.v_index(if i + 1 == mesh.nx { 0 } else { i + 1 }, j)]
            } else if i + 1 == mesh.nx {
                match boundaries.right.kind {
                    BoundaryKind::Outlet | BoundaryKind::Convective | BoundaryKind::Periodic => {
                        v[mesh.v_index(mesh.nx - 2, j)]
                    }
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.right.velocity[1] - centre
                    }
                }
            } else {
                v[mesh.v_index(i + 1, j)]
            };
            let south = if periodic_y && j == 0 {
                v[mesh.v_index(i, mesh.ny - 1)]
            } else {
                v[mesh.v_index(i, j - 1)]
            };
            let north = if periodic_y && j + 1 == mesh.ny {
                v[mesh.v_index(i, 0)]
            } else {
                v[mesh.v_index(i, j + 1)]
            };
            let diffusion = (right - 2.0 * centre + left) / (dx * dx)
                + (north - 2.0 * centre + south) / (dy * dy);
            let pressure_south = if j == 0 {
                mesh.pressure_index(i, mesh.ny - 1)
            } else {
                mesh.pressure_index(i, j - 1)
            };
            let pressure_gradient = (p[mesh.pressure_index(i, j)] - p[pressure_south]) / dy;
            let effective = nu + v_face_eddy(mesh, eddy, i, j);
            let porous_owner = face_porous_owner(
                mesh,
                porous,
                (j > 0).then(|| (i, j - 1)),
                (j < mesh.ny).then_some((i, j)),
            );
            let buoyancy_y = if j == 0 {
                if periodic_y {
                    face_buoyancy(
                        mesh,
                        model.buoyancy.as_ref(),
                        temperature,
                        (i, mesh.ny - 1),
                        (i, 0),
                        1,
                    )
                } else {
                    0.0
                }
            } else {
                face_buoyancy(
                    mesh,
                    model.buoyancy.as_ref(),
                    temperature,
                    (i, j - 1),
                    (i, j),
                    1,
                )
            };
            dv[index] = -convection + effective * diffusion - pressure_gradient / rho
                + source_y
                + buoyancy_y
                + porous_drag(model, porous_owner, centre, u_face);
        }
    }
    if periodic_y {
        for i in 0..mesh.nx {
            dv[mesh.v_index(i, mesh.ny)] = dv[mesh.v_index(i, 0)];
        }
    }
}

/// Advances the tentative velocity with explicit Euler.
#[allow(clippy::too_many_arguments)]
fn momentum_euler(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    temperature: Option<&[f64]>,
    eddy: Option<&[f64]>,
    porous: &[i32],
    dt: f64,
    u_star: &mut [f64],
    v_star: &mut [f64],
    du: &mut [f64],
    dv: &mut [f64],
) {
    momentum_derivative(mesh, model, u, v, p, temperature, eddy, porous, du, dv);
    for index in 0..u.len() {
        u_star[index] = u[index] + dt * du[index];
    }
    for index in 0..v.len() {
        v_star[index] = v[index] + dt * dv[index];
    }
}

/// Advances the tentative velocity with explicit second-order Runge-Kutta
/// (Heun), keeping the pressure at its current value and projecting afterwards.
#[allow(clippy::too_many_arguments)]
fn momentum_heun(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    temperature: Option<&[f64]>,
    eddy: Option<&[f64]>,
    porous: &[i32],
    dt: f64,
    u_star: &mut [f64],
    v_star: &mut [f64],
) {
    let mut u1 = u.to_vec();
    let mut v1 = v.to_vec();
    let mut du1 = vec![0.0; u.len()];
    let mut dv1 = vec![0.0; v.len()];
    let mut du2 = vec![0.0; u.len()];
    let mut dv2 = vec![0.0; v.len()];

    momentum_derivative(
        mesh,
        model,
        u,
        v,
        p,
        temperature,
        eddy,
        porous,
        &mut du1,
        &mut dv1,
    );
    for index in 0..u.len() {
        u1[index] = u[index] + dt * du1[index];
    }
    for index in 0..v.len() {
        v1[index] = v[index] + dt * dv1[index];
    }
    mesh.apply_velocity_boundaries(&model.boundaries, &mut u1, &mut v1);

    momentum_derivative(
        mesh,
        model,
        &u1,
        &v1,
        p,
        temperature,
        eddy,
        porous,
        &mut du2,
        &mut dv2,
    );
    for index in 0..u.len() {
        u_star[index] = u[index] + 0.5 * dt * (du1[index] + du2[index]);
    }
    for index in 0..v.len() {
        v_star[index] = v[index] + 0.5 * dt * (dv1[index] + dv2[index]);
    }
}

/// Convective derivative along x at u-face `(i, j)`.
///
/// QUICK falls back to first-order upwind when its quadratic stencil would
/// leave the domain, so the first and last interior faces stay bounded.
fn u_convective_derivative(
    mesh: &Mesh,
    u: &[f64],
    i: usize,
    j: usize,
    velocity: f64,
    scheme: ConvectionScheme,
    periodic: bool,
) -> f64 {
    let dx = mesh.dx;
    let value = |k: isize| {
        let index = if periodic {
            k.rem_euclid(mesh.nx as isize) as usize
        } else {
            k as usize
        };
        u[mesh.u_index(index, j)]
    };
    let upwind = |velocity: f64| {
        if velocity >= 0.0 {
            (value(i as isize) - value(i as isize - 1)) / dx
        } else {
            (value(i as isize + 1) - value(i as isize)) / dx
        }
    };
    match scheme {
        ConvectionScheme::Upwind => upwind(velocity),
        ConvectionScheme::Central => {
            (value(i as isize + 1) - value(i as isize - 1)) / (2.0 * mesh.dx)
        }
        ConvectionScheme::Quick => {
            if periodic {
                if velocity >= 0.0 {
                    let east = 0.375 * value(i as isize + 1) + 0.75 * value(i as isize)
                        - 0.125 * value(i as isize - 1);
                    let west = 0.375 * value(i as isize) + 0.75 * value(i as isize - 1)
                        - 0.125 * value(i as isize - 2);
                    (east - west) / dx
                } else {
                    let east = 0.375 * value(i as isize) + 0.75 * value(i as isize + 1)
                        - 0.125 * value(i as isize + 2);
                    let west = 0.375 * value(i as isize - 1) + 0.75 * value(i as isize)
                        - 0.125 * value(i as isize + 1);
                    (east - west) / dx
                }
            } else if velocity >= 0.0 {
                if i < 2 {
                    return upwind(velocity);
                }
                let east = 0.375 * value(i as isize + 1) + 0.75 * value(i as isize)
                    - 0.125 * value(i as isize - 1);
                let west = 0.375 * value(i as isize) + 0.75 * value(i as isize - 1)
                    - 0.125 * value(i as isize - 2);
                (east - west) / dx
            } else {
                if i + 2 > mesh.nx {
                    return upwind(velocity);
                }
                let east = 0.375 * value(i as isize) + 0.75 * value(i as isize + 1)
                    - 0.125 * value(i as isize + 2);
                let west = 0.375 * value(i as isize - 1) + 0.75 * value(i as isize)
                    - 0.125 * value(i as isize + 1);
                (east - west) / dx
            }
        }
    }
}

/// Convective derivative along y at v-face `(i, j)`.
fn v_convective_derivative(
    mesh: &Mesh,
    v: &[f64],
    i: usize,
    j: usize,
    velocity: f64,
    scheme: ConvectionScheme,
    periodic: bool,
) -> f64 {
    let dy = mesh.dy;
    let value = |k: isize| {
        let index = if periodic {
            k.rem_euclid(mesh.ny as isize) as usize
        } else {
            k as usize
        };
        v[mesh.v_index(i, index)]
    };
    let upwind = |velocity: f64| {
        if velocity >= 0.0 {
            (value(j as isize) - value(j as isize - 1)) / dy
        } else {
            (value(j as isize + 1) - value(j as isize)) / dy
        }
    };
    match scheme {
        ConvectionScheme::Upwind => upwind(velocity),
        ConvectionScheme::Central => {
            (value(j as isize + 1) - value(j as isize - 1)) / (2.0 * mesh.dy)
        }
        ConvectionScheme::Quick => {
            if periodic {
                if velocity >= 0.0 {
                    let north = 0.375 * value(j as isize + 1) + 0.75 * value(j as isize)
                        - 0.125 * value(j as isize - 1);
                    let south = 0.375 * value(j as isize) + 0.75 * value(j as isize - 1)
                        - 0.125 * value(j as isize - 2);
                    (north - south) / dy
                } else {
                    let north = 0.375 * value(j as isize) + 0.75 * value(j as isize + 1)
                        - 0.125 * value(j as isize + 2);
                    let south = 0.375 * value(j as isize - 1) + 0.75 * value(j as isize)
                        - 0.125 * value(j as isize + 1);
                    (north - south) / dy
                }
            } else if velocity >= 0.0 {
                if j < 2 {
                    return upwind(velocity);
                }
                let north = 0.375 * value(j as isize + 1) + 0.75 * value(j as isize)
                    - 0.125 * value(j as isize - 1);
                let south = 0.375 * value(j as isize) + 0.75 * value(j as isize - 1)
                    - 0.125 * value(j as isize - 2);
                (north - south) / dy
            } else {
                if j + 2 > mesh.ny {
                    return upwind(velocity);
                }
                let north = 0.375 * value(j as isize) + 0.75 * value(j as isize + 1)
                    - 0.125 * value(j as isize + 2);
                let south = 0.375 * value(j as isize - 1) + 0.75 * value(j as isize)
                    - 0.125 * value(j as isize + 1);
                (north - south) / dy
            }
        }
    }
}

fn eddy_viscosity_field(mesh: &Mesh, model: &Model, u: &[f64], v: &[f64]) -> Option<Vec<f64>> {
    let turbulence = model.turbulence.as_ref()?;
    match turbulence.model {
        TurbulenceModel::None => None,
        TurbulenceModel::Constant => {
            let mut field = vec![turbulence.nu_t; mesh.pressure_count()];
            for (index, value) in field.iter_mut().enumerate() {
                if mesh.solid[index] {
                    *value = 0.0;
                }
            }
            Some(field)
        }
        TurbulenceModel::MixingLength => Some(mixing_length_field(mesh, turbulence, u, v)),
    }
}

fn mixing_length_field(mesh: &Mesh, turbulence: &Turbulence, u: &[f64], v: &[f64]) -> Vec<f64> {
    let length_squared = turbulence.mixing_length * turbulence.mixing_length;
    let mut field = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            if mesh.solid[index] {
                continue;
            }
            let du_dx = cell_gradient(mesh, u, v, i, j, Gradient::UDx);
            let du_dy = cell_gradient(mesh, u, v, i, j, Gradient::UDy);
            let dv_dx = cell_gradient(mesh, u, v, i, j, Gradient::VDx);
            let dv_dy = cell_gradient(mesh, u, v, i, j, Gradient::VDy);
            let strain =
                (2.0 * (du_dx * du_dx + dv_dy * dv_dy) + (du_dy + dv_dx) * (du_dy + dv_dx)).sqrt();
            field[index] = length_squared * strain;
        }
    }
    field
}

#[derive(Clone, Copy)]
enum Gradient {
    UDx,
    UDy,
    VDx,
    VDy,
}

fn cell_gradient(mesh: &Mesh, u: &[f64], v: &[f64], i: usize, j: usize, kind: Gradient) -> f64 {
    let component = |ci: usize, cj: usize| {
        let (ux, uy) = mesh.cell_velocity(u, v, ci, cj);
        match kind {
            Gradient::UDx | Gradient::UDy => ux,
            Gradient::VDx | Gradient::VDy => uy,
        }
    };
    let horizontal = matches!(kind, Gradient::UDx | Gradient::VDx);
    if horizontal {
        if i == 0 {
            (component(1, j) - component(0, j)) / mesh.dx
        } else if i + 1 == mesh.nx {
            (component(mesh.nx - 1, j) - component(mesh.nx - 2, j)) / mesh.dx
        } else {
            (component(i + 1, j) - component(i - 1, j)) / (2.0 * mesh.dx)
        }
    } else if j == 0 {
        (component(i, 1) - component(i, 0)) / mesh.dy
    } else if j + 1 == mesh.ny {
        (component(i, mesh.ny - 1) - component(i, mesh.ny - 2)) / mesh.dy
    } else {
        (component(i, j + 1) - component(i, j - 1)) / (2.0 * mesh.dy)
    }
}

fn u_face_eddy(mesh: &Mesh, eddy: Option<&[f64]>, i: usize, j: usize) -> f64 {
    let Some(field) = eddy else {
        return 0.0;
    };
    let mut sum = 0.0;
    let mut count = 0;
    if i > 0 && !mesh.is_solid(i - 1, j) {
        sum += field[mesh.pressure_index(i - 1, j)];
        count += 1;
    }
    if i < mesh.nx && !mesh.is_solid(i, j) {
        sum += field[mesh.pressure_index(i, j)];
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn v_face_eddy(mesh: &Mesh, eddy: Option<&[f64]>, i: usize, j: usize) -> f64 {
    let Some(field) = eddy else {
        return 0.0;
    };
    let mut sum = 0.0;
    let mut count = 0;
    if j > 0 && !mesh.is_solid(i, j - 1) {
        sum += field[mesh.pressure_index(i, j - 1)];
        count += 1;
    }
    if j < mesh.ny && !mesh.is_solid(i, j) {
        sum += field[mesh.pressure_index(i, j)];
        count += 1;
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
}

fn eddy_stats(mesh: &Mesh, eddy: Option<&[f64]>) -> (f64, f64, f64) {
    let Some(field) = eddy else {
        return (0.0, 0.0, 0.0);
    };
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut sum = 0.0;
    let mut count = 0;
    for (index, value) in field.iter().enumerate() {
        if mesh.solid[index] {
            continue;
        }
        min = min.min(*value);
        max = max.max(*value);
        sum += *value;
        count += 1;
    }
    if count == 0 || !min.is_finite() || !max.is_finite() {
        (0.0, 0.0, 0.0)
    } else {
        (min, max, sum / count as f64)
    }
}

/// Per-region mean cell speed and Darcy–Forchheimer drag acceleration.
fn porous_summary(
    mesh: &Mesh,
    model: &Model,
    porous: &[i32],
    u: &[f64],
    v: &[f64],
) -> Vec<PorousSummary> {
    if model.porous.is_empty() {
        return Vec::new();
    }
    let mut cells = vec![0_usize; model.porous.len()];
    let mut speed_sum = vec![0.0; model.porous.len()];
    let mut drag_sum = vec![0.0; model.porous.len()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let cell = mesh.pressure_index(i, j);
            let owner = porous[cell];
            let Ok(index) = usize::try_from(owner) else {
                continue;
            };
            if mesh.solid[cell] {
                continue;
            }
            let (ux, uy) = mesh.cell_velocity(u, v, i, j);
            let speed = ux.hypot(uy);
            let (linear, quadratic) = porous_coefficient(model, owner);
            cells[index] += 1;
            speed_sum[index] += speed;
            drag_sum[index] += linear * speed + quadratic * speed * speed;
        }
    }
    model
        .porous
        .iter()
        .enumerate()
        .map(|(index, region)| {
            let count = cells[index];
            PorousSummary {
                id: if region.id().is_empty() {
                    format!("porous-{}", index + 1)
                } else {
                    region.id().to_string()
                },
                cells: count,
                mean_speed: if count > 0 {
                    speed_sum[index] / count as f64
                } else {
                    0.0
                },
                mean_drag: if count > 0 {
                    drag_sum[index] / count as f64
                } else {
                    0.0
                },
            }
        })
        .collect()
}

fn project_velocity(
    mesh: &Mesh,
    boundaries: &Boundaries,
    density: f64,
    dt: f64,
    p: &[f64],
    u: &mut [f64],
    v: &mut [f64],
) {
    let factor = dt / density;
    let periodic_x = boundaries.periodic_x();
    let periodic_y = boundaries.periodic_y();
    for j in 0..mesh.ny {
        let start = if periodic_x { 0 } else { 1 };
        for i in start..mesh.nx {
            let index = mesh.u_index(i, j);
            if !mesh.u_face_open(i, j) {
                u[index] = 0.0;
                continue;
            }
            let west = if i == 0 {
                mesh.pressure_index(mesh.nx - 1, j)
            } else {
                mesh.pressure_index(i - 1, j)
            };
            u[index] -= factor * (p[mesh.pressure_index(i, j)] - p[west]) / mesh.dx;
        }
        if periodic_x {
            u[mesh.u_index(mesh.nx, j)] = u[mesh.u_index(0, j)];
        }
    }
    let start = if periodic_y { 0 } else { 1 };
    for j in start..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.v_index(i, j);
            if !mesh.v_face_open(i, j) {
                v[index] = 0.0;
                continue;
            }
            let south = if j == 0 {
                mesh.pressure_index(i, mesh.ny - 1)
            } else {
                mesh.pressure_index(i, j - 1)
            };
            v[index] -= factor * (p[mesh.pressure_index(i, j)] - p[south]) / mesh.dy;
        }
    }
    if periodic_y {
        for i in 0..mesh.nx {
            v[mesh.v_index(i, mesh.ny)] = v[mesh.v_index(i, 0)];
        }
    }
}

fn check_explicit_stability(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    eddy: Option<&[f64]>,
) -> Result<(), CfdError> {
    let mut report = CheckReport::new();
    let max_speed = max_velocity(u, v).max(max_boundary_speed(model));
    let cfl = max_speed * model.time.dt / mesh.dx.min(mesh.dy);
    if cfl > 1.0 {
        report.push(Issue::error(
            "solver/unstable",
            format!(
                "explicit convection is unstable: CFL = {cfl:.3} > 1; reduce dt or refine the mesh"
            ),
        ));
    }
    let diffusion_number = (model.fluid.viscosity + max_eddy_viscosity(eddy))
        * model.time.dt
        * (1.0 / (mesh.dx * mesh.dx) + 1.0 / (mesh.dy * mesh.dy));
    if diffusion_number > 0.5 {
        report.push(Issue::error(
            "solver/unstable",
            format!("explicit diffusion is unstable: nu·dt·(1/dx²+1/dy²) = {diffusion_number:.3} > 0.5; reduce dt, refine the mesh or lower the eddy viscosity"),
        ));
    }
    if let Some(scalar) = &model.scalar {
        let scalar_number = scalar.diffusivity
            * model.time.dt
            * (1.0 / (mesh.dx * mesh.dx) + 1.0 / (mesh.dy * mesh.dy));
        if scalar_number > 0.5 {
            report.push(Issue::error(
                "solver/unstable",
                format!("explicit scalar diffusion is unstable: κ·dt·(1/dx²+1/dy²) = {scalar_number:.3} > 0.5; reduce dt, refine the mesh or lower diffusivity"),
            ));
        }
    }
    if let Some(issue) = report.issues.first() {
        return Err(CfdError::new(issue.code.clone(), issue.message.clone()));
    }
    check_porous_stability(model, max_speed, model.time.dt)
}

/// Porous drag must be resolved by the explicit step: the damping rate
/// `(ν/K + C_F·|U|/√K)` times `dt` may not exceed one.
fn check_porous_stability(model: &Model, max_speed: f64, dt: f64) -> Result<(), CfdError> {
    let rate = porous_rate(model, max_speed);
    if rate > 0.0 && rate * dt > 1.0 {
        return Err(CfdError::new(
            "solver/unstable",
            format!(
                "explicit porous drag is unstable: (ν/K + C_F·|U|/√K)·dt = {:.3} > 1; reduce dt or increase the permeability",
                rate * dt
            ),
        ));
    }
    Ok(())
}

fn max_eddy_viscosity(eddy: Option<&[f64]>) -> f64 {
    eddy.map_or(0.0, |field| {
        field
            .iter()
            .fold(0.0_f64, |maximum, value| maximum.max(*value))
    })
}

fn max_boundary_speed(model: &Model) -> f64 {
    [
        &model.boundaries.left,
        &model.boundaries.right,
        &model.boundaries.top,
        &model.boundaries.bottom,
    ]
    .iter()
    .map(|boundary| boundary.velocity[0].hypot(boundary.velocity[1]))
    .fold(0.0_f64, f64::max)
}

/// Thermal diffusivity of the configured passive scalar, if any.
fn scalar_diffusivity(model: &Model) -> f64 {
    let Some(config) = model.scalar.as_ref() else {
        return 0.0;
    };
    if let Some(conductivity) = model
        .thermal
        .as_ref()
        .and_then(|thermal| thermal.conductivity)
        .filter(|value| *value > 0.0)
    {
        let capacity =
            model.fluid.density * model.thermal.as_ref().map_or(0.0, |t| t.specific_heat);
        if capacity.is_finite() && capacity > 0.0 {
            return conductivity / capacity;
        }
    }
    config.diffusivity
}

/// Largest explicit-stable step at the current velocity and eddy viscosity.
///
/// The convective limit uses the configured `targetCfl`; diffusion uses the
/// classic `0.5·h²/ν` bound for the two-dimensional Laplacian and also accounts
/// for the passive scalar. The configured `time.dt` is always an upper bound, so
/// enabling adaptivity can only reduce a step.
fn adaptive_dt(mesh: &Mesh, model: &Model, u: &[f64], v: &[f64], eddy: Option<&[f64]>) -> f64 {
    let max_speed = max_velocity(u, v).max(max_boundary_speed(model));
    let cell_factor = 1.0 / (mesh.dx * mesh.dx) + 1.0 / (mesh.dy * mesh.dy);
    let mut limit = f64::INFINITY;
    if max_speed > 0.0 {
        limit = limit.min(model.time.target_cfl * mesh.dx.min(mesh.dy) / max_speed);
    }
    let viscosity = model.fluid.viscosity + max_eddy_viscosity(eddy);
    if viscosity > 0.0 {
        limit = limit.min(0.5 / (viscosity * cell_factor));
    }
    let diffusivity = scalar_diffusivity(model);
    if diffusivity > 0.0 {
        limit = limit.min(0.5 / (diffusivity * cell_factor));
    }
    let drag_rate = porous_rate(model, max_speed);
    if drag_rate > 0.0 {
        limit = limit.min(0.9 / drag_rate);
    }
    if limit.is_finite() && limit > 0.0 {
        limit.min(model.time.dt)
    } else {
        model.time.dt
    }
}

fn initial_temperature(mesh: &Mesh, config: &ScalarConfig) -> Vec<f64> {
    let mut field = vec![config.initial; mesh.pressure_count()];
    for (index, value) in field.iter_mut().enumerate() {
        if mesh.solid[index] {
            *value = 0.0;
        }
    }
    field
}

/// Advances the passive scalar by one explicit advection-diffusion step.
///
/// The scheme mirrors the momentum step: the selected convection scheme for the
/// advective flux and central diffusion on cell centres. Solid faces carry zero
/// flux and solid cells hold no scalar value. Inlet faces use
/// `config.inlet_value`; walls and outlets use a zero-gradient ghost. The
/// explicit diffusion number is checked before updating and `solver/unstable`
/// is returned when it exceeds `0.5`.
#[allow(clippy::too_many_arguments)]
pub fn transport_scalar(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    scheme: ConvectionScheme,
    dt: f64,
    u: &[f64],
    v: &[f64],
    temperature: &mut [f64],
) -> Result<(), CfdError> {
    scalar_step(
        mesh,
        boundaries,
        config,
        None,
        config.diffusivity,
        0.0,
        scheme,
        dt,
        u,
        v,
        temperature,
    )
}

/// Advances the temperature field with optional thermal boundaries and source.
///
/// This is the energy-equation variant of [`transport_scalar`]. The thermal
/// conductivity `k` is `thermal.conductivity` when provided, otherwise
/// `rho · cp · diffusivity`; the transported diffusivity is `k / (rho · cp)`.
/// Domain sides apply fixed temperature or fixed heat flux, all obstacle
/// surfaces share `obstacleTemperature`/`obstacleHeatFlux`, and `heatSource`
/// adds `S / (rho · cp)` per second uniformly.
#[allow(clippy::too_many_arguments)]
pub fn transport_thermal(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    thermal: Option<&ThermalConfig>,
    density: f64,
    scheme: ConvectionScheme,
    dt: f64,
    u: &[f64],
    v: &[f64],
    temperature: &mut [f64],
) -> Result<(), CfdError> {
    let Some(thermal) = thermal else {
        return transport_scalar(mesh, boundaries, config, scheme, dt, u, v, temperature);
    };
    if !density.is_finite() || density <= 0.0 {
        return Err(CfdError::new(
            "solver/invalid",
            "density must be a finite positive number",
        ));
    }
    if !thermal.specific_heat.is_finite() || thermal.specific_heat <= 0.0 {
        return Err(CfdError::new(
            "thermal/invalid",
            "thermal.specificHeat must be a finite positive number",
        ));
    }
    let conductivity = thermal
        .conductivity
        .filter(|value| *value > 0.0)
        .unwrap_or(density * thermal.specific_heat * config.diffusivity);
    let kappa = if conductivity > 0.0 {
        conductivity / (density * thermal.specific_heat)
    } else {
        config.diffusivity
    };
    let context = ThermalContext {
        config: thermal,
        kappa,
        density,
    };
    let source = thermal.heat_source / (density * thermal.specific_heat);
    scalar_step(
        mesh,
        boundaries,
        config,
        Some(&context),
        kappa,
        source,
        scheme,
        dt,
        u,
        v,
        temperature,
    )
}

struct ThermalContext<'a> {
    config: &'a ThermalConfig,
    kappa: f64,
    density: f64,
}

impl ThermalContext<'_> {
    /// Wall flux in the positive face-normal direction, where `sign` is `+1`
    /// for west/south faces and `-1` for east/north faces.
    fn wall_flux(
        &self,
        temperature: Option<f64>,
        heat_flux: Option<f64>,
        centre: f64,
        distance: f64,
        sign: f64,
    ) -> f64 {
        if let Some(wall) = temperature {
            sign * self.kappa * (wall - centre) / distance
        } else if let Some(flux) = heat_flux {
            sign * flux / (self.density * self.config.specific_heat)
        } else {
            0.0
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scalar_step(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
    kappa: f64,
    source: f64,
    scheme: ConvectionScheme,
    dt: f64,
    u: &[f64],
    v: &[f64],
    temperature: &mut [f64],
) -> Result<(), CfdError> {
    if !dt.is_finite() || dt <= 0.0 {
        return Err(CfdError::new(
            "solver/invalid",
            "dt must be a finite positive number",
        ));
    }
    if !kappa.is_finite() || kappa < 0.0 {
        return Err(CfdError::new(
            "scalar/invalid",
            "scalar.diffusivity must be a finite non-negative number",
        ));
    }
    if temperature.len() != mesh.pressure_count() {
        return Err(CfdError::new(
            "solver/invalid",
            "scalar array length does not match the mesh",
        ));
    }
    let diffusion_number = kappa * dt * (1.0 / (mesh.dx * mesh.dx) + 1.0 / (mesh.dy * mesh.dy));
    if diffusion_number > 0.5 {
        return Err(CfdError::new(
            "solver/unstable",
            format!("explicit scalar diffusion is unstable: κ·dt·(1/dx²+1/dy²) = {diffusion_number:.3} > 0.5"),
        ));
    }

    let mut next = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            if mesh.solid[index] {
                next[index] = 0.0;
                continue;
            }
            let centre = temperature[index];
            let west = scalar_west_flux(
                mesh,
                boundaries,
                config,
                context,
                kappa,
                scheme,
                u,
                temperature,
                i,
                j,
                centre,
            );
            let east = scalar_east_flux(
                mesh,
                boundaries,
                config,
                context,
                kappa,
                scheme,
                u,
                temperature,
                i,
                j,
                centre,
            );
            let south = scalar_south_flux(
                mesh,
                boundaries,
                config,
                context,
                kappa,
                scheme,
                v,
                temperature,
                i,
                j,
                centre,
            );
            let north = scalar_north_flux(
                mesh,
                boundaries,
                config,
                context,
                kappa,
                scheme,
                v,
                temperature,
                i,
                j,
                centre,
            );
            next[index] =
                centre - dt * ((east - west) / mesh.dx + (north - south) / mesh.dy) + dt * source;
        }
    }
    temperature.copy_from_slice(&next);
    if !temperature.iter().all(|value| value.is_finite()) {
        return Err(CfdError::new(
            "solver/non-finite",
            "scalar transport produced NaN or Inf; reduce dt or lower diffusivity",
        ));
    }
    Ok(())
}

fn side_advect_ghost(
    side: Side,
    centre: f64,
    inlet: bool,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
) -> f64 {
    if let Some(wall) = context.and_then(|context| context.config.side(side).temperature) {
        return wall;
    }
    if inlet {
        config.inlet_value
    } else {
        centre
    }
}

/// Interpolates the advected scalar to one interior face.
///
/// `upwind` and `downwind` are the values on the two sides of the face and
/// `upwind2` is the next cell on the upstream side, used only by QUICK. A
/// missing upstream neighbour means the interpolation falls back to first-order
/// upwind near the domain edge.
fn face_value(scheme: ConvectionScheme, upwind: f64, upwind2: Option<f64>, downwind: f64) -> f64 {
    match scheme {
        ConvectionScheme::Upwind => upwind,
        ConvectionScheme::Central => 0.5 * (upwind + downwind),
        ConvectionScheme::Quick => upwind2.map_or(upwind, |second| {
            (6.0 * upwind + 3.0 * downwind - second) / 8.0
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn scalar_west_flux(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
    kappa: f64,
    scheme: ConvectionScheme,
    u: &[f64],
    temperature: &[f64],
    i: usize,
    j: usize,
    centre: f64,
) -> f64 {
    let index = mesh.pressure_index(i, j);
    if i == 0 {
        let velocity = u[mesh.u_index(0, j)];
        let inlet = boundaries.left.kind == BoundaryKind::Inlet;
        let ghost = side_advect_ghost(Side::Left, centre, inlet, config, context);
        let mut flux = advect(velocity, if velocity >= 0.0 { ghost } else { centre });
        if let Some(context) = context {
            let side = context.config.left.clone();
            flux += context.wall_flux(side.temperature, side.heat_flux, centre, 0.5 * mesh.dx, 1.0);
        }
        flux
    } else if mesh.solid[index - 1] {
        context.map_or(0.0, |context| {
            context.wall_flux(
                context.config.obstacle_temperature,
                context.config.obstacle_heat_flux,
                centre,
                0.5 * mesh.dx,
                1.0,
            )
        })
    } else {
        let velocity = u[mesh.u_index(i, j)];
        let west = temperature[index - 1];
        let face = if velocity >= 0.0 {
            face_value(
                scheme,
                west,
                if i >= 2 && !mesh.solid[index - 2] {
                    Some(temperature[index - 2])
                } else {
                    None
                },
                centre,
            )
        } else {
            face_value(
                scheme,
                centre,
                if i + 1 < mesh.nx && !mesh.solid[index + 1] {
                    Some(temperature[index + 1])
                } else {
                    None
                },
                west,
            )
        };
        advect(velocity, face) - kappa * (centre - west) / mesh.dx
    }
}

#[allow(clippy::too_many_arguments)]
fn scalar_east_flux(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
    kappa: f64,
    scheme: ConvectionScheme,
    u: &[f64],
    temperature: &[f64],
    i: usize,
    j: usize,
    centre: f64,
) -> f64 {
    let index = mesh.pressure_index(i, j);
    if i + 1 == mesh.nx {
        let velocity = u[mesh.u_index(mesh.nx, j)];
        let inlet = boundaries.right.kind == BoundaryKind::Inlet;
        let ghost = side_advect_ghost(Side::Right, centre, inlet, config, context);
        let mut flux = advect(velocity, if velocity >= 0.0 { centre } else { ghost });
        if let Some(context) = context {
            let side = context.config.right.clone();
            flux += context.wall_flux(
                side.temperature,
                side.heat_flux,
                centre,
                0.5 * mesh.dx,
                -1.0,
            );
        }
        flux
    } else if mesh.solid[index + 1] {
        context.map_or(0.0, |context| {
            context.wall_flux(
                context.config.obstacle_temperature,
                context.config.obstacle_heat_flux,
                centre,
                0.5 * mesh.dx,
                -1.0,
            )
        })
    } else {
        let velocity = u[mesh.u_index(i + 1, j)];
        let east = temperature[index + 1];
        let face = if velocity >= 0.0 {
            face_value(
                scheme,
                centre,
                if i >= 1 && !mesh.solid[index - 1] {
                    Some(temperature[index - 1])
                } else {
                    None
                },
                east,
            )
        } else {
            face_value(
                scheme,
                east,
                if i + 2 < mesh.nx && !mesh.solid[index + 2] {
                    Some(temperature[index + 2])
                } else {
                    None
                },
                centre,
            )
        };
        advect(velocity, face) - kappa * (east - centre) / mesh.dx
    }
}

#[allow(clippy::too_many_arguments)]
fn scalar_south_flux(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
    kappa: f64,
    scheme: ConvectionScheme,
    v: &[f64],
    temperature: &[f64],
    i: usize,
    j: usize,
    centre: f64,
) -> f64 {
    let index = mesh.pressure_index(i, j);
    if j == 0 {
        let velocity = v[mesh.v_index(i, 0)];
        let inlet = boundaries.bottom.kind == BoundaryKind::Inlet;
        let ghost = side_advect_ghost(Side::Bottom, centre, inlet, config, context);
        let mut flux = advect(velocity, if velocity >= 0.0 { ghost } else { centre });
        if let Some(context) = context {
            let side = context.config.bottom.clone();
            flux += context.wall_flux(side.temperature, side.heat_flux, centre, 0.5 * mesh.dy, 1.0);
        }
        flux
    } else if mesh.solid[index - mesh.nx] {
        context.map_or(0.0, |context| {
            context.wall_flux(
                context.config.obstacle_temperature,
                context.config.obstacle_heat_flux,
                centre,
                0.5 * mesh.dy,
                1.0,
            )
        })
    } else {
        let velocity = v[mesh.v_index(i, j)];
        let south = temperature[index - mesh.nx];
        let face = if velocity >= 0.0 {
            face_value(
                scheme,
                south,
                if j >= 2 && !mesh.solid[index - 2 * mesh.nx] {
                    Some(temperature[index - 2 * mesh.nx])
                } else {
                    None
                },
                centre,
            )
        } else {
            face_value(
                scheme,
                centre,
                if j + 1 < mesh.ny && !mesh.solid[index + mesh.nx] {
                    Some(temperature[index + mesh.nx])
                } else {
                    None
                },
                south,
            )
        };
        advect(velocity, face) - kappa * (centre - south) / mesh.dy
    }
}

#[allow(clippy::too_many_arguments)]
fn scalar_north_flux(
    mesh: &Mesh,
    boundaries: &Boundaries,
    config: &ScalarConfig,
    context: Option<&ThermalContext>,
    kappa: f64,
    scheme: ConvectionScheme,
    v: &[f64],
    temperature: &[f64],
    i: usize,
    j: usize,
    centre: f64,
) -> f64 {
    let index = mesh.pressure_index(i, j);
    if j + 1 == mesh.ny {
        let velocity = v[mesh.v_index(i, mesh.ny)];
        let inlet = boundaries.top.kind == BoundaryKind::Inlet;
        let ghost = side_advect_ghost(Side::Top, centre, inlet, config, context);
        let mut flux = advect(velocity, if velocity >= 0.0 { centre } else { ghost });
        if let Some(context) = context {
            let side = context.config.top.clone();
            flux += context.wall_flux(
                side.temperature,
                side.heat_flux,
                centre,
                0.5 * mesh.dy,
                -1.0,
            );
        }
        flux
    } else if mesh.solid[index + mesh.nx] {
        context.map_or(0.0, |context| {
            context.wall_flux(
                context.config.obstacle_temperature,
                context.config.obstacle_heat_flux,
                centre,
                0.5 * mesh.dy,
                -1.0,
            )
        })
    } else {
        let velocity = v[mesh.v_index(i, j + 1)];
        let north = temperature[index + mesh.nx];
        let face = if velocity >= 0.0 {
            face_value(
                scheme,
                centre,
                if j >= 1 && !mesh.solid[index - mesh.nx] {
                    Some(temperature[index - mesh.nx])
                } else {
                    None
                },
                north,
            )
        } else {
            face_value(
                scheme,
                north,
                if j + 2 < mesh.ny && !mesh.solid[index + 2 * mesh.nx] {
                    Some(temperature[index + 2 * mesh.nx])
                } else {
                    None
                },
                centre,
            )
        };
        advect(velocity, face) - kappa * (north - centre) / mesh.dy
    }
}

fn advect(velocity: f64, upwind: f64) -> f64 {
    velocity * upwind
}

#[allow(clippy::too_many_arguments)]
fn append_force_samples(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    eddy: Option<&[f64]>,
    step: usize,
    samples: &mut [Vec<ForceSample>],
) {
    let raw = raw_forces(mesh, model, u, v, p, eddy);
    for (index, force) in raw.into_iter().enumerate() {
        samples[index].push(ForceSample {
            step,
            fx: force.fx,
            fy: force.fy,
            mz: force.mz,
            cd: force.cd,
            cl: force.cl,
            cm: force.cm,
        });
    }
}

fn build_forces(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    eddy: Option<&[f64]>,
    samples: &[Vec<ForceSample>],
) -> Vec<ObstacleForce> {
    let raw = raw_forces(mesh, model, u, v, p, eddy);
    model
        .obstacles
        .iter()
        .enumerate()
        .map(|(index, obstacle)| {
            let force = raw.get(index).copied().unwrap_or_default();
            let id = if obstacle.id().is_empty() {
                format!("obstacle-{}", index + 1)
            } else {
                obstacle.id().to_string()
            };
            ObstacleForce {
                id,
                kind: obstacle.kind().to_string(),
                fx: force.fx,
                fy: force.fy,
                mz: force.mz,
                cd: force.cd,
                cl: force.cl,
                cm: force.cm,
                reference_velocity: force.reference_velocity,
                reference_area: force.reference_area,
                reference_length: force.reference_length,
                history: samples.get(index).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

/// Resolves every probe to the nearest pressure cell.
///
/// A probe is rejected with `probe/invalid` when that cell is solid, which
/// keeps the recorded series on the fluid side of every obstacle.
fn locate_probes(mesh: &Mesh, model: &Model) -> Result<Vec<(usize, usize)>, CfdError> {
    model
        .probes
        .iter()
        .map(|probe| {
            let i = ((probe.x / mesh.dx).floor() as isize).clamp(0, mesh.nx as isize - 1) as usize;
            let j = ((probe.y / mesh.dy).floor() as isize).clamp(0, mesh.ny as isize - 1) as usize;
            if mesh.is_solid(i, j) {
                return Err(CfdError::new(
                    "probe/invalid",
                    format!(
                        "probe ({}, {}) lands in a solid cell; move it into the fluid",
                        probe.x, probe.y
                    ),
                ));
            }
            Ok((i, j))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn append_probe_samples(
    mesh: &Mesh,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    temperature: Option<&[f64]>,
    step: usize,
    time: f64,
    cells: &[(usize, usize)],
    samples: &mut [Vec<ProbeSample>],
) {
    for (index, &(i, j)) in cells.iter().enumerate() {
        let cell = mesh.pressure_index(i, j);
        let (u_value, v_value) = mesh.cell_velocity(u, v, i, j);
        samples[index].push(ProbeSample {
            step,
            time,
            u: u_value,
            v: v_value,
            speed: u_value.hypot(v_value),
            pressure: p[cell],
            temperature: temperature.map(|field| field[cell]),
        });
    }
}

fn build_probes(
    model: &Model,
    cells: &[(usize, usize)],
    samples: &[Vec<ProbeSample>],
) -> Vec<ProbeSeries> {
    model
        .probes
        .iter()
        .enumerate()
        .map(|(index, probe)| {
            let id = if probe.id().is_empty() {
                format!("probe-{}", index + 1)
            } else {
                probe.id().to_string()
            };
            let cell = cells.get(index).copied().unwrap_or((0, 0));
            ProbeSeries {
                id,
                x: probe.x,
                y: probe.y,
                cell: [cell.0, cell.1],
                history: samples.get(index).cloned().unwrap_or_default(),
            }
        })
        .collect()
}

/// Length-averaged wall shear and friction coefficient of every wall side.
///
/// The tangential gradient uses the half-cell distance from the wall to the
/// adjacent cell centre, so a linear tangential profile is reproduced exactly.
/// Solid cells along the side are skipped, and a side without fluid-adjacent
/// boundary cells is omitted. Only the molecular viscosity contributes; eddy
/// viscosity vanishes at a wall by construction. With a `wallFunction` section
/// the shear of cells above the viscous-sublayer crossover follows the log law
/// instead, and every side reports the mean `y⁺`.
fn wall_shear(mesh: &Mesh, model: &Model, u: &[f64], v: &[f64]) -> Vec<WallShear> {
    let speed = reference_speed(model);
    let scale = 0.5 * model.fluid.density * speed * speed;
    let density = model.fluid.density;
    let mu = model.fluid.viscosity;
    let wall_function = model.wall_function.as_ref();
    let mut report = Vec::new();
    for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
        let boundary = model.boundaries.get(side);
        if boundary.kind != BoundaryKind::Wall {
            continue;
        }
        let mut sum = 0.0;
        let mut peak = 0.0_f64;
        let mut y_plus_sum = 0.0;
        let mut count = 0_usize;
        match side {
            Side::Left | Side::Right => {
                let column = if side == Side::Left { 0 } else { mesh.nx - 1 };
                for j in 0..mesh.ny {
                    if mesh.is_solid(column, j) {
                        continue;
                    }
                    let centre =
                        0.5 * (v[mesh.v_index(column, j)] + v[mesh.v_index(column, j + 1)]);
                    let (shear, y_plus) = wall_shear_at(
                        boundary.velocity[1] - centre,
                        0.5 * mesh.dx,
                        density,
                        mu,
                        wall_function,
                    );
                    sum += shear;
                    peak = peak.max(shear.abs());
                    y_plus_sum += y_plus;
                    count += 1;
                }
            }
            Side::Top | Side::Bottom => {
                let row = if side == Side::Top { mesh.ny - 1 } else { 0 };
                for i in 0..mesh.nx {
                    if mesh.is_solid(i, row) {
                        continue;
                    }
                    let centre = 0.5 * (u[mesh.u_index(i, row)] + u[mesh.u_index(i + 1, row)]);
                    let (shear, y_plus) = wall_shear_at(
                        boundary.velocity[0] - centre,
                        0.5 * mesh.dy,
                        density,
                        mu,
                        wall_function,
                    );
                    sum += shear;
                    peak = peak.max(shear.abs());
                    y_plus_sum += y_plus;
                    count += 1;
                }
            }
        }
        if count == 0 {
            continue;
        }
        let mean = sum / count as f64;
        report.push(WallShear {
            side: side.as_str().to_string(),
            mean_shear: mean,
            max_shear: peak,
            mean_y_plus: y_plus_sum / count as f64,
            friction_coefficient: if scale > 0.0 { mean / scale } else { 0.0 },
        });
    }
    report
}

/// Signed wall shear stress and `y⁺` of one wall-adjacent cell.
///
/// Without a wall function the result is the laminar `ρ·ν·Δu/y`. With one, a
/// cell whose viscous estimate lies above `minYPlus` uses the log-law friction
/// velocity instead, and `τ_w = ρ·u_τ²`.
fn wall_shear_at(
    delta: f64,
    distance: f64,
    density: f64,
    viscosity: f64,
    config: Option<&WallFunction>,
) -> (f64, f64) {
    if distance <= 0.0 || viscosity <= 0.0 {
        return (0.0, 0.0);
    }
    let laminar = density * viscosity * delta.abs() / distance;
    let u_tau = (laminar / density).max(0.0).sqrt();
    let y_plus = u_tau * distance / viscosity;
    let Some(config) = config else {
        return (laminar.copysign(delta), y_plus);
    };
    if y_plus <= config.min_y_plus {
        return (laminar.copysign(delta), y_plus);
    }
    let u_tau = log_law_friction_velocity(delta.abs(), distance, viscosity, config);
    (
        (density * u_tau * u_tau).copysign(delta),
        u_tau * distance / viscosity,
    )
}

/// Friction velocity that satisfies `u = u_τ·((1/κ)·ln(u_τ·y/ν) + B)`.
///
/// Bisection keeps the solve deterministic and bounded; the residual increases
/// monotonically with `u_τ` above the log-law zero crossing.
fn log_law_friction_velocity(
    velocity: f64,
    distance: f64,
    viscosity: f64,
    config: &WallFunction,
) -> f64 {
    let residual = |u_tau: f64| {
        u_tau * ((u_tau * distance / viscosity).ln() / config.kappa + config.intercept) - velocity
    };
    let mut low = viscosity / distance * (-config.kappa * config.intercept).exp();
    low *= 1.0 + 1.0e-9;
    let mut high = velocity.max(low * 2.0);
    for _ in 0..200 {
        if residual(high) >= 0.0 {
            break;
        }
        high *= 2.0;
    }
    for _ in 0..100 {
        let mid = 0.5 * (low + high);
        if residual(mid) < 0.0 {
            low = mid;
        } else {
            high = mid;
        }
    }
    0.5 * (low + high)
}

/// Pressure and viscous force plus moment of every obstacle, in document order.
///
/// `mz` is taken about the obstacle centroid with counter-clockwise positive,
/// and the coefficients use the document reference quantities when present.
fn raw_forces(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    eddy: Option<&[f64]>,
) -> Vec<ForceValues> {
    let count = model.obstacles.len();
    let mut fx = vec![0.0; count];
    let mut fy = vec![0.0; count];
    let mut mz = vec![0.0; count];
    let pivots: Vec<(f64, f64)> = model
        .obstacles
        .iter()
        .map(|obstacle| obstacle.centroid())
        .collect();
    let mu = model.fluid.viscosity;

    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let left_solid = i > 0 && mesh.is_solid(i - 1, j);
            let right_solid = i < mesh.nx && mesh.is_solid(i, j);
            if left_solid == right_solid {
                continue;
            }
            let (fluid_i, solid_cell) = if right_solid {
                if i == 0 {
                    continue;
                }
                (i - 1, mesh.pressure_index(i, j))
            } else {
                if i == mesh.nx {
                    continue;
                }
                (i, mesh.pressure_index(i - 1, j))
            };
            let Some(owner) = obstacle_owner(mesh, solid_cell) else {
                continue;
            };
            let sign = if right_solid { 1.0 } else { -1.0 };
            let pressure = p[mesh.pressure_index(fluid_i, j)];
            let (uc, _) = mesh.cell_velocity(u, v, fluid_i, j);
            let eddy_face = eddy.map_or(0.0, |field| field[mesh.pressure_index(fluid_i, j)]);
            let shear = (mu + eddy_face) * uc / (0.5 * mesh.dx);
            let force_x = sign * (pressure + shear) * mesh.dy;
            fx[owner] += force_x;
            let (_, face_y) = mesh.u_position(i, j);
            mz[owner] -= (face_y - pivots[owner].1) * force_x;
        }
    }

    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            let below_solid = j > 0 && mesh.is_solid(i, j - 1);
            let above_solid = j < mesh.ny && mesh.is_solid(i, j);
            if below_solid == above_solid {
                continue;
            }
            let (fluid_j, solid_cell) = if above_solid {
                if j == 0 {
                    continue;
                }
                (j - 1, mesh.pressure_index(i, j))
            } else {
                if j == mesh.ny {
                    continue;
                }
                (j, mesh.pressure_index(i, j - 1))
            };
            let Some(owner) = obstacle_owner(mesh, solid_cell) else {
                continue;
            };
            let sign = if above_solid { 1.0 } else { -1.0 };
            let pressure = p[mesh.pressure_index(i, fluid_j)];
            let (_, vc) = mesh.cell_velocity(u, v, i, fluid_j);
            let eddy_face = eddy.map_or(0.0, |field| field[mesh.pressure_index(i, fluid_j)]);
            let shear = (mu + eddy_face) * vc / (0.5 * mesh.dy);
            let force_y = sign * (pressure + shear) * mesh.dx;
            fy[owner] += force_y;
            let (face_x, _) = mesh.v_position(i, j);
            mz[owner] += (face_x - pivots[owner].0) * force_y;
        }
    }

    let default_speed = reference_speed(model);
    model
        .obstacles
        .iter()
        .enumerate()
        .map(|(index, obstacle)| {
            let length = obstacle.characteristic_length();
            let speed = model
                .reference
                .as_ref()
                .and_then(|reference| reference.velocity)
                .unwrap_or(default_speed);
            let area = model
                .reference
                .as_ref()
                .and_then(|reference| reference.area)
                .unwrap_or(length);
            let reference_length = model
                .reference
                .as_ref()
                .and_then(|reference| reference.length)
                .unwrap_or(length);
            let force_scale = 2.0 / (model.fluid.density * speed * speed * area);
            let moment_scale =
                2.0 / (model.fluid.density * speed * speed * area * reference_length);
            ForceValues {
                fx: fx[index],
                fy: fy[index],
                mz: mz[index],
                cd: fx[index] * force_scale,
                cl: fy[index] * force_scale,
                cm: mz[index] * moment_scale,
                reference_velocity: speed,
                reference_area: area,
                reference_length,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, Default)]
struct ForceValues {
    fx: f64,
    fy: f64,
    mz: f64,
    cd: f64,
    cl: f64,
    cm: f64,
    reference_velocity: f64,
    reference_area: f64,
    reference_length: f64,
}

fn compute_nusselt(
    mesh: &Mesh,
    model: &Model,
    _u: &[f64],
    _v: &[f64],
    temperature: Option<&[f64]>,
) -> Vec<ObstacleNusselt> {
    let (Some(thermal), Some(field), Some(scalar)) =
        (&model.thermal, temperature, model.scalar.as_ref())
    else {
        return Vec::new();
    };
    let cp = thermal.specific_heat;
    let conductivity = thermal
        .conductivity
        .filter(|value| *value > 0.0)
        .unwrap_or(model.fluid.density * cp * scalar.diffusivity);
    if !conductivity.is_finite() || conductivity <= 0.0 || !cp.is_finite() || cp <= 0.0 {
        return Vec::new();
    }

    let mut fluid_sum = 0.0;
    let mut fluid_count = 0;
    for (index, value) in field.iter().enumerate() {
        if mesh.solid[index] {
            continue;
        }
        fluid_sum += *value;
        fluid_count += 1;
    }
    let mean_fluid = if fluid_count > 0 {
        fluid_sum / fluid_count as f64
    } else {
        0.0
    };
    let reference = thermal.reference_temperature.unwrap_or(mean_fluid);

    let count = model.obstacles.len();
    let mut flux_total = vec![0.0; count];
    let mut wall_total = vec![0.0; count];
    let mut area_total = vec![0.0; count];

    let mut record = |owner: usize, fluid: f64, distance: f64, area: f64| {
        let (flux, wall) = if let Some(wall) = thermal.obstacle_temperature {
            (conductivity * (wall - fluid) / distance, wall)
        } else if let Some(heat_flux) = thermal.obstacle_heat_flux {
            (heat_flux, fluid + heat_flux * distance / conductivity)
        } else {
            (0.0, fluid)
        };
        if owner < count {
            flux_total[owner] += flux * area;
            wall_total[owner] += wall * area;
            area_total[owner] += area;
        }
    };

    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let left_solid = i > 0 && mesh.is_solid(i - 1, j);
            let right_solid = i < mesh.nx && mesh.is_solid(i, j);
            if left_solid == right_solid {
                continue;
            }
            let (fluid_i, solid_cell) = if right_solid {
                if i == 0 {
                    continue;
                }
                (i - 1, mesh.pressure_index(i, j))
            } else {
                if i == mesh.nx {
                    continue;
                }
                (i, mesh.pressure_index(i - 1, j))
            };
            let Some(owner) = obstacle_owner(mesh, solid_cell) else {
                continue;
            };
            let fluid = field[mesh.pressure_index(fluid_i, j)];
            record(owner, fluid, 0.5 * mesh.dx, mesh.dy);
        }
    }

    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            let below_solid = j > 0 && mesh.is_solid(i, j - 1);
            let above_solid = j < mesh.ny && mesh.is_solid(i, j);
            if below_solid == above_solid {
                continue;
            }
            let (fluid_j, solid_cell) = if above_solid {
                if j == 0 {
                    continue;
                }
                (j - 1, mesh.pressure_index(i, j))
            } else {
                if j == mesh.ny {
                    continue;
                }
                (j, mesh.pressure_index(i, j - 1))
            };
            let Some(owner) = obstacle_owner(mesh, solid_cell) else {
                continue;
            };
            let fluid = field[mesh.pressure_index(i, fluid_j)];
            record(owner, fluid, 0.5 * mesh.dy, mesh.dx);
        }
    }

    model
        .obstacles
        .iter()
        .enumerate()
        .map(|(index, obstacle)| {
            let area = area_total[index];
            let mean_heat_flux = if area > 0.0 {
                flux_total[index] / area
            } else {
                0.0
            };
            let wall_temperature = if area > 0.0 {
                wall_total[index] / area
            } else {
                reference
            };
            let delta = wall_temperature - reference;
            let nusselt = if delta.abs() > 1e-12 && conductivity > 0.0 {
                mean_heat_flux * obstacle.characteristic_length() / (conductivity * delta)
            } else {
                0.0
            };
            let id = if obstacle.id().is_empty() {
                format!("obstacle-{}", index + 1)
            } else {
                obstacle.id().to_string()
            };
            ObstacleNusselt {
                id,
                kind: obstacle.kind().to_string(),
                mean_heat_flux,
                wall_temperature,
                reference_temperature: reference,
                nusselt,
            }
        })
        .collect()
}

fn obstacle_owner(mesh: &Mesh, cell: usize) -> Option<usize> {
    let id = *mesh.solid_id.get(cell)?;
    usize::try_from(id).ok()
}

fn reference_speed(model: &Model) -> f64 {
    let inlet = [
        &model.boundaries.left,
        &model.boundaries.right,
        &model.boundaries.top,
        &model.boundaries.bottom,
    ]
    .iter()
    .filter(|boundary| boundary.kind == BoundaryKind::Inlet)
    .map(|boundary| boundary.velocity[0].hypot(boundary.velocity[1]))
    .fold(0.0_f64, f64::max);
    if inlet > 0.0 {
        inlet
    } else {
        1.0
    }
}

fn pressure_range(mesh: &Mesh, pressure: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for (index, value) in pressure.iter().enumerate() {
        if mesh.solid[index] {
            continue;
        }
        min = min.min(*value);
        max = max.max(*value);
    }
    if min.is_finite() && max.is_finite() {
        (min, max)
    } else {
        (0.0, 0.0)
    }
}

fn velocity_residual(u_previous: &[f64], v_previous: &[f64], u: &[f64], v: &[f64]) -> f64 {
    let mut sum = 0.0;
    for (new, old) in u.iter().zip(u_previous.iter()) {
        let delta = new - old;
        sum += delta * delta;
    }
    for (new, old) in v.iter().zip(v_previous.iter()) {
        let delta = new - old;
        sum += delta * delta;
    }
    sum.sqrt()
}

fn max_velocity(u: &[f64], v: &[f64]) -> f64 {
    u.iter()
        .chain(v.iter())
        .fold(0.0_f64, |maximum, value| maximum.max(value.abs()))
}

/// Integrates the mass flux through the four domain sides.
///
/// Solid obstacles are interior and contribute no boundary flux. Periodic sides
/// carry no net exchange with the outside, so they are skipped. The outward
/// normal is `+x` on the right, `-x` on the left, `+y` on the top and `-y` on
/// the bottom, so the signs below match `outflow - inflow`.
fn mass_balance(
    mesh: &Mesh,
    boundaries: &Boundaries,
    density: f64,
    u: &[f64],
    v: &[f64],
) -> MassBalance {
    let mut inflow = 0.0;
    let mut outflow = 0.0;
    let mut accumulate = |flux: f64| {
        if flux >= 0.0 {
            outflow += flux;
        } else {
            inflow -= flux;
        }
    };
    if !boundaries.is_periodic(Side::Left) {
        for j in 0..mesh.ny {
            accumulate(-u[mesh.u_index(0, j)] * mesh.dy);
        }
    }
    if !boundaries.is_periodic(Side::Right) {
        for j in 0..mesh.ny {
            accumulate(u[mesh.u_index(mesh.nx, j)] * mesh.dy);
        }
    }
    if !boundaries.is_periodic(Side::Bottom) {
        for i in 0..mesh.nx {
            accumulate(-v[mesh.v_index(i, 0)] * mesh.dx);
        }
    }
    if !boundaries.is_periodic(Side::Top) {
        for i in 0..mesh.nx {
            accumulate(v[mesh.v_index(i, mesh.ny)] * mesh.dx);
        }
    }
    let inflow = inflow * density;
    let outflow = outflow * density;
    let net = outflow - inflow;
    let scale = inflow.max(outflow);
    MassBalance {
        inflow,
        outflow,
        net,
        relative_error: if scale > 0.0 { net.abs() / scale } else { 0.0 },
    }
}

/// Integrates the outward mass flux through every outlet side.
///
/// The outward normal is `+x` on the right, `-x` on the left, `+y` on the top
/// and `-y` on the bottom. Both `outlet` and `convective` sides are included,
/// so a domain with several outlets reports how the total discharge splits.
/// Periodic sides are skipped because they exchange no mass with the outside.
/// Sides are listed in `left`, `right`, `top`, `bottom` order; `share` is
/// `flow / Σ max(flow, 0)` and is zero when no outlet discharges.
pub fn outlet_flows(
    mesh: &Mesh,
    boundaries: &Boundaries,
    density: f64,
    u: &[f64],
    v: &[f64],
) -> Vec<OutletFlow> {
    let mut outlets = Vec::new();
    for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
        if boundaries.is_periodic(side) || !boundaries.get(side).kind.is_outlet() {
            continue;
        }
        let (length, flux) = match side {
            Side::Left => (
                mesh.length_y,
                -(0..mesh.ny)
                    .map(|j| u[mesh.u_index(0, j)] * mesh.dy)
                    .sum::<f64>(),
            ),
            Side::Right => (
                mesh.length_y,
                (0..mesh.ny)
                    .map(|j| u[mesh.u_index(mesh.nx, j)] * mesh.dy)
                    .sum::<f64>(),
            ),
            Side::Bottom => (
                mesh.length_x,
                -(0..mesh.nx)
                    .map(|i| v[mesh.v_index(i, 0)] * mesh.dx)
                    .sum::<f64>(),
            ),
            Side::Top => (
                mesh.length_x,
                (0..mesh.nx)
                    .map(|i| v[mesh.v_index(i, mesh.ny)] * mesh.dx)
                    .sum::<f64>(),
            ),
        };
        let flow = flux * density;
        outlets.push(OutletFlow {
            side: side.as_str().to_string(),
            flow,
            share: 0.0,
            mean_velocity: if length > 0.0 { flux / length } else { 0.0 },
            length,
        });
    }
    let total: f64 = outlets.iter().map(|outlet| outlet.flow.max(0.0)).sum();
    for outlet in &mut outlets {
        outlet.share = if total > 0.0 {
            outlet.flow / total
        } else {
            0.0
        };
    }
    outlets
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Boundary, MeshConfig, Probe};

    fn mesh() -> Mesh {
        Mesh::new(&MeshConfig {
            nx: 10,
            ny: 6,
            length_x: 1.0,
            length_y: 1.0,
        })
        .expect("mesh")
    }

    #[test]
    fn buoyancy_acceleration_points_away_from_gravity_when_hot() {
        let mesh = mesh();
        let buoyancy = Buoyancy {
            beta: 0.1,
            t_ref: 300.0,
            gravity: [0.0, -9.81],
        };
        let mut temperature = vec![300.0; mesh.pressure_count()];
        temperature[mesh.pressure_index(0, 0)] = 310.0;
        temperature[mesh.pressure_index(1, 0)] = 310.0;
        let vertical = face_buoyancy(
            &mesh,
            Some(&buoyancy),
            Some(&temperature),
            (0, 0),
            (1, 0),
            1,
        );
        assert!((vertical - 9.81).abs() < 1e-12);
        assert_eq!(
            face_buoyancy(
                &mesh,
                Some(&buoyancy),
                Some(&temperature),
                (0, 0),
                (1, 0),
                0,
            ),
            0.0
        );

        temperature[mesh.pressure_index(0, 0)] = 290.0;
        temperature[mesh.pressure_index(1, 0)] = 290.0;
        let cold = face_buoyancy(
            &mesh,
            Some(&buoyancy),
            Some(&temperature),
            (0, 0),
            (1, 0),
            1,
        );
        assert!((cold + 9.81).abs() < 1e-12);
        assert_eq!(
            face_buoyancy(&mesh, None, Some(&temperature), (0, 0), (1, 0), 1),
            0.0
        );
    }

    #[test]
    fn quick_and_central_are_exact_for_quadratic_profiles() {
        let mesh = mesh();
        let mut u = vec![0.0; mesh.u_count()];
        for j in 0..mesh.ny {
            for i in 0..=mesh.nx {
                let (x, _) = mesh.u_position(i, j);
                u[mesh.u_index(i, j)] = x * x;
            }
        }
        let i = 4;
        let j = 2;
        let exact = 2.0 * i as f64 * mesh.dx;
        let upwind = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Upwind, false);
        let central =
            u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Central, false);
        let quick = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Quick, false);
        let quick_reverse =
            u_convective_derivative(&mesh, &u, i, j, -1.0, ConvectionScheme::Quick, false);
        assert!((central - exact).abs() < 1e-12, "central {central}");
        assert!((quick - exact).abs() < 1e-12, "QUICK {quick}");
        assert!(
            (quick_reverse - exact).abs() < 1e-12,
            "reverse QUICK {quick_reverse}"
        );
        assert!(
            (upwind - exact).abs() > 0.5 * mesh.dx,
            "upwind {upwind} should be first-order diffusive"
        );

        let mut v = vec![0.0; mesh.v_count()];
        for j in 0..=mesh.ny {
            for i in 0..mesh.nx {
                let (_, y) = mesh.v_position(i, j);
                v[mesh.v_index(i, j)] = y * y;
            }
        }
        let j = 3;
        let exact = 2.0 * j as f64 * mesh.dy;
        let central =
            v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Central, false);
        let quick = v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Quick, false);
        let quick_reverse =
            v_convective_derivative(&mesh, &v, 2, j, -1.0, ConvectionScheme::Quick, false);
        assert!((central - exact).abs() < 1e-12, "central {central}");
        assert!((quick - exact).abs() < 1e-12, "QUICK {quick}");
        assert!(
            (quick_reverse - exact).abs() < 1e-12,
            "reverse QUICK {quick_reverse}"
        );
    }

    #[test]
    fn quick_falls_back_to_upwind_next_to_the_boundary() {
        let mesh = mesh();
        let mut u = vec![0.0; mesh.u_count()];
        for j in 0..mesh.ny {
            for i in 0..=mesh.nx {
                let (x, _) = mesh.u_position(i, j);
                u[mesh.u_index(i, j)] = x * x;
            }
        }
        let upwind = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Upwind, false);
        let quick = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Quick, false);
        assert_eq!(upwind, quick);
    }

    #[test]
    fn probe_samples_the_nearest_cell() {
        let mesh = mesh();
        let mut model = Model::new();
        model.mesh.nx = 10;
        model.mesh.ny = 6;
        model.time.dt = 0.5;
        model.probes = vec![Probe {
            id: "p".to_string(),
            x: 0.05,
            y: 0.95,
        }];
        let cells = locate_probes(&mesh, &model).expect("locate probe");
        assert_eq!(cells, vec![(0, 5)]);

        let mut u = vec![0.0; mesh.u_count()];
        let mut v = vec![0.0; mesh.v_count()];
        let mut p = vec![0.0; mesh.pressure_count()];
        u[mesh.u_index(0, 5)] = 0.2;
        u[mesh.u_index(1, 5)] = 0.4;
        v[mesh.v_index(0, 5)] = 0.1;
        v[mesh.v_index(0, 6)] = 0.3;
        p[mesh.pressure_index(0, 5)] = 12.5;
        let temperature = vec![300.0; mesh.pressure_count()];

        let mut samples = vec![Vec::new()];
        append_probe_samples(
            &mesh,
            &u,
            &v,
            &p,
            Some(temperature.as_slice()),
            3,
            1.5,
            &cells,
            &mut samples,
        );
        let sample = &samples[0][0];
        assert_eq!(sample.step, 3);
        assert!((sample.time - 1.5).abs() < 1e-12);
        assert!((sample.u - 0.3).abs() < 1e-12, "u = {}", sample.u);
        assert!((sample.v - 0.2).abs() < 1e-12, "v = {}", sample.v);
        assert!(
            (sample.speed - 0.13_f64.sqrt()).abs() < 1e-12,
            "speed = {}",
            sample.speed
        );
        assert_eq!(sample.pressure, 12.5);
        assert_eq!(sample.temperature, Some(300.0));
    }

    #[test]
    fn probe_inside_a_solid_cell_is_rejected() {
        let mut mesh = mesh();
        mesh.apply_obstacles(&[crate::model::Obstacle::Rect {
            id: "block".to_string(),
            x0: 0.0,
            y0: 0.9,
            x1: 0.2,
            y1: 1.0,
        }])
        .expect("obstacles");
        let mut model = Model::new();
        model.mesh.nx = 10;
        model.mesh.ny = 6;
        model.probes = vec![Probe {
            id: "blocked".to_string(),
            x: 0.05,
            y: 0.95,
        }];
        let error = locate_probes(&mesh, &model).expect_err("solid probe");
        assert_eq!(error.code, "probe/invalid");
    }

    #[test]
    fn wall_shear_reproduces_a_linear_tangential_profile() {
        let mesh = mesh();
        let mut model = Model::new();
        model.mesh.nx = 10;
        model.mesh.ny = 6;
        model.fluid.density = 1.0;
        model.fluid.viscosity = 0.02;
        model.boundaries.top = Boundary {
            kind: BoundaryKind::Wall,
            velocity: [1.0, 0.0],
            pressure: 0.0,
        };
        model.boundaries.bottom = Boundary {
            kind: BoundaryKind::Wall,
            velocity: [0.0, 0.0],
            pressure: 0.0,
        };
        model.boundaries.left = Boundary {
            kind: BoundaryKind::Wall,
            velocity: [0.0, 0.0],
            pressure: 0.0,
        };
        model.boundaries.right = Boundary {
            kind: BoundaryKind::Inlet,
            velocity: [0.0, 0.0],
            pressure: 0.0,
        };

        // Tangential profiles u = y and v = x have unit slope, so the wall
        // shear is exactly ±mu with the half-cell gradient.
        let mut u = vec![0.0; mesh.u_count()];
        for j in 0..mesh.ny {
            for i in 0..=mesh.nx {
                let (_, y) = mesh.u_position(i, j);
                u[mesh.u_index(i, j)] = y;
            }
        }
        let mut v = vec![0.0; mesh.v_count()];
        for j in 0..=mesh.ny {
            for i in 0..mesh.nx {
                let (x, _) = mesh.v_position(i, j);
                v[mesh.v_index(i, j)] = x;
            }
        }

        let shear = wall_shear(&mesh, &model, &u, &v);
        assert_eq!(shear.len(), 3, "the inlet side must be skipped");
        let mu = model.fluid.viscosity;
        let find = |side: &str| {
            shear
                .iter()
                .find(|item| item.side == side)
                .unwrap_or_else(|| panic!("missing {side} wall shear"))
        };
        let top = find("top");
        assert!((top.mean_shear - mu).abs() < 1e-12, "top {top:?}");
        assert!((top.max_shear - mu).abs() < 1e-12);
        assert!((top.friction_coefficient - 2.0 * mu).abs() < 1e-12);
        let bottom = find("bottom");
        assert!((bottom.mean_shear + mu).abs() < 1e-12, "bottom {bottom:?}");
        let left = find("left");
        assert!((left.mean_shear + mu).abs() < 1e-12, "left {left:?}");
    }

    #[test]
    fn wall_function_solves_the_log_law() {
        let config = WallFunction::default();
        let (viscosity, density) = (1.0e-5, 1.0);
        let (distance, velocity) = (0.01, 10.0);
        let (shear, y_plus) = wall_shear_at(velocity, distance, density, viscosity, Some(&config));
        assert!(y_plus > config.min_y_plus, "y+ = {y_plus}");
        let u_tau = (shear / density).sqrt();
        let log_law =
            u_tau * ((u_tau * distance / viscosity).ln() / config.kappa + config.intercept);
        assert!(
            (log_law - velocity).abs() < 1e-8,
            "log law {log_law} vs velocity {velocity}"
        );
        let laminar = density * viscosity * velocity / distance;
        assert!(
            shear > 5.0 * laminar,
            "wall-function shear {shear} should exceed the laminar {laminar}"
        );
        let (reversed, _) = wall_shear_at(-velocity, distance, density, viscosity, Some(&config));
        assert_eq!(reversed, -shear);
    }

    #[test]
    fn wall_function_keeps_the_viscous_sublayer_below_the_crossover() {
        let config = WallFunction::default();
        let (viscosity, density) = (0.01, 1.0);
        let (distance, velocity) = (0.03, 1.0);
        let (shear, y_plus) = wall_shear_at(velocity, distance, density, viscosity, Some(&config));
        assert!(y_plus <= config.min_y_plus, "y+ = {y_plus}");
        let laminar = density * viscosity * velocity / distance;
        assert!((shear - laminar).abs() < 1e-12);
        let (plain, plain_y_plus) = wall_shear_at(velocity, distance, density, viscosity, None);
        assert_eq!(plain, shear);
        assert_eq!(plain_y_plus, y_plus);
    }
}
