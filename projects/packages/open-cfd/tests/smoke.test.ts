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
