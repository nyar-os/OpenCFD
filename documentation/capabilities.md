# Capabilities

`CAPABILITIES_JSON` in `projects/crates/cfd-model/src/lib.rs` is the single
support claim for this build. `ocfd capabilities` and `open-cfd capabilities`
print it verbatim; a method name, CLI flag or document field is not evidence
that a capability exists.

## Capability matrix

### Solver

| Key | Status | Notes |
|-----|--------|-------|
| `cavity2d` | supported | Two-dimensional lid-driven cavity |
| `couette2d` | supported | Couette-like and channel flow |
| `obstacles` | supported | Rectangular (optionally rotated) and circular solid obstacles |
| `obstacleEditing` | supported | Add, list, update and remove obstacles in place; ids and order preserved |
| `probeEditing` | supported | Add, update, remove, list and clear point monitors |
| `stabilityLimits` | supported | Pre-flight CFL, diffusion, scalar and porous report; never edits `time.dt` |
| `porousMedia` | supported | Isotropic Darcy–Forchheimer regions |
| `scalar` | supported | Passive scalar (temperature) transport |
| `scalarSources` | supported | Rectangular volumetric sources with start time and linear ramp |
| `convergence` | supported | Steady-state velocity residual with early stop |
| `vorticity` | supported | Cell-centred vorticity and report extremes |
| `streamFunction` | supported | Cell-centred stream function and report extremes |
| `pressureCoefficient` | supported | `cp = 2(p − pRef)/(ρU²)` with gauge datum `pRef = 0` |
| `convectionSchemes` | supported | `upwind`, `central`, `quick` |
| `timeSchemes` | supported | `euler`, `rk2` |
| `adaptiveTimeStep` | supported | CFL-limited step with mean/min/max statistics |
| `probes` | supported | Point monitor time series |
| `wallShear` | supported | Wall shear, `y⁺` and friction coefficient per wall side |
| `wallFunction` | supported | Log-law evaluation of the reported wall shear |
| `slipBoundary` | supported | Free-slip walls |
| `pressureOutlet` | supported | Dirichlet gauge pressure on outlets |
| `convectiveOutlet` | supported | Local-outflow-speed relaxation at the outlet face |
| `periodic` | supported | Opposite-side pairs with wrapped stencils |
| `pressureDriven` | supported | Uniform imposed pressure gradient |
| `inletProfiles` | supported | `uniform`, `parabolic`, `powerLaw` |
| `transientInlets` | supported | Time-series `u`, `v` and outlet pressure drives |
| `momentCoefficients` | supported | `cd`, `cl`, `cm` with configurable reference quantities |
| `massBalance` | supported | Boundary inflow, outflow, net and relative error |
| `meshQuality` | supported | Non-orthogonality, aspect ratio and skewness |
| `outletAllocation` | supported | Signed flow, share and mean velocity per outlet side |
| `buoyancy` | supported | Boussinesq coupling; requires `scalar` |
| `turbulence` | supported | `none`, `constant`, `mixingLength` |
| `kEpsilonTurbulence` | supported | Standard k–ε with explicit `k` and `ε` transport |
| `heat` | supported | Thermal side/obstacle conditions, heat source and Nusselt numbers |
| `compressible` | unsupported | — |
| `threeD` | unsupported | — |

### Input and output

| Key | Status | Notes |
|-----|--------|-------|
| `ocfd` | supported | `.ocfd` JSON read and write |
| `csv` | supported | Cell-centred or node-sampled field CSV |
| `vtk` | supported | Legacy ASCII `UNSTRUCTURED_GRID` |
| `nodeLayout` | supported | `--layout cell|node` for CSV and VTK |
| `svg` | supported | Field preview |
| `streamlines` | supported | SVG streamlines |
| `contours` | supported | Filled contours and iso-lines |
| `history` | supported | Residual/continuity and force history CSV |
| `probes` | supported | Point-monitor history CSV |
| `coefficients` | supported | One drag/lift/moment summary row per obstacle |
| `profile` | supported | Line-profile CSV |
| `statistics` | supported | Per-field statistics JSON |
| `restart` | supported | Checkpoint read and write |
| `particles` | supported | Lagrangian trajectories as CSV |
| `inp` | unsupported | — |
| `openfoam` | unsupported | — |

Requests for an unsupported route fail with `capability/not-implemented`
(exit code `2`) instead of degrading silently.

## Solve report

