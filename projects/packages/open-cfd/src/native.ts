/**
 * Native addon discovery.
 *
 * The addon only maps values: document editing, validation, solving, export
 * and file IO live in the shared Rust `cfd-model` crate. An absent addon is a
 * declarative state, so calls report `capability/not-implemented` instead of
 * inventing a result.
 */

import { createRequire } from 'node:module';

const requireNative = createRequire(import.meta.url);

/** Orthogonal exports of the built addon. Every operation returns a JSON string. */
export interface NativeBinding {
    version(): string;
    capabilities(): string;
    create(path: string): string;
    open(path: string): string;
    save(handle: number, target?: string): string;
    mesh_set(handle: number, nx: number, ny: number, lengthX: number, lengthY: number): string;
    fluid_set(
        handle: number,
        density: number,
        viscosity: number,
        viscosityModel?: string,
        consistency?: number,
        power?: number,
        minViscosity?: number,
        maxViscosity?: number,
    ): string;
    time_set(handle: number, dt: number, steps: number, pressureIterations: number, tolerance?: number): string;
    time_scheme_set(handle: number, scheme: string): string;
    time_adaptive_set(handle: number, enabled: boolean, targetCfl?: number): string;
    boundary_set(handle: number, side: string, kind: string, u?: number, v?: number, pressure?: number): string;
    boundary_list(handle: number): string;
    boundary_reset(handle: number, side?: string): string;
    boundary_profile_set(handle: number, side: string, profile: string, exponent?: number): string;
    obstacle_add(handle: number, requestJson: string): string;
    obstacle_update(handle: number, id: string, requestJson: string): string;
    obstacle_remove(handle: number, id: string): string;
    obstacle_list(handle: number): string;
    porous_add(handle: number, requestJson: string): string;
    porous_list(handle: number): string;
    porous_remove(handle: number, id: string): string;
    probe_add(handle: number, requestJson: string): string;
    probe_update(handle: number, id: string, x: number, y: number): string;
    probe_remove(handle: number, id: string): string;
    probe_list(handle: number): string;
    probe_clear(handle: number): string;
    scalar_source_add(handle: number, requestJson: string): string;
    scalar_source_list(handle: number): string;
    scalar_source_clear(handle: number): string;
    transient_inlet_set(handle: number, requestJson: string): string;
    transient_inlet_list(handle: number): string;
    transient_inlet_clear(handle: number): string;
    scalar_set(handle: number, diffusivity: number, initial?: number, inlet?: number): string;
    turbulence_set(
        handle: number,
        model: string,
        nuT: number,
        mixingLength: number,
        k?: number,
        epsilon?: number,
    ): string;
    buoyancy_set(handle: number, beta: number, tRef: number, gx: number, gy: number): string;
    thermal_set(handle: number, requestJson: string, diffusivity?: number): string;
    wall_function_set(
        handle: number,
        kappa?: number,
        intercept?: number,
        minYPlus?: number,
    ): string;
    wall_function_clear(handle: number): string;
    convection_set(handle: number, scheme: string): string;
    pressure_gradient_set(handle: number, dpdx: number, dpdy: number): string;
    reference_set(handle: number, velocity?: number, area?: number, length?: number): string;
    validate(handle: number): string;
    mesh_quality(handle: number): string;
    reynolds(handle: number): string;
    stability(handle: number): string;
    solve(
        handle: number,
        steps?: number,
        tolerance?: number,
        checkEvery?: number,
        restart?: string,
        checkpoint?: string,
    ): string;
    export(handle: number, format: string, out: string, field?: string, layout?: string): string;
    export_profile(
        handle: number,
        out: string,
        x0: number,
        y0: number,
        x1: number,
        y1: number,
        samples: number,
        field?: string,
    ): string;
    export_statistics(handle: number, out: string): string;
    close(handle: number): boolean;
}

let cached: NativeBinding | null | undefined;

/** Loads the addon once, or returns `undefined` when it is not installed. */
export function nativeBinding(): NativeBinding | undefined {
    if (cached === undefined) {
        cached = load();
    }
    return cached ?? undefined;
}

function load(): NativeBinding | null {
    for (const candidate of candidates()) {
        try {
            return requireNative(candidate) as NativeBinding;
        } catch (error) {
            if (candidate === explicitPath()) {
                throw error;
            }
            if (!isModuleNotFound(error, candidate)) {
                throw error;
            }
        }
    }
    return null;
}

function candidates(): string[] {
    const explicit = explicitPath();
    if (explicit) {
        return [explicit];
    }
    return [`@wae/open-cfd-${process.platform}-${process.arch}`];
}

function explicitPath(): string | undefined {
    const value = process.env.OPEN_CFD_NATIVE;
    return value !== undefined && value.length > 0 ? value : undefined;
}

function isModuleNotFound(error: unknown, candidate: string): boolean {
    if (typeof error !== 'object' || error === null) {
        return false;
    }
    const code = (error as { code?: unknown }).code;
    // Node prints the resolved path, which uses the platform separator on
    // Windows. Normalize it so a linked platform package without a built
    // addon is still treated as "not installed" instead of a fatal error.
    const message = (error as Error).message.replace(/\\/g, '/');
    return code === 'MODULE_NOT_FOUND' && message.includes(candidate);
}
