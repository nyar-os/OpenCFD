use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{
    export_csv, export_svg, export_vtk, solve, solve_pressure, solve_with, stream_function,
    transport_scalar, vorticity, Boundary, BoundaryKind, ExportField, Mesh, Model, Obstacle,
    PressureSettings, ScalarConfig, SolveOptions, CAPABILITIES_JSON, VERSION,
};

fn temporary_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("ocfd-{name}-{}-{unique}", std::process::id()));
    fs::create_dir_all(&directory).expect("create temporary directory");
    directory
}

fn mesh_model(nx: usize, ny: usize) -> Model {
    let mut model = Model::new();
    model.mesh.nx = nx;
    model.mesh.ny = ny;
    model.time.steps = 50;
    model.time.pressure_iterations = 60;
    model.time.tolerance = 1e-3;
    model
}

fn assert_finite(values: &[f64], label: &str) {
    assert!(
        values.iter().all(|value| value.is_finite()),
        "{label} contains NaN or Inf"
    );
}

#[test]
fn still_cavity_stays_at_rest_without_nan() {
    let mut model = mesh_model(16, 16);
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [0.0, 0.0],
    };
    model.time.steps = 40;

    let solution = solve(&model).expect("solve");
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert_finite(&solution.p, "p");
    assert!(
        solution.report.max_velocity < 1e-12,
        "max velocity {}",
        solution.report.max_velocity
    );
    assert!(
        solution.report.divergence_l2 < 1e-12,
        "divergence {}",
        solution.report.divergence_l2
    );
    assert!(solution.report.converged);
    assert_eq!(solution.report.steps, 40);
}

#[test]
fn lid_driven_cavity_develops_clockwise_flow() {
    let mut model = Model::new();
    model.mesh.nx = 24;
    model.mesh.ny = 24;
    model.time.steps = 300;
    model.time.pressure_iterations = 120;
    model.time.tolerance = 1e-3;

    let solution = solve(&model).expect("solve");
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert_finite(&solution.p, "p");

    let mesh = &solution.mesh;
    let report = &solution.report;
    assert_eq!(report.format, "open-cfd-report");
    assert_eq!(report.version, 1);
    assert_eq!(report.steps, 300);
    assert_eq!(report.dt, model.time.dt);
    assert!((report.time - 3.0).abs() < 1e-12);
    assert!(
        report.max_velocity > 0.3 && report.max_velocity <= 1.05,
        "max velocity {}",
        report.max_velocity
    );
    assert!(report.min_pressure <= report.max_pressure);
    assert!(
        report.divergence_l2 < 1e-2,
        "divergence RMS {}",
        report.divergence_l2
    );
    assert!(report.pressure_iterations > 0);
    assert!(
        report.converged,
        "pressure residual {} initial {}",
        report.pressure_residual, report.pressure_residual_initial
    );
    assert!(report.pressure_residual < report.pressure_residual_initial);

    let center_column = mesh.nx / 2;
    let upper = mesh.ny * 7 / 8;
    let (upper_u, _) = mesh.cell_velocity(&solution.u, &solution.v, center_column, upper);
    assert!(upper_u > 0.05, "upper centreline u = {upper_u}");
    let (lid_u, _) = mesh.cell_velocity(&solution.u, &solution.v, center_column, mesh.ny - 1);
    assert!(lid_u > 0.3, "centre-column lid u = {lid_u}");

    let mut upper_sum = 0.0;
    let mut count = 0;
    for j in (mesh.ny * 3 / 4)..mesh.ny {
        for i in 0..mesh.nx {
            let (ux, _) = mesh.cell_velocity(&solution.u, &solution.v, i, j);
            upper_sum += ux;
            count += 1;
        }
    }
    assert!(
        upper_sum / count as f64 > 0.05,
        "upper-quarter mean u = {}",
        upper_sum / count as f64
    );

    let (center_u, _) = mesh.cell_velocity(&solution.u, &solution.v, center_column, mesh.ny / 2);
    assert!(center_u.abs() < 1.0);

    let value = serde_json::to_value(report).expect("report json");
    for key in [
        "format",
        "version",
        "steps",
        "dt",
        "time",
        "maxVelocity",
        "minPressure",
        "maxPressure",
        "divergenceL2",
        "vorticityMin",
        "vorticityMax",
        "streamFunctionMin",
        "streamFunctionMax",
        "converged",
        "pressureIterations",
    ] {
        assert!(value.get(key).is_some(), "missing report field {key}");
    }
}

