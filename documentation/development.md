# Development

Maintainer guide for building, testing and checking OpenCFD.

## Environment

- Node.js 24 and pnpm 10.28 (the versions CI uses).
- A current stable Rust toolchain with `rustfmt` and `clippy`; CI uses nightly.
- Windows, macOS or Linux. The native addon is built for the host unless a
  target is given.

```bash
pnpm install
```

## Build

`wae.config.ts` fixes the build order: native → TypeScript → staging.

```bash
pnpm run build          # the full pipeline
pnpm run build:napi     # only the open-cfd-napi addon for the host
pnpm run build:ts       # only @wae/open-cfd (dist/*.js)
pnpm run stage:native   # copy addons into the platform packages
pnpm run check          # cargo check -p open-cfd-napi
```

The addon is not committed. It is staged as
`open-cfd.<platform>-<arch>.node` into
`projects/packages/open-cfd-<platform>-<arch>`. Without the matching platform
package, SDK calls report `capability/not-implemented`; only local debugging
sets `OPEN_CFD_NATIVE` to a `.node` file.

The developer `ocfd` binary in `projects/crates/cfd-model/src/bin` is a thin
command-line driver over the same session and is not shipped in the npm package.

## Tests

```bash
cargo test -p cfd-model -p open-cfd-napi --offline
pnpm --filter @wae/open-cfd run typecheck
pnpm --filter @wae/open-cfd run test
```

Test layout:

- `projects/crates/cfd-model/tests/` — integration and solver tests
  (`cfd_model`, `session`, `sdk_parity`, …).
- `projects/crates/cfd-model/tests/unit/` — private-model tests included from
  `src/solver.rs` and `src/export.rs` through
  `#[cfg(test)] #[path = "../tests/unit/<name>.rs"] mod tests;`.
- `projects/packages/open-cfd/tests/` — TypeScript smoke tests compiled with
  `tsconfig.test.json` into `dist-test`; native-addon cases skip when no addon
  is installed.

Do not put test modules or `*.test.ts` files under `src/`.

## Formatting, linting and CI

```bash
cargo fmt -p cfd-model -p open-cfd-napi
cargo clippy -p cfd-model -p open-cfd-napi --all-targets -- -D warnings
pnpm typecheck
pnpm test
```

CI runs on `master` and `dev`:

- `check-rust.yml` — `cargo fmt --all --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo test --workspace` (nightly toolchain).
- `check-typescript.yml` — `pnpm install --frozen-lockfile`, `pnpm typecheck`,
  `pnpm run build:ts` on Node 24.

## Release

The application package is `private`; the publishable set is the per-platform
native packages produced by staging. `wae.config.ts` leaves `release` unset
while that is true, and there is no CI release workflow yet. Do not commit
generated addons or `dist/`.

## Adding a capability

1. Start in `projects/crates/cfd-model`: document shape, validation, session
   method, solver or export.
2. Add the Node-API export as a value mapping, then the SDK method
   (`open-cfd.ts`, `document.ts`, `service.ts`, `types.ts`) and the CLI
   (`cli.ts` plus `locales/en-US/cli.json`).
3. Add a model test under `tests/` and extend `tests/sdk_parity.rs` and the
   TypeScript smoke test when the surface changes.
4. Update `CAPABILITIES_JSON`, `documentation/capabilities.md` and
   `documentation/cli.md`.
5. Run the checks above plus `git diff --check`; stage explicit paths.
