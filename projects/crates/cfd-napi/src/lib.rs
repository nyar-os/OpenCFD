//! Thin Node-API surface for OpenCFD.
//!
//! Every export performs one orthogonal operation and returns a JSON string or
//! boolean, so the TypeScript package owns argument parsing, help text and exit
//! codes. The actual document editing, validation, solving and export live in
//! `cfd-model` through [`OpenCfd`], which the CLI uses as well.
//!
//! Sessions are process-wide and addressed by an opaque handle. A handle
//! returned by `create` or `open` stays valid until `close` releases it.

#![deny(missing_docs)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use cfd_model::{
    BoundaryKind, CfdError, ConvectionScheme, ExportField, ExportFormat, ExportLayout,
    InletProfile, Obstacle, OpenCfd, OpenCfdOptions, Probe, Side, SolveOptions, TimeScheme,
};
use serde_json::{json, Value};
use wae::{function, register_module, CallContext, Error, JsValue, Result};

static SESSIONS: Mutex<Option<HashMap<u64, OpenCfd>>> = Mutex::new(None);
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn store() -> MutexGuard<'static, Option<HashMap<u64, OpenCfd>>> {
    SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn insert(session: OpenCfd) -> u64 {
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
    store()
        .get_or_insert_with(HashMap::new)
        .insert(handle, session);
    handle
}

fn with_session<T>(
    handle: u64,
    work: impl FnOnce(&mut OpenCfd) -> std::result::Result<T, CfdError>,
) -> Result<T> {
    let mut guard = store();
    let sessions = guard.get_or_insert_with(HashMap::new);
    let session = sessions.get_mut(&handle).ok_or_else(|| {
        Error::new(format!(
            "session/handle-unknown: no OpenCFD session is open with handle {handle}"
        ))
    })?;
    work(session).map_err(fail)
}

fn fail(error: CfdError) -> Error {
    Error::new(format!("{}: {}", error.code, error.message))
}

fn argument(ctx: &CallContext, index: usize) -> Result<JsValue> {
    ctx.args()
        .get(index)
        .copied()
        .ok_or_else(|| Error::new("session/argument-invalid: a required argument is missing"))
}

fn text(ctx: &CallContext, index: usize) -> Result<String> {
    argument(ctx, index)?.as_string()
}

fn optional_text(ctx: &CallContext, index: usize) -> Result<Option<String>> {
    match ctx.args().get(index).copied() {
        Some(value) if !value.is_undefined()? => Ok(Some(value.as_string()?)),
        _ => Ok(None),
    }
}

fn number(ctx: &CallContext, index: usize) -> Result<f64> {
    argument(ctx, index)?.as_f64()
}

fn boolean(ctx: &CallContext, index: usize) -> Result<bool> {
    argument(ctx, index)?.as_bool()
}

fn optional_number(ctx: &CallContext, index: usize) -> Result<Option<f64>> {
    match ctx.args().get(index).copied() {
        Some(value) if !value.is_undefined()? => Ok(Some(value.as_f64()?)),
        _ => Ok(None),
    }
}

fn unsigned(ctx: &CallContext, index: usize) -> Result<usize> {
    let raw = argument(ctx, index)?.as_i64()?;
    usize::try_from(raw).map_err(|_| {
        Error::new("session/argument-invalid: the value must be a non-negative integer")
    })
}

fn optional_unsigned(ctx: &CallContext, index: usize) -> Result<Option<usize>> {
    match ctx.args().get(index).copied() {
        Some(value) if !value.is_undefined()? => {
            let raw = value.as_i64()?;
            usize::try_from(raw).map(Some).map_err(|_| {
                Error::new("session/argument-invalid: the value must be a non-negative integer")
            })
        }
        _ => Ok(None),
    }
}

fn handle(ctx: &CallContext, index: usize) -> Result<u64> {
    let raw = argument(ctx, index)?.as_i64()?;
    u64::try_from(raw).map_err(|_| {
        Error::new("session/argument-invalid: the session handle must be a non-negative integer")
    })
}

fn respond(ctx: &CallContext, value: &Value) -> Result<JsValue> {
    ctx.env().string(&value.to_string())
}

/// Library version of the native binding.
#[function]
fn version(ctx: CallContext) -> Result<JsValue> {
    ctx.env().string(env!("CARGO_PKG_VERSION"))
}

/// Reports the solver and IO capability matrix of the installed engine.
#[function]
fn capabilities(ctx: CallContext) -> Result<JsValue> {
    let session = OpenCfd::new(OpenCfdOptions::default());
    ctx.env().string(session.capabilities())
}

