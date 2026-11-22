/**
 * Orthogonal native operations.
 *
 * One function performs one operation and returns data; argument parsing, help
 * text and exit codes belong to `cli.ts` and to the session surface. Nothing
 * here knows about a command line. Every function forwards to the optional
 * native addon and decodes one JSON string.
 */

import { NotImplementedError, OpenCFDError, errorCodes, type OpenCFDErrorCode } from './errors.js';
import { nativeBinding } from './native.js';
import type {
    Boundaries,
    BoundaryProfileSetRequest,
    BoundarySetRequest,
    BuoyancySetRequest,
    CaseSummary,
    ConvectionSetRequest,
    ExportField,
    ExportFormat,
    ExportLayout,
    ExportReport,
    FluidSetRequest,
    MeshQuality,
    MeshSetRequest,
    Obstacle,
    OpenCfdCapabilities,
    PorousRegion,
    Probe,
    ProfileLine,
    PressureGradientSetRequest,
    ReferenceSetRequest,
    ReynoldsNumber,
    SaveReport,
    ScalarSetRequest,
    ScalarSource,
    Side,
    SolveOptions,
    SolveReport,
    StabilityLimits,
    TimeAdaptiveSetRequest,
    TimeSchemeSetRequest,
    TimeSetRequest,
    TransientInlet,
    TurbulenceSetRequest,
    ThermalSetRequest,
    WallFunctionSetRequest,
} from './types.js';

/** Result of creating or opening a case. */
export interface CaseHandle {
    readonly handle: number;
    readonly document: CaseSummary;
}

const knownCodes = new Set<string>(errorCodes);

function binding(feature: string) {
    const native = nativeBinding();
    if (!native) {
        throw new NotImplementedError(feature);
    }
    return native;
}

function decode<T>(work: () => string): T {
    try {
        return JSON.parse(work()) as T;
    } catch (error) {
        throw translate(error);
    }
}

function translate(error: unknown): OpenCFDError {
    if (error instanceof OpenCFDError) {
        return error;
    }
    const message = error instanceof Error ? error.message : String(error);
    const separator = message.indexOf(': ');
    const code = separator > 0 ? message.slice(0, separator) : '';
    if (knownCodes.has(code)) {
        return new OpenCFDError(code as OpenCFDErrorCode, message.slice(separator + 2), {
            details: { feature: code },
        });
    }
    return new OpenCFDError('cfd/unknown', message, { cause: error });
}

/** Reads the capability matrix of the installed addon. */
export function capabilities(): OpenCfdCapabilities {
    return decode<OpenCfdCapabilities>(() => binding('capabilities').capabilities());
}

/** Creates the default cavity case at `path` and returns its handle. */
export function create(path: string): CaseHandle {
    return decode<CaseHandle>(() => binding('create').create(path));
}

/** Opens an existing `.ocfd` document and returns its handle. */
export function open(path: string): CaseHandle {
    return decode<CaseHandle>(() => binding('open').open(path));
}

/** Validates and writes one case, optionally to a new path. */
export function save(handle: number, target?: string): SaveReport {
    return decode<SaveReport>(() =>
        target === undefined
            ? binding('save').save(handle)
            : binding('save').save(handle, target),
    );
}

/** Sets the structured mesh and writes the document back. */
export function meshSet(handle: number, request: MeshSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('mesh.set').mesh_set(
            handle,
            request.nx,
            request.ny,
            request.lengthX,
            request.lengthY,
        ),
    );
}

/** Sets density and dynamic viscosity and writes the document back. */
export function fluidSet(handle: number, request: FluidSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('fluid.set').fluid_set(handle, request.density, request.viscosity),
    );
}

/** Sets time integration settings and writes the document back. */
export function timeSet(handle: number, request: TimeSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('time.set').time_set(
            handle,
            request.dt,
            request.steps,
            request.pressureIterations,
            request.tolerance,
        ),
    );
}

/** Sets the explicit time integration scheme and writes the document back. */
export function timeSchemeSet(
    handle: number,
    request: TimeSchemeSetRequest,
): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('time.scheme.set').time_scheme_set(handle, request.scheme),
    );
}

/** Enables or disables adaptive time stepping and writes the document back. */
export function timeAdaptiveSet(
    handle: number,
    request: TimeAdaptiveSetRequest,
): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('time.adaptive.set').time_adaptive_set(
            handle,
            request.enabled,
            request.targetCfl,
        ),
    );
}

/** Sets one boundary and writes the document back. */
export function boundarySet(handle: number, request: BoundarySetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('boundary.set').boundary_set(
            handle,
            request.side,
            request.kind,
            request.u,
            request.v,
            request.pressure,
        ),
    );
}

