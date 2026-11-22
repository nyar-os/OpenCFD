use std::collections::{HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};
use std::process;

use cfd_model::{
    write_atomic, BoundaryKind, CaseSummary, CfdError, ConvectionScheme, ExportField, ExportFormat,
    ExportLayout, InletProfile, Obstacle, OpenCfd, OpenCfdOptions, ParticleConfig, ParticleSeed,
    PorousRegion, Probe, ScalarSource, Side, SideThermal, SolveOptions, ThermalConfig, TimeScheme,
    TimeSeries, TransientInlet, TurbulenceModel, WallFunction, DEFAULT_PARTICLE_STEPS,
};

const USAGE: &str = "\
ocfd - OpenCFD two-dimensional incompressible Navier-Stokes solver

Usage:
  ocfd create <file.ocfd>
  ocfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
  ocfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
  ocfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
  ocfd time scheme set <file> --scheme euler|rk2
  ocfd time adaptive set <file> --cfl <n|off>
  ocfd boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|convective|slip|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]
  ocfd boundary list <file>
  ocfd boundary reset <file> [--side left|right|top|bottom]
  ocfd boundary profile set <file> --side left|right|top|bottom --profile uniform|parabolic|powerLaw [--exponent <n>]
  ocfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>] [--id <name>]
  ocfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]
  ocfd obstacle update <file> --id <name> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>]
  ocfd obstacle update <file> --id <name> --kind circle --x <m> --y <m> --r <m>
  ocfd obstacle list <file>
  ocfd obstacle remove <file> --id <name>
  ocfd porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --permeability <m2> [--inertial <n>] [--id <name>]
  ocfd porous list <file>
  ocfd porous remove <file> --id <name>
  ocfd probe add <file> --x <m> --y <m> [--id <name>]
  ocfd probe update <file> --id <name> --x <m> --y <m>
  ocfd probe remove <file> --id <name>
  ocfd probe list <file>
  ocfd probe clear <file>
  ocfd inlet series set <file> --side left|right|top|bottom [--u <t:v,...>] [--v <t:v,...>] [--pressure <t:v,...>]
  ocfd inlet series list <file>
  ocfd inlet series clear <file>
  ocfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]
  ocfd scalar source add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --rate <unit/s> [--start <s>] [--ramp <s>] [--id <name>]
  ocfd scalar source list <file>
  ocfd scalar source clear <file>
  ocfd buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>
  ocfd convection set <file> --scheme upwind|central|quick
  ocfd pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>
  ocfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]
  ocfd turbulence set <file> --model none|constant|mixingLength|kEpsilon [--nu-t <m2/s>] [--mixing-length <m>] [--k <m2/s2>] [--epsilon <m2/s3>]
  ocfd heat set <file> [--conductivity <W/mK>] [--specific-heat <J/kgK>] [--diffusivity <m2/s>]
       [--heat-source <W/m3>] [--left-temperature <K>] [--right-temperature <K>]
       [--top-temperature <K>] [--bottom-temperature <K>]
       [--left-heat-flux <W/m2>] [--right-heat-flux <W/m2>] [--top-heat-flux <W/m2>]
       [--bottom-heat-flux <W/m2>] [--obstacle-temperature <K>] [--obstacle-heat-flux <W/m2>]
       [--reference-temperature <K>]
  ocfd wall function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]
  ocfd wall function clear <file>
  ocfd validate <file>
  ocfd quality <file>
  ocfd reynolds <file>
  ocfd stability <file>
  ocfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]
  ocfd export <file> --format csv|vtk|svg|contours|streamlines|residuals|forces|coefficients|probes|particles [--field pressure|temperature|vorticity|streamFunction|speed|turbulentKineticEnergy|turbulentDissipation] [--layout cell|node] [--seed <x,y>[;<x,y>...]] [--particle-steps <n>] --out <dir|file>
  ocfd capabilities
  ocfd --help

Exit codes: 0 success, 1 input/validation/IO error, 2 capability not implemented
";

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let exit_code = match dispatch(&args) {
        Ok(code) => code,
        Err(error) => {
            let payload = serde_json::to_string_pretty(&error)
                .unwrap_or_else(|_| format!("{{\"code\":\"{}\"}}", error.code));
            eprintln!("{payload}");
            error.exit_code()
        }
    };
    process::exit(exit_code);
}

fn dispatch(args: &[String]) -> Result<i32, CfdError> {
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return Ok(0);
    };
    match command {
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(0)
        }
        "capabilities" => {
            let session = OpenCfd::new(OpenCfdOptions::default());
            println!("{}", session.capabilities());
            Ok(0)
        }
        "create" => create(&args[1..]),
        "mesh" => subcommand(args, "set", "mesh"),
        "fluid" => subcommand(args, "set", "fluid"),
        "time" => time_command(&args[1..]),
        "boundary" => boundary_command(&args[1..]),
        "obstacle" => obstacle_command(&args[1..]),
        "porous" => porous_command(&args[1..]),
        "probe" => probe_command(&args[1..]),
        "inlet" => inlet_command(&args[1..]),
        "scalar" => scalar_command(&args[1..]),
        "buoyancy" => subcommand(args, "set", "buoyancy"),
        "convection" => convection_command(&args[1..]),
        "pressure" => pressure_command(&args[1..]),
        "reference" => subcommand(args, "set", "reference"),
        "turbulence" => turbulence_command(&args[1..]),
        "heat" | "thermal" => heat_command(&args[1..]),
        "wall" => wall_command(&args[1..]),
        "validate" => validate(&args[1..]),
        "quality" => quality(&args[1..]),
        "reynolds" => reynolds(&args[1..]),
        "stability" => stability(&args[1..]),
        "solve" => solve_command(&args[1..]),
        "export" => export(&args[1..]),
        "compressible" => Err(not_implemented("compressible flow")),
        "3d" | "threeD" => Err(not_implemented("three-dimensional flow")),
        "import" => Err(not_implemented("external format import")),
        other => Err(CfdError::new(
            "cli/unknown-command",
            format!("unknown command `{other}`; run --help for usage"),
        )),
    }
}

