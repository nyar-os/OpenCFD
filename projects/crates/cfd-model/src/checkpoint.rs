//! Restart checkpoints for the transient solver.
//!
//! A checkpoint stores the complete staggered state (`u`, `v`, `p` and the
//! optional passive scalar) together with the global step count, simulated time
//! and last time step. [`crate::solver::solve_with_checkpoint`] resumes from it
//! and continues the step and sample numbering, so a chain of checkpointed runs
//! reproduces a single continuous run.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::model::Model;
use crate::solver::Solution;
use crate::CfdError;

/// Identifier stored in the `format` field of a checkpoint.
pub const CHECKPOINT_FORMAT: &str = "open-cfd-checkpoint";

/// Version of the checkpoint understood by this build.
pub const CHECKPOINT_VERSION: u32 = 1;

/// Grid identity used to reject checkpoints written for another mesh.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointMesh {
    /// Number of cells in the x direction.
    pub nx: usize,
    /// Number of cells in the y direction.
    pub ny: usize,
    /// Physical domain length in x (m).
    pub length_x: f64,
    /// Physical domain length in y (m).
    pub length_y: f64,
}

/// Complete restart state of one simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    /// Checkpoint format tag; always [`CHECKPOINT_FORMAT`].
    pub format: String,
    /// Checkpoint version; always [`CHECKPOINT_VERSION`].
    pub version: u32,
    /// Mesh the stored state belongs to.
    pub mesh: CheckpointMesh,
    /// Number of time steps already advanced from `t = 0`.
    pub steps: usize,
    /// Simulated physical time at this state (s).
    pub time: f64,
    /// Time step that produced this state (s).
    pub dt: f64,
    /// Staggered x-face velocities.
    pub u: Vec<f64>,
    /// Staggered y-face velocities.
    pub v: Vec<f64>,
    /// Cell-centred pressure.
    pub p: Vec<f64>,
    /// Cell-centred passive scalar, when the case configures one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<Vec<f64>>,
}

impl Checkpoint {
    /// Captures the state of a solved field.
    pub fn capture(model: &Model, solution: &Solution) -> Self {
        Self {
            format: CHECKPOINT_FORMAT.to_string(),
            version: CHECKPOINT_VERSION,
            mesh: CheckpointMesh {
                nx: model.mesh.nx,
                ny: model.mesh.ny,
                length_x: model.mesh.length_x,
                length_y: model.mesh.length_y,
            },
            steps: solution.report.steps,
            time: solution.report.time,
            dt: solution.report.dt,
            u: solution.u.clone(),
            v: solution.v.clone(),
            p: solution.p.clone(),
            temperature: solution.temperature.clone(),
        }
    }

    /// Loads and structurally validates a checkpoint file.
    pub fn load(path: &Path) -> Result<Self, CfdError> {
        let text = fs::read_to_string(path).map_err(|error| {
            CfdError::new(
                "checkpoint/io",
                format!("cannot read checkpoint {}: {error}", path.display()),
            )
        })?;
        let checkpoint: Self = serde_json::from_str(&text).map_err(|error| {
            CfdError::new(
                "checkpoint/invalid",
                format!("cannot parse checkpoint {}: {error}", path.display()),
            )
        })?;
        checkpoint.validate_self()?;
        Ok(checkpoint)
    }