`CfdCase.solve()` and `ocfd solve` return one `open-cfd-report` object
(`format: "open-cfd-report"`, `version: 1`).

| Field | Meaning |
|-------|---------|
| `steps` / `dt` / `time` | Steps advanced since `t = 0` (including a restored checkpoint), the last step size and total physical time |
| `adaptive` / `averageDt` / `minDt` / `maxDt` | Whether adaptive stepping ran, and its mean, minimum and maximum step; all equal `dt` when adaptivity is off |
| `maxVelocity` | Maximum absolute face velocity |
| `minPressure` / `maxPressure` | Pressure extremes over fluid cell centres |
| `vorticityMin` / `vorticityMax` | Vorticity `∂v/∂x − ∂u/∂y` extremes (1/s) |
| `streamFunctionMin` / `streamFunctionMax` | Stream-function extremes (m²/s); the field has zero fluid-cell mean |
| `divergenceL2` | RMS discrete divergence after the final projection (1/s) |
| `meshQuality` | `{cells, fluidCells, dx, dy, minAspectRatio, meanAspectRatio, maxAspectRatio, meanNonOrthogonality, maxNonOrthogonality, meanSkewness, maxSkewness}`; angles in degrees, skewness as a fraction of cell-centre separation |
| `converged` | With a tolerance: whether steady state was reached. Without one: whether the final pressure solve met its relative tolerance |
| `steadyState` | Whether the L2 norm of the velocity change fell below `tolerance` |
| `residual` / `residualHistory` | Final and sampled L2 velocity change; each sample also carries `continuity`, the RMS divergence after that step |
| `terminationReason` | `maxSteps` or `steadyState` |
| `pressureConverged` | Whether the final pressure solve met `tolerance × max(initial residual, 1e-12)` |
| `pressureIterations` | SOR sweeps in the final step |
| `pressureResidual` / `pressureResidualInitial` | Final and initial pressure-Poisson residuals of the final step |
| `timeScheme` | `euler` or `rk2` |
| `convection` | `upwind`, `central` or `quick` |
| `turbulenceModel` | `none`, `constant`, `mixingLength` or `kEpsilon` |
| `eddyViscosityMin` / `eddyViscosityMax` / `eddyViscosityMean` | Eddy viscosity over fluid cells (m²/s) |
| `turbulentKineticEnergyMin` / `…Max` / `…Mean` | `k` statistics (m²/s²) under `kEpsilon` |
| `turbulentDissipationMin` / `…Max` / `…Mean` | `ε` statistics (m²/s³) under `kEpsilon` |
| `buoyancy` | Whether Boussinesq buoyancy coupled temperature to momentum |
| `massBalance` | `{inflow, outflow, net, relativeError}` in kg/s; `net = outflow − inflow` |
| `outlets` | Per outlet/convective side `{side, flow, share, meanVelocity, length}` in `left`, `right`, `top`, `bottom` order |
| `forces` | Per obstacle `{id, kind, fx, fy, fxPressure, fxViscous, fyPressure, fyViscous, mz, cd, cl, cm, referenceVelocity, referenceArea, referenceLength, history}` |
| `probes` | Per probe `{id, x, y, cell, history}`; `cell` is `[i, j]`, each sample is `{step, time, u, v, speed, pressure, temperature}` with `temperature: null` without a scalar |
| `wallShear` | Per wall side `{side, meanShear, maxShear, meanYPlus, frictionCoefficient}` |
| `wallFunction` | Whether the log law evaluated the reported wall shear |
| `porous` | Per region `{id, cells, meanSpeed, meanDrag}` |
| `scalarSources` | Per source `{id, cells, rate, appliedRate}` |
| `inletHistory` | Per driven side `{side, history}` with samples `{step, time, u, v, pressure}` |
| `nusselt` | Per obstacle `{id, kind, meanHeatFlux, wallTemperature, referenceTemperature, nusselt}` when a thermal section is present |

Detail on the nested values:

- `fxPressure + fxViscous = fx` and `fyPressure + fyViscous = fy`. The moment
  pivot is the obstacle centroid and positive `mz`/`cm` is counter-clockwise.
- Force coefficients use the resolved reference speed, area and length; defaults
  are the maximum inlet speed (1 m/s when there is no inlet), the obstacle
  characteristic length times unit depth, and the obstacle characteristic
  length. A `reference` section overrides them independently.
