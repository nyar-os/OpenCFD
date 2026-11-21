use std::fs;
use std::path::Path;
use std::str::FromStr;

use boost_check::{CheckReport, Issue};
use serde::{Deserialize, Serialize};

use crate::export;
use crate::CfdError;

/// Identifier of the document format stored in the `format` field.
pub const FORMAT: &str = "open-cfd-case";

/// Document version understood by this build.
pub const VERSION: u32 = 1;

/// Unit system declared by a document.
///
/// The solver does not convert units. Only the SI metre system is accepted:
/// lengths in `m`, density in `kg/m³`, dynamic viscosity in `Pa·s`, velocity in
/// `m/s`, time in `s` and pressure in `Pa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Units {
    /// Metre / kilogram / second / pascal system.
    #[serde(rename = "m")]
    #[default]
    Meter,
}

impl Units {
    /// Returns the short identifier stored in documents (`"m"`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Meter => "m",
        }
    }
}

impl FromStr for Units {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "m" => Ok(Self::Meter),
            other => Err(CfdError::new(
                "cli/invalid-units",
                format!("unsupported unit `{other}`; only `m` is supported"),
            )),
        }
    }
}

/// Structured mesh definition in physical coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshConfig {
    /// Number of cells in the x direction (at least 3).
    #[serde(default = "default_nx")]
    pub nx: usize,
    /// Number of cells in the y direction (at least 3).
    #[serde(default = "default_ny")]
    pub ny: usize,
    /// Physical domain length in x (m).
    #[serde(default = "default_length")]
    pub length_x: f64,
    /// Physical domain length in y (m).
    #[serde(default = "default_length")]
    pub length_y: f64,
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            nx: default_nx(),
            ny: default_ny(),
            length_x: 1.0,
            length_y: 1.0,
        }
    }
}

/// Constant-property Newtonian fluid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fluid {
    /// Density `rho` (kg/m³).
    #[serde(default = "default_density")]
    pub density: f64,
    /// Dynamic viscosity `mu` (Pa·s); zero means inviscid flow.
    #[serde(default = "default_viscosity")]
    pub viscosity: f64,
}

impl Default for Fluid {
    fn default() -> Self {
        Self {
            density: default_density(),
            viscosity: default_viscosity(),
        }
    }
}

/// Force and moment reference quantities.
///
/// The defaults are the maximum inlet speed, the obstacle characteristic length
/// times unit depth, and the obstacle characteristic length. Supplying a value
/// replaces only that quantity, so a case can match an experiment or a report
/// without changing the geometry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// Reference speed `U` (m/s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity: Option<f64>,
    /// Reference area `A` (m²) used by the force coefficients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<f64>,
    /// Reference length `L` (m) used by the moment coefficient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<f64>,
}

/// Time integration scheme of the explicit momentum step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeScheme {
    /// Explicit Euler: one first-order forward step per time step.
    #[default]
    Euler,
    /// Explicit second-order Runge-Kutta (Heun): two derivative evaluations
    /// per step followed by the pressure projection.
    Rk2,
}

impl TimeScheme {
    /// Returns the identifier used in documents, reports and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Euler => "euler",
            Self::Rk2 => "rk2",
        }
    }
}

impl FromStr for TimeScheme {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "euler" => Ok(Self::Euler),
            "rk2" | "heun" => Ok(Self::Rk2),
            other => Err(CfdError::new(
                "cli/invalid-time-scheme",
                format!("unknown time scheme `{other}`; only euler or rk2 is supported"),
            )),
        }
    }
}

/// Time integration settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSettings {
    /// Constant time step `dt` (s). With `adaptive` it is the upper bound.
    #[serde(default = "default_dt")]
    pub dt: f64,
    /// Number of time steps to advance.
    #[serde(default = "default_steps")]
    pub steps: usize,
    /// Maximum SOR sweeps for the pressure Poisson equation per time step.
    #[serde(default = "default_pressure_iterations")]
    pub pressure_iterations: usize,
    /// Relative pressure-residual tolerance that stops the SOR sweeps early.
    /// The solve converges when the residual is at most `tolerance` times its
    /// initial value.
    #[serde(default = "default_tolerance")]
    pub tolerance: f64,
    /// Explicit time integration scheme of the momentum equation.
    #[serde(default)]
    pub scheme: TimeScheme,
    /// Reduce every step to satisfy the explicit stability limits.
    #[serde(default)]
    pub adaptive: bool,
    /// Target convective CFL number of an adaptive step, in `(0, 1]`.
    #[serde(default = "default_target_cfl")]
    pub target_cfl: f64,
}

