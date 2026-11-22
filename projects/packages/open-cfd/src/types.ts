/**
 * Declared types of `@wae/open-cfd`.
 *
 * Shapes mirror the JSON the native session returns. Every report names the
 * file it belongs to, so a caller never has to remember a path separately.
 */

/** Disclosure level of one capability entry. */
export type SupportLevel = 'supported' | 'unsupported';

/** Matrix returned by `capabilities()`. */
export interface OpenCfdCapabilities {
    readonly solver: Readonly<Record<string, SupportLevel>>;
    readonly io: Readonly<Record<string, SupportLevel>>;
}

/** Session options. Constructing a session opens no file and no network. */
export interface OpenCfdOptions {
    /** BCP-47 tag used for diagnostics and default copy. Defaults to `en-US`. */
    readonly locale?: string;
}

/** Uniform structured Cartesian mesh. */
export interface MeshConfig {
    readonly nx: number;
    readonly ny: number;
    readonly lengthX: number;
    readonly lengthY: number;
}

/** Constant-property Newtonian fluid. */
export interface Fluid {
    readonly density: number;
    readonly viscosity: number;
}

/** Force and moment reference quantities; omitted values stay automatic. */
export interface Reference {
    /** Reference speed in m/s; default is the maximum inlet speed. */
    readonly velocity?: number;
    /** Reference area in m²; default is the obstacle length times unit depth. */
    readonly area?: number;
    /** Reference length in m; default is the obstacle characteristic length. */
    readonly length?: number;
}

/** Explicit time integration scheme of the momentum equation. */
export type TimeScheme = 'euler' | 'rk2';

/** Time integration settings. */
export interface TimeSettings {
    readonly dt: number;
    readonly steps: number;
    readonly pressureIterations: number;
    readonly tolerance: number;
    /** Explicit time integration scheme. */
    readonly scheme: TimeScheme;
    /** Whether every step is reduced to satisfy the explicit stability limits. */
    readonly adaptive: boolean;
    /** Target convective CFL number of an adaptive step, in `(0, 1]`. */
    readonly targetCfl: number;
}

/** Boundary kind of one side. */
export type BoundaryKind = 'wall' | 'inlet' | 'outlet' | 'convective' | 'slip' | 'periodic';

/** Normal-velocity shape of an inlet side. */
export type InletProfile = 'uniform' | 'parabolic' | 'powerLaw';

/** Per-side inlet velocity profile; only applies to `inlet` sides. */
export interface InletProfiles {
    readonly left: InletProfile;
    readonly right: InletProfile;
    readonly top: InletProfile;
    readonly bottom: InletProfile;
    /** Power-law exponent `n`; must be finite and positive. */
    readonly exponent: number;
}

/** Domain side name. */
export type Side = 'left' | 'right' | 'top' | 'bottom';

/** Boundary condition of one side. */
export interface Boundary {
    readonly kind: BoundaryKind;
    readonly velocity: readonly [number, number];
    /** Gauge pressure prescribed on an outlet or convective-outlet side in Pa; ignored otherwise. */
    readonly pressure: number;
}

/** Boundary conditions of the four sides. */
export interface Boundaries {
    readonly left: Boundary;
    readonly right: Boundary;
    readonly top: Boundary;
    readonly bottom: Boundary;
    /** Normal-velocity shapes of the four inlet sides. */
    readonly inletProfiles: InletProfiles;
}

/** Optional uniform initial velocity. */
export interface InitialField {
    readonly velocity: readonly [number, number];
}

/** Rectangle obstacle, optionally rotated counter-clockwise about its centroid. */
export interface RectObstacle {
    readonly kind: 'rect';
    /** Stable identifier; omitted means `obstacle-N` in document order. */
    readonly id?: string;
    readonly x0: number;
    readonly y0: number;
    readonly x1: number;
    readonly y1: number;
    /** Counter-clockwise rotation about the centroid in degrees; default 0. */
    readonly rotation?: number;
}

/** Circular obstacle. */
export interface CircleObstacle {
    readonly kind: 'circle';
    /** Stable identifier; omitted means `obstacle-N` in document order. */
    readonly id?: string;
    readonly x: number;
    readonly y: number;
    readonly r: number;
}

/** Solid obstacle accepted by `CfdCase.addObstacle()`. */
export type Obstacle = RectObstacle | CircleObstacle;

/** Point monitor sampled during a solve. */
export interface Probe {
    /** Stable identifier; omitted means `probe-N` in document order. */
    readonly id?: string;
    readonly x: number;
    readonly y: number;
}

