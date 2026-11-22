use super::*;
use crate::model::{Boundary, MeshConfig, Probe};

fn mesh() -> Mesh {
    Mesh::new(&MeshConfig {
        nx: 10,
        ny: 6,
        length_x: 1.0,
        length_y: 1.0,
    })
    .expect("mesh")
}

#[test]
fn buoyancy_acceleration_points_away_from_gravity_when_hot() {
    let mesh = mesh();
    let buoyancy = Buoyancy {
        beta: 0.1,
        t_ref: 300.0,
        gravity: [0.0, -9.81],
    };
    let mut temperature = vec![300.0; mesh.pressure_count()];
    temperature[mesh.pressure_index(0, 0)] = 310.0;
    temperature[mesh.pressure_index(1, 0)] = 310.0;
    let vertical = face_buoyancy(
        &mesh,
        Some(&buoyancy),
        Some(&temperature),
        (0, 0),
        (1, 0),
        1,
    );
    assert!((vertical - 9.81).abs() < 1e-12);
    assert_eq!(
        face_buoyancy(
            &mesh,
            Some(&buoyancy),
            Some(&temperature),
            (0, 0),
            (1, 0),
            0,
        ),
        0.0
    );

    temperature[mesh.pressure_index(0, 0)] = 290.0;
    temperature[mesh.pressure_index(1, 0)] = 290.0;
    let cold = face_buoyancy(
        &mesh,
        Some(&buoyancy),
        Some(&temperature),
        (0, 0),
        (1, 0),
        1,
    );
    assert!((cold + 9.81).abs() < 1e-12);
    assert_eq!(
        face_buoyancy(&mesh, None, Some(&temperature), (0, 0), (1, 0), 1),
        0.0
    );
}

#[test]
fn quick_and_central_are_exact_for_quadratic_profiles() {
    let mesh = mesh();
    let mut u = vec![0.0; mesh.u_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let (x, _) = mesh.u_position(i, j);
            u[mesh.u_index(i, j)] = x * x;
        }
    }
    let i = 4;
    let j = 2;
    let exact = 2.0 * i as f64 * mesh.dx;
    let upwind = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Upwind, false);
    let central = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Central, false);
    let quick = u_convective_derivative(&mesh, &u, i, j, 1.0, ConvectionScheme::Quick, false);
    let quick_reverse =
        u_convective_derivative(&mesh, &u, i, j, -1.0, ConvectionScheme::Quick, false);
    assert!((central - exact).abs() < 1e-12, "central {central}");
    assert!((quick - exact).abs() < 1e-12, "QUICK {quick}");
    assert!(
        (quick_reverse - exact).abs() < 1e-12,
        "reverse QUICK {quick_reverse}"
    );
    assert!(
        (upwind - exact).abs() > 0.5 * mesh.dx,
        "upwind {upwind} should be first-order diffusive"
    );

    let mut v = vec![0.0; mesh.v_count()];
    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            let (_, y) = mesh.v_position(i, j);
            v[mesh.v_index(i, j)] = y * y;
        }
    }
    let j = 3;
    let exact = 2.0 * j as f64 * mesh.dy;
    let central = v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Central, false);
    let quick = v_convective_derivative(&mesh, &v, 2, j, 1.0, ConvectionScheme::Quick, false);
    let quick_reverse =
        v_convective_derivative(&mesh, &v, 2, j, -1.0, ConvectionScheme::Quick, false);
    assert!((central - exact).abs() < 1e-12, "central {central}");
    assert!((quick - exact).abs() < 1e-12, "QUICK {quick}");
    assert!(
        (quick_reverse - exact).abs() < 1e-12,
        "reverse QUICK {quick_reverse}"
    );
}

