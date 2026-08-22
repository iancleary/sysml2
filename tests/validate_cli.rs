use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const REQUIREMENTS_PROFILE: &str = "sysml-2.0-requirements-structure-v1";
static NEXT_TEMP_PROJECT_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let sequence = NEXT_TEMP_PROJECT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "sysml-requirements-validation-{}-{unique}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary project directory should be created");
        Self { root }
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, source).expect("temporary SysML source should be written");
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn validate(path: &Path, format: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args([
            "validate",
            "--profile",
            REQUIREMENTS_PROFILE,
            "--format",
            format,
        ])
        .arg(path)
        .output()
        .expect("sysml validate should run")
}

fn json_report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout should contain a JSON report")
}

fn diagnostic_codes(report: &Value) -> Vec<&str> {
    report["files"]
        .as_array()
        .expect("files should be an array")
        .iter()
        .flat_map(|file| {
            file["diagnostics"]
                .as_array()
                .expect("diagnostics should be an array")
        })
        .map(|diagnostic| {
            diagnostic["code"]
                .as_str()
                .expect("diagnostic code should be a string")
        })
        .collect()
}

#[test]
fn validates_a_cross_file_requirement_project_with_an_explicit_profile() {
    let project = TempProject::new();
    project.write(
        "architecture.sysml",
        r#"
package Architecture {
    part def FlightComputer;
    part flightComputer : FlightComputer;
}
"#,
    );
    project.write(
        "requirements.sysml",
        r#"
package Requirements {
    private import Architecture::*;

    requirement def <'REQ-COMPUTE-001'> ComputeRequirement {
        doc /* The flight computer shall satisfy the selected compute property. */
        subject computer : FlightComputer;
        assume constraint { true }
        require constraint { true }
    }

    requirement <'REQ-COMPUTE-001-A'> selectedCompute : ComputeRequirement {
        subject computer : FlightComputer;
    }
}
"#,
    );

    let output = validate(&project.root, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let report = json_report(&output);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["profile"]["id"], REQUIREMENTS_PROFILE);
    assert_eq!(report["profile"]["language_version"], "2.0");
    assert_eq!(report["profile"]["source_release"], "2026-04");
    assert_eq!(
        report["profile"]["source_commit"],
        "9baca5908ca28b53da085de69336fde48420ea8f"
    );
    assert_eq!(report["profile"]["metamodel_version"], "20250201");
    assert_eq!(report["valid"], true);
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
}

#[test]
fn validates_the_positive_requirements_corpus_semantically() {
    let corpus =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/requirements/positive");

    let output = validate(&corpus, "json");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(json_report(&output)["valid"], true);
}

