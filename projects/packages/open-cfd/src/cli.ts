#!/usr/bin/env node
/**
 * Command-line assembly only.
 *
 * Every command maps options onto one operation of `OpenCFD`/`CfdCase`; the
 * solver, validation and atomic file writes stay native. The CLI owns argument
 * parsing, help text, printing and exit codes, and never reimplements physics.
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import { pathToFileURL } from 'node:url';

import { NotImplementedError, OpenCFDError, type OpenCFDErrorCode } from './errors.js';
import { OpenCFD } from './open-cfd.js';
import type {
    BoundaryKind,
    ConvectionScheme,
    ExportField,
    ExportFormat,
    Obstacle,
    Side,
} from './types.js';

const USAGE = `open-cfd - OpenCFD two-dimensional incompressible Navier-Stokes solver

usage:
  open-cfd create <file.ocfd>
  open-cfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
  open-cfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
  open-cfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
  open-cfd boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet [--u <m/s>] [--v <m/s>]
  open-cfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--id <name>]
  open-cfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]
  open-cfd obstacle list <file>
  open-cfd probe add <file> --x <m> --y <m> [--id <name>]
  open-cfd probe list <file>
  open-cfd probe clear <file>
  open-cfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]
  open-cfd convection set <file> --scheme upwind|central|quick
  open-cfd validate <file>
  open-cfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--out report.json]
  open-cfd export <file> --format csv|vtk|svg [--field pressure|temperature|vorticity|streamFunction] --out <dir|file>
  open-cfd capabilities
  open-cfd --help

exit codes: 0 success, 1 input/validation/IO error, 2 capability not implemented
`;

const SIDES: readonly Side[] = ['left', 'right', 'top', 'bottom'];
const BOUNDARY_KINDS: readonly BoundaryKind[] = ['wall', 'inlet', 'outlet'];
const CONVECTION_SCHEMES: readonly ConvectionScheme[] = ['upwind', 'central', 'quick'];

interface ParsedArguments {
    readonly positionals: readonly string[];
    readonly options: Readonly<Record<string, string>>;
}

function cliError(code: OpenCFDErrorCode, message: string): OpenCFDError {
    return new OpenCFDError(code, message);
}

function parseArguments(args: readonly string[], valueOptions: readonly string[]): ParsedArguments {
    const positionals: string[] = [];
    const options: Record<string, string> = {};
    let index = 0;
    while (index < args.length) {
        const argument = args[index];
        const equals = argument.indexOf('=');
        if (argument.startsWith('--') && equals > 2) {
            const name = argument.slice(0, equals);
            const value = argument.slice(equals + 1);
            if (!valueOptions.includes(name)) {
                throw cliError('cli/unknown-option', `unknown option \`${name}\``);
            }
            options[name] = value;
            index += 1;
            continue;
        }
        if (argument.startsWith('--')) {
            if (!valueOptions.includes(argument)) {
                throw cliError('cli/unknown-option', `unknown option \`${argument}\``);
            }
            const value = args[index + 1];
            if (value === undefined || value.startsWith('--')) {
                throw cliError('cli/missing-value', `\`${argument}\` needs a value`);
            }
            options[argument] = value;
            index += 2;
        } else {
            positionals.push(argument);
            index += 1;
        }
    }
    return { positionals, options };
}

function positional(parsed: ParsedArguments, index: number, usage: string): string {
    const value = parsed.positionals[index];
    if (value === undefined) {
        throw cliError('cli/missing-argument', `missing argument: ${usage}`);
    }
    return value;
}

function requiredOption(parsed: ParsedArguments, name: string): string {
    const value = parsed.options[name];
    if (value === undefined) {
        throw cliError('cli/missing-option', `missing required option \`${name}\``);
    }
    return value;
}

function numberOption(parsed: ParsedArguments, name: string): number {
    const raw = requiredOption(parsed, name);
    const value = Number(raw);
    if (!Number.isFinite(value)) {
        throw cliError('cli/invalid-number', `\`${name}\` value \`${raw}\` is not a number`);
    }
    return value;
}

function optionalNumber(parsed: ParsedArguments, name: string): number | undefined {
    return parsed.options[name] === undefined ? undefined : numberOption(parsed, name);
}

function integerOption(parsed: ParsedArguments, name: string): number {
    const value = numberOption(parsed, name);
    if (!Number.isInteger(value) || value < 0) {
        throw cliError('cli/invalid-number', `\`${name}\` must be a non-negative integer`);
    }
    return value;
}

function optionalInteger(parsed: ParsedArguments, name: string): number | undefined {
    return parsed.options[name] === undefined ? undefined : integerOption(parsed, name);
}

function writeAtomic(target: string, content: string): void {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const temporary = `${target}.${process.pid}.${Date.now()}.tmp`;
    fs.writeFileSync(temporary, content);
    if (fs.existsSync(target)) {
        fs.rmSync(target, { force: true });
    }
    fs.renameSync(temporary, target);
}

function parseSide(raw: string): Side {
    if (!SIDES.includes(raw as Side)) {
        throw cliError('cli/invalid-side', `unknown side \`${raw}\`; use left, right, top or bottom`);
    }
    return raw as Side;
}

function parseBoundaryKind(raw: string): BoundaryKind {
    if (!BOUNDARY_KINDS.includes(raw as BoundaryKind)) {
        throw cliError('cli/invalid-boundary-kind', `unknown boundary kind \`${raw}\`; use wall, inlet or outlet`);
    }
    return raw as BoundaryKind;
}

function parseConvectionScheme(raw: string): ConvectionScheme {
    if (!CONVECTION_SCHEMES.includes(raw as ConvectionScheme)) {
        throw cliError(
            'cli/invalid-convection-scheme',
            `unknown convection scheme \`${raw}\`; use upwind, central or quick`,
        );
    }
    return raw as ConvectionScheme;
}

function parseExportFormat(raw: string): ExportFormat {
    if (raw === 'inp' || raw === 'openfoam') {
        throw new NotImplementedError(`${raw} export`);
    }
    if (raw !== 'csv' && raw !== 'vtk' && raw !== 'svg') {
        throw cliError('cli/unknown-format', `unknown export format \`${raw}\`; use csv, vtk or svg`);
    }
    return raw;
}

const EXPORT_FIELDS: readonly ExportField[] = [
    'pressure',
    'temperature',
    'vorticity',
    'streamFunction',
];

function parseExportField(raw: string): ExportField {
    if (!EXPORT_FIELDS.includes(raw as ExportField)) {
        throw cliError(
            'cli/unknown-format',
            `unknown export field \`${raw}\`; use pressure, temperature, vorticity or streamFunction`,
        );
    }
    return raw as ExportField;
}

function parseObstacle(parsed: ParsedArguments): Obstacle {
    const kind = requiredOption(parsed, '--kind');
    const id = parsed.options['--id'];
    if (kind === 'rect') {
        return {
            kind: 'rect',
            ...(id === undefined ? {} : { id }),
            x0: numberOption(parsed, '--x0'),
            y0: numberOption(parsed, '--y0'),
            x1: numberOption(parsed, '--x1'),
            y1: numberOption(parsed, '--y1'),
        };
    }
    if (kind === 'circle') {
        return {
            kind: 'circle',
            ...(id === undefined ? {} : { id }),
            x: numberOption(parsed, '--x'),
            y: numberOption(parsed, '--y'),
            r: numberOption(parsed, '--r'),
        };
    }
    throw cliError('cli/invalid-obstacle-kind', `unknown obstacle kind \`${kind}\`; use rect or circle`);
}

async function dispatch(session: OpenCFD, argv: readonly string[]): Promise<number> {
    const command = argv[0];
    if (command === undefined) {
        process.stdout.write(USAGE);
        return 0;
    }
    const rest = argv.slice(1);
    switch (command) {
        case '-h':
        case '--help':
        case 'help':
            process.stdout.write(USAGE);
            return 0;
        case 'capabilities': {
            const matrix = await session.capabilities();
            console.log(JSON.stringify(matrix, null, 2));
            return 0;
        }
        case 'create': {
            const parsed = parseArguments(rest, []);
            const file = positional(parsed, 0, 'create <file.ocfd>');
            const document = await session.create(file);
            const summary = document.summary;
            console.log(
                `created ${summary.path}: ${summary.mesh.nx}x${summary.mesh.ny} mesh, lid 1 m/s, ${summary.time.steps} steps, dt=${summary.time.dt} s`,
            );
            return 0;
        }
        case 'mesh': {
            expectSet(rest, 'mesh');
            const parsed = parseArguments(rest.slice(1), ['--nx', '--ny', '--length-x', '--length-y']);
            const file = positional(parsed, 0, 'mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>');
            const document = await session.open(file);
            const summary = document.setMesh({
                nx: integerOption(parsed, '--nx'),
                ny: integerOption(parsed, '--ny'),
                lengthX: numberOption(parsed, '--length-x'),
                lengthY: numberOption(parsed, '--length-y'),
            });
            console.log(
                `mesh set: ${summary.mesh.nx}x${summary.mesh.ny}, ${summary.mesh.lengthX}x${summary.mesh.lengthY} m`,
            );
            return 0;
        }
        case 'fluid': {
            expectSet(rest, 'fluid');
            const parsed = parseArguments(rest.slice(1), ['--rho', '--mu']);
            const file = positional(parsed, 0, 'fluid set <file> --rho <kg/m3> --mu <Pa.s>');
            const document = await session.open(file);
            const summary = document.setFluid({
                density: numberOption(parsed, '--rho'),
                viscosity: numberOption(parsed, '--mu'),
            });
            console.log(
                `fluid set: rho=${summary.fluid.density} kg/m3, mu=${summary.fluid.viscosity} Pa.s`,
            );
            return 0;
        }
        case 'time': {
            expectSet(rest, 'time');
            const parsed = parseArguments(rest.slice(1), [
                '--dt',
                '--steps',
                '--pressure-iterations',
                '--tolerance',
            ]);
            const file = positional(
                parsed,
                0,
                'time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]',
            );
            const document = await session.open(file);
            const summary = document.setTime({
                dt: numberOption(parsed, '--dt'),
                steps: integerOption(parsed, '--steps'),
                pressureIterations: integerOption(parsed, '--pressure-iterations'),
                tolerance: optionalNumber(parsed, '--tolerance'),
            });
            console.log(
                `time set: dt=${summary.time.dt} s, steps=${summary.time.steps}, pressureIterations=${summary.time.pressureIterations}, relative tolerance=${summary.time.tolerance}`,
            );
            return 0;
        }
        case 'boundary': {
            expectSet(rest, 'boundary');
            const parsed = parseArguments(rest.slice(1), ['--side', '--kind', '--u', '--v']);
            const file = positional(
                parsed,
                0,
                'boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet [--u <m/s>] [--v <m/s>]',
            );
            const side = parseSide(requiredOption(parsed, '--side'));
            const kind = parseBoundaryKind(requiredOption(parsed, '--kind'));
            const document = await session.open(file);
            const summary = document.setBoundary({
                side,
                kind,
                u: optionalNumber(parsed, '--u'),
                v: optionalNumber(parsed, '--v'),
            });
            const boundary = summary.boundaries[side];
            console.log(
                `${side} boundary set to ${boundary.kind}, velocity=[${boundary.velocity[0]}, ${boundary.velocity[1]}] m/s`,
            );
            return 0;
        }
        case 'obstacle': {
            const action = rest[0];
            if (action === 'list') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'obstacle list <file>');
                const document = await session.open(file);
                console.log(JSON.stringify(document.listObstacles(), null, 2));
                return 0;
            }
            if (action !== 'add') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`obstacle` needs the `add` or `list` subcommand'
                        : `\`obstacle\` subcommand \`${action}\` does not exist; use add or list`,
                );
            }
            const parsed = parseArguments(rest.slice(1), [
                '--kind',
                '--id',
                '--x0',
                '--y0',
                '--x1',
                '--y1',
                '--x',
                '--y',
                '--r',
            ]);
            const file = positional(
                parsed,
                0,
                'obstacle add <file> --kind rect|circle ... [--id <name>]',
            );
            const document = await session.open(file);
            const summary = document.addObstacle(parseObstacle(parsed));
            const added = summary.obstacles[summary.obstacles.length - 1];
            console.log(
                `added ${added.kind} obstacle ${added.id ?? ''}; ${summary.obstacles.length} total`,
            );
            return 0;
        }
        case 'probe': {
            const action = rest[0];
            if (action === 'list') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'probe list <file>');
                const document = await session.open(file);
                console.log(JSON.stringify(document.listProbes(), null, 2));
                return 0;
            }
            if (action === 'clear') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'probe clear <file>');
                const document = await session.open(file);
                document.clearProbes();
                console.log('cleared all probes');
                return 0;
            }
            if (action !== 'add') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`probe` needs the `add`, `list` or `clear` subcommand'
                        : `\`probe\` subcommand \`${action}\` does not exist; use add, list or clear`,
                );
            }
            const parsed = parseArguments(rest.slice(1), ['--id', '--x', '--y']);
            const file = positional(
                parsed,
                0,
                'probe add <file> --x <m> --y <m> [--id <name>]',
            );
            const id = parsed.options['--id'];
            const document = await session.open(file);
            const summary = document.addProbe({
                ...(id === undefined ? {} : { id }),
                x: numberOption(parsed, '--x'),
                y: numberOption(parsed, '--y'),
            });
            const added = summary.probes[summary.probes.length - 1];
            console.log(
                `added probe ${added.id ?? ''} at (${added.x}, ${added.y}) m; ${summary.probes.length} total`,
            );
            return 0;
        }
        case 'scalar': {
            expectSet(rest, 'scalar');
            const parsed = parseArguments(rest.slice(1), [
                '--diffusivity',
                '--initial',
                '--inlet',
            ]);
            const file = positional(
                parsed,
                0,
                'scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]',
            );
            const document = await session.open(file);
            const summary = document.setScalar({
                diffusivity: numberOption(parsed, '--diffusivity'),
                initial: optionalNumber(parsed, '--initial'),
                inlet: optionalNumber(parsed, '--inlet'),
            });
            const scalar = summary.scalar;
            console.log(
                `scalar set: diffusivity=${scalar?.diffusivity} m2/s, initial=${scalar?.initial}, inletValue=${scalar?.inletValue}`,
            );
            return 0;
        }
        case 'convection': {
            expectSet(rest, 'convection');
            const parsed = parseArguments(rest.slice(1), ['--scheme']);
            const file = positional(
                parsed,
                0,
                'convection set <file> --scheme upwind|central|quick',
            );
            const scheme = parseConvectionScheme(requiredOption(parsed, '--scheme'));
            const document = await session.open(file);
            const summary = document.setConvection({ scheme });
            console.log(`convection set: ${summary.convection}`);
            return 0;
        }
        case 'validate': {
            const parsed = parseArguments(rest, []);
            const file = positional(parsed, 0, 'validate <file>');
            const document = await session.open(file);
            const summary = document.validate();
            console.log(
                `ok: ${summary.mesh.nx}x${summary.mesh.ny} mesh, ${summary.mesh.lengthX}x${summary.mesh.lengthY} m, rho=${summary.fluid.density} kg/m3, mu=${summary.fluid.viscosity} Pa.s, ${summary.time.steps} steps, dt=${summary.time.dt} s`,
            );
            return 0;
        }
        case 'solve': {
            const parsed = parseArguments(rest, [
                '--out',
                '--steps',
                '--tolerance',
                '--check-every',
            ]);
            const file = positional(
                parsed,
                0,
                'solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--out report.json]',
            );
            const steps = optionalInteger(parsed, '--steps');
            const tolerance = optionalNumber(parsed, '--tolerance');
            const checkEvery = optionalInteger(parsed, '--check-every');
            const document = await session.open(file);
            const report = document.solve({ steps, tolerance, checkEvery });
            const json = JSON.stringify(report, null, 2);
            const out = parsed.options['--out'];
            if (out !== undefined) {
                writeAtomic(out, json);
            }
            console.log(json);
            return 0;
        }
        case 'export': {
            const parsed = parseArguments(rest, ['--format', '--out', '--field']);
            const file = positional(
                parsed,
                0,
                'export <file> --format csv|vtk|svg [--field pressure|temperature|vorticity|streamFunction] --out <dir|file>',
            );
            const format = parseExportFormat(requiredOption(parsed, '--format'));
            const rawField = parsed.options['--field'];
            const field = rawField === undefined ? undefined : parseExportField(rawField);
            const out = requiredOption(parsed, '--out');
            const document = await session.open(file);
            const report = document.export(format, out, field);
            if (format === 'csv') {
                console.log(`exported CSV: ${report.path}`);
            } else if (format === 'vtk') {
                console.log(`exported legacy ASCII VTK: ${report.path}`);
            } else {
                console.log(`exported SVG (${field ?? 'pressure'}): ${report.path}`);
            }
            return 0;
        }
        case 'compressible':
            throw new NotImplementedError('compressible flow');
        case 'turbulence':
            throw new NotImplementedError('turbulence models');
        case 'heat':
        case 'thermal':
            throw new NotImplementedError('heat transfer');
        case '3d':
        case 'threeD':
            throw new NotImplementedError('three-dimensional flow');
        case 'import':
            throw new NotImplementedError('external format import');
        default:
            throw cliError('cli/unknown-command', `unknown command \`${command}\`; use --help`);
    }
}

function expectSet(rest: readonly string[], group: string): void {
    const action = rest[0];
    if (action === undefined) {
        throw cliError('cli/missing-argument', `\`${group}\` needs the \`set\` subcommand`);
    }
    if (action !== 'set') {
        throw cliError(
            'cli/unknown-command',
            `\`${group}\` subcommand \`${action}\` does not exist; only \`set\` is supported`,
        );
    }
}

function describeFailure(error: unknown): { code: string; message: string } {
    if (error instanceof OpenCFDError) {
        return { code: error.code, message: error.message };
    }
    return {
        code: 'cfd/unknown',
        message: error instanceof Error ? error.message : String(error),
    };
}

/** Runs one command line and returns the process exit code. */
export async function main(argv: readonly string[]): Promise<number> {
    const session = new OpenCFD();
    try {
        return await dispatch(session, argv);
    } catch (error) {
        const failure = describeFailure(error);
        console.error(JSON.stringify(failure, null, 2));
        return failure.code.startsWith('capability/') ? 2 : 1;
    } finally {
        await session.dispose();
    }
}

const entry = process.argv[1];
if (entry !== undefined && import.meta.url === pathToFileURL(entry).href) {
    process.exitCode = await main(process.argv.slice(2));
}
