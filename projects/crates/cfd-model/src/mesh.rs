use crate::model::{Boundaries, Boundary, BoundaryKind, MeshConfig, Obstacle, Side};
use crate::CfdError;

/// A structured, staggered (MAC) mesh over a rectangle.
///
/// Pressure lives at the centre of every `nx × ny` cell. The x velocity `u`
/// lives on the `(nx + 1) × ny` vertical faces and the y velocity `v` on the
/// `nx × (ny + 1)` horizontal faces. Cell sizes are uniform:
/// `dx = lengthX / nx` and `dy = lengthY / ny`.
///
/// After [`Mesh::apply_obstacles`] every cell also carries a solid flag. A cell
/// is solid when its centre is inside an obstacle; solid cells block every
/// adjacent face and are excluded from the pressure unknowns.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// Number of cells in x.
    pub nx: usize,
    /// Number of cells in y.
    pub ny: usize,
    /// Cell size in x (m).
    pub dx: f64,
    /// Cell size in y (m).
    pub dy: f64,
    /// Physical domain length in x (m).
    pub length_x: f64,
    /// Physical domain length in y (m).
    pub length_y: f64,
    /// Solid flag per cell (`true` means impermeable).
    pub solid: Vec<bool>,
    /// Obstacle index per cell; `-1` marks a fluid cell.
    pub solid_id: Vec<i32>,
}

impl Mesh {
    /// Builds a mesh from document settings, rejecting degenerate sizes.
    pub fn new(config: &MeshConfig) -> Result<Self, CfdError> {
        if config.nx < 3 || config.ny < 3 {
            return Err(CfdError::new(
                "mesh/invalid",
                format!(
                    "mesh nx and ny must be at least 3 (currently {}×{})",
                    config.nx, config.ny
                ),
            ));
        }
        if !config.length_x.is_finite()
            || config.length_x <= 0.0
            || !config.length_y.is_finite()
            || config.length_y <= 0.0
        {
            return Err(CfdError::new(
                "mesh/invalid",
                "lengthX and lengthY must be finite positive numbers",
            ));
        }
        let cells = config.nx * config.ny;
        Ok(Self {
            nx: config.nx,
            ny: config.ny,
            dx: config.length_x / config.nx as f64,
            dy: config.length_y / config.ny as f64,
            length_x: config.length_x,
            length_y: config.length_y,
            solid: vec![false; cells],
            solid_id: vec![-1; cells],
        })
    }

