use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{
    export_csv, export_streamlines, export_svg, export_vtk, solve, solve_pressure, solve_with,
    stream_function, transport_scalar, vorticity, Boundary, BoundaryKind, ConvectionScheme,
    ExportField, Mesh, Model, Obstacle, PressureSettings, Probe, Reference, ScalarConfig, Solution,
    SolveOptions, CAPABILITIES_JSON, VERSION,
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
        pressure: 0.0,
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
    let balance = solution.report.mass_balance;
    assert_eq!(balance.inflow, 0.0);
    assert_eq!(balance.outflow, 0.0);
    assert_eq!(balance.net, 0.0);
    assert_eq!(balance.relative_error, 0.0);
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
        pressure: 0.0,
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [1.0, 0.0],
        pressure: 0.0,
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
    assert!(CAPABILITIES_JSON.contains("\"convectionSchemes\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"probes\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"wallShear\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"slipBoundary\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"pressureOutlet\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"momentCoefficients\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"massBalance\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"streamlines\": \"supported\""));
    assert!(CAPABILITIES_JSON.contains("\"periodic\": \"unsupported\""));
    assert!(CAPABILITIES_JSON.contains("\"contours\": \"unsupported\""));
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
        pressure: 0.0,
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
        pressure: 0.0,
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
    assert!(force.cm.is_finite() && force.mz.is_finite());
    assert!(force.cd > 0.0, "drag coefficient {}", force.cd);
    assert!(force.fx > 0.0, "drag force {}", force.fx);
    assert!(!force.history.is_empty());
    for sample in &force.history {
        assert!(sample.cm.is_finite() && sample.mz.is_finite());
    }
    assert!((force.reference_velocity - 1.0).abs() < 1e-12);
    assert!((force.reference_area - 0.2).abs() < 1e-12);
    assert!((force.reference_length - 0.2).abs() < 1e-12);

    let balance = solution.report.mass_balance;
    assert!(balance.inflow.is_finite() && balance.outflow.is_finite());
    assert!(balance.inflow > 0.0 && balance.outflow > 0.0);
    assert!(balance.relative_error < 0.25, "mass balance {balance:?}");
}

