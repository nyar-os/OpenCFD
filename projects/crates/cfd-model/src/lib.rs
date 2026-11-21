//! OpenCFD: an honest two-dimensional incompressible Navier-Stokes solver.
//!
//! This crate intentionally implements a small, verifiable subset of CFD:
//!
//! * two-dimensional, single-phase, incompressible flow;
//! * constant density and constant dynamic viscosity (laminar);
//! * a structured Cartesian mesh with staggered pressure and velocity;
//! * first-order explicit time stepping with the Chorin projection method;
//! * solid rectangle and circle obstacles with drag, lift and moment
//!   coefficients and configurable reference quantities;
//! * passive scalar (temperature) transport with explicit advection-diffusion;
//! * optional eddy-viscosity turbulence (constant or mixing-length);
//! * thermal side/obstacle boundaries, volumetric heat sources and obstacle
//!   Nusselt numbers;
//! * SVG previews of cell-centred fields and traced streamlines;
//! * boundary mass balance, sampled continuity residuals, vorticity, stream
//!   function and point probe time series;
//! * lid-driven cavity and Couette-like channel cases with wall, slip, inlet and
//!   pressure-outlet sides.
//!
//! Compressibility, heat coupling with buoyancy, three-dimensional grids,
//! adaptive meshing and external format import are **not** implemented. See
//! [`CAPABILITIES_JSON`] for the machine-readable capability matrix and the
//! repository Readme for the user facing description.

#![deny(missing_docs)]

mod error;
mod export;
mod fields;
mod mesh;
mod model;
mod session;
mod solver;

pub use crate::error::CfdError;
pub use crate::export::{
    export_csv, export_streamlines, export_svg, export_vtk, write_atomic, ExportField,
};
pub use crate::fields::{stream_function, vorticity};
pub use crate::mesh::Mesh;
pub use crate::model::{
    Boundaries, Boundary, BoundaryKind, Buoyancy, ConvectionScheme, Fluid, InitialField,
    InletProfile, InletProfiles, MeshConfig, Model, Obstacle, Probe, Reference, ScalarConfig, Side,
    SideThermal, ThermalConfig, TimeSettings, Turbulence, TurbulenceModel, Units, FORMAT, VERSION,
};
pub use crate::session::{
    CaseSummary, ExportFormat, ExportReport, OpenCfd, OpenCfdOptions, SaveReport,
};
pub use crate::solver::{
    solve, solve_pressure, solve_with, transport_scalar, transport_thermal, ForceSample,
    MassBalance, ObstacleForce, ObstacleNusselt, PressureSettings, PressureStats, ProbeSample,
    ProbeSeries, Report, ResidualSample, Solution, SolveOptions, WallShear, REPORT_FORMAT,
    REPORT_VERSION,
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
    "vorticity": "supported",
    "streamFunction": "supported",
    "convectionSchemes": "supported",
    "probes": "supported",
    "wallShear": "supported",
    "slipBoundary": "supported",
    "pressureOutlet": "supported",
    "inletProfiles": "supported",
    "momentCoefficients": "supported",
    "massBalance": "supported",
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
    "streamlines": "supported",
    "contours": "unsupported",
    "periodic": "unsupported",
    "inp": "unsupported",
    "openfoam": "unsupported"
  }
}"#;
