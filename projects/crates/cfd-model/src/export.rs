use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use science_mesh::{Cell, CellKind, Field, Mesh as FieldMesh};

use crate::solver::Solution;
use crate::CfdError;

/// Cell-centred scalar fields available for SVG previews.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportField {
    /// Gauge pressure in pascals.
    Pressure,
    /// Temperature in kelvin.
    Temperature,
    /// Vorticity `∂v/∂x − ∂u/∂y` in 1/s.
    Vorticity,
    /// Stream function `ψ` in m²/s.
    StreamFunction,
}

impl ExportField {
    /// Identifier used by the CLI and TypeScript API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pressure => "pressure",
            Self::Temperature => "temperature",
            Self::Vorticity => "vorticity",
            Self::StreamFunction => "streamFunction",
        }
    }

    /// Physical unit shown in the legend.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Pressure => "Pa",
            Self::Temperature => "K",
            Self::Vorticity => "1/s",
            Self::StreamFunction => "m²/s",
        }
    }
}

impl FromStr for ExportField {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pressure" => Ok(Self::Pressure),
            "temperature" => Ok(Self::Temperature),
            "vorticity" => Ok(Self::Vorticity),
            "streamFunction" | "stream-function" | "stream_function" => Ok(Self::StreamFunction),
            other => Err(CfdError::new(
                "export/invalid-field",
                format!(
                    "unknown export field `{other}`; only pressure, temperature, vorticity or streamFunction is supported"
                ),
            )),
        }
    }
}

/// Fixed nine-stop blue-to-red palette used by every SVG preview.
const PALETTE: [&str; 9] = [
    "#08306b", "#2171b5", "#4292c6", "#6baed6", "#c7e9c0", "#fee391", "#fe9929", "#e31a1c",
    "#800026",
];

/// Writes a self-contained SVG preview of one cell-centred field.
///
/// Each fluid cell is drawn as a rectangle coloured with the fixed nine-stop
/// palette over the physical `[min, max]` range of the field. Solid cells are
/// dark grey. The file includes a title, the physical range and a discrete
/// colour legend. When `path` ends in `.svg` it is used as the output file;
/// otherwise it is treated as a directory and `solution-<field>.svg` is
/// created inside it.
pub fn export_svg(
    solution: &Solution,
    path: &Path,
    field: ExportField,
) -> Result<PathBuf, CfdError> {
    let default_name = format!("solution-{}.svg", field.as_str());
    let target = with_extension_or_file(path, "svg", &default_name);
    let values = match field {
        ExportField::Pressure => &solution.p,
        ExportField::Temperature => solution.temperature.as_ref().ok_or_else(|| {
            CfdError::new(
                "export/no-field",
                "the case does not configure scalar (temperature) transport, so temperature cannot be exported as SVG",
            )
        })?,
        ExportField::Vorticity => &solution.vorticity,
        ExportField::StreamFunction => &solution.stream_function,
    };
    let text = render_svg(solution, values, field);
    write_atomic(&target, &text)?;
    Ok(target)
}

