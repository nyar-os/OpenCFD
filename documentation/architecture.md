# Architecture

OpenCFD has one implementation and three entry points. The Rust model owns the
document, the solver and every export; the Node-API addon maps values across the
boundary; the TypeScript SDK and CLI only parse, forward and print. No solver
physics exists outside `cfd-model`.

## The case model

A case is one UTF-8 JSON document with the tag `format: "open-cfd-case"` and
`version: 1`, conventionally stored as `<name>.ocfd`. Fields are camelCase and
every field except `format` and `version` has a default, so a minimal document
is:

```json
{ "format": "open-cfd-case", "version": 1 }
```

A more complete document shows every optional section in place:

```json
{
  "format": "open-cfd-case",
  "version": 1,
  "units": "m",
  "mesh": { "nx": 24, "ny": 24, "lengthX": 1.0, "lengthY": 1.0 },
  "fluid": { "density": 1.0, "viscosity": 0.01 },
  "reference": { "velocity": 1.0, "area": 0.2, "length": 0.2 },
  "convection": "upwind",
  "pressureGradient": [-0.5, 0.0],
  "time": {
    "dt": 0.01,
    "steps": 200,
    "pressureIterations": 100,
    "tolerance": 1e-3,
    "scheme": "euler",
    "adaptive": false,
    "targetCfl": 0.9
  },
  "boundaries": {
    "left":   { "kind": "wall", "velocity": [0.0, 0.0] },
    "right":  { "kind": "outlet", "velocity": [0.0, 0.0], "pressure": 0.0 },
    "top":    { "kind": "wall", "velocity": [1.0, 0.0] },
    "bottom": { "kind": "wall", "velocity": [0.0, 0.0] },
    "inletProfiles": { "left": "uniform", "right": "uniform", "top": "uniform", "bottom": "uniform", "exponent": 7.0 }
  },
  "initial": { "velocity": [0.0, 0.0] },
  "obstacles": [
    { "kind": "circle", "id": "cyl", "x": 0.5, "y": 0.5, "r": 0.1 },
    { "kind": "rect", "id": "step", "x0": 0.0, "y0": 0.0, "x1": 0.2, "y1": 0.1 }
  ],
  "porous": [
    { "id": "foam", "x0": 0.6, "y0": 0.4, "x1": 0.8, "y1": 0.6, "permeability": 1e-4, "inertial": 0.5 }
  ],
  "probes": [
    { "id": "wake", "x": 0.8, "y": 0.5 }
  ],
  "scalar": { "diffusivity": 1e-3, "initial": 300.0, "inletValue": 320.0 },
  "thermal": { "specificHeat": 1000.0, "conductivity": 0.6, "heatSource": 0.0, "referenceTemperature": 300.0 },
  "wallFunction": { "kappa": 0.41, "intercept": 5.2, "minYPlus": 11.06 }
}
```

`Model` in `projects/crates/cfd-model/src/model.rs` is the in-memory form. It is
the only validator: it rejects unknown units, inconsistent meshes, invalid
boundaries, out-of-domain obstacles and probes, and non-finite values before any
file is written or any step is advanced.

