# OpenCFD

OpenCFD is a **narrowly scoped, verifiable** two-dimensional incompressible Navier-Stokes solver MVP. It targets:

- two-dimensional laminar cases such as the lid-driven cavity and Couette-like channels;
- teaching, algorithm validation and small regression models in CI;
- workflows that need plain-text JSON cases and reproducible command-line results.

**The Rust session API is the single core implementation**: `cfd_model::OpenCfd` owns document reading and writing, editing, validation, solving and export. The CLI (`ocfd`), the Node-API addon (`open-cfd-napi`) and the TypeScript SDK (`@wae/open-cfd`) are thin wrappers and do not duplicate any solver logic.

It is **not** a general-purpose commercial CFD package. The current build supports two-dimensional, single-phase, incompressible, constant-property laminar flow on a structured Cartesian mesh, rectangular and circular solid obstacles, isotropic Darcy–Forchheimer porous regions, passive scalar (temperature) transport, optional constant or mixing-length eddy viscosity, Boussinesq buoyancy coupling, thermal side and obstacle conditions with obstacle Nusselt numbers, selectable upwind, central or QUICK convection, selectable explicit Euler or RK2 time integration, uniform/parabolic/power-law inlet profiles, free-slip, pressure-outlet, convective-outlet and periodic sides, uniform imposed pressure gradients for pressure-driven flow, vorticity and stream-function post-processing, point monitor time series, wall shear and friction coefficients with an optional log-law wall function, drag, lift and moment coefficients with configurable reference quantities, steady-state velocity convergence detection, boundary mass-balance diagnostics, mesh quality metrics (non-orthogonality, aspect ratio and skewness), residual/force history CSV export and coefficient summary CSV export, SVG streamline export, filled-contour/iso-line SVG export, and CFL-limited adaptive time stepping with step statistics. It has no turbulence energy-equation models, compressible flow, three-dimensional flow, adaptive meshing or external format import. Unsupported capabilities return `capability/not-implemented` (exit code `2`) instead of degrading silently.

## TypeScript SDK

The SDK entry point is `new OpenCFD()`; a session holds several `CfdCase` documents, and every edit validates and atomically writes the file back.

```ts
import { OpenCFD } from '@wae/open-cfd';

const cfd = new OpenCFD({ locale: 'en-US' });

// The capability matrix comes from the Rust CAPABILITIES_JSON; method names are
// not treated as capability promises.
const capabilities = await cfd.capabilities();

const cfdCase = await cfd.create('demo.ocfd');
cfdCase.setMesh({ nx: 24, ny: 24, lengthX: 1, lengthY: 1 });
cfdCase.setFluid({ density: 1, viscosity: 0.01 });
cfdCase.setTime({ dt: 0.01, steps: 200, pressureIterations: 100, tolerance: 1e-3 });
cfdCase.setTimeScheme({ scheme: 'rk2' });
cfdCase.setBoundary({ side: 'top', kind: 'wall', u: 1, v: 0 });
cfdCase.setBoundary({ side: 'left', kind: 'inlet', u: 1, v: 0 });
cfdCase.setInletProfile({ side: 'left', profile: 'parabolic' });
cfdCase.setBoundary({ side: 'right', kind: 'outlet', pressure: 0 });
cfdCase.addObstacle({ kind: 'circle', id: 'cyl', x: 0.5, y: 0.5, r: 0.1 });
cfdCase.setReference({ velocity: 1, area: 0.2, length: 0.2 });
cfdCase.setConvection({ scheme: 'quick' });
cfdCase.setPressureGradient({ dpdx: -0.5, dpdy: 0 });
cfdCase.setScalar({ diffusivity: 1e-3, initial: 300, inlet: 320 });

cfdCase.validate();            // Throws OpenCFDError (stable code) on failure.
const report = cfdCase.solve({ steps: 500, tolerance: 1e-4, checkEvery: 10 });
const exported = cfdCase.export('csv', 'out/csv'); // Or 'vtk', 'svg' or 'streamlines'.
cfdCase.save();                // Also save('another.ocfd'); edits already write back.
await cfd.dispose();
```

- Session: `capabilities()`, `create(path)`, `open(path)`, `dispose()`.
- Document: `id`, `summary`, `obstacles`, `probes`, `setMesh`, `setFluid`, `setReference`, `setTime`, `setTimeScheme`, `setBoundary`, `setInletProfile`, `addObstacle`, `listObstacles`, `addProbe`, `listProbes`, `clearProbes`, `setScalar`, `setBuoyancy`, `setConvection`, `setPressureGradient`, `validate`, `solve(options?)`, `export`, `save`, `close`.
- `solve(options?)` accepts `steps`, `tolerance`, `checkEvery`, `restart` (checkpoint path to resume) and `checkpoint` (path that receives the final state). Resumed reports keep cumulative `steps`/`time`.
- `setBoundary` accepts `wall`, `inlet`, `outlet`, `convective`, `slip` and `periodic`; `pressure` sets the outlet gauge pressure. Periodic sides must be paired. `setInletProfile` shapes the normal velocity of one inlet side with `uniform`, `parabolic` or `powerLaw` (plus `exponent`).
- `setPressureGradient({dpdx, dpdy})` imposes a uniform pressure gradient in Pa/m as the momentum body force `-∇p/ρ`; setting both components to zero removes it. It drives pressure-driven channel flow, and combined with paired periodic sides it forms a fully developed periodic channel.
- `setTimeScheme` selects `euler` (default) or `rk2` for the explicit momentum step.
- `solve(options?)` overrides document steps with `steps`; when `tolerance` is given it stops early on the L2 norm of the velocity change and reports `converged`, `steadyState`, `residual`, `residualHistory` (with per-sample `continuity`), `terminationReason`, `timeScheme`, `massBalance`, `forces` (with `cm` and resolved reference quantities), `probes`, `wallShear`, and the vorticity and stream-function extremes.
- Errors are always `OpenCFDError`; `code` is the contract (for example `mesh/invalid`, `obstacle/invalid`, `solver/unstable`). A missing native addon reports `capability/not-implemented`.
- `export(format, target, field?, layout?)` writes CSV, legacy ASCII VTK, an SVG preview, contour SVG, a streamline SVG, a residual history CSV, a force history CSV or a drag/lift/moment summary CSV; `field` selects the SVG field (`pressure`, `temperature`, `vorticity` or `streamFunction`) and is ignored by `streamlines`, `residuals`, `forces` and `coefficients`. `layout` selects cell-centred or node-sampled data for CSV and VTK (`cell` by default). A target ending in `.csv`, `.vtk` or `.svg` is treated as a file; otherwise it is treated as a directory.