/** Passive scalar (temperature) transport settings. */
export interface ScalarConfig {
    /** Constant diffusivity in m²/s; zero means pure advection. */
    readonly diffusivity: number;
    /** Uniform initial value in every fluid cell. */
    readonly initial: number;
    /** Value prescribed on inlet faces. */
    readonly inletValue: number;
}

/** Piecewise linear time series; sampled and clamped at both ends. */
export interface TimeSeries {
    /** Strictly increasing sample times in seconds. */
    readonly times: readonly number[];
    /** Values at `times`. */
    readonly values: readonly number[];
}

/** Time-series drive of one domain side. */
export interface TransientInlet {
    readonly side: Side;
    /** X velocity series in m/s. */
    readonly u?: TimeSeries;
    /** Y velocity series in m/s. */
    readonly v?: TimeSeries;
    /** Outlet reference pressure series in Pa. */
    readonly pressure?: TimeSeries;
}

/** One sampled transient inlet value. */
export interface InletSample {
    readonly step: number;
    readonly time: number;
    readonly u: number;
    readonly v: number;
    readonly pressure: number;
}

/** Sampled history of one transient inlet side. */
export interface InletHistory {
    readonly side: Side;
    readonly history: readonly InletSample[];
}

/** Volumetric scalar source or sink over an axis-aligned rectangle. */
export interface ScalarSource {
    /** Stable identifier; omitted means `source-N` in document order. */
    readonly id?: string;
    readonly x0: number;
    readonly y0: number;
    readonly x1: number;
    readonly y1: number;
    /** Volumetric scalar rate per second; negative is a sink. */
    readonly rate: number;
    /** Start time in seconds; defaults to `0`. */
    readonly start?: number;
    /** Linear ramp duration in seconds; `0` means a step source. */
    readonly ramp?: number;
}

/** Per-region scalar source summary. */
export interface ScalarSourceSummary {
    readonly id: string;
    /** Number of fluid cells inside the region. */
    readonly cells: number;
    /** Configured volumetric scalar rate per second. */
    readonly rate: number;
    /** Rate applied at the final simulated time. */
    readonly appliedRate: number;
}

/** Eddy-viscosity turbulence model. */
export type TurbulenceModel = 'none' | 'constant' | 'mixingLength' | 'kEpsilon';

/** Convective discretisation of the momentum and scalar equations. */
export type ConvectionScheme = 'upwind' | 'central' | 'quick';

/** Eddy-viscosity turbulence settings. */
export interface Turbulence {
    readonly model: TurbulenceModel;
    /** Constant eddy viscosity in m²/s (used by the `constant` model). */
    readonly nuT: number;
    /** Mixing length in metres (used by the `mixingLength` model). */
    readonly mixingLength: number;
    /** Initial turbulent kinetic energy in m²/s² (used by `kEpsilon`). */
    readonly k: number;
    /** Initial turbulent dissipation rate in m²/s³ (used by `kEpsilon`). */
    readonly epsilon: number;
}

/** Thermal condition of one domain side. */
export interface SideThermal {
    /** Fixed wall temperature in K; takes precedence over `heatFlux`. */
    readonly temperature?: number;
    /** Fixed heat flux into the fluid in W/m². */
    readonly heatFlux?: number;
}

/** Energy-equation settings layered on passive scalar transport. */
export interface ThermalConfig {
    /** Specific heat capacity in J/(kg·K). */
    readonly specificHeat: number;
    /** Thermal conductivity in W/(m·K); omitted derives `rho·cp·diffusivity`. */
    readonly conductivity?: number;
    /** Uniform volumetric heat source in W/m³. */
    readonly heatSource: number;
    readonly left: SideThermal;
    readonly right: SideThermal;
    readonly top: SideThermal;
    readonly bottom: SideThermal;
    /** Fixed temperature of every obstacle surface in K. */
    readonly obstacleTemperature?: number;
    /** Heat flux from every obstacle surface into the fluid in W/m². */
    readonly obstacleHeatFlux?: number;
    /** Reference temperature for the Nusselt number in K. */
    readonly referenceTemperature?: number;
}

/**
 * Boussinesq buoyancy coupling to the scalar temperature field. Requires
 * `scalar` transport; the momentum equation gains `−β·(T − tRef)·g` per unit
 * mass.
 */
