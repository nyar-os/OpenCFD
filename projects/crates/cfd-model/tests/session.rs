use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{
    BoundaryKind, ConvectionScheme, ExportField, ExportFormat, InletProfile, Model, OpenCfd,
    OpenCfdOptions, Probe, Side, TimeScheme,
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