Build the native addon and SDK:

```shell
pnpm install
pnpm run build          # wae build: native -> ts -> staging, produces dist/*.js and platform packages
pnpm run build:ts       # tsc only, produces @wae/open-cfd/dist
```

The native addon is provided by the `@wae/open-cfd-<platform>-<arch>` platform packages: `wae native build` stages the `open-cfd-napi` cdylib as `open-cfd.<platform>-<arch>.node`, and the SDK resolves it from `process.platform` and `process.arch`. Without the matching platform package the SDK reports `capability/not-implemented`; only when debugging a locally built binary do you need to point `OPEN_CFD_NATIVE` at a `.node` file.

## CLI quick start

`ocfd` is a thin wrapper: it parses arguments, calls `OpenCfd`, prints results and returns an exit code; editing, validation, solving and file writes all go through the session API.

Run from the repository root (dependencies are cached, so this works fully offline):

```shell
cargo run -p cfd-model --bin ocfd --offline -- create demo.ocfd

cargo run -p cfd-model --bin ocfd --offline -- mesh set demo.ocfd --nx 24 --ny 24 --length-x 1 --length-y 1
cargo run -p cfd-model --bin ocfd --offline -- fluid set demo.ocfd --rho 1 --mu 0.01
cargo run -p cfd-model --bin ocfd --offline -- time set demo.ocfd --dt 0.01 --steps 200 --pressure-iterations 100 --tolerance 1e-3

cargo run -p cfd-model --bin ocfd --offline -- boundary set demo.ocfd --side left --kind wall --u 0 --v 0
cargo run -p cfd-model --bin ocfd --offline -- boundary set demo.ocfd --side right --kind wall --u 0 --v 0
cargo run -p cfd-model --bin ocfd --offline -- boundary set demo.ocfd --side bottom --kind wall --u 0 --v 0
cargo run -p cfd-model --bin ocfd --offline -- boundary set demo.ocfd --side top --kind wall --u 1 --v 0

cargo run -p cfd-model --bin ocfd --offline -- obstacle add demo.ocfd --kind circle --id cyl --x 0.6 --y 0.5 --r 0.1
cargo run -p cfd-model --bin ocfd --offline -- obstacle list demo.ocfd
cargo run -p cfd-model --bin ocfd --offline -- probe add demo.ocfd --id wake --x 0.8 --y 0.5
cargo run -p cfd-model --bin ocfd --offline -- probe list demo.ocfd
cargo run -p cfd-model --bin ocfd --offline -- scalar set demo.ocfd --diffusivity 1e-3 --initial 300 --inlet 320
cargo run -p cfd-model --bin ocfd --offline -- reference set demo.ocfd --velocity 1 --area 0.2 --length 0.2
cargo run -p cfd-model --bin ocfd --offline -- boundary set demo.ocfd --side right --kind outlet --pressure 0

cargo run -p cfd-model --bin ocfd --offline -- validate demo.ocfd
cargo run -p cfd-model --bin ocfd --offline -- solve demo.ocfd --steps 500 --tolerance 1e-4 --check-every 10 --out report.json
cargo run -p cfd-model --bin ocfd --offline -- export demo.ocfd --format csv --out out/csv
cargo run -p cfd-model --bin ocfd --offline -- export demo.ocfd --format vtk --out out/result.vtk
cargo run -p cfd-model --bin ocfd --offline -- export demo.ocfd --format streamlines --out out/streamlines.svg
```

`create` writes a default 32×32 cavity with 400 steps and a 1 m/s lid; `top` is already a moving wall. The `boundary set` commands above write all four sides explicitly for comparison. Delete `demo.ocfd` and rerun to create it again.

A measured report summary from this repository (24×24, dt=0.01, 200 steps, Re=100, tolerance=1e-3):

```json
{
  "steps": 200,
  "dt": 0.01,
  "maxVelocity": 0.8352,
  "minPressure": -0.1866,
  "maxPressure": 0.4767,
  "divergenceL2": 3.48e-4,
  "vorticityMin": -10.1342,
  "vorticityMax": 4.2168,
  "streamFunctionMin": -0.05959,
  "streamFunctionMax": 0.02278,
  "convection": "upwind",
  "converged": true,
  "pressureIterations": 37
}
```

## CLI commands

