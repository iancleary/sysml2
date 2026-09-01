# Agent Operating Loop

This repository is easiest to extend when each change preserves the product
boundary first, then grows coverage through explicit SysML examples.

## Orientation

`sysml2` publishes the `sysml2` crate, a `sysml` Rust library, and a `sysml`
CLI. The only user-owned model format is standard `.sysml` text.

The main implementation split is:

| Surface | File | Ownership |
| --- | --- | --- |
| Syntax checking | `src/check.rs` | `.sysml` discovery, tree-sitter parsing, syntax diagnostics, and `sysml check` report schema |
| Lowered project model | `src/project.rs` | private parser-neutral elements, imports, references, spans, and profile-boundary issues |
| Requirements profile | `src/validate.rs` | `sysml-2.0-requirements-structure-v1`, name resolution, semantic diagnostics, and validation report schema |
| CLI | `src/main.rs` | argument parsing, exit statuses, human output, and JSON output |
| Compatibility docs | `docs/cli.md`, `docs/adr/` | public command contracts, claim boundaries, and profile decisions |

Use the ADRs before making a large change:

- ADR 0001: `sysml check` is syntax-only and parser crates are replaceable.
- ADR 0002: standard `.sysml` text is the sole persisted model format.
- ADR 0003: requirements validation is a pinned, bounded SysML 2.0 profile.

## Change Loop

1. Start from the contract. Decide whether the change touches syntax checking,
   semantic profile behavior, CLI output, JSON schemas, or release workflow.
2. Add or update the smallest representative SysML fixture first when behavior
   changes. Prefer one-fault negative examples and focused positive examples.
3. Update implementation in the owning module. Keep parser-neutral structures
   private unless a separate public API decision is accepted.
4. Align docs with the observed behavior. Do not broaden conformance language
   unless the code and corpus prove the broader claim.
5. Run the lightest check that covers the change, and run `git diff --check`
   before committing.

## Accretive Coverage

Behavioral additions should leave future agents with a new foothold:

- new syntax behavior: add CLI or unit coverage showing the accepted or
  rejected `.sysml` form and keep `validation_level: "syntax"` honest;
- new profile rule: add positive and one-fault negative coverage, document the
  diagnostic code, and update ADR 0003 if the claim boundary changes;
- new resolution behavior: add a focused `tests/validate_cli.rs` case unless a
  corpus fixture communicates the rule more clearly;
- new public output field, exit behavior, or diagnostic code: update
  `docs/cli.md` and treat compatibility as part of the change;
- new release behavior: update `docs/release.md`, `AGENTS.md`, and
  `CLAUDE.md` together.

The tests under `tests/corpus/requirements/` are the durable examples for the
requirements profile. Keep them small and readable; prefer adding a new file
over mutating a fixture that already explains a different rule.

## Claim Boundaries

Keep these boundaries visible in code, docs, and tests:

- `sysml check` reports parser syntax coverage only.
- `sysml validate` requires an explicit profile.
- The current profile is pinned to the official SysML 2.0 `2026-04` release at
  commit `9baca5908ca28b53da085de69336fde48420ea8f` and metamodel `20250201`.
- The profile does not load standard libraries automatically, evaluate
  expressions, prove satisfaction, execute verification cases, capture
  evidence, enforce application policy, claim SysML 2.1 behavior, or claim full
  OMG SysML conformance.
- The lowered project representation is an implementation detail, not a public
  graph API or persisted serialization contract.

## Verification Guide

Use `just check` for changes to Rust code, CLI behavior, JSON contracts,
validation rules, release workflow, or packaging.

Narrower checks are acceptable for documentation-only changes:

```bash
git diff --check
```

For focused behavior changes, useful smaller checks include:

```bash
cargo test --test check_cli
cargo test --test validate_cli
cargo test --test requirements_corpus
cargo test --test validate_api
cargo doc --all-features --no-deps
```