fn subcommand(args: &[String], expected: &str, group: &str) -> Result<i32, CfdError> {
    match args.get(1).map(String::as_str) {
        Some(action) if action == expected => match (group, action) {
            ("mesh", "set") => mesh_set(&args[2..]),
            ("fluid", "set") => fluid_set(&args[2..]),
            ("reference", "set") => reference_set(&args[2..]),
            ("buoyancy", "set") => buoyancy_set(&args[2..]),
            _ => Err(CfdError::new(
                "cli/unknown-command",
                format!("unknown command `{group} {action}`"),
            )),
        },
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`{group}` has no subcommand `{other}`; only `{expected}` is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            format!("`{group}` requires a subcommand; only `{expected}` is supported"),
        )),
    }
}

fn open_session(file: &str) -> Result<OpenCfd, CfdError> {
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    session.open(Path::new(file))?;
    Ok(session)
}

fn create(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "create <file.ocfd>")?;
    let path = PathBuf::from(file);
    let mut session = OpenCfd::new(OpenCfdOptions::default());
    let summary = session.create(&path)?;
    println!(
        "created {}: {}×{} mesh, lid velocity 1 m/s, {} steps, dt={} s",
        path.display(),
        summary.mesh.nx,
        summary.mesh.ny,
        summary.time.steps,
        summary.time.dt
    );
    Ok(0)
}

fn mesh_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--nx", "--ny", "--length-x", "--length-y"], &[])?;
    let file = positional(
        &parsed,
        0,
        "mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>",
    )?;
    let nx = required_usize(&parsed, "--nx")?;
    let ny = required_usize(&parsed, "--ny")?;
    let length_x = required_number(&parsed, "--length-x")?;
    let length_y = required_number(&parsed, "--length-y")?;
    let mut session = open_session(file)?;
    let summary = session.mesh_set(nx, ny, length_x, length_y)?;
    println!(
        "mesh set: {}×{}, {}×{} m",
        summary.mesh.nx, summary.mesh.ny, summary.mesh.length_x, summary.mesh.length_y
    );
    Ok(0)
}

fn fluid_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--rho", "--mu"], &[])?;
    let file = positional(&parsed, 0, "fluid set <file> --rho <kg/m3> --mu <Pa.s>")?;
    let density = required_number(&parsed, "--rho")?;
    let viscosity = required_number(&parsed, "--mu")?;
    let mut session = open_session(file)?;
    let summary = session.fluid_set(density, viscosity)?;
    println!(
        "fluid set: rho={} kg/m3, mu={} Pa.s",
        summary.fluid.density, summary.fluid.viscosity
    );
    Ok(0)
}

fn time_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => time_set(&args[1..]),
        Some("scheme") => match args.get(1).map(String::as_str) {
            Some("set") => time_scheme_set(&args[2..]),
            Some(other) => Err(CfdError::new(
                "cli/unknown-command",
                format!("`time scheme` has no subcommand `{other}`; only set is supported"),
            )),
            None => Err(CfdError::new(
                "cli/missing-argument",
                "`time scheme` requires a subcommand; only set is supported",
            )),
        },
        Some("adaptive") => match args.get(1).map(String::as_str) {
            Some("set") => time_adaptive_set(&args[2..]),
            Some(other) => Err(CfdError::new(
                "cli/unknown-command",
                format!("`time adaptive` has no subcommand `{other}`; only set is supported"),
            )),
            None => Err(CfdError::new(
                "cli/missing-argument",
                "`time adaptive` requires a subcommand; only set is supported",
            )),
        },
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!(
                "`time` has no subcommand `{other}`; only set, scheme and adaptive are supported"
            ),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`time` requires a subcommand; only set, scheme and adaptive are supported",
        )),
    }
}

fn time_scheme_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--scheme"], &[])?;
    let file = positional(&parsed, 0, "time scheme set <file> --scheme euler|rk2")?;
    let scheme = required(&parsed, "--scheme")?.parse::<TimeScheme>()?;
    let mut session = open_session(file)?;
    let summary = session.time_scheme_set(scheme)?;
    println!("time scheme set: {}", summary.time.scheme.as_str());
    Ok(0)
}

fn time_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &["--dt", "--steps", "--pressure-iterations", "--tolerance"],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]",
    )?;
    let dt = required_number(&parsed, "--dt")?;
    let steps = required_usize(&parsed, "--steps")?;
    let pressure_iterations = required_usize(&parsed, "--pressure-iterations")?;
    let tolerance = number(&parsed, "--tolerance")?;
    let mut session = open_session(file)?;
    let summary = session.time_set(dt, steps, pressure_iterations, tolerance)?;
    println!(
        "time set: dt={} s, steps={}, pressureIterations={}, relative tolerance={}",
        summary.time.dt,
        summary.time.steps,
        summary.time.pressure_iterations,
        summary.time.tolerance
    );
    Ok(0)
}

fn time_adaptive_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--cfl"], &[])?;
    let file = positional(&parsed, 0, "time adaptive set <file> --cfl <n|off>")?;
    let raw = required(&parsed, "--cfl")?;
    let (enabled, target) = if raw.eq_ignore_ascii_case("off") {
        (false, None)
    } else {
        let cfl = raw.parse::<f64>().map_err(|_| {
            CfdError::new(
                "cli/invalid-number",
                format!("--cfl must be a number in (0, 1] or `off`, got `{raw}`"),
            )
        })?;
        (true, Some(cfl))
    };
    let mut session = open_session(file)?;
    let summary = session.time_adaptive_set(enabled, target)?;
    println!(
        "time adaptive set: adaptive={}, targetCfl={}",
        summary.time.adaptive, summary.time.target_cfl
    );
    Ok(0)
}

fn boundary_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => boundary_set(&args[1..]),
        Some("list") => boundary_list(&args[1..]),
        Some("reset") => boundary_reset(&args[1..]),
        Some("profile") => match args.get(1).map(String::as_str) {
            Some("set") => boundary_profile_set(&args[2..]),
            Some(other) => Err(CfdError::new(
                "cli/unknown-command",
                format!("`boundary profile` has no subcommand `{other}`; only set is supported"),
            )),
            None => Err(CfdError::new(
                "cli/missing-argument",
                "`boundary profile` requires a subcommand; only set is supported",
            )),
        },
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`boundary` has no subcommand `{other}`; use set, list, reset or profile"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`boundary` requires a subcommand; use set, list, reset or profile",
        )),
    }
}