#[test]
fn pressure_poisson_residual_decreases_with_sweeps() {
    let model = Model::new();
    let mesh = Mesh::new(&model.mesh).expect("mesh");
    let mut divergence = vec![0.0; mesh.pressure_count()];
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let x = (i as f64 + 0.5) * mesh.dx;
            let y = (j as f64 + 0.5) * mesh.dy;
            divergence[mesh.pressure_index(i, j)] =
                (std::f64::consts::TAU * x).sin() * (std::f64::consts::TAU * y).sin();
        }
    }
    let mut pressure = vec![0.0; mesh.pressure_count()];
    let stats = solve_pressure(
        &mesh,
        &model.boundaries,
        &mut pressure,
        &divergence,
        1.0,
        0.01,
        &PressureSettings {
            iterations: 300,
            tolerance: 1e-12,
        },
    )
    .expect("pressure solve");

    assert_eq!(stats.history.len(), 300);
    assert!(stats.initial_residual > 1.0);
    assert!(
        stats.residual < stats.history[0] * 1e-2,
        "initial {}, first {}, final {}",
        stats.initial_residual,
        stats.history[0],
        stats.residual
    );
    assert!(stats.residual < stats.initial_residual);
    assert!(
        !stats.converged,
        "1e-12 relative reduction should not be reached in 300 sweeps"
    );

    let mut short = vec![0.0; mesh.pressure_count()];
    let short_stats = solve_pressure(
        &mesh,
        &model.boundaries,
        &mut short,
        &divergence,
        1.0,
        0.01,
        &PressureSettings {
            iterations: 50,
            tolerance: 1e-12,
        },
    )
    .expect("short pressure solve");
    assert!(stats.residual < short_stats.residual);
}

#[test]
fn csv_and_vtk_exports_have_expected_shape() {
    let mut model = mesh_model(12, 10);
    model.time.steps = 5;
    model.time.pressure_iterations = 20;
    let solution = solve(&model).expect("solve");
    let directory = temporary_dir("export");

    let csv_path = export_csv(&solution, &directory).expect("csv");
    let csv = fs::read_to_string(&csv_path).expect("read csv");
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "x,y,z,pressure,velocity_x,velocity_y,velocity_z,magnitude,temperature,vorticity,streamFunction"
    );
    assert_eq!(lines.len(), 12 * 10 + 1);
    for line in &lines[1..] {
        assert_eq!(line.split(',').count(), 11);
    }

    let vtk_path = export_vtk(&solution, &directory).expect("vtk");
    let vtk = fs::read_to_string(&vtk_path).expect("read vtk");
    for expected in [
        "# vtk DataFile Version 3.0",
        "DATASET UNSTRUCTURED_GRID",
        "POINTS 120 double",
        "CELLS 99 495\n",
        "CELL_TYPES 99\n9\n",
        "POINT_DATA 120",
        "SCALARS pressure double 1",
        "SCALARS temperature double 1",
        "SCALARS magnitude double 1",
        "SCALARS vorticity double 1",
        "SCALARS streamFunction double 1",
        "LOOKUP_TABLE default",
        "VECTORS velocity double",
    ] {
        assert!(vtk.contains(expected), "VTK is missing `{expected}`");
    }
}

#[test]
fn document_round_trip_and_defaults() {
    let mut model = Model::new();
    model.mesh.nx = 20;
    model.mesh.ny = 14;
    model.mesh.length_x = 2.0;
    model.mesh.length_y = 1.0;
    model.fluid.density = 1.2;
    model.fluid.viscosity = 0.002;
    model.time.dt = 0.005;
    model.time.steps = 25;
    model.time.pressure_iterations = 40;
    model.time.tolerance = 1e-4;
    model.boundaries.left = Boundary {
        kind: BoundaryKind::Inlet,
        velocity: [0.5, 0.0],
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
    };
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [1.0, 0.0],
    };
    model.initial = Some(cfd_model::InitialField {
        velocity: [0.1, -0.05],
    });

    let directory = temporary_dir("document");
    let path = directory.join("case.ocfd");
    model.save_atomic(&path).expect("save");
    let loaded = Model::load(&path).expect("load");
    assert_eq!(model, loaded);
    assert_eq!(loaded.version, VERSION);

    let json = model.to_json_pretty().expect("json");
    let value: serde_json::Value = serde_json::from_str(&json).expect("parse json");
    assert_eq!(value["format"], "open-cfd-case");
    assert_eq!(value["units"], "m");
    assert_eq!(value["mesh"]["lengthX"], 2.0);
    assert_eq!(value["time"]["pressureIterations"], 40);
    assert_eq!(value["boundaries"]["right"]["kind"], "outlet");
    assert_eq!(value["initial"]["velocity"][0], 0.1);

    let minimal =
        Model::parse_json(r#"{ "format": "open-cfd-case", "version": 1 }"#).expect("minimal");
    minimal.validate().expect("minimal validates");
    assert_eq!(minimal.mesh.nx, 32);
    assert_eq!(minimal.time.pressure_iterations, 100);

    let mut wrong = minimal.clone();
    wrong.format = "open-cfd-other".to_string();
    assert_eq!(
        wrong.validate().expect_err("format mismatch").code,
        "document/format"
    );

    assert!(CAPABILITIES_JSON.contains("\"cavity2d\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"compressible\": \"unsupported\""));
    assert!(CAPABILITIES_JSON.contains("\"obstacles\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"buoyancy\": \"unsupported\""));
    assert!(CAPABILITIES_JSON.contains("\"vorticity\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"streamFunction\": \"supported\""));
}