/// Creates the default cavity case at `path` and returns `{ handle, document }`.
#[function]
fn create(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    let document = session.create(Path::new(&path)).map_err(fail)?;
    let handle = insert(session);
    respond(&ctx, &json!({ "handle": handle, "document": document }))
}

/// Opens an existing `.ocfd` document and returns `{ handle, document }`.
#[function]
fn open(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    let document = session.open(Path::new(&path)).map_err(fail)?;
    let handle = insert(session);
    respond(&ctx, &json!({ "handle": handle, "document": document }))
}

/// Validates and writes one session document, optionally to a new path.
#[function]
fn save(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let target = optional_text(&ctx, 1)?;
    let report = with_session(handle, |session| {
        session.save(target.as_deref().map(Path::new))
    })?;
    respond(&ctx, &json!(report))
}

/// Sets the structured mesh of one session document.
#[function]
fn mesh_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let nx = unsigned(&ctx, 1)?;
    let ny = unsigned(&ctx, 2)?;
    let length_x = number(&ctx, 3)?;
    let length_y = number(&ctx, 4)?;
    let summary = with_session(handle, |session| {
        session.mesh_set(nx, ny, length_x, length_y)
    })?;
    respond(&ctx, &json!(summary))
}

/// Sets density and dynamic viscosity of one session document.
#[function]
fn fluid_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let density = number(&ctx, 1)?;
    let viscosity = number(&ctx, 2)?;
    let summary = with_session(handle, |session| session.fluid_set(density, viscosity))?;
    respond(&ctx, &json!(summary))
}

/// Sets time integration settings of one session document.
#[function]
fn time_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let dt = number(&ctx, 1)?;
    let steps = unsigned(&ctx, 2)?;
    let pressure_iterations = unsigned(&ctx, 3)?;
    let tolerance = optional_number(&ctx, 4)?;
    let summary = with_session(handle, |session| {
        session.time_set(dt, steps, pressure_iterations, tolerance)
    })?;
    respond(&ctx, &json!(summary))
}

/// Sets the explicit time integration scheme of one session document.
#[function]
fn time_scheme_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let scheme = text(&ctx, 1)?.parse::<TimeScheme>().map_err(fail)?;
    let summary = with_session(handle, |session| session.time_scheme_set(scheme))?;
    respond(&ctx, &json!(summary))
}

/// Enables or disables adaptive time stepping of one session document.
#[function]
fn time_adaptive_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let enabled = boolean(&ctx, 1)?;
    let target_cfl = optional_number(&ctx, 2)?;
    let summary = with_session(handle, |session| {
        session.time_adaptive_set(enabled, target_cfl)
    })?;
    respond(&ctx, &json!(summary))
}

/// Sets one boundary of one session document.
#[function]
fn boundary_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let side = text(&ctx, 1)?.parse::<Side>().map_err(fail)?;
    let kind = text(&ctx, 2)?.parse::<BoundaryKind>().map_err(fail)?;
    let u = optional_number(&ctx, 3)?;
    let v = optional_number(&ctx, 4)?;
    let pressure = optional_number(&ctx, 5)?;
    let summary = with_session(handle, |session| {
        session.boundary_set(side, kind, u, v, pressure)
    })?;
    respond(&ctx, &json!(summary))
}

/// Sets the normal-velocity profile of one inlet side.
#[function]
fn boundary_profile_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let side = text(&ctx, 1)?.parse::<Side>().map_err(fail)?;
    let profile = text(&ctx, 2)?.parse::<InletProfile>().map_err(fail)?;
    let exponent = optional_number(&ctx, 3)?;
    let summary = with_session(handle, |session| {
        session.boundary_profile_set(side, profile, exponent)
    })?;
    respond(&ctx, &json!(summary))
}

/// Validates one session document and returns its summary.
#[function]
fn validate(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let summary = with_session(handle, |session| session.validate())?;
    respond(&ctx, &json!(summary))
}

/// Appends one solid obstacle from a JSON request and returns the summary.
#[function]
fn obstacle_add(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let request = text(&ctx, 1)?;
    let obstacle = serde_json::from_str::<Obstacle>(&request).map_err(|error| {
        Error::new(format!(
            "session/argument-invalid: cannot decode the obstacle request: {error}"
        ))
    })?;
    let summary = with_session(handle, |session| session.obstacle_add(obstacle))?;
    respond(&ctx, &json!(summary))
}

/// Returns the solid obstacles of one session document as a JSON array.
#[function]
fn obstacle_list(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let obstacles = with_session(handle, |session| Ok(session.obstacle_list()))?;
    respond(&ctx, &json!(obstacles))
}

