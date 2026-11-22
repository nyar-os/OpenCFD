# OpenCFD

Two-dimensional incompressible flow described in plain text and reproducible from the command line.

## 💡 What is OpenCFD?

OpenCFD is a small, deliberately narrow CFD engine for **two-dimensional, single-phase, incompressible flow** on a structured Cartesian mesh. You describe a case as one `.ocfd` JSON document, solve it, and read the result as JSON, CSV, VTK or SVG. The same document and the same commands give the same numbers on every machine, which makes it useful for teaching, validating a solver against published cavity and channel data, and regression checks in CI.

Typical uses:

- lid-driven cavity and Couette-like channel cases with numbers you can check by hand;
- drag, lift and moment coefficients for rectangular (optionally rotated) and circular obstacles;
- thermal and passive-scalar transport, with Boussinesq buoyancy and obstacle Nusselt numbers;
- small parametric studies that run from a script.

A case is one JSON document: mesh, fluid, time settings, four boundary sides, obstacles, porous regions, probes and optional scalar, thermal, buoyancy, turbulence and wall-function sections. Lengths are metres throughout; units are never converted.

**OpenCFD is not:**

- a three-dimensional, compressible or multiphase solver;
- a general commercial CFD package — there are no body-fitted or unstructured meshes, adaptive refinement, moving meshes, FSI or a graphical interface;
- a place where an unsupported feature silently degrades — an unimplemented capability returns a stable `capability/not-implemented` error instead of an approximate result.

## 🚀 Getting started

### 1. Point your agent at the repository

OpenCFD ships an agent skill package. Point your agent at the repository documentation and the `open-cfd` CLI, then give it a concrete task:

```text
Use OpenCFD (`open-cfd`) to run a 24×24 lid-driven cavity at Re=100.
Read documentation/cli.md for the commands and documentation/capabilities.md
before claiming a result, then solve it and export the pressure field.
```

The agent path works because every capability and error code is documented and the CLI prints machine-readable JSON.

### 2. Install and run

```shell
pnpm add @wae/open-cfd

pnpm exec open-cfd create cavity.ocfd
pnpm exec open-cfd mesh set cavity.ocfd --nx 24 --ny 24 --length-x 1 --length-y 1
pnpm exec open-cfd time set cavity.ocfd --dt 0.01 --steps 200 --pressure-iterations 100 --tolerance 1e-3
pnpm exec open-cfd boundary set cavity.ocfd --side top --kind wall --u 1 --v 0
pnpm exec open-cfd solve cavity.ocfd --tolerance 1e-4 --out report.json
pnpm exec open-cfd export cavity.ocfd --format csv --out out/
```

`solve` prints an `open-cfd-report` object; `--out` also keeps a copy. `pnpm exec open-cfd capabilities` lists what the installed build supports, and `pnpm exec open-cfd --help` lists every command. Exit code `1` means invalid input or an I/O error, `2` means the requested capability is not implemented.

The SDK is the same engine with a typed API. A session holds several cases, and every edit validates and atomically writes the document back:

```ts
import { OpenCFD } from '@wae/open-cfd';

const cfd = new OpenCFD();
const cfdCase = await cfd.open('cavity.ocfd');
cfdCase.setFluid({ density: 1, viscosity: 0.01 });
const report = cfdCase.solve({ steps: 500, tolerance: 1e-4, checkEvery: 10 });
console.log(report.maxVelocity, report.divergenceL2);
cfdCase.export('vtk', 'out/solution.vtk');
await cfd.dispose();
```

A measured 24×24, `dt = 0.01 s`, 200-step, Re=100 cavity reports a maximum velocity of 0.8352 m/s and `divergenceL2` of 3.48e-4.

## ✨ Highlights

- **Staggered MAC solver** with explicit Euler or second-order Runge–Kutta stepping, upwind/central/QUICK convection and CFL-limited adaptive time stepping.
- **Boundary conditions**: walls (including moving walls), inlets with uniform, parabolic or power-law profiles, pressure and convective outlets, free-slip walls and paired periodic sides.
- **Obstacles**: rectangular (optionally rotated) and circular solids with drag, lift and moment coefficients, configurable reference quantities and in-place editing.
- **Porous media**: isotropic Darcy–Forchheimer regions with a reported drag summary.
- **Heat and scalars**: passive scalar transport, thermal side and obstacle conditions, volumetric heat sources, obstacle Nusselt numbers and Boussinesq buoyancy.
- **Turbulence**: constant eddy viscosity, mixing length and standard k–ε.
- **Diagnostics**: residual and continuity history, boundary mass balance, per-outlet allocation, wall shear and `y⁺`, point probes, line profiles and field statistics.
- **Deterministic exports**: CSV and legacy ASCII VTK at cell centres or nodes, SVG field previews, filled contours, streamlines, history CSVs, coefficient summaries and Lagrangian particle trajectories.

## 🧪 Examples

- [Package README — CLI and SDK usage](projects/packages/open-cfd/README.md)
- [Agent skill](projects/packages/open-cfd-skills/skills/open-cfd/SKILL.md)
- [Capability matrix, numerical method and limitations](documentation/capabilities.md)
- [Architecture and data flow](documentation/architecture.md)

The full test suite lives in `projects/crates/cfd-model/tests/`; each file names the physical check it asserts.

## 📚 Documentation

| Document | Contents |
|----------|----------|
| [documentation/architecture.md](documentation/architecture.md) | Case model, module boundaries, data flow, determinism and restart |
| [documentation/capabilities.md](documentation/capabilities.md) | Supported features, report fields, numerical method, tests and limitations |
| [documentation/cli.md](documentation/cli.md) | Every `open-cfd` command and flag, plus the developer `ocfd` binary |
| [projects/packages/open-cfd/README.md](projects/packages/open-cfd/README.md) | The npm package page |
| [AGENTS.md](AGENTS.md) | Rules and verified commands for coding agents |

## 📜 License

[CC0 1.0 Universal](License.md)
