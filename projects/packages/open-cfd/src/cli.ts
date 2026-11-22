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
    ExportLayout,
    InletProfile,
    Obstacle,
    ProfileLine,
    Side,
    SideThermal,
    TimeScheme,
    ViscosityModel,
} from './types.js';

const USAGE = `open-cfd - OpenCFD two-dimensional incompressible Navier-Stokes solver

usage:
  open-cfd create <file.ocfd>
  open-cfd mesh set <file> --nx <n> --ny <n> --length-x <m> --length-y <m>
  open-cfd fluid set <file> --rho <kg/m3> --mu <Pa.s>
  open-cfd fluid set <file> --viscosity-model power-law --consistency <K> --power <n> [--min-viscosity <Pa.s>] [--max-viscosity <Pa.s>]
  open-cfd time set <file> --dt <s> --steps <n> --pressure-iterations <n> [--tolerance <v>]
  open-cfd time scheme set <file> --scheme euler|rk2
  open-cfd time adaptive set <file> --cfl <n|off>
  open-cfd boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|convective|slip|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]
  open-cfd boundary list <file>
  open-cfd boundary reset <file> [--side left|right|top|bottom]
  open-cfd boundary profile set <file> --side left|right|top|bottom --profile uniform|parabolic|powerLaw [--exponent <n>]
  open-cfd obstacle add <file> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>] [--id <name>]
  open-cfd obstacle add <file> --kind circle --x <m> --y <m> --r <m> [--id <name>]
  open-cfd obstacle update <file> --id <name> --kind rect --x0 <m> --y0 <m> --x1 <m> --y1 <m> [--rotation <deg>]
  open-cfd obstacle update <file> --id <name> --kind circle --x <m> --y <m> --r <m>
  open-cfd obstacle remove <file> --id <name>
  open-cfd obstacle list <file>
  open-cfd porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --permeability <m2> [--inertial <n>] [--id <name>]
  open-cfd porous list <file>
  open-cfd porous remove <file> --id <name>
  open-cfd probe add <file> --x <m> --y <m> [--id <name>]
  open-cfd probe update <file> --id <name> --x <m> --y <m>
  open-cfd probe remove <file> --id <name>
  open-cfd probe list <file>
  open-cfd probe clear <file>
  open-cfd scalar set <file> --diffusivity <m2/s> [--initial <T>] [--inlet <T>]
  open-cfd thermal set <file> [--specific-heat <J/kgK>] [--conductivity <W/mK>] [--diffusivity <m2/s>] [--heat-source <W/m3>] [--left-temperature <K>] [--right-temperature <K>] [--top-temperature <K>] [--bottom-temperature <K>] [--left-heat-flux <W/m2>] [--right-heat-flux <W/m2>] [--top-heat-flux <W/m2>] [--bottom-heat-flux <W/m2>] [--obstacle-temperature <K>] [--obstacle-heat-flux <W/m2>] [--reference-temperature <K>]
  open-cfd wall-function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]
  open-cfd wall-function clear <file>
  open-cfd buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>
  open-cfd convection set <file> --scheme upwind|central|quick
  open-cfd pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>
  open-cfd reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]
  open-cfd validate <file>
  open-cfd quality <file>
  open-cfd reynolds <file>
  open-cfd stability <file>
  open-cfd solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]
  open-cfd export <file> --format csv|vtk|svg|contours|streamlines|residuals|forces|coefficients|probes|particles|profile|statistics [--field pressure|temperature|vorticity|streamFunction|speed|pressureCoefficient] [--layout cell|node] [--line <x0,y0,x1,y1>] [--samples <n>] --out <dir|file>
  open-cfd capabilities
  open-cfd --help

exit codes: 0 success, 1 input/validation/IO error, 2 capability not implemented
`;

const SIDES: readonly Side[] = ['left', 'right', 'top', 'bottom'];
const BOUNDARY_KINDS: readonly BoundaryKind[] = [
    'wall',
    'inlet',
    'outlet',
    'convective',
    'slip',
    'periodic',
];
const INLET_PROFILES: readonly InletProfile[] = ['uniform', 'parabolic', 'powerLaw'];
const TIME_SCHEMES: readonly TimeScheme[] = ['euler', 'rk2'];
const CONVECTION_SCHEMES: readonly ConvectionScheme[] = ['upwind', 'central', 'quick'];
const VISCOSITY_MODELS: readonly ViscosityModel[] = ['newtonian', 'powerLaw'];

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
        throw cliError('cli/invalid-boundary-kind', `unknown boundary kind \`${raw}\`; use wall, inlet, outlet, convective, slip or periodic`);
    }
    return raw as BoundaryKind;
}

