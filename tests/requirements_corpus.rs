use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CORPUS_ROOT: &str = "tests/corpus/requirements";
const REQUIREMENTS_PROFILE: &str = "sysml-2.0-requirements-structure-v1";

fn corpus_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(CORPUS_ROOT)
        .join(relative)
}

fn check_json(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["check", "--format", "json"])
        .arg(path)
        .output()
        .expect("sysml check should run")
}

fn validate_json(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args([
            "validate",
            "--profile",
            REQUIREMENTS_PROFILE,
            "--format",
            "json",
        ])
        .arg(path)
        .output()
        .expect("sysml validate should run")
}

fn parse_report(output: &Output) -> Value {
    assert!(
        output.stderr.is_empty(),
        "JSON checks should not write stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout should contain a JSON check report")
}

#[test]
fn requirements_and_verification_positive_corpus_is_syntax_valid() {
    let output = check_json(&corpus_path("positive"));
    assert!(output.status.success(), "{output:?}");

    let report = parse_report(&output);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["validation_level"], "syntax");
    assert_eq!(report["valid"], true);

    let files = report["files"]
        .as_array()
        .expect("files should be an array");
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|file| file["valid"] == true));
    assert!(files
        .iter()
        .all(|file| { file["diagnostics"].as_array().is_some_and(Vec::is_empty) }));
}

#[test]
fn each_one_fault_negative_fixture_reports_a_syntax_diagnostic() {
    for fixture in [
        "negative/requirement_missing_semicolon.sysml",
        "negative/verification_missing_semicolon.sysml",
    ] {
        let output = check_json(&corpus_path(fixture));
        assert_eq!(output.status.code(), Some(1), "fixture: {fixture}");

        let report = parse_report(&output);
        assert_eq!(report["schema_version"], 1, "fixture: {fixture}");
        assert_eq!(report["validation_level"], "syntax", "fixture: {fixture}");
        assert_eq!(report["valid"], false, "fixture: {fixture}");

        let files = report["files"]
            .as_array()
            .expect("files should be an array");
        assert_eq!(files.len(), 1, "fixture: {fixture}");
        let diagnostics = files[0]["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array");
        assert!(!diagnostics.is_empty(), "fixture: {fixture}");
        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic["severity"] == "error"
                    && diagnostic["code"]
                        .as_str()
                        .is_some_and(|code| code.starts_with("syntax."))
            }),
            "fixture: {fixture}"
        );
    }
}

#[test]
fn each_one_fault_semantic_fixture_reports_its_stable_code() {
    for (fixture, expected_code) in [
        (
            "semantic-negative/unresolved_subject_type.sysml",
            "resolution.unresolved_reference",
        ),
        (
            "semantic-negative/requirement_wrong_type_kind.sysml",
            "semantic.requirement.type_kind",
        ),
        (
            "semantic-negative/satisfaction_subject_mismatch.sysml",
            "semantic.satisfaction.subject_conformance",
        ),
        (
            "semantic-negative/verification_target_wrong_kind.sysml",
            "semantic.verification.target_kind",
        ),
    ] {
        let output = validate_json(&corpus_path(fixture));
        assert_eq!(output.status.code(), Some(1), "fixture: {fixture}");

        let report = parse_report(&output);
        assert_eq!(report["schema_version"], 1, "fixture: {fixture}");
        assert_eq!(report["profile"]["id"], REQUIREMENTS_PROFILE);
        assert_eq!(report["valid"], false, "fixture: {fixture}");
        let diagnostics = report["files"][0]["diagnostics"]
            .as_array()
            .expect("diagnostics should be an array");
        assert_eq!(diagnostics.len(), 1, "fixture: {fixture}");
        assert_eq!(diagnostics[0]["code"], expected_code, "fixture: {fixture}");
    }
}