- Probe values are bilinearly interpolated from the four surrounding cell
  centres, clamped at the domain edge and renormalised over fluid cells.
- `wallShear.meanShear` is the length-averaged tangential stress the wall exerts
  on the fluid (Pa), positive along the side's positive tangent. `meanYPlus` is
  the length-averaged friction `y⁺`; `frictionCoefficient = meanShear / (0.5 ρ U²)`.
  Slip, inlet and outlet sides are not reported.
- `outlets.flow` is signed: positive when the side discharges, negative under
  recirculation. Shares divide by the total positive outlet flow, so they sum to
  one only when every reported outlet discharges.
- A tolerance on the pressure solve is relative: it passes when the residual
  falls to `tolerance × max(initial residual, 1e-12)`. The `solve` option
  `tolerance` instead applies to the L2 velocity change and does not change the
  document pressure tolerance.

A measured 24×24, `dt = 0.01 s`, 200-step, Re=100 cavity run reports:

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

## Exports

Every export solves the case first and writes one file atomically (temporary
file plus rename). A target ending in the format's extension is used as a file;
otherwise it is treated as a directory and the default name below is written
inside it.

| Format | Default name | Contents |
|--------|--------------|----------|
| `csv` | `solution.csv` (`solution-nodes.csv` for nodes) | Header `x,y,z,pressure,velocity_x,velocity_y,velocity_z,magnitude,temperature,vorticity,streamFunction,pressureCoefficient`; `nx × ny` rows at cell centres or `(nx + 1) × (ny + 1)` rows at vertices |
| `vtk` | `solution.vtk` (`solution-nodes.vtk`) | Legacy ASCII `UNSTRUCTURED_GRID` with point data `pressure`, `velocity`, `magnitude`, `temperature`, `vorticity`, `streamFunction`, `pressureCoefficient` |
| `svg` | `solution-<field>.svg` | Self-contained field preview with a nine-band palette and a legend |
| `contours` | `solution-<field>-contours.svg` | Filled bands, marching-squares iso-lines and a colour legend |
| `streamlines` | `solution-streamlines.svg` | Traced streamlines over a grey solid mask |
| `residuals` | `residuals.csv` | Header `step,residual,continuity` |
| `forces` | `forces.csv` | Header `obstacle,kind,step,fx,fy,fxPressure,fxViscous,fyPressure,fyViscous,mz,cd,cl,cm` |
| `coefficients` | `coefficients.csv` | Header `obstacle,kind,cd,cl,cm,fx,fy,mz,referenceVelocity,referenceArea,referenceLength` |
| `probes` | `probes.csv` | Header `probe,x,y,step,time,u,v,speed,pressure,temperature`; an empty temperature when no scalar is configured |
| `particles` | `particles.csv` | Header `particle,step,time,x,y,u,v,speed` |
| `profile` | `profile.csv` | Header `distance,x,y,value`; `--line x0,y0,x1,y1` is required, `--samples` defaults to 101 and must be at least 2, and the field defaults to `speed`; solid or outside samples leave `value` empty |
| `statistics` | `statistics.json` | Tag `open-cfd-statistics`, mesh counts, then `{min, max, mean, rms, count}` for `speed`, `pressure`, `u`, `v`, `vorticity`, `streamFunction`, and `temperature`, `turbulentKineticEnergy`, `turbulentDissipation` when available |

Field names accepted by `open-cfd export --field` are `pressure`, `temperature`,
`vorticity`, `streamFunction`, `speed` and `pressureCoefficient`. The Rust `ocfd`
binary additionally accepts `turbulentKineticEnergy` and `turbulentDissipation`,
which are also available through the SDK's `ExportField` type. When the
`kEpsilon` model is active, CSV and VTK carry `turbulentKineticEnergy` and
`turbulentDissipation` columns.

`--layout node` samples the `(nx + 1) × (ny + 1)` vertices instead of the
`nx × ny` cell centres: each vertex value is the mean of its adjacent fluid
cells, solid cells contribute nothing, and the VTK grid covers the full
rectangle with `nx × ny` quads.

## Numerical method

1. **Staggered (MAC) mesh**: pressure lives at the centre of each of the
   `nx × ny` cells; `u` lives on the `(nx+1) × ny` x-faces and `v` on the
   `nx × (ny+1)` y-faces. A cell whose centre lies inside an obstacle is solid;
   every face between a solid cell and a fluid cell, or between two solid
   cells, carries zero velocity.
