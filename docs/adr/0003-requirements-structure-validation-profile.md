# ADR 0003: Bounded requirements structure validation profile

- Status: accepted and implemented
- Date: 2026-08-07

## Context

`sysml check` deliberately reports syntax only. Architecture consumers also
need early, deterministic feedback for requirement capture across multiple
`.sysml` files, but calling that feedback complete semantic validation would
overstate the implementation. Full SysML semantics require the KerML and SysML
type systems, standard-library loading, expression evaluation, derived
properties, and all normative constraints.

Requirements introduce another important distinction: a structurally valid
`satisfy` assertion or `verify` declaration records model intent. It does not
prove that a constraint is true, show that a verification passed, or constitute
verification evidence.

The official artifacts also contain notation and validation edges that need an
explicit compatibility choice. In the pinned release, the extracted textual
BNF includes mandatory `assert` in `SatisfyRequirementUsage`, while the
official pilot examples accept both `satisfy` and `assert satisfy`. The pilot
validation suite requires a requirement verification to occur in an
`objective`.

## Decision

Add an explicit semantic command and one bounded profile:

```text
sysml validate --profile sysml-2.0-requirements-structure-v1 [--format human|json] <path>...
```

There is no default profile. `sysml check` remains syntax-only, and its report
schema and `validation_level: "syntax"` claim do not change.

The profile identity is:

| Field | Value |
| --- | --- |
| Profile ID | `sysml-2.0-requirements-structure-v1` |
| Language | `SysML` |
| Language version | `2.0` |
| Source release | `2026-04` |
| Source commit | `9baca5908ca28b53da085de69336fde48420ea8f` |
| Metamodel version | `20250201` |

The source release is fixed by commit, not by the moving repository default
branch. SysML 2.1 behavior first appearing in later incremental releases is
outside this profile.

## Processing and diagnostics

The command discovers and sorts `.sysml` inputs using the same rules as
`sysml check`. It runs syntax checking first. If any file has a syntax
diagnostic, semantic lowering and validation are skipped for the entire input
set to prevent cascaded diagnostics from malformed syntax trees.