#[test]
fn quick_falls_back_to_upwind_next_to_the_boundary() {
    let mesh = mesh();
    let mut u = vec![0.0; mesh.u_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let (x, _) = mesh.u_position(i, j);
            u[mesh.u_index(i, j)] = x * x;
        }
    }
    let upwind = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Upwind, false);
    let quick = u_convective_derivative(&mesh, &u, 1, 0, 1.0, ConvectionScheme::Quick, false);
    assert_eq!(upwind, quick);
}

#[test]
fn probe_samples_the_nearest_cell() {
    let mesh = mesh();
    let mut model = Model::new();
    model.mesh.nx = 10;
    model.mesh.ny = 6;
    model.time.dt = 0.5;
    model.probes = vec![Probe {
        id: "p".to_string(),
        x: 0.05,
        y: 0.95,
    }];
    let cells = locate_probes(&mesh, &model).expect("locate probe");
    assert_eq!(cells, vec![(0, 5)]);

    let mut u = vec![0.0; mesh.u_count()];
    let mut v = vec![0.0; mesh.v_count()];
    let mut p = vec![0.0; mesh.pressure_count()];
    u[mesh.u_index(0, 5)] = 0.2;
    u[mesh.u_index(1, 5)] = 0.4;
    v[mesh.v_index(0, 5)] = 0.1;
    v[mesh.v_index(0, 6)] = 0.3;
    p[mesh.pressure_index(0, 5)] = 12.5;
    let temperature = vec![300.0; mesh.pressure_count()];

    let mut samples = vec![Vec::new()];
    append_probe_samples(
        &mesh,
        &u,
        &v,
        &p,
        Some(temperature.as_slice()),
        3,
        1.5,
        &model.probes,
        &cells,
        &mut samples,
    );
    let sample = &samples[0][0];
    assert_eq!(sample.step, 3);
    assert!((sample.time - 1.5).abs() < 1e-12);
    assert!((sample.u - 0.3).abs() < 1e-12, "u = {}", sample.u);
    assert!((sample.v - 0.2).abs() < 1e-12, "v = {}", sample.v);
    assert!(
        (sample.speed - 0.13_f64.sqrt()).abs() < 1e-12,
        "speed = {}",
        sample.speed
    );
    assert_eq!(sample.pressure, 12.5);
    assert_eq!(sample.temperature, Some(300.0));
}

#[test]
fn probe_bilinear_interpolation_reproduces_a_linear_field() {
    let mesh = mesh();
    let mut model = Model::new();
    model.mesh.nx = 10;
    model.mesh.ny = 6;
    model.probes = vec![Probe {
        id: "linear".to_string(),
        x: 0.37,
        y: 0.45,
    }];
    let cells = locate_probes(&mesh, &model).expect("locate probe");
    assert_eq!(cells, vec![(3, 2)]);

    let mut u = vec![0.0; mesh.u_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            u[mesh.u_index(i, j)] = 2.0 * i as f64 * mesh.dx + 1.0;
        }
    }
    let mut v = vec![0.0; mesh.v_count()];
    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            v[mesh.v_index(i, j)] = 3.0 * j as f64 * mesh.dy + 2.0;
        }
    }
    let mut p = vec![0.0; mesh.pressure_count()];
    let mut temperature = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let (xc, yc) = mesh.cell_center(i, j);
            p[mesh.pressure_index(i, j)] = 10.0 * xc - 5.0 * yc + 7.0;
            temperature[mesh.pressure_index(i, j)] = 100.0 + xc + 2.0 * yc;
        }
    }

    let mut samples = vec![Vec::new()];
    append_probe_samples(
        &mesh,
        &u,
        &v,
        &p,
        Some(temperature.as_slice()),
        1,
        mesh.dx,
        &model.probes,
        &cells,
        &mut samples,
    );
    let sample = &samples[0][0];
    assert!((sample.u - 1.74).abs() < 1e-12, "u = {}", sample.u);
    assert!((sample.v - 3.35).abs() < 1e-12, "v = {}", sample.v);
    assert!(
        (sample.pressure - 8.45).abs() < 1e-12,
        "pressure = {}",
        sample.pressure
    );
    assert!((sample.temperature.expect("temperature") - 101.27).abs() < 1e-12);
}

