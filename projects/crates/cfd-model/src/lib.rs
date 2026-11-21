//! OpenCFD: an honest two-dimensional incompressible Navier-Stokes solver.
//!
//! This crate intentionally implements a small, verifiable subset of CFD:
//!
//! * two-dimensional, single-phase, incompressible flow;
//! * constant density and constant dynamic viscosity (laminar);
//! * a structured Cartesian mesh with staggered pressure and velocity;
//! * first-order explicit Euler or second-order Runge-Kutta time stepping with
//!   the Chorin projection method;
//! * solid rectangle and circle obstacles with drag, lift and moment
//!   coefficients and configurable reference quantities;
//! * uniform, parabolic and power-law inlet velocity profiles;
//! * passive scalar (temperature) transport with explicit advection-diffusion;
//! * isotropic porous regions with a Darcy–Forchheimer momentum sink;
//! * optional eddy-viscosity turbulence (constant or mixing-length);
//! * thermal side/obstacle boundaries, volumetric heat sources and obstacle
//!   Nusselt numbers;
//! * Boussinesq buoyancy coupling the temperature field back to the momentum
//!   equation;
//! * SVG previews of cell-centred fields, traced streamlines and filled
//!   contours with iso-lines;
//! * residual/continuity and obstacle force history CSV export;
//! * CSV and legacy VTK field export at cell centres or mesh nodes;
//! * boundary mass balance, sampled continuity residuals, vorticity, stream
//!   function and point probe time series;
//! * lid-driven cavity and Couette-like channel cases with wall, slip, inlet,
//!   pressure-outlet and convective-outlet sides.
//!
//! Compressibility, three-dimensional grids, adaptive meshing and external
//! format import are **not** implemented. See
//! [`CAPABILITIES_JSON`] for the machine-readable capability matrix and the
//! repository Readme for the user facing description.

#![deny(missing_docs)]

mod checkpoint;
mod error;
mod export;
mod fields;
mod mesh;
mod model;
mod session;
mod solver;

pub use crate::checkpoint::{Checkpoint, CheckpointMesh, CHECKPOINT_FORMAT, CHECKPOINT_VERSION};
pub use crate::error::CfdError;
pub use crate::export::{
    export_coefficients, export_contours, export_csv, export_csv_with_layout, export_forces,
    export_residuals, export_streamlines, export_svg, export_vtk, export_vtk_with_layout,
    write_atomic, ExportField, ExportLayout,
};
pub use crate::fields::{stream_function, vorticity};
pub use crate::mesh::Mesh;
pub use crate::model::{
    Boundaries, Boundary, BoundaryKind, Buoyancy, ConvectionScheme, Fluid, InitialField,
    InletProfile, InletProfiles, MeshConfig, Model, Obstacle, PorousRegion, Probe, Reference,
    ScalarConfig, Side, SideThermal, ThermalConfig, TimeScheme, TimeSettings, Turbulence,
    TurbulenceModel, Units, FORMAT, VERSION,
};
pub use crate::session::{
    CaseSummary, ExportFormat, ExportReport, OpenCfd, OpenCfdOptions, SaveReport,
};
pub use crate::solver::{
    solve, solve_pressure, solve_with, solve_with_checkpoint, transport_scalar, transport_thermal,
    ForceSample, MassBalance, ObstacleForce, ObstacleNusselt, PorousSummary, PressureSettings,
    PressureStats, ProbeSample, ProbeSeries, Report, ResidualSample, Solution, SolveOptions,
    WallShear, REPORT_FORMAT, REPORT_VERSION,
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
    "porousMedia": "supported",
    "scalar": "supported",
    "convergence": "supported",
    "vorticity": "supported",
    "streamFunction": "supported",
    "convectionSchemes": "supported",
    "timeSchemes": "supported",
    "adaptiveTimeStep": "supported",
    "probes": "supported",
    "wallShear": "supported",
    "slipBoundary": "supported",
    "pressureOutlet": "supported",
    "convectiveOutlet": "supported",
    "periodic": "supported",
    "pressureDriven": "supported",
    "inletProfiles": "supported",
    "momentCoefficients": "supported",
    "massBalance": "supported",
    "buoyancy": "supported",
    "compressible": "unsupported",
    "turbulence": "supported",
    "heat": "supported",
    "threeD": "unsupported"
  },
  "io": {
    "ocfd": "supported",
    "csv": "supported",
    "vtk": "supported",
    "nodeLayout": "supported",
    "svg": "supported",
    "streamlines": "supported",
    "history": "supported",
    "coefficients": "supported",
    "contours": "supported",
    "restart": "supported",
    "inp": "unsupported",
    "openfoam": "unsupported"
  }
}"#;
