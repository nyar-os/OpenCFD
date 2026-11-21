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
    BoundaryProfileSetRequest,
    BoundarySetRequest,
    CaseSummary,
    ConvectionSetRequest,
    ExportField,
    ExportFormat,
    ExportReport,
    FluidSetRequest,
    MeshSetRequest,
    Obstacle,
    OpenCfdCapabilities,
    Probe,
    ReferenceSetRequest,
    SaveReport,
    ScalarSetRequest,
    SolveOptions,
    SolveReport,
    TimeSetRequest,
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

/** Returns the solid obstacles in document order. */
export function obstacleList(handle: number): readonly Obstacle[] {
    return decode<readonly Obstacle[]>(() => binding('obstacle.list').obstacle_list(handle));
}

/** Appends one point monitor and writes the document back. */
export function probeAdd(handle: number, probe: Probe): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('probe.add').probe_add(handle, JSON.stringify(probe)),
    );
}

/** Returns the point monitors in document order. */
export function probeList(handle: number): readonly Probe[] {
    return decode<readonly Probe[]>(() => binding('probe.list').probe_list(handle));
}

/** Removes every point monitor and writes the document back. */
export function probeClear(handle: number): CaseSummary {
    return decode<CaseSummary>(() => binding('probe.clear').probe_clear(handle));
}

/** Configures passive scalar transport and writes the document back. */
export function scalarSet(handle: number, request: ScalarSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('scalar.set').scalar_set(
            handle,
            request.diffusivity,
            request.initial,
            request.inlet,
        ),
    );
}

/** Selects the convective discretisation and writes the document back. */
export function convectionSet(handle: number, request: ConvectionSetRequest): CaseSummary {
    return decode<CaseSummary>(() =>
        binding('convection.set').convection_set(handle, request.scheme),
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
              ),
    );
}

/** Solves one case and writes a CSV, VTK or SVG field. */
export function exportCase(
    handle: number,
    format: ExportFormat,
    target: string,
    field?: ExportField,
): ExportReport {
    return decode<ExportReport>(() =>
        field === undefined
            ? binding('export').export(handle, format, target)
            : binding('export').export(handle, format, target, field),
    );
}

/** Releases one case handle. */
export function close(handle: number): void {
    binding('close').close(handle);
}