fn boundary_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "boundary list <file>")?;
    let session = open_session(file)?;
    let boundaries = session.boundary_list();
    let json = serde_json::to_string_pretty(&boundaries).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize boundaries: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn boundary_reset(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--side"], &[])?;
    let file = positional(&parsed, 0, "boundary reset <file> [--side <side>]")?;
    let side = parsed
        .options
        .get("--side")
        .map(|value| value.parse::<Side>())
        .transpose()?;
    let mut session = open_session(file)?;
    session.boundary_reset(side)?;
    match side {
        Some(side) => println!(
            "reset {} boundary to a stationary wall; dropped its transient drive",
            side.as_str()
        ),
        None => println!("reset all boundaries to stationary walls; dropped all transient drives"),
    }
    Ok(0)
}

fn boundary_profile_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--side", "--profile", "--exponent"], &[])?;
    let file = positional(
        &parsed,
        0,
        "boundary profile set <file> --side left|right|top|bottom --profile uniform|parabolic|powerLaw [--exponent <n>]",
    )?;
    let side = required(&parsed, "--side")?.parse::<Side>()?;
    let profile = required(&parsed, "--profile")?.parse::<InletProfile>()?;
    let exponent = number(&parsed, "--exponent")?;
    let mut session = open_session(file)?;
    let summary = session.boundary_profile_set(side, profile, exponent)?;
    println!(
        "{} inlet profile set to {} (power-law exponent {})",
        side.as_str(),
        summary.boundaries.inlet_profiles.get(side).as_str(),
        summary.boundaries.inlet_profiles.exponent
    );
    Ok(0)
}

fn boundary_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--side", "--kind", "--u", "--v", "--pressure"], &[])?;
    let file = positional(
        &parsed,
        0,
        "boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|convective|slip|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]",
    )?;
    let side = required(&parsed, "--side")?.parse::<Side>()?;
    let kind = required(&parsed, "--kind")?.parse::<BoundaryKind>()?;
    let u = number(&parsed, "--u")?;
    let v = number(&parsed, "--v")?;
    let pressure = number(&parsed, "--pressure")?;
    let mut session = open_session(file)?;
    let summary = session.boundary_set(side, kind, u, v, pressure)?;
    print_boundary(&summary, side);
    Ok(0)
}

fn print_boundary(summary: &CaseSummary, side: Side) {
    let boundary = summary.boundaries.get(side);
    println!(
        "{} boundary set to {}, velocity=[{}, {}] m/s, pressure={} Pa",
        side.as_str(),
        boundary.kind.as_str(),
        boundary.velocity[0],
        boundary.velocity[1],
        boundary.pressure
    );
}

fn reference_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--velocity", "--area", "--length"], &[])?;
    let file = positional(
        &parsed,
        0,
        "reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]",
    )?;
    let velocity = number(&parsed, "--velocity")?;
    let area = number(&parsed, "--area")?;
    let length = number(&parsed, "--length")?;
    if velocity.is_none() && area.is_none() && length.is_none() {
        return Err(CfdError::new(
            "cli/missing-option",
            "reference set needs at least one of --velocity, --area or --length",
        ));
    }
    let mut session = open_session(file)?;
    let summary = session.reference_set(velocity, area, length)?;
    let reference = summary
        .reference
        .as_ref()
        .expect("reference was just configured");
    println!(
        "reference set: velocity={:?} m/s, area={:?} m2, length={:?} m",
        reference.velocity, reference.area, reference.length
    );
    Ok(0)
}

fn obstacle_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("add") => obstacle_add(&args[1..]),
        Some("update") => obstacle_update(&args[1..]),
        Some("list") => obstacle_list(&args[1..]),
        Some("remove") => obstacle_remove(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!(
                "`obstacle` has no subcommand `{other}`; only add, update, remove and list are supported"
            ),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`obstacle` requires a subcommand; only add, update, remove and list are supported",
        )),
    }
}

fn obstacle_add(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--kind",
            "--id",
            "--x0",
            "--y0",
            "--x1",
            "--y1",
            "--rotation",
            "--x",
            "--y",
            "--r",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "obstacle add <file> --kind rect|circle ... [--id <name>]",
    )?;
    let obstacle = parse_obstacle(&parsed)?;
    let mut session = open_session(file)?;
    let summary = session.obstacle_add(obstacle)?;
    let added = summary
        .obstacles
        .last()
        .expect("obstacle was just appended");
    println!(
        "added {} obstacle `{}`; {} total",
        added.kind(),
        added.id(),
        summary.obstacles.len()
    );
    Ok(0)
}

fn obstacle_update(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--kind",
            "--id",
            "--x0",
            "--y0",
            "--x1",
            "--y1",
            "--rotation",
            "--x",
            "--y",
            "--r",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "obstacle update <file> --id <name> --kind rect|circle ...",
    )?;
    let id = required(&parsed, "--id")?.to_string();
    let obstacle = parse_obstacle(&parsed)?;
    let mut session = open_session(file)?;
    let summary = session.obstacle_update(&id, obstacle)?;
    let updated = summary
        .obstacles
        .iter()
        .find(|obstacle| obstacle.id() == id)
        .expect("obstacle was just replaced");
    println!(
        "updated {} obstacle `{}`; {} total",
        updated.kind(),
        updated.id(),
        summary.obstacles.len()
    );
    Ok(0)
}

fn parse_obstacle(parsed: &Parsed) -> Result<Obstacle, CfdError> {
    let id = parsed.options.get("--id").cloned().unwrap_or_default();
    match required(parsed, "--kind")? {
        "rect" => Ok(Obstacle::Rect {
            id,
            x0: required_number(parsed, "--x0")?,
            y0: required_number(parsed, "--y0")?,
            x1: required_number(parsed, "--x1")?,
            y1: required_number(parsed, "--y1")?,
            rotation: number(parsed, "--rotation")?.unwrap_or(0.0),
        }),
        "circle" => Ok(Obstacle::Circle {
            id,
            x: required_number(parsed, "--x")?,
            y: required_number(parsed, "--y")?,
            r: required_number(parsed, "--r")?,
        }),
        other => Err(CfdError::new(
            "cli/invalid-obstacle-kind",
            format!("unknown obstacle kind `{other}`; only rect and circle are supported"),
        )),
    }
}