#[test]
fn resolves_membership_imports_and_typed_feature_chains() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Requirements {
    private import Architecture::Computer;

    requirement def ComputeRequirement {
        subject computer : Computer;
    }
    part def Context {
        part computer : Computer;
    }
    part context : Context;
    requirement selected : ComputeRequirement;
    satisfy selected by context.computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn a_membership_import_by_long_name_exposes_the_short_name() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def <FC> FlightComputer;
}
package Requirements {
    private import Architecture::FlightComputer;

    requirement def ComputeRequirement {
        actor computer : FC;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn a_membership_import_by_short_name_exposes_the_long_name() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def <FC> FlightComputer;
}
package Requirements {
    private import Architecture::FC;

    requirement def ComputeRequirement {
        actor computer : FlightComputer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn rejects_import_all_and_excludes_it_from_lookup() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Requirements {
    private import all Architecture::*;

    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.profile.unsupported_import",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn resolves_chained_membership_import_targets_in_the_same_scope() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Root {
    package Architecture {
        part def Computer;
    }
}
package Requirements {
    private import Root::Architecture;
    private import Architecture::Computer;

    requirement def ComputeRequirement {
        subject computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn resolves_a_membership_import_exposed_by_a_qualified_namespace() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Facade {
    public import Architecture::Computer;
}
package Requirements {
    private import Facade::Computer;

    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn resolves_a_private_import_inside_its_declaring_namespace() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Facade {
    private import Architecture::Computer;

    requirement def LocalRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn does_not_expose_a_private_import_through_a_qualified_namespace() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Facade {
    private import Architecture::Computer;
}
package Requirements {
    private import Facade::Computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn qualified_direct_members_shadow_imported_memberships() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Imported {
    constraint def Computer;
}
package Facade {
    public import Imported::Computer;
    part def Computer;
}
package Requirements {
    private import Facade::Computer;

    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn root_direct_members_shadow_root_imports() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Imported {
    constraint def Computer;
}
part def Computer;
private import Imported::Computer;
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn top_level_imports_do_not_leak_between_sources() {
    let project = TempProject::new();
    project.write(
        "types.sysml",
        r#"
package Types {
    part def Computer;
}
private import Types::Computer;
"#,
    );
    project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&project.root, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn top_level_imports_remain_visible_within_their_source() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Types {
    part def Computer;
}
private import Types::Computer;
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn top_level_import_targets_chain_within_their_source() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Root {
    package Architecture {
        part def Computer;
    }
}
private import Root::Architecture;
private import Architecture::Computer;
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn top_level_import_targets_do_not_chain_across_sources() {
    let project = TempProject::new();
    project.write(
        "aliases.sysml",
        r#"
package Root {
    package Architecture {
        part def Computer;
    }
}
private import Root::Architecture;
"#,
    );
    project.write(
        "requirements.sysml",
        r#"
private import Architecture::Computer;
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&project.root, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn cross_file_inherited_lookup_uses_the_declaring_sources_root_imports() {
    let project = TempProject::new();
    project.write(
        "architecture.sysml",
        r#"
package Types {
    part def Container {
        part def Computer;
    }
}
private import Types::Container;
part instance : Container;
"#,
    );
    project.write(
        "requirements.sysml",
        r#"
package Requirements {
    private import instance::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&project.root, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn cyclic_top_level_imports_do_not_poison_a_same_source_sibling() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Origin {
    part def Computer;
}
private import Architecture::Computer;
private import Computer::Architecture;
private import Origin::Computer;
package Requirements {
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn wildcard_imports_follow_public_wildcard_reexports() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Facade {
    public import Architecture::*;
}
package Requirements {
    private import Facade::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn wildcard_imports_do_not_follow_private_reexports() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Computer;
}
package Facade {
    private import Architecture::*;
}
package Requirements {
    private import Facade::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn wildcard_imports_find_members_inherited_by_typed_namespaces() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Container {
        part def Computer;
    }
    part instance : Container;
}
package Requirements {
    private import Architecture::instance::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn wildcard_imports_preserve_nested_ambiguity() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Primary {
    part def Computer;
}
package Backup {
    part def Computer;
}
package Facade {
    public import Primary::*;
    public import Backup::*;
}
package Requirements {
    private import Facade::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.ambiguous_reference"]
    );
}

#[test]
fn direct_members_shadow_public_wildcard_reexports() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Imported {
    constraint def Computer;
}
package Facade {
    public import Imported::*;
    part def Computer;
}
package Requirements {
    private import Facade::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn cyclic_public_wildcard_reexports_terminate() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package A {
    public import B::*;
}
package B {
    public import A::*;
}
package Requirements {
    private import A::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn a_cyclic_wildcard_branch_does_not_poison_a_concrete_sibling() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Origin {
    part def Computer;
}
package A {
    public import B::*;
    public import Origin::*;
}
package B {
    public import A::*;
}
package Requirements {
    private import A::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn ambiguous_wildcard_targets_remain_ambiguous_at_use_sites() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Shared {
    part def PrimaryComputer;
}
package Shared {
    part def BackupComputer;
}
package Requirements {
    private import Shared::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.ambiguous_reference",
            "resolution.ambiguous_reference"
        ]
    );
}

#[test]
fn duplicate_wildcard_paths_to_the_same_member_remain_unique() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Origin {
    part def Computer;
}
package PrimaryFacade {
    public import Origin::*;
}
package BackupFacade {
    public import Origin::*;
}
package CombinedFacade {
    public import PrimaryFacade::*;
    public import BackupFacade::*;
}
package Requirements {
    private import CombinedFacade::*;
    requirement def ComputeRequirement {
        actor computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn resolves_chained_import_targets_through_a_declared_type() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Architecture {
    part def Container {
        part nested;
    }
    part instance : Container;
}
package Requirements {
    private import Architecture::instance;
    private import instance::nested;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn reports_cyclic_chained_membership_import_targets_without_recursing_forever() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    private import Architecture::Computer;
    private import Computer::Architecture;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn reports_a_typed_self_referential_import_target_without_recursing_forever() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part instance : MissingType;
    private import instance::MissingType;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn reports_a_qualified_self_referential_type_without_recursing_forever() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part instance : instance::MissingType;
    private import instance::MissingMember;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn resolves_quoted_feature_segments_and_assertion_trivia() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Context {
        part 'flight.computer' : Computer;
        part backup : Computer;
    }
    part context : Context;
    requirement def ComputeRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputeRequirement;
    satisfy selected by context.'flight.computer';
    assert
        satisfy selected by context::backup;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn resolves_library_packages_and_root_imports() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
library package Types {
    part def Computer;
}
private import Types::*;
requirement def ComputeRequirement {
    subject computer : Computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn validates_an_inline_satisfied_requirement_usage() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part def Computer;
    part computer : Computer;
    requirement def ComputeRequirement {
        subject computer : Computer;
    }
    satisfy requirement selected : ComputeRequirement by computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn validates_body_bearing_part_and_constraint_definition_kinds() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part def Computer {
        part processor;
    }
    constraint def Available {
        true
    }
    constraint available : Available {
        true
    }
    requirement def ComputeRequirement {
        actor computer : Computer;
        assume constraint availability : Available;
        require available;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn reports_nested_content_in_inline_constraint_bodies_without_context_leakage() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def OuterRequirement {
        subject outer : Computer;
        assume constraint {
            private import Missing::Thing;
            requirement def InnerRequirement {
                subject inner : Computer;
            }
        }
        require constraint {
            private part hidden;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "semantic.profile.unsupported_visibility"
        ]
    );
}

#[test]
fn rejects_a_referenced_satisfaction_body_without_context_leakage() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def TargetRequirement;
    requirement selected : TargetRequirement;
    requirement def OuterRequirement {
        subject outer : Computer;
        satisfy selected {
            subject nested : Computer;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_relationship_body"]
    );
}

#[test]
fn reports_an_unresolved_unused_import() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Requirements { private import Missing::Thing; }",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn rejects_an_import_without_an_explicit_visibility_indicator() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Types; package Requirements { import Types::*; }",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.import.visibility"]
    );
}

#[test]
fn rejects_a_non_private_top_level_import() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Types { part def Computer; } public import Types::Computer;",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.import.visibility"]
    );
}

#[test]
fn rejects_protected_imports_outside_the_profile() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Types; package Requirements { protected import Types::*; }",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_import"]
    );
}

#[test]
fn rejects_private_and_protected_definitions_before_qualified_resolution() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Types {
    private part def PrivateComputer;
    protected part def ProtectedComputer;
}
package Requirements {
    private import Types::PrivateComputer;
    private import Types::ProtectedComputer;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.profile.unsupported_visibility",
            "semantic.profile.unsupported_visibility"
        ]
    );
}

#[test]
fn rejects_private_and_protected_features_before_typed_feature_resolution() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Model {
    part def Computer;
    part def Context {
        private part primary : Computer;
        protected part backup : Computer;
    }
    part context : Context;
    requirement def ComputeRequirement {
        subject computer : Computer;
    }
    requirement primaryRequirement : ComputeRequirement;
    requirement backupRequirement : ComputeRequirement;
    satisfy primaryRequirement by context.primary;
    satisfy backupRequirement by context.backup;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.profile.unsupported_visibility",
            "semantic.profile.unsupported_visibility"
        ]
    );
}

#[test]
fn keeps_membership_imports_in_their_lexical_scope() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package Requirements {
    package Types { part def Computer; }
    requirement def LocallyImported {
        private import Types::Computer;
        subject computer : Computer;
    }
    requirement def OutsideImport {
        subject computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn rejects_recursive_wildcard_imports_outside_the_profile() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Types; package Requirements { private import Types::**; }",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_import"]
    );
}

#[test]
fn rejects_filtered_wildcard_imports_outside_the_profile() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        "package Types; package Requirements { private import Types::*[true]; }",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_import"]
    );
}

#[test]
fn reports_an_ambiguous_wildcard_import_reference() {
    let project = TempProject::new();
    let path = project.write(
        "model.sysml",
        r#"
package A { part def Computer; }
package B { part def Computer; }
package Requirements {
    private import A::*;
    private import B::*;
    requirement def Requirement {
        subject computer : Computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.ambiguous_reference"]
    );
}

#[test]
fn rejects_an_unknown_validation_profile_as_an_invocation_error() {
    let project = TempProject::new();
    let path = project.write("model.sysml", "package Model;");

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["validate", "--profile", "full", "--format", "json"])
        .arg(path)
        .output()
        .expect("sysml validate should run");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("unsupported validation profile"));
}

#[test]
fn validate_help_names_the_profile_and_claim_boundary() {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["validate", "--help"])
        .output()
        .expect("sysml validate help should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains(REQUIREMENTS_PROFILE));
    assert!(stdout.contains("does not claim complete semantic or OMG conformance"));
}

#[test]
fn requires_an_explicit_validation_profile() {
    let project = TempProject::new();
    let path = project.write("model.sysml", "package Model;");

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("validate")
        .arg(path)
        .output()
        .expect("sysml validate should run");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("--profile is required"));
}

#[test]
fn reports_an_unresolved_subject_type_at_the_reference() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement def BrokenRequirement {
        subject computer : MissingComputer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let report = json_report(&output);
    assert_eq!(
        diagnostic_codes(&report),
        vec!["resolution.unresolved_reference"]
    );
    assert_eq!(
        report["files"][0]["diagnostics"][0]["span"]["start_line"],
        4
    );
}

#[test]
fn an_invalid_local_requirement_subject_does_not_donate_an_inherited_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement {
        subject actual : Missing;
    }
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn duplicate_requirement_subjects_do_not_donate_the_first_subject_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def AmbiguousRequirement {
        subject first : Computer;
        subject second : Valve;
    }
    requirement selected : AmbiguousRequirement;
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.subject_cardinality"]
    );
}

#[test]
fn rejects_a_requirement_usage_typed_by_a_part_definition() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def FlightComputer;
    requirement invalidRequirement : FlightComputer;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.type_kind"]
    );
}

#[test]
fn rejects_a_requirement_usage_specializing_a_part_usage() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part parent;
    requirement child :> parent;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.specialization_kind"]
    );
}

