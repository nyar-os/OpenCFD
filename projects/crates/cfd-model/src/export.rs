use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use science_mesh::{Cell, CellKind, Field, Mesh as FieldMesh};

use crate::solver::Solution;
use crate::CfdError;

/// Cell-centred scalar fields available for SVG previews and contours.
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
    /// Cell-centred speed `√(u² + v²)` in m/s.
    Speed,
    /// Turbulent kinetic energy `k` in m²/s² (`kEpsilon` model).
    TurbulentKineticEnergy,
    /// Turbulent dissipation rate `ε` in m²/s³ (`kEpsilon` model).
    TurbulentDissipation,
    /// Pressure coefficient `2(p − pRef)/(ρU²)` with `pRef = 0` (gauge) and
    /// `U` the resolved force reference speed.
    PressureCoefficient,
}

impl ExportField {
    /// Identifier used by the CLI and TypeScript API.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pressure => "pressure",
            Self::Temperature => "temperature",
            Self::Vorticity => "vorticity",
            Self::StreamFunction => "streamFunction",
            Self::Speed => "speed",
            Self::TurbulentKineticEnergy => "turbulentKineticEnergy",
            Self::TurbulentDissipation => "turbulentDissipation",
            Self::PressureCoefficient => "pressureCoefficient",
        }
    }

    /// Physical unit shown in the legend.
    pub fn unit(self) -> &'static str {
        match self {
            Self::Pressure => "Pa",
            Self::Temperature => "K",
            Self::Vorticity => "1/s",
            Self::StreamFunction => "m²/s",
            Self::Speed => "m/s",
            Self::TurbulentKineticEnergy => "m²/s²",
            Self::TurbulentDissipation => "m²/s³",
            Self::PressureCoefficient => "-",
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
            "speed" | "magnitude" => Ok(Self::Speed),
            "turbulentKineticEnergy" | "turbulent-kinetic-energy" | "tke" | "k" => {
                Ok(Self::TurbulentKineticEnergy)
            }
            "turbulentDissipation" | "turbulent-dissipation" | "epsilon" => {
                Ok(Self::TurbulentDissipation)
            }
            "pressureCoefficient" | "pressure-coefficient" | "cp" => Ok(Self::PressureCoefficient),
            other => Err(CfdError::new(
                "export/invalid-field",
                format!(
                    "unknown export field `{other}`; only pressure, temperature, vorticity, streamFunction, speed, turbulentKineticEnergy, turbulentDissipation or pressureCoefficient is supported"
                ),
            )),
        }
    }
}

/// Sampling layout of a CSV or VTK field export.
///
/// `Cell` writes one point per pressure cell centre, so the point index is the
/// mesh's pressure index. `Node` writes one point per mesh vertex, i.e. the
/// `(nx + 1) × (ny + 1)` corners of the cells, with every value averaged from
/// the adjacent fluid cells. Solid cells contribute nothing, so a vertex that
/// only touches solids reports zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportLayout {
    /// Cell-centred values in pressure-cell order.
    #[default]
    Cell,
    /// Node (vertex) values averaged from the adjacent cell centres.
    Node,
}

impl ExportLayout {
    /// Returns the lowercase identifier used by the CLI and the SDK.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cell => "cell",
            Self::Node => "node",
        }
    }
}

