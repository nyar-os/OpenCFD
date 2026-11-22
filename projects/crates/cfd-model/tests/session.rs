use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{
    BoundaryKind, Checkpoint, ConvectionScheme, ExportField, ExportFormat, ExportLayout,
    InletProfile, Model, Obstacle, OpenCfd, OpenCfdOptions, ParticleConfig, ParticleSeed,
    PorousRegion, Probe, Side, SolveOptions, TimeScheme,
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

fn text(path: &std::path::Path) -> String {
    path.to_str()
        .expect("temporary paths are UTF-8")
        .to_string()
}

fn run_cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ocfd"))
        .args(args)
        .output()
        .expect("run ocfd")
}

fn assert_report_close(actual: &serde_json::Value, expected: &serde_json::Value) {
    for key in [
        "format",
        "version",
        "steps",
        "pressureIterations",
        "converged",
    ] {
        assert_eq!(actual.get(key), expected.get(key), "report field `{key}`");
    }
    for key in [
        "dt",
        "time",
        "maxVelocity",
        "minPressure",
        "maxPressure",
        "divergenceL2",
        "pressureResidual",
        "pressureResidualInitial",
    ] {
        let actual = actual[key].as_f64().expect("numeric report field");
        let expected = expected[key].as_f64().expect("numeric report field");
        assert!(
            (actual - expected).abs() <= 1e-9 * expected.abs().max(1.0),
            "report field `{key}`: {actual} vs {expected}"
        );
    }
}

#[test]
fn session_slip_and_outlet_pressure_round_trip() {
    let directory = temporary_dir("slip");
    let path = directory.join("slip.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .boundary_set(Side::Right, BoundaryKind::Outlet, None, None, Some(1.5))
        .expect("outlet");
    assert_eq!(summary.boundaries.right.kind, BoundaryKind::Outlet);
    assert_eq!(summary.boundaries.right.pressure, 1.5);
    let summary = session
        .boundary_set(Side::Top, BoundaryKind::Slip, None, None, None)
        .expect("slip");
    assert_eq!(summary.boundaries.top.kind, BoundaryKind::Slip);

    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.boundaries.right.kind, BoundaryKind::Outlet);
    assert_eq!(stored.boundaries.right.pressure, 1.5);
    assert_eq!(stored.boundaries.top.kind, BoundaryKind::Slip);

    let cli_path = directory.join("cli-slip.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "boundary",
        "set",
        &cli,
        "--side",
        "right",
        "--kind",
        "outlet",
        "--pressure",
        "1.5",
    ]);
    assert!(
        output.status.success(),
        "ocfd boundary set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(cli_stored.boundaries.right.pressure, 1.5);
}

