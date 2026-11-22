/**
 * SDK smoke tests.
 *
 * These forward to the native addon, so every test is skipped when the addon is
 * not installed. They assert the mapping the SDK owns: request shapes, summary
 * refresh and the query methods. Domain rules and their error codes remain in
 * the Rust model and are covered by the Rust integration tests.
 */

import assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import test from 'node:test';

import { OpenCFD } from '../src/index.js';
import { isOpenCFDError } from '../src/errors.js';
import type { ViscosityModel } from '../src/types.js';

async function addonInstalled(): Promise<boolean> {
    const session = new OpenCFD();
    try {
        await session.capabilities();
        return true;
    } catch {
        return false;
    } finally {
        await session.dispose();
    }
}

const installed = await addonInstalled();

function workspace(): string {
    return fs.mkdtempSync(path.join(os.tmpdir(), 'open-cfd-test-'));
}

test('porous regions round-trip through the SDK', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'porous.ocfd'));
        const summary = cfdCase.addPorous({
            x0: 0.2,
            y0: 0.2,
            x1: 0.8,
            y1: 0.8,
            permeability: 1e-3,
            inertial: 0.5,
        });
        assert.equal(summary.porous.length, 1);
        assert.equal(summary.porous[0].id, 'porous-1');
        assert.deepEqual(cfdCase.listPorous(), summary.porous);

        cfdCase.removePorous('porous-1');
        assert.equal(cfdCase.listPorous().length, 0);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('thermal settings require scalar transport', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'thermal.ocfd'));
        const summary = cfdCase.setThermal({
            specificHeat: 1000,
            conductivity: 0.6,
            heatSource: 2,
            left: { temperature: 320 },
            right: {},
            top: {},
            bottom: {},
            referenceTemperature: 300,
            diffusivity: 1e-3,
        });
        assert.equal(summary.thermal?.specificHeat, 1000);
        assert.equal(summary.thermal?.left.temperature, 320);
        assert.equal(summary.scalar?.diffusivity, 1e-3);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('wall function settings set and clear', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'wall.ocfd'));
        const summary = cfdCase.setWallFunction({ kappa: 0.4 });
        assert.equal(summary.wallFunction?.kappa, 0.4);
        assert.equal(summary.wallFunction?.intercept, 5.2);
        assert.equal(summary.wallFunction?.minYPlus, 11.06);

        const cleared = cfdCase.clearWallFunction();
        assert.equal(cleared.wallFunction, undefined);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('obstacles update in place, rotate and remove', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'obstacles.ocfd'));
        cfdCase.addObstacle({ kind: 'circle', id: 'cyl', x: 0.5, y: 0.5, r: 0.05 });
        const added = cfdCase.addObstacle({
            kind: 'rect',
            id: 'bar',
            x0: 0.3,
            y0: 0.48,
            x1: 0.7,
            y1: 0.52,
        });
        assert.equal(added.obstacles.length, 2);

        const updated = cfdCase.updateObstacle('bar', {
            kind: 'rect',
            x0: 0.3,
            y0: 0.48,
            x1: 0.7,
            y1: 0.52,
            rotation: 90,
        });
        assert.equal(updated.obstacles[0].id, 'cyl');
        assert.equal(updated.obstacles[1].id, 'bar');
        const rect = updated.obstacles[1];
        assert.equal(rect.kind, 'rect');
        if (rect.kind === 'rect') {
            assert.equal(rect.rotation, 90);
        }

        const removed = cfdCase.removeObstacle('cyl');
        assert.equal(removed.obstacles.length, 1);
        assert.equal(removed.obstacles[0].id, 'bar');
        assert.equal(cfdCase.listObstacles().length, 1);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('power-law viscosity settings round-trip and appear in reports', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'power-law.ocfd'));
        const summary = cfdCase.setFluid({
            density: 1,
            viscosity: 0.005,
            viscosityModel: 'powerLaw',
            consistency: 0.005,
            power: 0.8,
            minViscosity: 0.001,
            maxViscosity: 0.02,
        });
        assert.equal(summary.fluid.viscosityModel, 'powerLaw');
        assert.equal(summary.fluid.consistency, 0.005);
        assert.equal(summary.fluid.power, 0.8);
        assert.equal(summary.fluid.minViscosity, 0.001);
        assert.equal(summary.fluid.maxViscosity, 0.02);
        assert.equal(cfdCase.summary.fluid.viscosityModel, 'powerLaw');

        const limits = cfdCase.stability();
        assert.equal(limits.viscosityModel, 'powerLaw');
        assert.ok(limits.apparentViscosityMin <= limits.apparentViscosityMax);

        const report = cfdCase.solve({ steps: 1 });
        assert.equal(report.viscosityModel, 'powerLaw');
        assert.ok(Number.isFinite(report.apparentViscosityMin));
        assert.ok(Number.isFinite(report.apparentViscosityMax));
        assert.ok(Number.isFinite(report.apparentViscosityMean));

        const newtonian = cfdCase.setFluid({ density: 1, viscosity: 0.005, viscosityModel: 'newtonian' });
        assert.equal(newtonian.fluid.viscosityModel, undefined);
        assert.equal(newtonian.fluid.consistency, undefined);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});

test('invalid viscosity settings keep the document unchanged', { skip: !installed }, async () => {
    const directory = workspace();
    const cfd = new OpenCFD();
    try {
        const cfdCase = await cfd.create(path.join(directory, 'invalid.ocfd'));
        const before = JSON.stringify(cfdCase.summary.fluid);

        assert.throws(
            () =>
                cfdCase.setFluid({
                    density: 1,
                    viscosity: 0.05,
                    viscosityModel: 'bogus' as ViscosityModel,
                }),
            (error) => isOpenCFDError(error) && error.code === 'fluid/invalid-viscosity-model',
        );
        assert.throws(
            () =>
                cfdCase.setFluid({
                    density: 1,
                    viscosity: 0.05,
                    viscosityModel: 'powerLaw',
                    power: 0.8,
                }),
            (error) => isOpenCFDError(error) && error.code === 'fluid/invalid-viscosity',
        );
        assert.equal(JSON.stringify(cfdCase.summary.fluid), before);
    } finally {
        await cfd.dispose();
        fs.rmSync(directory, { recursive: true, force: true });
    }
});
