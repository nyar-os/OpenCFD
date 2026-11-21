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
export type BoundaryKind = 'wall' | 'inlet' | 'outlet' | 'slip' | 'periodic';

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
    /** Gauge pressure prescribed on an outlet side in Pa; ignored otherwise. */
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

/** Axis-aligned rectangular obstacle. */
export interface RectObstacle {
    readonly kind: 'rect';
    /** Stable identifier; omitted means `obstacle-N` in document order. */
    readonly id?: string;
    readonly x0: number;
    readonly y0: number;
    readonly x1: number;
    readonly y1: number;
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

/** Eddy-viscosity turbulence model. */
export type TurbulenceModel = 'none' | 'constant' | 'mixingLength';

/** Convective discretisation of the momentum and scalar equations. */
export type ConvectionScheme = 'upwind' | 'central' | 'quick';

/** Eddy-viscosity turbulence settings. */
export interface Turbulence {
    readonly model: TurbulenceModel;
    /** Constant eddy viscosity in m²/s (used by the `constant` model). */
    readonly nuT: number;
    /** Mixing length in metres (used by the `mixingLength` model). */
    readonly mixingLength: number;
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

/** Surface heat-transfer summary of one obstacle. */
export interface ObstacleNusselt {
    readonly id: string;
    readonly kind: 'rect' | 'circle';
    readonly meanHeatFlux: number;
    readonly wallTemperature: number;
    readonly referenceTemperature: number;
    readonly nusselt: number;
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
    /** Point monitors in document order. */
    readonly probes: readonly Probe[];
    /** Passive scalar settings, when configured. */
    readonly scalar?: ScalarConfig;
    /** Eddy-viscosity turbulence settings, when configured. */
    readonly turbulence?: Turbulence;
    /** Thermal settings, when configured. */
    readonly thermal?: ThermalConfig;
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
    | 'forces';

/** Cell-centred field an SVG or contour export can visualise. */
export type ExportField =
    | 'pressure'
    | 'temperature'
    | 'vorticity'
    | 'streamFunction'
    | 'speed';

/** Result of one export. */
export interface ExportReport {
    readonly format: ExportFormat;
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
    /** Friction coefficient `meanShear / (0.5 ρ U²)`. */
    readonly frictionCoefficient: number;
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
    /** Net mass flow through the domain boundary. */
    readonly massBalance: MassBalance;
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
    /** Point monitor time series in document order. */
    readonly probes: readonly ProbeSeries[];
    /** Wall shear and friction summary of every wall side. */
    readonly wallShear: readonly WallShear[];
    /** Convection scheme used by the run. */
    readonly convection: string;
    /** Time integration scheme used by the run. */
    readonly timeScheme: TimeScheme;
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