2. **Momentum prediction**: explicit convection plus central-difference
   diffusion and the configured time scheme produce tentative `u*` and `v*`. A
   configured `pressureGradient` adds the uniform body force `-∇p/ρ` to every
   open face. A `porous` region adds the Darcy–Forchheimer sink
   `−(ν/K)U − (C_F/√K)|U|U` to the open faces whose adjacent cell the region
   owns. `time.scheme` selects explicit Euler (default) or second-order
   Runge–Kutta (Heun, two derivative evaluations per step), both followed by the
   same pressure projection. `convection` selects first-order upwind (default),
   second-order central or third-order QUICK for the face-normal derivative; the
   transverse gradient stays central, and QUICK falls back to upwind on the
   first interior face next to a boundary. Tangential wall velocities use
   half-cell ghost values; slip walls mirror the ghost for a zero tangential
   gradient; solid faces are not sampled as fluid. Inlet sides scale the normal
   velocity by the `inletProfiles` factor. Periodic sides mirror to the opposite
   side, and momentum, pressure and correction stencils wrap across the axis.
3. **Pressure Poisson**: `∇²p = (ρ/dt) ∇·u*`. Wall, inlet, slip and solid faces
   use homogeneous Neumann conditions; each outlet uses a Dirichlet value from
   `pressure`. Solid cells are not unknowns (pressure stays 0) and their
   divergence is forced to zero. SOR solves the system with a relaxation factor
   derived from the largest mesh side; iteration count and relative tolerance
   come from the document. In a fully closed domain the fluid-cell mean pressure
   is removed after the sweeps.
4. **Velocity correction**: `u = u* − (dt/ρ) ∇p` updates open faces only, then
   normal boundary conditions and zero solid velocities are reapplied. A
   `convective` side then relaxes towards its new interior value with the local
   outflow speed; its last cell column carries a small compensated divergence
   that the next projection absorbs.
5. **Obstacle forces**: on every fluid-solid face, pressure `p × face length` and
   viscous shear `μ·u_fluid/(half-cell distance) × face length` are accumulated
   per obstacle `id`, with the pressure and viscous parts kept separately. The
   face force crossed with its position relative to the centroid gives `mz`.
   `Cd = 2Fx/(ρU²A)`, `Cl = 2Fy/(ρU²A)` and `Cm = 2Mz/(ρU²AL)`.
6. **Passive scalar**: the same convection scheme plus central diffusion is
   advanced explicitly at cell centres; solid faces carry zero flux, solid cells
   hold no scalar, and inlet faces use `inletValue`. A violation returns
   `solver/unstable`. Scalar sources add `rate · f(t)` over their fluid cells,
   where `f` is zero before `start` and ramps linearly over `ramp` seconds.
7. **Convergence detection**: each step computes the L2 norm of the velocity
   change, samples it every `checkEvery` steps and tests it against `tolerance`,
   stopping early when it passes. Each sample stores the RMS divergence; history
   is capped at 1000 entries.
8. **Stability guards**: before solving, the explicit convective CFL number and
   momentum and scalar diffusion numbers are checked; a violation returns
   `solver/unstable` and never emits NaN or Inf. The standalone `stability`
   report measures the same bounds (including the porous drag rate) and the
   largest satisfying step, but never edits the document. Every step also checks
   that all fields are finite. With adaptive stepping the initial guard is
   skipped because every step is clamped instead.
9. **Turbulence**: `constant` applies a fixed `nuT`; `mixingLength` evaluates
   `nuT = l²·|S|` per cell from the resolved strain-rate tensor; `kEpsilon`
   solves standard `k` and `ε` transport explicitly with `νt = 0.09·k²/ε`,
   first-order upwind advection and central diffusion. Fluid faces use the mean
   of the adjacent cell values; solid cells carry zero. `kEpsilon` needs
   positive initial `k` and `epsilon` and shares the explicit diffusion and CFL
   guards.
10. **Thermal transport**: with a `thermal` section the scalar equation becomes
    an energy equation. The transported diffusivity is `k/(ρ·cp)`; sides and
    obstacle surfaces use fixed temperature or fixed heat flux, and a uniform
    volumetric source is added. Every obstacle reports `meanHeatFlux`,
    `wallTemperature` and `Nu = q̅·D/(k·ΔT)`.
