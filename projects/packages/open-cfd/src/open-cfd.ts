/**
 * `new OpenCFD(options?)` session.
 *
 * The constructor is synchronous and opens nothing: no network, no disk scan,
 * no file. Native initialisation and imports happen inside the async methods,
 * so a failure is always awaited and always carries a stable code.
 *
 * `create()` and `open()` return a {@link CfdCase}; the case owns the native
 * handle and exposes editing, validation, solving, export, save and close.
 */

import { CfdCase } from './document.js';
import { OpenCFDError } from './errors.js';
import * as service from './service.js';
import type { OpenCfdCapabilities, OpenCfdOptions } from './types.js';

/** CFD session. One instance owns the cases it opens. */
export class OpenCFD {
    /** BCP-47 tag used for diagnostics and default copy. */
    readonly locale: string;
    private readonly cases = new Set<CfdCase>();
    private closed = false;

    constructor(options: OpenCfdOptions = {}) {
        this.locale = options.locale ?? 'en-US';
    }

    /** Whether {@link dispose} has run. */
    get disposed(): boolean {
        return this.closed;
    }

    /**
     * Solver and IO disclosure for the installed engine. Read this instead of
     * treating a method name as a capability.
     */
    async capabilities(): Promise<OpenCfdCapabilities> {
        this.assertActive();
        return service.capabilities();
    }

    /** Creates the default cavity case at `path`. */
    async create(path: string): Promise<CfdCase> {
        this.assertActive();
        assertPath(path);
        const created = service.create(path);
        return this.track(new CfdCase(created.handle, created.document));
    }

    /** Opens an existing `.ocfd` document. */
    async open(path: string): Promise<CfdCase> {
        this.assertActive();
        assertPath(path);
        const opened = service.open(path);
        return this.track(new CfdCase(opened.handle, opened.document));
    }

    /** Releases every case the session still owns. Idempotent. */
    async dispose(): Promise<void> {
        if (this.closed) {
            return;
        }
        this.closed = true;
        for (const document of [...this.cases]) {
            document.close();
        }
        this.cases.clear();
    }

    private track(document: CfdCase): CfdCase {
        this.cases.add(document);
        return document;
    }

    private assertActive(): void {
        if (this.closed) {
            throw new OpenCFDError(
                'session/disposed',
                'the OpenCFD session has been disposed; create a new instance',
            );
        }
    }
}

function assertPath(path: unknown): asserts path is string {
    if (typeof path !== 'string' || path.length === 0) {
        throw new OpenCFDError('session/argument-invalid', '`path` must be a non-empty string');
    }
}