| Command | Description |
|---------|-------------|
| `ocfd create <file.ocfd>` | Create the default cavity case; fails when the file already exists |
| `ocfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>` | Set a uniform structured mesh; `nx` and `ny` are at least 3 |
| `ocfd fluid set <file> --rho <kg/m3> --mu <Pa.s>` | Set density and dynamic viscosity; `mu=0` means inviscid |
| `ocfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]` | Set the time step, step count, maximum pressure SOR iterations and relative residual tolerance |
| `ocfd time scheme set <file> --scheme euler\|rk2` | Select explicit Euler (default) or second-order Runge-Kutta (Heun) for the momentum step |
| `ocfd time adaptive set <file> --cfl <n\|off>` | Enable adaptive stepping with a target convective CFL in `(0, 1]`, or disable it with `--cfl off` |
| `ocfd boundary set <file> --side left\|right\|top\|bottom --kind wall\|inlet\|outlet\|convective\|slip\|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]` | Set a side's boundary type, velocity and outlet gauge pressure; omitted values are unchanged |
| `ocfd boundary profile set <file> --side left\|right\|top\|bottom --profile uniform\|parabolic\|powerLaw [--exponent <n>]` | Shape the normal velocity of an inlet side; `parabolic` is Poiseuille, `powerLaw` uses `(1−\|2s−1\|)^(1/n)` |
| `ocfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--id <name>]` | Add a rectangular obstacle; omitting `--id` generates `obstacle-N` in order |
| `ocfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]` | Add a circular obstacle |
| `ocfd obstacle list <file>` | Print the obstacle JSON array in document order |
| `ocfd porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --permeability <m2> [--inertial <n>] [--id <name>]` | Add an isotropic Darcy–Forchheimer porous region; omitting `--id` generates `porous-N` in order |
| `ocfd porous list <file>` | Print the porous-region JSON array in document order |
| `ocfd probe add <file> --x <m> --y <m> [--id <name>]` | Add a point monitor sampled during the solve; omitting `--id` generates `probe-N` in order |
| `ocfd probe list <file>` | Print the probe JSON array in document order |
| `ocfd probe clear <file>` | Remove every point monitor |
| `ocfd buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>` | Enable Boussinesq buoyancy (requires scalar transport); `--beta 0` removes the section |
| `ocfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]` | Enable or update passive scalar (temperature) transport; omitted values are unchanged |
| `ocfd convection set <file> --scheme upwind\|central\|quick` | Select the convective discretisation used by momentum and scalar transport |
| `ocfd pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>` | Impose a uniform pressure gradient as a momentum body force; `0 0` clears it |
| `ocfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]` | Override the force and moment reference quantities; omitted values stay automatic |
| `ocfd turbulence set <file> --model none\|constant\|mixingLength [--nu-t <m2/s>] [--mixing-length <m>]` | Select the eddy-viscosity model; `none` removes the turbulence section |
| `ocfd heat set <file> [--conductivity <W/mK>] [--specific-heat <J/kgK>] [--diffusivity <m2/s>] [--heat-source <W/m3>] [--left-temperature <K>] ...` | Configure energy-equation boundaries, volumetric source and reference temperature; requires scalar transport |
| `ocfd wall function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]` | Evaluate the reported wall shear with the log law; defaults are `kappa 0.41`, `intercept 5.2` and `minYPlus 11.06` |
| `ocfd wall function clear <file>` | Remove the wall-function section |
| `ocfd validate <file>` | Structural validation; on failure prints a JSON error and returns exit code `1` |
| `ocfd quality <file>` | Measure the mesh quality (`cells`, `fluidCells`, aspect-ratio extremes, non-orthogonality and skewness) and print the JSON report without solving |
| `ocfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]` | Solve and print an `open-cfd-report` JSON on stdout and a convergence/mass-balance summary on stderr; `--tolerance` enables steady-state early stopping, `--check-every` controls the sampling interval, `--restart` resumes from a checkpoint and `--checkpoint` writes the final state |
| `ocfd export <file> --format csv [--layout cell\|node] --out <dir\|file.csv>` | After solving, write `x,y,z,pressure,velocity_x,velocity_y,velocity_z,magnitude,temperature,vorticity,streamFunction` rows at cell centres or mesh nodes |
| `ocfd export <file> --format vtk [--layout cell\|node] --out <dir\|file.vtk>` | After solving, write legacy ASCII `UNSTRUCTURED_GRID` VTK with `pressure`, `velocity`, `magnitude`, `temperature`, `vorticity` and `streamFunction` point data over cell centres or mesh nodes |
| `ocfd export <file> --format svg [--field pressure\|temperature\|vorticity\|streamFunction\|speed] --out <dir\|file.svg>` | After solving, write a self-contained SVG preview of one cell-centred field |
| `ocfd export <file> --format contours [--field pressure\|temperature\|vorticity\|streamFunction\|speed] --out <dir\|file.svg>` | After solving, write a filled-contour and iso-line SVG with a discrete colour legend |
| `ocfd export <file> --format streamlines --out <dir\|file.svg>` | After solving, trace streamlines and write a self-contained SVG |
| `ocfd export <file> --format residuals --out <dir\|file.csv>` | After solving, write the sampled `step,residual,continuity` history |
| `ocfd export <file> --format forces --out <dir\|file.csv>` | After solving, write every sampled obstacle force and coefficient |
| `ocfd export <file> --format coefficients --out <dir\|file.csv>` | After solving, write one final drag/lift/moment summary row per obstacle |
| `ocfd capabilities` | Print the machine-readable capability matrix |
| `ocfd --help` | Print usage |

Exit codes: `0` success, `1` input, validation or IO error, `2` capability not implemented.

An `export` `--out` ending in `.csv`, `.vtk` or `.svg` is treated as a file path; otherwise it is treated as a directory and `solution.csv`, `solution-nodes.csv`, `solution.vtk`, `solution-nodes.vtk`, `solution-<field>.svg`, `solution-<field>-contours.svg`, `solution-streamlines.svg`, `residuals.csv`, `forces.csv` or `coefficients.csv` is written inside it. CSV, VTK, all SVG outputs and both history CSVs are all written atomically through a temporary file plus rename.

`--layout node` samples the `(nx + 1) × (ny + 1)` mesh vertices instead of the `nx × ny` cell centres: every vertex value is the mean of its adjacent fluid cells, solid cells contribute nothing, and the VTK grid covers the full rectangle with `nx × ny` quads. CSV and VTK reports include the `layout` that was written.

### Restart checkpoints

`ocfd solve --checkpoint state.json` writes an `open-cfd-checkpoint` JSON document containing the mesh size, cumulative `steps`, `time`, last `dt`, the staggered `u`/`v` faces, cell pressure and the optional scalar field. `--restart state.json` validates that the mesh and scalar configuration match the case, restores the state and continues the step and sample numbering; the report's `steps` and `time` are then cumulative totals since `t = 0`. A chain of checkpointed runs reproduces a single continuous run. Checkpoints are rejected with `checkpoint/invalid` when the format, version, array sizes or values are malformed, and with `checkpoint/mismatch` when the case mesh or scalar configuration differs.

## The `.ocfd` document format