#[test]
fn rejects_repeated_requirement_usage_specialization_parts() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement base;
    requirement child :> base :> Missing;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "semantic.profile.unsupported_multityping"
        ]
    );
}

#[test]
fn rejects_satisfaction_of_a_non_requirement_usage() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def FlightComputer;
    part flightComputer : FlightComputer;
    part notARequirement : FlightComputer;

    part context {
        satisfy notARequirement by flightComputer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.target_kind"]
    );
}

#[test]
fn rejects_an_incompatible_satisfaction_subject() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_conformance"]
    );
}

#[test]
fn rejects_a_satisfying_usage_bound_to_a_definition_without_a_conformance_cascade() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def SpecializedComputer :> Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    requirement def SatisfactionContext {
        actor candidate : Computer = SpecializedComputer;
        satisfy selected by candidate;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_kind"]
    );
}

#[test]
fn rejects_multityped_conformance_endpoints_outside_the_profile() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part candidate : Computer, Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_multityping"]
    );
}

#[test]
fn checks_a_subject_type_inherited_by_requirement_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def BaseRequirement {
        subject computer : Computer;
    }
    requirement def SpecializedRequirement :> BaseRequirement;
    requirement selected : SpecializedRequirement;
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_conformance"]
    );
}

#[test]
fn checks_a_requirement_subject_type_derived_from_its_binding() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part computer : Computer;
    part valve : Valve;
    requirement def BoundRequirement {
        subject expected = computer;
    }
    requirement selected : BoundRequirement;
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_conformance"]
    );
}

