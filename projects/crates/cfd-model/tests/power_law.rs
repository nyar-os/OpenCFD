mod common {
    use std::fs;
    use std::path::PathBuf;

    pub fn temporary_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ocfd-power-law-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temporary directory");
        root
    }
}

use cfd_model::{
    solve, stability_limits, Boundary, BoundaryKind, Model, OpenCfd, OpenCfdOptions, ViscosityModel,
};
use common::temporary_dir;

fn cavity_model(viscosity: f64) -> Model {
    let mut model = Model::new();
    model.mesh.nx = 16;
    model.mesh.ny = 16;
    model.mesh.length_x = 1.0;
    model.mesh.length_y = 1.0;
    model.fluid.density = 1.0;
    model.fluid.viscosity = viscosity;
    model.time.dt = 0.01;
    model.time.steps = 20;
    model.time.pressure_iterations = 60;
    model.time.tolerance = 1e-12;
    model.boundaries.top = Boundary {
        kind: BoundaryKind::Wall,
        velocity: [1.0, 0.0],
        pressure: 0.0,
    };
    model.boundaries.bottom = Boundary::wall();
    model.boundaries.left = Boundary::wall();
    model.boundaries.right = Boundary::wall();
    model
}

fn power_law_model(viscosity: f64, consistency: f64, power: f64) -> Model {
    let mut model = cavity_model(viscosity);
    model.fluid.viscosity_model = ViscosityModel::PowerLaw;
    model.fluid.consistency = Some(consistency);
    model.fluid.power = Some(power);
    model
}

fn assert_finite(values: &[f64], label: &str) {
    assert!(
        values.iter().all(|value| value.is_finite()),
        "{label} contains NaN or Inf"
    );
}

#[test]
fn power_law_n_equals_one_reproduces_the_newtonian_report() {
    let viscosity = 0.05;
    let newtonian = solve(&cavity_model(viscosity)).expect("newtonian solve");
    let power_law = solve(&power_law_model(viscosity, viscosity, 1.0)).expect("power-law solve");

    assert_finite(&power_law.u, "u");
    assert_finite(&power_law.v, "v");
    assert_finite(&power_law.p, "p");

    let expected = newtonian.report.clone();
    let mut actual = power_law.report.clone();
    assert_eq!(expected.viscosity_model, "newtonian");
    assert_eq!(actual.viscosity_model, "powerLaw");
    assert_eq!(actual.apparent_viscosity_min, viscosity);
    assert_eq!(actual.apparent_viscosity_max, viscosity);
    assert_eq!(actual.apparent_viscosity_mean, viscosity);
    actual.viscosity_model = expected.viscosity_model.clone();
    assert_eq!(actual, expected, "n = 1 must match the Newtonian report");
    assert_eq!(power_law.u, newtonian.u);
    assert_eq!(power_law.v, newtonian.v);
    assert_eq!(power_law.p, newtonian.p);
}

#[test]
fn shear_thinning_lowers_viscosity_in_sheared_cells_and_stays_finite() {
    let model = power_law_model(0.05, 0.05, 0.7);
    let solution = solve(&model).expect("shear-thinning solve");
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
    assert_finite(&solution.p, "p");
    let report = &solution.report;
    assert_eq!(report.viscosity_model, "powerLaw");
    assert!(
        report.apparent_viscosity_min < report.apparent_viscosity_mean,
        "min {} mean {}",
        report.apparent_viscosity_min,
        report.apparent_viscosity_mean
    );
    assert!(
        report.apparent_viscosity_mean < report.apparent_viscosity_max,
        "mean {} max {}",
        report.apparent_viscosity_mean,
        report.apparent_viscosity_max
    );
    assert!(
        report.apparent_viscosity_min < 0.05,
        "sheared cells must fall below the consistency K, got {}",
        report.apparent_viscosity_min
    );
    assert!(report.apparent_viscosity_max.is_finite());
}

#[test]
fn viscosity_clamps_bound_the_reported_range() {
    let mut model = power_law_model(0.05, 0.05, 0.7);
    model.fluid.min_viscosity = Some(0.02);
    model.fluid.max_viscosity = Some(0.04);
    let solution = solve(&model).expect("clamped solve");
    let report = &solution.report;
    assert!(
        report.apparent_viscosity_min >= 0.02,
        "min {}",
        report.apparent_viscosity_min
    );
    assert!(
        report.apparent_viscosity_max <= 0.04,
        "max {}",
        report.apparent_viscosity_max
    );
    assert_finite(&solution.u, "u");
    assert_finite(&solution.v, "v");
}