impl Default for TimeSettings {
    fn default() -> Self {
        Self {
            dt: default_dt(),
            steps: default_steps(),
            pressure_iterations: default_pressure_iterations(),
            tolerance: default_tolerance(),
            scheme: TimeScheme::default(),
            adaptive: false,
            target_cfl: default_target_cfl(),
        }
    }
}

/// Convective discretisation of the momentum and scalar equations.
///
/// `Upwind` is the first-order default and the most diffusive. `Central` is
/// second-order and can oscillate when the cell Péclet number exceeds 2.
/// `Quick` is the third-order quadratic upstream interpolation, which is
/// slightly sharper than upwind while staying less oscillatory than central.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConvectionScheme {
    /// First-order upwind differencing.
    #[default]
    Upwind,
    /// Second-order central differencing.
    Central,
    /// Third-order quadratic upstream interpolation (QUICK).
    Quick,
}

impl ConvectionScheme {
    /// Returns the identifier used in documents, reports and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upwind => "upwind",
            Self::Central => "central",
            Self::Quick => "quick",
        }
    }
}

impl FromStr for ConvectionScheme {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "upwind" => Ok(Self::Upwind),
            "central" => Ok(Self::Central),
            "quick" => Ok(Self::Quick),
            other => Err(CfdError::new(
                "cli/invalid-convection-scheme",
                format!(
                    "unknown convection scheme `{other}`; only upwind, central or quick is supported"
                ),
            )),
        }
    }
}

/// Kind of a domain side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryKind {
    /// No-slip wall; a non-zero velocity makes it a moving wall.
    #[default]
    Wall,
    /// Prescribed inflow velocity.
    Inlet,
    /// Zero-gradient velocity and a prescribed reference pressure.
    Outlet,
    /// Free-slip wall: zero normal velocity and zero tangential gradient.
    Slip,
}

impl BoundaryKind {
    /// Returns the lowercase identifier used in documents.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Inlet => "inlet",
            Self::Outlet => "outlet",
            Self::Slip => "slip",
        }
    }
}

impl FromStr for BoundaryKind {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "wall" => Ok(Self::Wall),
            "inlet" => Ok(Self::Inlet),
            "outlet" => Ok(Self::Outlet),
            "slip" | "freeSlip" | "free-slip" | "free_slip" => Ok(Self::Slip),
            other => Err(CfdError::new(
                "cli/invalid-boundary-kind",
                format!(
                    "unknown boundary kind `{other}`; only wall, inlet, outlet or slip is supported"
                ),
            )),
        }
    }
}

/// Shape of the normal velocity across an inlet side.
///
/// The profile multiplies the normal component of [`Boundary::velocity`] and
/// leaves the tangential component unchanged. It is ignored by walls, outlets
/// and slip sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InletProfile {
    /// Uniform inflow at the prescribed velocity.
    #[default]
    Uniform,
    /// Poiseuille parabola: zero at both ends and the prescribed velocity at
    /// the middle of the side.
    Parabolic,
    /// Symmetric power law `(1 − |2s − 1|)^(1/n)` with `s ∈ [0, 1]`, zero at
    /// both ends and the prescribed velocity at the middle.
    PowerLaw,
}

impl InletProfile {
    /// Returns the identifier used in documents, reports and the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Parabolic => "parabolic",
            Self::PowerLaw => "powerLaw",
        }
    }
}

impl FromStr for InletProfile {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "uniform" => Ok(Self::Uniform),
            "parabolic" => Ok(Self::Parabolic),
            "powerLaw" | "power-law" | "power_law" => Ok(Self::PowerLaw),
            other => Err(CfdError::new(
                "cli/invalid-inlet-profile",
                format!(
                    "unknown inlet profile `{other}`; only uniform, parabolic or powerLaw is supported"
                ),
            )),
        }
    }
}

/// Boundary condition of one side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boundary {
    /// Boundary kind.
    #[serde(default)]
    pub kind: BoundaryKind,
    /// Prescribed velocity `[u, v]` (m/s). Used by walls and inlets; ignored by
    /// outlets, which use zero-gradient velocity, and by slip walls, which
    /// always carry zero normal velocity.
    #[serde(default)]
    pub velocity: [f64; 2],
    /// Gauge pressure prescribed on an outlet side (Pa). Ignored by other
    /// boundary kinds.
    #[serde(default)]
    pub pressure: f64,
}

impl Boundary {
    /// A stationary no-slip wall.
    pub fn wall() -> Self {
        Self {
            kind: BoundaryKind::Wall,
            velocity: [0.0, 0.0],
            pressure: 0.0,
        }
    }
}