function parseTimeScheme(raw: string): TimeScheme {
    if (!TIME_SCHEMES.includes(raw as TimeScheme)) {
        throw cliError('cli/invalid-time-scheme', `unknown time scheme \`${raw}\`; use euler or rk2`);
    }
    return raw as TimeScheme;
}

function parseInletProfile(raw: string): InletProfile {    if (!INLET_PROFILES.includes(raw as InletProfile)) {
        throw cliError(
            'cli/invalid-inlet-profile',
            `unknown inlet profile \`${raw}\`; use uniform, parabolic or powerLaw`,
        );
    }
    return raw as InletProfile;
}

function parseConvectionScheme(raw: string): ConvectionScheme {    if (!CONVECTION_SCHEMES.includes(raw as ConvectionScheme)) {
        throw cliError(
            'cli/invalid-convection-scheme',
            `unknown convection scheme \`${raw}\`; use upwind, central or quick`,
        );
    }
    return raw as ConvectionScheme;
}

function parseViscosityModel(raw: string): ViscosityModel {
    const normalized =
        raw === 'power-law' || raw === 'power_law' ? 'powerLaw' : (raw as ViscosityModel);
    if (!VISCOSITY_MODELS.includes(normalized)) {
        throw cliError(
            'fluid/invalid-viscosity-model',
            `unknown viscosity model \`${raw}\`; use newtonian or power-law`,
        );
    }
    return normalized;
}

function parseExportFormat(raw: string): ExportFormat {
    if (raw === 'inp' || raw === 'openfoam') {
        throw new NotImplementedError(`${raw} export`);
    }
    if (
        raw !== 'csv' &&
        raw !== 'vtk' &&
        raw !== 'svg' &&
        raw !== 'contours' &&
        raw !== 'streamlines' &&
        raw !== 'residuals' &&
        raw !== 'forces' &&
        raw !== 'coefficients' &&
        raw !== 'probes' &&
        raw !== 'particles' &&
        raw !== 'profile' &&
        raw !== 'statistics'
    ) {
        throw cliError(
            'cli/unknown-format',
            `unknown export format \`${raw}\`; use csv, vtk, svg, contours, streamlines, residuals, forces, coefficients, probes, particles, profile or statistics`,
        );
    }
    return raw;
}

const EXPORT_FIELDS: readonly ExportField[] = [
    'pressure',
    'temperature',
    'vorticity',
    'streamFunction',
    'speed',
    'pressureCoefficient',
];

function parseExportField(raw: string): ExportField {
    if (!EXPORT_FIELDS.includes(raw as ExportField)) {
        throw cliError(
            'cli/unknown-format',
            `unknown export field \`${raw}\`; use pressure, temperature, vorticity, streamFunction, speed or pressureCoefficient`,
        );
    }
    return raw as ExportField;
}

const EXPORT_LAYOUTS: readonly ExportLayout[] = ['cell', 'node'];

function parseExportLayout(raw: string): ExportLayout {
    if (!EXPORT_LAYOUTS.includes(raw as ExportLayout)) {
        throw cliError(
            'cli/unknown-format',
            `unknown export layout \`${raw}\`; use cell or node`,
        );
    }
    return raw as ExportLayout;
}

function parseProfileLine(raw: string): ProfileLine {
    const parts = raw.split(',').map((part) => part.trim());
    if (parts.length !== 4) {
        throw cliError(
            'cli/invalid-line',
            `profile line \`${raw}\` must look like \`x0,y0,x1,y1\``,
        );
    }
    const values = parts.map((part) => Number(part));
    if (values.some((value) => !Number.isFinite(value))) {
        throw cliError(
            'cli/invalid-line',
            `profile line \`${raw}\` must contain four finite numbers`,
        );
    }
    const [x0, y0, x1, y1] = values as [number, number, number, number];
    if (x0 === x1 && y0 === y1) {
        throw cliError(
            'cli/invalid-line',
            'profile line start and end must be different points',
        );
    }
    return { x0, y0, x1, y1 };
}

