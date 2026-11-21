//! Derived cell-centred fields computed from a solved velocity field.
//!
//! Both fields use the staggered face velocities directly, so they are
//! deterministic post-processing values and never feed back into the solve.

use crate::mesh::Mesh;

/// Cell-centred vorticity `ω = ∂v/∂x − ∂u/∂y` (1/s).
///
/// The gradient of a linear velocity field is reproduced exactly because every
/// interior cell uses central differences over cell-centred velocities, while
/// the first and last row or column fall back to one-sided differences. Solid
/// cells carry no flow and report zero vorticity.
pub fn vorticity(mesh: &Mesh, u: &[f64], v: &[f64]) -> Vec<f64> {
    let mut field = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            if mesh.solid[index] {
                continue;
            }
            let dv_dx = horizontal_gradient(mesh, u, v, i, j, true);
            let du_dy = vertical_gradient(mesh, u, v, i, j, false);
            field[index] = dv_dx - du_dy;
        }
    }
    field
}

/// Cell-centred stream function `ψ` (m²/s) with zero mean over fluid cells.
///
/// `ψ` is integrated along a monotone path: first up the left column with
/// `∂ψ/∂y = u`, then across each row with `∂ψ/∂x = −v`. It is defined up to an
/// additive constant, so the mean over fluid cells is removed. In a domain cut
/// by obstacles the value is path dependent, which is why solid cells report
/// zero and every fluid cell is measured from the same bottom-left corner.
pub fn stream_function(mesh: &Mesh, u: &[f64], v: &[f64]) -> Vec<f64> {
    let mut field = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        if j > 0 {
            let above = mesh.pressure_index(0, j);
            let below = mesh.pressure_index(0, j - 1);
            let (ux, _) = mesh.cell_velocity(u, v, 0, j - 1);
            field[above] = field[below] + ux * mesh.dy;
        }
        for i in 0..mesh.nx - 1 {
            let current = mesh.pressure_index(i, j);
            let next = mesh.pressure_index(i + 1, j);
            let (_, uy) = mesh.cell_velocity(u, v, i, j);
            field[next] = field[current] - uy * mesh.dx;
        }
    }

    let mut sum = 0.0;
    let mut count = 0;
    for (index, value) in field.iter().enumerate() {
        if mesh.solid[index] {
            continue;
        }
        sum += *value;
        count += 1;
    }
    if count > 0 {
        let mean = sum / count as f64;
        for (index, value) in field.iter_mut().enumerate() {
            if mesh.solid[index] {
                *value = 0.0;
            } else {
                *value -= mean;
            }
        }
    } else {
        field.fill(0.0);
    }
    field
}

fn horizontal_gradient(
    mesh: &Mesh,
    u: &[f64],
    v: &[f64],
    i: usize,
    j: usize,
    y_component: bool,
) -> f64 {
    let component = |ci: usize, cj: usize| {
        let (ux, uy) = mesh.cell_velocity(u, v, ci, cj);
        if y_component {
            uy
        } else {
            ux
        }
    };
    if i == 0 {
        (component(1, j) - component(0, j)) / mesh.dx
    } else if i + 1 == mesh.nx {
        (component(mesh.nx - 1, j) - component(mesh.nx - 2, j)) / mesh.dx
    } else {
        (component(i + 1, j) - component(i - 1, j)) / (2.0 * mesh.dx)
    }
}

fn vertical_gradient(
    mesh: &Mesh,
    u: &[f64],
    v: &[f64],
    i: usize,
    j: usize,
    y_component: bool,
) -> f64 {
    let component = |ci: usize, cj: usize| {
        let (ux, uy) = mesh.cell_velocity(u, v, ci, cj);
        if y_component {
            uy
        } else {
            ux
        }
    };
    if j == 0 {
        (component(i, 1) - component(i, 0)) / mesh.dy
    } else if j + 1 == mesh.ny {
        (component(i, mesh.ny - 1) - component(i, mesh.ny - 2)) / mesh.dy
    } else {
        (component(i, j + 1) - component(i, j - 1)) / (2.0 * mesh.dy)
    }
}