There is one known parser coverage gap in this release. The official 2026-04
[SysML Xtext grammar](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation/blob/2026-04/org.omg.sysml.xtext/src/org/omg/sysml/xtext/SysML.xtext)
allows a `RequirementBody` on `RequirementVerificationUsage`, but
`tree-sitter-sysml` 0.1.0 at
[commit `07a94a3`](https://gitlab.com/nomograph/tree-sitter-sysml/-/blob/07a94a38c3090a0f730dc2b3ecdcd025d63226be/grammar.js#L1999)
accepts only the semicolon form of inline `verify requirement`. An inline
verification body therefore receives a syntax diagnostic and never reaches
semantic lowering. Profile v1 validates the semicolon form's type and
specialization structure; it does not claim syntax or semantic body coverage
until the parser supports that official production.

For syntax-valid input, the implementation builds a private parser-neutral
project representation. It resolves names through enclosing ownership and
package scopes, library packages, visibility-aware root and scoped membership
imports, non-recursive unfiltered wildcard imports without the `all` modifier,
qualified names, declared types, specialization, and typed feature chains
present in the supplied input set. Direct members take precedence over imports,
imports over inherited
members, and the nearest lexical scope over outer scopes. Private imports
participate in lookup inside their declaring namespace only. Public imports
also participate in qualified lookup and can therefore re-export a membership.
Top-level declarations remain discoverable across all supplied sources, while
a top-level private import belongs to the implicit root namespace of its source
and participates only in lookups originating from that source. Imports owned by
an explicit namespace remain keyed to that owner across files.
A reference used by a profile rule, including an otherwise-unused import
target, produces `resolution.unresolved_reference` or
`resolution.ambiguous_reference` when it cannot resolve uniquely.

Import notation must contain an explicit visibility indicator, and a
top-level import must be private. Violations produce
`semantic.import.visibility`. Protected imports, imports with the `all`
modifier, and recursive or filtered wildcard imports produce
`semantic.profile.unsupported_import`; they are valid language features but
outside this bounded resolver and do not participate in resolution. The
implementation does not automatically load the standard libraries.

Explicit private or protected visibility on a modeled non-import membership
produces `semantic.profile.unsupported_visibility`. Correctly resolving those
members requires caller-ownership and specialization context that this bounded
resolver does not model. Default and explicit public membership visibility are
supported. The profile rejects the unsupported forms instead of treating them
as public or silently accepting an inaccessible qualified or typed-feature
reference.

Relationship bodies on referenced `satisfy` and `verify` forms, multiple or
mixed declared-type/specialization parents on subject-conformance endpoints,
and mixed type/specialization parents or multiple specialization parents on
requirement and verification elements are also valid language surfaces beyond
this slice. They produce
`semantic.profile.unsupported_relationship_body` and
`semantic.profile.unsupported_multityping` rather than being silently treated
as validated.

The profile implements these structural checks:

- requirement declared types have at most one explicit type, and explicit
  types resolve to requirement definitions;
- requirement-definition specializations resolve to requirement definitions,
  while requirement-usage specializations resolve to requirement usages;
  objectives and inline `verify requirement` declarations are composite
  requirement usages for these checks; requirement type and specialization
  parent chains are acyclic;
- a requirement owns at most one directly declared explicit subject; directly
  owned subject type references resolve, while directly owned actor and
  stakeholder types resolve to part definitions;
- requirement-usage subject types and bound subject features conform to the
  declared or inherited requirement-definition subject type;
- typed assumption and required-constraint members resolve to constraint
  definitions, and referenced members resolve to constraint or requirement
  usages;
- shorthand, explicitly asserted, negated, and inline satisfaction forms are
  lowered; each satisfaction target resolves to a requirement usage, including
  an objective;
- an explicitly named satisfying subject resolves to a non-requirement usage,
  so an objective cannot be used as the satisfying subject;
  when the requirement subject type is available, the satisfying subject must
  have a resolvable effective type that is the same type as or a specialization
  of it;
- verification declared types resolve to verification definitions;
  definition specializations resolve to verification definitions and usage
  specializations resolve to verification usages, with at most one explicit
  type on a verification usage; verification type and specialization parent
  chains are acyclic;
- a verification case owns at most one directly declared objective; directly
  owned verification subject references and bindings resolve and conform to
  the declared or inherited verification subject type;
- a `verify` declaration is directly inside an `objective` that is directly
  owned by a verification definition or usage; an inline `verify requirement`
  declaration in the supported semicolon form is lowered once as a requirement
  usage and receives the same requirement type and specialization checks as any
  other requirement usage;
- a referenced verification target resolves to a requirement usage, including
  an objective or a previously named inline requirement in the same objective;
  and
- the effective verification subject type conforms to the verified
  requirement's effective subject type when that requirement type is known.

The stable semantic code families implemented by profile v1 are:

| Area | Diagnostic codes |
| --- | --- |
| Resolution and profile boundary | `resolution.unresolved_reference`, `resolution.ambiguous_reference`, `semantic.import.visibility`, `semantic.profile.unsupported_import`, `semantic.profile.unsupported_visibility`, `semantic.profile.unsupported_relationship_body`, `semantic.profile.unsupported_multityping` |
| Requirement definition and usage | `semantic.requirement.type_cardinality`, `semantic.requirement.type_kind`, `semantic.requirement.specialization_kind`, `semantic.requirement.inheritance_cycle`, `semantic.requirement.subject_cardinality`, `semantic.requirement.subject_binding`, `semantic.requirement.part_parameter_type`, `semantic.requirement.constraint_type_kind`, `semantic.requirement.required_member_kind` |
| Satisfaction | `semantic.satisfaction.target_kind`, `semantic.satisfaction.subject_kind`, `semantic.satisfaction.subject_conformance` |
| Verification case | `semantic.verification.case_type_cardinality`, `semantic.verification.case_type_kind`, `semantic.verification.case_specialization_kind`, `semantic.verification.inheritance_cycle`, `semantic.verification.objective_cardinality`, `semantic.verification.subject_binding`, `semantic.verification.subject_conformance` |
| Verified requirement | `semantic.verification.placement`, `semantic.verification.target_kind` |

The profile does not add a requirement for an explicit subject or objective
where the language permits inherited/default structure. Every implemented
diagnostic is an error and includes a source span with 1-based line and
1-based UTF-8 byte columns.

## Compatibility resolutions

### Satisfaction assertion prefix

The pinned [2026-04 textual
BNF](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/bnf/SysML-textual-bnf.kebnf#L1467-L1474)
spells `SatisfyRequirementUsage` with a mandatory `assert`. The official
[2026-04 pilot requirement
example](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation/blob/2026-04/sysml/src/examples/Simple%20Tests/RequirementTest.sysml)
contains both unprefixed `satisfy` and `assert satisfy` forms. This profile
accepts both forms and does not use presence of the prefix as a semantic
diagnostic. That compatibility choice is bounded to this profile and is not a
claim that the complete notation ambiguity has been resolved.

### Verification placement

The profile follows the official [2026-04 pilot invalid-verification
coverage](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation/blob/2026-04/org.omg.sysml.xpect.tests/src/org/omg/sysml/xpect/tests/validation/invalid/Verification_invalid.sysml.xt)
by requiring each `verify` declaration to be directly owned by an `objective`
of a verification definition or usage. This is a structural placement check
only; it does not execute a verification method or establish a pass result.

### Import visibility

The profile follows KerML 1.0 clauses 7.2.5.4, 8.2.3.4.2, and
8.3.2.4.2-8.3.2.4.5 for the import-visibility subset. In particular, the
textual notation requires the visibility indicator to be shown, top-level
imports are private, a private import makes names available locally without
re-exporting them, and a public import contributes visible memberships for
qualified lookup. The pinned [2026-04 KerML textual
BNF](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/bnf/KerML-textual-bnf.kebnf)
also makes `VisibilityIndicator` mandatory in `Import`.

Protected imports are valid KerML and can expose imported memberships to
specializing types. This profile does not yet model that visibility relation,
so a protected import is rejected with
`semantic.profile.unsupported_import` and does not participate in resolution.
This is an explicit claim boundary, not an interpretation of protected as
private.

The KerML `all` modifier changes an import's `isImportAll` semantics. This
profile does not yet model that distinction, so imports using `all` are also
rejected with `semantic.profile.unsupported_import` and excluded from lookup.

## Machine-readable contract

Validation JSON report schema version 1 contains:

- `schema_version`, `tool_version`, `profile`, `valid`, and `files` at the top
  level;
- `id`, `language`, `language_version`, `source_release`, `source_commit`, and
  `metamodel_version` in `profile`;
- `path`, `valid`, and `diagnostics` for each file; and
- `severity`, `code`, `message`, and `span` for each diagnostic, with
  `start_line`, `start_column`, `end_line`, and `end_column` in each span.

Report schema version 1 and diagnostic codes are automation contracts. An
incompatible schema change requires a schema-version change. A materially
different semantic rule set requires a separately documented profile ID.

The exit statuses are:

| Status | Meaning |
| --- | --- |
| `0` | All supplied models pass syntax and the selected profile |
| `1` | At least one syntax, resolution, or profile diagnostic is reported |
| `2` | Invocation, unsupported profile, discovery, I/O, or tool execution fails |

Invocation and I/O failures go to standard error rather than into a model
report.

## Exclusions and claim boundary

This profile does not implement or claim:

- expression evaluation, constraint truth, derived-property computation, or
  proof of satisfaction;
- execution of verification cases, pass/fail results, evidence capture, or
  verification closure;
- the complete KerML/SysML type system, all normative semantic constraints,
  or full textual-notation coverage;
- automatic standard-library loading;
- alias resolution, protected imports, imports using the `all` modifier,
  recursive or filtered wildcard imports, private or protected non-import
  membership semantics, and the complete SysML namespace/import semantics
  beyond the subset above (explicit unsupported forms are rejected as
  documented above);
- semantic lowering of bodies on referenced satisfaction or verification
  relationships, and multi-typing on subject-conformance endpoints;
- application policy such as requirement-ID format or uniqueness, immutable
  IDs, owners, row colors, lifecycle state, approval, or traceability policy;
  or
- SysML 2.1 compatibility or full OMG SysML conformance.

Application-specific policy belongs in the consuming repository or a separate
explicit profile. Passing this profile means only that the input passed the
implemented syntax and structural rules identified above.

## Consequences

- CI consumers can distinguish syntax checks from a pinned semantic subset.
- Diagnostic reports state the exact profile and standards baseline used.
- Requirement capture can be checked incrementally without presenting
  structural assertions as engineering evidence.
- Additional semantic coverage must be added deliberately, with corpus tests
  and an honest compatibility decision for the profile identifier.
- The private project representation can evolve without creating another
  persisted model format or public serialization contract.

## Official sources

- [OMG SysML 2.0 formal specification](https://www.omg.org/spec/SysML/2.0/)
- [OMG KerML 1.0 formal specification](https://www.omg.org/spec/KerML/1.0/PDF)
- [Official SysML v2 release 2026-04](https://github.com/Systems-Modeling/SysML-v2-Release/releases/tag/2026-04)
  and pinned [release
  commit](https://github.com/Systems-Modeling/SysML-v2-Release/commit/9baca5908ca28b53da085de69336fde48420ea8f)
- Pinned textual standard libraries for
  [Requirements](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/sysml.library/Systems%20Library/Requirements.sysml)
  and
  [VerificationCases](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/sysml.library/Systems%20Library/VerificationCases.sysml)
- [Official SysML v2 Pilot Implementation 2026-04](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation/releases/tag/2026-04)
- Pinned release examples for
  [requirements and satisfaction](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/sysml/src/examples/Simple%20Tests/RequirementTest.sysml)
  and
  [verification](https://github.com/Systems-Modeling/SysML-v2-Release/blob/9baca5908ca28b53da085de69336fde48420ea8f/sysml/src/examples/Simple%20Tests/VerificationTest.sysml)
