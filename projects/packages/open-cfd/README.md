# @wae/open-cfd

The typed SDK and `open-cfd` command line for OpenCFD, a two-dimensional
incompressible Navier–Stokes engine. Every call edits, validates and solves a
single `.ocfd` case document through one native session, so the SDK, the CLI
and the Rust library return the same numbers.

## Install

```shell
pnpm add @wae/open-cfd
```

The native addon is resolved from the matching
`@wae/open-cfd-<platform>-<arch>` package and needs no build step. When the
addon is absent every operation rejects with `capability/not-implemented`
instead of returning a fabricated result.

## Quick start

```shell
pnpm exec open-cfd create cavity.ocfd
pnpm exec open-cfd mesh set cavity.ocfd --nx 24 --ny 24 --length-x 1 --length-y 1
pnpm exec open-cfd time set cavity.ocfd --dt 0.01 --steps 200 --pressure-iterations 100
pnpm exec open-cfd boundary set cavity.ocfd --side top --kind wall --u 1 --v 0
pnpm exec open-cfd solve cavity.ocfd --out report.json
pnpm exec open-cfd export cavity.ocfd --format vtk --out out/solution.vtk
```

`open-cfd capabilities` prints the support matrix and `open-cfd --help` lists
every command. Exit codes are `0` on success, `1` for a CLI, validation or I/O
error, and `2` for an unimplemented capability.

```ts
import { OpenCFD } from '@wae/open-cfd';

const cfd = new OpenCFD({ locale: 'en-US' });
const cfdCase = await cfd.create('cavity.ocfd');
cfdCase.setMesh({ nx: 24, ny: 24, lengthX: 1, lengthY: 1 });
cfdCase.setFluid({ density: 1, viscosity: 0.01 });
cfdCase.setTime({ dt: 0.01, steps: 200, pressureIterations: 100 });
cfdCase.setBoundary({ side: 'top', kind: 'wall', u: 1, v: 0 });
const report = cfdCase.solve({ tolerance: 1e-4, checkEvery: 10 });
cfdCase.export('csv', 'out/');
await cfd.dispose();
```

## Session and case API

`new OpenCFD()` opens nothing. `create(path)` writes the default cavity;
`open(path)` loads an existing document; both return a `CfdCase` that owns one
native handle. `dispose()` closes every case it owns.

| Task | `CfdCase` methods |
|------|-------------------|
| Mesh, fluid, references | `setMesh`, `setFluid`, `setReference` |
| Time integration | `setTime`, `setTimeScheme`, `setTimeAdaptive` |
| Boundaries | `setBoundary`, `listBoundaries`, `resetBoundary`, `setInletProfile` |
| Obstacles | `addObstacle`, `updateObstacle`, `listObstacles` |
| Porous regions | `addPorous`, `listPorous`, `removePorous` |
| Probes | `addProbe`, `updateProbe`, `removeProbe`, `listProbes`, `clearProbes` |
| Scalar, sources, inlets | `setScalar`, `addScalarSource`, `listScalarSources`, `clearScalarSources`, `setTransientInlet`, `listTransientInlets`, `clearTransientInlets` |
| Physics models | `setConvection`, `setPressureGradient`, `setTurbulence`, `setBuoyancy`, `setThermal`, `setWallFunction`, `clearWallFunction` |
| Checks | `validate`, `meshQuality`, `reynolds`, `stability` |
| Solve and deliver | `solve`, `export`, `exportProfile`, `exportStatistics`, `save` |

Every edit validates against the same rules as the Rust model and writes the
document atomically through a temporary file and rename; a failed edit leaves
the previous file intact. Errors are always `OpenCFDError` and `code` is the
contract (for example `mesh/invalid`, `obstacle/invalid`, `solver/unstable`).

`solve(options?)` accepts `steps`, `tolerance`, `checkEvery`, `restart` (a
checkpoint to resume) and `checkpoint` (a path for the final state). Resumed
reports keep cumulative `steps` and `time`.

## Export formats

`export(format, target, field?, layout?)` solves the case and writes one file.
A target ending in the format's extension is used as a file; otherwise it is
treated as a directory.

| Format | Output |
|--------|--------|
| `csv` | `solution.csv`, or `solution-nodes.csv` with `layout: 'node'` |
| `vtk` | Legacy ASCII `UNSTRUCTURED_GRID`, cell centres or nodes |
| `svg` | Field preview of `pressure`, `temperature`, `vorticity`, `streamFunction`, `speed` or `pressureCoefficient` |
| `contours` | Filled bands and marching-squares iso-lines |
| `streamlines` | Traced streamlines over the solid mask |
| `residuals` | `step,residual,continuity` history |
| `forces` | Per-sample obstacle forces and coefficients |
| `coefficients` | One drag/lift/moment summary row per obstacle |
| `probes` | Point-monitor history |
| `particles` | Lagrangian trajectories |
| `profile` | Line-profile CSV; use `exportProfile(target, { line, samples, field })` |
| `statistics` | Deterministic per-field statistics JSON |

## Constraints

- Two dimensions, single phase, incompressible, constant properties, on a
  uniform structured Cartesian mesh. Obstacles and porous regions are
  stair-stepped by cell centre.
- Explicit time stepping: a step that violates the CFL or diffusion limits
  fails with `solver/unstable` unless `setTimeAdaptive({ enabled: true })`
  clamps it.
- The wall function changes only the reported wall shear and friction;
  momentum keeps its no-slip ghost.
- `OPEN_CFD_NATIVE` points the loader at a locally built `.node` file. It is a
  debugging override, not a deployment path.

## Links

- [Product overview](../../README.md)
- [Capability matrix, numerical method and limitations](../../documentation/capabilities.md)
- [Command reference](../../documentation/cli.md)
- [Architecture](../../documentation/architecture.md)