/// Normal-velocity shape of the four inlet sides.
///
/// A profile multiplies the normal component of the side velocity and leaves
/// the tangential component unchanged. It only takes effect on an `inlet` side;
/// walls, outlets and slip sides ignore it. `exponent` is the `n` of
/// [`InletProfile::PowerLaw`] and is shared by every side.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InletProfiles {
    /// Left side (`x = 0`) profile.
    #[serde(default)]
    pub left: InletProfile,
    /// Right side (`x = lengthX`) profile.
    #[serde(default)]
    pub right: InletProfile,
    /// Top side (`y = lengthY`) profile.
    #[serde(default)]
    pub top: InletProfile,
    /// Bottom side (`y = 0`) profile.
    #[serde(default)]
    pub bottom: InletProfile,
    /// Power-law exponent `n`; must be finite and positive.
    #[serde(default = "default_profile_exponent")]
    pub exponent: f64,
}

impl Default for InletProfiles {
    fn default() -> Self {
        Self {
            left: InletProfile::Uniform,
            right: InletProfile::Uniform,
            top: InletProfile::Uniform,
            bottom: InletProfile::Uniform,
            exponent: default_profile_exponent(),
        }
    }
}

impl InletProfiles {
    /// Profile of one side.
    pub fn get(&self, side: Side) -> InletProfile {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
            Side::Top => self.top,
            Side::Bottom => self.bottom,
        }
    }

    /// Mutable profile of one side.
    pub fn get_mut(&mut self, side: Side) -> &mut InletProfile {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
            Side::Top => &mut self.top,
            Side::Bottom => &mut self.bottom,
        }
    }

    /// Scale factor of the normal velocity at the normalized position
    /// `fraction ∈ [0, 1]` along `side`.
    ///
    /// `Uniform` gives `1`, `Parabolic` gives `4s(1−s)` and `PowerLaw` gives
    /// `(1−|2s−1|)^(1/n)`, so both shaped profiles vanish at the two ends and
    /// reach `1` at the middle.
    pub fn scale(&self, side: Side, fraction: f64) -> f64 {
        let s = fraction.clamp(0.0, 1.0);
        match self.get(side) {
            InletProfile::Uniform => 1.0,
            InletProfile::Parabolic => 4.0 * s * (1.0 - s),
            InletProfile::PowerLaw => (1.0 - (2.0 * s - 1.0).abs()).powf(1.0 / self.exponent),
        }
    }
}

impl Default for Boundary {
    fn default() -> Self {
        Self::wall()
    }
}

/// Boundary conditions of the four sides of the rectangular domain.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boundaries {
    /// Left side (`x = 0`).
    #[serde(default)]
    pub left: Boundary,
    /// Right side (`x = lengthX`).
    #[serde(default)]
    pub right: Boundary,
    /// Top side (`y = lengthY`).
    #[serde(default)]
    pub top: Boundary,
    /// Bottom side (`y = 0`).
    #[serde(default)]
    pub bottom: Boundary,
    /// Normal-velocity shapes of the four inlet sides.
    #[serde(default)]
    pub inlet_profiles: InletProfiles,
}

/// Names of the four domain sides, used by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Left side (`x = 0`).
    Left,
    /// Right side (`x = lengthX`).
    Right,
    /// Top side (`y = lengthY`).
    Top,
    /// Bottom side (`y = 0`).
    Bottom,
}

impl Side {
    /// Returns the lowercase identifier used by the CLI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

impl FromStr for Side {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            other => Err(CfdError::new(
                "cli/invalid-side",
                format!("unknown side `{other}`; only left, right, top or bottom is supported"),
            )),
        }
    }
}

impl Boundaries {
    /// Returns the boundary condition of one side.
    pub fn get(&self, side: Side) -> &Boundary {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
            Side::Top => &self.top,
            Side::Bottom => &self.bottom,
        }
    }

    /// Returns a mutable boundary condition of one side.
    pub fn get_mut(&mut self, side: Side) -> &mut Boundary {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
            Side::Top => &mut self.top,
            Side::Bottom => &mut self.bottom,
        }
    }
}

/// Optional uniform initial velocity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitialField {
    /// Uniform initial velocity `[u, v]` (m/s).
    #[serde(default)]
    pub velocity: [f64; 2],
}

/// Solid obstacle embedded in the fluid domain.
///
/// A cell is solid when its centre lies inside the obstacle. Obstacles are
/// listed and applied in document order; when two obstacles overlap, the first
/// one owns the cell, which keeps cell assignment and force reporting stable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Obstacle {
    /// Axis-aligned rectangle spanned by `(x0, y0)` and `(x1, y1)` (m).
    Rect {
        /// Stable identifier used by force reports; empty means `obstacle-N`.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        id: String,
        /// Lower x coordinate (m).
        x0: f64,
        /// Lower y coordinate (m).
        y0: f64,
        /// Upper x coordinate (m).
        x1: f64,
        /// Upper y coordinate (m).
        y1: f64,
    },
    /// Circle with centre `(x, y)` and radius `r` (m).
    Circle {
        /// Stable identifier used by force reports; empty means `obstacle-N`.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        id: String,
        /// Centre x coordinate (m).
        x: f64,
        /// Centre y coordinate (m).
        y: f64,
        /// Radius (m); must be positive.
        r: f64,
    },
}