| Field | Meaning |
|-------|---------|
| `format` / `version` | Fixed to `open-cfd-case` / `1`; a mismatch fails validation |
| `units` | Only `"m"` is accepted: length m, density kg/m³, viscosity Pa·s, velocity m/s, time s, pressure Pa |
| `mesh` | Uniform Cartesian mesh: cell counts `nx`, `ny` and physical sizes `lengthX`, `lengthY` |
| `fluid` | Constant `density` and constant dynamic `viscosity`; `viscosity = 0` means inviscid |
| `reference` | Optional `velocity`, `area` and `length` overrides for force and moment coefficients |
| `convection` | `upwind` (default), `central` or `quick` for momentum and scalar advection |
| `pressureGradient` | Optional `[dp/dx, dp/dy]` in Pa/m, applied as the body force `-∇p/ρ` |
| `time` | `dt`, `steps`, `pressureIterations`, `tolerance`, `scheme` (`euler` or `rk2`), `adaptive`, `targetCfl` |
| `boundaries` | One `kind`/`velocity`/`pressure` per side plus `inletProfiles` |
| `initial` | Optional uniform initial velocity; at rest by default |
| `obstacles` | Rectangles (`x0`, `y0`, `x1`, `y1`, optional `rotation`) and circles (`x`, `y`, `r`) with optional `id` |
| `porous` | Isotropic Darcy–Forchheimer rectangles with `permeability` and `inertial` |
| `probes` | Point monitors `{id, x, y}` sampled during the solve |
| `scalarSources` | Rectangular volumetric scalar sources with `rate`, `start` and `ramp` |
| `transientInlets` | Per-side `u`, `v` and `pressure` time series |
| `scalar` | Passive scalar transport: `diffusivity`, `initial`, `inletValue` |
| `turbulence` | `model` (`none`, `constant`, `mixingLength`, `kEpsilon`), `nuT`, `mixingLength`, `k`, `epsilon` |
| `thermal` | Energy-equation boundaries, heat source and reference temperature; requires `scalar` |
| `buoyancy` | Boussinesq `beta`, `tRef` and `gravity`; requires `scalar` |
| `wallFunction` | Log-law `kappa`, `intercept` and `minYPlus` for the reported wall shear |

Omitting `id` on an obstacle, porous region, probe or scalar source assigns
`obstacle-N`, `porous-N`, `probe-N` or `source-N` in document order. Repeated
edits preserve ids and order.

Boundary semantics are part of the model:

- `wall` uses no-slip or the moving-wall velocity from `velocity`; `top` with
  `[1, 0]` is the driven lid.
- `inlet` prescribes the normal component of `velocity`, scaled by the side's
  `inletProfiles` entry: `uniform`, `parabolic` (`4s(1−s)`) or `powerLaw`
  (`(1−|2s−1|)^(1/n)`).
- `outlet` uses zero-gradient velocity and the Dirichlet gauge `pressure`.
- `convective` relaxes the outlet face as `(φ_bⁿ + β·φ_i)/(1 + β)` with
  `β = max(uₙⁿ, 0)·dt/h` and the same Dirichlet pressure as `outlet`.
- `slip` is a free-slip wall: zero normal velocity and zero tangential gradient.
- `periodic` copies the opposite side; sides must be paired, and the momentum
  and pressure stencils wrap across the pair.

The document stores inputs, not solved state: every solve starts from the
document (or from a checkpoint), never from the previous solve.

## Module boundaries

| Layer | Files | Owns |
|-------|-------|------|
| Document model | `projects/crates/cfd-model/src/model.rs` | `.ocfd` parsing, defaults, validation, atomic save |
| Mesh | `projects/crates/cfd-model/src/mesh.rs` | Staggered MAC indexing, solid mask, spacing |
| Solver | `projects/crates/cfd-model/src/solver.rs` | Momentum prediction, pressure Poisson, projection, scalar/thermal/turbulence transport, reports |
| Derived fields | `projects/crates/cfd-model/src/fields.rs` | Vorticity and stream function |
| Exports | `projects/crates/cfd-model/src/export.rs` | CSV, VTK, SVG, contours, streamlines, histories, coefficients, probes, particles, profiles, statistics |
| Checkpoints | `projects/crates/cfd-model/src/checkpoint.rs` | `open-cfd-checkpoint` read/write and compatibility checks |
| Session | `projects/crates/cfd-model/src/session.rs` | `OpenCfd`: create, open, edit, validate, solve, export, save |
| Particle tracing | `projects/crates/cfd-model/src/particles.rs` | Lagrangian midpoint tracing |
| Mesh quality | `projects/crates/cfd-model/src/quality.rs` | Non-orthogonality, aspect ratio, skewness |
| Node-API | `projects/crates/cfd-napi/src/lib.rs` | Opaque u64 handles, one JSON string per operation |
| SDK | `projects/packages/open-cfd/src/{open-cfd,document,service,native,types,errors}.ts` | Session wrapper, case handle, typed requests and reports, addon resolution |
| CLI | `projects/packages/open-cfd/src/cli.ts` | Argument parsing, help text, printing, exit codes |