#[test]
fn solid_cell_has_zero_velocity_and_divergence() {
    let mut model = mesh_model(12, 12);
    model.time.steps = 8;
    model.time.pressure_iterations = 40;
    model.boundaries.top = Boundary::wall();
    model.obstacles = vec![Obstacle::Rect {
        id: "block".to_string(),
        x0: 0.5,
        y0: 0.5,
        x1: 0.58,
        y1: 0.58,
    }];

    let solution = solve(&model).expect("solve");
    let mesh = &solution.mesh;
    let i = 6;
    let j = 6;
    assert!(mesh.is_solid(i, j), "cell (6, 6) should be solid");
    for index in [
        mesh.u_index(i, j),
        mesh.u_index(i + 1, j),
        mesh.v_index(i, j),
        mesh.v_index(i, j + 1),
    ] {
        assert_eq!(solution.u.get(index).copied().unwrap_or(0.0), 0.0);
        assert_eq!(solution.v.get(index).copied().unwrap_or(0.0), 0.0);
    }
    let divergence = mesh.divergence(&solution.u, &solution.v);
    assert_eq!(divergence[mesh.pressure_index(i, j)], 0.0);
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert_eq!(solution.report.forces.len(), 1);
    assert_eq!(solution.report.forces[0].id, "block");
}

#[test]
fn channel_with_circle_obstacle_has_finite_positive_drag() {
    let mut model = Model::new();
    model.mesh.nx = 48;
    model.mesh.ny = 24;
    model.mesh.length_x = 2.0;
    model.mesh.length_y = 1.0;
    model.fluid.density = 1.0;
    model.fluid.viscosity = 0.002;
    model.time.dt = 0.01;
    model.time.steps = 400;
    model.time.pressure_iterations = 80;
    model.time.tolerance = 1e-3;
    model.boundaries.left = Boundary {
        kind: BoundaryKind::Inlet,
        velocity: [1.0, 0.0],
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
    };
    model.boundaries.top = Boundary::wall();
    model.boundaries.bottom = Boundary::wall();
    model.obstacles = vec![Obstacle::Circle {
        id: "cylinder".to_string(),
        x: 0.8,
        y: 0.5,
        r: 0.1,
    }];

    let solution = solve(&model).expect("solve");
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert_finite(&solution.p, "p");
    assert_eq!(solution.report.forces.len(), 1);
    let force = &solution.report.forces[0];
    assert_eq!(force.id, "cylinder");
    assert!(force.fx.is_finite() && force.cd.is_finite() && force.cl.is_finite());
    assert!(force.cd > 0.0, "drag coefficient {}", force.cd);
    assert!(force.fx > 0.0, "drag force {}", force.fx);
    assert!(!force.history.is_empty());
}

#[test]
fn scalar_diffusion_decays_hot_spot_and_keeps_uniform_field() {
    let model = mesh_model(12, 12);
    let mesh = Mesh::new(&model.mesh).expect("mesh");
    let config = ScalarConfig {
        diffusivity: 0.05,
        initial: 0.0,
        inlet_value: 0.0,
    };
    let dt = 0.005;
    let u = vec![0.0; mesh.u_count()];
    let v = vec![0.0; mesh.v_count()];

    let mut uniform = vec![0.25; mesh.pressure_count()];
    for _ in 0..5 {
        transport_scalar(&mesh, &model.boundaries, &config, dt, &u, &v, &mut uniform)
            .expect("uniform scalar step");
    }
    for value in &uniform {
        assert!(
            (value - 0.25).abs() < 1e-12,
            "uniform scalar drifted to {value}"
        );
    }

    let mut temperature = vec![0.0; mesh.pressure_count()];
    temperature[mesh.pressure_index(6, 6)] = 1.0;
    let mut previous_peak = 1.0;
    for _ in 0..12 {
        transport_scalar(
            &mesh,
            &model.boundaries,
            &config,
            dt,
            &u,
            &v,
            &mut temperature,
        )
        .expect("scalar step");
        let peak = temperature
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(peak.is_finite());
        assert!(
            peak <= previous_peak + 1e-15,
            "peak grew from {previous_peak} to {peak}"
        );
        previous_peak = peak;
    }
    assert!(
        previous_peak < 1.0,
        "hot spot did not decay: {previous_peak}"
    );
    assert!(previous_peak > 0.0);
}