    /// Writes the checkpoint as pretty JSON.
    pub fn save(&self, path: &Path) -> Result<(), CfdError> {
        self.validate_self()?;
        let body = serde_json::to_string_pretty(self).map_err(|error| {
            CfdError::new(
                "checkpoint/invalid",
                format!("cannot serialize checkpoint: {error}"),
            )
        })?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| {
                    CfdError::new(
                        "checkpoint/io",
                        format!("cannot create {}: {error}", parent.display()),
                    )
                })?;
            }
        }
        fs::write(path, body + "\n").map_err(|error| {
            CfdError::new(
                "checkpoint/io",
                format!("cannot write checkpoint {}: {error}", path.display()),
            )
        })
    }

    /// Checks the format tag, version, shape and finite values.
    pub fn validate_self(&self) -> Result<(), CfdError> {
        if self.format != CHECKPOINT_FORMAT {
            return Err(CfdError::new(
                "checkpoint/invalid",
                format!(
                    "checkpoint format `{}` is not `{CHECKPOINT_FORMAT}`",
                    self.format
                ),
            ));
        }
        if self.version != CHECKPOINT_VERSION {
            return Err(CfdError::new(
                "checkpoint/invalid",
                format!(
                    "checkpoint version {} is not supported; expected {CHECKPOINT_VERSION}",
                    self.version
                ),
            ));
        }
        if self.mesh.nx == 0 || self.mesh.ny == 0 {
            return Err(CfdError::new(
                "checkpoint/invalid",
                "checkpoint mesh dimensions must be positive",
            ));
        }
        if !(self.mesh.length_x.is_finite() && self.mesh.length_x > 0.0)
            || !(self.mesh.length_y.is_finite() && self.mesh.length_y > 0.0)
        {
            return Err(CfdError::new(
                "checkpoint/invalid",
                "checkpoint mesh lengths must be finite and positive",
            ));
        }
        if !(self.steps == 0 || self.dt.is_finite() && self.dt > 0.0) || !self.time.is_finite() {
            return Err(CfdError::new(
                "checkpoint/invalid",
                "checkpoint time and dt must be finite, with dt > 0 after the first step",
            ));
        }
        let u_count = (self.mesh.nx + 1) * self.mesh.ny;
        let v_count = self.mesh.nx * (self.mesh.ny + 1);
        let p_count = self.mesh.nx * self.mesh.ny;
        if self.u.len() != u_count || self.v.len() != v_count || self.p.len() != p_count {
            return Err(CfdError::new(
                "checkpoint/invalid",
                format!(
                    "checkpoint arrays do not match the mesh: u={}, v={}, p={}, expected {u_count}, {v_count}, {p_count}",
                    self.u.len(),
                    self.v.len(),
                    self.p.len()
                ),
            ));
        }
        if self
            .temperature
            .as_ref()
            .is_some_and(|field| field.len() != p_count)
        {
            return Err(CfdError::new(
                "checkpoint/invalid",
                "checkpoint scalar array does not match the mesh",
            ));
        }
        let finite = self
            .u
            .iter()
            .chain(self.v.iter())
            .chain(self.p.iter())
            .chain(self.temperature.iter().flat_map(|field| field.iter()))
            .all(|value| value.is_finite());
        if !finite {
            return Err(CfdError::new(
                "checkpoint/invalid",
                "checkpoint contains NaN or Inf",
            ));
        }
        Ok(())
    }

    /// Checks that the checkpoint state matches a case before resuming it.
    pub fn validate_against(&self, model: &Model) -> Result<(), CfdError> {
        self.validate_self()?;
        if self.mesh.nx != model.mesh.nx
            || self.mesh.ny != model.mesh.ny
            || (self.mesh.length_x - model.mesh.length_x).abs() > 1e-12
            || (self.mesh.length_y - model.mesh.length_y).abs() > 1e-12
        {
            return Err(CfdError::new(
                "checkpoint/mismatch",
                format!(
                    "checkpoint mesh {}x{} over {}x{} does not match the case mesh {}x{} over {}x{}",
                    self.mesh.nx,
                    self.mesh.ny,
                    self.mesh.length_x,
                    self.mesh.length_y,
                    model.mesh.nx,
                    model.mesh.ny,
                    model.mesh.length_x,
                    model.mesh.length_y
                ),
            ));
        }
        if self.temperature.is_some() != model.scalar.is_some() {
            return Err(CfdError::new(
                "checkpoint/mismatch",
                "checkpoint scalar state does not match the scalar configuration of the case",
            ));
        }
        Ok(())
    }
}