export interface Buoyancy {
    /** Thermal expansion coefficient in 1/K. */
    readonly beta: number;
    /** Reference temperature in K where the density variation vanishes. */
    readonly tRef: number;
    /** Gravity vector `[gx, gy]` in m/s², typically `[0, -9.81]`. */
    readonly gravity: readonly [number, number];
}

/**
 * Logarithmic wall function for the reported wall shear. The friction velocity
 * solves `u⁺ = (1/κ)·ln(y⁺) + B`; below `minYPlus` the viscous sublayer value
 * `τ = ρ·ν·Δu/y` is used.
 */
export interface WallFunction {
    /** von Kármán constant κ; finite and positive. */
    readonly kappa: number;
    /** Log-law additive constant B. */
    readonly intercept: number;
    /** `y⁺` below which the viscous sublayer estimate is used. */
    readonly minYPlus: number;
}

/** Surface heat-transfer summary of one obstacle. */
export interface ObstacleNusselt {
    readonly id: string;
    readonly kind: 'rect' | 'circle';
    readonly meanHeatFlux: number;
    readonly wallTemperature: number;
    readonly referenceTemperature: number;
    readonly nusselt: number;
}

/** Isotropic porous region with a Darcy–Forchheimer resistance. */
export interface PorousRegion {
    /** Stable identifier; omitted means `porous-N` in document order. */
    readonly id?: string;
    readonly x0: number;
    readonly y0: number;
    readonly x1: number;
    readonly y1: number;
    /** Permeability K in m²; must be positive. */
    readonly permeability: number;
    /** Forchheimer inertial coefficient C_F; non-negative, `0` by default. */
    readonly inertial: number;
}

/** Per-region porous resistance summary. */
export interface PorousSummary {
    readonly id: string;
    /** Number of fluid cells owned by the region. */
    readonly cells: number;
    /** Mean cell speed inside the region in m/s. */
    readonly meanSpeed: number;
    /** Mean drag acceleration magnitude inside the region in m/s². */
    readonly meanDrag: number;
}

/** Snapshot of one open case document. */
export interface CaseSummary {
    /** Stable document identity; currently the file path. */
    readonly id: string;
    /** File the document is bound to. */
    readonly path: string;
    readonly format: string;
    readonly version: number;
    readonly mesh: MeshConfig;
    readonly fluid: Fluid;
    /** Force and moment reference quantities, when configured. */
    readonly reference?: Reference;
    /** Convective discretisation of the momentum and scalar equations. */
    readonly convection: ConvectionScheme;
    /** Uniform imposed pressure gradient `[dpdx, dpdy]` in Pa/m, when set. */
    readonly pressureGradient?: readonly [number, number];
    readonly time: TimeSettings;
    readonly boundaries: Boundaries;
    readonly initial?: InitialField;
    /** Solid obstacles in document order. */
    readonly obstacles: readonly Obstacle[];
    /** Porous regions in document order. */
    readonly porous: readonly PorousRegion[];
    /** Point monitors in document order. */
    readonly probes: readonly Probe[];
    /** Volumetric scalar sources in document order. */
    readonly scalarSources: readonly ScalarSource[];
    /** Time-series boundary drives in document order. */
    readonly transientInlets: readonly TransientInlet[];
    /** Passive scalar settings, when configured. */
    readonly scalar?: ScalarConfig;
    /** Eddy-viscosity turbulence settings, when configured. */
    readonly turbulence?: Turbulence;
    /** Thermal settings, when configured. */
    readonly thermal?: ThermalConfig;
    /** Boussinesq buoyancy settings, when configured. */
    readonly buoyancy?: Buoyancy;
    /** Log-law wall-function settings, when configured. */
    readonly wallFunction?: WallFunction;
}

/** Result of writing a document back to disk. */
export interface SaveReport {
    readonly path: string;
    readonly bytes: number;
}

/** Exchange formats the engine can write. */
export type ExportFormat =
    | 'csv'
    | 'vtk'
    | 'svg'
    | 'contours'
    | 'streamlines'
    | 'residuals'
    | 'forces'
    | 'coefficients'
    | 'probes'
    | 'particles';

/** Cell-centred field an SVG or contour export can visualise. */
export type ExportField =
    | 'pressure'
    | 'temperature'
    | 'vorticity'
    | 'streamFunction'
    | 'speed'
    | 'turbulentKineticEnergy'
    | 'turbulentDissipation';

/**
 * Sampling layout of a CSV or VTK field export. `node` averages the adjacent
 * fluid cells onto the `(nx + 1) × (ny + 1)` mesh vertices.
 */
export type ExportLayout = 'cell' | 'node';