#[test]
fn stability_report_uses_the_apparent_viscosity_range() {
    let model = power_law_model(0.05, 0.05, 0.7);
    let limits = stability_limits(&model).expect("stability");
    assert_eq!(limits.viscosity_model, "powerLaw");
    assert!(limits.apparent_viscosity_min.is_finite());
    assert!(limits.apparent_viscosity_max.is_finite());
    assert!(limits.apparent_viscosity_min <= limits.apparent_viscosity_max);
    assert!(limits.apparent_viscosity_min > 0.0);
    let cell_factor = 1.0 / (limits.dx * limits.dx) + 1.0 / (limits.dy * limits.dy);
    let expected = limits.apparent_viscosity_max * limits.dt * cell_factor;
    assert!(
        (limits.diffusion_number - expected).abs() <= 1e-12 * expected.abs().max(1.0),
        "diffusion number {} should use the maximum apparent viscosity (expected {expected})",
        limits.diffusion_number
    );

    let newtonian = stability_limits(&cavity_model(0.05)).expect("newtonian stability");
    assert_eq!(newtonian.viscosity_model, "newtonian");
    assert_eq!(newtonian.apparent_viscosity_min, 0.05);
    assert_eq!(newtonian.apparent_viscosity_max, 0.05);
    assert_eq!(newtonian.apparent_viscosity_mean, 0.05);
}

#[test]
fn invalid_viscosity_models_and_parameters_are_rejected() {
    let unknown = Model::parse_json(
        r#"{ "format": "open-cfd-case", "version": 1, "fluid": { "viscosityModel": "bogus" } }"#,
    )
    .expect("document parses");
    let error = unknown.validate().expect_err("unknown model");
    assert_eq!(error.code, "fluid/invalid-viscosity-model");

    for (consistency, power, min, max) in [
        (None, Some(0.7), None, None),
        (Some(0.0), Some(0.7), None, None),
        (Some(0.05), None, None, None),
        (Some(0.05), Some(0.0), None, None),
        (Some(0.05), Some(2.5), None, None),
        (Some(0.05), Some(0.7), Some(-0.1), None),
        (Some(0.05), Some(0.7), Some(0.2), Some(0.1)),
    ] {
        let mut model = Model::new();
        model.fluid.viscosity_model = ViscosityModel::PowerLaw;
        model.fluid.consistency = consistency;
        model.fluid.power = power;
        model.fluid.min_viscosity = min;
        model.fluid.max_viscosity = max;
        let error = model.validate().expect_err("invalid power law");
        assert_eq!(error.code, "fluid/invalid-viscosity");
    }
}

#[test]
fn failed_update_does_not_write_the_document() {
    let directory = temporary_dir("no-write");
    let path = directory.join("case.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let before = std::fs::read(&path).expect("read before");

    let error = session
        .fluid_set_with_model(
            1.0,
            0.05,
            ViscosityModel::PowerLaw,
            None,
            Some(0.7),
            None,
            None,
        )
        .expect_err("missing consistency");
    assert_eq!(error.code, "fluid/invalid-viscosity");
    assert_eq!(std::fs::read(&path).expect("read after"), before);

    let error = session
        .fluid_set_with_model(
            1.0,
            0.05,
            ViscosityModel::Unknown("bogus".to_string()),
            Some(0.05),
            Some(1.0),
            None,
            None,
        )
        .expect_err("unknown model");
    assert_eq!(error.code, "fluid/invalid-viscosity-model");
    assert_eq!(std::fs::read(&path).expect("read after unknown"), before);

    let summary = session
        .fluid_set_with_model(
            1.0,
            0.05,
            ViscosityModel::PowerLaw,
            Some(0.05),
            Some(0.8),
            Some(0.01),
            Some(0.1),
        )
        .expect("valid update");
    assert_eq!(summary.fluid.viscosity_model, ViscosityModel::PowerLaw);
    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.fluid.consistency, Some(0.05));
    assert_eq!(stored.fluid.power, Some(0.8));
    assert_eq!(stored.fluid.min_viscosity, Some(0.01));
    assert_eq!(stored.fluid.max_viscosity, Some(0.1));
}