/// Appends one point monitor from a JSON request and returns the summary.
#[function]
fn probe_add(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let request = text(&ctx, 1)?;
    let probe = serde_json::from_str::<Probe>(&request).map_err(|error| {
        Error::new(format!(
            "session/argument-invalid: cannot decode the probe request: {error}"
        ))
    })?;
    let summary = with_session(handle, |session| session.probe_add(probe))?;
    respond(&ctx, &json!(summary))
}

/// Returns the point monitors of one session document as a JSON array.
#[function]
fn probe_list(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let probes = with_session(handle, |session| Ok(session.probe_list()))?;
    respond(&ctx, &json!(probes))
}

/// Removes every point monitor and returns the summary.
#[function]
fn probe_clear(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let summary = with_session(handle, |session| session.probe_clear())?;
    respond(&ctx, &json!(summary))
}

/// Configures passive scalar transport and returns the summary.
#[function]
fn scalar_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let diffusivity = number(&ctx, 1)?;
    let initial = optional_number(&ctx, 2)?;
    let inlet = optional_number(&ctx, 3)?;
    let summary = with_session(handle, |session| {
        session.scalar_set(diffusivity, initial, inlet)
    })?;
    respond(&ctx, &json!(summary))
}

/// Selects the convective discretisation and returns the summary.
#[function]
fn convection_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let scheme = text(&ctx, 1)?.parse::<ConvectionScheme>().map_err(fail)?;
    let summary = with_session(handle, |session| session.convection_set(scheme))?;
    respond(&ctx, &json!(summary))
}

/// Sets the uniform imposed pressure gradient and returns the summary.
#[function]
fn pressure_gradient_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let dpdx = number(&ctx, 1)?;
    let dpdy = number(&ctx, 2)?;
    let summary = with_session(handle, |session| session.pressure_gradient_set(dpdx, dpdy))?;
    respond(&ctx, &json!(summary))
}

/// Sets force and moment reference quantities and returns the summary.
#[function]
fn reference_set(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let velocity = optional_number(&ctx, 1)?;
    let area = optional_number(&ctx, 2)?;
    let length = optional_number(&ctx, 3)?;
    let summary = with_session(handle, |session| {
        session.reference_set(velocity, area, length)
    })?;
    respond(&ctx, &json!(summary))
}

/// Solves one session document with optional solve-time overrides.
#[function]
fn solve(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let options = SolveOptions {
        steps: optional_unsigned(&ctx, 1)?,
        tolerance: optional_number(&ctx, 2)?,
        check_every: optional_unsigned(&ctx, 3)?,
    };
    let restart = optional_text(&ctx, 4)?.map(PathBuf::from);
    let checkpoint = optional_text(&ctx, 5)?.map(PathBuf::from);
    let report = with_session(handle, |session| {
        session.solve_with_checkpoint(&options, restart.as_deref(), checkpoint.as_deref())
    })?;
    respond(&ctx, &json!(report))
}

/// Solves and exports one session document as CSV, VTK, SVG or contours.
///
/// `field` affects SVG and contour output and defaults to `pressure`. `layout`
/// selects cell-centred or node-sampled CSV/VTK data and defaults to `cell`.
#[function]
fn export(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let format = text(&ctx, 1)?.parse::<ExportFormat>().map_err(fail)?;
    let out = text(&ctx, 2)?;
    let field = match optional_text(&ctx, 3)? {
        Some(value) => value.parse::<ExportField>().map_err(fail)?,
        None => ExportField::Pressure,
    };
    let layout = match optional_text(&ctx, 4)? {
        Some(value) => value.parse::<ExportLayout>().map_err(fail)?,
        None => ExportLayout::Cell,
    };
    let report = with_session(handle, |session| {
        session.export_field_with_layout(format, Path::new(&out), field, layout)
    })?;
    respond(&ctx, &json!(report))
}

/// Releases one session handle.
#[function]
fn close(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let removed = store()
        .get_or_insert_with(HashMap::new)
        .remove(&handle)
        .is_some();
    if !removed {
        return Err(Error::new(format!(
            "session/handle-unknown: no OpenCFD session is open with handle {handle}"
        )));
    }
    ctx.env().boolean(true)
}

register_module![
    version,
    capabilities,
    create,
    open,
    save,
    mesh_set,
    fluid_set,
    time_set,
    time_scheme_set,
    time_adaptive_set,
    boundary_set,
    boundary_profile_set,
    obstacle_add,
    obstacle_list,
    probe_add,
    probe_list,
    probe_clear,
    scalar_set,
    convection_set,
    pressure_gradient_set,
    reference_set,
    validate,
    solve,
    export,
    close
];