/** Result of one export. */
export interface ExportReport {
    readonly format: ExportFormat;
    /** Sampling layout of a CSV or VTK export; other formats report `cell`. */
    readonly layout: ExportLayout;
    readonly path: string;
}

/** One sampled velocity residual. */
export interface ResidualSample {
    readonly step: number;
    readonly residual: number;
    /** RMS of the discrete divergence after the step in 1/s. */
    readonly continuity: number;
}

/** Net mass flow through the domain boundary in kg/s. */
export interface MassBalance {
    readonly inflow: number;
    readonly outflow: number;
    /** Signed net outward mass flow, `outflow - inflow`. */
    readonly net: number;
    /** `|net| / max(inflow, outflow)`; zero when no flow crosses the boundary. */
    readonly relativeError: number;
}

/** Outward mass flow through one outlet side. */
export interface OutletFlow {
    readonly side: Side;
    /** Signed outward mass flow in kg/s; positive when the side discharges. */
    readonly flow: number;
    /** Share of the total positive outlet flow; negative for recirculation. */
    readonly share: number;
    /** Signed normal velocity averaged over the side in m/s. */
    readonly meanVelocity: number;
    /** Side length in m. */
    readonly length: number;
}

/** One sampled obstacle force. */
export interface ForceSample {
    readonly step: number;
    readonly fx: number;
    readonly fy: number;
    /** Moment about the obstacle centroid in N·m, positive counter-clockwise. */
    readonly mz: number;
    readonly cd: number;
    readonly cl: number;
    /** Moment coefficient, positive counter-clockwise. */
    readonly cm: number;
}

/** Force report of one obstacle. */
export interface ObstacleForce {
    readonly id: string;
    readonly kind: 'rect' | 'circle';
    readonly fx: number;
    readonly fy: number;
    /** Moment about the obstacle centroid in N·m, positive counter-clockwise. */
    readonly mz: number;
    readonly cd: number;
    readonly cl: number;
    readonly cm: number;
    /** Reference speed used by the coefficients in m/s. */
    readonly referenceVelocity: number;
    /** Reference area used by the force coefficients in m². */
    readonly referenceArea: number;
    /** Reference length used by the moment coefficient in m. */
    readonly referenceLength: number;
    readonly history: readonly ForceSample[];
}

/** One sampled probe reading. */
export interface ProbeSample {
    readonly step: number;
    /** Physical time of the sample in seconds. */
    readonly time: number;
    readonly u: number;
    readonly v: number;
    /** Speed in m/s. */
    readonly speed: number;
    /** Cell-centred gauge pressure in Pa. */
    readonly pressure: number;
    /** Temperature in K, or `null` when no scalar is configured. */
    readonly temperature: number | null;
}

/** Time series recorded at one point monitor. */
export interface ProbeSeries {
    readonly id: string;
    readonly x: number;
    readonly y: number;
    /** Nearest cell `[i, j]`. */
    readonly cell: readonly [number, number];
    readonly history: readonly ProbeSample[];
}

/** Wall shear and friction summary of one wall side. */
export interface WallShear {
    readonly side: Side;
    /** Length-averaged wall shear stress in Pa. */
    readonly meanShear: number;
    /** Largest absolute wall shear stress along the side in Pa. */
    readonly maxShear: number;
    /** Length-averaged `y⁺` of the wall-adjacent cells. */
    readonly meanYPlus: number;
    /** Friction coefficient `meanShear / (0.5 ρ U²)`. */
    readonly frictionCoefficient: number;
}

/** Mesh quality metrics measured from the structured mesh. */
export interface MeshQuality {
    readonly cells: number;
    readonly fluidCells: number;
    readonly dx: number;
    readonly dy: number;
    readonly minAspectRatio: number;
    readonly meanAspectRatio: number;
    readonly maxAspectRatio: number;
    /** Mean non-orthogonality angle in degrees. */
    readonly meanNonOrthogonality: number;
    /** Largest non-orthogonality angle in degrees. */
    readonly maxNonOrthogonality: number;
    readonly meanSkewness: number;
    readonly maxSkewness: number;
}

/** Derived Reynolds number, resolved from the fluid, reference and geometry. */
export interface ReynoldsNumber {
    readonly density: number;
    readonly dynamicViscosity: number;
    /** `mu / rho` in m²/s, or `null` for an inviscid case. */
    readonly kinematicViscosity: number | null;
    /** Reference speed in m/s. */
    readonly velocity: number;
    /** Reference length in m. */
    readonly length: number;
    /** `rho·U·L/mu`, or `null` for an inviscid case. */
    readonly reynolds: number | null;
    /** Where the speed came from. */
    readonly velocitySource: 'reference' | 'inlet' | 'default';
    /** Where the length came from. */
    readonly lengthSource: 'reference' | 'obstacle' | 'domainHeight';
}

