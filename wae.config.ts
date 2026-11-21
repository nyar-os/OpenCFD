/**
 * Build and release configuration for OpenCFD.
 *
 * Pure literals only: `wae` parses this file statically and never executes it.
 * Build order is fixed: native → ts → staging.
 *
 * The package lives under `projects/packages`; the native addon is built from
 * the `open-cfd-napi` crate and staged into the per-platform packages, so
 * `@wae/open-cfd` resolves the addon without any environment variable.
 */
import { defineConfig } from '@wae/wae';

export default defineConfig({
    native: {
        crate: 'open-cfd-napi',
        cdylib: 'open_cfd_napi',
        targets: [
            {
                platform: 'win32',
                arch: 'x64',
                triple: 'x86_64-pc-windows-msvc',
                package: '@wae/open-cfd-win32-x64',
                nodeFile: 'open-cfd.win32-x64.node',
                dir: 'projects/packages/open-cfd-win32-x64',
            },
            {
                platform: 'darwin',
                arch: 'x64',
                triple: 'x86_64-apple-darwin',
                package: '@wae/open-cfd-darwin-x64',
                nodeFile: 'open-cfd.darwin-x64.node',
                dir: 'projects/packages/open-cfd-darwin-x64',
            },
            {
                platform: 'darwin',
                arch: 'arm64',
                triple: 'aarch64-apple-darwin',
                package: '@wae/open-cfd-darwin-arm64',
                nodeFile: 'open-cfd.darwin-arm64.node',
                dir: 'projects/packages/open-cfd-darwin-arm64',
            },
            {
                platform: 'linux',
                arch: 'x64',
                triple: 'x86_64-unknown-linux-gnu',
                package: '@wae/open-cfd-linux-x64',
                nodeFile: 'open-cfd.linux-x64.node',
                dir: 'projects/packages/open-cfd-linux-x64',
            },
            {
                platform: 'linux',
                arch: 'arm64',
                triple: 'aarch64-unknown-linux-gnu',
                package: '@wae/open-cfd-linux-arm64',
                nodeFile: 'open-cfd.linux-arm64.node',
                dir: 'projects/packages/open-cfd-linux-arm64',
            },
        ],
    },
    packages: [
        {
            name: '@wae/open-cfd-win32-x64',
            dir: 'projects/packages/open-cfd-win32-x64',
            os: ['win32'],
            cpu: ['x64'],
            main: 'open-cfd.win32-x64.node',
        },
        {
            name: '@wae/open-cfd-darwin-x64',
            dir: 'projects/packages/open-cfd-darwin-x64',
            os: ['darwin'],
            cpu: ['x64'],
            main: 'open-cfd.darwin-x64.node',
        },
        {
            name: '@wae/open-cfd-darwin-arm64',
            dir: 'projects/packages/open-cfd-darwin-arm64',
            os: ['darwin'],
            cpu: ['arm64'],
            main: 'open-cfd.darwin-arm64.node',
        },
        {
            name: '@wae/open-cfd-linux-x64',
            dir: 'projects/packages/open-cfd-linux-x64',
            os: ['linux'],
            cpu: ['x64'],
            main: 'open-cfd.linux-x64.node',
        },
        {
            name: '@wae/open-cfd-linux-arm64',
            dir: 'projects/packages/open-cfd-linux-arm64',
            os: ['linux'],
            cpu: ['arm64'],
            main: 'open-cfd.linux-arm64.node',
        },
    ],
    build: {
        ts: { packages: ['@wae/open-cfd'] },
        outputs: ['projects/packages/open-cfd/dist/cli.js', 'projects/packages/open-cfd/dist/index.js'],
    },
});