11. **Derived fields**: cell-centred vorticity `ω = ∂v/∂x − ∂u/∂y` uses central
    differences of averaged face velocities, with one-sided differences on the
    first and last row or column. The stream function satisfies `∂ψ/∂y = u` and
    `∂ψ/∂x = −v`, is integrated from the bottom-left corner and has its fluid-cell
    mean removed, so it is gauge-fixed rather than unique in multiply connected
    domains.
12. **Probes and wall shear**: each probe is bound to the nearest pressure cell
    and records `u`, `v`, speed, pressure and temperature every `checkEvery`
    sampling step, using the same bounded history as residuals and forces. Every
    wall side reports `μ·(u_t,wall − u_t,cell)/(half-cell distance)` averaged over
    its fluid-adjacent boundary cells, so a linear tangential profile is
    reproduced exactly. A `wallFunction` section replaces that estimate above
    `minYPlus` by solving `u⁺ = (1/κ)ln(y⁺) + B` for the friction velocity and
    reporting `τ_w = ρu_τ²`; every side also reports `meanYPlus`.
13. **Boundary mass balance**: after the final projection, outward mass flux
    `ρ·u·dy` or `ρ·v·dx` is integrated over all four sides. The report splits it
    into inflow and outflow magnitudes and reports the signed net and
    `|net| / max(inflow, outflow)`.
14. **Streamlines**: a self-contained SVG traces streamlines from a regular seed
    grid with a second-order midpoint rule in both directions, using bilinear
    cell-centred velocity. Integration stops at solids, domain sides, stagnation
    or 600 steps; solid cells are drawn as a grey mask.
15. **Contours**: a self-contained SVG samples one field at pressure-cell
    centres, clips every interior quad into nine value bands by linear
    interpolation along its edges, fills each band with one palette colour and
    traces the eight band boundaries with marching squares. Ambiguous saddle
    cells use the average corner value; solid cells stay dark grey, and the
    legend carries the discrete bands and the physical range. `speed` is
    `√(u² + v²)` at cell centres.
16. **Adaptive time step**: with `time.adaptive`, every step is recomputed as
    `min(time.dt, targetCfl·min(dx,dy)/max(U), 0.5/((ν+νt)·(1/dx²+1/dy²)),
    0.5/(κ·(1/dx²+1/dy²)), 0.9/(ν/K + C_F·|U|/√K))`, where `U` includes boundary
    velocities, `κ` is the scalar or thermal diffusivity and the last term bounds
    explicit porous drag. The report stores the mean, smallest and largest step
    and the summed physical time, so a fixed step budget reaches a different
    final time than the non-adaptive run.

Upwind plus explicit Euler is first-order in space and time, so the default
configuration has visible numerical diffusion. `central` and `quick` sharpen
fronts at the cost of possible dispersive overshoot and a tighter cell-Péclet
limit.

## Validation and tests

```shell
cargo test -p cfd-model --offline
```

`projects/crates/cfd-model/tests/cfd_model.rs` covers:

1. **Still cavity**: 40 steps leave velocity and divergence near zero, exact
   zero boundary mass balance, no NaN or Inf.
2. **Lid-driven cavity**: 24×24, 300 steps, Re=100; positive `u` on the upper
   centreline, maximum speed at most 1.05, `divergenceL2 < 1e-2`, every report
   field present; measured maximum speed about 0.84 and `divergenceL2` about
   1.4e-5.
3. **Pressure Poisson residual**: 300 SOR sweeps on a fixed divergence field
   leave a residual below 1% of the first residual, and 300 sweeps beat 50.
4. **Export formats**: the cell CSV has `nx*ny+1` rows, the node CSV
   `(nx+1)*(ny+1)+1`, both with the header through `pressureCoefficient`; the
   VTK contains the grid, `SCALARS` fields (including `pressureCoefficient`) and
   `VECTORS velocity`; the SVG preview names the field and unit; the streamline
   SVG contains polylines.
5. **Document round trip**: save and load are identical, camelCase fields are
   correct, defaults deserialize, and a wrong `format` is rejected.
6. **Solid mask**: all four faces of a solid cell carry zero velocity,
   divergence stays zero, and an obstacle force report is present.
7. **Flow past a cylinder**: a 48×24 channel at Re≈100 yields a finite positive
   `Cd`, a finite `Cl` and no NaN.
8. **Scalar diffusion**: a hot spot decays monotonically in still fluid; a
   uniform field stays uniform.