/** Solve-time overrides accepted by `CfdCase.solve()`. */
export interface SolveOptions {
    /** Steps to advance; overrides `time.steps`. */
    readonly steps?: number;
    /** Steady-state tolerance on the L2 velocity change; enables early stop. */
    readonly tolerance?: number;
    /** Sample and test the steady-state residual every this many steps. */
    readonly checkEvery?: number;
    /** Checkpoint file to resume from before advancing. */
    readonly restart?: string;
    /** Path that receives the final state as a restart checkpoint. */
    readonly checkpoint?: string;
}

/** Report returned by `solve()`. */
export interface SolveReport {
    readonly format: string;
    readonly version: number;
    readonly steps: number;
    /** Time step of the last advanced step. */
    readonly dt: number;
    /** Whether adaptive time stepping reduced the configured upper bound. */
    readonly adaptive: boolean;
    /** Mean time step over the advanced steps. */
    readonly averageDt: number;
    /** Smallest time step used. */
    readonly minDt: number;
    /** Largest time step used. */
    readonly maxDt: number;
    /** Simulated physical time, the sum of the advanced steps. */
    readonly time: number;
    readonly maxVelocity: number;
    readonly minPressure: number;
    readonly maxPressure: number;
    readonly divergenceL2: number;
    /** Quality metrics of the mesh the case was solved on. */
    readonly meshQuality: MeshQuality;
    /** Net mass flow through the domain boundary. */
    readonly massBalance: MassBalance;
    /** Per-outlet signed mass flow and share, in side order. */
    readonly outlets: readonly OutletFlow[];
    /** Minimum vorticity over fluid cells in 1/s. */
    readonly vorticityMin: number;
    /** Maximum vorticity over fluid cells in 1/s. */
    readonly vorticityMax: number;
    /** Minimum stream function over fluid cells in m²/s. */
    readonly streamFunctionMin: number;
    /** Maximum stream function over fluid cells in m²/s. */
    readonly streamFunctionMax: number;
    /**
     * Run convergence: velocity convergence when a tolerance was supplied,
     * otherwise pressure-solve convergence.
     */
    readonly converged: boolean;
    /** Whether the velocity change fell below the requested tolerance. */
    readonly steadyState: boolean;
    /** Final L2 norm of the per-step velocity change. */
    readonly residual: number;
    readonly residualHistory: readonly ResidualSample[];
    /** Why the run stopped: `maxSteps` or `steadyState`. */
    readonly terminationReason: string;
    readonly pressureConverged: boolean;
    readonly pressureIterations: number;
    readonly pressureResidual: number;
    readonly pressureResidualInitial: number;
    readonly forces: readonly ObstacleForce[];
    /** Porous resistance summary in document order. */
    readonly porous: readonly PorousSummary[];
    /** Scalar source summary in document order. */
    readonly scalarSources: readonly ScalarSourceSummary[];
    /** Transient inlet histories in document order. */
    readonly inletHistory: readonly InletHistory[];
    /** Point monitor time series in document order. */
    readonly probes: readonly ProbeSeries[];
    /** Wall shear and friction summary of every wall side. */
    readonly wallShear: readonly WallShear[];
    /** Convection scheme used by the run. */
    readonly convection: string;
    /** Time integration scheme used by the run. */
    readonly timeScheme: TimeScheme;
    /** Turbulence model used by the run. */
    readonly turbulenceModel: TurbulenceModel;
    /** Minimum eddy viscosity over fluid cells in m²/s. */
    readonly eddyViscosityMin: number;
    /** Maximum eddy viscosity over fluid cells in m²/s. */
    readonly eddyViscosityMax: number;
    /** Mean eddy viscosity over fluid cells in m²/s. */
    readonly eddyViscosityMean: number;
    /** Minimum turbulent kinetic energy over fluid cells in m²/s². */
    readonly turbulentKineticEnergyMin: number;
    /** Maximum turbulent kinetic energy over fluid cells in m²/s². */
    readonly turbulentKineticEnergyMax: number;
    /** Mean turbulent kinetic energy over fluid cells in m²/s². */
    readonly turbulentKineticEnergyMean: number;
    /** Minimum turbulent dissipation rate over fluid cells in m²/s³. */
    readonly turbulentDissipationMin: number;
    /** Maximum turbulent dissipation rate over fluid cells in m²/s³. */
    readonly turbulentDissipationMax: number;
    /** Mean turbulent dissipation rate over fluid cells in m²/s³. */
    readonly turbulentDissipationMean: number;
    /** Whether Boussinesq buoyancy coupled temperature to momentum. */
    readonly buoyancy: boolean;
    /** Whether a log-law wall function evaluated the reported wall shear. */
    readonly wallFunction: boolean;
}

