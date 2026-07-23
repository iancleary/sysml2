# CLAUDE.md - sysml

## Overview

Rust crate for text-file-backed system models based on the major concept and
relationship families in OMG SysML v2. The crates.io package is `sysml2`; the
Rust library and executable names are `sysml`.

Version 0.1 uses a documented `*.sysml.toml` schema. It does not claim to parse
the complete standard `.sysml` textual notation.

## Commands

```bash
cargo run -- examples/vehicle.sysml.toml
just check
just cut-release --dry-run --version <semver> --notes-file <path>
```

## Module Map

| Module | File | Description |
| --- | --- | --- |
| `model` | `src/model.rs` | Elements, relationships, multiplicity, validation |
| `text` | `src/text.rs` | TOML parse, serialization, load, and save |
| `error` | `src/error.rs` | Parse, I/O, serialization, and validation errors |
| CLI | `src/main.rs` | Minimal model-file validator and summary |

## Releases

Maintain the release workflow with `create-release-process`. Execute ordinary
releases with `cut-release` through `just cut-release`; see `docs/release.md`.
