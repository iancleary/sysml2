# sysml2

`sysml2` provides typed Rust building blocks and a human-editable TOML format
for defining system models. The Rust library and executable are both named
`sysml`.

The initial API follows the major concept families in the OMG SysML v2.0
Language Specification: attributes, occurrences, items, parts, ports,
connections, interfaces, allocations, flows, actions, states, calculations,
constraints, requirements, cases, views, and metadata. Definitions and usages
are represented as elements; specialization, typing, structural, behavioral,
requirements, and presentation links are represented as relationships.

This crate does **not** yet parse or emit the complete standard `.sysml`
textual notation. Version 0.1 uses an explicit `*.sysml.toml` schema so models
can be edited, reviewed, validated, and versioned as ordinary text files while
the standards-compliant parser is developed separately.

## Example model

```toml
schema_version = 1
name = "Vehicle"

[[elements]]
id = "Vehicle"
name = "Vehicle"
kind = "part_definition"

[[elements]]
id = "vehicle"
name = "vehicle"
kind = "part_usage"

[[elements]]
id = "engine"
name = "engine"
kind = "part_usage"
owner = "vehicle"

[[relationships]]
id = "vehicle_type"
kind = "feature_typing"
sources = ["vehicle"]
targets = ["Vehicle"]
```

Load and validate the model:

```rust
use sysml::Model;

let model = Model::load("examples/vehicle.sysml.toml")?;
println!("{:?}", model.summary());
# Ok::<(), sysml::ModelError>(())
```

Or validate it from the command line:

```bash
cargo run -- examples/vehicle.sysml.toml
```

## Programmatic construction

```rust
use sysml::{Element, ElementKind, Model, Relationship, RelationshipKind};

let mut model = Model::new("Vehicle");
model
    .add_element(Element::new(
        "Vehicle",
        "Vehicle",
        ElementKind::PartDefinition,
    ))
    .add_element(Element::new(
        "vehicle",
        "vehicle",
        ElementKind::PartUsage,
    ))
    .add_relationship(Relationship::new(
        "vehicle_type",
        RelationshipKind::FeatureTyping,
        "vehicle",
        "Vehicle",
    ));

model.validate()?;
# Ok::<(), sysml::ValidationError>(())
```

## Validation

Validation currently checks:

- supported schema version and non-empty names;
- globally unique element and relationship identifiers;
- owner and relationship references;
- ownership cycles and multiplicity bounds;
- relationship arity; and
- definition/usage roles for typing, subclassification, subsetting,
  redefinition, requirements, and use-case inclusion.

## Specification source

The model taxonomy is based on the formal [OMG Systems Modeling Language v2.0,
Part 1: Language Specification](https://www.omg.org/spec/SysML/2.0/Language/PDF)
(`formal/2026-03-02`, March 2026). A downloaded development copy may be kept
under `docs/`; it is ignored by Git and excluded from crates.io packages so the
public project links to the authoritative OMG document instead of redistributing
it.

## Development

```bash
just check
just cut-release --dry-run --version 0.1.0 --notes-file /tmp/sysml-notes.md
```

The release workflow is documented in [`docs/release.md`](docs/release.md).