    /// Marks cells whose centre lies inside an obstacle.
    ///
    /// The first obstacle in document order owns a cell when obstacles overlap,
    /// so `solid_id` and force accumulation are stable across runs.
    pub fn apply_obstacles(&mut self, obstacles: &[Obstacle]) -> Result<(), CfdError> {
        self.solid.fill(false);
        self.solid_id.fill(-1);
        for (index, obstacle) in obstacles.iter().enumerate() {
            obstacle.validate(self.length_x, self.length_y)?;
            for j in 0..self.ny {
                for i in 0..self.nx {
                    let (x, y) = self.cell_center(i, j);
                    if obstacle.contains(x, y) {
                        let cell = self.pressure_index(i, j);
                        if !self.solid[cell] {
                            self.solid[cell] = true;
                            self.solid_id[cell] = index as i32;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether the cell index is solid.
    pub fn is_solid_index(&self, index: usize) -> bool {
        self.solid.get(index).copied().unwrap_or(false)
    }

    /// Whether cell `(i, j)` is solid.
    pub fn is_solid(&self, i: usize, j: usize) -> bool {
        self.solid[self.pressure_index(i, j)]
    }

    /// Whether the x-face `(i, j)` is open to flow.
    ///
    /// A face adjacent to a solid cell, or between two solid cells, is closed.
    /// Domain boundary faces only have one adjacent cell.
    pub fn u_face_open(&self, i: usize, j: usize) -> bool {
        let left = i > 0 && self.is_solid(i - 1, j);
        let right = i < self.nx && self.is_solid(i, j);
        !(left || right)
    }

    /// Whether the y-face `(i, j)` is open to flow.
    pub fn v_face_open(&self, i: usize, j: usize) -> bool {
        let below = j > 0 && self.is_solid(i, j - 1);
        let above = j < self.ny && self.is_solid(i, j);
        !(below || above)
    }

    /// Number of pressure values (`nx * ny`).
    pub fn pressure_count(&self) -> usize {
        self.nx * self.ny
    }

    /// Number of x-face velocities (`(nx + 1) * ny`).
    pub fn u_count(&self) -> usize {
        (self.nx + 1) * self.ny
    }

    /// Number of y-face velocities (`nx * (ny + 1)`).
    pub fn v_count(&self) -> usize {
        self.nx * (self.ny + 1)
    }

    /// Linear index of the pressure at cell `(i, j)`.
    pub fn pressure_index(&self, i: usize, j: usize) -> usize {
        j * self.nx + i
    }

    /// Linear index of the x-face velocity at `(i, j)`.
    pub fn u_index(&self, i: usize, j: usize) -> usize {
        j * (self.nx + 1) + i
    }

    /// Linear index of the y-face velocity at `(i, j)`.
    pub fn v_index(&self, i: usize, j: usize) -> usize {
        j * self.nx + i
    }

    /// Coordinates of pressure cell centre `(i, j)`.
    pub fn cell_center(&self, i: usize, j: usize) -> (f64, f64) {
        ((i as f64 + 0.5) * self.dx, (j as f64 + 0.5) * self.dy)
    }

    /// Coordinates of x-face `(i, j)`.
    pub fn u_position(&self, i: usize, j: usize) -> (f64, f64) {
        (i as f64 * self.dx, (j as f64 + 0.5) * self.dy)
    }

    /// Coordinates of y-face `(i, j)`.
    pub fn v_position(&self, i: usize, j: usize) -> (f64, f64) {
        ((i as f64 + 0.5) * self.dx, j as f64 * self.dy)
    }

    /// Cell-centred velocity, averaged from the two opposite faces.
    pub fn cell_velocity(&self, u: &[f64], v: &[f64], i: usize, j: usize) -> (f64, f64) {
        let ux = 0.5 * (u[self.u_index(i, j)] + u[self.u_index(i + 1, j)]);
        let uy = 0.5 * (v[self.v_index(i, j)] + v[self.v_index(i, j + 1)]);
        (ux, uy)
    }

    /// Enforces the normal velocity on domain faces and closes solid faces.
    ///
    /// `u` is normal to the left and right sides and `v` is normal to the
    /// bottom and top sides. Walls and inlets prescribe the stored velocity;
    /// slip walls impose zero normal velocity; outlets copy the adjacent
    /// interior face, a first-order zero-gradient condition. Periodic sides are
    /// left untouched and mirrored afterwards, so both boundary faces of a
    /// periodic axis hold the same physical value. Tangential wall velocities
    /// live half a cell inside the domain and are imposed through ghost values
    /// in the momentum step, so they remain part of the projection and keep the
    /// discrete divergence consistent at boundary cells. Every face touching a
    /// solid cell is set to zero afterwards, which makes the obstacle a no-slip,
    /// no-flux boundary.
    pub fn apply_velocity_boundaries(&self, boundaries: &Boundaries, u: &mut [f64], v: &mut [f64]) {
        let ny = self.ny as f64;
        let nx = self.nx as f64;
        for j in 0..self.ny {
            let fraction = (j as f64 + 0.5) / ny;
            if !boundaries.is_periodic(Side::Left) {
                u[self.u_index(0, j)] = normal_velocity(
                    &boundaries.left,
                    u[self.u_index(1, j)],
                    0,
                    boundaries.inlet_profiles.scale(Side::Left, fraction),
                );
            }
            if !boundaries.is_periodic(Side::Right) {
                u[self.u_index(self.nx, j)] = normal_velocity(
                    &boundaries.right,
                    u[self.u_index(self.nx - 1, j)],
                    0,
                    boundaries.inlet_profiles.scale(Side::Right, fraction),
                );
            }
        }
        for i in 0..self.nx {
            let fraction = (i as f64 + 0.5) / nx;
            if !boundaries.is_periodic(Side::Bottom) {
                v[self.v_index(i, 0)] = normal_velocity(
                    &boundaries.bottom,
                    v[self.v_index(i, 1)],
                    1,
                    boundaries.inlet_profiles.scale(Side::Bottom, fraction),
                );
            }
            if !boundaries.is_periodic(Side::Top) {
                v[self.v_index(i, self.ny)] = normal_velocity(
                    &boundaries.top,
                    v[self.v_index(i, self.ny - 1)],
                    1,
                    boundaries.inlet_profiles.scale(Side::Top, fraction),
                );
            }
        }
        if boundaries.periodic_x() {
            for j in 0..self.ny {
                u[self.u_index(self.nx, j)] = u[self.u_index(0, j)];
            }
        }
        if boundaries.periodic_y() {
            for i in 0..self.nx {
                v[self.v_index(i, self.ny)] = v[self.v_index(i, 0)];
            }
        }
        self.zero_solid_faces(u, v);
    }

    /// Applies the first-order convective outlet condition on every
    /// `convective` side.
    ///
    /// The boundary face is relaxed as `φ_b = (φ_bⁿ + β·φ_i)/(1 + β)` with
    /// `β = max(uₙⁿ, 0)·dt/h`, where `φ_bⁿ` is the previous boundary value,
    /// `φ_i` the new adjacent interior face and `uₙⁿ` the previous interior
    /// normal velocity. The result is a bounded convex blend: at rest it equals
    /// the zero-gradient value, and outgoing flow advects the interior value
    /// outwards. A side with inflow (`uₙⁿ ≤ 0`) falls back to zero gradient.
    pub fn apply_convective_outlets(
        &self,
        boundaries: &Boundaries,
        previous_u: &[f64],
        previous_v: &[f64],
        u: &mut [f64],
        v: &mut [f64],
        dt: f64,
    ) {
        if !dt.is_finite() || dt <= 0.0 {
            return;
        }
        let blend = |previous: f64, interior: f64, rate: f64| {
            if rate <= 0.0 {
                return interior;
            }
            let beta = rate * dt;
            (previous + beta * interior) / (1.0 + beta)
        };
        for j in 0..self.ny {
            if boundaries.left.kind == BoundaryKind::Convective
                && !boundaries.is_periodic(Side::Left)
            {
                let face = self.u_index(0, j);
                let interior = self.u_index(1, j);
                if self.u_face_open(0, j) {
                    u[face] = blend(
                        previous_u[face],
                        u[interior],
                        previous_u[interior] / self.dx,
                    );
                }
            }
            if boundaries.right.kind == BoundaryKind::Convective
                && !boundaries.is_periodic(Side::Right)
            {
                let face = self.u_index(self.nx, j);
                let interior = self.u_index(self.nx - 1, j);
                if self.u_face_open(self.nx, j) {
                    u[face] = blend(
                        previous_u[face],
                        u[interior],
                        previous_u[interior] / self.dx,
                    );
                }
            }
        }
        for i in 0..self.nx {
            if boundaries.bottom.kind == BoundaryKind::Convective
                && !boundaries.is_periodic(Side::Bottom)
            {
                let face = self.v_index(i, 0);
                let interior = self.v_index(i, 1);
                if self.v_face_open(i, 0) {
                    v[face] = blend(
                        previous_v[face],
                        v[interior],
                        previous_v[interior] / self.dy,
                    );
                }
            }
            if boundaries.top.kind == BoundaryKind::Convective && !boundaries.is_periodic(Side::Top)
            {
                let face = self.v_index(i, self.ny);
                let interior = self.v_index(i, self.ny - 1);
                if self.v_face_open(i, self.ny) {
                    v[face] = blend(
                        previous_v[face],
                        v[interior],
                        previous_v[interior] / self.dy,
                    );
                }
            }
        }
    }

    /// Sets every face adjacent to a solid cell to zero velocity.
    pub fn zero_solid_faces(&self, u: &mut [f64], v: &mut [f64]) {
        for j in 0..self.ny {
            for i in 0..=self.nx {
                if !self.u_face_open(i, j) {
                    u[self.u_index(i, j)] = 0.0;
                }
            }
        }
        for j in 0..=self.ny {
            for i in 0..self.nx {
                if !self.v_face_open(i, j) {
                    v[self.v_index(i, j)] = 0.0;
                }
            }
        }
    }

    /// Computes the discrete divergence of every cell.
    ///
    /// Solid cells carry no flow and report zero divergence.
    pub fn divergence(&self, u: &[f64], v: &[f64]) -> Vec<f64> {
        let mut divergence = vec![0.0; self.pressure_count()];
        for j in 0..self.ny {
            for i in 0..self.nx {
                let index = self.pressure_index(i, j);
                if self.solid[index] {
                    continue;
                }
                divergence[index] = (u[self.u_index(i + 1, j)] - u[self.u_index(i, j)]) / self.dx
                    + (v[self.v_index(i, j + 1)] - v[self.v_index(i, j)]) / self.dy;
            }
        }
        divergence
    }
}

/// Normal velocity stored on one domain boundary face.
///
/// Outlets continue the adjacent interior value (zero gradient), slip walls
/// block penetration, and walls prescribe the document velocity. Inlets scale
/// the prescribed normal velocity by `scale`, the configured profile factor at
/// the face position measured from the low corner of the side. A convective
/// outlet starts from the same zero-gradient value and is relaxed towards the
/// interior by [`Mesh::apply_convective_outlets`] after every step.
fn normal_velocity(boundary: &Boundary, interior: f64, component: usize, scale: f64) -> f64 {
    match boundary.kind {
        BoundaryKind::Outlet | BoundaryKind::Convective | BoundaryKind::Periodic => interior,
        BoundaryKind::Slip => 0.0,
        BoundaryKind::Wall => boundary.velocity[component],
        BoundaryKind::Inlet => boundary.velocity[component] * scale,
    }
}