#[test]
fn resolves_subject_types_inherited_through_usage_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part base : Computer;
    part alias :> base;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement {
        subject computer : Computer = alias;
    }
    satisfy selected by alias;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(0));
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn resolves_a_keywordless_satisfying_subject_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part base : Computer;
    candidate :> base;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn reports_an_unresolved_type_in_a_transitive_usage_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part base : Missing;
    part alias :> base;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by alias;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn reports_multityping_in_a_transitive_usage_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part base : Computer, Valve;
    part alias :> base;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by alias;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_multityping"]
    );
}

#[test]
fn reports_a_missing_sibling_specialization_without_a_conformance_cascade() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Candidate :> Computer, Missing;
    part candidate : Candidate;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn reports_an_ambiguous_sibling_specialization_without_a_conformance_cascade() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Primary {
    part def Parent;
}
package Backup {
    part def Parent;
}
package Requirements {
    private import Primary::*;
    private import Backup::*;
    part def Computer;
    part def Candidate :> Computer, Parent;
    part candidate : Candidate;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.ambiguous_reference"]
    );
}

#[test]
fn cyclic_specialization_conformance_terminates() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def First :> Second;
    part def Second :> First;
    part candidate : First;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_conformance"]
    );
}

#[test]
fn cyclic_requirement_parents_with_conflicting_subjects_are_not_false_valid() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def First :> Second {
        subject first : Computer;
    }
    requirement def Second :> First {
        subject second : Valve;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.requirement.inheritance_cycle",
            "semantic.requirement.inheritance_cycle"
        ]
    );
}