impl Obstacle {
    /// Stable identifier, or an empty string when the document omitted it.
    pub fn id(&self) -> &str {
        match self {
            Self::Rect { id, .. } | Self::Circle { id, .. } => id,
        }
    }

    /// Returns a copy of this obstacle carrying `id`.
    pub fn with_id(self, id: String) -> Self {
        match self {
            Self::Rect { x0, y0, x1, y1, .. } => Self::Rect { id, x0, y0, x1, y1 },
            Self::Circle { x, y, r, .. } => Self::Circle { id, x, y, r },
        }
    }

    /// Lowercase kind identifier (`"rect"` or `"circle"`).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Rect { .. } => "rect",
            Self::Circle { .. } => "circle",
        }
    }

    /// Characteristic length `D` (m): rectangle height or circle diameter.
    pub fn characteristic_length(&self) -> f64 {
        match self {
            Self::Rect { y0, y1, .. } => y1 - y0,
            Self::Circle { r, .. } => 2.0 * r,
        }
    }

    /// Geometric centroid `(x, y)` (m), used as the moment pivot.
    pub fn centroid(&self) -> (f64, f64) {
        match self {
            Self::Rect { x0, y0, x1, y1, .. } => (0.5 * (x0 + x1), 0.5 * (y0 + y1)),
            Self::Circle { x, y, .. } => (*x, *y),
        }
    }

    /// Whether the point `(x, y)` lies inside the obstacle (edges included).
    pub fn contains(&self, x: f64, y: f64) -> bool {
        match self {
            Self::Rect { x0, y0, x1, y1, .. } => x >= *x0 && x <= *x1 && y >= *y0 && y <= *y1,
            Self::Circle {
                x: cx, y: cy, r, ..
            } => {
                let dx = x - cx;
                let dy = y - cy;
                dx * dx + dy * dy <= r * r
            }
        }
    }

    /// Checks positive size and that the obstacle lies inside the domain.
    pub fn validate(&self, length_x: f64, length_y: f64) -> Result<(), CfdError> {
        match self {
            Self::Rect { x0, y0, x1, y1, .. } => {
                if ![x0, y0, x1, y1].iter().all(|value| value.is_finite()) {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        "rectangle obstacle coordinates must be finite",
                    ));
                }
                if x1 <= x0 || y1 <= y0 {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        "rectangle obstacle must satisfy x1 > x0 and y1 > y0",
                    ));
                }
                if *x0 < 0.0 || *y0 < 0.0 || *x1 > length_x || *y1 > length_y {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        format!(
                            "rectangle obstacle ({x0}, {y0})-({x1}, {y1}) lies outside the {}×{} m domain",
                            length_x, length_y
                        ),
                    ));
                }
            }
            Self::Circle { x, y, r, .. } => {
                if ![x, y, r].iter().all(|value| value.is_finite()) {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        "circle obstacle coordinates and radius must be finite",
                    ));
                }
                if *r <= 0.0 {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        "circle obstacle radius must be greater than 0",
                    ));
                }
                if x - r < 0.0 || y - r < 0.0 || x + r > length_x || y + r > length_y {
                    return Err(CfdError::new(
                        "obstacle/invalid",
                        format!(
                            "circle obstacle (x={x}, y={y}, r={r}) lies outside the {}×{} m domain",
                            length_x, length_y
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Point monitor sampled during the solve.
///
/// The probe is bound to the cell whose centre is nearest to `(x, y)`. It must
/// lie inside the domain and outside every obstacle, so the recorded value
/// always belongs to a fluid cell. When `id` is empty the report names the
/// probe `probe-N` by its one-based document position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// Stable identifier used by probe reports; empty means `probe-N`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    /// Probe x coordinate (m).
    pub x: f64,
    /// Probe y coordinate (m).
    pub y: f64,
}

impl Probe {
    /// Stable identifier, or an empty string when the document omitted it.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns a copy of this probe carrying `id`.
    pub fn with_id(self, id: String) -> Self {
        Self { id, ..self }
    }

    /// Checks that the probe lies inside the domain and outside every obstacle.
    pub fn validate(
        &self,
        length_x: f64,
        length_y: f64,
        obstacles: &[Obstacle],
    ) -> Result<(), CfdError> {
        if !self.x.is_finite() || !self.y.is_finite() {
            return Err(CfdError::new(
                "probe/invalid",
                "probe coordinates must be finite",
            ));
        }
        if self.x < 0.0 || self.y < 0.0 || self.x > length_x || self.y > length_y {
            return Err(CfdError::new(
                "probe/invalid",
                format!(
                    "probe ({}, {}) lies outside the {}×{} m domain",
                    self.x, self.y, length_x, length_y
                ),
            ));
        }
        if obstacles
            .iter()
            .any(|obstacle| obstacle.contains(self.x, self.y))
        {
            return Err(CfdError::new(
                "probe/invalid",
                format!(
                    "probe ({}, {}) lies inside an obstacle; move it into the fluid",
                    self.x, self.y
                ),
            ));
        }
        Ok(())
    }
}

/// Passive scalar (temperature) transport settings.
///
/// The scalar is advected with the computed velocity and diffuses with a
/// constant diffusivity. Solid cells carry no scalar value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalarConfig {
    /// Constant diffusivity `κ` (m²/s); zero means pure advection.
    #[serde(default)]
    pub diffusivity: f64,
    /// Uniform initial value in every fluid cell.
    #[serde(default)]
    pub initial: f64,
    /// Value prescribed on inlet faces.
    #[serde(default)]
    pub inlet_value: f64,
}

impl Default for ScalarConfig {
    fn default() -> Self {
        Self {
            diffusivity: 0.0,
            initial: 0.0,
            inlet_value: 0.0,
        }
    }
}

/// Eddy-viscosity turbulence model selected by a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurbulenceModel {
    /// Laminar flow; no eddy viscosity.
    #[default]
    None,
    /// Uniform, user-provided eddy viscosity `nuT`.
    Constant,
    /// Algebraic mixing-length model `nuT = l² · |S|`.
    MixingLength,
}

impl TurbulenceModel {
    /// Returns the identifier used in documents and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Constant => "constant",
            Self::MixingLength => "mixingLength",
        }
    }
}

