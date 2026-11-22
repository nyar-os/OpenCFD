/**
 * Handle-backed CFD case document.
 *
 * `OpenCFD.create()` and `OpenCFD.open()` return one of these. The summary
 * stays in sync with every mutation, and every method forwards to the native
 * session through the handle it owns. Edit operations validate and atomically
 * write the file back before returning.
 */

import { OpenCFDError } from './errors.js';
import * as service from './service.js';
import type {
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
    PorousRegion,
    Probe,
    ProfileExportRequest,
    PressureGradientSetRequest,
    Boundaries,
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

/** One open case. Owns exactly one native session handle. */
export class CfdCase {
    /** Native handle of this case. */
    readonly handle: number;
    private state: CaseSummary;
    private closed = false;

    constructor(handle: number, summary: CaseSummary) {
        this.handle = handle;
        this.state = summary;
    }

    /** Document identity; currently the file path. */
    get id(): string {
        return this.state.id;
    }

    /** Latest summary returned by the native session. */
    get summary(): CaseSummary {
        return this.state;
    }

    /** Whether {@link close} has run. */
    get isClosed(): boolean {
        return this.closed;
    }

    /** Sets the mesh and writes the document back. */
    setMesh(request: MeshSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.meshSet(this.handle, request));
    }

    /** Sets fluid properties and writes the document back. */
    setFluid(request: FluidSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.fluidSet(this.handle, request));
    }

    /** Sets time integration settings and writes the document back. */
    setTime(request: TimeSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.timeSet(this.handle, request));
    }

    /** Sets the explicit time integration scheme and writes the document back. */
    setTimeScheme(request: TimeSchemeSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.timeSchemeSet(this.handle, request));
    }

    /** Enables or disables adaptive time stepping and writes the document back. */
    setTimeAdaptive(request: TimeAdaptiveSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.timeAdaptiveSet(this.handle, request));
    }

    /** Sets one boundary and writes the document back. */
    setBoundary(request: BoundarySetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.boundarySet(this.handle, request));
    }

    /** Returns the four side boundary conditions and inlet profiles. */
    listBoundaries(): Boundaries {
        this.assertOpen();
        return service.boundaryList(this.handle);
    }

    /** Resets one side, or every side, to a stationary wall and writes it back. */
    resetBoundary(side?: Side): CaseSummary {
        this.assertOpen();
        return this.adopt(service.boundaryReset(this.handle, side));
    }

    /** Sets one inlet's normal-velocity profile and writes the document back. */
    setInletProfile(request: BoundaryProfileSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.boundaryProfileSet(this.handle, request));
    }

    /** Solid obstacles in document order. */
    get obstacles(): readonly Obstacle[] {
        return this.state.obstacles;
    }

    /** Appends one solid obstacle and writes the document back. */
    addObstacle(obstacle: Obstacle): CaseSummary {
        this.assertOpen();
        return this.adopt(service.obstacleAdd(this.handle, obstacle));
    }

    /** Replaces one solid obstacle in place, keeping its id and position order. */
    updateObstacle(id: string, obstacle: Obstacle): CaseSummary {
        this.assertOpen();
        return this.adopt(service.obstacleUpdate(this.handle, id, obstacle));
    }

    /** Returns the solid obstacles in document order. */
    listObstacles(): readonly Obstacle[] {
        this.assertOpen();
        return service.obstacleList(this.handle);
    }

    /** Porous regions in document order. */
    get porous(): readonly PorousRegion[] {
        return this.state.porous;
    }

    /** Appends one porous region and writes the document back. */
    addPorous(region: PorousRegion): CaseSummary {
        this.assertOpen();
        return this.adopt(service.porousAdd(this.handle, region));
    }

    /** Returns the porous regions in document order. */
    listPorous(): readonly PorousRegion[] {
        this.assertOpen();
        return service.porousList(this.handle);
    }

    /** Removes the first porous region with `id` and writes the document back. */
    removePorous(id: string): CaseSummary {
        this.assertOpen();
        return this.adopt(service.porousRemove(this.handle, id));
    }

    /** Point monitors in document order. */
    get probes(): readonly Probe[] {
        return this.state.probes;
    }

    /** Appends one point monitor and writes the document back. */
    addProbe(probe: Probe): CaseSummary {
        this.assertOpen();
        return this.adopt(service.probeAdd(this.handle, probe));
    }

    /** Moves one point monitor and writes the document back. */
    updateProbe(id: string, x: number, y: number): CaseSummary {
        this.assertOpen();
        return this.adopt(service.probeUpdate(this.handle, id, x, y));
    }

    /** Removes one point monitor and writes the document back. */
    removeProbe(id: string): CaseSummary {
        this.assertOpen();
        return this.adopt(service.probeRemove(this.handle, id));
    }

    /** Returns the point monitors in document order. */
    listProbes(): readonly Probe[] {
        this.assertOpen();
        return service.probeList(this.handle);
    }

    /** Removes every point monitor and writes the document back. */
    clearProbes(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.probeClear(this.handle));
    }

    /** Selects the convective discretisation and writes the document back. */
    setConvection(request: ConvectionSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.convectionSet(this.handle, request));
    }

    /** Sets the uniform imposed pressure gradient and writes the document back. */
    setPressureGradient(request: PressureGradientSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.pressureGradientSet(this.handle, request));
    }

    /** Sets force and moment reference quantities and writes the document back. */
    setReference(request: ReferenceSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.referenceSet(this.handle, request));
    }

    /** Configures passive scalar transport and writes the document back. */
    setScalar(request: ScalarSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.scalarSet(this.handle, request));
    }

    /** Adds one volumetric scalar source and writes the document back. */
    addScalarSource(source: ScalarSource): CaseSummary {
        this.assertOpen();
        return this.adopt(service.scalarSourceAdd(this.handle, source));
    }

    /** Returns the volumetric scalar sources in document order. */
    listScalarSources(): readonly ScalarSource[] {
        this.assertOpen();
        return service.scalarSourceList(this.handle);
    }

    /** Removes every volumetric scalar source and writes the document back. */
    clearScalarSources(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.scalarSourceClear(this.handle));
    }

    /** Adds or replaces one transient inlet time series and writes the document back. */
    setTransientInlet(inlet: TransientInlet): CaseSummary {
        this.assertOpen();
        return this.adopt(service.transientInletSet(this.handle, inlet));
    }

    /** Returns the transient inlet time series in document order. */
    listTransientInlets(): readonly TransientInlet[] {
        this.assertOpen();
        return service.transientInletList(this.handle);
    }

    /** Removes every transient inlet time series and writes the document back. */
    clearTransientInlets(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.transientInletClear(this.handle));
    }

    /** Configures the eddy-viscosity turbulence model and writes the document back. */
    setTurbulence(request: TurbulenceSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.turbulenceSet(this.handle, request));
    }

    /** Configures Boussinesq buoyancy and writes the document back. */
    setBuoyancy(request: BuoyancySetRequest): CaseSummary {        this.assertOpen();
        return this.adopt(service.buoyancySet(this.handle, request));
    }

    /** Configures thermal boundaries and source and writes the document back. */
    setThermal(request: ThermalSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.thermalSet(this.handle, request));
    }

    /** Configures the log-law wall function and writes the document back. */
    setWallFunction(request: WallFunctionSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.wallFunctionSet(this.handle, request));
    }

    /** Removes the wall function and writes the document back. */
    clearWallFunction(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.wallFunctionClear(this.handle));
    }

    /** Validates the document and returns its summary. */
    validate(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.validate(this.handle));
    }

    /** Measures the mesh quality of this case without solving it. */
    meshQuality(): MeshQuality {
        this.assertOpen();
        return service.meshQuality(this.handle);
    }

    /** Reports the derived Reynolds number of this case without solving it. */
    reynolds(): ReynoldsNumber {
        this.assertOpen();
        return service.reynolds(this.handle);
    }

    /** Reports the explicit time-step stability limits without solving. */
    stability(): StabilityLimits {
        this.assertOpen();
        return service.stability(this.handle);
    }

    /** Solves the document and returns the report. */
    solve(options?: SolveOptions): SolveReport {
        this.assertOpen();
        return service.solve(this.handle, options);
    }

    /**
     * Solves and exports the field as CSV, VTK or an SVG preview.
     *
     * `layout` selects cell centres (default) or mesh nodes for CSV and VTK.
     */
    export(
        format: ExportFormat,
        target: string,
        field?: ExportField,
        layout?: ExportLayout,
    ): ExportReport {
        this.assertOpen();
        return service.exportCase(this.handle, format, target, field, layout);
    }

    /**
     * Solves and samples one field along a straight segment.
     *
     * The segment is sampled at `samples` equally spaced points (default 101)
     * and the field defaults to `speed`. Values come from the fluid cell that
     * contains each point; samples inside solids or outside the domain are
     * written as empty CSV values.
     */
    exportProfile(target: string, request: ProfileExportRequest): ExportReport {
        this.assertOpen();
        return service.exportProfileCase(
            this.handle,
            target,
            request.line,
            request.samples ?? 101,
            request.field ?? 'speed',
        );
    }

    /** Solves and writes deterministic per-field statistics as JSON. */
    exportStatistics(target: string): ExportReport {
        this.assertOpen();
        return service.exportStatisticsCase(this.handle, target);
    }

    /** Writes the document, optionally to a new path. */
    save(target?: string): SaveReport {
        this.assertOpen();
        const report = service.save(this.handle, target);
        this.state = { ...this.state, id: report.path, path: report.path };
        return report;
    }

    /** Releases the native handle. Idempotent. */
    close(): void {
        if (this.closed) {
            return;
        }
        this.closed = true;
        try {
            service.close(this.handle);
        } catch {
            // Releasing must not mask the caller's outcome; the handle is gone.
        }
    }

    private adopt(summary: CaseSummary): CaseSummary {
        this.state = summary;
        return summary;
    }

    private assertOpen(): void {
        if (this.closed) {
            throw new OpenCFDError('cfd/not-found', `case handle ${this.handle} has been closed`);
        }
    }
}