#[test]
fn reports_a_missing_typed_member_parent_even_when_a_sibling_has_the_member() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Good {
        part member : Computer;
    }
    part def Broken :> Good, Missing;
    part context : Broken;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by context.member;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn reports_a_later_qualified_segment_after_a_tainted_known_prefix() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Good {
        part member;
    }
    part def Broken :> Good, Missing;
    part context : Broken;
    requirement selected;
    satisfy selected by context.member.absent;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn an_unrelated_exact_import_branch_does_not_taint_lookup() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Good {
        part imported;
    }
    part def Broken :> Good, Missing;
    part namespace : Broken;
    private import namespace::imported;
    requirement def Requirement {
        actor unresolved : Unrelated;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn a_found_member_with_a_broken_sibling_parent_is_indeterminate() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Good {
        requirement candidate;
    }
    part def Broken :> Good, Missing;
    part candidate : OuterMissing;
    part context : Broken {
        requirement selected;
        satisfy selected by candidate;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn reports_proven_inherited_ambiguity_alongside_a_broken_parent() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def First {
        part member;
    }
    part def Second {
        part member;
    }
    part def Broken :> First, Second, Missing;
    part context : Broken;
    requirement selected;
    satisfy selected by context.member;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.ambiguous_reference"
        ]
    );
}

#[test]
fn a_tainted_inner_scope_suppresses_lower_precedence_ambiguity() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Broken :> Missing;
    part inner : Broken {
        requirement selected;
        satisfy selected by candidate;
    }
    part candidate;
    requirement candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn a_tainted_import_tier_suppresses_inherited_ambiguity() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def BrokenNamespace :> Missing;
    part namespace : BrokenNamespace;
    part def First {
        part member;
    }
    part def Second {
        part member;
    }
    part def Combined :> First, Second {
        public import namespace::member;
    }
    part context : Combined;
    requirement selected;
    satisfy selected by context.member;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn tainted_feature_chain_types_do_not_donate_subject_types() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part def Good {
        part def member;
    }
    part def Broken :> Good, Missing;
    part context : Broken;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement {
        subject actual : context::member = valve;
    }
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification check : ComputerVerification {
        subject actual : context::member = valve;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn a_proven_parent_ambiguity_survives_an_unrelated_wildcard_cycle() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Primary {
    part def Parent;
}
package Backup {
    part def Parent;
}
package A {
    public import B::*;
}
package B {
    public import A::*;
}
package Requirements {
    private import Primary::*;
    private import Backup::*;
    private import A::*;
    part def Broken :> Parent;
    part context : Broken {
        requirement selected;
        satisfy selected by candidate;
    }
    part candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.ambiguous_reference"]
    );
}

