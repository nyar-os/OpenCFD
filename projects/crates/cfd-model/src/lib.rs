//! OpenCFD: an honest two-dimensional incompressible Navier-Stokes solver.
//!
//! This crate intentionally implements a small, verifiable subset of CFD:
//!
//! * two-dimensional, single-phase, incompressible flow;
//! * constant density and constant dynamic viscosity (laminar);
//! * a structured Cartesian mesh with staggered pressure and velocity;
//! * first-order explicit time stepping with the Chorin projection method;
//! * solid rectangle and circle obstacles with force coefficients;
//! * passive scalar (temperature) transport with explicit advection-diffusion;
//! * optional eddy-viscosity turbulence (constant or mixing-length);
//! * thermal side/obstacle boundaries, volumetric heat sources and obstacle
//!   Nusselt numbers;
//! * SVG previews of the pressure and temperature fields;
//! * lid-driven cavity and Couette-like channel cases with wall, inlet and
//!   outlet sides.
//!
//! Compressibility, heat coupling with buoyancy, three-dimensional grids,
//! adaptive meshing and external format import are **not** implemented. See
//! [`CAPABILITIES_JSON`] for the machine-readable capability matrix and the
//! repository Readme for the user facing description.

#![deny(missing_docs)]

mod error;
mod export;
mod mesh;
mod model;
mod session;
mod solver;

pub use crate::error::CfdError;
pub use crate::export::{export_csv, export_svg, export_vtk, write_atomic, ExportField};
pub use crate::mesh::Mesh;
pub use crate::model::{
    Boundaries, Boundary, BoundaryKind, Buoyancy, Fluid, InitialField, MeshConfig, Model, Obstacle,
    ScalarConfig, Side, SideThermal, ThermalConfig, TimeSettings, Turbulence, TurbulenceModel,
    Units, FORMAT, VERSION,
};
pub use crate::session::{
    CaseSummary, ExportFormat, ExportReport, OpenCfd, OpenCfdOptions, SaveReport,
};
pub use crate::solver::{
    solve, solve_pressure, solve_with, transport_scalar, transport_thermal, ForceSample,
    ObstacleForce, ObstacleNusselt, PressureSettings, PressureStats, Report, ResidualSample,
    Solution, SolveOptions, REPORT_FORMAT, REPORT_VERSION,
};

/// Stable machine-readable capability matrix of this build.
///
/// The value is a pretty-printed JSON object with two sections: `solver` and
/// `io`. Supported entries are marked `"supported"`, everything else is
/// `"unsupported"` and must not be silently treated as working.
pub const CAPABILITIES_JSON: &str = r#"{
  "solver": {
    "cavity2d": "supported",
    "couette2d": "supported",
    "obstacles": "supported",
    "scalar": "supported",
    "convergence": "supported",
    "buoyancy": "unsupported",
    "compressible": "unsupported",
    "turbulence": "supported",
    "heat": "supported",
    "threeD": "unsupported"
  },
  "io": {
    "ocfd": "supported",
    "csv": "supported",
    "vtk": "supported",
    "svg": "supported",
    "inp": "unsupported",
    "openfoam": "unsupported"
  }
}"#;