/** Arguments of `CfdCase.setMesh()`. */
export interface MeshSetRequest {
    readonly nx: number;
    readonly ny: number;
    readonly lengthX: number;
    readonly lengthY: number;
}

/** Arguments of `CfdCase.setFluid()`. */
export interface FluidSetRequest {
    readonly density: number;
    readonly viscosity: number;
}

/** Arguments of `CfdCase.setTime()`. */
export interface TimeSetRequest {
    readonly dt: number;
    readonly steps: number;
    readonly pressureIterations: number;
    /** Relative pressure residual tolerance; omitted keeps the current value. */
    readonly tolerance?: number;
}

/** Arguments of `CfdCase.setTimeScheme()`. */
export interface TimeSchemeSetRequest {
    readonly scheme: TimeScheme;
}

/** Arguments of `CfdCase.setTimeAdaptive()`. */
export interface TimeAdaptiveSetRequest {
    readonly enabled: boolean;
    /** Target CFL in `(0, 1]`; omitted keeps the current value. */
    readonly targetCfl?: number;
}

/** Arguments of `CfdCase.setBoundary()`. */
export interface BoundarySetRequest {
    readonly side: Side;
    readonly kind: BoundaryKind;
    /** Prescribed x velocity; omitted keeps the current value. */
    readonly u?: number;
    /** Prescribed y velocity; omitted keeps the current value. */
    readonly v?: number;
    /** Outlet reference pressure in Pa; omitted keeps the current value. */
    readonly pressure?: number;
}

/** Arguments of `CfdCase.setInletProfile()`. */
export interface BoundaryProfileSetRequest {
    readonly side: Side;
    readonly profile: InletProfile;
    /** Power-law exponent; omitted keeps the current value. */
    readonly exponent?: number;
}

/** Arguments of `CfdCase.setConvection()`. */
export interface ConvectionSetRequest {
    readonly scheme: ConvectionScheme;
}

/** Arguments of `CfdCase.setPressureGradient()`. */
export interface PressureGradientSetRequest {
    /** Imposed pressure gradient in x in Pa/m. */
    readonly dpdx: number;
    /** Imposed pressure gradient in y in Pa/m. */
    readonly dpdy: number;
}

/** Arguments of `CfdCase.setReference()`. */
export interface ReferenceSetRequest {
    /** Reference speed in m/s; omitted keeps the current value. */
    readonly velocity?: number;
    /** Reference area in m²; omitted keeps the current value. */
    readonly area?: number;
    /** Reference length in m; omitted keeps the current value. */
    readonly length?: number;
}

/** Arguments of `CfdCase.setScalar()`. */
export interface ScalarSetRequest {
    /** Constant diffusivity in m²/s. */
    readonly diffusivity: number;
    /** Uniform initial value; omitted keeps the current value. */
    readonly initial?: number;
    /** Inlet value; omitted keeps the current value. */
    readonly inlet?: number;
}

/** Arguments of `CfdCase.setTurbulence()`. */
export interface TurbulenceSetRequest {
    /** Selected model; `none` removes the turbulence section. */
    readonly model: TurbulenceModel;
    /** Eddy viscosity in m²/s for the `constant` model. */
    readonly nuT?: number;
    /** Mixing length in metres for the `mixingLength` model. */
    readonly mixingLength?: number;
    /** Initial turbulent kinetic energy in m²/s² for `kEpsilon`. */
    readonly k?: number;
    /** Initial turbulent dissipation rate in m²/s³ for `kEpsilon`. */
    readonly epsilon?: number;
}

/** Arguments of `CfdCase.setBuoyancy()`. */
export interface BuoyancySetRequest {    /** Thermal expansion coefficient in 1/K; `0` removes the buoyancy section. */
    readonly beta: number;
    /** Reference temperature in K. */
    readonly tRef: number;
    /** Gravity x component in m/s². */
    readonly gx: number;
    /** Gravity y component in m/s². */
    readonly gy: number;
}
