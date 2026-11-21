---
name: open-cfd
description: Create, edit, validate, solve and export two-dimensional incompressible Navier-Stokes cases with OpenCFD. Use when a task involves cavity or channel flow cases, mesh, fluid, time, boundary (including slip, pressure outlets and periodic sides), pressure-driven flow, obstacle, passive scalar, turbulence or thermal settings, force/moment references, solver reports, mass balance, or CSV, VTK, SVG and streamline output.
---

# open-cfd

## Status

The two-dimensional incompressible document core is executable. The CLI and SDK
create and reopen one `.ocfd` case, edit mesh, fluid, time (including the
`euler`/`rk2` scheme), boundary (including free-slip walls, outlet pressure, periodic sides and
parabolic/power-law inlet profiles), uniform pressure gradients for
pressure-driven flow, obstacle, force/moment reference, passive
scalar, turbulence and thermal settings, validate it, solve it and export CSV,
legacy ASCII VTK, SVG field previews, filled-contour/iso-line SVG, SVG
streamlines, residual history CSV or force history CSV. `solve` can resume from
a checkpoint and write one back. The solve report also
contains point monitor time series (`probes`), wall shear/friction summaries
(`wallShear`), drag/lift/moment coefficients (`forces`), sampled continuity and
boundary mass balance (`massBalance`), the selected `timeScheme`, adaptive-step statistics (`adaptive`,
`averageDt`, `minDt`, `maxDt`) and the `terminationReason`. Compressible flow, three-dimensional flow, Boussinesq
buoyancy, adaptive meshing and external format import are
not implemented and report `capability/not-implemented`.

Only these commands are executable today:

```bash
ocfd capabilities
ocfd create <file.ocfd>
ocfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
ocfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
ocfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
ocfd time scheme set <file> --scheme euler|rk2
ocfd time adaptive set <file> --cfl <n|off>
ocfd boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|slip|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]
ocfd boundary profile set <file> --side left|right|top|bottom --profile uniform|parabolic|powerLaw [--exponent <n>]
ocfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--id <name>]
ocfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]
ocfd obstacle list <file>
ocfd probe add <file> --x <m> --y <m> [--id <name>]
ocfd probe list <file>
ocfd probe clear <file>
ocfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]
ocfd turbulence set <file> --model none|constant|mixingLength [--nu-t <m2/s>] [--mixing-length <m>]
ocfd heat set <file> [thermal options]
ocfd convection set <file> --scheme upwind|central|quick
ocfd pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>
ocfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]
ocfd validate <file>
ocfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]
ocfd export <file> --format csv|vtk|svg|contours|streamlines|residuals|forces [--field pressure|temperature|vorticity|streamFunction|speed] --out <dir|file>
```

Every editing command except `capabilities`, `validate`, `obstacle list`, `probe list`, `solve`
and `export` rewrites the input document in place through a temporary file and a
rename. Confirm the path before running them.

## Preflight

1. Run `ocfd capabilities` and confirm that the setting, solver mode or export
   target the task needs is reported as `supported`. A method or extension name
   is not a capability.
2. Run `ocfd validate <file.ocfd>` and confirm that the mesh, fluid, time and
   boundary settings the task will address are present and consistent.
3. If the preflight reports a missing capability, stop and report it. Never
   degrade silently or swap in an unrelated tool.

## Executable routes

| Route | Intent | Minimum evidence |
|-------|--------|------------------|
| create | Build a case from a brief | `ocfd validate` output and the saved `.ocfd` document |
| edit | Change mesh, fluid, time, boundary, obstacle, reference or scheme settings | `ocfd validate` before and after |
| validate | Check that a case is well formed | `ocfd validate` findings with codes |
| solve | Integrate the case to a report | the solver report with residual/continuity history, mass balance and forces |
| deliver | Write CSV, VTK, SVG or streamlines output | the exported file plus the solver report it came from |

## Solver and export limits

- The solver is two-dimensional, single-phase, incompressible and
  constant-property, on a structured Cartesian mesh.
- Turbulence is limited to `none`, `constant` and `mixingLength` eddy viscosity.
- Heat transfer uses the implemented passive scalar and thermal configuration;
  there is no coupled buoyancy.
- `csv`, `vtk`, `svg`, `contours`, `streamlines`, `residuals` and `forces` are
  the only export targets. `inp` and `openfoam` reject with
  `capability/not-implemented`. Periodic sides must come in opposite pairs and
  are excluded from the boundary mass balance.
  `contours` writes nine filled value bands plus marching-squares iso-lines and
  accepts the same `--field` names as `svg`, including `speed`.
- Momentum time integration is `euler` (default) or `rk2`; both are explicit and
  share the same CFL and diffusion stability guards. Set `time.adaptive` with
  `time.targetCfl` in `(0, 1]` (or `--cfl <n>` / `--cfl off`) to clamp every
  step; `dt` remains the upper bound and the report adds `averageDt`, `minDt`
  and `maxDt`.
- Inlet profiles (`uniform`, `parabolic`, `powerLaw`) shape only the normal
  velocity of an `inlet` side.
- `pressure gradient set --dpdx ... --dpdy ...` imposes a uniform pressure
  gradient in Pa/m as the body force `-∇p/ρ`; `0 0` clears it. It is constant in
  space and time and is not a general body-force field.
- `solve` stops early on the L2 norm of the velocity change when `--tolerance`
  is given and reports `converged`, `steadyState`, `residual`,
  `residualHistory` (with `continuity`), `terminationReason`, `timeScheme`,
  `massBalance` and `forces` (with `cm` and the resolved reference quantities).
  `--restart` resumes from an `open-cfd-checkpoint` JSON document and
  `--checkpoint` writes the final state; resumed reports keep cumulative
  `steps` and `time`. The mesh and scalar configuration must match or the
  command fails with `checkpoint/mismatch`.
- Do not hand-edit checkpoints: they are written by the solver and validated on
  load (`checkpoint/invalid` for a malformed format, version, shape or values).
  It prints the JSON report on stdout and a one-line summary on stderr.

## Hard rules

- Never claim a solve or validation ran when it did not, and never fabricate
  residuals or convergence results.
- Never describe a format as supported because an extension is listed; read the
  capability matrix.
- Report the exact command, revision and export path used.
- Do not invent mesh refinement, turbulence models or boundary types outside the
  implemented commands; `slip`, outlet `pressure` and paired `periodic` sides are
  the supported boundary additions.

## Safety

- Confirm with the user before any command that writes or overwrites a case
  document, and state the target path first.
- Never overwrite an input case unless the task asks for an in-place edit.
- Solver and export outputs are analysis inputs; state exactly what was produced
  and what was not.