impl FromStr for TurbulenceModel {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "none" | "off" => Ok(Self::None),
            "constant" => Ok(Self::Constant),
            "mixingLength" | "mixing-length" | "mixing_length" => Ok(Self::MixingLength),
            other => Err(CfdError::new(
                "cli/invalid-turbulence-model",
                format!("unknown turbulence model `{other}`; only none, constant or mixingLength is supported"),
            )),
        }
    }
}

/// Eddy-viscosity turbulence settings.
///
/// The constant model applies `nu_t` everywhere. The mixing-length model
/// evaluates `nu_t = l² · |S|` per cell, where `|S|` is the magnitude of the
/// resolved strain-rate tensor and `l` is `mixingLength`. Both are
/// deterministic functions of the current velocity field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turbulence {
    /// Selected model.
    #[serde(default)]
    pub model: TurbulenceModel,
    /// Eddy viscosity `nu_t` (m²/s) used by [`TurbulenceModel::Constant`].
    #[serde(default)]
    pub nu_t: f64,
    /// Mixing length `l` (m) used by [`TurbulenceModel::MixingLength`].
    #[serde(default = "default_mixing_length")]
    pub mixing_length: f64,
}

impl Default for Turbulence {
    fn default() -> Self {
        Self {
            model: TurbulenceModel::None,
            nu_t: 0.0,
            mixing_length: default_mixing_length(),
        }
    }
}

/// Thermal condition of one domain side.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SideThermal {
    /// Fixed wall temperature (K). Takes precedence over `heatFlux`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Fixed heat flux into the fluid (W/m²).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heat_flux: Option<f64>,
}

/// Energy-equation settings layered on top of the passive scalar transport.
///
/// The scalar field carries temperature. `conductivity` is the thermal
/// conductivity `k` (W/(m·K)); when omitted it is derived as
/// `rho · cp · diffusivity`. `heatSource` is a uniform volumetric source
/// (W/m³). Domain sides accept fixed temperature or fixed heat flux, and all
/// obstacle surfaces share the same `obstacleTemperature`/`obstacleHeatFlux`
/// condition. A Nusselt number is reported per obstacle using
/// `Nu = q̅ · D / (k · ΔT)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThermalConfig {
    /// Specific heat capacity `cp` (J/(kg·K)).
    #[serde(default = "default_specific_heat")]
    pub specific_heat: f64,
    /// Thermal conductivity `k` (W/(m·K)); omitted derives `rho·cp·κ`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductivity: Option<f64>,
    /// Uniform volumetric heat source (W/m³).
    #[serde(default)]
    pub heat_source: f64,
    /// Left side (`x = 0`) thermal condition.
    #[serde(default)]
    pub left: SideThermal,
    /// Right side (`x = lengthX`) thermal condition.
    #[serde(default)]
    pub right: SideThermal,
    /// Top side (`y = lengthY`) thermal condition.
    #[serde(default)]
    pub top: SideThermal,
    /// Bottom side (`y = 0`) thermal condition.
    #[serde(default)]
    pub bottom: SideThermal,
    /// Fixed obstacle surface temperature (K); all obstacles share it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obstacle_temperature: Option<f64>,
    /// Fixed heat flux from every obstacle surface into the fluid (W/m²).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obstacle_heat_flux: Option<f64>,
    /// Reference temperature for the Nusselt number (K); omitted uses the mean
    /// fluid temperature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_temperature: Option<f64>,
}