fn obstacle_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "obstacle list <file>")?;
    let session = open_session(file)?;
    let obstacles = session.obstacle_list();
    let json = serde_json::to_string_pretty(&obstacles).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize obstacles: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn obstacle_remove(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--id"], &[])?;
    let file = positional(&parsed, 0, "obstacle remove <file> --id <name>")?;
    let id = required(&parsed, "--id")?;
    let mut session = open_session(file)?;
    let summary = session.obstacle_remove(id)?;
    println!("removed obstacle `{id}`; {} total", summary.obstacles.len());
    Ok(0)
}

fn porous_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("add") => porous_add(&args[1..]),
        Some("list") => porous_list(&args[1..]),
        Some("remove") => porous_remove(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!(
                "`porous` has no subcommand `{other}`; only add, remove and list are supported"
            ),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`porous` requires a subcommand; only add, remove and list are supported",
        )),
    }
}

fn porous_add(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--id",
            "--x0",
            "--y0",
            "--x1",
            "--y1",
            "--permeability",
            "--inertial",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --permeability <m2> [--inertial <n>] [--id <name>]",
    )?;
    let region = PorousRegion {
        id: parsed.options.get("--id").cloned().unwrap_or_default(),
        x0: required_number(&parsed, "--x0")?,
        y0: required_number(&parsed, "--y0")?,
        x1: required_number(&parsed, "--x1")?,
        y1: required_number(&parsed, "--y1")?,
        permeability: required_number(&parsed, "--permeability")?,
        inertial: number(&parsed, "--inertial")?.unwrap_or(0.0),
    };
    let mut session = open_session(file)?;
    let summary = session.porous_add(region)?;
    let added = summary.porous.last().expect("region was just appended");
    println!(
        "added porous region `{}` with K={} m2 and C_F={}; {} total",
        added.id,
        added.permeability,
        added.inertial,
        summary.porous.len()
    );
    Ok(0)
}

fn porous_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "porous list <file>")?;
    let session = open_session(file)?;
    let regions = session.porous_list();
    let json = serde_json::to_string_pretty(&regions).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize porous regions: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn porous_remove(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--id"], &[])?;
    let file = positional(&parsed, 0, "porous remove <file> --id <name>")?;
    let id = required(&parsed, "--id")?;
    let mut session = open_session(file)?;
    let summary = session.porous_remove(id)?;
    println!(
        "removed porous region `{id}`; {} total",
        summary.porous.len()
    );
    Ok(0)
}

fn wall_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("function") => match args.get(1).map(String::as_str) {
            Some("set") => wall_function_set(&args[2..]),
            Some("clear") => wall_function_clear(&args[2..]),
            Some(other) => Err(CfdError::new(
                "cli/unknown-command",
                format!(
                    "`wall function` has no subcommand `{other}`; only set and clear are supported"
                ),
            )),
            None => Err(CfdError::new(
                "cli/missing-argument",
                "`wall function` requires a subcommand; only set and clear are supported",
            )),
        },
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`wall` has no subcommand `{other}`; only function is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`wall` requires a subcommand; only function is supported",
        )),
    }
}

fn wall_function_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--kappa", "--intercept", "--min-y-plus"], &[])?;
    let file = positional(
        &parsed,
        0,
        "wall function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]",
    )?;
    let defaults = WallFunction::default();
    let kappa = number(&parsed, "--kappa")?.unwrap_or(defaults.kappa);
    let intercept = number(&parsed, "--intercept")?.unwrap_or(defaults.intercept);
    let min_y_plus = number(&parsed, "--min-y-plus")?.unwrap_or(defaults.min_y_plus);
    let mut session = open_session(file)?;
    let summary = session.wall_function_set(kappa, intercept, min_y_plus)?;
    let wall = summary
        .wall_function
        .as_ref()
        .expect("wall function was just configured");
    println!(
        "wall function set: kappa={}, intercept={}, minYPlus={}",
        wall.kappa, wall.intercept, wall.min_y_plus
    );
    Ok(0)
}

fn wall_function_clear(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "wall function clear <file>")?;
    let mut session = open_session(file)?;
    session.wall_function_clear()?;
    println!("wall function cleared");
    Ok(0)
}

fn probe_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("add") => probe_add(&args[1..]),
        Some("update") => probe_update(&args[1..]),
        Some("remove") => probe_remove(&args[1..]),
        Some("list") => probe_list(&args[1..]),
        Some("clear") => probe_clear(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`probe` has no subcommand `{other}`; use add, update, remove, list or clear"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`probe` requires a subcommand; use add, update, remove, list or clear",
        )),
    }
}

fn probe_add(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--id", "--x", "--y"], &[])?;
    let file = positional(&parsed, 0, "probe add <file> --x <m> --y <m> [--id <name>]")?;
    let probe = Probe {
        id: parsed.options.get("--id").cloned().unwrap_or_default(),
        x: required_number(&parsed, "--x")?,
        y: required_number(&parsed, "--y")?,
    };
    let mut session = open_session(file)?;
    let summary = session.probe_add(probe)?;
    let added = summary.probes.last().expect("probe was just appended");
    println!(
        "added probe `{}` at ({}, {}) m; {} total",
        added.id(),
        added.x,
        added.y,
        summary.probes.len()
    );
    Ok(0)
}

fn probe_update(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--id", "--x", "--y"], &[])?;
    let file = positional(
        &parsed,
        0,
        "probe update <file> --id <name> --x <m> --y <m>",
    )?;
    let id = required(&parsed, "--id")?.to_string();
    let x = required_number(&parsed, "--x")?;
    let y = required_number(&parsed, "--y")?;
    let mut session = open_session(file)?;
    let summary = session.probe_update(&id, x, y)?;
    println!(
        "moved probe `{id}` to ({x}, {y}) m; {} total",
        summary.probes.len()
    );
    Ok(0)
}