The file is UTF-8 JSON with camelCase fields. A minimal document only needs `format` and `version`; every other field has a default:

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
  "probes": [
    { "id": "wake", "x": 0.8, "y": 0.5 }
  ],
  "scalar": { "diffusivity": 1e-3, "initial": 300.0, "inletValue": 320.0 }
}
```

| Field | Description |
|-------|-------------|
| `format` / `version` | Fixed to `open-cfd-case` / `1`; a mismatch fails validation |
| `units` | Only `"m"` is supported. Length m, density kg/m³, dynamic viscosity Pa·s, velocity m/s, time s, pressure Pa |
| `mesh` | Uniform Cartesian mesh; `nx` and `ny` are cell counts, `lengthX` and `lengthY` are physical sizes |
| `fluid` | Constant density `density` and constant dynamic viscosity `viscosity` |
| `reference` | Optional; `velocity` (m/s), `area` (m²) and `length` (m) override the force and moment reference quantities independently. Defaults are the maximum inlet speed, the obstacle characteristic length times unit depth, and the obstacle characteristic length |
| `convection` | Optional; `upwind` (default), `central` or `quick`, applied to the face-normal advective derivative of momentum and scalar transport |
| `pressureGradient` | Optional `[dp/dx, dp/dy]` in Pa/m; enters the momentum equation as the body force `-∇p/ρ` and drives pressure-driven flow |
| `time` | `dt`, step count `steps`, maximum pressure SOR sweeps per step `pressureIterations`, relative residual tolerance `tolerance`, the momentum time scheme `scheme` (`euler` default or `rk2`), adaptive stepping `adaptive` and its target convective CFL `targetCfl` in `(0, 1]` |
| `boundaries` | One `kind`, velocity `[u, v]` and outlet `pressure` (Pa) for each of the four sides, plus `inletProfiles` with a `profile` per side (`uniform`, `parabolic`, `powerLaw`) and the shared power-law `exponent`. `kind` is `wall`, `inlet`, `outlet`, `convective`, `slip` or `periodic`; periodic sides must come in opposite pairs |
| `initial` | Optional; uniform initial velocity `[u, v]`, at rest by default |
| `obstacles` | Optional; rectangle `{kind:"rect",x0,y0,x1,y1}` or circle `{kind:"circle",x,y,r}`; `id` may be omitted and becomes `obstacle-N` in order |
| `porous` | Optional; isotropic regions `{x0,y0,x1,y1,permeability,inertial}` with permeability `K` (m², positive) and Forchheimer coefficient `C_F` (default `0`); `id` may be omitted and becomes `porous-N` in order |
| `probes` | Optional; point monitors `{id,x,y}` sampled during the solve; a probe must lie inside the domain and outside every obstacle, and `id` may be omitted and becomes `probe-N` in order |
| `scalar` | Optional; `diffusivity` (m²/s), `initial` and `inletValue`, enabling passive scalar transport |
| `turbulence` | Optional; `model` (`none`, `constant`, `mixingLength`), `nuT` (m²/s) and `mixingLength` (m) |
| `thermal` | Optional; `specificHeat`, `conductivity`, `heatSource`, per-side `temperature`/`heatFlux`, `obstacleTemperature`, `obstacleHeatFlux` and `referenceTemperature`; requires `scalar` |
| `buoyancy` | Optional; `beta` (1/K), `tRef` (K) and `gravity` (`[gx, gy]`, m/s²); couples `−β(T − tRef)g` into momentum and requires `scalar` |
| `wallFunction` | Optional; `kappa` (default `0.41`), `intercept` (default `5.2`) and `minYPlus` (default `11.06`); evaluates the reported wall shear from the log law above the viscous-sublayer crossover |

Boundary semantics:

- `wall`: the normal velocity comes from the matching component of `velocity`; the tangential velocity uses no-slip or a moving wall. For example `top` with `velocity: [1, 0]` is the driven lid.
- `inlet`: `velocity` `[u, v]` is the fixed inflow velocity. The normal component is scaled by the side's `inletProfiles` entry: `uniform` (default), `parabolic` (`4s(1−s)`, zero at both ends) or `powerLaw` (`(1−|2s−1|)^(1/n)`), so a channel inlet can prescribe a developed profile while the outlet stays zero-gradient.
- `outlet`: zero-gradient velocity and a Dirichlet reference pressure set by `pressure` (default `0`). A fully closed cavity (all four sides `wall` or `slip`) is allowed, so an outlet is not required.
- `convective`: the outlet face relaxes as `φ_b = (φ_bⁿ + β·φ_i)/(1 + β)` with `β = max(uₙⁿ, 0)·dt/h`, where `φ_i` is the new adjacent interior face and `uₙⁿ` the previous interior normal velocity. It removes the reflective zero-gradient artefact for vortices crossing the outlet while remaining bounded; at rest or under inflow it reduces to the zero-gradient value. It prescribes the same Dirichlet `pressure` as an `outlet`.
- `slip`: zero normal velocity and zero tangential gradient, a free-slip wall. Slip sides are excluded from the `wallShear` report because they exert no tangential stress.
- `periodic`: the side and its opposite copy each other, so flow leaving one side re-enters the other. Periodic sides must come in opposite pairs (`left`/`right` and/or `top`/`bottom`), they carry no net boundary mass flux, and the momentum and pressure stencils wrap across them.

`solve` outputs an `open-cfd-report`:

| Field | Meaning |
|-------|---------|
| `steps` / `dt` / `time` | Steps actually advanced, time step of the last advanced step, and total physical time (the sum of all advanced steps) |
| `adaptive` / `averageDt` / `minDt` / `maxDt` | Whether adaptive stepping was enabled and the mean, smallest and largest step actually used; all four equal `dt` when adaptivity is off |
| `maxVelocity` | Maximum absolute velocity over all faces |
| `minPressure` / `maxPressure` | Pressure extremes at fluid cell centres |
| `vorticityMin` / `vorticityMax` | Vorticity `∂v/∂x − ∂u/∂y` extremes over fluid cells (1/s) |
| `streamFunctionMin` / `streamFunctionMax` | Stream-function extremes over fluid cells (m²/s), with zero mean |
| `divergenceL2` | RMS of the discrete divergence of the final velocity field |
| `meshQuality` | `{cells, fluidCells, dx, dy, minAspectRatio, meanAspectRatio, maxAspectRatio, meanNonOrthogonality, maxNonOrthogonality, meanSkewness, maxSkewness}` measured on the solved mesh; angles are in degrees and skewness is a fraction of the cell-centre separation |
| `converged` | With `tolerance`, whether steady state was reached; otherwise whether the final pressure solve met its relative tolerance |
| `steadyState` | Whether the L2 norm of the velocity change fell below `tolerance` |
| `residual` / `residualHistory` | Final and `checkEvery`-sampled L2 norms of the velocity change; each sample also carries `continuity`, the RMS discrete divergence after that step |
| `terminationReason` | `maxSteps` when every requested step was advanced, `steadyState` when the velocity residual met the tolerance first |
| `timeScheme` | Momentum time scheme actually used: `euler` or `rk2` |
| `buoyancy` | Whether Boussinesq buoyancy coupled temperature to momentum in this run |
| `massBalance` | Boundary mass flow `{inflow, outflow, net, relativeError}` in kg/s; `net = outflow − inflow` and `relativeError = |net| / max(inflow, outflow)` |
| `pressureConverged` | Whether the final pressure solve met its relative tolerance |
| `pressureIterations` | SOR sweeps actually performed in the final step |
| `pressureResidual` / `pressureResidualInitial` | Final and initial pressure-Poisson residuals of the final step |
| `forces` | Per-obstacle `{id, kind, fx, fy, mz, cd, cl, cm, referenceVelocity, referenceArea, referenceLength, history}`; the moment pivot is the obstacle centroid and positive `mz`/`cm` is counter-clockwise. Defaults are `D` for area and length and the maximum inlet speed for `U` (1 when there is no inlet); a `reference` section overrides them |
| `probes` | Per-probe `{id, x, y, cell, history}`; `cell` is the nearest pressure cell `[i, j]` and each sample is `{step, time, u, v, speed, pressure, temperature}` with `temperature: null` when no scalar is configured |
| `wallShear` | Per wall side `{side, meanShear, maxShear, meanYPlus, frictionCoefficient}`; `meanShear` is the length-averaged tangential stress the wall exerts on the fluid (Pa, positive along the side's positive tangent), `meanYPlus` the length-averaged friction `y⁺` of the adjacent cells and `frictionCoefficient` is `meanShear / (0.5 ρ U²)` |
| `porous` | Per-region `{id, cells, meanSpeed, meanDrag}`: owned fluid cells, mean cell speed (m/s) and mean Darcy–Forchheimer acceleration magnitude `(ν/K)|U| + (C_F/√K)|U|²` (m/s²) |
| `wallFunction` | Whether a log-law wall function evaluated the reported wall shear in this run |
| `convection` | Convective discretisation actually used: `upwind`, `central` or `quick` |

`tolerance` is a **relative** threshold: the pressure solve counts as converged when its residual falls to `tolerance × max(initial residual, 1e-12)`. The solver option `tolerance` instead applies to the velocity change, measured as an L2 norm (the square root of the sum of squared face-velocity increments), and does not change the document pressure tolerance.

## Capability matrix

| Capability | Status |
|------------|--------|
| Two-dimensional lid-driven cavity (`cavity2d`) | ✅ Supported |
| Two-dimensional Couette and channel-like flow (`couette2d`) | ✅ Supported |
| Rectangular and circular solid obstacles with drag, lift and moment coefficients (`obstacles`, `momentCoefficients`) | ✅ Supported (configurable reference quantities) |
| Free-slip walls and prescribed outlet pressure (`slipBoundary`, `pressureOutlet`) | ✅ Supported |
| Convective outlet with a local outflow-speed relaxation (`convectiveOutlet`) | ✅ Supported |
| Isotropic Darcy–Forchheimer porous regions (`porousMedia`) | ✅ Supported |
| Log-law wall function for the reported wall shear (`wallFunction`) | ✅ Supported (wall shear report; momentum keeps its no-slip ghost) |
| Periodic sides in opposite pairs (`periodic`) | ✅ Supported |
| Uniform imposed pressure gradient / pressure-driven flow (`pressureDriven`) | ✅ Supported |
| Uniform, parabolic and power-law inlet velocity profiles (`inletProfiles`) | ✅ Supported |
| Boundary mass balance and sampled continuity residual (`massBalance`) | ✅ Supported |
| Mesh quality metrics: non-orthogonality, aspect ratio and skewness (`meshQuality`) | ✅ Supported |
| Passive scalar (temperature) transport (`scalar`) | ✅ Supported |
| Steady-state velocity convergence detection and early stopping (`convergence`) | ✅ Supported |
| Vorticity and stream-function fields with report extremes (`vorticity`, `streamFunction`) | ✅ Supported |
| Selectable upwind, central or QUICK convection (`convectionSchemes`) | ✅ Supported |
| Selectable explicit Euler or RK2 time integration (`timeSchemes`) | ✅ Supported |
| CFL-limited adaptive time step with step statistics (`adaptiveTimeStep`) | ✅ Supported |
| Point monitor time series (`probes`) | ✅ Supported |
| Wall shear stress and friction coefficients (`wallShear`) | ✅ Supported |
| Eddy-viscosity turbulence, constant or mixing length (`turbulence`) | ✅ Supported (no energy-equation closure) |
| Thermal side/obstacle conditions, heat sources, obstacle Nusselt numbers and Boussinesq buoyancy (`heat`, `buoyancy`) | ✅ Supported |
| `.ocfd` JSON read and write | ✅ Supported |
| CSV, legacy ASCII VTK, SVG field preview, SVG contours and SVG streamlines export (`contours`) | ✅ Supported (CSV/VTK at cell centres or nodes) |
| Residual/continuity and obstacle force history CSV export (`history`) | ✅ Supported |
| Final per-obstacle drag/lift/moment summary CSV export (`coefficients`) | ✅ Supported |
| Restart checkpoints read/write (`restart`) | ✅ Supported |
| Compressible flow (`compressible`) | ❌ Not implemented |
| Three-dimensional flow (`threeD`) | ❌ Not implemented |
| External format import and export such as Abaqus `.inp` and OpenFOAM | ❌ Not implemented |

The machine-readable version is printed by `ocfd capabilities`, and the single source of truth in the source tree is `CAPABILITIES_JSON`.

## Numerical method

1. **Staggered (MAC) mesh**: pressure lives at the centre of each of the `nx × ny` cells; `u` lives on the `(nx+1) × ny` x-faces and `v` on the `nx × (ny+1)` y-faces. A cell whose centre lies inside an obstacle is marked solid; every face between a solid cell and a fluid cell, or between two solid cells, carries zero velocity.
2. **Momentum prediction**: explicit convection plus central-difference diffusion and the configured time scheme produce the tentative velocities `u*` and `v*`. A configured `pressureGradient` adds the uniform body force `-∇p/ρ` to every open face. A configured `porous` region adds the Darcy–Forchheimer sink `−(ν/K)U − (C_F/√K)|U|U` to the open faces whose adjacent cell the region owns. `time.scheme` selects first-order explicit Euler (default) or second-order Runge-Kutta (Heun: two derivative evaluations per step), both followed by the same pressure projection. The `convection` field selects first-order upwind (default), second-order central or third-order QUICK for the face-normal advective derivative; the transverse velocity gradient stays central, and QUICK falls back to upwind on the first interior face next to a boundary. Tangential wall velocities are imposed through half-cell ghost values so boundary cells take part in the projection; slip walls use mirrored ghost values for a zero tangential gradient, and solid faces are not sampled as fluid. Inlet sides scale their normal velocity by the configured `inletProfiles` factor at each face. Periodic sides are mirrored to their opposite side, and the momentum, pressure-Poisson and velocity-correction stencils wrap across the periodic axis.
3. **Pressure Poisson**: pressure follows from `∇²p = (ρ/dt) ∇·u*`. Wall, inlet, slip and solid faces use homogeneous Neumann conditions and each outlet uses a Dirichlet value from its `pressure` field; solid cells are not unknowns (their pressure stays 0) and their divergence is forced to zero. SOR solves the system with a relaxation factor derived from the largest mesh side, and the iteration count and relative tolerance come from the document. In a fully closed domain only the fluid cells have their mean pressure removed after the sweeps.
4. **Velocity correction**: `u = u* − (dt/ρ) ∇p` updates open faces only, then normal boundary conditions and zero solid velocities are reapplied to obtain a divergence-free velocity field. A `convective` side is then relaxed towards its new interior value with the local outflow speed, so its last cell column carries a small compensated divergence that the next projection absorbs.
5. **Obstacle forces**: on every fluid-solid face, pressure `p × face length` and viscous shear `μ·u_fluid/(half-cell distance) × face length` are accumulated per obstacle `id`. The face force is also crossed with its position relative to the obstacle centroid to obtain `mz`; `Cd = 2Fx/(ρU²A)`, `Cl = 2Fy/(ρU²A)` and `Cm = 2Mz/(ρU²AL)` use the resolved reference speed, area and length.
6. **Passive scalar**: the same selectable convection scheme plus central diffusion is advanced explicitly at cell centres; solid faces carry zero flux, solid cells hold no scalar and inlet faces use `inletValue`. A violation returns `solver/unstable`.
7. **Convergence detection**: each step computes the L2 norm of the velocity change, samples it every `checkEvery` steps and checks it against `tolerance`, stopping early when it passes; each sample also stores the RMS divergence, and history is capped at 1000 entries.
8. **Stability guards**: before solving, the explicit convective CFL number and the momentum and scalar diffusion numbers are checked; a violation returns `solver/unstable` and never emits NaN or Inf. Every step also checks that all fields are finite. With adaptive stepping enabled the initial guard is skipped because every step is clamped instead.
9. **Eddy viscosity**: the optional turbulence section either applies a constant `nuT` or evaluates `nuT = l²·|S|` per cell from the resolved strain-rate tensor. Fluid faces use the mean of the adjacent cell values and solid cells carry zero.
10. **Thermal transport**: when a `thermal` section is present, the scalar equation becomes an energy equation. The transported diffusivity is `k/(ρ·cp)`, sides and obstacle surfaces use fixed temperature or fixed heat flux, and a uniform volumetric source is added. Every obstacle reports `meanHeatFlux`, `wallTemperature` and `Nu = q̅·D/(k·ΔT)`.
11. **Derived fields**: after the solve, cell-centred vorticity `ω = ∂v/∂x − ∂u/∂y` uses central differences of the averaged face velocities, with one-sided differences on the first and last row or column. The stream function satisfies `∂ψ/∂y = u` and `∂ψ/∂x = −v`, is integrated from the bottom-left corner and has its fluid-cell mean removed, so it is a gauge-fixed value rather than a unique potential in multiply connected domains.
12. **Probes and wall shear**: each probe is bound to the nearest pressure cell and records `u`, `v`, speed, pressure and temperature every `checkEvery` sampling step, using the same bounded history as the residuals and forces. Every wall side reports `μ·(u_t,wall − u_t,cell)/(half-cell distance)` averaged over its fluid-adjacent boundary cells, so a linear tangential profile is reproduced exactly; solid cells are skipped, slip walls exert no tangential stress, and inlet/outlet sides are not reported. A `wallFunction` section replaces that estimate above `minYPlus` by solving `u⁺ = (1/κ)ln(y⁺) + B` for the friction velocity and reporting `τ_w = ρu_τ²`, while cells below the crossover keep the viscous value; every side also reports `meanYPlus`.
13. **Boundary mass balance**: after the final projection, the outward mass flux `ρ·u·dy` or `ρ·v·dx` is integrated over all four sides. The report splits it into inflow and outflow magnitudes and reports the signed net and `|net| / max(inflow, outflow)` as a conservation check.
14. **Streamlines**: a self-contained SVG traces streamlines from a regular seed grid with a second-order midpoint rule in both directions, using bilinear cell-centred velocity. Integration stops at solids, domain sides, stagnation or 600 steps, and solid cells are drawn as a grey mask.
15. **Contours**: a self-contained SVG samples one field at pressure-cell centres, clips every interior quad into nine value bands by linear interpolation along its edges, fills each band with one palette colour and traces the eight band boundaries with marching squares. Ambiguous saddle cells are resolved by the average corner value, solid cells stay dark grey, and the legend carries the discrete bands and the physical range. `speed` is derived as `√(u² + v²)` at cell centres.
16. **Adaptive time step**: with `time.adaptive`, every step is recomputed as `min(time.dt, targetCfl·min(dx,dy)/max(U), 0.5/((ν+νt)·(1/dx²+1/dy²)), 0.5/(κ·(1/dx²+1/dy²)), 0.9/(ν/K + C_F·|U|/√K))`, where `U` includes boundary velocities, `κ` is the scalar or thermal diffusivity and the last term bounds the explicit porous drag. The report stores the mean, smallest and largest step and the summed physical time, so a run under a fixed step budget reaches a different final time than its non-adaptive counterpart.

Upwind and explicit Euler are first-order in space and time, so the default configuration has visible numerical diffusion. Selecting `central` or `quick` sharpens fronts and reduces diffusion, at the cost of possible dispersive overshoot and a tighter cell-Péclet stability limit.

## Validation and tests

```shell
cargo test -p cfd-model --offline
```

The integration tests in `projects/crates/cfd-model/tests/cfd_model.rs` cover:

1. **Still cavity**: four stationary walls and zero initial velocity; after 40 steps the velocity and divergence stay near zero, the boundary mass balance is exactly zero, and there is no NaN or Inf.
2. **Lid-driven cavity**: 24×24, 300 steps, Re=100; `u > 0` on the upper centreline, mean `u > 0` in the upper quarter, maximum speed at most 1.05, `divergenceL2 < 1e-2` and every report field present. The measured maximum speed is about 0.84 and `divergenceL2` about 1.4e-5.
3. **Pressure Poisson residual**: 300 SOR sweeps on a fixed divergence field leave a residual below 1% of the first residual, and 300 sweeps beat 50.
4. **Export formats**: the cell-centred CSV row count is `nx*ny+1` and the node CSV row count is `(nx+1)*(ny+1)+1`, both with the header `x,y,z,pressure,velocity_x,velocity_y,velocity_z,magnitude,temperature,vorticity,streamFunction`; the VTK contains `DATASET UNSTRUCTURED_GRID`, `POINTS`, `POINT_DATA`, `SCALARS pressure`, `SCALARS temperature`, `SCALARS magnitude`, `SCALARS vorticity`, `SCALARS streamFunction` and `VECTORS velocity`, with points at cell centres or mesh vertices and one quad per interior cell or per cell; an SVG preview names the selected field and its unit; the streamline SVG names itself and contains traced polylines.
5. **Document round trip**: `.ocfd` save and load are identical, camelCase fields are correct, default fields deserialize and a wrong `format` is rejected.
6. **Solid mask**: all four faces of a solid cell carry zero velocity, divergence stays zero and an obstacle force report is present.
7. **Flow past a cylinder**: a 48×24 channel at Re≈100 with a circular obstacle yields a finite positive `Cd`, a finite `Cl` and no NaN anywhere.
8. **Scalar diffusion**: a hot spot in still fluid decays monotonically in peak value and a uniform field stays uniform.
9. **Steady-state convergence**: a loose tolerance stops before `steps` and reports `converged: true`; a strict tolerance with few steps reports `converged: false`.
10. **Derived fields**: a linear shear `u = y` gives vorticity `−1` in every cell and stream-function increments `u·dy`; solid-body rotation `(−y, x)` gives vorticity `2`; the solved stream function has zero fluid-cell mean and the report extremes match the stored field.
11. **Vorticity SVG**: exporting the vorticity field produces a self-contained SVG naming `vorticity (1/s)` and its numerical range.
12. **Convection schemes**: central and QUICK differentiate a quadratic velocity profile exactly while upwind carries an `O(dx)` error; QUICK equals upwind on the first interior face where its stencil would leave the domain; a scalar step is at least twice as thick under upwind as under central or QUICK and the sharper fronts have at least 1.5× the maximum gradient; lid-cavity solutions differ between schemes and remain finite and bounded.
13. **Probes**: a centre probe resolves to cell `[8, 8]`, records one finite sample per sampled step ending at the reported step, and a probe outside the domain or inside an obstacle is rejected with `probe/invalid`.
14. **Wall shear**: a synthetic linear tangential profile gives exactly `±μ` on the top, bottom and left walls and `2μ` for the friction coefficient with `U = 1 m/s`; a moving-lid cavity reports positive shear on the top and bottom walls, and a still cavity reports zero shear everywhere.
15. **Slip walls and pressure outlets**: slip faces carry zero normal velocity but leave tangential faces untouched, an outlet with `pressure = 2.5 Pa` drives the whole pressure field to 2.5, and a slip channel reports no wall shear with a flat vertical profile.
16. **Moment and references**: every obstacle report exposes a finite `mz` and `cm`, and overriding the reference speed, area and length scales `cd` by `(U₀²A₀)/(U₁²A₁)` and `cm` by that factor times `L₀/L₁`.
17. **Mass balance and continuity**: a closed cavity reports exactly zero inflow, outflow and net mass flow; a channel reports positive inflow and outflow with a relative imbalance below 25%; every residual sample carries a finite non-negative continuity value and the last sample ends at the reported step.
18. **Streamlines**: exporting the lid-driven cavity writes `solution-streamlines.svg`, containing the title, the line/speed subtitle and at least one polyline.
19. **Inlet profiles**: a parabolic inlet reproduces `4s(1−s)` on every boundary face exactly, a power-law inlet with `n=1` reproduces the triangular profile, a uniform profile is constant, and the solved window mass balance equals the discrete parabolic mask integral; a zero exponent is rejected with `boundary/invalid`.
20. **History exports**: `residuals.csv` has the header `step,residual,continuity` with one bounded row per sample, `forces.csv` has one 9-column row per sampled obstacle force in document order, and `coefficients.csv` has one 11-column summary row per obstacle (`obstacle,kind,cd,cl,cm,fx,fy,mz,referenceVelocity,referenceArea,referenceLength`).
21. **Time schemes**: Euler and RK2 both stay finite on a lid-driven cavity at the same physical time and report their scheme; against a fine RK2 reference the RK2 coarse solution has less than half the Euler temporal error.
22. **Termination reason**: a loose tolerance stops with `terminationReason = steadyState`; a strict short run reports `maxSteps`.
23. **Contours**: exporting `speed` writes `solution-speed-contours.svg` with the field/unit title, filled-band polygon elements, marching-squares iso-lines and the legend text; a named `.svg` target is used verbatim and temperature contours without a scalar field fail with `export/no-field`.
24. **Adaptive time step**: a lid-driven cavity with `dt = 0.05 s`, target CFL 0.5 and `h = 0.05 m` reduces every step to about 0.025 s, reports `adaptive = true` and `time = averageDt × steps`, and rejects `targetCfl > 1` with `time/invalid`.
25. **Periodic sides**: a single periodic side without its opposite pair is rejected with `boundary/invalid`; `apply_velocity_boundaries` copies the opposite boundary face; a periodic channel keeps a streamwise initial flow, wraps both boundary faces exactly and reports zero inflow, outflow and net mass balance because periodic sides are excluded.
27. **Restart checkpoints**: a five-step solve saved to a checkpoint and resumed for five more steps matches a continuous ten-step solve field-for-field, reports cumulative `steps = 10`, and a checkpoint captured on a 12×8 mesh is rejected with `checkpoint/mismatch` on a 10×8 case; the CLI `solve --checkpoint` / `--restart` pair writes and chains checkpoints.
28. **Convective outlet**: `convective` parses and owns the Dirichlet pressure gauge, `apply_velocity_boundaries` gives the face the zero-gradient value before the relaxation, `apply_convective_outlets` reproduces `(φ_bⁿ + βφ_i)/(1 + β)` exactly with `β = uₙ·dt/dx` and falls back to zero gradient on an inflow basis, and a 24×12 channel keeps the interior divergence below `5e-3 1/s` with positive inflow and outflow.
29. **Porous regions**: a valid region round-trips through JSON, zero permeability, a region outside the domain and a negative `C_F` are rejected with `porous/invalid`, a full-domain region slows a pressure-driven channel below its free mean speed and reports `cells = nx·ny` with positive `meanDrag`, and a step too large for `(ν/K)·dt` is rejected with `solver/unstable`.
30. **Wall function**: a 10 m/s, 0.01 m, 1e-5 m²/s sample solves the log law to a velocity residual below `1e-8`, gives a shear more than five times the laminar estimate and reverses sign with the velocity; a low-`y⁺` sample stays on the viscous value and matches the plain report; `kappa ≤ 0` and a negative `minYPlus` are rejected with `wallFunction/invalid`.
31. **Wall-function integration**: a lid-driven cavity with `wallFunction` reports `wallFunction: true`, keeps every side on the viscous sublayer (`meanYPlus` below `11.06`) and reproduces the laminar shear of the same run without a wall function.
32. **Mesh quality**: a 10×5 uniform mesh with `dx = 0.1 m` and `dy = 0.2 m` reports an aspect ratio of `2`, zero non-orthogonality and zero skewness; a solid obstacle lowers `fluidCells` below `cells`; the solve report carries the same metrics, and the session and CLI `quality` command print the identical JSON.

`projects/crates/cfd-model/tests/session.rs` covers the full session API path: `create → mesh/fluid/time/boundary set → validate → solve → CSV/VTK/SVG/streamline/residual/force export`, comparing each item with the report and CSV produced by the `ocfd` CLI to confirm both entry points share one implementation. It also round-trips slip walls, outlet pressure, reference quantities, inlet profiles, the time scheme, adaptive time settings, porous regions and the wall function through both the session and CLI paths.

Node-API and TypeScript layer checks:

```shell
cargo check -p open-cfd-napi --offline
node --experimental-strip-types --check projects/packages/open-cfd/src/cli.ts
```

Formatting and building:

```shell
cargo fmt -p cfd-model
cargo build -p cfd-model --offline
```

## Limitations

- Two dimensions only; no three-dimensional mesh or axisymmetric support.
- Single-phase, incompressible, constant-density, constant-viscosity Newtonian flow only; the optional eddy viscosity is algebraic (constant or mixing length), so high-Reynolds-number results remain a laminar approximation.
- Uniform structured Cartesian meshes only; no body-fitted meshes, adaptive refinement, moving meshes or immersed boundaries. Obstacles are stair-stepped by cell centre. Because the mesh is uniform, the reported `meshQuality` metrics are constant over every fluid cell; they are measured rather than assumed, but they cannot diagnose a graded mesh that the solver does not support.
- Explicit time stepping is limited by CFL and diffusion numbers; finer meshes and higher viscosity require a smaller `dt`. Without adaptivity a violation raises an error; with `time.adaptive` every step is clamped to the target CFL and diffusion bound instead.
- Adaptive stepping changes the physical time reached for a fixed step count. It does not adapt the mesh, and it uses the velocity at the start of the step, so a strong transient can briefly exceed the target before the next step clamps again.
- The default first-order upwind plus explicit Euler makes space and time first-order, with significant numerical diffusion. `central` and `quick` are available, but above a cell Péclet number of about 2 they can oscillate and destabilise the explicit scheme. `rk2` reduces the temporal error (about 3.5× at a representative cavity step) but keeps the same pressure projection and CFL limits, so it does not permit a larger `dt`.
- Inlet profiles shape only the normal velocity of an `inlet` side; the tangential component stays uniform, and walls, slip sides and outlets ignore the profile.
- Porous regions are isotropic and fixed in time: `K` and `C_F` are scalar constants, corners are stair-stepped by cell centre like obstacles, and the Darcy sink is explicit, so `(ν/K + C_F|U|/√K)·dt` must stay at or below one.
- The wall function only changes the reported wall shear and friction; momentum keeps the no-slip ghost, so it is a reporting model rather than a fully wall-modelled boundary layer.
- The imposed pressure gradient is uniform in space and constant in time; it is not a body-force field or a time-varying forcing.
- Passive scalar (temperature) transport is supported and Boussinesq buoyancy couples it back to momentum through the `buoyancy` section; a buoyancy section requires `scalar` and validates `beta`, `tRef` and both gravity components as finite.
- The stream function is integrated along one path from the bottom-left corner, so in a domain cut by obstacles it is a gauge-fixed diagnostic rather than a unique potential.
- Point probes report the nearest cell centre without interpolation, and wall shear uses the molecular viscosity with a first-order half-cell gradient (or the optional log-law wall function).
- Boundary handling is first-order: tangential wall velocity uses a half-cell ghost, slip walls use mirrored ghost values, outlets use zero-gradient velocity with the prescribed Dirichlet pressure, convective outlets relax the face with the local outflow speed, and periodic sides wrap the momentum and pressure stencils across the opposite side.
- Obstacle moments use the geometric centroid as a fixed pivot and a first-order face pressure/shear distribution; the default reference area and length are the characteristic length times unit depth and the characteristic length.
- Streamlines and contours are visual diagnostics: streamlines use bilinear cell-centred velocity, a fixed step cap and a fixed seed grid, while contours interpolate linearly between cell centres with a fixed nine-band palette. They are not time-accurate particle paths or adaptive iso-level plots.
- No restart, parallel execution or external format (Abaqus, OpenFOAM and similar) import or export.
- No graphical interface; use ParaView to open exported VTK.

## License

The code and documentation in this repository are provided under `CC0-1.0` / CC0 Universal; see `License.md`.
