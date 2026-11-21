---
name: open-cfd
description: Create, edit, validate, solve and export two-dimensional incompressible Navier-Stokes cases with OpenCFD. Use when a task involves cavity or channel flow cases, mesh, fluid, time, boundary (including slip and pressure outlets), obstacle, passive scalar, turbulence or thermal settings, force/moment references, solver reports, mass balance, or CSV, VTK, SVG and streamline output.
---

# open-cfd

## Status

The two-dimensional incompressible document core is executable. The CLI and SDK
create and reopen one `.ocfd` case, edit mesh, fluid, time, boundary (including
free-slip walls and outlet pressure), obstacle, force/moment reference, passive
scalar, turbulence and thermal settings, validate it, solve it and export CSV,
legacy ASCII VTK, SVG field previews or SVG streamlines. The solve report also
contains point monitor time series (`probes`), wall shear/friction summaries
(`wallShear`), drag/lift/moment coefficients (`forces`), sampled continuity and
boundary mass balance (`massBalance`). Compressible flow, three-dimensional flow,
Boussinesq buoyancy, periodic boundaries, contour export, adaptive meshing and
external format import are not implemented and report
`capability/not-implemented`.

Only these commands are executable today:

```bash
ocfd capabilities
ocfd create <file.ocfd>
ocfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
ocfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
ocfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
ocfd boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|slip [--u <m/s>] [--v <m/s>] [--pressure <Pa>]
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
ocfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]
ocfd validate <file>
ocfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--out report.json]
ocfd export <file> --format csv|vtk|svg|streamlines [--field pressure|temperature|vorticity|streamFunction] --out <dir|file>
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
- `csv`, `vtk`, `svg` and `streamlines` are the only export targets. `inp` and
  `openfoam` reject with `capability/not-implemented`; periodic boundaries and
  contour export are not implemented.
- `solve` stops early on the L2 norm of the velocity change when `--tolerance`
  is given and reports `converged`, `steadyState`, `residual`,
  `residualHistory` (with `continuity`), `massBalance` and `forces` (with
  `cm` and the resolved reference quantities). It prints the JSON report on
  stdout and a one-line summary on stderr.

## Hard rules

- Never claim a solve or validation ran when it did not, and never fabricate
  residuals or convergence results.
- Never describe a format as supported because an extension is listed; read the
  capability matrix.
- Report the exact command, revision and export path used.
- Do not invent mesh refinement, turbulence models or boundary types outside the
  implemented commands; `slip` and outlet `pressure` are the supported additions.

## Safety

- Confirm with the user before any command that writes or overwrites a case
  document, and state the target path first.
- Never overwrite an input case unless the task asks for an in-place edit.
- Solver and export outputs are analysis inputs; state exactly what was produced
  and what was not.
