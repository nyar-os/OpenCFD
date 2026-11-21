use boost_check::{CheckReport, Issue};
use serde::{Deserialize, Serialize};

use crate::fields::{stream_function, vorticity};
use crate::mesh::Mesh;
use crate::model::{
    Boundaries, BoundaryKind, ConvectionScheme, Model, ScalarConfig, Side, ThermalConfig,
    Turbulence, TurbulenceModel,
};
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
    /// Number of time steps that were actually advanced.
    pub steps: usize,
    /// Time step `dt` (s).
    pub dt: f64,
    /// Simulated physical time `steps * dt` (s).
    pub time: f64,
    /// Maximum velocity magnitude on all faces (m/s).
    pub max_velocity: f64,
    /// Minimum cell-centred pressure of fluid cells (Pa).
    pub min_pressure: f64,
    /// Maximum cell-centred pressure of fluid cells (Pa).
    pub max_pressure: f64,
    /// RMS of the discrete divergence after the final projection (1/s).
    pub divergence_l2: f64,
    /// Net mass flow through the domain boundary.
    #[serde(default)]
    pub mass_balance: MassBalance,
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
    model.validate()?;
    options.validate()?;
    let mut mesh = Mesh::new(&model.mesh)?;
    mesh.apply_obstacles(&model.obstacles)?;
    let steps = options.steps.unwrap_or(model.time.steps);

    let mut u = vec![0.0; mesh.u_count()];
    let mut v = vec![0.0; mesh.v_count()];
    let mut p = vec![0.0; mesh.pressure_count()];

    if let Some(initial) = &model.initial {
        u.fill(initial.velocity[0]);
        v.fill(initial.velocity[1]);
    }
    mesh.apply_velocity_boundaries(&model.boundaries, &mut u, &mut v);

    let mut temperature = model
        .scalar
        .as_ref()
        .map(|config| initial_temperature(&mesh, config));

    let settings = PressureSettings {
        iterations: model.time.pressure_iterations,
        tolerance: model.time.tolerance,
    };
    let mut eddy = eddy_viscosity_field(&mesh, model, &u, &v);
    check_explicit_stability(&mesh, model, &u, &v, eddy.as_deref())?;

    let mut u_star = u.clone();
    let mut v_star = v.clone();
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
    let mut completed = steps;
    let mut residual = 0.0;
    let mut steady_state = false;
    let mut residual_history = Vec::new();
    let mut force_samples: Vec<Vec<ForceSample>> = vec![Vec::new(); model.obstacles.len()];
    let probe_cells = locate_probes(&mesh, model)?;
    let mut probe_samples: Vec<Vec<ProbeSample>> = vec![Vec::new(); model.probes.len()];

    for step in 1..=steps {
        u_previous.copy_from_slice(&u);
        v_previous.copy_from_slice(&v);
        eddy = eddy_viscosity_field(&mesh, model, &u, &v);

        momentum_step(
            &mesh,
            model,
            &u,
            &v,
            &p,
            eddy.as_deref(),
            &mut u_star,
            &mut v_star,
        );
        mesh.apply_velocity_boundaries(&model.boundaries, &mut u_star, &mut v_star);

        let divergence = mesh.divergence(&u_star, &v_star);
        last_stats = solve_pressure(
            &mesh,
            &model.boundaries,
            &mut p,
            &divergence,
            model.fluid.density,
            model.time.dt,
            &settings,
        )?;

        u.copy_from_slice(&u_star);
        v.copy_from_slice(&v_star);
        project_velocity(
            &mesh,
            model.fluid.density,
            model.time.dt,
            &p,
            &mut u,
            &mut v,
        );
        mesh.apply_velocity_boundaries(&model.boundaries, &mut u, &mut v);

        if let (Some(config), Some(field)) = (model.scalar.as_ref(), temperature.as_mut()) {
            transport_thermal(
                &mesh,
                &model.boundaries,
                config,
                model.thermal.as_ref(),
                model.fluid.density,
                model.convection,
                model.time.dt,
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

        residual = velocity_residual(&u_previous, &v_previous, &u, &v);
        let sample = step % sample_stride == 0 || step == steps;
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
                model,
                &u,
                &v,
                &p,
                temperature.as_deref(),
                step,
                &probe_cells,
                &mut probe_samples,
            );
        }

        if let Some(tolerance) = options.tolerance {
            if step % check_every == 0 && residual <= tolerance {
                steady_state = true;
                completed = step;
                break;
            }
        }
    }

    let divergence = mesh.divergence(&u, &v);
    let divergence_l2 = rms(&divergence);
    let mass_balance = mass_balance(&mesh, model.fluid.density, &u, &v);
    let max_velocity = max_velocity(&u, &v);
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

    let report = Report {
        format: REPORT_FORMAT.to_string(),
        version: REPORT_VERSION,
        steps: completed,
        dt: model.time.dt,
        time: completed as f64 * model.time.dt,
        max_velocity,
        min_pressure,
        max_pressure,
        divergence_l2,
        mass_balance,
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
        turbulence_model: model
            .turbulence
            .as_ref()
            .map_or("none", |turbulence| turbulence.model.as_str())
            .to_string(),
        eddy_viscosity_min: eddy_min,
        eddy_viscosity_max: eddy_max,
        eddy_viscosity_mean: eddy_mean,
        nusselt,
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
                    if boundaries.left.kind == BoundaryKind::Outlet {
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
                    if boundaries.right.kind == BoundaryKind::Outlet {
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
                    if boundaries.bottom.kind == BoundaryKind::Outlet {
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
                    if boundaries.top.kind == BoundaryKind::Outlet {
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
    boundaries.left.kind == BoundaryKind::Outlet
        || boundaries.right.kind == BoundaryKind::Outlet
        || boundaries.top.kind == BoundaryKind::Outlet
        || boundaries.bottom.kind == BoundaryKind::Outlet
}

fn pressure_residual(
    mesh: &Mesh,
    boundaries: &Boundaries,
    pressure: &[f64],
    divergence: &[f64],
    rhs_scale: f64,
) -> f64 {
    let mut maximum: f64 = 0.0;
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            if mesh.solid[index] {
                continue;
            }
            let centre = pressure[index];
            let west = if i == 0 {
                if boundaries.left.kind == BoundaryKind::Outlet {
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
                if boundaries.right.kind == BoundaryKind::Outlet {
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
                if boundaries.bottom.kind == BoundaryKind::Outlet {
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
                if boundaries.top.kind == BoundaryKind::Outlet {
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

#[allow(clippy::too_many_arguments)]
fn momentum_step(
    mesh: &Mesh,
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    eddy: Option<&[f64]>,
    u_star: &mut [f64],
    v_star: &mut [f64],
) {
    let nu = model.fluid.viscosity;
    let rho = model.fluid.density;
    let dt = model.time.dt;
    let dx = mesh.dx;
    let dy = mesh.dy;
    let boundaries = &model.boundaries;

    for j in 0..mesh.ny {
        for i in 1..mesh.nx {
            let index = mesh.u_index(i, j);
            if !mesh.u_face_open(i, j) {
                u_star[index] = 0.0;
                continue;
            }
            let centre = u[index];
            let du_dx = u_convective_derivative(mesh, u, i, j, centre, model.convection);
            let v_face = 0.25
                * (v[mesh.v_index(i - 1, j)]
                    + v[mesh.v_index(i - 1, j + 1)]
                    + v[mesh.v_index(i, j)]
                    + v[mesh.v_index(i, j + 1)]);
            let dv_dy = 0.5
                * ((v[mesh.v_index(i - 1, j + 1)] - v[mesh.v_index(i - 1, j)]) / dy
                    + (v[mesh.v_index(i, j + 1)] - v[mesh.v_index(i, j)]) / dy);
            let convection = centre * du_dx + v_face * dv_dy;

            let below = if j == 0 {
                match boundaries.bottom.kind {
                    BoundaryKind::Outlet => u[mesh.u_index(i, 1)],
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.bottom.velocity[0] - centre
                    }
                }
            } else {
                u[mesh.u_index(i, j - 1)]
            };
            let above = if j + 1 == mesh.ny {
                match boundaries.top.kind {
                    BoundaryKind::Outlet => u[mesh.u_index(i, mesh.ny - 2)],
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.top.velocity[0] - centre
                    }
                }
            } else {
                u[mesh.u_index(i, j + 1)]
            };
            let diffusion = (u[mesh.u_index(i + 1, j)] - 2.0 * centre + u[mesh.u_index(i - 1, j)])
                / (dx * dx)
                + (above - 2.0 * centre + below) / (dy * dy);
            let pressure_gradient =
                (p[mesh.pressure_index(i, j)] - p[mesh.pressure_index(i - 1, j)]) / dx;
            let effective = nu + u_face_eddy(mesh, eddy, i, j);
            u_star[index] =
                centre + dt * (-convection + effective * diffusion - pressure_gradient / rho);
        }
    }

    for j in 1..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.v_index(i, j);
            if !mesh.v_face_open(i, j) {
                v_star[index] = 0.0;
                continue;
            }
            let centre = v[index];
            let dv_dy = v_convective_derivative(mesh, v, i, j, centre, model.convection);
            let u_face = 0.25
                * (u[mesh.u_index(i, j)]
                    + u[mesh.u_index(i + 1, j)]
                    + u[mesh.u_index(i, j - 1)]
                    + u[mesh.u_index(i + 1, j - 1)]);
            let du_dx = 0.5
                * ((u[mesh.u_index(i + 1, j)] - u[mesh.u_index(i, j)]) / dx
                    + (u[mesh.u_index(i + 1, j - 1)] - u[mesh.u_index(i, j - 1)]) / dx);
            let convection = u_face * du_dx + centre * dv_dy;

            let left = if i == 0 {
                match boundaries.left.kind {
                    BoundaryKind::Outlet => v[mesh.v_index(1, j)],
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.left.velocity[1] - centre
                    }
                }
            } else {
                v[mesh.v_index(i - 1, j)]
            };
            let right = if i + 1 == mesh.nx {
                match boundaries.right.kind {
                    BoundaryKind::Outlet => v[mesh.v_index(mesh.nx - 2, j)],
                    BoundaryKind::Slip => centre,
                    BoundaryKind::Wall | BoundaryKind::Inlet => {
                        2.0 * boundaries.right.velocity[1] - centre
                    }
                }
            } else {
                v[mesh.v_index(i + 1, j)]
            };
            let diffusion = (right - 2.0 * centre + left) / (dx * dx)
                + (v[mesh.v_index(i, j + 1)] - 2.0 * centre + v[mesh.v_index(i, j - 1)])
                    / (dy * dy);
            let pressure_gradient =
                (p[mesh.pressure_index(i, j)] - p[mesh.pressure_index(i, j - 1)]) / dy;
            let effective = nu + v_face_eddy(mesh, eddy, i, j);
            v_star[index] =
                centre + dt * (-convection + effective * diffusion - pressure_gradient / rho);
        }
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
) -> f64 {
    let dx = mesh.dx;
    let upwind = |velocity: f64| {
        if velocity >= 0.0 {
            (u[mesh.u_index(i, j)] - u[mesh.u_index(i - 1, j)]) / dx
        } else {
            (u[mesh.u_index(i + 1, j)] - u[mesh.u_index(i, j)]) / dx
        }
    };
    let value = |k: usize| u[mesh.u_index(k, j)];
    match scheme {
        ConvectionScheme::Upwind => upwind(velocity),
        ConvectionScheme::Central => (value(i + 1) - value(i - 1)) / (2.0 * mesh.dx),
        ConvectionScheme::Quick => {
            if velocity >= 0.0 {
                if i < 2 {
                    return upwind(velocity);
                }
                let east = 0.375 * value(i + 1) + 0.75 * value(i) - 0.125 * value(i - 1);
                let west = 0.375 * value(i) + 0.75 * value(i - 1) - 0.125 * value(i - 2);
                (east - west) / dx
            } else {
                if i + 2 > mesh.nx {
                    return upwind(velocity);
                }
                let east = 0.375 * value(i) + 0.75 * value(i + 1) - 0.125 * value(i + 2);
                let west = 0.375 * value(i - 1) + 0.75 * value(i) - 0.125 * value(i + 1);
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
) -> f64 {
    let dy = mesh.dy;
    let upwind = |velocity: f64| {
        if velocity >= 0.0 {
            (v[mesh.v_index(i, j)] - v[mesh.v_index(i, j - 1)]) / dy
        } else {
            (v[mesh.v_index(i, j + 1)] - v[mesh.v_index(i, j)]) / dy
        }
    };
    let value = |k: usize| v[mesh.v_index(i, k)];
    match scheme {
        ConvectionScheme::Upwind => upwind(velocity),
        ConvectionScheme::Central => (value(j + 1) - value(j - 1)) / (2.0 * mesh.dy),
        ConvectionScheme::Quick => {
            if velocity >= 0.0 {
                if j < 2 {
                    return upwind(velocity);
                }
                let north = 0.375 * value(j + 1) + 0.75 * value(j) - 0.125 * value(j - 1);
                let south = 0.375 * value(j) + 0.75 * value(j - 1) - 0.125 * value(j - 2);
                (north - south) / dy
            } else {
                if j + 2 > mesh.ny {
                    return upwind(velocity);
                }
                let north = 0.375 * value(j) + 0.75 * value(j + 1) - 0.125 * value(j + 2);
                let south = 0.375 * value(j - 1) + 0.75 * value(j) - 0.125 * value(j + 1);
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

fn project_velocity(mesh: &Mesh, density: f64, dt: f64, p: &[f64], u: &mut [f64], v: &mut [f64]) {
    let factor = dt / density;
    for j in 0..mesh.ny {
        for i in 1..mesh.nx {
            let index = mesh.u_index(i, j);
            if !mesh.u_face_open(i, j) {
                u[index] = 0.0;
                continue;
            }
            u[index] -= factor * (p[mesh.pressure_index(i, j)] - p[mesh.pressure_index(i - 1, j)])
                / mesh.dx;
        }
    }
    for j in 1..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.v_index(i, j);
            if !mesh.v_face_open(i, j) {
                v[index] = 0.0;
                continue;
            }
            v[index] -= factor * (p[mesh.pressure_index(i, j)] - p[mesh.pressure_index(i, j - 1)])
                / mesh.dy;
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
    let boundary_speed = [
        &model.boundaries.left,
        &model.boundaries.right,
        &model.boundaries.top,
        &model.boundaries.bottom,
    ]
    .iter()
    .map(|boundary| boundary.velocity[0].hypot(boundary.velocity[1]))
    .fold(0.0_f64, f64::max);
    let max_speed = max_velocity(u, v).max(boundary_speed);
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
    Ok(())
}

fn max_eddy_viscosity(eddy: Option<&[f64]>) -> f64 {
    eddy.map_or(0.0, |field| {
        field
            .iter()
            .fold(0.0_f64, |maximum, value| maximum.max(*value))
    })
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
    model: &Model,
    u: &[f64],
    v: &[f64],
    p: &[f64],
    temperature: Option<&[f64]>,
    step: usize,
    cells: &[(usize, usize)],
    samples: &mut [Vec<ProbeSample>],
) {
    for (index, &(i, j)) in cells.iter().enumerate() {
        let cell = mesh.pressure_index(i, j);
        let (u_value, v_value) = mesh.cell_velocity(u, v, i, j);
        samples[index].push(ProbeSample {
            step,
            time: step as f64 * model.time.dt,
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
/// viscosity vanishes at a wall by construction.
fn wall_shear(mesh: &Mesh, model: &Model, u: &[f64], v: &[f64]) -> Vec<WallShear> {
    let speed = reference_speed(model);
    let scale = 0.5 * model.fluid.density * speed * speed;
    let mu = model.fluid.viscosity;
    let mut report = Vec::new();
    for side in [Side::Left, Side::Right, Side::Top, Side::Bottom] {
        let boundary = model.boundaries.get(side);
        if boundary.kind != BoundaryKind::Wall {
            continue;
        }
        let mut sum = 0.0;
        let mut peak = 0.0_f64;
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
                    let shear = mu * (boundary.velocity[1] - centre) / (0.5 * mesh.dx);
                    sum += shear;
                    peak = peak.max(shear.abs());
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
                    let shear = mu * (boundary.velocity[0] - centre) / (0.5 * mesh.dy);
                    sum += shear;
                    peak = peak.max(shear.abs());
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
            friction_coefficient: if scale > 0.0 { mean / scale } else { 0.0 },
        });
    }
    report
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
/// Solid obstacles are interior and contribute no boundary flux. The outward
/// normal is `+x` on the right, `-x` on the left, `+y` on the top and `-y` on
/// the bottom, so the signs below match `outflow - inflow`.
fn mass_balance(mesh: &Mesh, density: f64, u: &[f64], v: &[f64]) -> MassBalance {
    let mut inflow = 0.0;
    let mut outflow = 0.0;
    let mut accumulate = |flux: f64| {
        if flux >= 0.0 {
            outflow += flux;
        } else {
            inflow -= flux;
        }
    };
    for j in 0..mesh.ny {
        accumulate(-u[mesh.u_index(0, j)] * mesh.dy);
        accumulate(u[mesh.u_index(mesh.nx, j)] * mesh.dy);
    }
    for i in 0..mesh.nx {
        accumulate(-v[mesh.v_index(i, 0)] * mesh.dx);
        accumulate(v[mesh.v_index(i, mesh.ny)] * mesh.dx);
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
        let upwind = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Upwind);
        let central = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Central);
        let quick = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Quick);
        let quick_reverse = u_convective_derivative(&mesh, &u, i, j, -1.0, ConvectionScheme::Quick);
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
        let central = v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Central);
        let quick = v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Quick);
        let quick_reverse = v_convective_derivative(&mesh, &v, 2, j, -1.0, ConvectionScheme::Quick);
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
        let upwind = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Upwind);
        let quick = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Quick);
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
            &model,
            &u,
            &v,
            &p,
            Some(&temperature),
            3,
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
}
