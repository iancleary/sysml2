# sysml2

`sysml2` is a CLI-first, headless toolchain for standard SysML v2 textual
models. The crates.io package is `sysml2`; the Rust library and installed
executable are both named `sysml`.

Standard `.sysml` text is the project's sole persisted model and authoring
format. The CLI recursively checks syntax or applies an explicitly selected,
bounded semantic profile. Both commands can produce deterministic JSON for
CI:

```bash
cargo run -- check examples/vehicle.sysml
cargo run -- check --format json examples/vehicle.sysml
cargo run -- validate --profile sysml-2.0-requirements-structure-v1 path/to/model
```

`sysml check` reports syntax coverage only. `sysml validate` currently
implements one requirements-structure profile; it does **not** evaluate
expressions, prove satisfaction or verification results, apply
application-specific policy, or claim complete OMG SysML v2 conformance. The
CLI contract and exit statuses are documented in
[`docs/cli.md`](docs/cli.md). The product and replaceable-parser boundary is recorded in
[`docs/adr/0001-cli-first-toolchain.md`](docs/adr/0001-cli-first-toolchain.md).

## Standard text checking

Check one file or a directory tree:

```bash
cargo run -- check path/to/model
```

Directories are searched recursively for `.sysml` files. Human-readable
diagnostics are the default; use `--format json` for the versioned report
contract. The initial frontend is `tree-sitter-sysml`, but parser choice is an
internal implementation detail.

The Rust library exposes the same syntax checker and report types:

```rust
use std::path::PathBuf;
use sysml::check_paths;

let report = check_paths(&[PathBuf::from("examples/vehicle.sysml")])?;
assert!(report.valid);
# Ok::<(), sysml::CheckError>(())
```

## Requirements structure validation

Validate one file or a multi-file directory tree against the pinned SysML 2.0
requirements structure profile:

```bash
cargo run -- validate \
  --profile sysml-2.0-requirements-structure-v1 \
  --format json \
  path/to/model
```

The Rust API requires the same explicit profile selection:

```rust
use std::path::PathBuf;
use sysml::{validate_paths, ValidationProfileId};

let paths = [PathBuf::from("path/to/model")];
let report = validate_paths(
    &paths,
    ValidationProfileId::RequirementsStructureV1,
)?;
assert!(report.valid);
# Ok::<(), sysml::CheckError>(())
```

The profile runs syntax checking first, then resolves the provided project and
checks a documented subset of requirement, satisfaction, and verification
structure. Syntax errors suppress semantic checking to avoid cascaded
diagnostics. The exact supported rules, standards baseline, and exclusions are
recorded in
[`docs/adr/0003-requirements-structure-validation-profile.md`](docs/adr/0003-requirements-structure-validation-profile.md).

## Persisted-model decision

Earlier alpha source contained a custom `*.sysml.toml` graph, a positional
validation command, and a public element/relationship graph API. They were
removed together as an intentional breaking alpha change. Standard `.sysml`
is not translated to or authored through a second project-owned format.

Semantic checking uses a private parser-neutral representation for project
loading, resolution, and validation. That representation is replaceable and
has no persisted-format or public serialization promise. See
[`docs/adr/0002-standard-sysml-only-persisted-model.md`](docs/adr/0002-standard-sysml-only-persisted-model.md).

## Specification source

The implemented requirements profile is pinned to the formal [OMG Systems
Modeling Language v2.0](https://www.omg.org/spec/SysML/2.0/) and the official
[2026-04 release](https://github.com/Systems-Modeling/SysML-v2-Release/releases/tag/2026-04)
at commit
[`9baca5908ca28b53da085de69336fde48420ea8f`](https://github.com/Systems-Modeling/SysML-v2-Release/commit/9baca5908ca28b53da085de69336fde48420ea8f),
using metamodel version `20250201`. SysML 2.1 changes in later incremental
releases are intentionally outside this profile. Compatibility of the syntax
frontend with the complete pinned release corpus has not yet been established.
A downloaded development copy may be kept under `docs/`; it is ignored by Git
and excluded from crates.io packages.

## Development

```bash
just install
cargo run -- check examples/vehicle.sysml
cargo run -- validate --profile sysml-2.0-requirements-structure-v1 path/to/model
just check
just cut-release --dry-run --version <semver> --notes-file /tmp/sysml-notes.md
```

`just install` installs the local checkout's `sysml` executable through Cargo.
Paths passed to `sysml check` are resolved relative to the directory where the
command is run.

The release workflow is documented in [`docs/release.md`](docs/release.md).
Future agent contributors should also read
[`docs/agent-operating-loop.md`](docs/agent-operating-loop.md) before changing
the CLI contract, validation profile, corpus, or release workflow.