9. **Steady-state convergence**: a loose tolerance stops early and reports
   `converged: true`; a strict tolerance with few steps reports `false`.
10. **Derived fields**: linear shear `u = y` gives vorticity `−1` and
    stream-function increments `u·dy`; solid-body rotation `(−y, x)` gives
    vorticity `2`; the solved stream function has zero fluid-cell mean.
11. **Vorticity SVG**: exporting vorticity writes a self-contained SVG naming
    `vorticity (1/s)` and its numerical range.
12. **Convection schemes**: central and QUICK differentiate a quadratic profile
    exactly while upwind carries `O(dx)` error; QUICK equals upwind on the first
    interior face; a scalar step is at least twice as thick under upwind; cavity
    solutions differ between schemes and remain finite.
13. **Probes**: a probe at `(0.05, 0.95)` on a `10 × 6` mesh resolves to cell
    `[0, 5]`; out-of-domain and in-solid probes fail with `probe/invalid`.
14. **Wall shear**: a synthetic linear tangential profile gives exactly `±μ` on
    the top, bottom and left walls and `2μ` for the friction coefficient at
    `U = 1 m/s`; a moving-lid cavity reports positive shear; a still cavity
    reports zero.
15. **Slip walls and pressure outlets**: slip faces carry zero normal velocity
    but leave tangential faces untouched; `pressure = 2.5 Pa` drives the whole
    pressure field to 2.5; a slip channel reports no wall shear.
16. **Moment and references**: every obstacle exposes finite `mz` and `cm`;
    overriding reference speed, area and length scales `cd` and `cm` as
    documented.
17. **Mass balance and continuity**: a closed cavity reports exactly zero
    inflow, outflow and net; a channel reports positive flow with relative
    imbalance below 25%; every residual sample carries finite non-negative
    continuity.
18. **Streamlines**: exporting the cavity writes `solution-streamlines.svg`
    with the title, subtitle and at least one polyline.
19. **Inlet profiles**: a parabolic inlet reproduces `4s(1−s)` exactly, a
    power-law inlet with `n=1` is triangular, a uniform profile is constant, and
    the solved mass balance equals the discrete parabolic mask integral; a zero
    exponent fails with `boundary/invalid`.
20. **History exports**: `residuals.csv` has `step,residual,continuity`;
    `forces.csv` has one 13-column row per sample; `coefficients.csv` has one
    11-column summary row per obstacle; `probes.csv` has one 10-column row per
    sampled reading.
21. **Time schemes**: Euler and RK2 stay finite at the same physical time and
    report their scheme; RK2 has less than half the Euler temporal error against
    a fine RK2 reference.
22. **Termination reason**: a loose tolerance stops with `steadyState`; a strict
    short run reports `maxSteps`.
23. **Contours**: exporting `speed` writes filled-band polygons, iso-lines and
    legend text; a named `.svg` target is used verbatim; temperature contours
    without a scalar fail with `export/no-field`.
24. **Adaptive time step**: `dt = 0.05 s`, target CFL 0.5 and `h = 0.05 m`
    reduce each step to about 0.025 s, report `adaptive = true` and
    `time = averageDt × steps`; `targetCfl > 1` fails with `time/invalid`.
25. **Periodic sides**: an unpaired side fails with `boundary/invalid`; the
    opposite face is copied; a periodic channel wraps both faces and reports
    zero boundary mass balance because periodic sides are excluded.
26. **Restart checkpoints**: five steps plus a resumed five steps match a
    continuous ten-step solve field-for-field, report cumulative `steps = 10`,
    and reject a checkpoint from a different mesh with `checkpoint/mismatch`.
27. **Convective outlet**: the kind parses and owns the Dirichlet pressure; the
    face gets the zero-gradient value before relaxation;
    `(φ_bⁿ + βφ_i)/(1 + β)` is reproduced exactly with `β = uₙ·dt/dx` and falls
    back to zero gradient on an inflow basis; a channel keeps interior
    divergence below `5e-3 1/s`.
28. **Porous regions**: a valid region round-trips; zero permeability, an
    outside region and a negative `C_F` fail with `porous/invalid`; a full-domain
    region slows a pressure-driven channel and reports `cells = nx·ny` with
    positive `meanDrag`; a step too large for `(ν/K)·dt` fails with
    `solver/unstable`.
