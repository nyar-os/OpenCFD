//! Lagrangian particle (tracer) tracking on a solved velocity field.
//!
//! A tracer is a massless point that follows the instantaneous cell-centred
//! velocity. The trajectory is integrated with a second-order midpoint rule,
//! starting from each configured seed and recording one sample per step. A
//! tracer stops when it leaves the domain or reaches a solid cell, so a track
//! never reports a position inside an obstacle. Unlike the streamline preview,
//! the result is a time-accurate path over the solved field, not an
//! instantaneous streamline.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::export::{with_extension_or_file, write_atomic};
use crate::mesh::Mesh;
use crate::solver::Solution;
use crate::CfdError;

/// Default number of advection steps of one tracer.
pub const DEFAULT_PARTICLE_STEPS: usize = 200;

/// Number of seed rows and columns of the default regular grid.
const DEFAULT_SEED_ROWS: usize = 5;
const DEFAULT_SEED_COLUMNS: usize = 5;

/// Starting position of one tracer (m).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleSeed {
    /// Seed x coordinate (m).
    pub x: f64,
    /// Seed y coordinate (m).
    pub y: f64,
}

/// Settings of one particle trace.
///
/// An empty `seeds` list asks for a regular 5×5 grid of fluid seeds inset by
/// half a grid spacing, so a preview works without naming positions. Every
/// explicit seed must lie inside the domain and outside every solid cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleConfig {
    /// Explicit seed positions (m); empty means the default regular grid.
    #[serde(default)]
    pub seeds: Vec<ParticleSeed>,
    /// Advection time step (s); finite and positive.
    pub dt: f64,
    /// Maximum number of advection steps per tracer; positive.
    #[serde(default = "default_steps")]
    pub max_steps: usize,
}

impl Default for ParticleConfig {
    fn default() -> Self {
        Self {
            seeds: Vec::new(),
            dt: 0.0,
            max_steps: DEFAULT_PARTICLE_STEPS,
        }
    }
}

/// One recorded tracer position and the local velocity there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleSample {
    /// Advection step; the initial sample is step 0.
    pub step: usize,
    /// Physical time `step × dt` (s).
    pub time: f64,
    /// Tracer x coordinate (m).
    pub x: f64,
    /// Tracer y coordinate (m).
    pub y: f64,
    /// Sampled x velocity (m/s).
    pub u: f64,
    /// Sampled y velocity (m/s).
    pub v: f64,
    /// Sampled speed `√(u² + v²)` (m/s).
    pub speed: f64,
}

/// Trajectory of one tracer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParticleTrack {
    /// Stable identifier `particle-N` in seed order.
    pub id: String,
    /// Seed position `[x, y]` (m).
    pub start: [f64; 2],
    /// Samples in time order; the first sample is the seed at `t = 0`.
    pub samples: Vec<ParticleSample>,
}

/// Traces every tracer through a velocity field.
///
/// `dt` and `max_steps` come from `config`. A tracer advances with the
/// midpoint rule `xⁿ⁺¹ = xⁿ + dt·U(xⁿ + dt/2·U(xⁿ))`, using bilinear
/// interpolation of the cell-centred velocity. The repeated or solid
/// interpolation neighbours are renormalised, so a tracer can pass next to an
/// obstacle. The trace stops before a step would enter a solid cell or leave
/// the domain, and the last recorded sample is the last valid position.
pub fn trace_particles(
    mesh: &Mesh,
    u: &[f64],
    v: &[f64],
    config: &ParticleConfig,
) -> Result<Vec<ParticleTrack>, CfdError> {
    if !config.dt.is_finite() || config.dt <= 0.0 {
        return Err(CfdError::new(
            "particle/invalid",
            "particle dt must be a finite positive number",
        ));
    }
    if config.max_steps == 0 {
        return Err(CfdError::new(
            "particle/invalid",
            "particle maxSteps must be greater than 0",
        ));
    }
    let seeds = if config.seeds.is_empty() {
        default_seeds(mesh)
    } else {
        let mut seeds = Vec::with_capacity(config.seeds.len());
        for seed in &config.seeds {
            validate_seed(mesh, *seed)?;
            seeds.push(*seed);
        }
        seeds
    };
    let mut tracks = Vec::with_capacity(seeds.len());
    for (index, seed) in seeds.into_iter().enumerate() {
        tracks.push(ParticleTrack {
            id: format!("particle-{}", index + 1),
            start: [seed.x, seed.y],
            samples: trace_one(mesh, u, v, seed, config),
        });
    }
    Ok(tracks)
}

/// Traces every tracer through the velocity field of a solved case.
pub fn trace_solution_particles(
    solution: &Solution,
    config: &ParticleConfig,
) -> Result<Vec<ParticleTrack>, CfdError> {
    trace_particles(&solution.mesh, &solution.u, &solution.v, config)
}

/// Writes the trajectories as long-form CSV rows.
///
/// The header is `particle,step,time,x,y,u,v,speed`; each track contributes one
/// row per sample, grouped by seed order. When `path` ends in `.csv` it is used
/// as the output file; otherwise it is treated as a directory and
/// `particles.csv` is created inside it.
pub fn export_particles(
    solution: &Solution,
    path: &Path,
    config: &ParticleConfig,
) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "particles.csv");
    let tracks = trace_solution_particles(solution, config)?;
    let mut text = String::from("particle,step,time,x,y,u,v,speed\n");
    for track in &tracks {
        for sample in &track.samples {
            use std::fmt::Write as _;
            let _ = writeln!(
                text,
                "{},{},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e}",
                track.id,
                sample.step,
                sample.time,
                sample.x,
                sample.y,
                sample.u,
                sample.v,
                sample.speed
            );
        }
    }
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Regular seed grid with half-spacing insets, skipping solid seeds.
fn default_seeds(mesh: &Mesh) -> Vec<ParticleSeed> {
    let mut seeds = Vec::new();
    for row in 0..DEFAULT_SEED_ROWS {
        for column in 0..DEFAULT_SEED_COLUMNS {
            let x = (column as f64 + 0.5) / DEFAULT_SEED_COLUMNS as f64 * mesh.length_x;
            let y = (row as f64 + 0.5) / DEFAULT_SEED_ROWS as f64 * mesh.length_y;
            if !inside(mesh, x, y) || solid_at(mesh, x, y) {
                continue;
            }
            seeds.push(ParticleSeed { x, y });
        }
    }
    seeds
}