#[test]
fn pressure_coefficient_uses_the_gauge_datum_and_reference_speed() {
    let cp = pressure_coefficient(&[-2.0, 0.0, 2.0], 1.0, 2.0);
    assert_eq!(cp, vec![-1.0, 0.0, 1.0]);
    let scaled = pressure_coefficient(&[4.0], 2.0, 1.0);
    assert!((scaled[0] - 4.0).abs() < 1e-12);
    assert_eq!(pressure_coefficient(&[1.0], 0.0, 1.0), vec![0.0]);
}

#[test]
fn force_decomposition_separates_pressure_and_shear() {
    let mut mesh = Mesh::new(&MeshConfig {
        nx: 6,
        ny: 4,
        length_x: 1.0,
        length_y: 1.0,
    })
    .expect("mesh");
    let obstacle = crate::model::Obstacle::Rect {
        id: "block".to_string(),
        x0: 2.0 * mesh.dx,
        y0: mesh.dy,
        x1: 4.0 * mesh.dx,
        y1: 3.0 * mesh.dy,
        rotation: 0.0,
    };
    mesh.apply_obstacles(std::slice::from_ref(&obstacle))
        .expect("obstacles");
    let mut model = Model::new();
    model.fluid.viscosity = 0.01;
    model.obstacles = vec![obstacle];

    let u = vec![0.0; mesh.u_count()];
    let v = vec![0.0; mesh.v_count()];
    let mut pressure = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            pressure[mesh.pressure_index(i, j)] = 2.0 + i as f64 + 3.0 * j as f64;
        }
    }
    let pressure_only = raw_forces(&mesh, &model, &u, &v, &pressure, None, None);
    let force = pressure_only[0];
    assert!(
        force.fx_pressure.abs() > 1e-9,
        "fxPressure {}",
        force.fx_pressure
    );
    assert!(
        force.fy_pressure.abs() > 1e-9,
        "fyPressure {}",
        force.fy_pressure
    );
    assert_eq!(force.fx_viscous, 0.0);
    assert_eq!(force.fy_viscous, 0.0);
    assert_eq!(force.fx, force.fx_pressure);
    assert_eq!(force.fy, force.fy_pressure);

    let zeros = vec![0.0; mesh.pressure_count()];
    let mut shear_u = vec![0.0; mesh.u_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            shear_u[mesh.u_index(i, j)] = i as f64;
        }
    }
    let mut shear_v = vec![0.0; mesh.v_count()];
    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            shear_v[mesh.v_index(i, j)] = j as f64;
        }
    }
    let shear_only = raw_forces(&mesh, &model, &shear_u, &shear_v, &zeros, None, None);
    let force = shear_only[0];
    assert!(
        force.fx_viscous.abs() > 1e-9,
        "fxViscous {}",
        force.fx_viscous
    );
    assert!(
        force.fy_viscous.abs() > 1e-9,
        "fyViscous {}",
        force.fy_viscous
    );
    assert_eq!(force.fx_pressure, 0.0);
    assert_eq!(force.fy_pressure, 0.0);
    assert_eq!(force.fx, force.fx_viscous);
    assert_eq!(force.fy, force.fy_viscous);
    assert!((force.fx - (force.fx_pressure + force.fx_viscous)).abs() < 1e-12);
    assert!((force.fy - (force.fy_pressure + force.fy_viscous)).abs() < 1e-12);
}