29. **Wall function**: a 10 m/s, 0.01 m, 1e-5 m²/s sample solves the log law to a
    velocity residual below `1e-8`, gives more than five times the laminar shear
    and reverses sign with the velocity; a low-`y⁺` sample stays on the viscous
    value; `kappa ≤ 0` and negative `minYPlus` fail with `wallFunction/invalid`.
30. **Wall-function integration**: a cavity with `wallFunction` reports
    `wallFunction: true`, keeps every side below `meanYPlus` 11.06 and
    reproduces the laminar shear.
31. **Mesh quality**: a 10×5 mesh with `dx = 0.1 m`, `dy = 0.2 m` reports
    aspect ratio 2, zero non-orthogonality and zero skewness; a solid lowers
    `fluidCells`; the solve report and the `quality` command print the same JSON.
32. **Particles**: a tracer in a uniform `1 m/s` field advances exactly `dt·U`
    per midpoint step and records one initial sample plus one per step; it stops
    before a solid and never reports a solid position; the CSV header is
    `particle,step,time,x,y,u,v,speed`; a zero step or an outside seed fails with
    `particle/invalid`; the default 5×5 grid produces 25 tracks.
33. **Outlet allocation**: one `outlet` and one `convective` side report the same
    `2 kg/s` flow and a `0.5` share each; a single right outlet reports share 1;
    a closed cavity reports an empty array.
34. **Pressure coefficient**: a stationary cavity reports a constant
    `pressureCoefficient`; a channel with a pressure drop decreases downstream;
    the CSV column matches the field and `cp` parses as a field name.
35. **Pressure gradient**: an imposed gradient accelerates a periodic domain and
    drives a symmetric Poiseuille-like profile; non-finite values fail
    validation.
36. **Buoyancy**: a buoyancy section requires `scalar` and finite settings; a hot
    bottom drives upward flow.
37. **Rotated rectangles**: the four rotated corners lie inside the rotated
    rectangle, a point inside the unrotated box but outside the rotated one is
    excluded, a rotated corner outside the domain fails validation, and a
    rotated bar marks a diagonal band of solid cells.
38. **k–ε turbulence**: the model transports finite positive `k` and `ε`, reports
    `turbulenceModel = "kEpsilon"`, positive means and eddy viscosity, and writes
    both fields to CSV; non-positive initial values and a field export without
    the model fail.
39. **Scalar sources**: a source raises the field only inside its region
    (`0.2` at the centre after ten 0.01 s steps at rate 2), reports id
    `source-1`, one cell and the applied rate; bad geometry, a negative ramp and
    a source without scalar transport fail with `scalar/invalid`.
40. **Transient inlets**: time series interpolate linearly and clamp at both
    ends; duplicate times, mismatched lengths and empty series fail with
    `transient/invalid`; a driven inlet reports its history.
41. **Reynolds number**: the report resolves the speed and length sources
    (`reference`, `inlet`, `default`; `reference`, `obstacle`, `domainHeight`).

Unit tests in `projects/crates/cfd-model/src/solver.rs` cover the pieces that
are easier to assert in isolation:

- `buoyancy_acceleration_points_away_from_gravity_when_hot`
- `quick_and_central_are_exact_for_quadratic_profiles`
- `quick_falls_back_to_upwind_next_to_the_boundary`
- `probe_samples_the_nearest_cell`
- `probe_bilinear_interpolation_reproduces_a_linear_field` — a probe inside a
  linear field reproduces the analytic value exactly through the bilinear stencil
- `pressure_coefficient_uses_the_gauge_datum_and_reference_speed`
- `force_decomposition_separates_pressure_and_shear` — a stagnant non-uniform
  pressure gives non-zero pressure parts and zero viscous parts; a uniform shear
  with zero pressure gives the mirror result; both satisfy
  `fx = fxPressure + fxViscous` and `fy = fyPressure + fyViscous`
- `probe_inside_a_solid_cell_is_rejected`
- `wall_shear_reproduces_a_linear_tangential_profile`
- `wall_function_solves_the_log_law`
- `wall_function_keeps_the_viscous_sublayer_below_the_crossover`

`projects/crates/cfd-model/tests/session.rs` covers the full session API:
`create → mesh/fluid/time/boundary set → validate → solve → CSV/VTK/SVG/
streamline/residual/force export`, comparing each item with the report and the
`ocfd` CLI output, and round-trips slip walls, outlet pressure, reference
quantities, inlet profiles, time schemes, adaptive settings, porous regions and
the wall function through both entry points.

