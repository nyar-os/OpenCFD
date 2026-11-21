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
    BoundarySetRequest,
    CaseSummary,
    ExportFormat,
    ExportReport,
    FluidSetRequest,
    MeshSetRequest,
    Obstacle,
    SaveReport,
    ScalarSetRequest,
    SolveOptions,
    SolveReport,
    TimeSetRequest,
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

    /** Sets one boundary and writes the document back. */
    setBoundary(request: BoundarySetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.boundarySet(this.handle, request));
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

    /** Returns the solid obstacles in document order. */
    listObstacles(): readonly Obstacle[] {
        this.assertOpen();
        return service.obstacleList(this.handle);
    }

    /** Configures passive scalar transport and writes the document back. */
    setScalar(request: ScalarSetRequest): CaseSummary {
        this.assertOpen();
        return this.adopt(service.scalarSet(this.handle, request));
    }

    /** Validates the document and returns its summary. */
    validate(): CaseSummary {
        this.assertOpen();
        return this.adopt(service.validate(this.handle));
    }

    /** Solves the document and returns the report. */
    solve(options?: SolveOptions): SolveReport {
        this.assertOpen();
        return service.solve(this.handle, options);
    }

    /** Solves and exports the field as CSV or VTK. */
    export(format: ExportFormat, target: string): ExportReport {
        this.assertOpen();
        return service.exportCase(this.handle, format, target);
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