#[test]
fn session_time_scheme_round_trips_through_cli() {
    let directory = temporary_dir("time-scheme");
    let path = directory.join("scheme.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .time_scheme_set(TimeScheme::Rk2)
        .expect("scheme set");
    assert_eq!(summary.time.scheme, TimeScheme::Rk2);
    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.time.scheme, TimeScheme::Rk2);

    let cli_path = directory.join("cli-scheme.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&["time", "scheme", "set", &cli, "--scheme", "rk2"]);
    assert!(
        output.status.success(),
        "ocfd time scheme set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(cli_stored.time.scheme, TimeScheme::Rk2);

    let output = run_cli(&["time", "scheme", "set", &cli, "--scheme", "bogus"]);
    assert!(!output.status.success(), "unknown scheme must be rejected");
}

#[test]
fn session_inlet_profile_round_trips_through_cli() {
    let directory = temporary_dir("inlet-profile");
    let path = directory.join("profile.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .boundary_profile_set(Side::Left, InletProfile::Parabolic, Some(5.0))
        .expect("profile set");
    assert_eq!(
        summary.boundaries.inlet_profiles.left,
        InletProfile::Parabolic
    );
    assert_eq!(summary.boundaries.inlet_profiles.exponent, 5.0);

    let stored = Model::load(&path).expect("load");
    assert_eq!(
        stored.boundaries.inlet_profiles.left,
        InletProfile::Parabolic
    );
    assert_eq!(stored.boundaries.inlet_profiles.exponent, 5.0);

    let cli_path = directory.join("cli-profile.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "boundary",
        "profile",
        "set",
        &cli,
        "--side",
        "left",
        "--profile",
        "powerLaw",
        "--exponent",
        "9",
    ]);
    assert!(
        output.status.success(),
        "ocfd boundary profile set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(
        cli_stored.boundaries.inlet_profiles.left,
        InletProfile::PowerLaw
    );
    assert_eq!(cli_stored.boundaries.inlet_profiles.exponent, 9.0);

    let output = run_cli(&[
        "boundary",
        "profile",
        "set",
        &cli,
        "--side",
        "left",
        "--profile",
        "parabolic",
        "--exponent",
        "0",
    ]);
    assert!(!output.status.success(), "zero exponent must be rejected");
}

#[test]
fn session_reference_quantities_round_trip_through_cli() {
    let directory = temporary_dir("reference");
    let path = directory.join("reference.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .reference_set(Some(1.2), Some(0.4), Some(0.2))
        .expect("reference set");
    let reference = summary.reference.expect("reference");
    assert_eq!(reference.velocity, Some(1.2));
    assert_eq!(reference.area, Some(0.4));
    assert_eq!(reference.length, Some(0.2));

    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.reference.expect("stored reference"), reference);

    let cli_path = directory.join("cli-reference.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "reference",
        "set",
        &cli,
        "--velocity",
        "0.8",
        "--area",
        "0.25",
    ]);
    assert!(
        output.status.success(),
        "ocfd reference set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    let reference = cli_stored.reference.expect("CLI reference");
    assert_eq!(reference.velocity, Some(0.8));
    assert_eq!(reference.area, Some(0.25));
    assert_eq!(reference.length, None);
}

#[test]
fn session_create_edit_solve_export_matches_cli() {
    let directory = temporary_dir("session");
    let session_path = directory.join("session.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());

    session.create(&session_path).expect("create");
    let summary = session.mesh_set(12, 10, 1.0, 1.0).expect("mesh set");
    assert_eq!(summary.mesh.nx, 12);
    assert_eq!(summary.mesh.ny, 10);
    session.fluid_set(1.0, 0.01).expect("fluid set");
    session.time_set(0.01, 5, 20, Some(1e-3)).expect("time set");
    let summary = session
        .boundary_set(Side::Top, BoundaryKind::Wall, Some(1.0), Some(0.0), None)
        .expect("boundary set");
    assert_eq!(summary.boundaries.top.velocity, [1.0, 0.0]);
    let summary = session.validate().expect("validate");
    assert_eq!(summary.time.steps, 5);
    let summary = session
        .convection_set(ConvectionScheme::Quick)
        .expect("convection set");
    assert_eq!(summary.convection.as_str(), "quick");
    let summary = session
        .probe_add(Probe {
            id: String::new(),
            x: 0.5,
            y: 0.5,
        })
        .expect("probe add");
    assert_eq!(summary.probes.len(), 1);
    assert_eq!(summary.probes[0].id, "probe-1");
    assert_eq!(session.probe_list().len(), 1);

    let stored = Model::load(&session_path).expect("load saved document");
    assert_eq!(stored.mesh.nx, 12);
    assert_eq!(stored.time.pressure_iterations, 20);
    assert_eq!(stored.boundaries.top.velocity[0], 1.0);
    assert_eq!(stored.convection.as_str(), "quick");

    let report = session.solve().expect("solve");
    assert!(report.max_velocity.is_finite());
    assert!(report.max_velocity > 0.0);
    assert_eq!(report.steps, 5);
    assert_eq!(report.convection, "quick");
    assert_eq!(report.probes.len(), 1);
    assert_eq!(report.probes[0].id, "probe-1");
    assert_eq!(report.probes[0].history.len(), 5);
    assert_eq!(report.wall_shear.len(), 4);

    let csv = session
        .export(ExportFormat::Csv, &directory.join("csv"))
        .expect("csv export");
    assert_eq!(csv.layout, "cell");
    let node_csv_path = directory.join("nodes.csv");
    let node_csv = session
        .export_field_with_layout(
            ExportFormat::Csv,
            &node_csv_path,
            ExportField::Pressure,
            ExportLayout::Node,
        )
        .expect("node csv export");
    assert_eq!(node_csv.layout, "node");
    let node_text = fs::read_to_string(&node_csv_path).expect("read node csv");
    assert_eq!(node_text.lines().count(), 13 * 11 + 1);
    let vtk_path = directory.join("result.vtk");
    let vtk = session
        .export(ExportFormat::Vtk, &vtk_path)
        .expect("vtk export");
    let svg_path = directory.join("vorticity.svg");
    let svg = session
        .export_field(ExportFormat::Svg, &svg_path, ExportField::Vorticity)
        .expect("svg export");
    let stream_path = directory.join("streamlines.svg");
    let streams = session
        .export(ExportFormat::Streamlines, &stream_path)
        .expect("streamline export");
    let contour_path = directory.join("contours.svg");
    let contours = session
        .export_field(ExportFormat::Contours, &contour_path, ExportField::Speed)
        .expect("contour export");
    assert!(PathBuf::from(&csv.path).is_file());
    assert!(PathBuf::from(&vtk.path).is_file());
    assert!(PathBuf::from(&svg.path).is_file());
    assert!(PathBuf::from(&streams.path).is_file());
    assert!(PathBuf::from(&contours.path).is_file());
    assert_eq!(contours.format, "contours");
    let residual_path = directory.join("residuals.csv");
    let residual = session
        .export(ExportFormat::Residuals, &residual_path)
        .expect("residual export");
    assert_eq!(residual.format, "residuals");
    assert!(fs::read_to_string(&residual_path)
        .expect("read residuals")
        .starts_with("step,residual,continuity\n"));
    let force_path = directory.join("forces.csv");
    let force = session
        .export(ExportFormat::Forces, &force_path)
        .expect("force export");
    assert_eq!(force.format, "forces");
    assert!(fs::read_to_string(&force_path)
        .expect("read forces")
        .starts_with("obstacle,kind,step,fx,fy,mz,cd,cl,cm\n"));
    let coefficient_path = directory.join("coefficients.csv");
    let coefficient = session
        .export(ExportFormat::Coefficients, &coefficient_path)
        .expect("coefficient export");
    assert_eq!(coefficient.format, "coefficients");
    assert!(fs::read_to_string(&coefficient_path)
        .expect("read coefficients")
        .starts_with(
            "obstacle,kind,cd,cl,cm,fx,fy,mz,referenceVelocity,referenceArea,referenceLength\n"
        ));
    let stream_text = fs::read_to_string(&stream_path).expect("read streamlines");
    assert!(stream_text.contains("OpenCFD streamlines"));
    assert!(stream_text.contains("<polyline"));
    let svg_text = fs::read_to_string(&svg_path).expect("read svg");
    assert!(svg_text.contains("OpenCFD vorticity field (1/s)"));
    let contour_text = fs::read_to_string(&contour_path).expect("read contours");
    assert!(contour_text.contains("OpenCFD speed contours (m/s)"));
    assert!(contour_text.contains("<polygon"));
    let vtk_text = fs::read_to_string(&vtk_path).expect("read vtk");
    assert!(vtk_text.contains("DATASET UNSTRUCTURED_GRID"));
    assert!(vtk_text.contains("POINT_DATA 120"));

    let cli_path = directory.join("cli.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    for args in [
        vec![
            "mesh",
            "set",
            cli.as_str(),
            "--nx",
            "12",
            "--ny",
            "10",
            "--length-x",
            "1",
            "--length-y",
            "1",
        ],
        vec!["fluid", "set", cli.as_str(), "--rho", "1", "--mu", "0.01"],
        vec!["convection", "set", cli.as_str(), "--scheme", "quick"],
        vec![
            "time",
            "set",
            cli.as_str(),
            "--dt",
            "0.01",
            "--steps",
            "5",
            "--pressure-iterations",
            "20",
            "--tolerance",
            "1e-3",
        ],
        vec![
            "boundary",
            "set",
            cli.as_str(),
            "--side",
            "top",
            "--kind",
            "wall",
            "--u",
            "1",
            "--v",
            "0",
        ],
        vec!["probe", "add", cli.as_str(), "--x", "0.5", "--y", "0.5"],
    ] {
        let output = run_cli(&args);
        assert!(
            output.status.success(),
            "ocfd {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let report_path = directory.join("report.json");
    let output = run_cli(&["solve", &text(&cli_path), "--out", &text(&report_path)]);
    assert!(
        output.status.success(),
        "ocfd solve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse CLI report");
    let session_report = serde_json::to_value(&report).expect("serialize session report");
    assert_report_close(&cli_report, &session_report);
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse written report");
    assert_report_close(&written, &session_report);

    let cli_csv_dir = directory.join("cli-csv");
    let output = run_cli(&[
        "export",
        &text(&cli_path),
        "--format",
        "csv",
        "--out",
        &text(&cli_csv_dir),
    ]);
    assert!(
        output.status.success(),
        "ocfd export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(cli_csv_dir.join("solution.csv")).expect("CLI csv"),
        fs::read_to_string(&csv.path).expect("session csv")
    );

    let cli_svg_path = directory.join("cli-stream-function.svg");
    let output = run_cli(&[
        "export",
        &text(&cli_path),
        "--format",
        "svg",
        "--field",
        "streamFunction",
        "--out",
        &text(&cli_svg_path),
    ]);
    assert!(
        output.status.success(),
        "ocfd svg export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_svg = fs::read_to_string(&cli_svg_path).expect("CLI svg");
    assert!(cli_svg.contains("OpenCFD streamFunction field (m²/s)"));

    let cli_stream_path = directory.join("cli-streamlines.svg");
    let output = run_cli(&[
        "export",
        &text(&cli_path),
        "--format",
        "streamlines",
        "--out",
        &text(&cli_stream_path),
    ]);
    assert!(
        output.status.success(),
        "ocfd streamlines export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stream = fs::read_to_string(&cli_stream_path).expect("CLI streamlines");
    assert!(cli_stream.contains("OpenCFD streamlines"));
    assert!(cli_stream.contains("<polyline"));

    let cli_contour_path = directory.join("cli-contours.svg");
    let output = run_cli(&[
        "export",
        &text(&cli_path),
        "--format",
        "contours",
        "--field",
        "speed",
        "--out",
        &text(&cli_contour_path),
    ]);
    assert!(
        output.status.success(),
        "ocfd contour export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_contour = fs::read_to_string(&cli_contour_path).expect("CLI contours");
    assert!(cli_contour.contains("OpenCFD speed contours (m/s)"));
    assert!(cli_contour.contains("<polygon"));

    let cli_residual_path = directory.join("cli-residuals.csv");
    let output = run_cli(&[
        "export",
        &text(&cli_path),
        "--format",
        "residuals",
        "--out",
        &text(&cli_residual_path),
    ]);
    assert!(
        output.status.success(),
        "ocfd residual export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_to_string(&cli_residual_path)
        .expect("CLI residuals")
        .starts_with("step,residual,continuity\n"));

    let output = run_cli(&["probe", "list", &text(&cli_path)]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("probe-1"));

    let summary = session.probe_clear().expect("probe clear");
    assert!(summary.probes.is_empty());
    assert!(session.probe_list().is_empty());
}

#[test]
fn session_save_and_create_refuse_existing_file() {
    let directory = temporary_dir("save");
    let original = directory.join("case.ocfd");
    let copy = directory.join("copy.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&original).expect("create");
    assert_eq!(
        session.create(&original).expect_err("exists").code,
        "file/exists"
    );

    let report = session.save(Some(&copy)).expect("save as");
    assert_eq!(report.path, text(&copy));
    assert!(copy.is_file());

    let mut reopened = OpenCfd::new(OpenCfdOptions::default());
    let summary = reopened.open(&copy).expect("open");
    assert_eq!(summary.path, text(&copy));
    assert_eq!(summary.mesh.nx, 32);

    let invalid = directory.join("invalid.ocfd");
    fs::write(
        &invalid,
        "{\"format\":\"open-cfd-case\",\"version\":1,\"mesh\":{\"nx\":2,\"ny\":2}}",
    )
    .expect("write invalid");
    let mut invalid_session = OpenCfd::new(OpenCfdOptions::default());
    invalid_session.open(&invalid).expect("open invalid");
    assert_eq!(
        invalid_session.validate().expect_err("invalid mesh").code,
        "mesh/invalid"
    );
}

#[test]
fn session_adaptive_time_setting_round_trips() {
    let directory = temporary_dir("adaptive");
    let case = directory.join("case.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&case).expect("create");

    let summary = session
        .time_adaptive_set(true, Some(0.4))
        .expect("enable adaptive");
    assert!(summary.time.adaptive);
    assert!((summary.time.target_cfl - 0.4).abs() < 1e-12);

    let mut reopened = OpenCfd::new(OpenCfdOptions::default());
    let summary = reopened.open(&case).expect("reopen");
    assert!(summary.time.adaptive);
    assert!((summary.time.target_cfl - 0.4).abs() < 1e-12);

    let summary = reopened
        .time_adaptive_set(false, None)
        .expect("disable adaptive");
    assert!(!summary.time.adaptive);
    assert!((summary.time.target_cfl - 0.4).abs() < 1e-12);
    assert_eq!(
        reopened
            .time_adaptive_set(true, Some(2.0))
            .expect_err("target CFL above 1")
            .code,
        "time/invalid"
    );
}

#[test]
fn session_pressure_gradient_round_trips_through_cli() {
    let directory = temporary_dir("pressure-gradient");
    let path = directory.join("gradient.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .pressure_gradient_set(-0.5, 0.25)
        .expect("pressure gradient");
    assert_eq!(summary.pressure_gradient, Some([-0.5, 0.25]));
    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.pressure_gradient, Some([-0.5, 0.25]));

    let cli_path = directory.join("cli-gradient.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "pressure", "gradient", "set", &cli, "--dpdx", "-0.5", "--dpdy", "0.25",
    ]);
    assert!(
        output.status.success(),
        "ocfd pressure gradient set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(cli_stored.pressure_gradient, Some([-0.5, 0.25]));

    let summary = session
        .pressure_gradient_set(0.0, 0.0)
        .expect("clear gradient");
    assert_eq!(summary.pressure_gradient, None);
    let stored = Model::load(&path).expect("reload");
    assert_eq!(stored.pressure_gradient, None);
}

#[test]
fn session_porous_region_round_trips_through_cli() {
    let directory = temporary_dir("porous");
    let path = directory.join("porous.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .porous_add(PorousRegion {
            id: String::new(),
            x0: 0.2,
            y0: 0.2,
            x1: 0.8,
            y1: 0.8,
            permeability: 1.0e-3,
            inertial: 0.5,
        })
        .expect("porous add");
    assert_eq!(summary.porous.len(), 1);
    assert_eq!(summary.porous[0].id, "porous-1");
    assert_eq!(session.porous_list().len(), 1);
    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.porous, summary.porous);

    let cli_path = directory.join("cli-porous.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "porous",
        "add",
        &cli,
        "--x0",
        "0.2",
        "--y0",
        "0.2",
        "--x1",
        "0.8",
        "--y1",
        "0.8",
        "--permeability",
        "0.001",
        "--inertial",
        "0.5",
        "--id",
        "filter",
    ]);
    assert!(
        output.status.success(),
        "ocfd porous add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(cli_stored.porous.len(), 1);
    assert_eq!(cli_stored.porous[0].id, "filter");
    assert_eq!(cli_stored.porous[0].permeability, 1.0e-3);
    assert_eq!(cli_stored.porous[0].inertial, 0.5);

    let listed = run_cli(&["porous", "list", &cli]);
    assert!(listed.status.success());
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(listing.contains("\"filter\""), "listing {listing}");
}

#[test]
fn session_obstacle_remove_keeps_the_remaining_obstacle() {
    let directory = temporary_dir("obstacle-remove");
    let path = directory.join("obstacles.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session
        .obstacle_add(Obstacle::Circle {
            id: "first".to_string(),
            x: 0.3,
            y: 0.5,
            r: 0.1,
        })
        .expect("first obstacle");
    session
        .obstacle_add(Obstacle::Rect {
            id: "second".to_string(),
            x0: 0.5,
            y0: 0.4,
            x1: 0.7,
            y1: 0.6,
            rotation: 0.0,
        })
        .expect("second obstacle");

    let summary = session.obstacle_remove("first").expect("remove first");
    assert_eq!(summary.obstacles.len(), 1);
    assert_eq!(summary.obstacles[0].id(), "second");

    let mut reopened = OpenCfd::new(OpenCfdOptions::default());
    let reloaded = reopened.open(&path).expect("reload");
    assert_eq!(reloaded.obstacles.len(), 1);
    assert_eq!(reloaded.obstacles[0].id(), "second");
}

#[test]
fn session_obstacle_remove_unknown_id_leaves_the_case_unchanged() {
    let directory = temporary_dir("obstacle-remove-missing");
    let path = directory.join("obstacles.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session
        .obstacle_add(Obstacle::Circle {
            id: "kept".to_string(),
            x: 0.3,
            y: 0.5,
            r: 0.1,
        })
        .expect("obstacle");
    let before = fs::read_to_string(&path).expect("read document before");

    let error = session
        .obstacle_remove("missing")
        .expect_err("unknown obstacle id");
    assert_eq!(error.code, "obstacle/not-found");
    assert!(error.message.contains("`missing`"), "{}", error.message);
    assert!(error.message.contains("`kept`"), "{}", error.message);

    assert_eq!(
        fs::read_to_string(&path).expect("read document after"),
        before
    );
    assert_eq!(session.obstacle_list().len(), 1);
}

#[test]
fn session_porous_remove_keeps_the_remaining_region() {
    let directory = temporary_dir("porous-remove");
    let path = directory.join("porous.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session
        .porous_add(PorousRegion {
            id: "first".to_string(),
            x0: 0.1,
            y0: 0.1,
            x1: 0.3,
            y1: 0.3,
            permeability: 1.0e-3,
            inertial: 0.0,
        })
        .expect("first region");
    session
        .porous_add(PorousRegion {
            id: "second".to_string(),
            x0: 0.6,
            y0: 0.6,
            x1: 0.8,
            y1: 0.8,
            permeability: 2.0e-3,
            inertial: 0.5,
        })
        .expect("second region");

    let summary = session.porous_remove("first").expect("remove first");
    assert_eq!(summary.porous.len(), 1);
    assert_eq!(summary.porous[0].id, "second");
    assert_eq!(summary.porous[0].permeability, 2.0e-3);

    let mut reopened = OpenCfd::new(OpenCfdOptions::default());
    let reloaded = reopened.open(&path).expect("reload");
    assert_eq!(reloaded.porous.len(), 1);
    assert_eq!(reloaded.porous[0].id, "second");

    let error = session
        .porous_remove("missing")
        .expect_err("unknown porous id");
    assert_eq!(error.code, "porous/not-found");
    assert!(error.message.contains("`missing`"), "{}", error.message);
    assert!(error.message.contains("`second`"), "{}", error.message);
    assert_eq!(session.porous_list().len(), 1);
}

#[test]
fn session_wall_function_round_trips_through_cli() {
    let directory = temporary_dir("wall-function");
    let path = directory.join("wall.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let summary = session
        .wall_function_set(0.4, 5.0, 12.0)
        .expect("wall function");
    let stored = summary.wall_function.clone().expect("configured");
    assert_eq!(stored.kappa, 0.4);
    assert_eq!(stored.intercept, 5.0);
    assert_eq!(stored.min_y_plus, 12.0);
    assert_eq!(
        Model::load(&path).expect("load").wall_function,
        summary.wall_function
    );

    let cli_path = directory.join("cli-wall.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "wall",
        "function",
        "set",
        &cli,
        "--kappa",
        "0.4",
        "--intercept",
        "5.0",
        "--min-y-plus",
        "12.0",
    ]);
    assert!(
        output.status.success(),
        "ocfd wall function set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli_stored = Model::load(&cli_path).expect("load CLI document");
    assert_eq!(cli_stored.wall_function, summary.wall_function);

    assert!(run_cli(&["wall", "function", "clear", &cli])
        .status
        .success());
    assert_eq!(Model::load(&cli_path).expect("reload").wall_function, None);
}

#[test]
fn session_buoyancy_requires_scalar_and_round_trips_through_cli() {
    let directory = temporary_dir("buoyancy");
    let path = directory.join("buoyancy.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    assert_eq!(
        session
            .buoyancy_set(0.2, 300.0, 0.0, -9.81)
            .expect_err("buoyancy without scalar")
            .code,
        "buoyancy/invalid"
    );
    session
        .scalar_set(1.0e-3, Some(300.0), Some(300.0))
        .expect("scalar");
    let summary = session
        .buoyancy_set(0.2, 300.0, 0.0, -9.81)
        .expect("buoyancy");
    assert_eq!(
        summary.buoyancy,
        Some(cfd_model::Buoyancy {
            beta: 0.2,
            t_ref: 300.0,
            gravity: [0.0, -9.81],
        })
    );
    assert_eq!(Model::load(&path).expect("load").buoyancy, summary.buoyancy);

    let cli_path = directory.join("cli-buoyancy.ocfd");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    assert!(run_cli(&["scalar", "set", &cli, "--diffusivity", "1e-3"])
        .status
        .success());
    let output = run_cli(&[
        "buoyancy", "set", &cli, "--beta", "0.2", "--t-ref", "300", "--gx", "0", "--gy", "-9.81",
    ]);
    assert!(
        output.status.success(),
        "ocfd buoyancy set failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        Model::load(&cli_path).expect("load CLI document").buoyancy,
        summary.buoyancy
    );

    let summary = session
        .buoyancy_set(0.0, 300.0, 0.0, -9.81)
        .expect("clear buoyancy");
    assert_eq!(summary.buoyancy, None);
}

#[test]
fn session_and_cli_report_the_same_mesh_quality() {
    let directory = temporary_dir("quality");
    let path = directory.join("quality.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session.mesh_set(10, 5, 1.0, 1.0).expect("mesh");
    let quality = session.mesh_quality().expect("quality");
    assert_eq!(quality.cells, 50);
    assert_eq!(quality.fluid_cells, 50);
    assert!((quality.max_aspect_ratio - 2.0).abs() < 1e-12);
    assert!(quality.max_non_orthogonality < 1e-9);

    let cli = text(&path);
    let output = run_cli(&["quality", &cli]);
    assert!(
        output.status.success(),
        "ocfd quality failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let decoded: serde_json::Value = serde_json::from_slice(&output.stdout).expect("quality JSON");
    assert_eq!(decoded["cells"].as_u64(), Some(50));
    assert_eq!(decoded["fluidCells"].as_u64(), Some(50));
    assert!((decoded["maxAspectRatio"].as_f64().expect("ratio") - 2.0).abs() < 1e-12);
}

#[test]
fn session_and_cli_export_particle_trajectories() {
    let directory = temporary_dir("particles");
    let path = directory.join("particles.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session.time_set(0.01, 5, 30, None).expect("time");
    let config = ParticleConfig {
        seeds: vec![ParticleSeed { x: 0.4, y: 0.4 }],
        dt: 0.0,
        max_steps: 3,
    };
    let report = session
        .export_particles(&directory.join("session"), &config)
        .expect("session export");
    assert_eq!(report.format, "particles");
    assert!(std::path::Path::new(&report.path).exists());
    let csv = fs::read_to_string(&report.path).expect("read particles");
    assert!(csv.starts_with("particle,step,time,x,y,u,v,speed\n"));
    assert!(csv.contains("particle-1,"));

    let cli = text(&path);
    let out = text(&directory.join("cli"));
    let output = run_cli(&[
        "export",
        &cli,
        "--format",
        "particles",
        "--seed",
        "0.4,0.4",
        "--particle-steps",
        "3",
        "--out",
        &out,
    ]);
    assert!(
        output.status.success(),
        "ocfd particle export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(directory.join("cli").join("particles.csv").exists());
}

#[test]
fn session_and_cli_report_outlet_allocation() {
    let directory = temporary_dir("outlets");
    let path = directory.join("outlets.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session.mesh_set(12, 6, 1.0, 1.0).expect("mesh");
    session.time_set(0.01, 6, 60, None).expect("time");
    session
        .boundary_set(Side::Left, BoundaryKind::Inlet, Some(1.0), Some(0.0), None)
        .expect("inlet");
    session
        .boundary_set(Side::Right, BoundaryKind::Outlet, None, None, Some(0.0))
        .expect("outlet");
    session
        .boundary_set(Side::Top, BoundaryKind::Wall, Some(0.0), Some(0.0), None)
        .expect("top wall");
    session
        .boundary_set(Side::Bottom, BoundaryKind::Wall, Some(0.0), Some(0.0), None)
        .expect("bottom wall");
    let report = session.solve().expect("solve");
    assert_eq!(report.outlets.len(), 1);
    assert_eq!(report.outlets[0].side, "right");
    assert!((report.outlets[0].share - 1.0).abs() < 1e-12);

    let cli = text(&path);
    let output = run_cli(&["solve", &cli]);
    assert!(
        output.status.success(),
        "ocfd solve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let decoded: serde_json::Value = serde_json::from_slice(&output.stdout).expect("solve JSON");
    let outlets = decoded["outlets"].as_array().expect("outlets array");
    assert_eq!(outlets.len(), 1);
    assert_eq!(outlets[0]["side"], "right");
    assert_eq!(outlets[0]["share"].as_f64(), Some(1.0));
}

#[test]
fn session_and_cli_restart_from_checkpoints() {
    let directory = temporary_dir("restart");
    let path = directory.join("restart.ocfd");
    let first = directory.join("first.json");
    let second = directory.join("second.json");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    session.time_set(0.01, 10, 30, None).expect("time settings");

    let options = SolveOptions {
        steps: Some(4),
        ..SolveOptions::default()
    };
    let report = session
        .solve_with_checkpoint(&options, None, Some(&first))
        .expect("first run");
    assert_eq!(report.steps, 4);
    let checkpoint = Checkpoint::load(&first).expect("load checkpoint");
    assert_eq!(checkpoint.steps, 4);

    let report = session
        .solve_with_checkpoint(&options, Some(&first), Some(&second))
        .expect("resumed run");
    assert_eq!(report.steps, 8);
    let checkpoint = Checkpoint::load(&second).expect("load second checkpoint");
    assert_eq!(checkpoint.steps, 8);

    let cli_path = directory.join("cli-restart.ocfd");
    let cli_first = directory.join("cli-first.json");
    let cli_second = directory.join("cli-second.json");
    let cli = text(&cli_path);
    assert!(run_cli(&["create", &cli]).status.success());
    let output = run_cli(&[
        "solve",
        &cli,
        "--steps",
        "3",
        "--checkpoint",
        &text(&cli_first),
    ]);
    assert!(
        output.status.success(),
        "ocfd solve --checkpoint failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = run_cli(&[
        "solve",
        &cli,
        "--steps",
        "2",
        "--restart",
        &text(&cli_first),
        "--checkpoint",
        &text(&cli_second),
    ]);
    assert!(
        output.status.success(),
        "ocfd solve --restart failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let checkpoint = Checkpoint::load(&cli_second).expect("load CLI checkpoint");
    assert_eq!(checkpoint.steps, 5);
}