#[test]
fn a_tainted_wildcard_import_target_does_not_donate_members() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Good {
        part def bucket {
            part def Present;
        }
    }
    part def Broken :> Good, Missing;
    part context : Broken;
    private import context::bucket::*;
    requirement def Requirement {
        actor value : Present;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn separately_tainted_prefix_and_suffix_report_only_their_causal_failures() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def InnerGood;
    part def InnerBroken :> InnerGood, InnerMissing;
    part def OuterGood {
        part member : InnerBroken;
    }
    part def OuterBroken :> OuterGood, OuterMissing;
    part context : OuterBroken;
    requirement selected;
    satisfy selected by context.member.absent;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn a_clean_and_a_tainted_candidate_do_not_prove_ambiguity() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Clean {
        part member;
    }
    part def TaintedGood {
        part member;
    }
    part def Tainted :> TaintedGood, Missing;
    part def Combined :> Clean, Tainted;
    part context : Combined;
    requirement selected;
    satisfy selected by context.member;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn two_tainted_candidates_do_not_prove_ambiguity() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def FirstGood {
        part member;
    }
    part def FirstTainted :> FirstGood, FirstMissing;
    part def SecondGood {
        part member;
    }
    part def SecondTainted :> SecondGood, SecondMissing;
    part def Combined :> FirstTainted, SecondTainted;
    part context : Combined;
    requirement selected;
    satisfy selected by context.member;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn a_proven_missing_parent_survives_candidate_aggregation() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def OuterGood {
        part broken;
    }
    part def OuterBroken :> OuterGood, Missing {
        part item : broken::Parent;
    }
    part context : OuterBroken;
    requirement selected;
    satisfy selected by context.item.absent;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "resolution.unresolved_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn ambiguous_exact_import_targets_expose_each_candidate_name() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Types {
    part def <C> Computer;
    part def <C> Computer;
}
package Requirements {
    private import Types::Computer;
    requirement def FirstRequirement {
        actor first : Computer;
    }
    requirement def SecondRequirement {
        actor second : C;
    }
    requirement def UnrelatedRequirement {
        actor third : NotExposed;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.ambiguous_reference",
            "resolution.ambiguous_reference",
            "resolution.ambiguous_reference",
            "resolution.unresolved_reference"
        ]
    );
}

#[test]
fn a_proven_conformance_path_allows_independent_binding_validation() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part def Candidate :> Computer, Missing;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement {
        subject actual : Candidate = valve;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "resolution.unresolved_reference",
            "semantic.requirement.subject_binding"
        ]
    );
}

#[test]
fn does_not_validate_an_untraversed_generic_parent_relationship() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Unused :> Missing;
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn rejects_an_incompatible_requirement_subject_binding() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part computer : Computer;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement {
        subject computer : Computer = valve;
    }
    satisfy selected by computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.subject_binding"]
    );
}

#[test]
fn rejects_a_requirement_subject_bound_to_a_conforming_definition() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def SpecializedComputer :> Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement {
        subject tested = SpecializedComputer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.subject_binding"]
    );
}

#[test]
fn accepts_requirement_and_verification_bindings_to_a_specialized_usage() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part base : Computer;
    part specialized :> base;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement {
        subject tested = specialized;
    }
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification check : ComputerVerification {
        subject tested = specialized;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn rejects_verification_of_a_non_requirement_usage() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def FlightComputer;
    part flightComputer : FlightComputer;

    verification def ComputerVerification {
        objective computerObjective {
            verify flightComputer;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.target_kind"]
    );
}

#[test]
fn rejects_a_verify_outside_a_verification_objective() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement def Requirement;
    requirement selected : Requirement;
    case def AnalysisCase {
        objective analysisObjective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.placement"]
    );
}

#[test]
fn rejects_an_incompatible_verification_subject() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def ComputerVerification {
        subject valve : Valve;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    let report = json_report(&output);
    assert_eq!(
        diagnostic_codes(&report),
        vec!["semantic.verification.subject_conformance"],
        "{report:#}"
    );
}

#[test]
fn an_invalid_local_verification_subject_does_not_donate_an_inherited_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ValveRequirement {
        subject expected : Valve;
    }
    requirement selected : ValveRequirement;
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification check : ComputerVerification {
        subject actual : Missing;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["resolution.unresolved_reference"]
    );
}

#[test]
fn an_untyped_local_verification_subject_still_reports_missing_effective_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def Check {
        subject actual;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_conformance"]
    );
}

#[test]
fn an_untyped_local_verification_subject_with_an_inherited_type_does_not_cascade() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification check : ComputerVerification {
        subject actual;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_binding"]
    );
}

#[test]
fn checks_a_subject_type_inherited_through_verification_usage_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def ParentVerification {
        subject valve : Valve;
    }
    verification parent : ParentVerification;
    verification child :> parent {
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    let report = json_report(&output);
    assert_eq!(
        diagnostic_codes(&report),
        vec!["semantic.verification.subject_conformance"],
        "{report:#}"
    );
}