fn render_svg(solution: &Solution, values: &[f64], field: ExportField) -> String {
    let mesh = &solution.mesh;
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for (index, value) in values.iter().enumerate() {
        if mesh.solid[index] || !value.is_finite() {
            continue;
        }
        min = min.min(*value);
        max = max.max(*value);
    }
    if !min.is_finite() || !max.is_finite() {
        min = 0.0;
        max = 0.0;
    }
    if (max - min).abs() < 1e-15 {
        let pad = if min.abs() > 1e-12 {
            min.abs() * 0.05
        } else {
            1.0
        };
        min -= pad;
        max += pad;
    }

    let cell_size = 18.0;
    let margin = 24.0;
    let plot_width = mesh.nx as f64 * cell_size;
    let plot_height = mesh.ny as f64 * cell_size;
    let legend_height = 84.0;
    let width = plot_width + 2.0 * margin;
    let height = plot_height + 2.0 * margin + 52.0 + legend_height;

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\">"
    );
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{title_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"18\" font-weight=\"600\">OpenCFD {name} field ({unit})</text>",
        title_y = margin + 4.0,
        name = field.as_str(),
        unit = field.unit(),
    );
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{range_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"12\" fill=\"#444\">range {min:.6} – {max:.6} {unit}</text>",
        range_y = margin + 28.0,
        unit = field.unit(),
    );

    let top = margin + 52.0;
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let index = mesh.pressure_index(i, j);
            let fill = if mesh.solid[index] {
                "#3a3a3a"
            } else {
                palette_color(values[index], min, max)
            };
            let _ = writeln!(
                svg,
                "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{cell_size:.2}\" height=\"{cell_size:.2}\" fill=\"{fill}\"/>",
                x = margin + i as f64 * cell_size,
                y = top + (mesh.ny - 1 - j) as f64 * cell_size,
            );
        }
    }
    let _ = writeln!(
        svg,
        "<rect x=\"{margin:.2}\" y=\"{top:.2}\" width=\"{plot_width:.2}\" height=\"{plot_height:.2}\" fill=\"none\" stroke=\"#222\" stroke-width=\"1\"/>"
    );

    let legend_y = top + plot_height + 34.0;
    let legend_width = plot_width;
    let stop_width = legend_width / PALETTE.len() as f64;
    for (index, color) in PALETTE.iter().enumerate() {
        let _ = writeln!(
            svg,
            "<rect x=\"{x:.2}\" y=\"{legend_y:.2}\" width=\"{stop_width:.2}\" height=\"16\" fill=\"{color}\"/>",
            x = margin + index as f64 * stop_width,
        );
    }
    let _ = writeln!(
        svg,
        "<rect x=\"{margin:.2}\" y=\"{legend_y:.2}\" width=\"{legend_width:.2}\" height=\"16\" fill=\"none\" stroke=\"#222\" stroke-width=\"1\"/>"
    );
    let label_y = legend_y + 34.0;
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.2}\" y=\"{label_y:.2}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"12\" fill=\"#222\">{min:.6} {unit}</text>",
        unit = field.unit(),
    );
    let _ = writeln!(
        svg,
        "<text x=\"{right:.2}\" y=\"{label_y:.2}\" text-anchor=\"end\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"12\" fill=\"#222\">{max:.6} {unit}</text>",
        right = margin + legend_width,
        unit = field.unit(),
    );
    let _ = writeln!(
        svg,
        "<text x=\"{middle:.2}\" y=\"{label_y:.2}\" text-anchor=\"middle\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"11\" fill=\"#666\">solid cells shown dark grey</text>",
        middle = margin + 0.5 * legend_width,
    );
    svg.push_str("</svg>\n");
    svg
}

fn palette_color(value: f64, min: f64, max: f64) -> &'static str {
    let normalized = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let last = PALETTE.len() - 1;
    let index = ((normalized * last as f64).round() as usize).min(last);
    PALETTE[index]
}

/// Writes text to `path` through a same-directory temporary file published by
/// `boost-session`, which replaces an existing target with a single rename.
pub fn write_atomic(path: &Path, contents: &str) -> Result<(), CfdError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).map_err(|error| {
            CfdError::new(
                "io/write",
                format!("cannot create directory `{}`: {error}", parent.display()),
            )
        })?;
    }
    let target = if path
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        path.to_path_buf()
    } else {
        Path::new(".").join(path)
    };
    boost_session::save_atomic(&target, contents.as_bytes()).map_err(|error| {
        CfdError::new(
            "io/write",
            format!("cannot write `{}`: {error}", target.display()),
        )
    })
}

