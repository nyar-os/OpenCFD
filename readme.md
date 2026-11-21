# OpenCFD

OpenCFD is a **narrowly scoped, verifiable** two-dimensional incompressible Navier-Stokes solver MVP. It targets:

- two-dimensional laminar cases such as the lid-driven cavity and Couette-like channels;
- teaching, algorithm validation and small regression models in CI;
- workflows that need plain-text JSON cases and reproducible command-line results.

**The Rust session API is the single core implementation**: `cfd_model::OpenCfd` owns document reading and writing, editing, validation, solving and export. The CLI (`ocfd`), the Node-API addon (`open-cfd-napi`) and the TypeScript SDK (`@wae/open-cfd`) are thin wrappers and do not duplicate any solver logic.

It is **not** a general-purpose commercial CFD package. The current build supports two-dimensional, single-phase, incompressible, constant-property laminar flow on a structured Cartesian mesh, rectangular and circular solid obstacles, passive scalar (temperature) transport, and steady-state velocity convergence detection. It has no turbulence, compressible flow, Boussinesq buoyancy, coupled heat transfer, three-dimensional flow, adaptive meshing or external format import. Unsupported capabilities return `capability/not-implemented` (exit code `2`) instead of degrading silently.

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
cfdCase.setBoundary({ side: 'top', kind: 'wall', u: 1, v: 0 });
cfdCase.addObstacle({ kind: 'circle', id: 'cyl', x: 0.5, y: 0.5, r: 0.1 });
cfdCase.setScalar({ diffusivity: 1e-3, initial: 300, inlet: 320 });