#[test]
fn cyclic_verification_parents_are_reported_without_a_conformance_cascade() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def First :> Second;
    verification def Second :> First;
    verification check : First {
        subject actual : Valve;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.verification.inheritance_cycle",
            "semantic.verification.inheritance_cycle"
        ]
    );
}

#[test]
fn cyclic_verification_parents_with_conflicting_subjects_are_not_false_valid() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    verification def First :> Second {
        subject first : Computer;
    }
    verification def Second :> First {
        subject second : Valve;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.verification.inheritance_cycle",
            "semantic.verification.inheritance_cycle"
        ]
    );
}

#[test]
fn multityped_verification_parents_do_not_cascade_to_subject_conformance() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def FirstVerification {
        subject expected : Computer;
    }
    verification def SecondVerification {
        subject expected : Computer;
    }
    verification check : FirstVerification, SecondVerification {
        subject actual : Computer;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.case_type_cardinality"]
    );
}

#[test]
fn an_invalid_verification_parent_does_not_cascade_to_missing_subject_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part wrongParent;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification check : wrongParent {
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.case_type_kind"]
    );
}

#[test]
fn wrong_kind_verification_usage_parents_do_not_donate_subjects() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification base : ComputerVerification;
    verification wrongSpecialization :> ComputerVerification {
        subject actual : Valve;
    }
    verification wrongType : base {
        subject actual : Valve;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.verification.case_specialization_kind",
            "semantic.verification.case_type_kind"
        ]
    );
}

#[test]
fn validates_a_subject_type_inherited_by_verification_definition_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def BaseVerification {
        subject expected : Computer;
    }
    verification def DerivedVerification :> BaseVerification {
        subject actual : Valve;
        objective {
            verify selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_binding"]
    );
}

#[test]
fn rejects_an_incompatible_typed_verification_subject_binding() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selectedRequirement : ComputerRequirement;
    verification def ComputerVerification {
        subject computer : Computer;
    }
    verification selected : ComputerVerification {
        subject computer : Computer = valve;
        objective {
            verify selectedRequirement;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_binding"]
    );
}

#[test]
fn rejects_a_verification_subject_bound_to_a_conforming_definition() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def SpecializedComputer :> Computer;
    verification def ComputerVerification {
        subject expected : Computer;
    }
    verification check : ComputerVerification {
        subject tested = SpecializedComputer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_binding"]
    );
}

#[test]
fn resolves_a_pure_dotted_verification_subject_binding() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Context {
        part computer : Computer;
    }
    part context : Context;
    verification def CheckDefinition {
        subject expected : Computer;
    }
    verification check : CheckDefinition {
        subject tested = context.computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(json_report(&output)["valid"], true);
}

#[test]
fn resolves_an_inherited_member_before_an_unrelated_package_homonym() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part computer : Valve;
    verification def CheckDefinition {
        subject expected : Computer;
        part computer : Computer;
    }
    verification check : CheckDefinition {
        subject tested = computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert_eq!(json_report(&output)["valid"], true);
}

#[test]
fn direct_and_imported_members_shadow_inherited_members() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Imported {
    part def Valve;
    part computer : Valve;
}
package Requirements {
    part def Computer;
    part def Valve;
    verification def CheckDefinition {
        subject expected : Computer;
        part computer : Computer;
    }
    verification directCheck : CheckDefinition {
        part computer : Valve;
        subject tested = computer;
    }
    verification importedCheck : CheckDefinition {
        private import Imported::computer;
        subject tested = computer;
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.verification.subject_binding",
            "semantic.verification.subject_binding"
        ]
    );
}

#[test]
fn rejects_an_objective_nested_below_an_unmodeled_perform_owner() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement def Requirement;
    requirement selected : Requirement;
    verification def Check {
        perform action {
            objective nested {
                verify selected;
            }
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.placement"]
    );
}

#[test]
fn rejects_objectives_nested_below_loop_and_body_wrapped_statement_owners() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement def Requirement;
    requirement selected : Requirement;
    verification def Check {
        part start;
        part finish;
        for item in items {
            objective belowLoop {
                verify selected;
            }
        }
        first start then finish {
            objective belowFirst {
                verify selected;
            }
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.verification.placement",
            "semantic.verification.placement"
        ]
    );
}

