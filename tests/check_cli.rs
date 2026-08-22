use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEMP_MODEL_ID: AtomicU64 = AtomicU64::new(0);

struct TempModel {
    root: PathBuf,
}

impl TempModel {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let sequence = NEXT_TEMP_MODEL_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "sysml-check-{}-{unique}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temporary model directory should be created");
        Self { root }
    }

    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(&path, source).expect("temporary SysML model should be written");
        path
    }
}

impl Drop for TempModel {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn check_accepts_a_directory_and_emits_stable_json() {
    let model = TempModel::new();
    model.write("b.sysml", "package B;");
    model.write("a.sysml", "package A;");
    model.write("ignored.txt", "not a model");

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["check", "--format", "json"])
        .arg(&model.root)
        .output()
        .expect("sysml check should run");

    assert!(output.status.success(), "{:?}", output);
    assert!(output.stderr.is_empty());

    let report: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should contain valid JSON");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["validation_level"], "syntax");
    assert_eq!(report["valid"], true);
    assert_eq!(report["files"].as_array().unwrap().len(), 2);
    assert!(report["files"][0]["path"]
        .as_str()
        .unwrap()
        .ends_with("a.sysml"));
    assert!(report["files"][1]["path"]
        .as_str()
        .unwrap()
        .ends_with("b.sysml"));
}

#[test]
fn check_returns_one_for_invalid_sysml() {
    let model = TempModel::new();
    let path = model.write("invalid.sysml", "package Broken { part def Component;");

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("check")
        .arg(path)
        .output()
        .expect("sysml check should run");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("syntax."));
    assert!(stderr.contains("syntax errors found"));
}

#[test]
fn check_keeps_parser_accepted_bare_imports_at_the_syntax_level() {
    let model = TempModel::new();
    let path = model.write(
        "bare-import.sysml",
        "package Types; package Requirements { import Types::*; }",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["check", "--format", "json"])
        .arg(path)
        .output()
        .expect("sysml check should run");

    assert!(output.status.success(), "{output:?}");
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should contain valid JSON");
    assert_eq!(report["validation_level"], "syntax");
    assert_eq!(report["valid"], true);
}

#[test]
fn check_reports_the_current_inline_verification_body_parser_gap() {
    let model = TempModel::new();
    let path = model.write(
        "inline-verification-body.sysml",
        r#"
package Requirements {
    part def Computer;
    requirement def ComputerRequirement {
        subject expected : Computer;
    }
    verification def Check {
        objective {
            verify requirement selected : ComputerRequirement {
                subject actual : Computer;
            }
        }
    }
}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["check", "--format", "json"])
        .arg(path)
        .output()
        .expect("sysml check should run");

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("stdout should contain valid JSON");
    assert_eq!(report["validation_level"], "syntax");
    assert_eq!(report["valid"], false);
    assert!(report["files"][0]["diagnostics"]
        .as_array()
        .is_some_and(
            |diagnostics| diagnostics.iter().all(|diagnostic| diagnostic["code"]
                .as_str()
                .is_some_and(|code| code.starts_with("syntax.")))
        ));
}

#[test]
fn positional_model_path_is_rejected_as_an_unknown_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("legacy.sysml.toml")
        .output()
        .expect("sysml CLI should run");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("error: unknown command legacy.sysml.toml"));
    assert!(stderr.contains("usage:"));
}

#[test]
fn top_level_help_lists_standard_sysml_as_the_only_model_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .arg("--help")
        .output()
        .expect("sysml help should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("check [--format human|json] <path>..."));
    assert!(!stdout.contains("sysml.toml"));
}

#[test]
fn check_help_is_successful_and_states_the_coverage_boundary() {
    let output = Command::new(env!("CARGO_BIN_EXE_sysml"))
        .args(["check", "--help"])
        .output()
        .expect("sysml check help should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("syntax diagnostics only"));
    assert!(stdout.contains("does not claim full semantic"));
}
