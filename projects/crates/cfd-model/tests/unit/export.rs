use super::*;
use crate::model::MeshConfig;

fn mesh() -> crate::mesh::Mesh {
    crate::mesh::Mesh::new(&MeshConfig {
        nx: 3,
        ny: 3,
        length_x: 1.0,
        length_y: 1.0,
    })
    .expect("mesh")
}

#[test]
fn marching_squares_places_a_linear_iso_line_between_columns() {
    let mesh = mesh();
    let mut values = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            values[mesh.pressure_index(i, j)] = i as f64;
        }
    }
    let segments = marching_squares_segments(&mesh, &values, 0.5);
    assert_eq!(segments.len(), 2, "one segment per interior quad row");
    let x = mesh.dx;
    for ((x0, y0), (x1, y1)) in segments {
        assert!((x0 - x).abs() < 1e-12, "x0 = {x0}");
        assert!((x1 - x).abs() < 1e-12, "x1 = {x1}");
        assert!((y1 - y0 - mesh.dy).abs() < 1e-12);
    }
}

#[test]
fn band_polygon_clips_the_quad_to_the_value_window() {
    let quad = [
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0),
        (0.0, 1.0, 0.0),
    ];
    let all = band_polygon(&quad, 0.0, 1.0);
    assert_eq!(all.len(), 4, "the full window keeps the quad");

    let none = band_polygon(&quad, 1.5, 2.0);
    assert!(none.is_empty(), "a disjoint window keeps nothing");

    let left_half = band_polygon(&quad, 0.0, 0.5);
    assert_eq!(left_half.len(), 4, "a half window is a quad");
    let area: f64 = left_half
        .iter()
        .zip(left_half.iter().cycle().skip(1))
        .take(left_half.len())
        .map(|((x0, y0), (x1, y1))| x0 * y1 - x1 * y0)
        .sum::<f64>()
        .abs()
        / 2.0;
    assert!((area - 0.5).abs() < 1e-12, "area = {area}");
}
