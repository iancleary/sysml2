# CLI contract

The installed executable is `sysml`.

## Syntax checking

```text
sysml check [--format human|json] <path>...
```

Each path may be a `.sysml` file or a directory. Directories are searched
recursively, directory symlinks encountered during recursion are not followed,
and discovered files are sorted before checking. Other files inside a
directory are ignored; an explicitly supplied non-`.sysml` file is rejected.

Version 1 checks textual syntax only. A successful result is not a claim of
complete SysML v2 semantic or OMG conformance.

### Output

Human output is the default. Diagnostics use 1-based line numbers and 1-based
UTF-8 byte columns:

```text
model.sysml:4:9: error[syntax.error]: invalid or unexpected SysML syntax
```

`--format json` writes one deterministic report to standard output:

```json
{
  "schema_version": 1,
  "tool_version": "0.2.0-alpha.1",
  "validation_level": "syntax",
  "valid": true,
  "files": [
    {
      "path": "model.sysml",
      "valid": true,
      "diagnostics": []
    }
  ]
}
```

Each diagnostic contains `severity`, `code`, `message`, and a `span` with
1-based lines and 1-based UTF-8 byte columns. Parser implementation details are
not part of this contract.

### Exit status

| Status | Meaning |
| --- | --- |
| `0` | Every discovered SysML file passed the implemented checks |
| `1` | One or more model diagnostics were reported |
| `2` | Invocation, input discovery, I/O, or tool execution failed |

Invocation and I/O failures are written to standard error, including when JSON
output was requested. They are not model diagnostics.

## Requirements structure validation

```text
sysml validate --profile sysml-2.0-requirements-structure-v1 [--format human|json] <path>...
```

`--profile` is required; there is no implicit semantic profile. File discovery
has the same behavior as `sysml check`. The implemented profile is pinned to:

| Field | Value |
| --- | --- |
| Profile ID | `sysml-2.0-requirements-structure-v1` |
| Language | `SysML` |
| Language version | `2.0` |
| Official source release | `2026-04` |
| Release commit | `9baca5908ca28b53da085de69336fde48420ea8f` |
| Metamodel version | `20250201` |

Validation first runs the syntax pass. If any syntax diagnostic is present,
semantic processing is skipped for the entire input set so that malformed
trees do not cause cascaded semantic diagnostics. Otherwise, the command
lowers the supplied files into a private project model, resolves references
within that input set, and checks the bounded requirement, satisfaction, and
verification rules listed in
[ADR 0003](adr/0003-requirements-structure-validation-profile.md).
The current parser accepts the semicolon form of inline `verify requirement`
but not its official body form; the latter reports a syntax diagnostic and is
documented as a parser coverage gap in ADR 0003.
The syntax and semantic passes use one in-memory source snapshot. Membership
imports and non-recursive, unfiltered wildcard imports without the `all`
modifier are supported. Import visibility must be explicit: private imports
are local to their declaring namespace, while public imports can be found
through qualified lookup and re-exported. Top-level imports must be private.
Missing visibility or a non-private top-level import reports
`semantic.import.visibility`; protected imports, imports with the `all`
modifier, and recursive or filtered imports report
`semantic.profile.unsupported_import` and do not participate in resolution
under this profile.
Explicit private or protected visibility on a modeled non-import membership
reports `semantic.profile.unsupported_visibility`; default and explicit public
membership visibility remain supported.

The profile does not automatically load the standard libraries. It does not
evaluate expressions, prove constraint truth, treat `satisfy` or `verify` as
verification evidence, apply application-specific ID/owner/status policy, or
claim full OMG SysML conformance.

### Output

Human diagnostics use the same format and source-coordinate convention as
`sysml check`. A valid run ends with the selected profile ID:

```text
validated 2 SysML files: sysml-2.0-requirements-structure-v1
```

`--format json` writes a validation report with its own versioned schema:

```json
{
  "schema_version": 1,
  "tool_version": "0.2.0-alpha.1",
  "profile": {
    "id": "sysml-2.0-requirements-structure-v1",
    "language": "SysML",
    "language_version": "2.0",
    "source_release": "2026-04",
    "source_commit": "9baca5908ca28b53da085de69336fde48420ea8f",
    "metamodel_version": "20250201"
  },
  "valid": true,
  "files": [
    {
      "path": "requirements.sysml",
      "valid": true,
      "diagnostics": []
    }
  ]
}
```

The top-level fields are `schema_version`, `tool_version`, `profile`, `valid`,
and `files`. Profile metadata contains `id`, `language`, `language_version`,
`source_release`, `source_commit`, and `metamodel_version`. Each file contains
`path`, `valid`, and `diagnostics`. Each diagnostic contains `severity`, `code`,
`message`, and `span`; a span contains `start_line`, `start_column`, `end_line`,
and `end_column`. Paths and diagnostics are emitted deterministically. The
`sysml check` JSON schema is unchanged and continues to use
`validation_level: "syntax"` rather than a `profile` object.

### Exit status

| Status | Meaning |
| --- | --- |
| `0` | Every discovered SysML file passed syntax and the selected profile |
| `1` | One or more syntax, resolution, or profile diagnostics were reported |
| `2` | Invocation, unsupported profile, input discovery, I/O, or tool execution failed |

Invocation and I/O failures are written to standard error, including when JSON
output was requested. They are not embedded in a validation report.