#[test]
fn reference_quantities_scale_force_and_moment_coefficients() {
    let mut model = Model::new();
    model.mesh.nx = 24;
    model.mesh.ny = 12;
    model.mesh.length_x = 2.0;
    model.mesh.length_y = 1.0;
    model.fluid.density = 1.0;
    model.fluid.viscosity = 0.02;
    model.time.dt = 0.005;
    model.time.steps = 120;
    model.time.pressure_iterations = 80;
    model.time.tolerance = 1e-3;
    model.boundaries.left = Boundary {
        kind: BoundaryKind::Inlet,
        velocity: [1.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.top = Boundary::wall();
    model.boundaries.bottom = Boundary::wall();
    model.obstacles = vec![Obstacle::Rect {
        id: "plate".to_string(),
        x0: 0.7,
        y0: 0.3,
        x1: 0.9,
        y1: 0.6,
    }];

    let base = solve(&model).expect("base solve");
    let base_force = &base.report.forces[0];
    assert!(
        base_force.cd.abs() > 1e-9,
        "base drag coefficient {}",
        base_force.cd
    );
    assert!(
        base_force.cm.abs() > 1e-9,
        "base moment coefficient is zero"
    );

    model.reference = Some(Reference {
        velocity: Some(2.0),
        area: Some(0.4),
        length: Some(0.4),
    });
    let scaled = solve(&model).expect("scaled solve");
    let scaled_force = &scaled.report.forces[0];
    assert!(
        (scaled_force.reference_velocity - 2.0).abs() < 1e-12,
        "reference velocity {}",
        scaled_force.reference_velocity
    );
    assert!((scaled_force.reference_area - 0.4).abs() < 1e-12);
    assert!((scaled_force.reference_length - 0.4).abs() < 1e-12);
    let drag_ratio = scaled_force.cd / base_force.cd;
    let expected_drag_ratio = base_force.reference_velocity.powi(2) * base_force.reference_area
        / (scaled_force.reference_velocity.powi(2) * scaled_force.reference_area);
    assert!(
        (drag_ratio - expected_drag_ratio).abs() < 1e-9,
        "drag ratio {drag_ratio}, expected {expected_drag_ratio}"
    );
    let moment_ratio = scaled_force.cm / base_force.cm;
    let expected_moment_ratio =
        expected_drag_ratio * base_force.reference_length / scaled_force.reference_length;
    assert!(
        (moment_ratio - expected_moment_ratio).abs() < 1e-9,
        "moment ratio {moment_ratio}, expected {expected_moment_ratio}"
    );
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
        transport_scalar(
            &mesh,
            &model.boundaries,
            &config,
            ConvectionScheme::Upwind,
            dt,
            &u,
            &v,
            &mut uniform,
        )
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
            ConvectionScheme::Upwind,
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
    for sample in &loose.report.residual_history {
        assert!(sample.residual.is_finite() && sample.residual >= 0.0);
        assert!(sample.continuity.is_finite() && sample.continuity >= 0.0);
    }
    assert_eq!(
        loose
            .report
            .residual_history
            .last()
            .map(|sample| sample.step),
        Some(loose.report.steps)
    );
    let value = serde_json::to_value(&loose.report).expect("report json");
    assert!(value.get("massBalance").is_some());
    assert!(value.get("residualHistory").is_some());

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

    let stream_path = export_streamlines(&solution, &directory).expect("streamlines");
    assert_eq!(
        stream_path.file_name().and_then(|name| name.to_str()),
        Some("solution-streamlines.svg")
    );
    let streamlines = fs::read_to_string(&stream_path).expect("read streamlines");
    assert!(streamlines.contains("OpenCFD streamlines"));
    assert!(
        streamlines.matches("<polyline").count() > 0,
        "streamline SVG has no polylines"
    );
    assert!(streamlines.contains("lines, maximum face speed"));
}

#[test]
fn scalar_front_sharpens_with_central_and_quick() {
    let mut model = mesh_model(24, 8);
    model.mesh.length_x = 2.0;
    model.mesh.length_y = 1.0;
    let mesh = Mesh::new(&model.mesh).expect("mesh");
    let config = ScalarConfig {
        diffusivity: 1e-4,
        initial: 0.0,
        inlet_value: 0.0,
    };
    let dt = 0.002;
    let u = vec![0.5; mesh.u_count()];
    let v = vec![0.0; mesh.v_count()];

    let mut fields = Vec::new();
    for scheme in [
        ConvectionScheme::Upwind,
        ConvectionScheme::Central,
        ConvectionScheme::Quick,
    ] {
        let mut temperature = vec![0.0; mesh.pressure_count()];
        for j in 0..mesh.ny {
            for i in 0..4 {
                temperature[mesh.pressure_index(i, j)] = 1.0;
            }
        }
        for _ in 0..200 {
            transport_scalar(
                &mesh,
                &model.boundaries,
                &config,
                scheme,
                dt,
                &u,
                &v,
                &mut temperature,
            )
            .expect("scalar step");
        }
        assert_finite(&temperature, scheme.as_str());
        let maximum = temperature
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let minimum = temperature.iter().copied().fold(f64::INFINITY, f64::min);
        // Upwind is monotone. Central and QUICK are dispersive and may
        // overshoot a sharp front, but must stay bounded over the run.
        let ceiling = if scheme == ConvectionScheme::Upwind {
            1.0 + 1e-12
        } else {
            1.5
        };
        assert!(
            maximum <= ceiling,
            "{} overshoots to {maximum}",
            scheme.as_str()
        );
        assert!(
            minimum >= -0.5,
            "{} undershoots to {minimum}",
            scheme.as_str()
        );
        fields.push(temperature);
    }

    let row = mesh.ny / 2;
    let thickness = |field: &[f64]| {
        (0..mesh.nx)
            .map(|i| {
                let value = field[mesh.pressure_index(i, row)];
                value * (1.0 - value)
            })
            .sum::<f64>()
    };
    let gradient = |field: &[f64]| {
        (0..mesh.nx - 1)
            .map(|i| {
                (field[mesh.pressure_index(i + 1, row)] - field[mesh.pressure_index(i, row)]).abs()
            })
            .fold(0.0_f64, f64::max)
    };

    // The interface thickness Σ T(1−T) is a robust diffusion measure: a
    // smeared upwind front spreads the transition over many cells, so its
    // thickness is several times larger than the central or QUICK fronts.
    let upwind_thickness = thickness(&fields[0]);
    let central_thickness = thickness(&fields[1]);
    let quick_thickness = thickness(&fields[2]);
    assert!(
        upwind_thickness > 2.0 * quick_thickness,
        "upwind thickness {upwind_thickness} should exceed 2x QUICK thickness {quick_thickness}"
    );
    assert!(
        upwind_thickness > 2.0 * central_thickness,
        "upwind thickness {upwind_thickness} should exceed 2x central thickness {central_thickness}"
    );

    // A sharper front also means a steeper maximum gradient.
    let upwind_gradient = gradient(&fields[0]);
    assert!(
        gradient(&fields[1]) > 1.5 * upwind_gradient,
        "central gradient {} should exceed 1.5x upwind {upwind_gradient}",
        gradient(&fields[1])
    );
    assert!(
        gradient(&fields[2]) > 1.5 * upwind_gradient,
        "QUICK gradient {} should exceed 1.5x upwind {upwind_gradient}",
        gradient(&fields[2])
    );
}

#[test]
fn slip_boundary_blocks_normal_flow_and_keeps_tangential_faces_free() {
    let mut model = Model::new();
    model.mesh.nx = 8;
    model.mesh.ny = 6;
    let mesh = Mesh::new(&model.mesh).expect("mesh");
    let boundaries = cfd_model::Boundaries {
        left: Boundary {
            kind: BoundaryKind::Slip,
            velocity: [5.0, 7.0],
            pressure: 0.0,
        },
        right: Boundary {
            kind: BoundaryKind::Outlet,
            velocity: [0.0, 0.0],
            pressure: 0.0,
        },
        top: Boundary {
            kind: BoundaryKind::Slip,
            velocity: [3.0, 4.0],
            pressure: 0.0,
        },
        ..Default::default()
    };

    let mut u = vec![9.0; mesh.u_count()];
    let mut v = vec![9.0; mesh.v_count()];
    mesh.apply_velocity_boundaries(&boundaries, &mut u, &mut v);

    for j in 0..mesh.ny {
        assert_eq!(u[mesh.u_index(0, j)], 0.0, "slip left normal velocity");
        assert_eq!(
            u[mesh.u_index(mesh.nx, j)],
            u[mesh.u_index(mesh.nx - 1, j)],
            "outlet copies the interior face"
        );
    }
    for i in 0..mesh.nx {
        assert_eq!(v[mesh.v_index(i, mesh.ny)], 0.0, "slip top normal velocity");
    }
    assert_eq!(u[mesh.u_index(4, 2)], 9.0, "interior x face untouched");
    assert_eq!(v[mesh.v_index(3, 3)], 9.0, "interior y face untouched");
    assert!(
        BoundaryKind::from_str("slip").expect("slip") == BoundaryKind::Slip,
        "`slip` must parse"
    );
    assert!(BoundaryKind::from_str("freeSlip").expect("freeSlip") == BoundaryKind::Slip);
}

#[test]
fn outlet_pressure_sets_the_reference_pressure() {
    let mut model = Model::new();
    model.mesh.nx = 8;
    model.mesh.ny = 8;
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
        pressure: 2.5,
    };
    let mesh = Mesh::new(&model.mesh).expect("mesh");
    let divergence = vec![0.0; mesh.pressure_count()];
    let mut pressure = vec![0.0; mesh.pressure_count()];
    let stats = solve_pressure(
        &mesh,
        &model.boundaries,
        &mut pressure,
        &divergence,
        1.0,
        0.01,
        &PressureSettings {
            iterations: 2000,
            tolerance: 1e-10,
        },
    )
    .expect("pressure solve");
    assert!(stats.converged, "pressure residual {}", stats.residual);
    for j in 0..mesh.ny {
        let value = pressure[mesh.pressure_index(mesh.nx - 1, j)];
        assert!(
            (value - 2.5).abs() < 1e-6,
            "outlet pressure at row {j} is {value}, expected 2.5"
        );
    }
    assert!((pressure[mesh.pressure_index(3, 3)] - 2.5).abs() < 1e-4);
}

#[test]
fn slip_channel_has_no_wall_shear_report() {
    let mut model = mesh_model(24, 12);
    model.mesh.length_x = 2.0;
    model.time.steps = 40;
    model.time.pressure_iterations = 60;
    model.boundaries.left = Boundary {
        kind: BoundaryKind::Inlet,
        velocity: [1.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.right = Boundary {
        kind: BoundaryKind::Outlet,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Slip,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.bottom = Boundary {
        kind: BoundaryKind::Slip,
        velocity: [0.0, 0.0],
        pressure: 0.0,
    };

    let solution = solve(&model).expect("slip channel");
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert!(
        solution.report.wall_shear.is_empty(),
        "slip sides must not report wall shear: {:?}",
        solution.report.wall_shear
    );
    let mesh = &solution.mesh;
    let mut vertical_variation = 0.0_f64;
    for j in 1..mesh.ny - 1 {
        let (middle, _) = mesh.cell_velocity(&solution.u, &solution.v, mesh.nx / 2, j);
        let (bottom, _) = mesh.cell_velocity(&solution.u, &solution.v, mesh.nx / 2, 0);
        vertical_variation = vertical_variation.max((middle - bottom).abs());
    }
    assert!(
        vertical_variation < 0.2,
        "slip channel profile varies by {vertical_variation} m/s across y"
    );
}

#[test]
fn probe_series_records_every_sample_point() {
    let mut model = mesh_model(16, 16);
    model.time.steps = 40;
    model.time.pressure_iterations = 60;
    model.probes = vec![Probe {
        id: "centre".to_string(),
        x: 0.5,
        y: 0.5,
    }];

    let solution = solve(&model).expect("solve");
    assert_eq!(solution.report.probes.len(), 1);
    let probe = &solution.report.probes[0];
    assert_eq!(probe.id, "centre");
    assert_eq!(probe.x, 0.5);
    assert_eq!(probe.cell, [8, 8]);
    assert!(!probe.history.is_empty());
    assert_eq!(
        probe.history.last().map(|sample| sample.step),
        Some(solution.report.steps)
    );
    for sample in &probe.history {
        assert!(sample.u.is_finite() && sample.v.is_finite());
        assert!(sample.pressure.is_finite());
        assert!(sample.temperature.is_none());
        assert!(sample.time > 0.0);
        assert!((sample.speed - sample.u.hypot(sample.v)).abs() < 1e-12);
    }
    let value = serde_json::to_value(&solution.report).expect("report json");
    assert!(value.get("probes").is_some());
    assert!(value.get("wallShear").is_some());
}

#[test]
fn probe_outside_the_domain_or_inside_an_obstacle_is_rejected() {
    let mut model = mesh_model(12, 12);
    model.probes = vec![Probe {
        id: "outside".to_string(),
        x: 1.5,
        y: 0.5,
    }];
    assert_eq!(model.validate().expect_err("outside").code, "probe/invalid");

    model.probes = vec![Probe {
        id: "blocked".to_string(),
        x: 0.5,
        y: 0.5,
    }];
    model.obstacles = vec![Obstacle::Rect {
        id: "block".to_string(),
        x0: 0.4,
        y0: 0.4,
        x1: 0.6,
        y1: 0.6,
    }];
    assert_eq!(model.validate().expect_err("blocked").code, "probe/invalid");
    assert_eq!(solve(&model).expect_err("solve").code, "probe/invalid");
}

#[test]
fn wall_shear_reports_moving_lid_drag_and_zero_when_at_rest() {
    let mut model = mesh_model(16, 16);
    model.time.steps = 80;
    model.time.pressure_iterations = 60;

    let solution = solve(&model).expect("solve");
    let shear = &solution.report.wall_shear;
    assert_eq!(shear.len(), 4, "all four sides are walls by default");
    for item in shear {
        assert!(item.mean_shear.is_finite());
        assert!(item.max_shear.is_finite());
        assert!(item.friction_coefficient.is_finite());
        assert!(item.max_shear >= item.mean_shear.abs() - 1e-12);
    }
    let find = |side: &str| {
        shear
            .iter()
            .find(|item| item.side == side)
            .unwrap_or_else(|| panic!("missing {side} wall shear"))
    };
    let top = find("top");
    let bottom = find("bottom");
    assert!(top.mean_shear > 0.0, "top mean shear {}", top.mean_shear);
    // The lid drives a clockwise vortex: the bottom wall retards a negative-x
    // flow, so it also exerts a positive-x shear on the fluid.
    assert!(
        bottom.mean_shear > 0.0,
        "bottom mean shear {}",
        bottom.mean_shear
    );
    assert!(top.friction_coefficient > 0.0);
    assert!(top.max_shear > 0.0);

    let mut still = mesh_model(16, 16);
    still.time.steps = 30;
    still.boundaries.top = Boundary::wall();
    let still_solution = solve(&still).expect("still solve");
    for item in &still_solution.report.wall_shear {
        assert!(item.mean_shear.abs() < 1e-12, "{} {:?}", item.side, item);
        assert!(item.max_shear.abs() < 1e-12);
        assert!(item.friction_coefficient.abs() < 1e-12);
    }
}

#[test]
fn convection_scheme_changes_the_cavity_solution_and_stays_stable() {
    let mut model = mesh_model(16, 16);
    model.fluid.viscosity = 0.05;
    model.time.dt = 0.002;
    model.time.steps = 120;
    model.time.pressure_iterations = 80;

    let mut solutions = Vec::new();
    for scheme in [
        ConvectionScheme::Upwind,
        ConvectionScheme::Central,
        ConvectionScheme::Quick,
    ] {
        model.convection = scheme;
        let solution = solve(&model).expect("solve");
        assert_finite(&solution.u, "u");
        assert_finite(&solution.v, "v");
        assert_eq!(solution.report.convection, scheme.as_str());
        assert!(
            solution.report.divergence_l2 < 1e-2,
            "{} divergence {}",
            scheme.as_str(),
            solution.report.divergence_l2
        );
        assert!(
            solution.report.max_velocity > 0.2 && solution.report.max_velocity <= 1.05,
            "{} max velocity {}",
            scheme.as_str(),
            solution.report.max_velocity
        );
        solutions.push(solution);
    }

    let difference = |left: &Solution, right: &Solution| {
        left.u
            .iter()
            .zip(right.u.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max)
    };
    assert!(
        difference(&solutions[0], &solutions[1]) > 1e-4,
        "central produced an identical velocity field to upwind"
    );
    assert!(
        difference(&solutions[0], &solutions[2]) > 1e-4,
        "QUICK produced an identical velocity field to upwind"
    );
}