/// Writes cell-centred `x, y, z, pressure, velocity_x, velocity_y, velocity_z,
/// magnitude, temperature, vorticity, streamFunction` rows.
///
/// Points follow the mesh's i/j row-major pressure order, so the row order
/// still encodes `(i, j)` exactly as before. `vorticity` is `∂v/∂x − ∂u/∂y`
/// in 1/s and `streamFunction` is `ψ` in m²/s with zero mean over fluid cells.
/// When `path` ends in `.csv` it is used as the output file; otherwise it is
/// treated as a directory and `solution.csv` is created inside it.
pub fn export_csv(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "solution.csv");
    let mesh = field_mesh(solution);
    let text = mesh
        .to_csv(&solution_fields(solution))
        .map_err(mesh_error)?;
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Writes a legacy ASCII VTK `UNSTRUCTURED_GRID` file over the cell centres.
///
/// Points are the `nx × ny` cell centres in i/j row-major order and every quad
/// joins four neighbouring centres, so the point data keeps the cell-centred
/// `pressure`, `velocity`, `magnitude`, `temperature`, `vorticity` and
/// `streamFunction` values. When `path` ends in `.vtk` it is used as the output
/// file; otherwise it is treated as a directory and `solution.vtk` is created
/// inside it.
pub fn export_vtk(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "vtk", "solution.vtk");
    let mesh = field_mesh(solution);
    let text = mesh
        .to_legacy_vtk(&solution_fields(solution))
        .map_err(mesh_error)?;
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Writes the sampled residual history as `step,residual,continuity` CSV rows.
///
/// One row is written per [`crate::solver::ResidualSample`] the solve kept, in
/// time order. When `path` ends in `.csv` it is used as the output file;
/// otherwise it is treated as a directory and `residuals.csv` is created inside
/// it.
pub fn export_residuals(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "residuals.csv");
    let mut text = String::from("step,residual,continuity\n");
    for sample in &solution.report.residual_history {
        let _ = writeln!(
            text,
            "{},{:.10e},{:.10e}",
            sample.step, sample.residual, sample.continuity
        );
    }
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Writes every sampled obstacle force as a long-form CSV.
///
/// The header is `obstacle,kind,step,fx,fy,mz,cd,cl,cm`; each obstacle
/// contributes one row per sample it recorded, grouped in document order.
/// When `path` ends in `.csv` it is used as the output file; otherwise it is
/// treated as a directory and `forces.csv` is created inside it.
pub fn export_forces(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "forces.csv");
    let mut text = String::from("obstacle,kind,step,fx,fy,mz,cd,cl,cm\n");
    for force in &solution.report.forces {
        for sample in &force.history {
            let _ = writeln!(
                text,
                "{},{},{},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e}",
                force.id,
                force.kind,
                sample.step,
                sample.fx,
                sample.fy,
                sample.mz,
                sample.cd,
                sample.cl,
                sample.cm
            );
        }
    }
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Writes a self-contained SVG of streamlines traced through the velocity field.
///
/// Lines are seeded on a regular grid, integrated with a second-order midpoint
/// rule and traced both upstream and downstream. Integration stops at a solid
/// cell, outside the domain, at a stagnation point or after a fixed step cap,
/// so closed vortices cannot loop forever. When `path` ends in `.svg` it is
/// used as the output file; otherwise it is treated as a directory and
/// `solution-streamlines.svg` is created inside it.
pub fn export_streamlines(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "svg", "solution-streamlines.svg");
    let lines = trace_streamlines(solution);
    let text = render_streamlines(solution, &lines);
    write_atomic(&target, &text)?;
    Ok(target)
}

const STREAMLINE_SEEDS: usize = 6;
const MAX_STREAMLINE_STEPS: usize = 600;

/// Traces deterministic streamlines seeded on a regular grid.
fn trace_streamlines(solution: &Solution) -> Vec<Vec<(f64, f64)>> {
    let mesh = &solution.mesh;
    let step = 0.5 * mesh.dx.min(mesh.dy);
    let mut lines = Vec::new();
    for sy in 0..STREAMLINE_SEEDS {
        for sx in 0..STREAMLINE_SEEDS {
            let x = (sx as f64 + 0.5) / STREAMLINE_SEEDS as f64 * mesh.length_x;
            let y = (sy as f64 + 0.5) / STREAMLINE_SEEDS as f64 * mesh.length_y;
            if !is_fluid_point(mesh, x, y) {
                continue;
            }
            let downstream = trace_line(solution, x, y, 1.0, step);
            let upstream = trace_line(solution, x, y, -1.0, step);
            let mut line: Vec<(f64, f64)> = upstream.into_iter().rev().collect();
            line.extend(downstream.into_iter().skip(1));
            if line.len() >= 4 {
                lines.push(line);
            }
        }
    }
    lines
}

