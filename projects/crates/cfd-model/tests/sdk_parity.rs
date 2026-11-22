//! Session-surface parity tests for features the SDK and CLI map one-to-one.
//!
//! The numerical behaviour of these models is covered by the integration tests;
//! these checks pin the editing surface the TypeScript SDK and `open-cfd` CLI
//! forward to: generated ids, document order, and the stable error codes.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cfd_model::{
    Model, OpenCfd, OpenCfdOptions, PorousRegion, SideThermal, ThermalConfig, WallFunction,
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

fn porous_region(id: &str) -> PorousRegion {
    PorousRegion {
        id: id.to_string(),
        x0: 0.2,
        y0: 0.2,
        x1: 0.8,
        y1: 0.8,
        permeability: 1.0e-3,
        inertial: 0.5,
    }
}

#[test]
fn porous_surface_generates_ids_and_persists_removal() {
    let directory = temporary_dir("surface-porous");
    let path = directory.join("porous.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");

    let summary = session.porous_add(porous_region("")).expect("porous add");
    assert_eq!(summary.porous.len(), 1);
    assert_eq!(summary.porous[0].id, "porous-1");
    assert_eq!(summary.porous[0].permeability, 1.0e-3);
    assert_eq!(summary.porous[0].inertial, 0.5);
    assert_eq!(session.porous_list(), summary.porous);

    let summary = session
        .porous_add(porous_region("filter"))
        .expect("second add");
    assert_eq!(summary.porous.len(), 2);
    assert_eq!(summary.porous[1].id, "filter");

    let error = session
        .porous_remove("missing")
        .expect_err("unknown region id");
    assert_eq!(error.code, "porous/not-found");
    assert_eq!(session.porous_list().len(), 2);

    let summary = session.porous_remove("porous-1").expect("remove first");
    assert_eq!(summary.porous.len(), 1);
    assert_eq!(summary.porous[0].id, "filter");

    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.porous, summary.porous);

    fs::remove_dir_all(directory).ok();
}

#[test]
fn porous_validation_stays_in_the_model() {
    let directory = temporary_dir("surface-porous-invalid");
    let path = directory.join("porous.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");
    let before = fs::read_to_string(&path).expect("read document before");

    let mut invalid = porous_region("bad");
    invalid.permeability = 0.0;
    let error = session.porous_add(invalid).expect_err("zero permeability");
    assert_eq!(error.code, "porous/invalid");

    let mut outside = porous_region("outside");
    outside.x1 = 2.0;
    let error = session.porous_add(outside).expect_err("outside domain");
    assert_eq!(error.code, "porous/invalid");

    let after = fs::read_to_string(&path).expect("read document after");
    assert_eq!(before, after, "a rejected edit must not touch the document");
    assert!(session.porous_list().is_empty());

    fs::remove_dir_all(directory).ok();
}

#[test]
fn thermal_surface_requires_scalar_and_persists_settings() {
    let directory = temporary_dir("surface-thermal");
    let path = directory.join("thermal.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");

    let error = session
        .thermal_set(ThermalConfig::default(), None)
        .expect_err("thermal without scalar");
    assert_eq!(error.code, "thermal/invalid");

    let thermal = ThermalConfig {
        specific_heat: 1000.0,
        conductivity: Some(0.6),
        heat_source: 2.0,
        left: SideThermal {
            temperature: Some(320.0),
            heat_flux: None,
        },
        ..ThermalConfig::default()
    };
    let summary = session
        .thermal_set(thermal.clone(), Some(1.0e-3))
        .expect("thermal with diffusivity");
    assert_eq!(
        summary.scalar.as_ref().map(|scalar| scalar.diffusivity),
        Some(1.0e-3)
    );
    assert_eq!(summary.thermal.as_ref(), Some(&thermal));

    let stored = Model::load(&path).expect("load");
    assert_eq!(stored.thermal.as_ref(), Some(&thermal));

    let error = session
        .thermal_set(
            ThermalConfig {
                specific_heat: 0.0,
                ..ThermalConfig::default()
            },
            None,
        )
        .expect_err("non-positive specific heat");
    assert_eq!(error.code, "thermal/invalid");
    assert_eq!(
        Model::load(&path).expect("reload").thermal.as_ref(),
        Some(&thermal)
    );

    fs::remove_dir_all(directory).ok();
}

#[test]
fn wall_function_surface_sets_validates_and_clears() {
    let directory = temporary_dir("surface-wall-function");
    let path = directory.join("wall.ocfd");
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.create(&path).expect("create");

    let settings = WallFunction {
        kappa: 0.4,
        intercept: 5.0,
        min_y_plus: 12.0,
    };
    let summary = session
        .wall_function_set(settings.kappa, settings.intercept, settings.min_y_plus)
        .expect("wall function");
    assert_eq!(summary.wall_function.as_ref(), Some(&settings));

    let error = session
        .wall_function_set(0.0, 5.0, 12.0)
        .expect_err("non-positive kappa");
    assert_eq!(error.code, "wallFunction/invalid");
    assert_eq!(
        session.summary().expect("summary").wall_function.as_ref(),
        Some(&settings)
    );

    let summary = session.wall_function_clear().expect("clear");
    assert!(summary.wall_function.is_none());
    assert!(Model::load(&path).expect("load").wall_function.is_none());

    fs::remove_dir_all(directory).ok();
}