#[test]
fn solve_options_stop_on_loose_tolerance_and_report_strict() {
    let mut model = mesh_model(16, 16);
    model.time.steps = 200;
    model.time.pressure_iterations = 50;
    model.time.tolerance = 1e-3;

    let loose = solve_with(
        &model,
        &SolveOptions {
            steps: Some(200),
            tolerance: Some(1.0),
            check_every: Some(5),
        },
    )
    .expect("loose solve");
    assert!(loose.report.converged);
    assert!(loose.report.steady_state);
    assert!(
        loose.report.steps < 200,
        "stopped at {}",
        loose.report.steps
    );
    assert!(!loose.report.residual_history.is_empty());
    assert_eq!(
        loose
            .report
            .residual_history
            .last()
            .map(|sample| sample.step),
        Some(loose.report.steps)
    );

    let strict = solve_with(
        &model,
        &SolveOptions {
            steps: Some(3),
            tolerance: Some(1e-12),
            check_every: Some(1),
        },
    )
    .expect("strict solve");
    assert!(!strict.report.converged);
    assert!(!strict.report.steady_state);
    assert_eq!(strict.report.steps, 3);
    assert!(strict.report.residual > 1e-12);
}

#[test]
fn vorticity_and_stream_function_match_analytic_fields() {
    let mut model = Model::new();
    model.mesh.nx = 16;
    model.mesh.ny = 12;
    let mesh = Mesh::new(&model.mesh).expect("mesh");

    let mut u = vec![0.0; mesh.u_count()];
    let v = vec![0.0; mesh.v_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let (_, y) = mesh.u_position(i, j);
            u[mesh.u_index(i, j)] = y;
        }
    }

    let omega = vorticity(&mesh, &u, &v);
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let value = omega[mesh.pressure_index(i, j)];
            assert!(
                (value + 1.0).abs() < 1e-12,
                "Couette shear vorticity at ({i}, {j}) is {value}, expected -1"
            );
        }
    }

    let psi = stream_function(&mesh, &u, &v);
    for j in 0..mesh.ny - 1 {
        for i in 0..mesh.nx {
            let below = mesh.pressure_index(i, j);
            let above = mesh.pressure_index(i, j + 1);
            let (ux, _) = mesh.cell_velocity(&u, &v, i, j);
            let difference = psi[above] - psi[below];
            assert!(
                (difference - ux * mesh.dy).abs() < 1e-9,
                "stream function increment at ({i}, {j}) is {difference}, expected {}",
                ux * mesh.dy
            );
        }
    }
    let mean = psi.iter().sum::<f64>() / psi.len() as f64;
    assert!(mean.abs() < 1e-12, "stream function mean is {mean}");

    let mut rotating_u = vec![0.0; mesh.u_count()];
    let mut rotating_v = vec![0.0; mesh.v_count()];
    for j in 0..mesh.ny {
        for i in 0..=mesh.nx {
            let (_, y) = mesh.u_position(i, j);
            rotating_u[mesh.u_index(i, j)] = -y;
        }
    }
    for j in 0..=mesh.ny {
        for i in 0..mesh.nx {
            let (x, _) = mesh.v_position(i, j);
            rotating_v[mesh.v_index(i, j)] = x;
        }
    }
    let rotation = vorticity(&mesh, &rotating_u, &rotating_v);
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let value = rotation[mesh.pressure_index(i, j)];
            assert!(
                (value - 2.0).abs() < 1e-12,
                "solid-body vorticity at ({i}, {j}) is {value}, expected 2"
            );
        }
    }
}

#[test]
fn solve_reports_derived_fields_and_svg_export() {
    let mut model = mesh_model(16, 16);
    model.time.steps = 60;
    model.time.pressure_iterations = 60;
    let solution = solve(&model).expect("solve");

    assert_finite(&solution.vorticity, "vorticity");
    assert_finite(&solution.stream_function, "streamFunction");
    assert_eq!(solution.vorticity.len(), solution.mesh.pressure_count());
    assert_eq!(
        solution.stream_function.len(),
        solution.mesh.pressure_count()
    );
    let report = &solution.report;
    assert!(report.vorticity_min < report.vorticity_max);
    assert!(report.stream_function_min < report.stream_function_max);

    let fluid_mean = solution.stream_function.iter().sum::<f64>()
        / solution.mesh.solid.iter().filter(|solid| !**solid).count() as f64;
    assert!(
        fluid_mean.abs() < 1e-9,
        "stream function mean is {fluid_mean}"
    );

    let directory = temporary_dir("derived");
    let svg_path = export_svg(&solution, &directory, ExportField::Vorticity).expect("svg");
    let svg = fs::read_to_string(&svg_path).expect("read svg");
    assert!(svg.contains("OpenCFD vorticity field (1/s)"));
    assert!(svg.contains("range "));
}
