/**
 * Stable error surface of `@wae/open-cfd`.
 *
 * Codes are contract, not prose: callers compare them, localize around them and
 * never parse the message. The native layer prefixes every failure with its
 * stable code, so translation only splits on the first `: `.
 */

/** Every stable code the CFD surface may report. */
export const errorCodes = [
    'session/disposed',
    'session/argument-invalid',
    'session/handle-unknown',
    'cfd/not-found',
    'cfd/unknown',
    'document/no-path',
    'document/parse',
    'document/serialize',
    'document/format',
    'document/version',
    'file/exists',
    'io/read',
    'io/write',
    'mesh/invalid',
    'fluid/invalid',
    'time/invalid',
    'boundary/invalid',
    'initial/invalid',
    'obstacle/invalid',
    'scalar/invalid',
    'solver/invalid',
    'solver/unstable',
    'solver/non-finite',
    'capability/not-implemented',
    'cli/unknown-command',
    'cli/unknown-option',
    'cli/unknown-format',
    'cli/missing-argument',
    'cli/missing-option',
    'cli/missing-value',
    'cli/invalid-number',
    'cli/invalid-side',
    'cli/invalid-boundary-kind',
    'cli/invalid-obstacle-kind',
    'cli/invalid-convection-scheme',
] as const;

/** Union of {@link errorCodes}. */
export type OpenCFDErrorCode = (typeof errorCodes)[number];

/** Optional locators a failure can carry. */
export interface OpenCFDErrorDetails {
    /** Capability or field involved in the failure, e.g. `mesh.nx`. */
    readonly feature?: string;
}

/** Options accepted by {@link OpenCFDError}. */
export interface OpenCFDErrorOptions {
    readonly recoverable?: boolean;
    readonly details?: OpenCFDErrorDetails;
    readonly cause?: unknown;
}

/** Every failure thrown or rejected by this package. */
export class OpenCFDError extends Error {
    /** Stable, comparable code. */
    readonly code: OpenCFDErrorCode;
    /** Whether retrying with different input or state can succeed. */
    readonly recoverable: boolean;
    /** Optional locators (`feature`). */
    readonly details: OpenCFDErrorDetails;

    constructor(
        code: OpenCFDErrorCode,
        message: string,
        options: OpenCFDErrorOptions = {},
    ) {
        super(message, options.cause === undefined ? undefined : { cause: options.cause });
        this.name = 'OpenCFDError';
        this.code = code;
        this.recoverable = options.recoverable ?? false;
        this.details = Object.freeze({ ...options.details });
    }
}

/**
 * Reported when the optional native addon is absent or a declared route is not
 * implemented by the installed build.
 */
export class NotImplementedError extends OpenCFDError {
    constructor(feature: string, message = `\`${feature}\` is not implemented yet`) {
        super('capability/not-implemented', message, {
            recoverable: false,
            details: { feature },
        });
        this.name = 'NotImplementedError';
    }
}

/** Narrows an unknown thrown value to {@link OpenCFDError}. */
export function isOpenCFDError(value: unknown): value is OpenCFDError {
    return value instanceof OpenCFDError;
}