fn probe_remove(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--id"], &[])?;
    let file = positional(&parsed, 0, "probe remove <file> --id <name>")?;
    let id = required(&parsed, "--id")?;
    let mut session = open_session(file)?;
    let summary = session.probe_remove(id)?;
    println!("removed probe `{id}`; {} total", summary.probes.len());
    Ok(0)
}

fn probe_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "probe list <file>")?;
    let session = open_session(file)?;
    let probes = session.probe_list();
    let json = serde_json::to_string_pretty(&probes).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize probes: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn probe_clear(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "probe clear <file>")?;
    let mut session = open_session(file)?;
    session.probe_clear()?;
    println!("cleared all probes");
    Ok(0)
}

fn inlet_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("series") => match args.get(1).map(String::as_str) {
            Some("set") => inlet_series_set(&args[2..]),
            Some("list") => inlet_series_list(&args[2..]),
            Some("clear") => inlet_series_clear(&args[2..]),
            Some(other) => Err(CfdError::new(
                "cli/unknown-command",
                format!("`inlet series` has no subcommand `{other}`; use set, list or clear"),
            )),
            None => Err(CfdError::new(
                "cli/missing-argument",
                "`inlet series` requires a subcommand; use set, list or clear",
            )),
        },
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`inlet` has no subcommand `{other}`; only series is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`inlet` requires a subcommand; only series is supported",
        )),
    }
}

fn inlet_series_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--side", "--u", "--v", "--pressure"], &[])?;
    let file = positional(
        &parsed,
        0,
        "inlet series set <file> --side left|right|top|bottom [--u <t:v,...>] [--v <t:v,...>] [--pressure <t:v,...>]",
    )?;
    let side = required(&parsed, "--side")?.parse::<Side>()?;
    let u = parsed
        .options
        .get("--u")
        .map(|value| parse_series(value))
        .transpose()?;
    let v = parsed
        .options
        .get("--v")
        .map(|value| parse_series(value))
        .transpose()?;
    let pressure = parsed
        .options
        .get("--pressure")
        .map(|value| parse_series(value))
        .transpose()?;
    let mut session = open_session(file)?;
    let summary = session.transient_inlet_set(TransientInlet {
        side,
        u,
        v,
        pressure,
    })?;
    println!(
        "transient inlet set: side={} ({} entries)",
        side.as_str(),
        summary.transient_inlets.len()
    );
    Ok(0)
}

fn inlet_series_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "inlet series list <file>")?;
    let session = open_session(file)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&session.transient_inlet_list())
            .unwrap_or_else(|_| "[]".to_string())
    );
    Ok(0)
}

fn inlet_series_clear(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "inlet series clear <file>")?;
    let mut session = open_session(file)?;
    session.transient_inlet_clear()?;
    println!("cleared all transient inlets");
    Ok(0)
}

fn parse_series(value: &str) -> Result<TimeSeries, CfdError> {
    let mut times = Vec::new();
    let mut values = Vec::new();
    for pair in value.split(',').filter(|pair| !pair.is_empty()) {
        let (time, sample) = pair.split_once(':').ok_or_else(|| {
            CfdError::new(
                "cli/invalid-time-series",
                format!("time-series sample `{pair}` must use `time:value`"),
            )
        })?;
        times.push(time.parse::<f64>().map_err(|_| {
            CfdError::new(
                "cli/invalid-time-series",
                format!("time-series time `{time}` is not a number"),
            )
        })?);
        values.push(sample.parse::<f64>().map_err(|_| {
            CfdError::new(
                "cli/invalid-time-series",
                format!("time-series value `{sample}` is not a number"),
            )
        })?);
    }
    let series = TimeSeries { times, values };
    series.validate()?;
    Ok(series)
}

fn scalar_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => scalar_set(&args[1..]),
        Some("source") => scalar_source_command(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`scalar` has no subcommand `{other}`; use `set`, `source add`, `source list` or `source clear`"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`scalar` requires a subcommand; use `set`, `source add`, `source list` or `source clear`",
        )),
    }
}

fn scalar_source_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("add") => scalar_source_add(&args[1..]),
        Some("list") => scalar_source_list(&args[1..]),
        Some("clear") => scalar_source_clear(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`scalar source` has no subcommand `{other}`; use add, list or clear"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`scalar source` requires a subcommand; use add, list or clear",
        )),
    }
}

fn scalar_source_add(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--x0", "--y0", "--x1", "--y1", "--rate", "--start", "--ramp", "--id",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "scalar source add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --rate <unit/s> [--start <s>] [--ramp <s>] [--id <name>]",
    )?;
    let mut session = open_session(file)?;
    let source = ScalarSource {
        id: parsed.options.get("--id").cloned().unwrap_or_default(),
        x0: required_number(&parsed, "--x0")?,
        y0: required_number(&parsed, "--y0")?,
        x1: required_number(&parsed, "--x1")?,
        y1: required_number(&parsed, "--y1")?,
        rate: required_number(&parsed, "--rate")?,
        start: number(&parsed, "--start")?.unwrap_or(0.0),
        ramp: number(&parsed, "--ramp")?.unwrap_or(0.0),
    };
    let summary = session.scalar_source_add(source)?;
    let added = summary
        .scalar_sources
        .last()
        .expect("scalar source was just added");
    println!(
        "added scalar source {} rate={} unit/s, start={} s, ramp={} s; {} total",
        added.id,
        added.rate,
        added.start,
        added.ramp,
        summary.scalar_sources.len()
    );
    Ok(0)
}

fn scalar_source_list(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "scalar source list <file>")?;
    let session = open_session(file)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&session.scalar_source_list())
            .unwrap_or_else(|_| "[]".to_string())
    );
    Ok(0)
}

fn scalar_source_clear(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "scalar source clear <file>")?;
    let mut session = open_session(file)?;
    session.scalar_source_clear()?;
    println!("cleared all scalar sources");
    Ok(0)
}