#[test]
fn probe_inside_a_solid_cell_is_rejected() {
    let mut mesh = mesh();
    mesh.apply_obstacles(&[crate::model::Obstacle::Rect {
        id: "block".to_string(),
        x0: 0.0,
        y0: 0.9,
        x1: 0.2,
        y1: 1.0,
        rotation: 0.0,
    }])
    .expect("obstacles");
    let mut model = Model::new();
    model.mesh.nx = 10;
    model.mesh.ny = 6;
    model.probes = vec![Probe {
        id: "blocked".to_string(),
        x: 0.05,
        y: 0.95,
    }];
    let error = locate_probes(&mesh, &model).expect_err("solid probe");
    assert_eq!(error.code, "probe/invalid");
}

#[test]
fn wall_shear_reproduces_a_linear_tangential_profile() {
    let mesh = mesh();
    let mut model = Model::new();
    model.mesh.nx = 10;
    model.mesh.ny = 6;
    model.fluid.density = 1.0;
    model.fluid.viscosity = 0.02;
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [1.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.bottom = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.left = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Inlet,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };

    // Tangential profiles u = y and v = x have unit slope, so the wall
    // shear is exactly ±mu with the half-cell gradient.
    let mut u = vec![0.0; mesh.u_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let (_, y) = mesh.u_position(i, j);
            u[mesh.u_index(i, j)] = y;
        }
    }
    let mut v = vec![0.0; mesh.v_count()];
    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            let (x, _) = mesh.v_position(i, j);
            v[mesh.v_index(i, j)] = x;
        }
    }

    let shear = wall_shear(&mesh, &model, None, &u, &v);
    assert_eq!(shear.len(), 3, "the inlet side must be skipped");
    let mu = model.fluid.viscosity;
    let find = |side: &str| {
        shear
            .iter()
            .find(|item| item.side == side)
            .unwrap_or_else(|| panic!("missing {side} wall shear"))
    };
    let top = find("top");
    assert!((top.mean_shear - mu).abs() < 1e-12, "top {top:?}");
    assert!((top.max_shear - mu).abs() < 1e-12);
    assert!((top.friction_coefficient - 2.0 * mu).abs() < 1e-12);
    let bottom = find("bottom");
    assert!((bottom.mean_shear + mu).abs() < 1e-12, "bottom {bottom:?}");
    let left = find("left");
    assert!((left.mean_shear + mu).abs() < 1e-12, "left {left:?}");
}

#[test]
fn wall_function_solves_the_log_law() {
    let config = WallFunction::default();
    let (viscosity, density) = (1.0e-5, 1.0);
    let (distance, velocity) = (0.01, 10.0);
    let (shear, y_plus) = wall_shear_at(velocity, distance, density, viscosity, Some(&config));
    assert!(y_plus > config.min_y_plus, "y+ = {y_plus}");
    let u_tau = (shear / density).sqrt();
    let log_law = u_tau * ((u_tau * distance / viscosity).ln() / config.kappa + config.intercept);
    assert!(
        (log_law - velocity).abs() < 1e-8,
        "log law {log_law} vs velocity {velocity}"
    );
    let laminar = density * viscosity * velocity / distance;
    assert!(
        shear > 5.0 * laminar,
        "wall-function shear {shear} should exceed the laminar {laminar}"
    );
    let (reversed, _) = wall_shear_at(-velocity, distance, density, viscosity, Some(&config));
    assert_eq!(reversed, -shear);
}

#[test]
fn wall_function_keeps_the_viscous_sublayer_below_the_crossover() {
    let config = WallFunction::default();
    let (viscosity, density) = (0.01, 1.0);
    let (distance, velocity) = (0.03, 1.0);
    let (shear, y_plus) = wall_shear_at(velocity, distance, density, viscosity, Some(&config));
    assert!(y_plus <= config.min_y_plus, "y+ = {y_plus}");
    let laminar = density * viscosity * velocity / distance;
    assert!((shear - laminar).abs() < 1e-12);
    let (plain, plain_y_plus) = wall_shear_at(velocity, distance, density, viscosity, None);
    assert_eq!(plain, shear);
    assert_eq!(plain_y_plus, y_plus);
}
