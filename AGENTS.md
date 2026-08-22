# AGENTS.md - sysml2

Rust CLI and library for checking standard SysML v2 textual models.

## Commands

```bash
cargo run -- check examples/vehicle.sysml
cargo run -- validate --profile sysml-2.0-requirements-structure-v1 path/to/model
just check
just cut-release --dry-run --version <semver> --notes-file <path>
```

## Releases

Maintain the deterministic release workflow with `create-release-process`.
Execute ordinary releases with `cut-release` via `just cut-release`; see
`docs/release.md` for the repo-local contract. The runner requires an explicit
SemVer `--version`, supports read-only version queries, and creates the GitHub
release.

## Notes

- Standard `.sysml` text is the sole persisted model and authoring format.
- The legacy TOML graph, positional command, and public graph API were removed
  under ADR 0002. Do not restore or replace them with another authoring format.
- Keep the `sysml check --format json` schema and exit-status contract
  backward compatible within a report schema version.
- Keep `sysml check` syntax-only. Semantic rules belong behind an explicit
  `sysml validate --profile <id>` selection.
- The `sysml-2.0-requirements-structure-v1` profile is pinned to the official
  2026-04 release at commit
  `9baca5908ca28b53da085de69336fde48420ea8f` and metamodel `20250201`.
  SysML 2.1 behavior and application-specific policy are outside that profile.
- Keep the `sysml validate --format json` schema and diagnostic codes backward
  compatible within validation report schema version 1. A new or materially
  changed rule set needs a new documented profile ID.
- Do not claim complete OMG SysML textual-notation conformance until the full
  KerML and SysML grammar and semantic constraints are implemented.
- Keep the parser-neutral semantic representation private and replaceable
  until a separate public API decision is accepted.
- Treat syntax parser crates as replaceable implementation dependencies; the
  CLI, diagnostics, validation levels, and compatibility policy belong here.
- Run `just check` for changes to the model, text format, CLI, or release flow.
- Keep `AGENTS.md`, `CLAUDE.md`, and `docs/release.md` consistent when changing
  repository workflows.
