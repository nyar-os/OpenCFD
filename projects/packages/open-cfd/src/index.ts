/**
 * CFD surface of `@wae/open-cfd`.
 *
 * The package wraps the native session core: create/open/save round trips,
 * mesh/fluid/time/boundary editing, solid obstacles with in-place replacement
 * and force coefficients, per-probe editing, passive scalar transport,
 * convergence detection, explicit stability limits, the Chorin projection solve
 * and CSV/VTK/SVG export (including vorticity and stream function fields) work
 * today. Buoyancy, compressible flow and three-dimensional grids are not
 * implemented and are reported as such by `capabilities()`.
 */

export { CfdCase } from './document.js';
export { OpenCFD } from './open-cfd.js';
export { NotImplementedError, OpenCFDError, errorCodes, isOpenCFDError } from './errors.js';
export type { OpenCFDErrorCode, OpenCFDErrorDetails, OpenCFDErrorOptions } from './errors.js';
export { capabilities } from './service.js';
export type * from './types.js';