fn scalar_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--diffusivity", "--initial", "--inlet"], &[])?;
    let file = positional(
        &parsed,
        0,
        "scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]",
    )?;
    let diffusivity = required_number(&parsed, "--diffusivity")?;
    let initial = number(&parsed, "--initial")?;
    let inlet = number(&parsed, "--inlet")?;
    let mut session = open_session(file)?;
    let summary = session.scalar_set(diffusivity, initial, inlet)?;
    let scalar = summary.scalar.as_ref().expect("scalar was just configured");
    println!(
        "scalar set: diffusivity={} m2/s, initial={}, inletValue={}",
        scalar.diffusivity, scalar.initial, scalar.inlet_value
    );
    Ok(0)
}

fn buoyancy_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--beta", "--t-ref", "--gx", "--gy"], &[])?;
    let file = positional(
        &parsed,
        0,
        "buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>",
    )?;
    let beta = required_number(&parsed, "--beta")?;
    let t_ref = required_number(&parsed, "--t-ref")?;
    let gx = required_number(&parsed, "--gx")?;
    let gy = required_number(&parsed, "--gy")?;
    let mut session = open_session(file)?;
    let summary = session.buoyancy_set(beta, t_ref, gx, gy)?;
    match summary.buoyancy.as_ref() {
        Some(buoyancy) => println!(
            "buoyancy set: beta={} 1/K, tRef={} K, gravity=[{}, {}] m/s2",
            buoyancy.beta, buoyancy.t_ref, buoyancy.gravity[0], buoyancy.gravity[1]
        ),
        None => println!("buoyancy disabled"),
    }
    Ok(0)
}

fn convection_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => convection_set(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`convection` has no subcommand `{other}`; only set is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`convection` requires a subcommand; only set is supported",
        )),
    }
}

fn convection_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &["--scheme"], &[])?;
    let file = positional(
        &parsed,
        0,
        "convection set <file> --scheme upwind|central|quick",
    )?;
    let scheme = required(&parsed, "--scheme")?.parse::<ConvectionScheme>()?;
    let mut session = open_session(file)?;
    let summary = session.convection_set(scheme)?;
    println!("convection set: {}", summary.convection.as_str());
    Ok(0)
}

fn pressure_command(args: &[String]) -> Result<i32, CfdError> {
    if args.first().map(String::as_str) != Some("gradient") {
        return Err(CfdError::new(
            "cli/unknown-command",
            "`pressure` needs the `gradient` subcommand",
        ));
    }
    if args.get(1).map(String::as_str) != Some("set") {
        return Err(CfdError::new(
            "cli/unknown-command",
            "`pressure gradient` needs the `set` subcommand",
        ));
    }
    let parsed = parse(&args[2..], &["--dpdx", "--dpdy"], &[])?;
    let file = positional(
        &parsed,
        0,
        "pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>",
    )?;
    let dpdx = required_number(&parsed, "--dpdx")?;
    let dpdy = required_number(&parsed, "--dpdy")?;
    let mut session = open_session(file)?;
    let summary = session.pressure_gradient_set(dpdx, dpdy)?;
    match summary.pressure_gradient {
        Some(gradient) => println!(
            "pressure gradient set: dpdx={} Pa/m, dpdy={} Pa/m",
            gradient[0], gradient[1]
        ),
        None => println!("pressure gradient cleared"),
    }
    Ok(0)
}

fn turbulence_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => turbulence_set(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`turbulence` has no subcommand `{other}`; only set is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`turbulence` requires a subcommand; only set is supported",
        )),
    }
}

fn turbulence_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &["--model", "--nu-t", "--mixing-length", "--k", "--epsilon"],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "turbulence set <file> --model none|constant|mixingLength|kEpsilon [--nu-t <m2/s>] [--mixing-length <m>] [--k <m2/s2>] [--epsilon <m2/s3>]",
    )?;
    let model = required(&parsed, "--model")?.parse::<TurbulenceModel>()?;
    let nu_t = number(&parsed, "--nu-t")?.unwrap_or(0.0);
    let mixing_length = number(&parsed, "--mixing-length")?.unwrap_or(0.1);
    let k = number(&parsed, "--k")?;
    let epsilon = number(&parsed, "--epsilon")?;
    let mut session = open_session(file)?;
    let summary = session.turbulence_set(model, nu_t, mixing_length, k, epsilon)?;
    match summary.turbulence.as_ref() {
        Some(turbulence) => println!(
            "turbulence set: {}, nuT={} m2/s, mixingLength={} m, k={} m2/s2, epsilon={} m2/s3",
            turbulence.model.as_str(),
            turbulence.nu_t,
            turbulence.mixing_length,
            turbulence.k,
            turbulence.epsilon
        ),
        None => println!("turbulence disabled (laminar)"),
    }
    Ok(0)
}

fn heat_command(args: &[String]) -> Result<i32, CfdError> {
    match args.first().map(String::as_str) {
        Some("set") => heat_set(&args[1..]),
        Some(other) => Err(CfdError::new(
            "cli/unknown-command",
            format!("`heat` has no subcommand `{other}`; only set is supported"),
        )),
        None => Err(CfdError::new(
            "cli/missing-argument",
            "`heat` requires a subcommand; only set is supported",
        )),
    }
}

fn side_thermal(
    parsed: &Parsed,
    temperature: &str,
    heat_flux: &str,
) -> Result<SideThermal, CfdError> {
    Ok(SideThermal {
        temperature: number(parsed, temperature)?,
        heat_flux: number(parsed, heat_flux)?,
    })
}