The Node-API crate has no schema of its own. Each export takes already-decoded
values or a JSON string, calls one `OpenCfd` method, and returns the serialized
result. Sessions live in a process-wide `HashMap<u64, OpenCfd>`; the handle is
the only state the JavaScript side holds.

## Model → Node-API → SDK/CLI flow

```text
.ocfd JSON
   │  Model::load + validate
   ▼
OpenCfd (cfd-model/src/session.rs)
   │  one method per operation
   ▼
open-cfd-napi (process-wide handle map, JSON string out)
   │  native addon loaded from @wae/open-cfd-<platform>-<arch>
   ▼
OpenCFD / CfdCase (SDK)  ──►  open-cfd CLI
```

1. `OpenCFD.create()` or `OpenCFD.open()` returns a `CfdCase` that owns one
   native handle and the latest `CaseSummary`.
2. Every edit method validates, writes the document through a temporary file
   plus rename, and returns the refreshed summary. A failed edit leaves the
   previous document intact.
3. `CfdCase.solve()` forwards one `SolveOptions` record; the Rust session
   solves and returns the full `open-cfd-report`.
4. `CfdCase.export()` solves the case and writes one file; `exportProfile()` and
   `exportStatistics()` are separate native entry points because they take a
   line or a JSON target.
5. The CLI is `cli.ts` only: it parses argv, calls the same SDK methods, prints
   JSON or a one-line confirmation, and maps an error to exit code `1` or `2`.

Native resolution lives in `native.ts`: the addon is required from
`@wae/open-cfd-<process.platform>-<process.arch>`; when it is missing every
operation throws `capability/not-implemented`. `OPEN_CFD_NATIVE` is a local
debug override for a `.node` file, not a supported deployment path.

## Determinism and restart

- There is no random source in the solver or the exporters. Streamline,
  contour and particle outputs use fixed seed grids, fixed palettes and fixed
  step caps. Reports are ordered by document order or side order
  (`left`, `right`, `top`, `bottom`).
- All field and history files are written atomically: temporary file, then
  rename. A failed export cannot leave a partial `solution.csv` or report.
- The SOR pressure solve uses a deterministic sweep order and a relaxation
  factor derived from the largest mesh side; there is no time-based or
  thread-count-dependent branch.
- A checkpoint is an `open-cfd-checkpoint` document holding the mesh size,
  cumulative `steps`, `time`, last `dt`, staggered `u`/`v` face values, cell
  pressure and the optional scalar field.
- `solve --restart` validates that the checkpoint mesh and scalar configuration
  match the case, restores the state, and continues step and sample numbering.
  Report `steps` and `time` are then cumulative since `t = 0`.
- A chain of checkpointed runs reproduces a single continuous run. The
  integration test compares both field-for-field.
- Checkpoints fail with `checkpoint/invalid` for malformed format, version,
  sizes or values, and `checkpoint/mismatch` when the case mesh or scalar
  configuration differs.

## Workspace layout

```text
Cargo.toml                  Rust workspace; members projects/crates/*
pnpm-workspace.yaml         pnpm workspace; packages projects/packages/*
wae.config.ts               native targets and TypeScript build outputs
nifty.config.ts             npm Trusted Publisher contract
projects/
  crates/
    cfd-model/              model, solver, exporters, session, integration tests
    cfd-napi/               Node-API cdylib
  packages/
    open-cfd/               @wae/open-cfd: SDK and open-cfd CLI
    open-cfd-<platform>-<arch>/  prebuilt addon packages (five targets)
    open-cfd-skills/        @wae/open-cfd-skills: agent skill documents
  sites/
    homepage/               project site
documentation/              these maintainer documents
```

The Rust workspace pins `science-mesh`, `boost-check` and `boost-session` to one
`boost.rs` revision and `wae` to one framework revision, so local and CI builds
resolve the same commits.