/// Rejects a seed outside the domain or inside a solid cell.
fn validate_seed(mesh: &Mesh, seed: ParticleSeed) -> Result<(), CfdError> {
    if !seed.x.is_finite() || !seed.y.is_finite() {
        return Err(CfdError::new(
            "particle/invalid",
            "particle seeds must have finite coordinates",
        ));
    }
    if !inside(mesh, seed.x, seed.y) {
        return Err(CfdError::new(
            "particle/invalid",
            format!(
                "particle seed ({}, {}) lies outside the {}×{} m domain",
                seed.x, seed.y, mesh.length_x, mesh.length_y
            ),
        ));
    }
    if solid_at(mesh, seed.x, seed.y) {
        return Err(CfdError::new(
            "particle/invalid",
            format!(
                "particle seed ({}, {}) lies inside an obstacle; move it into the fluid",
                seed.x, seed.y
            ),
        ));
    }
    Ok(())
}

/// Advances one tracer and returns its samples.
fn trace_one(
    mesh: &Mesh,
    u: &[f64],
    v: &[f64],
    seed: ParticleSeed,
    config: &ParticleConfig,
) -> Vec<ParticleSample> {
    let dt = config.dt;
    let mut samples = Vec::with_capacity(config.max_steps + 1);
    let mut x = seed.x;
    let mut y = seed.y;
    let Some((u0, v0)) = sample_velocity(mesh, u, v, x, y) else {
        return samples;
    };
    samples.push(ParticleSample {
        step: 0,
        time: 0.0,
        x,
        y,
        u: u0,
        v: v0,
        speed: u0.hypot(v0),
    });
    for step in 1..=config.max_steps {
        let Some((k1u, k1v)) = sample_velocity(mesh, u, v, x, y) else {
            break;
        };
        let mid_x = x + 0.5 * dt * k1u;
        let mid_y = y + 0.5 * dt * k1v;
        let Some((k2u, k2v)) = sample_velocity(mesh, u, v, mid_x, mid_y) else {
            break;
        };
        let next_x = x + dt * k2u;
        let next_y = y + dt * k2v;
        if !next_x.is_finite() || !next_y.is_finite() {
            break;
        }
        if !inside(mesh, next_x, next_y) || solid_at(mesh, next_x, next_y) {
            break;
        }
        let Some((su, sv)) = sample_velocity(mesh, u, v, next_x, next_y) else {
            break;
        };
        x = next_x;
        y = next_y;
        samples.push(ParticleSample {
            step,
            time: step as f64 * dt,
            x,
            y,
            u: su,
            v: sv,
            speed: su.hypot(sv),
        });
    }
    samples
}

/// Bilinear cell-centred velocity, renormalised over the available fluid cells.
fn sample_velocity(mesh: &Mesh, u: &[f64], v: &[f64], x: f64, y: f64) -> Option<(f64, f64)> {
    if !inside(mesh, x, y) {
        return None;
    }
    let gx = (x / mesh.dx - 0.5).clamp(0.0, (mesh.nx - 1) as f64);
    let gy = (y / mesh.dy - 0.5).clamp(0.0, (mesh.ny - 1) as f64);
    let i0 = gx.floor() as usize;
    let j0 = gy.floor() as usize;
    let i1 = (i0 + 1).min(mesh.nx - 1);
    let j1 = (j0 + 1).min(mesh.ny - 1);
    let tx = gx - i0 as f64;
    let ty = gy - j0 as f64;
    let corners = [
        (i0, j0, (1.0 - tx) * (1.0 - ty)),
        (i1, j0, tx * (1.0 - ty)),
        (i0, j1, (1.0 - tx) * ty),
        (i1, j1, tx * ty),
    ];
    let mut sum_u = 0.0;
    let mut sum_v = 0.0;
    let mut weight = 0.0;
    for (i, j, corner_weight) in corners {
        if corner_weight <= 0.0 || mesh.is_solid(i, j) {
            continue;
        }
        let (cu, cv) = mesh.cell_velocity(u, v, i, j);
        sum_u += corner_weight * cu;
        sum_v += corner_weight * cv;
        weight += corner_weight;
    }
    if weight <= 0.0 {
        return None;
    }
    Some((sum_u / weight, sum_v / weight))
}

/// Whether a point lies inside the closed domain rectangle.
fn inside(mesh: &Mesh, x: f64, y: f64) -> bool {
    x >= 0.0 && y >= 0.0 && x <= mesh.length_x && y <= mesh.length_y
}

/// Whether the cell containing a point is solid.
fn solid_at(mesh: &Mesh, x: f64, y: f64) -> bool {
    let i = ((x / mesh.dx).floor() as isize).clamp(0, mesh.nx as isize - 1) as usize;
    let j = ((y / mesh.dy).floor() as isize).clamp(0, mesh.ny as isize - 1) as usize;
    mesh.is_solid(i, j)
}

fn default_steps() -> usize {
    DEFAULT_PARTICLE_STEPS
}