/** Returns the four side boundary conditions and inlet profiles. */
export function boundaryList(handle: number): Boundaries {
    return decode<Boundaries>(() => binding('boundary.list').boundary_list(handle));
}

/** Resets one side or every side to a stationary wall and writes the document back. */
export function boundaryReset(handle: number, side?: Side): CaseSummary {
    return decode<CaseSummary>(() =>
        side === undefined
            ? binding('boundary.reset').boundary_reset(handle)
            : binding('boundary.reset').boundary_reset(handle, side),
    );
}

/** Sets the normal-velocity profile of one inlet side and writes the document back. */
export function boundaryProfileSet(
    handle: number,
    request: BoundaryProfileSetRequest,
): CaseSummary {
    return decode<CaseSummary>(() =>
        request.exponent === undefined
            ? binding('boundary.profile.set').boundary_profile_set(
                  handle,
                  request.side,
                  request.profile,
              )
            : binding('boundary.profile.set').boundary_profile_set(
                  handle,
                  request.side,
                  request.profile,
                  request.exponent,
              ),
    );
}

/** Appends one solid obstacle and writes the document back. */
export function obstacleAdd(handle: number, obstacle: Obstacle): CaseSummary {    return decode<CaseSummary>(() =>
        binding('obstacle.add').obstacle_add(handle, JSON.stringify(obstacle)),
    );
}

/** Replaces one solid obstacle in place and writes the document back. */
export function obstacleUpdate(
    handle: number,
    id: string,
    obstacle: Obstacle,
): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('obstacle.update').obstacle_update(handle, id, JSON.stringify(obstacle)),
    );
}

/** Returns the solid obstacles in document order. */
export function obstacleList(handle: number): readonly Obstacle[] {
    return decode<readonly Obstacle[]>(() => binding('obstacle.list').obstacle_list(handle));
}

/** Appends one porous region and writes the document back. */
export function porousAdd(handle: number, region: PorousRegion): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('porous.add').porous_add(handle, JSON.stringify(region)),
    );
}

/** Returns the porous regions in document order. */
export function porousList(handle: number): readonly PorousRegion[] {
    return decode<readonly PorousRegion[]>(() =>
        binding('porous.list').porous_list(handle),
    );
}

/** Removes one porous region and writes the document back. */
export function porousRemove(handle: number, id: string): CaseSummary {
    return decode<CaseSummary>(() => binding('porous.remove').porous_remove(handle, id));
}

/** Appends one point monitor and writes the document back. */
export function probeAdd(handle: number, probe: Probe): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('probe.add').probe_add(handle, JSON.stringify(probe)),
    );
}

/** Moves one point monitor and writes the document back. */
export function probeUpdate(handle: number, id: string, x: number, y: number): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('probe.update').probe_update(handle, id, x, y),
    );
}

/** Removes one point monitor and writes the document back. */
export function probeRemove(handle: number, id: string): CaseSummary {
    return decode<CaseSummary>(() => binding('probe.remove').probe_remove(handle, id));
}

/** Returns the point monitors in document order. */
export function probeList(handle: number): readonly Probe[] {
    return decode<readonly Probe[]>(() => binding('probe.list').probe_list(handle));
}

/** Removes every point monitor and writes the document back. */
export function probeClear(handle: number): CaseSummary {
    return decode<CaseSummary>(() => binding('probe.clear').probe_clear(handle));
}

/** Appends one volumetric scalar source and writes the document back. */
export function scalarSourceAdd(handle: number, source: ScalarSource): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('scalar.source.add').scalar_source_add(handle, JSON.stringify(source)),
    );
}

/** Returns the volumetric scalar sources in document order. */
export function scalarSourceList(handle: number): readonly ScalarSource[] {
    return decode<readonly ScalarSource[]>(() =>
        binding('scalar.source.list').scalar_source_list(handle),
    );
}

/** Removes every volumetric scalar source and writes the document back. */
export function scalarSourceClear(handle: number): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('scalar.source.clear').scalar_source_clear(handle),
    );
}

/** Adds or replaces one transient inlet time series and writes the document back. */
export function transientInletSet(handle: number, inlet: TransientInlet): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('inlet.series.set').transient_inlet_set(handle, JSON.stringify(inlet)),
    );
}

/** Returns the transient inlet time series in document order. */
export function transientInletList(handle: number): readonly TransientInlet[] {
    return decode<readonly TransientInlet[]>(() =>
        binding('inlet.series.list').transient_inlet_list(handle),
    );
}

/** Removes every transient inlet time series and writes the document back. */
export function transientInletClear(handle: number): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('inlet.series.clear').transient_inlet_clear(handle),
    );
}