fn trace_line(
    solution: &Solution,
    start_x: f64,
    start_y: f64,
    direction: f64,
    step: f64,
) -> Vec<(f64, f64)> {
    let mesh = &solution.mesh;
    let mut points = vec![(start_x, start_y)];
    let (mut x, mut y) = (start_x, start_y);
    for _ in 0..MAX_STREAMLINE_STEPS {
        let Some((ux, uy)) = sample_velocity(solution, x, y) else {
            break;
        };
        let speed = ux.hypot(uy);
        if speed < 1e-10 {
            break;
        }
        let k1x = direction * ux / speed;
        let k1y = direction * uy / speed;
        let mid_x = x + 0.5 * step * k1x;
        let mid_y = y + 0.5 * step * k1y;
        let Some((mx, my)) = sample_velocity(solution, mid_x, mid_y) else {
            break;
        };
        let mid_speed = mx.hypot(my);
        if mid_speed < 1e-10 {
            break;
        }
        let next_x = x + step * direction * mx / mid_speed;
        let next_y = y + step * direction * my / mid_speed;
        if next_x < 0.0
            || next_y < 0.0
            || next_x > mesh.length_x
            || next_y > mesh.length_y
            || !is_fluid_point(mesh, next_x, next_y)
        {
            break;
        }
        points.push((next_x, next_y));
        x = next_x;
        y = next_y;
    }
    points
}

fn is_fluid_point(mesh: &crate::mesh::Mesh, x: f64, y: f64) -> bool {
    if x < 0.0 || y < 0.0 || x > mesh.length_x || y > mesh.length_y {
        return false;
    }
    let i = ((x / mesh.dx).floor() as isize).clamp(0, mesh.nx as isize - 1) as usize;
    let j = ((y / mesh.dy).floor() as isize).clamp(0, mesh.ny as isize - 1) as usize;
    !mesh.is_solid(i, j)
}

/// Bilinear velocity at an arbitrary point, or `None` inside a solid cell.
fn sample_velocity(solution: &Solution, x: f64, y: f64) -> Option<(f64, f64)> {
    let mesh = &solution.mesh;
    if !is_fluid_point(mesh, x, y) {
        return None;
    }
    let fx = (x / mesh.dx - 0.5).clamp(0.0, (mesh.nx - 1) as f64);
    let fy = (y / mesh.dy - 0.5).clamp(0.0, (mesh.ny - 1) as f64);
    let i0 = fx.floor() as usize;
    let j0 = fy.floor() as usize;
    let i1 = (i0 + 1).min(mesh.nx - 1);
    let j1 = (j0 + 1).min(mesh.ny - 1);
    let tx = fx - i0 as f64;
    let ty = fy - j0 as f64;
    let value = |i: usize, j: usize, component_x: bool| {
        let index = mesh.pressure_index(i, j);
        if mesh.solid[index] {
            return 0.0;
        }
        let (ux, uy) = mesh.cell_velocity(&solution.u, &solution.v, i, j);
        if component_x {
            ux
        } else {
            uy
        }
    };
    let interpolate = |component_x: bool| {
        let lower = (1.0 - tx) * value(i0, j0, component_x) + tx * value(i1, j0, component_x);
        let upper = (1.0 - tx) * value(i0, j1, component_x) + tx * value(i1, j1, component_x);
        (1.0 - ty) * lower + ty * upper
    };
    Some((interpolate(true), interpolate(false)))
}

