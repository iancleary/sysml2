# Release Process

This repo has a deterministic local release runner at `scripts/cut-release.sh`.
Use `just cut-release` as the normal entrypoint.

## Versioning

The crate version is SemVer and lives in the root `Cargo.toml`. The next version
is not inferred because the repo has no checked-in bump policy. Pass the
intended version explicitly with `--version`.

Read-only queries:

```bash
just cut-release --print-current-version
just cut-release --print-next-version --version 0.1.0
```

The first release may tag the version already present in `Cargo.toml`. Later
releases update `Cargo.toml` and `Cargo.lock` before tagging.

## Dry Run

```bash
just cut-release --dry-run --version 0.1.0 --notes-file /tmp/sysml-notes.md
```

The dry run operates on a temporary archive of `HEAD`, validates the requested
version and package, and does not edit files, create commits, create tags, push,
publish, or create a GitHub release.

## Real Release

Prepare release notes in a local Markdown file, then run from the default branch
with a clean working tree:

```bash
just cut-release --version 0.1.0 --notes-file /tmp/sysml-notes.md
```

The runner updates the package version when needed, runs formatting, clippy,
tests, and packaging, commits a version change when one exists, creates an
annotated `v<version>` tag, pushes the default branch and tag, and creates the
GitHub release using `gh release create`.

Publishing to crates.io is handled by GitHub Actions. The published GitHub
release event runs CI and then `cargo publish --verbose` with the repository's
`CARGO_REGISTRY_TOKEN` Actions secret. Configure that secret before publishing
the first release.

## Agent Routing

Use `create-release-process` when maintaining this workflow. Use `cut-release`
when executing an ordinary release through the checked-in runner.