fn heat_set(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--conductivity",
            "--specific-heat",
            "--diffusivity",
            "--heat-source",
            "--left-temperature",
            "--right-temperature",
            "--top-temperature",
            "--bottom-temperature",
            "--left-heat-flux",
            "--right-heat-flux",
            "--top-heat-flux",
            "--bottom-heat-flux",
            "--obstacle-temperature",
            "--obstacle-heat-flux",
            "--reference-temperature",
        ],
        &[],
    )?;
    let file = positional(&parsed, 0, "heat set <file> [thermal options]")?;
    let mut session = open_session(file)?;
    let summary = session.summary()?;
    let specific_heat = number(&parsed, "--specific-heat")?.unwrap_or(1.0);
    let conductivity = number(&parsed, "--conductivity")?;
    let diffusivity = match (number(&parsed, "--diffusivity")?, conductivity) {
        (Some(diffusivity), _) => Some(diffusivity),
        (None, Some(conductivity)) if summary.fluid.density * specific_heat > 0.0 => {
            Some(conductivity / (summary.fluid.density * specific_heat))
        }
        _ => None,
    };
    let thermal = ThermalConfig {
        specific_heat,
        conductivity,
        heat_source: number(&parsed, "--heat-source")?.unwrap_or(0.0),
        left: side_thermal(&parsed, "--left-temperature", "--left-heat-flux")?,
        right: side_thermal(&parsed, "--right-temperature", "--right-heat-flux")?,
        top: side_thermal(&parsed, "--top-temperature", "--top-heat-flux")?,
        bottom: side_thermal(&parsed, "--bottom-temperature", "--bottom-heat-flux")?,
        obstacle_temperature: number(&parsed, "--obstacle-temperature")?,
        obstacle_heat_flux: number(&parsed, "--obstacle-heat-flux")?,
        reference_temperature: number(&parsed, "--reference-temperature")?,
    };
    let summary = session.thermal_set(thermal, diffusivity)?;
    let thermal = summary
        .thermal
        .as_ref()
        .expect("thermal was just configured");
    println!(
        "heat set: cp={} J/kgK, k={:?} W/mK, heatSource={} W/m3, obstacleTemperature={:?}, obstacleHeatFlux={:?}",
        thermal.specific_heat, thermal.conductivity, thermal.heat_source,
        thermal.obstacle_temperature, thermal.obstacle_heat_flux
    );
    Ok(0)
}

fn validate(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "validate <file>")?;
    let session = open_session(file)?;
    let summary = session.validate()?;
    println!(
        "validation passed: {}×{} mesh, {}×{} m, rho={} kg/m3, mu={} Pa.s, {} steps, dt={} s",
        summary.mesh.nx,
        summary.mesh.ny,
        summary.mesh.length_x,
        summary.mesh.length_y,
        summary.fluid.density,
        summary.fluid.viscosity,
        summary.time.steps,
        summary.time.dt
    );
    Ok(0)
}

fn quality(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "quality <file>")?;
    let session = open_session(file)?;
    let report = session.mesh_quality()?;
    let json = serde_json::to_string_pretty(&report).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize the mesh quality report: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn reynolds(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "reynolds <file>")?;
    let session = open_session(file)?;
    let report = session.reynolds()?;
    let json = serde_json::to_string_pretty(&report).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize the Reynolds number report: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn stability(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(args, &[], &[])?;
    let file = positional(&parsed, 0, "stability <file>")?;
    let session = open_session(file)?;
    let report = session.stability()?;
    let json = serde_json::to_string_pretty(&report).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize the stability report: {error}"),
        )
    })?;
    println!("{json}");
    Ok(0)
}