function parseProfileSamples(raw: string): number {
    const samples = Number(raw);
    if (!/^\d+$/.test(raw.trim()) || !Number.isInteger(samples) || samples < 2) {
        throw cliError(
            'cli/invalid-samples',
            `profile samples \`${raw}\` must be an integer of at least 2`,
        );
    }
    return samples;
}

function parseSideThermal(
    parsed: ParsedArguments,
    temperatureOption: string,
    heatFluxOption: string,
): SideThermal {
    const temperature = optionalNumber(parsed, temperatureOption);
    const heatFlux = optionalNumber(parsed, heatFluxOption);
    return {
        ...(temperature === undefined ? {} : { temperature }),
        ...(heatFlux === undefined ? {} : { heatFlux }),
    };
}

function parseObstacle(parsed: ParsedArguments): Obstacle {
    const kind = requiredOption(parsed, '--kind');
    const id = parsed.options['--id'];
    if (kind === 'rect') {
        const rotation = optionalNumber(parsed, '--rotation');
        return {
            kind: 'rect',
            ...(id === undefined ? {} : { id }),
            x0: numberOption(parsed, '--x0'),
            y0: numberOption(parsed, '--y0'),
            x1: numberOption(parsed, '--x1'),
            y1: numberOption(parsed, '--y1'),
            ...(rotation === undefined ? {} : { rotation }),
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
            const parsed = parseArguments(rest.slice(1), [
                '--rho',
                '--mu',
                '--viscosity-model',
                '--consistency',
                '--power',
                '--min-viscosity',
                '--max-viscosity',
            ]);
            const file = positional(
                parsed,
                0,
                'fluid set <file> --rho <kg/m3> --mu <Pa.s> | --viscosity-model power-law --consistency <K> --power <n>',
            );
            const document = await session.open(file);
            const modelRaw = parsed.options['--viscosity-model'];
            if (modelRaw === undefined) {
                const summary = document.setFluid({
                    density: numberOption(parsed, '--rho'),
                    viscosity: numberOption(parsed, '--mu'),
                });
                console.log(
                    `fluid set: rho=${summary.fluid.density} kg/m3, mu=${summary.fluid.viscosity} Pa.s`,
                );
                return 0;
            }
            const current = document.summary.fluid;
            const summary = document.setFluid({
                density: optionalNumber(parsed, '--rho') ?? current.density,
                viscosity: optionalNumber(parsed, '--mu') ?? current.viscosity,
                viscosityModel: parseViscosityModel(modelRaw),
                consistency: optionalNumber(parsed, '--consistency'),
                power: optionalNumber(parsed, '--power'),
                minViscosity: optionalNumber(parsed, '--min-viscosity'),
                maxViscosity: optionalNumber(parsed, '--max-viscosity'),
            });
            const properties = [
                `rho=${summary.fluid.density} kg/m3`,
                `mu=${summary.fluid.viscosity} Pa.s`,
                `viscosityModel=${summary.fluid.viscosityModel ?? 'newtonian'}`,
            ];
            if (summary.fluid.consistency !== undefined) {
                properties.push(`consistency=${summary.fluid.consistency}`);
            }
            if (summary.fluid.power !== undefined) {
                properties.push(`power=${summary.fluid.power}`);
            }
            if (summary.fluid.minViscosity !== undefined) {
                properties.push(`minViscosity=${summary.fluid.minViscosity}`);
            }
            if (summary.fluid.maxViscosity !== undefined) {
                properties.push(`maxViscosity=${summary.fluid.maxViscosity}`);
            }
            console.log(`fluid set: ${properties.join(', ')}`);
            return 0;
        }
        case 'time': {
            if (rest[0] === 'adaptive') {
                if (rest[1] !== 'set') {
                    throw cliError(
                        'cli/unknown-command',
                        rest[1] === undefined
                            ? '`time adaptive` needs the `set` subcommand'
                            : `\`time adaptive\` subcommand \`${rest[1]}\` does not exist; only \`set\` is supported`,
                    );
                }
                const parsed = parseArguments(rest.slice(2), ['--cfl']);
                const file = positional(parsed, 0, 'time adaptive set <file> --cfl <n|off>');
                const raw = requiredOption(parsed, '--cfl').trim();
                const enabled = raw.toLowerCase() !== 'off';
                const targetCfl = enabled ? Number(raw) : undefined;
                if (
                    enabled &&
                    (targetCfl === undefined ||
                        !Number.isFinite(targetCfl) ||
                        targetCfl <= 0 ||
                        targetCfl > 1)
                ) {
                    throw cliError(
                        'cli/invalid-number',
                        `--cfl must be a number in (0, 1] or \`off\`, got \`${raw}\``,
                    );
                }
                const document = await session.open(file);
                const summary = document.setTimeAdaptive({ enabled, targetCfl });
                console.log(
                    `time adaptive set: adaptive=${summary.time.adaptive}, targetCfl=${summary.time.targetCfl}`,
                );
                return 0;
            }
            if (rest[0] === 'scheme') {
                if (rest[1] !== 'set') {
                    throw cliError(
                        'cli/unknown-command',
                        rest[1] === undefined
                            ? '`time scheme` needs the `set` subcommand'
                            : `\`time scheme\` subcommand \`${rest[1]}\` does not exist; only \`set\` is supported`,
                    );
                }
                const parsed = parseArguments(rest.slice(2), ['--scheme']);
                const file = positional(parsed, 0, 'time scheme set <file> --scheme euler|rk2');
                const scheme = parseTimeScheme(requiredOption(parsed, '--scheme'));
                const document = await session.open(file);
                const summary = document.setTimeScheme({ scheme });
                console.log(`time scheme set: ${summary.time.scheme}`);
                return 0;
            }
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
            if (rest[0] === 'list') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'boundary list <file>');
                const document = await session.open(file);
                console.log(JSON.stringify(document.listBoundaries(), null, 2));
                return 0;
            }
            if (rest[0] === 'reset') {
                const parsed = parseArguments(rest.slice(1), ['--side']);
                const file = positional(parsed, 0, 'boundary reset <file> [--side <side>]');
                const raw = parsed.options['--side'];
                const side = raw === undefined ? undefined : parseSide(raw);
                const document = await session.open(file);
                document.resetBoundary(side);
                console.log(
                    side === undefined
                        ? 'reset all boundaries to stationary walls; dropped all transient drives'
                        : `reset ${side} boundary to a stationary wall; dropped its transient drive`,
                );
                return 0;
            }
            if (rest[0] === 'profile') {
                if (rest[1] !== 'set') {
                    throw cliError(
                        'cli/unknown-command',
                        rest[1] === undefined
                            ? '`boundary profile` needs the `set` subcommand'
                            : `\`boundary profile\` subcommand \`${rest[1]}\` does not exist; only \`set\` is supported`,
                    );
                }
                const parsed = parseArguments(rest.slice(2), [
                    '--side',
                    '--profile',
                    '--exponent',
                ]);
                const file = positional(
                    parsed,
                    0,
                    'boundary profile set <file> --side left|right|top|bottom --profile uniform|parabolic|powerLaw [--exponent <n>]',
                );
                const side = parseSide(requiredOption(parsed, '--side'));
                const profile = parseInletProfile(requiredOption(parsed, '--profile'));
                const exponent = optionalNumber(parsed, '--exponent');
                const document = await session.open(file);
                const summary = document.setInletProfile({ side, profile, exponent });
                console.log(
                    `${side} inlet profile set to ${summary.boundaries.inletProfiles[side]} (power-law exponent ${summary.boundaries.inletProfiles.exponent})`,
                );
                return 0;
            }
            if (rest[0] !== 'set') {
                throw cliError(
                    'cli/unknown-command',
                    rest[0] === undefined
                        ? '`boundary` needs the `set`, `list`, `reset` or `profile` subcommand'
                        : `\`boundary\` subcommand \`${rest[0]}\` does not exist; use set, list, reset or profile`,
                );
            }
            const parsed = parseArguments(rest.slice(1), [
                '--side',
                '--kind',
                '--u',
                '--v',
                '--pressure',
            ]);
            const file = positional(
                parsed,
                0,
                'boundary set <file> --side left|right|top|bottom --kind wall|inlet|outlet|convective|slip|periodic [--u <m/s>] [--v <m/s>] [--pressure <Pa>]',
            );
            const side = parseSide(requiredOption(parsed, '--side'));
            const kind = parseBoundaryKind(requiredOption(parsed, '--kind'));
            const document = await session.open(file);
            const summary = document.setBoundary({
                side,
                kind,
                u: optionalNumber(parsed, '--u'),
                v: optionalNumber(parsed, '--v'),
                pressure: optionalNumber(parsed, '--pressure'),
            });
            const boundary = summary.boundaries[side];
            console.log(
                `${side} boundary set to ${boundary.kind}, velocity=[${boundary.velocity[0]}, ${boundary.velocity[1]}] m/s, pressure=${boundary.pressure} Pa`,
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
            if (action === 'remove') {
                const parsed = parseArguments(rest.slice(1), ['--id']);
                const file = positional(parsed, 0, 'obstacle remove <file> --id <name>');
                const id = requiredOption(parsed, '--id');
                const document = await session.open(file);
                const summary = document.removeObstacle(id);
                console.log(`removed obstacle ${id}; ${summary.obstacles.length} total`);
                return 0;
            }
            if (action !== 'add' && action !== 'update') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`obstacle` needs the `add`, `update`, `remove` or `list` subcommand'
                        : `\`obstacle\` subcommand \`${action}\` does not exist; use add, update, remove or list`,
                );
            }
            const parsed = parseArguments(rest.slice(1), [
                '--kind',
                '--id',
                '--x0',
                '--y0',
                '--x1',
                '--y1',
                '--rotation',
                '--x',
                '--y',
                '--r',
            ]);
            const file = positional(
                parsed,
                0,
                action === 'add'
                    ? 'obstacle add <file> --kind rect|circle ... [--id <name>]'
                    : 'obstacle update <file> --id <name> --kind rect|circle ...',
            );
            const document = await session.open(file);
            if (action === 'update') {
                const id = requiredOption(parsed, '--id');
                const summary = document.updateObstacle(id, parseObstacle(parsed));
                const updated = summary.obstacles.find((obstacle) => obstacle.id === id);
                console.log(
                    `updated ${updated?.kind ?? 'obstacle'} obstacle ${id}; ${summary.obstacles.length} total`,
                );
                return 0;
            }
            const summary = document.addObstacle(parseObstacle(parsed));
            const added = summary.obstacles[summary.obstacles.length - 1];
            console.log(
                `added ${added.kind} obstacle ${added.id ?? ''}; ${summary.obstacles.length} total`,
            );
            return 0;
        }
        case 'porous': {
            const action = rest[0];
            if (action === 'list') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'porous list <file>');
                const document = await session.open(file);
                console.log(JSON.stringify(document.listPorous(), null, 2));
                return 0;
            }
            if (action === 'remove') {
                const parsed = parseArguments(rest.slice(1), ['--id']);
                const file = positional(parsed, 0, 'porous remove <file> --id <name>');
                const id = requiredOption(parsed, '--id');
                const document = await session.open(file);
                const summary = document.removePorous(id);
                console.log(`removed porous region ${id}; ${summary.porous.length} total`);
                return 0;
            }
            if (action !== 'add') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`porous` needs the `add`, `list` or `remove` subcommand'
                        : `\`porous\` subcommand \`${action}\` does not exist; use add, list or remove`,
                );
            }
            const parsed = parseArguments(rest.slice(1), [
                '--id',
                '--x0',
                '--y0',
                '--x1',
                '--y1',
                '--permeability',
                '--inertial',
            ]);
            const file = positional(
                parsed,
                0,
                'porous add <file> --x0 <m> --y0 <m> --x1 <m> --y1 <m> --permeability <m2> [--inertial <n>] [--id <name>]',
            );
            const id = parsed.options['--id'];
            const document = await session.open(file);
            const summary = document.addPorous({
                ...(id === undefined ? {} : { id }),
                x0: numberOption(parsed, '--x0'),
                y0: numberOption(parsed, '--y0'),
                x1: numberOption(parsed, '--x1'),
                y1: numberOption(parsed, '--y1'),
                permeability: numberOption(parsed, '--permeability'),
                inertial: optionalNumber(parsed, '--inertial') ?? 0,
            });
            const added = summary.porous[summary.porous.length - 1];
            console.log(
                `added porous region ${added.id ?? ''} with K=${added.permeability} m2 and C_F=${added.inertial}; ${summary.porous.length} total`,
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
            if (action !== 'add' && action !== 'update' && action !== 'remove') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`probe` needs the `add`, `update`, `remove`, `list` or `clear` subcommand'
                        : `\`probe\` subcommand \`${action}\` does not exist; use add, update, remove, list or clear`,
                );
            }
            const parsed = parseArguments(rest.slice(1), ['--id', '--x', '--y']);
            if (action === 'remove') {
                const file = positional(parsed, 0, 'probe remove <file> --id <name>');
                const id = requiredOption(parsed, '--id');
                const document = await session.open(file);
                const summary = document.removeProbe(id);
                console.log(`removed probe ${id}; ${summary.probes.length} total`);
                return 0;
            }
            if (action === 'update') {
                const file = positional(
                    parsed,
                    0,
                    'probe update <file> --id <name> --x <m> --y <m>',
                );
                const id = requiredOption(parsed, '--id');
                const document = await session.open(file);
                const summary = document.updateProbe(
                    id,
                    numberOption(parsed, '--x'),
                    numberOption(parsed, '--y'),
                );
                console.log(
                    `moved probe ${id}; ${summary.probes.length} total`,
                );
                return 0;
            }
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
        case 'thermal':
        case 'heat': {
            expectSet(rest, 'thermal');
            const parsed = parseArguments(rest.slice(1), [
                '--specific-heat',
                '--conductivity',
                '--diffusivity',
                '--heat-source',
                '--left-temperature',
                '--right-temperature',
                '--top-temperature',
                '--bottom-temperature',
                '--left-heat-flux',
                '--right-heat-flux',
                '--top-heat-flux',
                '--bottom-heat-flux',
                '--obstacle-temperature',
                '--obstacle-heat-flux',
                '--reference-temperature',
            ]);
            const file = positional(
                parsed,
                0,
                'thermal set <file> [--specific-heat <J/kgK>] [--conductivity <W/mK>] [thermal options]',
            );
            const document = await session.open(file);
            const specificHeat = optionalNumber(parsed, '--specific-heat') ?? 1;
            const conductivity = optionalNumber(parsed, '--conductivity');
            let diffusivity = optionalNumber(parsed, '--diffusivity');
            if (diffusivity === undefined && conductivity !== undefined) {
                const density = document.summary.fluid.density;
                if (density * specificHeat > 0) {
                    diffusivity = conductivity / (density * specificHeat);
                }
            }
            const obstacleTemperature = optionalNumber(parsed, '--obstacle-temperature');
            const obstacleHeatFlux = optionalNumber(parsed, '--obstacle-heat-flux');
            const referenceTemperature = optionalNumber(parsed, '--reference-temperature');
            const summary = document.setThermal({
                specificHeat,
                ...(conductivity === undefined ? {} : { conductivity }),
                heatSource: optionalNumber(parsed, '--heat-source') ?? 0,
                left: parseSideThermal(parsed, '--left-temperature', '--left-heat-flux'),
                right: parseSideThermal(parsed, '--right-temperature', '--right-heat-flux'),
                top: parseSideThermal(parsed, '--top-temperature', '--top-heat-flux'),
                bottom: parseSideThermal(parsed, '--bottom-temperature', '--bottom-heat-flux'),
                ...(obstacleTemperature === undefined ? {} : { obstacleTemperature }),
                ...(obstacleHeatFlux === undefined ? {} : { obstacleHeatFlux }),
                ...(referenceTemperature === undefined ? {} : { referenceTemperature }),
                ...(diffusivity === undefined ? {} : { diffusivity }),
            });
            const thermal = summary.thermal;
            console.log(
                `thermal set: cp=${thermal?.specificHeat} J/kgK, k=${thermal?.conductivity} W/mK, heatSource=${thermal?.heatSource} W/m3, obstacleTemperature=${thermal?.obstacleTemperature ?? 'none'}, obstacleHeatFlux=${thermal?.obstacleHeatFlux ?? 'none'}`,
            );
            return 0;
        }
        case 'wall-function': {
            const action = rest[0];
            if (action === 'clear') {
                const parsed = parseArguments(rest.slice(1), []);
                const file = positional(parsed, 0, 'wall-function clear <file>');
                const document = await session.open(file);
                document.clearWallFunction();
                console.log('wall function cleared');
                return 0;
            }
            if (action !== 'set') {
                throw cliError(
                    'cli/unknown-command',
                    action === undefined
                        ? '`wall-function` needs the `set` or `clear` subcommand'
                        : `\`wall-function\` subcommand \`${action}\` does not exist; use set or clear`,
                );
            }
            const parsed = parseArguments(rest.slice(1), [
                '--kappa',
                '--intercept',
                '--min-y-plus',
            ]);
            const file = positional(
                parsed,
                0,
                'wall-function set <file> [--kappa <n>] [--intercept <n>] [--min-y-plus <n>]',
            );
            const document = await session.open(file);
            const summary = document.setWallFunction({
                kappa: optionalNumber(parsed, '--kappa'),
                intercept: optionalNumber(parsed, '--intercept'),
                minYPlus: optionalNumber(parsed, '--min-y-plus'),
            });
            const wall = summary.wallFunction;
            console.log(
                `wall function set: kappa=${wall?.kappa}, intercept=${wall?.intercept}, minYPlus=${wall?.minYPlus}`,
            );
            return 0;
        }
        case 'buoyancy': {
            expectSet(rest, 'buoyancy');
            const parsed = parseArguments(rest.slice(1), [
                '--beta',
                '--t-ref',
                '--gx',
                '--gy',
            ]);
            const file = positional(
                parsed,
                0,
                'buoyancy set <file> --beta <1/K> --t-ref <K> --gx <m/s2> --gy <m/s2>',
            );
            const document = await session.open(file);
            const summary = document.setBuoyancy({
                beta: numberOption(parsed, '--beta'),
                tRef: numberOption(parsed, '--t-ref'),
                gx: numberOption(parsed, '--gx'),
                gy: numberOption(parsed, '--gy'),
            });
            const buoyancy = summary.buoyancy;
            console.log(
                buoyancy === undefined
                    ? 'buoyancy disabled'
                    : `buoyancy set: beta=${buoyancy.beta} 1/K, tRef=${buoyancy.tRef} K, gravity=[${buoyancy.gravity[0]}, ${buoyancy.gravity[1]}] m/s2`,
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
        case 'pressure': {
            if (rest[0] !== 'gradient' || rest[1] !== 'set') {
                throw cliError(
                    'cli/unknown-command',
                    '`pressure` needs the `gradient set` subcommand',
                );
            }
            const parsed = parseArguments(rest.slice(2), ['--dpdx', '--dpdy']);
            const file = positional(
                parsed,
                0,
                'pressure gradient set <file> --dpdx <Pa/m> --dpdy <Pa/m>',
            );
            const document = await session.open(file);
            const summary = document.setPressureGradient({
                dpdx: numberOption(parsed, '--dpdx'),
                dpdy: numberOption(parsed, '--dpdy'),
            });
            const gradient = summary.pressureGradient;
            console.log(
                gradient === undefined
                    ? 'pressure gradient cleared'
                    : `pressure gradient set: dpdx=${gradient[0]} Pa/m, dpdy=${gradient[1]} Pa/m`,
            );
            return 0;
        }
        case 'reference': {
            expectSet(rest, 'reference');
            const parsed = parseArguments(rest.slice(1), [
                '--velocity',
                '--area',
                '--length',
            ]);
            const file = positional(
                parsed,
                0,
                'reference set <file> [--velocity <m/s>] [--area <m2>] [--length <m>]',
            );
            const velocity = optionalNumber(parsed, '--velocity');
            const area = optionalNumber(parsed, '--area');
            const length = optionalNumber(parsed, '--length');
            if (velocity === undefined && area === undefined && length === undefined) {
                throw cliError(
                    'cli/missing-option',
                    'reference set needs at least one of --velocity, --area or --length',
                );
            }
            const document = await session.open(file);
            const summary = document.setReference({ velocity, area, length });
            const reference = summary.reference;
            console.log(
                `reference set: velocity=${reference?.velocity}, area=${reference?.area}, length=${reference?.length}`,
            );
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
        case 'quality': {
            const parsed = parseArguments(rest, []);
            const file = positional(parsed, 0, 'quality <file>');
            const document = await session.open(file);
            console.log(JSON.stringify(document.meshQuality(), null, 2));
            return 0;
        }
        case 'reynolds': {
            const parsed = parseArguments(rest, []);
            const file = positional(parsed, 0, 'reynolds <file>');
            const document = await session.open(file);
            console.log(JSON.stringify(document.reynolds(), null, 2));
            return 0;
        }
        case 'stability': {
            const parsed = parseArguments(rest, []);
            const file = positional(parsed, 0, 'stability <file>');
            const document = await session.open(file);
            console.log(JSON.stringify(document.stability(), null, 2));
            return 0;
        }
        case 'solve': {
            const parsed = parseArguments(rest, [
                '--out',
                '--steps',
                '--tolerance',
                '--check-every',
                '--restart',
                '--checkpoint',
            ]);
            const file = positional(
                parsed,
                0,
                'solve <file> [--steps <n>] [--tolerance <v>] [--check-every <n>] [--restart <checkpoint.json>] [--checkpoint <checkpoint.json>] [--out report.json]',
            );
            const steps = optionalInteger(parsed, '--steps');
            const tolerance = optionalNumber(parsed, '--tolerance');
            const checkEvery = optionalInteger(parsed, '--check-every');
            const restart = parsed.options['--restart'];
            const checkpoint = parsed.options['--checkpoint'];
            const document = await session.open(file);
            const report = document.solve({ steps, tolerance, checkEvery, restart, checkpoint });
            const json = JSON.stringify(report, null, 2);
            const out = parsed.options['--out'];
            if (out !== undefined) {
                writeAtomic(out, json);
            }
            console.log(json);
            return 0;
        }
        case 'export': {
            const parsed = parseArguments(rest, [
                '--format',
                '--out',
                '--field',
                '--layout',
                '--line',
                '--samples',
            ]);
            const file = positional(
                parsed,
                0,
                'export <file> --format csv|vtk|svg|contours|streamlines|residuals|forces|coefficients|probes|particles|profile|statistics [--field pressure|temperature|vorticity|streamFunction|speed|pressureCoefficient] [--layout cell|node] [--line <x0,y0,x1,y1>] [--samples <n>] --out <dir|file>',
            );
            const format = parseExportFormat(requiredOption(parsed, '--format'));
            const rawField = parsed.options['--field'];
            const field = rawField === undefined ? undefined : parseExportField(rawField);
            const rawLayout = parsed.options['--layout'];
            const layout = rawLayout === undefined ? undefined : parseExportLayout(rawLayout);
            const out = requiredOption(parsed, '--out');
            const document = await session.open(file);
            if (format === 'profile') {
                const rawLine = parsed.options['--line'];
                if (rawLine === undefined) {
                    throw cliError(
                        'cli/missing-option',
                        '`export --format profile` needs `--line <x0,y0,x1,y1>`',
                    );
                }
                const rawSamples = parsed.options['--samples'];
                const samples = rawSamples === undefined ? 101 : parseProfileSamples(rawSamples);
                const profileField = field ?? 'speed';
                const report = document.exportProfile(out, {
                    line: parseProfileLine(rawLine),
                    samples,
                    field: profileField,
                });
                console.log(
                    `exported line profile CSV (${profileField}, ${samples} samples): ${report.path}`,
                );
                return 0;
            }
            if (format === 'statistics') {
                const report = document.exportStatistics(out);
                console.log(`exported field statistics JSON: ${report.path}`);
                return 0;
            }
            const report = document.export(format, out, field, layout);
            const layoutLabel = report.layout === 'node' ? ' (nodes)' : '';
            if (format === 'csv') {
                console.log(`exported CSV${layoutLabel}: ${report.path}`);
            } else if (format === 'vtk') {
                console.log(`exported legacy ASCII VTK${layoutLabel}: ${report.path}`);
            } else if (format === 'streamlines') {
                console.log(`exported streamlines SVG: ${report.path}`);
            } else if (format === 'contours') {
                console.log(`exported contour SVG (${field ?? 'pressure'}): ${report.path}`);
            } else if (format === 'residuals') {
                console.log(`exported residual history CSV: ${report.path}`);
            } else if (format === 'forces') {
                console.log(`exported force history CSV: ${report.path}`);
            } else if (format === 'coefficients') {
                console.log(`exported coefficient summary CSV: ${report.path}`);
            } else if (format === 'probes') {
                console.log(`exported probe history CSV: ${report.path}`);
            } else if (format === 'particles') {
                console.log(`exported particle trajectories CSV: ${report.path}`);
            } else {
                console.log(`exported SVG (${field ?? 'pressure'}): ${report.path}`);
            }
            return 0;
        }
        case 'compressible':
            throw new NotImplementedError('compressible flow');
        case 'turbulence':
            throw new NotImplementedError('turbulence models');
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