fn render_streamlines(solution: &Solution, lines: &[Vec<(f64, f64)>]) -> String {
    let mesh = &solution.mesh;
    let cell_size = 18.0;
    let margin = 24.0;
    let plot_width = mesh.nx as f64 * cell_size;
    let plot_height = mesh.ny as f64 * cell_size;
    let width = plot_width + 2.0 * margin;
    let height = plot_height + 2.0 * margin + 52.0;
    let max_speed = solution
        .u
        .iter()
        .chain(solution.v.iter())
        .fold(0.0_f64, |maximum, value| maximum.max(value.abs()));

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\">"
    );
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{title_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"18\" font-weight=\"600\">OpenCFD streamlines</text>",
        title_y = margin + 4.0,
    );
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{range_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"12\" fill=\"#444\">{count} lines, maximum face speed {max_speed:.6} m/s</text>",
        range_y = margin + 28.0,
        count = lines.len(),
    );

    let top = margin + 52.0;
    for (index, value) in mesh.solid.iter().enumerate() {
        if !*value {
            continue;
        }
        let i = index % mesh.nx;
        let j = index / mesh.nx;
        let _ = writeln!(
            svg,
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{cell_size:.2}\" height=\"{cell_size:.2}\" fill=\"#d9d9d9\"/>",
            x = margin + i as f64 * cell_size,
            y = top + (mesh.ny - 1 - j) as f64 * cell_size,
        );
    }

    let to_pixel = |x: f64, y: f64| {
        (
            margin + x / mesh.length_x * plot_width,
            top + (1.0 - y / mesh.length_y) * plot_height,
        )
    };
    for line in lines {
        let mut points = String::new();
        for (x, y) in line {
            let (px, py) = to_pixel(*x, *y);
            let _ = write!(points, "{px:.2},{py:.2} ");
        }
        let _ = writeln!(
            svg,
            "<polyline points=\"{}\" fill=\"none\" stroke=\"#1d4ed8\" stroke-width=\"1.1\" stroke-linecap=\"round\" opacity=\"0.85\"/>",
            points.trim_end()
        );
    }
    let _ = writeln!(
        svg,
        "<rect x=\"{margin:.2}\" y=\"{top:.2}\" width=\"{plot_width:.2}\" height=\"{plot_height:.2}\" fill=\"none\" stroke=\"#222\" stroke-width=\"1\"/>"
    );
    svg.push_str("</svg>\n");
    svg
}

/// Samples the staggered solution as a quad mesh over the pressure cells.
///
/// Point `j * nx + i` is the centre of cell `(i, j)`, matching
/// [`crate::mesh::Mesh::pressure_index`], so the unstructured grid preserves
/// the structured i/j layout without moving any value.
fn field_mesh(solution: &Solution) -> FieldMesh {
    let mesh = &solution.mesh;
    let mut points = Vec::with_capacity(mesh.pressure_count());
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let (x, y) = mesh.cell_center(i, j);
            points.push([x, y, 0.0]);
        }
    }

    let columns = mesh.nx.saturating_sub(1);
    let rows = mesh.ny.saturating_sub(1);
    let mut cells = Vec::with_capacity(columns * rows);
    for j in 0..rows {
        for i in 0..columns {
            cells.push(Cell::new(
                CellKind::Quad,
                vec![
                    mesh.pressure_index(i, j),
                    mesh.pressure_index(i + 1, j),
                    mesh.pressure_index(i + 1, j + 1),
                    mesh.pressure_index(i, j + 1),
                ],
            ));
        }
    }
    FieldMesh::new(points, cells)
}

/// Field values in the same i/j row-major order as the sampled points.
fn solution_fields(solution: &Solution) -> Vec<Field> {
    let mesh = &solution.mesh;
    let mut velocity = Vec::with_capacity(mesh.pressure_count() * 3);
    let mut magnitude = Vec::with_capacity(mesh.pressure_count());
    let mut temperature = Vec::with_capacity(mesh.pressure_count());
    for j in 0..mesh.ny {
        for i in 0..mesh.nx {
            let (ux, uy) = mesh.cell_velocity(&solution.u, &solution.v, i, j);
            velocity.extend_from_slice(&[ux, uy, 0.0]);
            magnitude.push((ux * ux + uy * uy).sqrt());
            temperature.push(temperature_at(solution, i, j));
        }
    }
    vec![
        Field::scalar("pressure", solution.p.clone()),
        Field::vector("velocity", velocity),
        Field::scalar("magnitude", magnitude),
        Field::scalar("temperature", temperature),
        Field::scalar("vorticity", solution.vorticity.clone()),
        Field::scalar("streamFunction", solution.stream_function.clone()),
    ]
}

fn mesh_error(error: science_mesh::MeshError) -> CfdError {
    CfdError::new("export/mesh", error.message().to_string())
}

fn temperature_at(solution: &Solution, i: usize, j: usize) -> f64 {
    let index = solution.mesh.pressure_index(i, j);
    if solution.mesh.solid[index] {
        return 0.0;
    }
    solution
        .temperature
        .as_ref()
        .and_then(|field| field.get(index))
        .copied()
        .unwrap_or(0.0)
}

fn with_extension_or_file(path: &Path, extension: &str, default_name: &str) -> PathBuf {
    if path.extension().and_then(|value| value.to_str()) == Some(extension) {
        path.to_path_buf()
    } else {
        path.join(default_name)
    }
}