fn solve_command(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--out",
            "--steps",
            "--tolerance",
            "--check-every",
            "--restart",
            "--checkpoint",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]",
    )?;
    let options = SolveOptions {
        steps: optional_usize(&parsed, "--steps")?,
        tolerance: number(&parsed, "--tolerance")?,
        check_every: optional_usize(&parsed, "--check-every")?,
    };
    let restart = parsed.options.get("--restart").map(PathBuf::from);
    let checkpoint = parsed.options.get("--checkpoint").map(PathBuf::from);
    let session = open_session(file)?;
    let report =
        session.solve_with_checkpoint(&options, restart.as_deref(), checkpoint.as_deref())?;
    eprintln!(
        "solve summary: {} steps, t={} s, termination={}, steadyState={}, residual={:.6e}, continuity={:.6e}, massBalance={:.6e} kg/s (relative {:.6e})",
        report.steps,
        report.time,
        report.termination_reason,
        report.steady_state,
        report.residual,
        report.divergence_l2,
        report.mass_balance.net,
        report.mass_balance.relative_error
    );
    if report.adaptive {
        eprintln!(
            "adaptive step: averageDt={} s, minDt={} s, maxDt={} s",
            report.average_dt, report.min_dt, report.max_dt
        );
    }
    if !report.outlets.is_empty() {
        let summary = report
            .outlets
            .iter()
            .map(|outlet| {
                format!(
                    "{}={:.6e} kg/s (share {:.4})",
                    outlet.side, outlet.flow, outlet.share
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        eprintln!("outlet allocation: {summary}");
    }
    let json = serde_json::to_string_pretty(&report).map_err(|error| {
        CfdError::new(
            "document/serialize",
            format!("cannot serialize report: {error}"),
        )
    })?;
    if let Some(out) = parsed.options.get("--out") {
        write_atomic(Path::new(out), &json)?;
    }
    println!("{json}");
    Ok(0)
}

fn export(args: &[String]) -> Result<i32, CfdError> {
    let parsed = parse(
        args,
        &[
            "--format",
            "--out",
            "--field",
            "--layout",
            "--seed",
            "--particle-steps",
        ],
        &[],
    )?;
    let file = positional(
        &parsed,
        0,
        "export <file> --format csv|vtk|svg|contours|streamlines|residuals|forces|coefficients|probes|particles [--field pressure|temperature|vorticity|streamFunction|speed] [--layout cell|node] [--seed <x,y>] [--particle-steps <n>] --out <dir|file>",
    )?;
    let format = required(&parsed, "--format")?.parse::<ExportFormat>()?;
    let field = match parsed.options.get("--field") {
        Some(value) => value.parse::<ExportField>()?,
        None => ExportField::Pressure,
    };
    let layout = match parsed.options.get("--layout") {
        Some(value) => value.parse::<ExportLayout>()?,
        None => ExportLayout::Cell,
    };
    let out = required(&parsed, "--out")?;
    let session = open_session(file)?;
    let report = match format {
        ExportFormat::Particles => {
            let seeds = parsed
                .options
                .get("--seed")
                .map(|raw| particle_seeds(raw))
                .transpose()?
                .unwrap_or_default();
            let max_steps = match parsed.options.get("--particle-steps") {
                Some(raw) => {
                    let steps = raw.parse::<usize>().map_err(|_| {
                        CfdError::new(
                            "cli/invalid-number",
                            format!(
                                "value `{raw}` of `--particle-steps` is not a positive integer"
                            ),
                        )
                    })?;
                    if steps == 0 {
                        return Err(CfdError::new(
                            "cli/invalid-number",
                            "`--particle-steps` must be greater than 0",
                        ));
                    }
                    steps
                }
                None => DEFAULT_PARTICLE_STEPS,
            };
            let config = ParticleConfig {
                seeds,
                dt: 0.0,
                max_steps,
            };
            session.export_particles(Path::new(out), &config)?
        }
        other => session.export_field_with_layout(other, Path::new(out), field, layout)?,
    };
    let layout_label = match layout {
        ExportLayout::Cell => "",
        ExportLayout::Node => " (nodes)",
    };
    match format {
        ExportFormat::Csv => println!("exported CSV{layout_label}: {}", report.path),
        ExportFormat::Vtk => println!("exported legacy ASCII VTK{layout_label}: {}", report.path),
        ExportFormat::Svg => println!("exported SVG ({}): {}", field.as_str(), report.path),
        ExportFormat::Contours => {
            println!("exported contour SVG ({}): {}", field.as_str(), report.path)
        }
        ExportFormat::Streamlines => println!("exported streamlines SVG: {}", report.path),
        ExportFormat::Residuals => println!("exported residual history CSV: {}", report.path),
        ExportFormat::Forces => println!("exported force history CSV: {}", report.path),
        ExportFormat::Coefficients => {
            println!("exported coefficient summary CSV: {}", report.path)
        }
        ExportFormat::Probes => println!("exported probe history CSV: {}", report.path),
        ExportFormat::Particles => println!("exported particle trajectories CSV: {}", report.path),
    }
    Ok(0)
}

/// Parses `x,y` seeds separated by semicolons.
fn particle_seeds(raw: &str) -> Result<Vec<ParticleSeed>, CfdError> {
    let mut seeds = Vec::new();
    for part in raw.split(';').filter(|part| !part.trim().is_empty()) {
        let (x, y) = part.split_once(',').ok_or_else(|| {
            CfdError::new(
                "cli/invalid-seed",
                format!("particle seed `{part}` must look like `x,y`"),
            )
        })?;
        let x = x.trim().parse::<f64>().map_err(|_| {
            CfdError::new(
                "cli/invalid-seed",
                format!("particle seed x coordinate `{}` is not a number", x.trim()),
            )
        })?;
        let y = y.trim().parse::<f64>().map_err(|_| {
            CfdError::new(
                "cli/invalid-seed",
                format!("particle seed y coordinate `{}` is not a number", y.trim()),
            )
        })?;
        seeds.push(ParticleSeed { x, y });
    }
    Ok(seeds)
}

fn not_implemented(feature: &str) -> CfdError {
    CfdError::new(
        "capability/not-implemented",
        format!("{feature} is not implemented; only two-dimensional, single-phase, incompressible flow and heat transfer (with constant or mixing-length eddy viscosity) are supported"),
    )
}

#[derive(Default)]
struct Parsed {
    positionals: Vec<String>,
    flags: HashSet<String>,
    options: HashMap<String, String>,
}

fn parse(
    args: &[String],
    value_options: &[&str],
    flag_options: &[&str],
) -> Result<Parsed, CfdError> {
    let mut parsed = Parsed::default();
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if let Some((name, value)) = argument.split_once('=') {
            if name.starts_with("--") {
                if !value_options.contains(&name) {
                    return Err(CfdError::new(
                        "cli/unknown-option",
                        format!("unknown option `{name}`"),
                    ));
                }
                parsed.options.insert(name.to_string(), value.to_string());
                index += 1;
                continue;
            }
        }
        if argument.starts_with("--") {
            if flag_options.contains(&argument.as_str()) {
                parsed.flags.insert(argument.clone());
                index += 1;
            } else if value_options.contains(&argument.as_str()) {
                let value = args.get(index + 1).ok_or_else(|| {
                    CfdError::new(
                        "cli/missing-value",
                        format!("`{argument}` is missing a value"),
                    )
                })?;
                if value.starts_with("--") {
                    return Err(CfdError::new(
                        "cli/missing-value",
                        format!("`{argument}` is missing a value"),
                    ));
                }
                parsed.options.insert(argument.clone(), value.clone());
                index += 2;
            } else {
                return Err(CfdError::new(
                    "cli/unknown-option",
                    format!("unknown option `{argument}`"),
                ));
            }
        } else {
            parsed.positionals.push(argument.clone());
            index += 1;
        }
    }
    Ok(parsed)
}

fn positional<'a>(parsed: &'a Parsed, index: usize, usage: &str) -> Result<&'a str, CfdError> {
    parsed
        .positionals
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| CfdError::new("cli/missing-argument", format!("missing argument: {usage}")))
}

fn required<'a>(parsed: &'a Parsed, name: &str) -> Result<&'a str, CfdError> {
    parsed.options.get(name).map(String::as_str).ok_or_else(|| {
        CfdError::new(
            "cli/missing-option",
            format!("missing required option `{name}`"),
        )
    })
}

fn number(parsed: &Parsed, name: &str) -> Result<Option<f64>, CfdError> {
    parsed
        .options
        .get(name)
        .map(|value| {
            value.parse::<f64>().map_err(|_| {
                CfdError::new(
                    "cli/invalid-number",
                    format!("value `{value}` of `{name}` is not a valid number"),
                )
            })
        })
        .transpose()
}

fn required_number(parsed: &Parsed, name: &str) -> Result<f64, CfdError> {
    number(parsed, name)?.ok_or_else(|| {
        CfdError::new(
            "cli/missing-option",
            format!("missing required option `{name}`"),
        )
    })
}

fn required_usize(parsed: &Parsed, name: &str) -> Result<usize, CfdError> {
    let value = required(parsed, name)?;
    value.parse::<usize>().map_err(|_| {
        CfdError::new(
            "cli/invalid-number",
            format!("value `{value}` of `{name}` is not a valid positive integer"),
        )
    })
}

fn optional_usize(parsed: &Parsed, name: &str) -> Result<Option<usize>, CfdError> {
    parsed
        .options
        .get(name)
        .map(|_| required_usize(parsed, name))
        .transpose()
}