cfdCase.validate();            // Throws OpenCFDError (stable code) on failure.
const report = cfdCase.solve({ steps: 500, tolerance: 1e-4, checkEvery: 10 });
const exported = cfdCase.export('csv', 'out/csv'); // Or 'vtk'.
cfdCase.save();                // Also save('another.ocfd'); edits already write back.
await cfd.dispose();
```

- Session: `capabilities()`, `create(path)`, `open(path)`, `dispose()`.
- Document: `id`, `summary`, `obstacles`, `setMesh`, `setFluid`, `setTime`, `setBoundary`, `addObstacle`, `listObstacles`, `setScalar`, `validate`, `solve(options?)`, `export`, `save`, `close`.
- `solve(options?)` overrides document steps with `steps`; when `tolerance` is given it stops early on the L2 norm of the velocity change and reports `converged`, `steadyState`, `residual`, `residualHistory` and `forces`.
- Errors are always `OpenCFDError`; `code` is the contract (for example `mesh/invalid`, `obstacle/invalid`, `solver/unstable`). A missing native addon reports `capability/not-implemented`.
- An `export` target ending in `.csv` or `.vtk` is treated as a file; otherwise it is treated as a directory.

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
cargo run -p cfd-model --bin ocfd --offline -- scalar set demo.ocfd --diffusivity 1e-3 --initial 300 --inlet 320

cargo run -p cfd-model --bin ocfd --offline -- validate demo.ocfd
cargo run -p cfd-model --bin ocfd --offline -- solve demo.ocfd --steps 500 --tolerance 1e-4 --check-every 10 --out report.json
cargo run -p cfd-model --bin ocfd --offline -- export demo.ocfd --format csv --out out/csv
cargo run -p cfd-model --bin ocfd --offline -- export demo.ocfd --format vtk --out out/result.vtk
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
| `ocfd boundary set <file> --side left\|right\|top\|bottom --kind wall\|inlet\|outlet [--u <m/s>] [--v <m/s>]` | Set a side's boundary type and velocity; omitted velocity components are unchanged |
| `ocfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--id <name>]` | Add a rectangular obstacle; omitting `--id` generates `obstacle-N` in order |
| `ocfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]` | Add a circular obstacle |
| `ocfd obstacle list <file>` | Print the obstacle JSON array in document order |
| `ocfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]` | Enable or update passive scalar (temperature) transport; omitted values are unchanged |
| `ocfd validate <file>` | Structural validation; on failure prints a JSON error and returns exit code `1` |
| `ocfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--out report.json]` | Solve and print an `open-cfd-report` JSON; `--tolerance` enables steady-state early stopping and `--check-every` controls the sampling interval |
| `ocfd export <file> --format csv --out <dir\|file.csv>` | After solving, write cell-centred `x,y,pressure,u,v,magnitude,temperature` |
| `ocfd export <file> --format vtk --out <dir\|file.vtk>` | After solving, write legacy ASCII `STRUCTURED_POINTS` VTK (with `temperature` SCALARS) |
| `ocfd capabilities` | Print the machine-readable capability matrix |
| `ocfd --help` | Print usage |

Exit codes: `0` success, `1` input, validation or IO error, `2` capability not implemented.

An `export` `--out` ending in `.csv` or `.vtk` is treated as a file path; otherwise it is treated as a directory and `solution.csv` or `solution.vtk` is written inside it. CSV and VTK are both written atomically through a temporary file plus rename.

## The `.ocfd` document format

The file is UTF-8 JSON with camelCase fields. A minimal document only needs `format` and `version`; every other field has a default:

```json
{
  "format": "open-cfd-case",
  "version": 1,
  "units": "m",
  "mesh": { "nx": 24, "ny": 24, "lengthX": 1.0, "lengthY": 1.0 },
  "fluid": { "density": 1.0, "viscosity": 0.01 },
  "time": {
    "dt": 0.01,
    "steps": 200,
    "pressureIterations": 100,
    "tolerance": 1e-3
  },
  "boundaries": {
    "left":   { "kind": "wall", "velocity": [0.0, 0.0] },
    "right":  { "kind": "wall", "velocity": [0.0, 0.0] },
    "top":    { "kind": "wall", "velocity": [1.0, 0.0] },
    "bottom": { "kind": "wall", "velocity": [0.0, 0.0] }
  },
  "initial": { "velocity": [0.0, 0.0] },
  "obstacles": [
    { "kind": "circle", "id": "cyl", "x": 0.5, "y": 0.5, "r": 0.1 },
    { "kind": "rect", "id": "step", "x0": 0.0, "y0": 0.0, "x1": 0.2, "y1": 0.1 }
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
| `time` | `dt`, step count `steps`, maximum pressure SOR sweeps per step `pressureIterations`, and relative residual tolerance `tolerance` |
| `boundaries` | One `kind` and velocity `[u, v]` for each of the four sides |
| `initial` | Optional; uniform initial velocity `[u, v]`, at rest by default |
| `obstacles` | Optional; rectangle `{kind:"rect",x0,y0,x1,y1}` or circle `{kind:"circle",x,y,r}`; `id` may be omitted and becomes `obstacle-N` in order |
| `scalar` | Optional; `diffusivity` (m²/s), `initial` and `inletValue`, enabling passive scalar transport |
| `buoyancy` | Declared only for document compatibility; this build does not implement Boussinesq and validation returns `capability/not-implemented` |

Boundary semantics:

- `wall`: the normal velocity comes from the matching component of `velocity`; the tangential velocity uses no-slip or a moving wall. For example `top` with `velocity: [1, 0]` is the driven lid.
- `inlet`: `velocity` `[u, v]` is the fixed inflow velocity.
- `outlet`: zero-gradient velocity and a Dirichlet reference pressure of 0. A fully closed cavity (all four sides `wall`) is allowed, so an outlet is not required.

`solve` outputs an `open-cfd-report`:

| Field | Meaning |
|-------|---------|
| `steps` / `dt` / `time` | Steps actually advanced, time step, total physical time |
| `maxVelocity` | Maximum absolute velocity over all faces |
| `minPressure` / `maxPressure` | Pressure extremes at fluid cell centres |
| `divergenceL2` | RMS of the discrete divergence of the final velocity field |
| `converged` | With `tolerance`, whether steady state was reached; otherwise whether the final pressure solve met its relative tolerance |
| `steadyState` | Whether the L2 norm of the velocity change fell below `tolerance` |
| `residual` / `residualHistory` | Final and `checkEvery`-sampled L2 norms of the velocity change |
| `pressureConverged` | Whether the final pressure solve met its relative tolerance |
| `pressureIterations` | SOR sweeps actually performed in the final step |
| `pressureResidual` / `pressureResidualInitial` | Final and initial pressure-Poisson residuals of the final step |
| `forces` | Per-obstacle `{id, kind, fx, fy, cd, cl, history}`; `D` is the rectangle height or circle diameter and `U` is the maximum inlet speed (1 when there is no inlet) |

`tolerance` is a **relative** threshold: the pressure solve counts as converged when its residual falls to `tolerance × max(initial residual, 1e-12)`. The solver option `tolerance` instead applies to the velocity change, measured as an L2 norm (the square root of the sum of squared face-velocity increments), and does not change the document pressure tolerance.

## Capability matrix

| Capability | Status |
|------------|--------|
| Two-dimensional lid-driven cavity (`cavity2d`) | ✅ Supported |
| Two-dimensional Couette and channel-like flow (`couette2d`) | ✅ Supported |
| Rectangular and circular solid obstacles with `Cd` and `Cl` force coefficients (`obstacles`) | ✅ Supported |
| Passive scalar (temperature) transport (`scalar`) | ✅ Supported |
| Steady-state velocity convergence detection and early stopping (`convergence`) | ✅ Supported |
| Boussinesq buoyancy (`buoyancy`) | ❌ Not implemented (avoids an unvalidated unstable path) |
| `.ocfd` JSON read and write | ✅ Supported |
| CSV and legacy ASCII VTK export | ✅ Supported |
| Compressible flow (`compressible`) | ❌ Not implemented |
| Turbulence models (`turbulence`) | ❌ Not implemented |
| Coupled heat transfer (`heat`) | ❌ Not implemented |
| Three-dimensional flow (`threeD`) | ❌ Not implemented |
| External format import and export such as Abaqus `.inp` and OpenFOAM | ❌ Not implemented |

The machine-readable version is printed by `ocfd capabilities`, and the single source of truth in the source tree is `CAPABILITIES_JSON`.

## Numerical method

1. **Staggered (MAC) mesh**: pressure lives at the centre of each of the `nx × ny` cells; `u` lives on the `(nx+1) × ny` x-faces and `v` on the `nx × (ny+1)` y-faces. A cell whose centre lies inside an obstacle is marked solid; every face between a solid cell and a fluid cell, or between two solid cells, carries zero velocity.
2. **Momentum prediction**: first-order upwind convection, central-difference diffusion and explicit Euler produce the tentative velocities `u*` and `v*`. Tangential wall velocities are imposed through half-cell ghost values so boundary cells take part in the projection; solid faces are not sampled as fluid.
3. **Pressure Poisson**: pressure follows from `∇²p = (ρ/dt) ∇·u*`. Wall, inlet and solid faces use homogeneous Neumann conditions and outlets use homogeneous Dirichlet; solid cells are not unknowns (their pressure stays 0) and their divergence is forced to zero. SOR solves the system with a relaxation factor derived from the largest mesh side, and the iteration count and relative tolerance come from the document. In a fully closed domain only the fluid cells have their mean pressure removed after the sweeps.
4. **Velocity correction**: `u = u* − (dt/ρ) ∇p` updates open faces only, then normal boundary conditions and zero solid velocities are reapplied to obtain a divergence-free velocity field.
5. **Obstacle forces**: on every fluid-solid face, pressure `p × face length` and viscous shear `μ·u_fluid/(half-cell distance) × face length` are accumulated per obstacle `id`; `Cd = 2Fx/(ρU²D)` and `Cl = 2Fy/(ρU²D)`.
6. **Passive scalar**: the same upwind convection plus central diffusion is advanced explicitly at cell centres; solid faces carry zero flux, solid cells hold no scalar and inlet faces use `inletValue`. A violation returns `solver/unstable`.
7. **Convergence detection**: each step computes the L2 norm of the velocity change, samples it every `checkEvery` steps and checks it against `tolerance`, stopping early when it passes; history is capped at 1000 entries.
8. **Stability guards**: before solving, the explicit convective CFL number and the momentum and scalar diffusion numbers are checked; a violation returns `solver/unstable` and never emits NaN or Inf. Every step also checks that all fields are finite.

This is a first-order in time and space MVP with visible numerical diffusion. It suits validation and teaching, not high-accuracy engineering results.

## Validation and tests

```shell
cargo test -p cfd-model --offline
```

The integration tests in `projects/crates/cfd-model/tests/cfd_model.rs` cover:

1. **Still cavity**: four stationary walls and zero initial velocity; after 40 steps the velocity and divergence stay near zero with no NaN or Inf.
2. **Lid-driven cavity**: 24×24, 300 steps, Re=100; `u > 0` on the upper centreline, mean `u > 0` in the upper quarter, maximum speed at most 1.05, `divergenceL2 < 1e-2` and every report field present. The measured maximum speed is about 0.84 and `divergenceL2` about 1.4e-5.
3. **Pressure Poisson residual**: 300 SOR sweeps on a fixed divergence field leave a residual below 1% of the first residual, and 300 sweeps beat 50.
4. **Export formats**: the CSV row count is `nx*ny+1` with the header `x,y,pressure,u,v,magnitude,temperature`; the VTK contains `STRUCTURED_POINTS`, `DIMENSIONS`, `POINT_DATA`, `SCALARS pressure`, `SCALARS temperature` and `VECTORS velocity`.
5. **Document round trip**: `.ocfd` save and load are identical, camelCase fields are correct, default fields deserialize and a wrong `format` is rejected.
6. **Solid mask**: all four faces of a solid cell carry zero velocity, divergence stays zero and an obstacle force report is present.
7. **Flow past a cylinder**: a 48×24 channel at Re≈100 with a circular obstacle yields a finite positive `Cd`, a finite `Cl` and no NaN anywhere.
8. **Scalar diffusion**: a hot spot in still fluid decays monotonically in peak value and a uniform field stays uniform.
9. **Steady-state convergence**: a loose tolerance stops before `steps` and reports `converged: true`; a strict tolerance with few steps reports `converged: false`.

`projects/crates/cfd-model/tests/session.rs` covers the full session API path: `create → mesh/fluid/time/boundary set → validate → solve → CSV/VTK export`, comparing each item with the report and CSV produced by the `ocfd` CLI to confirm both entry points share one implementation.

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
- Single-phase, incompressible, constant-density, constant-viscosity Newtonian flow only; there is no turbulence model at high Re, so results are a laminar approximation.
- Uniform structured Cartesian meshes only; no body-fitted meshes, adaptive refinement, moving meshes or immersed boundaries. Obstacles are stair-stepped by cell centre.
- Explicit time stepping is limited by CFL and diffusion numbers; finer meshes and higher viscosity require a smaller `dt`. Violations raise an error instead of reducing the step automatically.
- First-order upwind plus explicit Euler makes both space and time first-order, with significant numerical diffusion.
- Passive scalar (temperature) transport is supported, but Boussinesq buoyancy and coupled heat transfer are not; the `buoyancy` field is rejected with `capability/not-implemented`.
- Boundary handling is first-order: tangential wall velocity uses a half-cell ghost and outlets use zero-gradient velocity with pressure 0.
- No restart, parallel execution or external format (Abaqus, OpenFOAM and similar) import or export.
- No graphical interface; use ParaView to open exported VTK.

## License

The code and documentation in this repository are provided under `CC0-1.0` / CC0 Universal; see `License.md`.
