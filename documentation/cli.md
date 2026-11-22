# Command line

The npm package installs one binary, `open-cfd`, which is a thin wrapper over
the TypeScript SDK. Every command opens the `.ocfd` document, performs one
native operation, and prints a one-line confirmation or a JSON report. The Rust
binary `ocfd` is the developer entry point and exposes a wider surface; see
[Developer binary `ocfd`](#developer-binary-ocfd).

```shell
pnpm exec open-cfd --help
```

Exit codes are `0` on success, `1` for a CLI, validation or I/O error, and `2`
for a capability that is declared unsupported. Errors are printed to stderr as
`{ "code": "...", "message": "..." }`. Every edit is validated before it is
written, and every write is atomic: a failed command leaves the previous
document intact.

## Conventions

- `<file>` is a `.ocfd` path. Relative paths resolve against the shell's current
  directory; `--out` directories are created when missing.
- Sides are `left`, `right`, `top` and `bottom`.
- Options accept both `--name value` and `--name=value`.
- Integer options must be non-negative integers; numeric options must be finite.
- Omitting `--id` on an obstacle or probe assigns `obstacle-N` or `probe-N` in
  document order. Edits keep ids and order.
- `create` refuses to overwrite an existing file. Use `open` through the SDK or
  start from an existing document to edit it.

## Document commands

### create

```shell
open-cfd create <file.ocfd>
```

Writes the default lid-driven cavity: 32×32 mesh over 1×1 m, density 1 kg/m³,
viscosity 0.01 Pa·s, `dt = 0.01 s`, 400 steps, 100 pressure iterations, top lid
at 1 m/s.

### mesh set

```shell
open-cfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
```

### fluid set

```shell
open-cfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
```

`--mu 0` is accepted and means inviscid flow.

### time

```shell
open-cfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
open-cfd time scheme set <file> --scheme euler|rk2
open-cfd time adaptive set <file> --cfl <n|off>
```

`time set --tolerance` sets the relative pressure-solve tolerance in the
document; it is not the steady-state tolerance used by `solve --tolerance`.
`time adaptive set --cfl` accepts a number in `(0, 1]` or `off`.

### boundary

```shell
open-cfd boundary set <file> --side <side> --kind wall|inlet|outlet|convective|slip|periodic \
    [--u <m/s>] [--v <m/s>] [--pressure <Pa>]
open-cfd boundary list <file>
open-cfd boundary reset <file> [--side <side>]
open-cfd boundary profile set <file> --side <side> \
    --profile uniform|parabolic|powerLaw [--exponent <n>]
```

`--u` and `--v` default to the current side values; `--pressure` is the Dirichlet
gauge used by `outlet` and `convective` sides. `boundary reset` restores
stationary walls; with `--side` it affects one side and drops that side's
transient drive. `boundary profile set` writes the side's inlet profile:
`parabolic` is `4s(1−s)` and `powerLaw` is `(1−|2s−1|)^(1/n)`; `--exponent` is
the `n` used by `powerLaw`.

### obstacle

```shell
open-cfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>] [--id <name>]
open-cfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]
open-cfd obstacle update <file> --id <name> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>]
open-cfd obstacle update <file> --id <name> --kind circle --x <m> --y <m> --r <m>
open-cfd obstacle remove <file> --id <name>
open-cfd obstacle list <file>
```

Rects and circles become solid cells by cell-centre containment and are
stair-stepped. `--rotation` turns a rectangle counter-clockwise about its
centroid, in degrees. `obstacle update` replaces one obstacle in place: its id
and position in the list are preserved, and an omitted `--rotation` resets it to
`0`. `obstacle remove` deletes the obstacle and reports the remaining count; an
unknown id fails with `obstacle/not-found`. `obstacle list` prints the current
geometry as JSON.

### porous

```shell
open-cfd porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> \
    --permeability <m2> [--inertial <n>] [--id <name>]
open-cfd porous list <file>
open-cfd porous remove <file> --id <name>
```

`porous add` appends an isotropic Darcy–Forchheimer region: `--permeability` is
`K` in m² (positive) and `--inertial` is `C_F` (non-negative, default `0`).
Omitting `--id` assigns `porous-N` in document order. Validation stays in the
model, so a zero permeability, a region outside the domain or a negative `C_F`
fails with `porous/invalid` and leaves the document unchanged. `porous list`
prints the regions as JSON; `porous remove` deletes the first region with that
id and fails with `porous/not-found` for an unknown id.

### probe

```shell
open-cfd probe add <file> --x <m> --y <m> [--id <name>]
open-cfd probe update <file> --id <name> --x <m> --y <m>
open-cfd probe remove <file> --id <name>
open-cfd probe list <file>
open-cfd probe clear <file>
```

A probe must be finite and inside a fluid cell; otherwise the edit fails with
`probe/invalid` and the document is unchanged.

### scalar, buoyancy, convection, pressure, reference

```shell
open-cfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]
open-cfd buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>
open-cfd convection set <file> --scheme upwind|central|quick
open-cfd pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>
open-cfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]
```

`buoyancy set` requires scalar transport to be configured. `reference set`
requires at least one of `--velocity`, `--area` or `--length`.

### thermal

```shell
open-cfd thermal set <file> [--specific-heat <J/kgK>] [--conductivity <W/mK>] \
    [--diffusivity <m2/s>] [--heat-source <W/m3>] \
    [--left-temperature <K>] [--right-temperature <K>] [--top-temperature <K>] \
    [--bottom-temperature <K>] [--left-heat-flux <W/m2>] [--right-heat-flux <W/m2>] \
    [--top-heat-flux <W/m2>] [--bottom-heat-flux <W/m2>] \
    [--obstacle-temperature <K>] [--obstacle-heat-flux <W/m2>] \
    [--reference-temperature <K>]
```

`thermal set` turns the scalar equation into an energy equation. It matches the
developer `ocfd heat set` command: `--specific-heat` defaults to `1 J/(kg·K)`,
`--heat-source` to `0 W/m³`, and every side accepts a fixed temperature (which
takes precedence over a heat flux). When `--diffusivity` is omitted and
`--conductivity` is given, the CLI derives the diffusivity as
`k / (rho · cp)`, so scalar transport is configured at the same time. Thermal
settings require scalar transport; without it the edit fails with
`thermal/invalid` and the document is unchanged. `heat` is accepted as an alias.

### wall-function

```shell
open-cfd wall-function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]
open-cfd wall-function clear <file>
```

`wall-function set` evaluates the reported wall shear from the log law
`u⁺ = (1/κ)·ln(y⁺) + B` above `minYPlus`; omitted values keep the model defaults
`kappa 0.41`, `intercept 5.2` and `minYPlus 11.06`. `κ` must be positive or the
edit fails with `wallFunction/invalid`. `wall-function clear` removes the
section. The momentum step keeps its no-slip ghost, so the wall function changes
the report, not the boundary layer.

## Checks

```shell
open-cfd validate <file>
open-cfd quality <file>
open-cfd reynolds <file>
open-cfd stability <file>
open-cfd capabilities
```

`validate` prints the resolved mesh, fluid, step count and step size, or exits
with the validation error. `quality`, `reynolds` and `stability` print JSON.
`stability` reports the CFL, diffusion, scalar and porous limits without editing
`time.dt`; a case that violates them fails on `solve`. `capabilities` prints the
capability matrix from the native library.

## solve

```shell
open-cfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] \
    [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out <report.json>]
```

| Flag | Meaning |
|------|---------|
| `--steps` | One-shot override of `time.steps`; must be greater than zero |
| `--tolerance` | Steady-state tolerance on the L2 velocity change; enables early stop |
| `--check-every` | Sample and test the residual every *n* steps (default 1) |
| `--restart` | Resume from an `open-cfd-checkpoint` file |
| `--checkpoint` | Write the final state as an `open-cfd-checkpoint` file |
| `--out` | Also write the JSON report to this path |

The report is always printed to stdout. Without `--tolerance` the run advances
exactly `--steps` (or `time.steps`) steps and `converged` reports the final
pressure solve instead.

## export

```shell
open-cfd export <file> --format <format> --out <dir|file> \
    [--field <field>] [--layout cell|node] [--line <x0,y0,x1,y1>] [--samples <n>]
```

`--format` is one of `csv`, `vtk`, `svg`, `contours`, `streamlines`,
`residuals`, `forces`, `coefficients`, `probes`, `particles`, `profile` or
`statistics`. `--field` is one of `pressure`, `temperature`, `vorticity`,
`streamFunction`, `speed` or `pressureCoefficient`; the default is `pressure` for
`svg` and `contours` and `speed` for `profile`. `--layout node` is accepted by
`csv` and `vtk`.

| Format | Default output |
|--------|----------------|
| `csv` | `solution.csv` (`solution-nodes.csv` with `--layout node`) |
| `vtk` | `solution.vtk` (`solution-nodes.vtk`) |
| `svg` | `solution-<field>.svg` |
| `contours` | `solution-<field>-contours.svg` |
| `streamlines` | `solution-streamlines.svg` |
| `residuals` | `residuals.csv` |
| `forces` | `forces.csv` |
| `coefficients` | `coefficients.csv` |
| `probes` | `probes.csv` |
| `particles` | `particles.csv` |
| `profile` | `profile.csv` |
| `statistics` | `statistics.json` |

`--format profile` requires `--line`; `--samples` defaults to 101 and must be an
integer of at least 2. Options that a format does not consume have no effect.
`--format particles` uses the default 5×5 seed grid and 200 particle steps; the
`ocfd` binary exposes both settings.

## Examples

```shell
open-cfd create cavity.ocfd
open-cfd mesh set cavity.ocfd --nx 24 --ny 24 --length-x 1 --length-y 1
open-cfd time set cavity.ocfd --dt 0.01 --steps 200 --pressure-iterations 100 --tolerance 1e-3
open-cfd solve cavity.ocfd --tolerance 1e-4 --out report.json
open-cfd export cavity.ocfd --format csv --out out/
open-cfd export cavity.ocfd --format svg --field pressureCoefficient --out out/
open-cfd export cavity.ocfd --format profile --line 0.5,0,0.5,1 --samples 51 --out out/
```

```shell
open-cfd obstacle add cavity.ocfd --kind circle --x 0.5 --y 0.5 --r 0.1 --id cylinder
open-cfd boundary set cavity.ocfd --side left --kind inlet --u 1 --v 0
open-cfd boundary set cavity.ocfd --side right --kind outlet --pressure 0
open-cfd convection set cavity.ocfd --scheme quick
open-cfd solve cavity.ocfd --checkpoint state.json --out report.json
open-cfd solve cavity.ocfd --restart state.json --steps 200 --out report2.json
```

## Developer binary `ocfd`

`cargo run -p cfd-model --bin ocfd -- <command>` runs the same session code as
the SDK, with additional maintainer commands. It uses the same exit codes and
the same atomic writes.

Additional document commands:

| Command | Purpose |
|---------|---------|
| `obstacle add … [--rotation <deg>]` | Add a rotated rectangle |
| `obstacle update … [--rotation <deg>]` | Update a rectangle and set its rotation |
| `obstacle remove <file> --id <name>` | Remove an obstacle |
| `porous add <file> --x0 … --y1 … --permeability <m2> [--inertial <n>] [--id <name>]` | Add a Darcy–Forchheimer region |
| `porous list <file>` / `porous remove <file> --id <name>` | Inspect or remove regions |
| `inlet series set <file> --side <side> [--u <t:v,…>] [--v <t:v,…>] [--pressure <t:v,…>]` | Write a transient drive |
| `inlet series list <file>` / `inlet series clear <file>` | Inspect or clear drives |
| `scalar source add <file> --x0 … --y1 … --rate <unit/s> [--start <s>] [--ramp <s>] [--id <name>]` | Add a volumetric source |
| `scalar source list <file>` / `scalar source clear <file>` | Inspect or clear sources |
| `turbulence set <file> --model none\|constant\|mixingLength\|kEpsilon [--nu-t <m2/s>] [--mixing-length <m>] [--k <m2/s2>] [--epsilon <m2/s3>]` | Select a turbulence model |
| `heat set <file> [--conductivity <W/mK>] [--specific-heat <J/kgK>] [--diffusivity <m2/s>] [--heat-source <W/m3>] [--left-temperature …] [--right-temperature …] [--top-temperature …] [--bottom-temperature …] [--left-heat-flux …] [--right-heat-flux …] [--top-heat-flux …] [--bottom-heat-flux …] [--obstacle-temperature <K>] [--obstacle-heat-flux <W/m2>] [--reference-temperature <K>]` | Configure thermal transport |
| `wall function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]` | Configure the reported log-law shear |
| `wall function clear <file>` | Remove the wall function |

`ocfd export` additionally accepts
`--field turbulentKineticEnergy|turbulentDissipation`, `--seed <x,y>` (separate
points with `;`) and `--particle-steps <n>`. `ocfd solve` accepts the same flags
as `open-cfd solve`.
