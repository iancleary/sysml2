use std::process::Command;
use sysml::{
    Element, ElementKind, Model, Multiplicity, Relationship, RelationshipKind, SCHEMA_VERSION,
};

fn vehicle_model() -> Model {
    let mut model = Model::new("Vehicle");
    model
        .add_element(Element::new(
            "Vehicle",
            "Vehicle",
            ElementKind::PartDefinition,
        ))
        .add_element(Element::new("vehicle", "vehicle", ElementKind::PartUsage))
        .add_element(
            Element::new("engine", "engine", ElementKind::PartUsage)
                .owned_by("vehicle")
                .with_multiplicity(Multiplicity::exactly(1)),
        )
        .add_relationship(Relationship::new(
            "vehicle_type",
            RelationshipKind::FeatureTyping,
            "vehicle",
            "Vehicle",
        ));
    model
}

#[test]
fn round_trips_a_valid_text_model() {
    let model = vehicle_model();

    let text = model.to_toml_string().expect("model should serialize");
    let parsed = Model::from_toml_str(&text).expect("serialized model should parse");

    assert_eq!(parsed, model);
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);
    assert_eq!(parsed.summary().definitions, 1);
    assert_eq!(parsed.summary().usages, 2);
}

#[test]
fn loads_the_checked_in_vehicle_example() {
    let model = Model::load("examples/vehicle.sysml.toml").expect("example should be valid");

    assert_eq!(model.name, "Vehicle");
    assert_eq!(model.summary().elements, 11);
    assert_eq!(model.summary().relationships, 6);
}

#[test]
fn reports_all_dangling_relationship_endpoints() {
    let mut model = vehicle_model();
    model.add_relationship(Relationship::new(
        "missing_flow",
        RelationshipKind::Flow,
        "missing_source",
        "missing_target",
    ));

    let error = model.validate().expect_err("dangling references must fail");
    let messages = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>();

    assert!(messages.contains(&"unknown element id \"missing_source\""));
    assert!(messages.contains(&"unknown element id \"missing_target\""));
}

#[test]
fn rejects_invalid_feature_typing_roles() {
    let mut model = vehicle_model();
    model.relationships[0].sources = vec!["Vehicle".into()];
    model.relationships[0].targets = vec!["vehicle".into()];

    let error = model
        .validate()
        .expect_err("typing direction must be checked");

    assert!(error.issues.iter().any(|issue| {
        issue
            .message
            .contains("feature_typing must relate a usage to a definition")
    }));
}

#[test]
fn rejects_ownership_cycles() {
    let mut model = vehicle_model();
    model.elements[1].owner = Some("engine".into());

    let error = model.validate().expect_err("ownership cycles must fail");

    assert!(error
        .issues
        .iter()
        .any(|issue| issue.message == "ownership cycle detected"));
}

#[test]
fn rejects_inverted_multiplicity_bounds() {
    let mut model = vehicle_model();
    model.elements[2].multiplicity = Some(Multiplicity::bounded(4, 2));

    let error = model
        .validate()
        .expect_err("inverted multiplicity bounds must fail");

    assert!(error
        .issues
        .iter()
        .any(|issue| issue.message == "lower bound 4 exceeds upper bound 2"));
}

#[test]
fn attribute_usage_constructor_is_referential() {
    let attribute = Element::new("mass", "mass", ElementKind::AttributeUsage);

    assert_eq!(attribute.ownership, sysml::UsageOwnership::Reference);
}

#[test]
fn minimal_cli_validates_a_model_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("examples/vehicle.sysml.toml")
        .output()
        .expect("CLI should run");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("stdout should be UTF-8"),
        "Vehicle: 11 elements (5 definitions, 6 usages), 6 relationships\n"
    );
}
