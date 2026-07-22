# AGENTS.md - sysml

Rust crate for text-file-backed SysML v2 system models.

## Commands

```bash
cargo run -- examples/vehicle.sysml.toml
just check
just cut-release --dry-run --version <semver> --notes-file <path>
```

## Releases

Maintain the deterministic release workflow with `create-release-process`.
Execute ordinary releases with `cut-release` via `just cut-release`; see
`docs/release.md` for the repo-local contract. The runner requires an explicit
SemVer `--version`, supports read-only version queries, and creates the GitHub
release, which triggers the crates.io publish workflow.

## Notes

- Keep the TOML schema backward compatible within a schema version.
- Do not claim complete OMG SysML textual-notation conformance until the full
  KerML and SysML grammar and semantic constraints are implemented.
- Run `just check` for changes to the model, text format, CLI, or release flow.
- Keep `AGENTS.md`, `CLAUDE.md`, and `docs/release.md` consistent when changing
  repository workflows.