impl Default for ThermalConfig {
    fn default() -> Self {
        Self {
            specific_heat: default_specific_heat(),
            conductivity: None,
            heat_source: 0.0,
            left: SideThermal::default(),
            right: SideThermal::default(),
            top: SideThermal::default(),
            bottom: SideThermal::default(),
            obstacle_temperature: None,
            obstacle_heat_flux: None,
            reference_temperature: None,
        }
    }
}

impl ThermalConfig {
    /// Returns the thermal condition of one side.
    pub fn side(&self, side: Side) -> &SideThermal {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
            Side::Top => &self.top,
            Side::Bottom => &self.bottom,
        }
    }
}

/// Boussinesq buoyancy settings.
///
/// Declared for documentation completeness only. This build rejects a document
/// that enables buoyancy with `capability/not-implemented`; no unstable path is
/// shipped silently.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Buoyancy {
    /// Thermal expansion coefficient `β` (1/K).
    #[serde(default)]
    pub beta: f64,
    /// Reference temperature `T_ref` (K).
    #[serde(default)]
    pub t_ref: f64,
    /// Gravity vector `[gx, gy]` (m/s²).
    #[serde(default)]
    pub gravity: [f64; 2],
}

/// In-memory representation of an `.ocfd` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    /// Document format tag; must equal [`FORMAT`].
    #[serde(default = "default_format")]
    pub format: String,
    /// Document version; this build understands [`VERSION`].
    #[serde(default = "default_version")]
    pub version: u32,
    /// Declared unit system.
    #[serde(default)]
    pub units: Units,
    /// Structured mesh.
    #[serde(default)]
    pub mesh: MeshConfig,
    /// Fluid properties.
    #[serde(default)]
    pub fluid: Fluid,
    /// Optional force and moment reference quantities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    /// Convective discretisation of the momentum and scalar equations.
    #[serde(default)]
    pub convection: ConvectionScheme,
    /// Time integration settings.
    #[serde(default)]
    pub time: TimeSettings,
    /// Boundary conditions.
    #[serde(default)]
    pub boundaries: Boundaries,
    /// Optional uniform initial velocity; missing means fluid at rest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<InitialField>,
    /// Solid obstacles, applied in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub obstacles: Vec<Obstacle>,
    /// Point monitors sampled during the solve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub probes: Vec<Probe>,
    /// Optional passive scalar (temperature) transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<ScalarConfig>,
    /// Optional eddy-viscosity turbulence model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turbulence: Option<Turbulence>,
    /// Optional energy-equation settings (thermal boundaries and source).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal: Option<ThermalConfig>,
    /// Optional Boussinesq buoyancy; declared but not supported by this build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buoyancy: Option<Buoyancy>,
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    /// Creates a lid-driven cavity case: a 32×32 unit square, water-like
    /// viscosity `0.01 Pa·s`, density `1 kg/m³`, `dt = 0.01 s`, 400 steps,
    /// 100 pressure sweeps with a `1e-3` relative tolerance, and a `1 m/s` lid
    /// on the top wall.
    pub fn new() -> Self {
        let mut boundaries = Boundaries::default();
        boundaries.top.velocity = [1.0, 0.0];
        Self {
            format: FORMAT.to_string(),
            version: VERSION,
            units: Units::Meter,
            mesh: MeshConfig::default(),
            fluid: Fluid::default(),
            reference: None,
            convection: ConvectionScheme::default(),
            time: TimeSettings::default(),
            boundaries,
            initial: None,
            obstacles: Vec::new(),
            probes: Vec::new(),
            scalar: None,
            turbulence: None,
            thermal: None,
            buoyancy: None,
        }
    }

    /// Parses a document from JSON text.
    pub fn parse_json(text: &str) -> Result<Self, CfdError> {
        serde_json::from_str(text).map_err(|error| {
            CfdError::new(
                "document/parse",
                format!("cannot parse .ocfd JSON: {error}"),
            )
        })
    }

    /// Serialises the document as pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, CfdError> {
        serde_json::to_string_pretty(self).map_err(|error| {
            CfdError::new(
                "document/serialize",
                format!("cannot serialize the document: {error}"),
            )
        })
    }

    /// Reads and parses an `.ocfd` file.
    pub fn load(path: &Path) -> Result<Self, CfdError> {
        let text = fs::read_to_string(path).map_err(|error| {
            CfdError::new(
                "io/read",
                format!("cannot read `{}`: {error}", path.display()),
            )
        })?;
        Self::parse_json(&text)
    }

    /// Serialises and atomically writes an `.ocfd` file.
    pub fn save_atomic(&self, path: &Path) -> Result<(), CfdError> {
        let text = self.to_json_pretty()?;
        export::write_atomic(path, &text)
    }

    /// Checks that the document can be solved by this build.
    ///
    /// Returns the first structural problem found. Numeric stability of the
    /// explicit time step is checked separately by the solver, because it
    /// depends on the initial and boundary velocities.
    pub fn validate(&self) -> Result<(), CfdError> {
        let report = self.validation_report();
        if let Some(issue) = report.issues.first() {
            return Err(CfdError::new(issue.code.clone(), issue.message.clone()));
        }
        Ok(())
    }

    /// Collects every structural problem in document order.
    ///
    /// [`Self::validate`] keeps returning the first issue so the stable error
    /// code and JSON shape stay unchanged.
    fn validation_report(&self) -> CheckReport {
        let mut report = CheckReport::new();
        if self.format != FORMAT {
            report.push(Issue::error(
                "document/format",
                format!("document format is `{}`, expected `{FORMAT}`", self.format),
            ));
        }
        if self.version != VERSION {
            report.push(Issue::error(
                "document/version",
                format!(
                    "document version is {}, but this build only supports {VERSION}",
                    self.version
                ),
            ));
        }
        if self.mesh.nx < 3 || self.mesh.ny < 3 {
            report.push(Issue::error(
                "mesh/invalid",
                format!(
                    "mesh nx and ny must be at least 3 (currently {}×{}), otherwise the staggered mesh has no interior faces",
                    self.mesh.nx, self.mesh.ny
                ),
            ));
        }
        if !finite_positive(self.mesh.length_x) || !finite_positive(self.mesh.length_y) {
            report.push(Issue::error(
                "mesh/invalid",
                "lengthX and lengthY must be finite positive numbers",
            ));
        }
        if !finite_positive(self.fluid.density) {
            report.push(Issue::error(
                "fluid/invalid",
                "density must be a finite positive number",
            ));
        }
        if !self.fluid.viscosity.is_finite() || self.fluid.viscosity < 0.0 {
            report.push(Issue::error(
                "fluid/invalid",
                "viscosity must be a finite non-negative number",
            ));
        }
        if let Some(reference) = &self.reference {
            for (name, value) in [
                ("velocity", reference.velocity),
                ("area", reference.area),
                ("length", reference.length),
            ] {
                if value.is_some_and(|value| !finite_positive(value)) {
                    report.push(Issue::error(
                        "reference/invalid",
                        format!("reference.{name} must be a finite positive number"),
                    ));
                }
            }
        }
        if !finite_positive(self.time.dt) {
            report.push(Issue::error(
                "time/invalid",
                "dt must be a finite positive number",
            ));
        }
        if self.time.steps == 0 {
            report.push(Issue::error("time/invalid", "steps must be greater than 0"));
        }
        if self.time.pressure_iterations == 0 {
            report.push(Issue::error(
                "time/invalid",
                "pressureIterations must be greater than 0",
            ));
        }
        if !finite_positive(self.time.tolerance) {
            report.push(Issue::error(
                "time/invalid",
                "tolerance must be a finite positive number",
            ));
        }
        if !self.time.target_cfl.is_finite()
            || self.time.target_cfl <= 0.0
            || self.time.target_cfl > 1.0
        {
            report.push(Issue::error(
                "time/invalid",
                "time.targetCfl must be a finite number in (0, 1]",
            ));
        }
        for (name, boundary) in [
            ("left", &self.boundaries.left),
            ("right", &self.boundaries.right),
            ("top", &self.boundaries.top),
            ("bottom", &self.boundaries.bottom),
        ] {
            if !boundary.velocity.iter().all(|value| value.is_finite()) {
                report.push(Issue::error(
                    "boundary/invalid",
                    format!("velocity on the {name} side must be finite"),
                ));
            }
            if !boundary.pressure.is_finite() {
                report.push(Issue::error(
                    "boundary/invalid",
                    format!("outlet pressure on the {name} side must be finite"),
                ));
            }
        }
        if !finite_positive(self.boundaries.inlet_profiles.exponent) {
            report.push(Issue::error(
                "boundary/invalid",
                "boundaries.inletProfiles.exponent must be a finite positive number",
            ));
        }
        if let Some(initial) = &self.initial {
            if !initial.velocity.iter().all(|value| value.is_finite()) {
                report.push(Issue::error(
                    "initial/invalid",
                    "initial.velocity must be finite",
                ));
            }
        }
        for obstacle in &self.obstacles {
            if let Err(error) = obstacle.validate(self.mesh.length_x, self.mesh.length_y) {
                report.push(Issue::error(error.code, error.message));
            }
        }
        for probe in &self.probes {
            if let Err(error) =
                probe.validate(self.mesh.length_x, self.mesh.length_y, &self.obstacles)
            {
                report.push(Issue::error(error.code, error.message));
            }
        }
        if let Some(scalar) = &self.scalar {
            if !scalar.diffusivity.is_finite() || scalar.diffusivity < 0.0 {
                report.push(Issue::error(
                    "scalar/invalid",
                    "scalar.diffusivity must be a finite non-negative number",
                ));
            }
            if !scalar.initial.is_finite() || !scalar.inlet_value.is_finite() {
                report.push(Issue::error(
                    "scalar/invalid",
                    "scalar.initial and scalar.inletValue must be finite",
                ));
            }
        }
        if let Some(turbulence) = &self.turbulence {
            if !turbulence.nu_t.is_finite() || turbulence.nu_t < 0.0 {
                report.push(Issue::error(
                    "turbulence/invalid",
                    "turbulence.nuT must be a finite non-negative number",
                ));
            }
            if !finite_positive(turbulence.mixing_length) {
                report.push(Issue::error(
                    "turbulence/invalid",
                    "turbulence.mixingLength must be a finite positive number",
                ));
            }
        }
        if let Some(thermal) = &self.thermal {
            if self.scalar.is_none() {
                report.push(Issue::error(
                    "thermal/invalid",
                    "thermal requires scalar (temperature field) transport to be configured",
                ));
            }
            if !finite_positive(thermal.specific_heat) {
                report.push(Issue::error(
                    "thermal/invalid",
                    "thermal.specificHeat must be a finite positive number",
                ));
            }
            if let Some(conductivity) = thermal.conductivity {
                if !finite_positive(conductivity) {
                    report.push(Issue::error(
                        "thermal/invalid",
                        "thermal.conductivity must be a finite positive number",
                    ));
                }
            }
            if !thermal.heat_source.is_finite() {
                report.push(Issue::error(
                    "thermal/invalid",
                    "thermal.heatSource must be finite",
                ));
            }
            for (name, side) in [
                ("left", &thermal.left),
                ("right", &thermal.right),
                ("top", &thermal.top),
                ("bottom", &thermal.bottom),
            ] {
                if side.temperature.is_some_and(|value| !value.is_finite()) {
                    report.push(Issue::error(
                        "thermal/invalid",
                        format!("temperature on the {name} side must be finite"),
                    ));
                }
                if side.heat_flux.is_some_and(|value| !value.is_finite()) {
                    report.push(Issue::error(
                        "thermal/invalid",
                        format!("heatFlux on the {name} side must be finite"),
                    ));
                }
            }
            for (name, value) in [
                ("obstacleTemperature", thermal.obstacle_temperature),
                ("obstacleHeatFlux", thermal.obstacle_heat_flux),
                ("referenceTemperature", thermal.reference_temperature),
            ] {
                if value.is_some_and(|value| !value.is_finite()) {
                    report.push(Issue::error(
                        "thermal/invalid",
                        format!("thermal.{name} must be finite"),
                    ));
                }
            }
        }
        if self.buoyancy.is_some() {
            report.push(Issue::error(
                "capability/not-implemented",
                "Boussinesq buoyancy has not passed stability validation, so this build does not support buoyancy; remove the field from the document",
            ));
        }
        report
    }
}

fn finite_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn default_format() -> String {
    FORMAT.to_string()
}

fn default_version() -> u32 {
    VERSION
}

fn default_nx() -> usize {
    32
}

fn default_ny() -> usize {
    32
}

fn default_length() -> f64 {
    1.0
}

fn default_density() -> f64 {
    1.0
}

fn default_viscosity() -> f64 {
    0.01
}

fn default_dt() -> f64 {
    0.01
}

fn default_steps() -> usize {
    400
}

fn default_pressure_iterations() -> usize {
    100
}

fn default_tolerance() -> f64 {
    1e-3
}

fn default_target_cfl() -> f64 {
    0.9
}

fn default_mixing_length() -> f64 {
    0.1
}

fn default_specific_heat() -> f64 {
    1.0
}

fn default_profile_exponent() -> f64 {
    7.0
}
