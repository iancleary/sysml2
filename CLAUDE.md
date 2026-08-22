# CLAUDE.md - sysml

## Overview

CLI-first Rust toolchain for standard OMG SysML v2 textual models. The
crates.io package is `sysml2`; the Rust library and executable names are
`sysml`.

The `sysml check` command checks standard `.sysml` syntax and owns a versioned
JSON diagnostic contract. `sysml validate` applies an explicit, documented
semantic profile; the current
`sysml-2.0-requirements-structure-v1` profile is bounded to requirement,
satisfaction, and verification structure. Standard `.sysml` is the sole
persisted model and authoring format. Neither command claims complete OMG
conformance.

## Commands

```bash
cargo run -- check examples/vehicle.sysml
cargo run -- validate --profile sysml-2.0-requirements-structure-v1 path/to/model
just check
just cut-release --dry-run --version <semver> --notes-file <path>
```

## Module Map

| Module | File | Description |
| --- | --- | --- |
| `check` | `src/check.rs` | Standard `.sysml` discovery, syntax diagnostics, and JSON report contract |
| `project` | `src/project.rs` | Private parser-neutral lowering and project/reference model |
| `validate` | `src/validate.rs` | Requirements structure profile, semantic diagnostics, and validation report contract |
| CLI | `src/main.rs` | `check` and `validate` argument parsing, human output, and JSON output |

ADR 0002 removed the legacy TOML graph, positional command, and public graph
API. The parser-neutral semantic representation remains private until a
separate API decision is accepted.

Keep `sysml check` syntax-only and preserve its JSON schema. The requirements
profile is pinned to SysML 2.0 release `2026-04`, commit
`9baca5908ca28b53da085de69336fde48420ea8f`, metamodel `20250201`; it excludes
expression evaluation, proof or evidence claims, application policy, full OMG
conformance, and SysML 2.1 behavior. See
[`docs/adr/0003-requirements-structure-validation-profile.md`](docs/adr/0003-requirements-structure-validation-profile.md).

## Releases

Maintain the release workflow with `create-release-process`. Execute ordinary
releases with `cut-release` through `just cut-release`; see `docs/release.md`.
