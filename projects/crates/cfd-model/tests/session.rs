use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{BoundaryKind, ExportFormat, Model, OpenCfd, OpenCfdOptions, Side};

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
        .boundary_set(Side::Top, BoundaryKind::Wall, Some(1.0), Some(0.0))
        .expect("boundary set");
    assert_eq!(summary.boundaries.top.velocity, [1.0, 0.0]);
    let summary = session.validate().expect("validate");
    assert_eq!(summary.time.steps, 5);

    let stored = Model::load(&session_path).expect("load saved document");
    assert_eq!(stored.mesh.nx, 12);
    assert_eq!(stored.time.pressure_iterations, 20);
    assert_eq!(stored.boundaries.top.velocity[0], 1.0);

    let report = session.solve().expect("solve");
    assert!(report.max_velocity.is_finite());
    assert!(report.max_velocity > 0.0);
    assert_eq!(report.steps, 5);

    let csv = session
        .export(ExportFormat::Csv, &directory.join("csv"))
        .expect("csv export");
    let vtk_path = directory.join("result.vtk");
    let vtk = session
        .export(ExportFormat::Vtk, &vtk_path)
        .expect("vtk export");
    assert!(PathBuf::from(&csv.path).is_file());
    assert!(PathBuf::from(&vtk.path).is_file());
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