Node-API and TypeScript checks:

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

- Two dimensions only; no three-dimensional or axisymmetric support.
- Single-phase, incompressible, constant-density, constant-viscosity Newtonian
  flow. Turbulence is limited to a constant eddy viscosity, a mixing length and
  standard k–ε; there is no Reynolds-stress, LES or wall-modelled momentum
  treatment, so high-Reynolds-number results remain an approximation.
- Uniform structured Cartesian meshes only; no body-fitted meshes, adaptive
  refinement, moving meshes or immersed boundaries. Obstacles and porous regions
  are stair-stepped by cell centre. Because the mesh is uniform, `meshQuality`
  is constant over every fluid cell: it is measured, not assumed, but cannot
  diagnose a graded mesh the solver does not support.
- Explicit time stepping is limited by CFL and diffusion numbers; finer meshes
  and higher viscosity require a smaller `dt`. Without adaptivity a violation
  raises an error; with `time.adaptive` every step is clamped instead.
  `stability` only reports the limits: it never changes `time.dt`, enables
  adaptivity or advances the solution.
- Adaptive stepping changes the physical time reached for a fixed step count,
  does not adapt the mesh, and uses the velocity at the start of the step, so a
  strong transient can briefly exceed the target before the next step clamps.
- The default upwind plus explicit Euler is first-order in space and time with
  significant numerical diffusion. `central` and `quick` can oscillate above a
  cell Péclet number of about 2. `rk2` reduces temporal error but keeps the same
  pressure projection and CFL limits, so it does not permit a larger `dt`.
- Inlet profiles shape only the normal velocity of an `inlet` side; the
  tangential component stays uniform, and walls, slip sides and outlets ignore
  the profile.
- Porous regions are isotropic and fixed in time: `K` and `C_F` are scalar
  constants, corners are stair-stepped, and the Darcy sink is explicit, so
  `(ν/K + C_F|U|/√K)·dt` must stay at or below one.
- The wall function changes only the reported wall shear and friction; momentum
  keeps the no-slip ghost, so it is a reporting model rather than a fully
  wall-modelled boundary layer.
- The imposed pressure gradient is uniform in space and constant in time; it is
  not a body-force field or a time-varying forcing.
- Passive scalar transport supports Boussinesq buoyancy through the `buoyancy`
  section, which requires `scalar` and validates `beta`, `tRef` and both gravity
  components as finite.
- The stream function is integrated along one path from the bottom-left corner,
  so in a domain cut by obstacles it is a gauge-fixed diagnostic, not a unique
  potential.
- Point probes interpolate bilinearly from up to four surrounding fluid cell
  centres, clamped at the domain edge. Wall shear uses molecular viscosity with
  a first-order half-cell gradient, or the optional log-law report.
- Boundary handling is first-order: tangential wall velocity uses a half-cell
  ghost, slip walls mirror the ghost, outlets use zero-gradient velocity with the
  prescribed Dirichlet pressure, convective outlets relax with the local outflow
  speed, and periodic sides wrap the stencils.
- The `outlets` allocation is measured from the final velocity field. A
  recirculating outlet can carry a negative flow and share, so shares sum to one
  only when every outlet discharges; it is a diagnostic, not a control target.
- Obstacle moments use the geometric centroid as a fixed pivot and a first-order
  face pressure/shear distribution.
- The pressure coefficient uses gauge datum `pRef = 0` and the same reference
  speed as the force coefficients; it is not a compressible `cp` based on a
  freestream static pressure. A zero resolved speed defines the field as zero.
- Streamlines and contours are visual diagnostics with a fixed step cap, seed
  grid and nine-band palette. Particle trajectories are tracked separately from
  the solved field with a frozen velocity field and a second-order midpoint
  rule, so they are path lines of the final state rather than coupled particles.
- No parallel execution and no external format (Abaqus, OpenFOAM or similar)
  import or export.
- No graphical interface; use ParaView to open exported VTK.
- The TypeScript SDK and the `open-cfd` CLI expose mesh, fluid, time,
  boundaries, obstacles, porous regions, probes, scalar transport and sources,
  transient inlets, turbulence, buoyancy, references, solve and export. Thermal
  (`heat`) settings, wall-function settings, obstacle removal and obstacle
  rotation have no SDK method or `open-cfd` flag: author them in the `.ocfd`
  document or use the Rust `ocfd` binary.