/** Configures passive scalar transport and writes the document back. */
export function scalarSet(handle: number, request: ScalarSetRequest): CaseSummary {    return decode<CaseSummary>(() =>
        binding('scalar.set').scalar_set(
            handle,
            request.diffusivity,
            request.initial,
            request.inlet,
        ),
    );
}

/** Configures the eddy-viscosity turbulence model and writes the document back. */
export function turbulenceSet(handle: number, request: TurbulenceSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('turbulence.set').turbulence_set(
            handle,
            request.model,
            request.nuT ?? 0,
            request.mixingLength ?? 0.1,
            request.k,
            request.epsilon,
        ),
    );
}

/** Configures Boussinesq buoyancy and writes the document back. */
export function buoyancySet(handle: number, request: BuoyancySetRequest): CaseSummary {    return decode<CaseSummary>(() =>
        binding('buoyancy.set').buoyancy_set(
            handle,
            request.beta,
            request.tRef,
            request.gx,
            request.gy,
        ),
    );
}

/** Configures thermal boundaries and source and writes the document back. */
export function thermalSet(handle: number, request: ThermalSetRequest): CaseSummary {
    const { diffusivity, ...thermal } = request;
    return decode<CaseSummary>(() =>
        diffusivity === undefined
            ? binding('thermal.set').thermal_set(handle, JSON.stringify(thermal))
            : binding('thermal.set').thermal_set(handle, JSON.stringify(thermal), diffusivity),
    );
}

/** Configures the log-law wall function and writes the document back. */
export function wallFunctionSet(
    handle: number,
    request: WallFunctionSetRequest,
): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('wall.function.set').wall_function_set(
            handle,
            request.kappa,
            request.intercept,
            request.minYPlus,
        ),
    );
}

/** Removes the wall function and writes the document back. */
export function wallFunctionClear(handle: number): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('wall.function.clear').wall_function_clear(handle),
    );
}

/** Selects the convective discretisation and writes the document back. */
export function convectionSet(handle: number, request: ConvectionSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('convection.set').convection_set(handle, request.scheme),
    );
}

/** Sets the uniform imposed pressure gradient and writes the document back. */
export function pressureGradientSet(
    handle: number,
    request: PressureGradientSetRequest,
): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('pressure.gradient.set').pressure_gradient_set(
            handle,
            request.dpdx,
            request.dpdy,
        ),
    );
}

/** Sets force and moment reference quantities and writes the document back. */
export function referenceSet(handle: number, request: ReferenceSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('reference.set').reference_set(
            handle,
            request.velocity,
            request.area,
            request.length,
        ),
    );
}

/** Validates one case and returns its summary. */
export function validate(handle: number): CaseSummary {
    return decode<CaseSummary>(() => binding('validate').validate(handle));
}

/** Measures the mesh quality of one case without solving it. */
export function meshQuality(handle: number): MeshQuality {
    return decode<MeshQuality>(() => binding('mesh.quality').mesh_quality(handle));
}

/** Reports the derived Reynolds number of one case without solving it. */
export function reynolds(handle: number): ReynoldsNumber {
    return decode<ReynoldsNumber>(() => binding('reynolds').reynolds(handle));
}

/** Reports the explicit time-step stability limits without solving. */
export function stability(handle: number): StabilityLimits {
    return decode<StabilityLimits>(() => binding('stability').stability(handle));
}

/** Solves one case and returns the report. */
export function solve(handle: number, options?: SolveOptions): SolveReport {
    return decode<SolveReport>(() =>
        options === undefined
            ? binding('solve').solve(handle)
            : binding('solve').solve(
                  handle,
                  options.steps,
                  options.tolerance,
                  options.checkEvery,
                  options.restart,
                  options.checkpoint,
              ),
    );
}

/** Solves one case and writes a CSV, VTK or SVG field. */
export function exportCase(
    handle: number,
    format: ExportFormat,
    target: string,
    field?: ExportField,
    layout?: ExportLayout,
): ExportReport {
    return decode<ExportReport>(() =>
        binding('export').export(handle, format, target, field, layout),
    );
}

/** Solves one case and samples a field along a straight segment. */
export function exportProfileCase(
    handle: number,
    target: string,
    line: ProfileLine,
    samples: number,
    field: ExportField,
): ExportReport {
    return decode<ExportReport>(() =>
        binding('export_profile').export_profile(
            handle,
            target,
            line.x0,
            line.y0,
            line.x1,
            line.y1,
            samples,
            field,
        ),
    );
}

/** Solves one case and writes per-field statistics as JSON. */
export function exportStatisticsCase(handle: number, target: string): ExportReport {
    return decode<ExportReport>(() => binding('export_statistics').export_statistics(handle, target));
}

/** Releases one case handle. */
export function close(handle: number): void {
    binding('close').close(handle);
}
