# OpenCFD documentation

Reference material for people maintaining, extending or integrating OpenCFD. If
you only want to run a case, start with the [project README](../README.md).

| Document | Contents |
|----------|----------|
| [architecture.md](architecture.md) | Case model, module boundaries, the model → Node-API → SDK/CLI flow, determinism and restart semantics, workspace layout |
| [capabilities.md](capabilities.md) | Capability matrix, report and export fields, numerical method, validation map, limitations |
| [cli.md](cli.md) | Full `open-cfd` command reference, plus the developer-only `ocfd` binary |
| [AGENTS.md](../AGENTS.md) | Rules, verified commands and commit style for coding agents |
| [package README](../projects/packages/open-cfd/README.md) | Registry page for `@wae/open-cfd` |

The source tree is the authority. `CAPABILITIES_JSON` in
`projects/crates/cfd-model/src/lib.rs` is the only support claim, and the
command tables in [cli.md](cli.md) are checked against
`projects/packages/open-cfd/src/cli.ts` and
`projects/crates/cfd-model/src/bin/ocfd.rs`.