impl FromStr for ExportLayout {
    type Err = CfdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cell" | "cells" | "cell-centred" | "cellCentered" => Ok(Self::Cell),
            "node" | "nodes" | "vertex" | "vertices" => Ok(Self::Node),
            other => Err(CfdError::new(
                "export/invalid-layout",
                format!("unknown export layout `{other}`; only cell or node is supported"),
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
    let values = field_values(solution, field)?;
    let text = render_svg(solution, &values, field);
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Copies one cell-centred field in pressure-cell order.
///
/// `speed` is derived from the staggered velocity; solid cells report zero so
/// every consumer sees the same value as CSV, VTK and the SVG preview.
fn field_values(solution: &Solution, field: ExportField) -> Result<Vec<f64>, CfdError> {
    let mesh = &solution.mesh;
    let values = match field {
        ExportField::Pressure => solution.p.clone(),
        ExportField::Temperature => solution.temperature.clone().ok_or_else(|| {
            CfdError::new(
                "export/no-field",
                "the case does not configure scalar (temperature) transport, so temperature cannot be exported",
            )
        })?,
        ExportField::Vorticity => solution.vorticity.clone(),
        ExportField::StreamFunction => solution.stream_function.clone(),
        ExportField::PressureCoefficient => solution.pressure_coefficient.clone(),
        ExportField::Speed => {
            let mut speed = vec![0.0; mesh.pressure_count()];
            for j in 0..mesh.ny {
                for i in 0..mesh.nx {
                    let index = mesh.pressure_index(i, j);
                    if mesh.solid[index] {
                        continue;
                    }
                    let (ux, uy) = mesh.cell_velocity(&solution.u, &solution.v, i, j);
                    speed[index] = ux.hypot(uy);
                }
            }
            speed
        }
        ExportField::TurbulentKineticEnergy => {
            solution.turbulent_kinetic_energy.clone().ok_or_else(|| {
                CfdError::new(
                    "export/no-field",
                    "the case does not use the kEpsilon turbulence model, so turbulent kinetic energy cannot be exported",
                )
            })?
        }
        ExportField::TurbulentDissipation => {
            solution.turbulent_dissipation.clone().ok_or_else(|| {
                CfdError::new(
                    "export/no-field",
                    "the case does not use the kEpsilon turbulence model, so turbulent dissipation cannot be exported",
                )
            })?
        }
    };
    Ok(values)
}

fn render_svg(solution: &Solution, values: &[f64], field: ExportField) -> String {
    let mesh = &solution.mesh;
    let (min, max) = field_range(mesh, values);

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

/// Range of one field over fluid cells, padded when the field is constant.
fn field_range(mesh: &crate::mesh::Mesh, values: &[f64]) -> (f64, f64) {
    let (mut min, mut max) = (f64::INFINITY, f64::NEG_INFINITY);
    for (index, value) in values.iter().enumerate() {
        if mesh.solid[index] || !value.is_finite() {
            continue;
        }
        min = min.min(*value);
        max = max.max(*value);
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 0.0);
    }
    if (max - min).abs() < 1e-15 {
        let pad = if min.abs() > 1e-12 {
            min.abs() * 0.05
        } else {
            1.0
        };
        (min - pad, max + pad)
    } else {
        (min, max)
    }
}

/// Writes a filled-contour and iso-line SVG of one cell-centred field.
///
/// The field is sampled at pressure-cell centres. Each interior quad is split
/// into `PALETTE.len()` value bands by linear interpolation along its edges
/// (Sutherland–Hodgman clipping in value space), so the filled regions follow
/// the solution rather than the cell boundaries. Iso-lines at the band edges
/// are traced with marching squares and drawn over the fill. Solid cells are
/// dark grey and the file carries a discrete colour legend with the physical
/// range. When `path` ends in `.svg` it is used as the output file; otherwise it
/// is treated as a directory and `solution-<field>-contours.svg` is created
/// inside it.
pub fn export_contours(
    solution: &Solution,
    path: &Path,
    field: ExportField,
) -> Result<PathBuf, CfdError> {
    let default_name = format!("solution-{}-contours.svg", field.as_str());
    let target = with_extension_or_file(path, "svg", &default_name);
    let values = field_values(solution, field)?;
    let text = render_contours(solution, &values, field);
    write_atomic(&target, &text)?;
    Ok(target)
}

fn render_contours(solution: &Solution, values: &[f64], field: ExportField) -> String {
    let mesh = &solution.mesh;
    let (min, max) = field_range(mesh, values);
    let bands = PALETTE.len();
    let levels: Vec<f64> = (0..=bands)
        .map(|band| min + (max - min) * band as f64 / bands as f64)
        .collect();

    let cell_size = 18.0;
    let margin = 24.0;
    let plot_width = mesh.nx as f64 * cell_size;
    let plot_height = mesh.ny as f64 * cell_size;
    let legend_height = 96.0;
    let width = plot_width + 2.0 * margin;
    let height = plot_height + 2.0 * margin + 52.0 + legend_height;
    let top = margin + 52.0;

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\">"
    );
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>\n");
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{title_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"18\" font-weight=\"600\">OpenCFD {name} contours ({unit})</text>",
        title_y = margin + 4.0,
        name = field.as_str(),
        unit = field.unit(),
    );
    let _ = writeln!(
        svg,
        "<text x=\"{margin:.0}\" y=\"{range_y:.0}\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"12\" fill=\"#444\">{bands} filled bands, range {min:.6} – {max:.6} {unit}</text>",
        range_y = margin + 28.0,
        unit = field.unit(),
    );

    let to_pixel = |x: f64, y: f64| {
        (
            margin + x / mesh.length_x * plot_width,
            top + (1.0 - y / mesh.length_y) * plot_height,
        )
    };

    for (index, solid) in mesh.solid.iter().enumerate() {
        if !*solid {
            continue;
        }
        let i = index % mesh.nx;
        let j = index / mesh.nx;
        let _ = writeln!(
            svg,
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{cell_size:.2}\" height=\"{cell_size:.2}\" fill=\"#3a3a3a\"/>",
            x = margin + i as f64 * cell_size,
            y = top + (mesh.ny - 1 - j) as f64 * cell_size,
        );
    }

    for j in 0..mesh.ny.saturating_sub(1) {
        for i in 0..mesh.nx.saturating_sub(1) {
            let indices = [
                mesh.pressure_index(i, j),
                mesh.pressure_index(i + 1, j),
                mesh.pressure_index(i + 1, j + 1),
                mesh.pressure_index(i, j + 1),
            ];
            if indices.iter().any(|index| mesh.solid[*index]) {
                continue;
            }
            let corners = [
                ((i as f64 + 0.5) * mesh.dx, (j as f64 + 0.5) * mesh.dy),
                ((i as f64 + 1.5) * mesh.dx, (j as f64 + 0.5) * mesh.dy),
                ((i as f64 + 1.5) * mesh.dx, (j as f64 + 1.5) * mesh.dy),
                ((i as f64 + 0.5) * mesh.dx, (j as f64 + 1.5) * mesh.dy),
            ];
            let quad: Vec<Vertex> = corners
                .iter()
                .zip(indices.iter())
                .map(|((x, y), index)| (*x, *y, values[*index]))
                .collect();
            for band in 0..bands {
                let polygon = band_polygon(&quad, levels[band], levels[band + 1]);
                if polygon.len() < 3 {
                    continue;
                }
                let mut points = String::new();
                for (x, y) in polygon {
                    let (px, py) = to_pixel(x, y);
                    let _ = write!(points, "{px:.2},{py:.2} ");
                }
                let _ = writeln!(
                    svg,
                    "<polygon points=\"{}\" fill=\"{color}\" stroke=\"{color}\" stroke-width=\"0.4\"/>",
                    points.trim_end(),
                    color = PALETTE[band],
                );
            }
        }
    }

    for level in levels.iter().skip(1).take(bands - 1) {
        for ((x0, y0), (x1, y1)) in marching_squares_segments(mesh, values, *level) {
            let (px0, py0) = to_pixel(x0, y0);
            let (px1, py1) = to_pixel(x1, y1);
            let _ = writeln!(
                svg,
                "<line x1=\"{px0:.2}\" y1=\"{py0:.2}\" x2=\"{px1:.2}\" y2=\"{py1:.2}\" stroke=\"#202020\" stroke-width=\"0.7\" opacity=\"0.75\"/>"
            );
        }
    }

    let _ = writeln!(
        svg,
        "<rect x=\"{margin:.2}\" y=\"{top:.2}\" width=\"{plot_width:.2}\" height=\"{plot_height:.2}\" fill=\"none\" stroke=\"#222\" stroke-width=\"1\"/>"
    );

    let legend_y = top + plot_height + 30.0;
    let legend_width = plot_width;
    let stop_width = legend_width / bands as f64;
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
        "<text x=\"{middle:.2}\" y=\"{label_y:.2}\" text-anchor=\"middle\" font-family=\"Segoe UI, Arial, sans-serif\" font-size=\"11\" fill=\"#666\">{iso} iso-lines between bands, solid cells dark grey</text>",
        middle = margin + 0.5 * legend_width,
        iso = bands - 1,
    );
    svg.push_str("</svg>\n");
    svg
}

/// One contour vertex in physical coordinates and field value.
type Vertex = (f64, f64, f64);

/// Sutherland–Hodgman clip of a value-tagged polygon to `lo <= value <= hi`.
fn band_polygon(polygon: &[Vertex], low: f64, high: f64) -> Vec<(f64, f64)> {
    let lower = clip_value(polygon, low, true);
    clip_value(&lower, high, false)
        .into_iter()
        .map(|(x, y, _)| (x, y))
        .collect()
}

fn clip_value(polygon: &[Vertex], level: f64, keep_above: bool) -> Vec<Vertex> {
    if polygon.is_empty() {
        return Vec::new();
    }
    let inside = |vertex: &Vertex| {
        if keep_above {
            vertex.2 >= level
        } else {
            vertex.2 <= level
        }
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);
    for index in 0..polygon.len() {
        let current = polygon[index];
        let next = polygon[(index + 1) % polygon.len()];
        let current_inside = inside(&current);
        let next_inside = inside(&next);
        if current_inside {
            output.push(current);
        }
        if current_inside != next_inside {
            let t = (level - current.2) / (next.2 - current.2);
            output.push((
                current.0 + t * (next.0 - current.0),
                current.1 + t * (next.1 - current.1),
                level,
            ));
        }
    }
    output
}

/// Marching-squares iso-line segments at `level` over the pressure-cell centres.
///
/// Corners of every interior quad are its four cell centres; segments are
/// interpolated linearly along the crossed edges. Ambiguous saddle cells are
/// resolved with the average corner value, which is deterministic and keeps
/// neighbouring cells consistent.
fn marching_squares_segments(
    mesh: &crate::mesh::Mesh,
    values: &[f64],
    level: f64,
) -> Vec<((f64, f64), (f64, f64))> {
    let mut segments = Vec::new();
    for j in 0..mesh.ny.saturating_sub(1) {
        for i in 0..mesh.nx.saturating_sub(1) {
            let cells = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
            if cells.iter().any(|(ci, cj)| mesh.is_solid(*ci, *cj)) {
                continue;
            }
            let corners: Vec<(f64, f64, f64)> = cells
                .iter()
                .map(|(ci, cj)| {
                    let (x, y) = mesh.cell_center(*ci, *cj);
                    (x, y, values[mesh.pressure_index(*ci, *cj)])
                })
                .collect();
            let mut crossings: Vec<(usize, (f64, f64))> = Vec::new();
            for (edge, &(a, b)) in [(0usize, 1usize), (1, 2), (2, 3), (3, 0)]
                .iter()
                .enumerate()
            {
                let (pa, pb) = (corners[a], corners[b]);
                if (pa.2 < level) != (pb.2 < level) {
                    let t = (level - pa.2) / (pb.2 - pa.2);
                    crossings.push((edge, (pa.0 + t * (pb.0 - pa.0), pa.1 + t * (pb.1 - pa.1))));
                }
            }
            match crossings.len() {
                2 => segments.push((crossings[0].1, crossings[1].1)),
                4 => {
                    let centre = corners.iter().map(|corner| corner.2).sum::<f64>() / 4.0;
                    let pairs: [(usize, usize); 2] = if centre >= level {
                        // Bottom-right and top-left corners are above the level.
                        [(0, 1), (2, 3)]
                    } else {
                        [(0, 3), (1, 2)]
                    };
                    for (first, second) in pairs {
                        let point = |edge: usize| {
                            crossings
                                .iter()
                                .find(|(crossing, _)| *crossing == edge)
                                .map(|(_, point)| *point)
                        };
                        if let (Some(start), Some(end)) = (point(first), point(second)) {
                            segments.push((start, end));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    segments
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
    export_csv_with_layout(solution, path, ExportLayout::Cell)
}

/// Writes cell-centred or node-centred CSV rows.
///
/// The `Cell` layout writes `nx × ny` rows in pressure-cell order with the
/// header `x,y,z,pressure,velocity_x,velocity_y,velocity_z,magnitude,temperature,vorticity,streamFunction`.
/// The `Node` layout writes `(nx + 1) × (ny + 1)` rows in vertex order
/// (`j * (nx + 1) + i`) with the same header; every value is the mean of the
/// adjacent fluid cell centres. Omitted values (for example `temperature`
/// without scalar transport) are written as zero. A directory receives
/// `solution.csv` for the cell layout and `solution-nodes.csv` for the node
/// layout.
pub fn export_csv_with_layout(
    solution: &Solution,
    path: &Path,
    layout: ExportLayout,
) -> Result<PathBuf, CfdError> {
    let default_name = match layout {
        ExportLayout::Cell => "solution.csv",
        ExportLayout::Node => "solution-nodes.csv",
    };
    let target = with_extension_or_file(path, "csv", default_name);
    let mesh = field_mesh(solution, layout);
    let text = mesh
        .to_csv(&solution_fields(solution, layout))
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
    export_vtk_with_layout(solution, path, ExportLayout::Cell)
}

/// Writes a legacy ASCII VTK `UNSTRUCTURED_GRID` over the cells or the nodes.
///
/// The `Cell` layout keeps the historical sampling: points are cell centres and
/// every quad joins four neighbouring centres. The `Node` layout samples the
/// `(nx + 1) × (ny + 1)` mesh vertices and joins them into `nx × ny` quads, so
/// the VTK grid covers the full rectangle with one point per vertex. Both
/// layouts carry the same point data: `pressure`, `velocity`, `magnitude`,
/// `temperature`, `vorticity` and `streamFunction`. A directory receives
/// `solution.vtk` for the cell layout and `solution-nodes.vtk` for the node
/// layout.
pub fn export_vtk_with_layout(
    solution: &Solution,
    path: &Path,
    layout: ExportLayout,
) -> Result<PathBuf, CfdError> {
    let default_name = match layout {
        ExportLayout::Cell => "solution.vtk",
        ExportLayout::Node => "solution-nodes.vtk",
    };
    let target = with_extension_or_file(path, "vtk", default_name);
    let mesh = field_mesh(solution, layout);
    let text = mesh
        .to_legacy_vtk(&solution_fields(solution, layout))
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
/// The header is
/// `obstacle,kind,step,fx,fy,fxPressure,fxViscous,fyPressure,fyViscous,mz,cd,cl,cm`;
/// each obstacle contributes one row per sample it recorded, grouped in
/// document order. The pressure and viscous columns sum to `fx` and `fy`,
/// while `mz` remains the total moment. When `path` ends in `.csv` it is used
/// as the output file; otherwise it is treated as a directory and
/// `forces.csv` is created inside it.
pub fn export_forces(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "forces.csv");
    let mut text = String::from(
        "obstacle,kind,step,fx,fy,fxPressure,fxViscous,fyPressure,fyViscous,mz,cd,cl,cm\n",
    );
    for force in &solution.report.forces {
        for sample in &force.history {
            let _ = writeln!(
                text,
                "{},{},{},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e}",
                force.id,
                force.kind,
                sample.step,
                sample.fx,
                sample.fy,
                sample.fx_pressure,
                sample.fx_viscous,
                sample.fy_pressure,
                sample.fy_viscous,
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

/// Writes one final drag, lift and moment row per obstacle.
///
/// The header is
/// `obstacle,kind,cd,cl,cm,fx,fy,mz,referenceVelocity,referenceArea,referenceLength`;
/// each obstacle contributes exactly one row taken from the final solve state,
/// in document order. Unlike [`export_forces`] this is a summary, not a time
/// history. When `path` ends in `.csv` it is used as the output file; otherwise
/// it is treated as a directory and `coefficients.csv` is created inside it.
pub fn export_coefficients(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "coefficients.csv");
    let mut text = String::from(
        "obstacle,kind,cd,cl,cm,fx,fy,mz,referenceVelocity,referenceArea,referenceLength\n",
    );
    for force in &solution.report.forces {
        let _ = writeln!(
            text,
            "{},{},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e}",
            force.id,
            force.kind,
            force.cd,
            force.cl,
            force.cm,
            force.fx,
            force.fy,
            force.mz,
            force.reference_velocity,
            force.reference_area,
            force.reference_length
        );
    }
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Straight sampling segment used by [`export_profile`].
///
/// The four coordinates are the start `(x0, y0)` and the end `(x1, y1)` in
/// metres. Both endpoints must be finite and distinct.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProfileLine {
    /// Start point x coordinate in metres.
    pub x0: f64,
    /// Start point y coordinate in metres.
    pub y0: f64,
    /// End point x coordinate in metres.
    pub x1: f64,
    /// End point y coordinate in metres.
    pub y1: f64,
}

impl ProfileLine {
    /// Builds a line from its start and end coordinates.
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Length of the segment in metres.
    ///
    /// Fails with `export/invalid-line` when a coordinate is not finite or the
    /// two endpoints coincide.
    pub fn length(self) -> Result<f64, CfdError> {
        if ![self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(CfdError::new(
                "export/invalid-line",
                "line coordinates must be finite numbers",
            ));
        }
        let length = (self.x1 - self.x0).hypot(self.y1 - self.y0);
        if length == 0.0 {
            return Err(CfdError::new(
                "export/invalid-line",
                "line start and end must be different points",
            ));
        }
        Ok(length)
    }
}

/// Writes a cell-centred field sampled along a straight segment as CSV rows.
///
/// The rows are the `samples` equally spaced points with `t = i / (samples − 1)`
/// from the start to the end of the line. The header is `distance,x,y,value`,
/// where `distance` is the distance from the start in metres. The value is the
/// field at the centre of the cell that contains the sample point, exactly the
/// value the cell-centred CSV export reports. A sample outside the domain or
/// inside a solid cell has no value, so its `value` column is left empty rather
/// than written as zero. `samples` must be at least two. When `path` ends in
/// `.csv` it is used as the output file; otherwise it is treated as a directory
/// and `profile.csv` is created inside it.
pub fn export_profile(
    solution: &Solution,
    path: &Path,
    line: ProfileLine,
    samples: usize,
    field: ExportField,
) -> Result<PathBuf, CfdError> {
    let length = line.length()?;
    if samples < 2 {
        return Err(CfdError::new(
            "export/invalid-samples",
            "profile samples must be at least 2",
        ));
    }
    let target = with_extension_or_file(path, "csv", "profile.csv");
    let values = field_values(solution, field)?;
    let mut text = String::from("distance,x,y,value\n");
    let denominator = (samples - 1) as f64;
    for sample in 0..samples {
        let t = sample as f64 / denominator;
        let x = line.x0 + t * (line.x1 - line.x0);
        let y = line.y0 + t * (line.y1 - line.y0);
        let distance = t * length;
        match cell_value(solution, &values, x, y) {
            Some(value) => {
                let _ = writeln!(text, "{distance:.10e},{x:.10e},{y:.10e},{value:.10e}");
            }
            None => {
                let _ = writeln!(text, "{distance:.10e},{x:.10e},{y:.10e},");
            }
        }
    }
    write_atomic(&target, &text)?;
    Ok(target)
}

/// Value of a cell-centred field at the fluid cell containing `(x, y)`.
///
/// A point outside the domain or inside a solid cell has no value and returns
/// `None`, so callers never invent a zero for a non-fluid sample.
fn cell_value(solution: &Solution, values: &[f64], x: f64, y: f64) -> Option<f64> {
    let mesh = &solution.mesh;
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    if x < 0.0 || y < 0.0 || x > mesh.length_x || y > mesh.length_y {
        return None;
    }
    let i = ((x / mesh.dx).floor() as isize).clamp(0, mesh.nx as isize - 1) as usize;
    let j = ((y / mesh.dy).floor() as isize).clamp(0, mesh.ny as isize - 1) as usize;
    let index = mesh.pressure_index(i, j);
    if mesh.solid[index] {
        return None;
    }
    let value = values[index];
    value.is_finite().then_some(value)
}

/// Summary of one field over the fluid cells.
#[derive(Debug, Clone, Copy, PartialEq)]
struct FieldSummary {
    min: f64,
    max: f64,
    mean: f64,
    rms: f64,
    count: usize,
}

/// Writes deterministic statistics of every available field as JSON.
///
/// The document tag is `open-cfd-statistics` and the object starts with the mesh
/// (`nx`, `ny`, `cells`, `fluidCells`) followed by one entry per field in a
/// fixed order: `speed`, `pressure`, `u`, `v`, `vorticity`, `streamFunction`,
/// and, when the case provides them, `temperature`, `turbulentKineticEnergy`
/// and `turbulentDissipation`. Every entry is `{min, max, mean, rms, count}`
/// over the fluid cells only, the same cells the CSV export writes. Solid cells
/// and non-finite values are skipped. A field without a single fluid value
/// reports `count: 0` and `null` statistics instead of an invented number.
/// When `path` ends in `.json` it is used as the output file; otherwise it is
/// treated as a directory and `statistics.json` is created inside it.
pub fn export_statistics(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "json", "statistics.json");
    let text = render_statistics(solution);
    write_atomic(&target, &text)?;
    Ok(target)
}

fn render_statistics(solution: &Solution) -> String {
    let mesh = &solution.mesh;
    let fluid_cells = (0..mesh.pressure_count())
        .filter(|index| !mesh.solid[*index])
        .count();
    let mut fields: Vec<(&str, Option<FieldSummary>)> = vec![
        (
            "speed",
            summarize(mesh, |index| {
                let (ux, uy) =
                    mesh.cell_velocity(&solution.u, &solution.v, index % mesh.nx, index / mesh.nx);
                ux.hypot(uy)
            }),
        ),
        ("pressure", summarize(mesh, |index| solution.p[index])),
        (
            "u",
            summarize(mesh, |index| {
                mesh.cell_velocity(&solution.u, &solution.v, index % mesh.nx, index / mesh.nx)
                    .0
            }),
        ),
        (
            "v",
            summarize(mesh, |index| {
                mesh.cell_velocity(&solution.u, &solution.v, index % mesh.nx, index / mesh.nx)
                    .1
            }),
        ),
        (
            "vorticity",
            summarize(mesh, |index| solution.vorticity[index]),
        ),
        (
            "streamFunction",
            summarize(mesh, |index| solution.stream_function[index]),
        ),
    ];
    if let Some(values) = &solution.temperature {
        fields.push(("temperature", summarize(mesh, |index| values[index])));
    }
    if let Some(values) = &solution.turbulent_kinetic_energy {
        fields.push((
            "turbulentKineticEnergy",
            summarize(mesh, |index| values[index]),
        ));
    }
    if let Some(values) = &solution.turbulent_dissipation {
        fields.push((
            "turbulentDissipation",
            summarize(mesh, |index| values[index]),
        ));
    }

    let mut text = String::new();
    let _ = writeln!(text, "{{");
    let _ = writeln!(text, "  \"format\": \"open-cfd-statistics\",");
    let _ = writeln!(text, "  \"mesh\": {{");
    let _ = writeln!(text, "    \"nx\": {},", mesh.nx);
    let _ = writeln!(text, "    \"ny\": {},", mesh.ny);
    let _ = writeln!(text, "    \"cells\": {},", mesh.pressure_count());
    let _ = writeln!(text, "    \"fluidCells\": {fluid_cells}");
    let _ = writeln!(text, "  }},");
    let _ = writeln!(text, "  \"fields\": {{");
    for (position, (name, summary)) in fields.iter().enumerate() {
        let comma = if position + 1 == fields.len() {
            ""
        } else {
            ","
        };
        let _ = writeln!(text, "    \"{name}\": {}{comma}", summary_json(*summary));
    }
    let _ = writeln!(text, "  }}");
    let _ = writeln!(text, "}}");
    text
}

/// Statistics of `value` over the fluid cells with a finite sample.
fn summarize(mesh: &crate::mesh::Mesh, value: impl Fn(usize) -> f64) -> Option<FieldSummary> {
    let mut summary = FieldSummary {
        min: f64::INFINITY,
        max: f64::NEG_INFINITY,
        mean: 0.0,
        rms: 0.0,
        count: 0,
    };
    let mut sum = 0.0;
    let mut square_sum = 0.0;
    for index in 0..mesh.pressure_count() {
        if mesh.solid[index] {
            continue;
        }
        let sample = value(index);
        if !sample.is_finite() {
            continue;
        }
        summary.min = summary.min.min(sample);
        summary.max = summary.max.max(sample);
        sum += sample;
        square_sum += sample * sample;
        summary.count += 1;
    }
    if summary.count == 0 {
        return None;
    }
    let count = summary.count as f64;
    summary.mean = sum / count;
    summary.rms = (square_sum / count).sqrt();
    Some(summary)
}

fn summary_json(summary: Option<FieldSummary>) -> String {
    match summary {
        Some(summary) => format!(
            "{{\"min\": {:.10e}, \"max\": {:.10e}, \"mean\": {:.10e}, \"rms\": {:.10e}, \"count\": {}}}",
            summary.min, summary.max, summary.mean, summary.rms, summary.count
        ),
        None => "{\"min\": null, \"max\": null, \"mean\": null, \"rms\": null, \"count\": 0}"
            .to_string(),
    }
}

/// Writes every sampled probe reading as long-form CSV.
///
/// The header is `probe,x,y,step,time,u,v,speed,pressure,temperature`; each
/// probe contributes one row per sample it recorded, grouped in document order.
/// `temperature` is left empty when no scalar field is configured. When `path`
/// ends in `.csv` it is used as the output file; otherwise it is treated as a
/// directory and `probes.csv` is created inside it.
pub fn export_probes(solution: &Solution, path: &Path) -> Result<PathBuf, CfdError> {
    let target = with_extension_or_file(path, "csv", "probes.csv");
    let mut text = String::from("probe,x,y,step,time,u,v,speed,pressure,temperature\n");
    for probe in &solution.report.probes {
        for sample in &probe.history {
            let temperature = sample
                .temperature
                .map(|value| format!("{value:.10e}"))
                .unwrap_or_default();
            let _ = writeln!(
                text,
                "{},{:.10e},{:.10e},{},{:.10e},{:.10e},{:.10e},{:.10e},{:.10e},{}",
                probe.id,
                probe.x,
                probe.y,
                sample.step,
                sample.time,
                sample.u,
                sample.v,
                sample.speed,
                sample.pressure,
                temperature
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

/// Samples the staggered solution as a quad mesh over the cells or the nodes.
///
/// `Cell`: point `j * nx + i` is the centre of cell `(i, j)`, matching
/// [`crate::mesh::Mesh::pressure_index`], so the unstructured grid preserves
/// the structured i/j layout without moving any value.
///
/// `Node`: point `j * (nx + 1) + i` is the vertex at `(i·dx, j·dy)` and the
/// `nx × ny` quads join the four vertices of every cell, so the grid covers the
/// full rectangle.
fn field_mesh(solution: &Solution, layout: ExportLayout) -> FieldMesh {
    let mesh = &solution.mesh;
    match layout {
        ExportLayout::Cell => {
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
        ExportLayout::Node => {
            let columns = mesh.nx + 1;
            let mut points = Vec::with_capacity(columns * (mesh.ny + 1));
            for j in 0..=mesh.ny {
                for i in 0..=mesh.nx {
                    points.push([i as f64 * mesh.dx, j as f64 * mesh.dy, 0.0]);
                }
            }

            let mut cells = Vec::with_capacity(mesh.nx * mesh.ny);
            for j in 0..mesh.ny {
                for i in 0..mesh.nx {
                    let bottom_left = node_index(mesh, i, j);
                    cells.push(Cell::new(
                        CellKind::Quad,
                        vec![
                            bottom_left,
                            bottom_left + 1,
                            bottom_left + 1 + columns,
                            bottom_left + columns,
                        ],
                    ));
                }
            }
            FieldMesh::new(points, cells)
        }
    }
}

/// Index of mesh vertex `(i, j)`, row-major over `nx + 1` columns.
fn node_index(mesh: &crate::mesh::Mesh, i: usize, j: usize) -> usize {
    j * (mesh.nx + 1) + i
}

/// Field values in the same row-major order as the sampled points.
fn solution_fields(solution: &Solution, layout: ExportLayout) -> Vec<Field> {
    let mesh = &solution.mesh;
    let count = match layout {
        ExportLayout::Cell => mesh.pressure_count(),
        ExportLayout::Node => (mesh.nx + 1) * (mesh.ny + 1),
    };
    let mut velocity = Vec::with_capacity(count * 3);
    let mut magnitude = Vec::with_capacity(count);
    let mut temperature = Vec::with_capacity(count);
    let mut pressure = Vec::with_capacity(count);
    let mut vorticity = Vec::with_capacity(count);
    let mut stream_function = Vec::with_capacity(count);
    let mut pressure_coefficient = Vec::with_capacity(count);
    let mut turbulent_kinetic_energy = Vec::with_capacity(count);
    let mut turbulent_dissipation = Vec::with_capacity(count);
    match layout {
        ExportLayout::Cell => {
            for j in 0..mesh.ny {
                for i in 0..mesh.nx {
                    let (ux, uy) = mesh.cell_velocity(&solution.u, &solution.v, i, j);
                    velocity.extend_from_slice(&[ux, uy, 0.0]);
                    magnitude.push((ux * ux + uy * uy).sqrt());
                    temperature.push(temperature_at(solution, i, j));
                    let index = mesh.pressure_index(i, j);
                    pressure.push(solution.p[index]);
                    vorticity.push(solution.vorticity[index]);
                    stream_function.push(solution.stream_function[index]);
                    pressure_coefficient.push(solution.pressure_coefficient[index]);
                    if let Some(field) = &solution.turbulent_kinetic_energy {
                        turbulent_kinetic_energy.push(field[index]);
                    }
                    if let Some(field) = &solution.turbulent_dissipation {
                        turbulent_dissipation.push(field[index]);
                    }
                }
            }
        }
        ExportLayout::Node => {
            for j in 0..=mesh.ny {
                for i in 0..=mesh.nx {
                    let ux = node_average(mesh, i, j, |ci, cj| {
                        mesh.cell_velocity(&solution.u, &solution.v, ci, cj).0
                    });
                    let uy = node_average(mesh, i, j, |ci, cj| {
                        mesh.cell_velocity(&solution.u, &solution.v, ci, cj).1
                    });
                    velocity.extend_from_slice(&[ux, uy, 0.0]);
                    magnitude.push(ux.hypot(uy));
                    temperature.push(node_average(mesh, i, j, |ci, cj| {
                        temperature_at(solution, ci, cj)
                    }));
                    pressure.push(node_average(mesh, i, j, |ci, cj| {
                        solution.p[mesh.pressure_index(ci, cj)]
                    }));
                    vorticity.push(node_average(mesh, i, j, |ci, cj| {
                        solution.vorticity[mesh.pressure_index(ci, cj)]
                    }));
                    stream_function.push(node_average(mesh, i, j, |ci, cj| {
                        solution.stream_function[mesh.pressure_index(ci, cj)]
                    }));
                    pressure_coefficient.push(node_average(mesh, i, j, |ci, cj| {
                        solution.pressure_coefficient[mesh.pressure_index(ci, cj)]
                    }));
                    if let Some(field) = &solution.turbulent_kinetic_energy {
                        turbulent_kinetic_energy.push(node_average(mesh, i, j, |ci, cj| {
                            field[mesh.pressure_index(ci, cj)]
                        }));
                    }
                    if let Some(field) = &solution.turbulent_dissipation {
                        turbulent_dissipation.push(node_average(mesh, i, j, |ci, cj| {
                            field[mesh.pressure_index(ci, cj)]
                        }));
                    }
                }
            }
        }
    }
    let mut fields = vec![
        Field::scalar("pressure", pressure),
        Field::vector("velocity", velocity),
        Field::scalar("magnitude", magnitude),
        Field::scalar("temperature", temperature),
        Field::scalar("vorticity", vorticity),
        Field::scalar("streamFunction", stream_function),
        Field::scalar("pressureCoefficient", pressure_coefficient),
    ];
    if !turbulent_kinetic_energy.is_empty() {
        fields.push(Field::scalar(
            "turbulentKineticEnergy",
            turbulent_kinetic_energy,
        ));
    }
    if !turbulent_dissipation.is_empty() {
        fields.push(Field::scalar("turbulentDissipation", turbulent_dissipation));
    }
    fields
}

/// Mean of `value` over the fluid cells adjacent to vertex `(i, j)`.
///
/// A vertex owns up to four surrounding cells. Solid cells are skipped, so a
/// vertex on a solid surface averages only its fluid neighbours and a vertex
/// surrounded by solids reports zero.
fn node_average(
    mesh: &crate::mesh::Mesh,
    i: usize,
    j: usize,
    value: impl Fn(usize, usize) -> f64,
) -> f64 {
    let mut sum = 0.0;
    let mut count = 0_usize;
    let i_start = i.saturating_sub(1);
    let i_end = (i + 1).min(mesh.nx);
    let j_start = j.saturating_sub(1);
    let j_end = (j + 1).min(mesh.ny);
    for cj in j_start..j_end {
        for ci in i_start..i_end {
            if mesh.is_solid(ci, cj) {
                continue;
            }
            sum += value(ci, cj);
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f64
    }
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

pub(crate) fn with_extension_or_file(path: &Path, extension: &str, default_name: &str) -> PathBuf {
    if path.extension().and_then(|value| value.to_str()) == Some(extension) {
        path.to_path_buf()
    } else {
        path.join(default_name)
    }
}

#[cfg(test)]
#[path = "../tests/unit/export.rs"]
mod tests;