#[test]
fn rejects_multiple_inline_verified_requirement_specializations() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    requirement first;
    requirement second;
    verification def Check {
        objective {
            verify requirement child :> first :> second;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.profile.unsupported_multityping"]
    );
}

#[test]
fn rejects_an_inline_verified_requirement_specializing_a_part_usage() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part parent;
    verification def Check {
        objective {
            verify requirement child :> parent;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.specialization_kind"]
    );
}

#[test]
fn a_wrong_kind_inline_parent_does_not_donate_its_requirement_subject() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    verification def Check {
        subject actual : Valve;
        objective {
            verify requirement child :> ComputerRequirement;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.specialization_kind"]
    );
}

#[test]
fn wrong_kind_usage_parents_do_not_donate_requirement_subjects() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected :> ComputerRequirement {
        subject actual : Valve;
    }
    verification def Check {
        objective selectedObjective :> ComputerRequirement {
            subject actual : Valve;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.requirement.specialization_kind",
            "semantic.requirement.specialization_kind"
        ]
    );
}

#[test]
fn wrong_kind_requirement_types_and_definition_parents_do_not_donate_subjects() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement baseUsage : ComputerRequirement;
    verification def Producer {
        objective baseObjective : ComputerRequirement;
    }
    requirement incorrectlyTyped : Producer::baseObjective;
    requirement def IncorrectDefinition :> baseUsage;
    requirement selected : IncorrectDefinition;
    satisfy incorrectlyTyped by valve;
    satisfy selected by valve;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec![
            "semantic.requirement.type_kind",
            "semantic.requirement.specialization_kind"
        ]
    );
}

#[test]
fn checks_subject_conformance_for_an_inline_verified_requirement_specialization() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement parent : ComputerRequirement;
    verification def Check {
        subject valve : Valve;
        objective {
            verify requirement child :> parent;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.verification.subject_conformance"]
    );
}

#[test]
fn accepts_an_objective_as_a_require_satisfy_and_verify_target() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part computer : Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    verification def Producer {
        objective base : ComputerRequirement;
    }
    verification def Derived {
        objective selected :> Producer::base;
    }
    verification def Consumer {
        subject actual : Computer;
        objective consumerObjective {
            require Derived::selected;
            satisfy Derived::selected by computer;
            verify Derived::selected;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn rejects_an_objective_as_a_satisfying_subject() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement selected : ComputerRequirement;
    verification def Producer {
        objective candidate : ComputerRequirement;
    }
    satisfy selected by Producer::candidate;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.satisfaction.subject_kind"]
    );
}

#[test]
fn validates_an_objective_subject_against_its_requirement_type() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    part computer : Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    verification def Check {
        objective selected : ComputerRequirement {
            subject actual : Valve;
        }
    }
    satisfy Check::selected by computer;
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.subject_binding"]
    );
}

#[test]
fn resolves_a_named_inline_verified_requirement_before_a_package_homonym() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    part def Valve;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    requirement def ValveRequirement {
        subject expected : Valve;
    }
    requirement child : ValveRequirement;
    verification def Check {
        subject actual : Computer;
        objective {
            verify requirement child : ComputerRequirement;
            verify child;
            verify requirement : ComputerRequirement;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert!(output.status.success(), "{output:?}");
    assert!(diagnostic_codes(&json_report(&output)).is_empty());
}

#[test]
fn reports_one_requirement_kind_error_for_an_inline_verified_requirement() {
    let project = TempProject::new();
    let path = project.write(
        "requirements.sysml",
        r#"
package Requirements {
    part def Computer;
    verification def Check {
        objective {
            verify requirement child : Computer;
        }
    }
}
"#,
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(
        diagnostic_codes(&json_report(&output)),
        vec!["semantic.requirement.type_kind"]
    );
}

#[test]
fn syntax_diagnostics_suppress_semantic_cascades() {
    let project = TempProject::new();
    let path = project.write(
        "broken.sysml",
        "package Broken { requirement def Requirement { subject x : Missing;",
    );

    let output = validate(&path, "json");

    assert_eq!(output.status.code(), Some(1));
    let report = json_report(&output);
    assert_eq!(
        report["profile"]["source_commit"],
        "9baca5908ca28b53da085de69336fde48420ea8f"
    );
    let codes = diagnostic_codes(&report);
    assert!(!codes.is_empty());
    assert!(codes.iter().all(|code| code.starts_with("syntax.")));
}
