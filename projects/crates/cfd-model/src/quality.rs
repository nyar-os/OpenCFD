//! Structured-mesh quality metrics.
//!
//! The solver only accepts a uniform Cartesian mesh, so the metrics below are
//! constant over the domain for a given case. They are still computed cell by
//! cell and face by face, so the report stays an honest measurement of the mesh
//! instead of a copied formula, and it would keep working if graded spacing
//! ever becomes supported.
//!
//! * `nonOrthogonality` is the angle between the cell-centre-to-neighbour
//!   vector and the face normal, in degrees. Zero means perfectly orthogonal.
//! * `skewness` is the distance from an interior face centre to the midpoint of
//!   the two adjacent cell centres divided by the distance between those
//!   centres, in `[0, 1]`. Zero means a perfectly centred face.
//! * `aspectRatio` is the longest over the shortest edge of one cell.

use serde::{Deserialize, Serialize};

use crate::mesh::Mesh;

/// Quality report of one structured mesh.
///
/// `cells` counts every cell of the rectangle and `fluidCells` counts only the
/// cells that are not inside a solid obstacle. The aspect-ratio extremes are
/// taken over fluid cells; non-orthogonality and skewness are taken over the
/// faces that touch a fluid cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshQuality {
    /// Number of cells in the rectangle (`nx × ny`).
    pub cells: usize,
    /// Number of cells that are not solid.
    pub fluid_cells: usize,
    /// Cell size in x (m).
    pub dx: f64,
    /// Cell size in y (m).
    pub dy: f64,
    /// Smallest cell aspect ratio over fluid cells.
    pub min_aspect_ratio: f64,
    /// Mean cell aspect ratio over fluid cells.
    pub mean_aspect_ratio: f64,
    /// Largest cell aspect ratio over fluid cells.
    pub max_aspect_ratio: f64,
    /// Mean non-orthogonality angle in degrees.
    pub mean_non_orthogonality: f64,
    /// Largest non-orthogonality angle in degrees.
    pub max_non_orthogonality: f64,
    /// Mean interior-face skewness.
    pub mean_skewness: f64,
    /// Largest interior-face skewness.
    pub max_skewness: f64,
}

impl Default for MeshQuality {
    fn default() -> Self {
        Self {
            cells: 0,
            fluid_cells: 0,
            dx: 0.0,
            dy: 0.0,
            min_aspect_ratio: 1.0,
            mean_aspect_ratio: 1.0,
            max_aspect_ratio: 1.0,
            mean_non_orthogonality: 0.0,
            max_non_orthogonality: 0.0,
            mean_skewness: 0.0,
            max_skewness: 0.0,
        }
    }
}

/// Measures the quality of a structured mesh.
///
/// A mesh without fluid cells reports the default ratios of one and zero
/// angles, so the report never contains NaN.
pub fn mesh_quality(mesh: &Mesh) -> MeshQuality {
    let cells = mesh.pressure_count();
    let mut fluid_cells = 0_usize;
    let mut aspect_min = f64::INFINITY;
    let mut aspect_max = 0.0_f64;
    let mut aspect_sum = 0.0;
    let mut orthogonality_sum = 0.0;
    let mut orthogonality_max = 0.0_f64;
    let mut orthogonality_count = 0_usize;
    let (aspect_low, aspect_high) = if mesh.dx < mesh.dy {
        (mesh.dx, mesh.dy)
    } else {
        (mesh.dy, mesh.dx)
    };
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            if mesh.is_solid(i, j) {
                continue;
            }
            fluid_cells += 1;
            let aspect = if aspect_low > 0.0 {
                aspect_high / aspect_low
            } else {
                1.0
            };
            aspect_min = aspect_min.min(aspect);
            aspect_max = aspect_max.max(aspect);
            aspect_sum += aspect;
            for angle in [
                non_orthogonality(mesh, i, j, Face::West),
                non_orthogonality(mesh, i, j, Face::East),
                non_orthogonality(mesh, i, j, Face::South),
                non_orthogonality(mesh, i, j, Face::North),
            ] {
                orthogonality_sum += angle;
                orthogonality_max = orthogonality_max.max(angle);
                orthogonality_count += 1;
            }
        }
    }
    let (skewness_sum, skewness_max, skewness_count) = interior_skewness(mesh);
    MeshQuality {
        cells,
        fluid_cells,
        dx: mesh.dx,
        dy: mesh.dy,
        min_aspect_ratio: if aspect_min.is_finite() {
            aspect_min
        } else {
            1.0
        },
        mean_aspect_ratio: if fluid_cells > 0 {
            aspect_sum / fluid_cells as f64
        } else {
            1.0
        },
        max_aspect_ratio: if fluid_cells > 0 { aspect_max } else { 1.0 },
        mean_non_orthogonality: if orthogonality_count > 0 {
            orthogonality_sum / orthogonality_count as f64
        } else {
            0.0
        },
        max_non_orthogonality: orthogonality_max,
        mean_skewness: if skewness_count > 0 {
            skewness_sum / skewness_count as f64
        } else {
            0.0
        },
        max_skewness: skewness_max,
    }
}

