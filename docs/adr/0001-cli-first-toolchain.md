# ADR 0001: CLI-first SysML toolchain

- Status: accepted for the CLI-first and parser-boundary decisions; TOML
  retention superseded by ADR 0002
- Date: 2026-07-24

## Context

The crate began as typed Rust building blocks plus a deterministic
`*.sysml.toml` graph. The first real consumer is a downstream
architecture repository authored in standard `.sysml` text. That consumer
needs a locally installable, headless command for CI and a later path to
machine-readable model extraction.

Calling the product only a "validator" would prematurely constrain it and
could imply complete OMG conformance before syntax, name resolution, standard
libraries, semantic constraints, and verification rules are implemented.

Two Rust syntax frontends were evaluated against the three initial downstream
models:

- `tree-sitter-sysml` 0.1.0 parsed all three without error;
- `sysml-v2-parser` 0.47.0 parsed two, but rejected a standards-valid interface
  usage containing both a short name and a declared name.

The tree-sitter grammar deliberately over-accepts some context-sensitive
forms. Therefore a clean syntax tree alone cannot establish semantic
conformance.

## Decision

`sysml2` is a CLI-first, headless SysML v2 toolchain. The installed executable
is `sysml`.

The first standard-text command is:

```text
sysml check [--format human|json] <path>...
```

The crate owns:

- command and exit-status behavior;
- the versioned JSON diagnostic contract;
- validation-level labels and conformance claims;
- semantic lowering into a private, tool-owned intermediate model;
- compatibility policy for automation consumers.

The syntax parser is an internal, replaceable dependency.
`tree-sitter-sysml` is the initial frontend because it handles the dogfood
corpus and provides a lossless, error-tolerant syntax tree. Reports explicitly
state `validation_level: "syntax"`.

Historical decision (superseded): ADR 0001 originally retained the TOML graph
and Rust model as candidates for an intermediate representation.
[ADR 0002](0002-standard-sysml-only-persisted-model.md) supersedes that
paragraph; the graph, positional command, and public model API are removed and
unsupported.

The syntax command is named `check`, not `validate`. The reserved `validate`
command was subsequently activated for the explicit bounded profile in
[ADR 0003](0003-requirements-structure-validation-profile.md).

## Consequences

- A downstream repository can use the CLI immediately without claiming that
  its models are semantically conformant.
- Parser dependencies can change without breaking CLI consumers.
- Semantic work can proceed incrementally: lowering, symbol resolution,
  library loading, rule checks, and domain-specific exports.
- Every new validation level requires explicit documentation and corpus tests.
- Syntax accepted by tree-sitter but forbidden by SysML body or semantic rules
  remains a known gap until the corresponding owned check exists.

## Standard and implementation sources

- [OMG SysML 2.0 specification](https://www.omg.org/spec/SysML/2.0/)
- [Official SysML v2 2026-04 release](https://github.com/Systems-Modeling/SysML-v2-Release/releases/tag/2026-04)
- [Official pilot Xtext grammar](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation/blob/master/org.omg.sysml.xtext/src/org/omg/sysml/xtext/SysML.xtext)
- [`tree-sitter-sysml`](https://crates.io/crates/tree-sitter-sysml/0.1.0)
- [`sysml-v2-parser`](https://crates.io/crates/sysml-v2-parser/0.47.0)
