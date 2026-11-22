# AGENTS.md

Guidance for agents working in the OpenCFD repository.

## Purpose

OpenCFD is a two-dimensional incompressible CFD product. A case is one `.ocfd`
JSON file; the product exposes it through a Rust session, a Node-API addon, a
TypeScript SDK and the `open-cfd` CLI, plus a developer `ocfd` binary.

## Documentation map

| File | Audience |
|------|----------|
| [README.md](README.md) | Product landing page |
| [projects/packages/open-cfd/README.md](projects/packages/open-cfd/README.md) | npm registry page |
| [projects/packages/open-cfd-skills/README.md](projects/packages/open-cfd-skills/README.md) | Agent skill package page |
| [documentation/architecture.md](documentation/architecture.md) | Case model, module boundaries, data flow |
| [documentation/capabilities.md](documentation/capabilities.md) | Supported features, report fields, numerical method, tests, limitations |
| [documentation/cli.md](documentation/cli.md) | Command and flag reference |
| [documentation/development.md](documentation/development.md) | Build, tests, addon staging, CI and release |
| [documentation/README.md](documentation/README.md) | Maintainer doc index |

## Architecture invariants

1. **The Rust session is the only implementation.** Document I/O, editing,
   validation, solving and CSV/VTK/SVG export live in
   `projects/crates/cfd-model`. Do not reimplement model logic elsewhere.
2. **The Node-API layer maps values only.** Each `projects/crates/cfd-napi`
   export takes decoded values or one JSON request, calls one `OpenCfd` method
   and returns a JSON string. It must not add domain rules, defaults that
   change results, or caching.
3. **The TypeScript SDK/CLI must not duplicate domain logic.** It parses
   arguments, calls native functions, formats output and maps error codes to
   exit codes (`capability/*` → `2`, other failures → `1`).
4. **The capability matrix is the only support claim.** If a feature is not
   marked supported in `CAPABILITIES_JSON` and `documentation/capabilities.md`,
   it is not supported, regardless of an API name or CLI flag.
5. **Unsupported work fails explicitly.** Return `capability/not-implemented`
   (or the specific stable code) instead of approximating silently.
6. **Edits persist atomically.** Every editing call writes the document with
   the temporary-file-and-rename path. There is no undo stack.
7. **Reports stay deterministic.** No random source, fixed seed grids and
   palettes; repeated exports are byte-identical.
8. **Tests live in `tests/`.** Rust unit tests are declared from `src` with
   `#[cfg(test)] #[path = "../tests/unit/<name>.rs"] mod tests;` and their
   bodies live in `projects/crates/cfd-model/tests/unit/`. Integration tests
   live directly in `projects/crates/cfd-model/tests/`. TypeScript tests live
   in `projects/packages/open-cfd/tests/`. Never put a test file under `src/`.

## Verified commands

```shell
cargo fmt -p cfd-model -p open-cfd-napi
cargo test -p cfd-model -p open-cfd-napi --offline
cargo clippy -p cfd-model -p open-cfd-napi --offline --all-targets -- -D warnings

pnpm --filter @wae/open-cfd run typecheck
pnpm --filter @wae/open-cfd run test
```

Full builds:

```shell
pnpm install
pnpm run build        # wae build: native -> ts -> staging
pnpm run build:ts     # tsc only, produces @wae/open-cfd/dist
```

`wae native build` stages the `open-cfd-napi` cdylib as
`open-cfd.<platform>-<arch>.node` in the `@wae/open-cfd-<platform>-<arch>`
package. The addon is not committed; without it the SDK reports
`capability/not-implemented`. `OPEN_CFD_NATIVE` is a local debugging override.

The Rust workspace pins `science-mesh`, `boost-check` and `boost-session` to one
`boost.rs` revision and `wae` to one framework revision, so local and CI builds
resolve the same commits. Use `--offline` when the dependency cache is warm.

## Change checklist

- Search the repository before assuming a command exists; verify flags against
  `projects/packages/open-cfd/src/cli.ts` and `projects/crates/cfd-model/src/bin/ocfd.rs`.
- Add or update a test for every numerical or behavioral change.
- Keep the Rust session, Node-API surface, SDK types, service functions and CLI
  in sync when a capability is added or removed.
- Update `documentation/capabilities.md`, `documentation/cli.md` and
  `CAPABILITIES_JSON` when the support claim changes.
- Update `locales/en-US/cli.json` for every new command or option.
- Update the package README only for registry-facing changes (exports, usage,
  constraints). Keep product narrative in the root README.
- Never put build, monorepo, publish or architecture detail in either README.
- Run the checks above plus `git diff --check` before committing.
- Stage explicit paths; never `git add -A`. Do not push unless asked.

## Commit style

Use a one-line subject with an emoji prefix:

- `✨ Add ...` — new capability or user-visible feature
- `🐛 Fix ...` — bug fix
- `📝 ...` — documentation, comments or prose
- `🚚 ...` — moving or renaming files without behaviour changes
- `🔖 ...` — version or lockfile bookkeeping

Keep the subject imperative and specific. If a change spans documentation and
behavior, split it into separate commits.