/// One of the four faces of a pressure cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Face {
    West,
    East,
    South,
    North,
}

/// Angle in degrees between the centre-to-neighbour vector and the face normal.
///
/// A boundary face uses the cell centre mirrored across the face, so a boundary
/// cell is measured against the same `dx`/`dy` spacing as an interior one.
fn non_orthogonality(mesh: &Mesh, i: usize, j: usize, face: Face) -> f64 {
    let (cx, cy) = mesh.cell_center(i, j);
    let (normal, neighbour) = match face {
        Face::West => {
            let neighbour = if i > 0 {
                mesh.cell_center(i - 1, j)
            } else {
                (cx - mesh.dx, cy)
            };
            ((-1.0, 0.0), neighbour)
        }
        Face::East => {
            let neighbour = if i + 1 < mesh.nx {
                mesh.cell_center(i + 1, j)
            } else {
                (cx + mesh.dx, cy)
            };
            ((1.0, 0.0), neighbour)
        }
        Face::South => {
            let neighbour = if j > 0 {
                mesh.cell_center(i, j - 1)
            } else {
                (cx, cy - mesh.dy)
            };
            ((0.0, -1.0), neighbour)
        }
        Face::North => {
            let neighbour = if j + 1 < mesh.ny {
                mesh.cell_center(i, j + 1)
            } else {
                (cx, cy + mesh.dy)
            };
            ((0.0, 1.0), neighbour)
        }
    };
    let direction = (neighbour.0 - cx, neighbour.1 - cy);
    let length = direction.0.hypot(direction.1);
    if length == 0.0 {
        return 0.0;
    }
    let cosine = ((direction.0 * normal.0 + direction.1 * normal.1) / length).clamp(-1.0, 1.0);
    cosine.abs().acos().to_degrees()
}

/// Mean and maximum skewness over interior faces that join two fluid cells.
fn interior_skewness(mesh: &Mesh) -> (f64, f64, usize) {
    let mut sum = 0.0;
    let mut worst = 0.0_f64;
    let mut count = 0_usize;
    for j in 0..mesh.ny {
        for i in 1..mesh.nx {
            if mesh.is_solid(i - 1, j) || mesh.is_solid(i, j) {
                continue;
            }
            let left = mesh.cell_center(i - 1, j);
            let right = mesh.cell_center(i, j);
            let face = (i as f64 * mesh.dx, (j as f64 + 0.5) * mesh.dy);
            sum += skewness(left, right, face);
            worst = worst.max(skewness(left, right, face));
            count += 1;
        }
    }
    for j in 1..mesh.ny {
        for i in 0..mesh.nx {
            if mesh.is_solid(i, j - 1) || mesh.is_solid(i, j) {
                continue;
            }
            let lower = mesh.cell_center(i, j - 1);
            let upper = mesh.cell_center(i, j);
            let face = ((i as f64 + 0.5) * mesh.dx, j as f64 * mesh.dy);
            sum += skewness(lower, upper, face);
            worst = worst.max(skewness(lower, upper, face));
            count += 1;
        }
    }
    (sum, worst, count)
}

/// Distance from the face centre to the midpoint of the two cell centres,
/// divided by their separation.
fn skewness(first: (f64, f64), second: (f64, f64), face: (f64, f64)) -> f64 {
    let separation = (second.0 - first.0).hypot(second.1 - first.1);
    if separation == 0.0 {
        return 0.0;
    }
    let midpoint = (0.5 * (first.0 + second.0), 0.5 * (first.1 + second.1));
    (face.0 - midpoint.0).hypot(face.1 - midpoint.1) / separation
}
