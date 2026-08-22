# ADR 0002: Standard SysML is the only persisted model format

- Status: accepted and implemented
- Date: 2026-07-25
- Supersedes: the TOML-retention decision in ADR 0001

## Context

The first product consumer was a downstream repository. Its architecture
sources are standard `.sysml` files. There are
no known consumers of the crate's earlier `*.sysml.toml` graph format or its
public Rust graph-building API.

Maintaining both authoring surfaces would add compatibility work without
improving downstream model feedback. The graph also predates
the required multi-file project, package/import resolution, source
provenance, allocation, deployment, and interface-validation model.

Semantic checking will still require an internal representation after
parsing and name resolution. That need does not justify retaining the old
graph as a public API or alternate persisted syntax.

## Decision

Standard `.sysml` text is the only persisted model and authoring format owned
by `sysml2`.

In one explicit breaking alpha change, remove:

- the `*.sysml.toml` schema and parser;
- the positional legacy CLI command;
- the exported `Model`, `ModelSummary`, element, relationship, multiplicity,
  direction, ownership, `SCHEMA_VERSION`, and model/validation error types,
  together with all graph construction, validation, and TOML I/O methods;
- legacy examples, tests, dependencies, documentation, and compatibility
  obligations.

This is removal, not deprecation: the deleted format, command, and API are
unsupported. Do not add a compatibility shim or migration utility because
there are no known model consumers to migrate.

The tool may introduce a parser-neutral semantic representation for project
loading, resolution, validation, traceability, and generic export. Initially
that representation is private and replaceable. It is not a second authoring
format or a replacement public graph API, and it carries no public
serialization-compatibility promise.

This decision does not change the separately versioned
`sysml check --format json` diagnostic report or its exit-status contract.
Any later change to that contract requires its own compatibility decision.

## Consequences

- Users and documentation have one source format to understand.
- Semantic structures can be designed around resolved SysML rather than the
  legacy graph.
- The removal must be called out in release notes as an intentional breaking
  alpha change.
- A downstream repository can exercise unreleased sibling builds during rapid
  iteration and pin small `sysml2` releases for repeatable checks.
- Domain-specific policies and ICD projections remain outside this
  reusable tool.

## Repository strategy

Keep `sysml2` independent from downstream consumers. Consumers may use local
sibling builds during development and pin releases for repeatable checks.
Reconsider colocation only if version boundaries materially impede development.
