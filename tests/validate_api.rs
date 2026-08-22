use std::path::PathBuf;

use sysml::{validate_paths, ValidationProfileId, REQUIREMENTS_STRUCTURE_PROFILE_ID};

#[test]
fn public_validation_api_requires_and_reports_the_selected_profile() {
    let model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/requirements/positive/requirement_capture.sysml");

    let selected_profile = ValidationProfileId::RequirementsStructureV1;
    let report = validate_paths(&[model], selected_profile).expect("validation should run");

    assert!(report.valid, "{report:#?}");
    assert_eq!(selected_profile.as_str(), REQUIREMENTS_STRUCTURE_PROFILE_ID);
    assert_eq!(report.profile.id, selected_profile.as_str());
    assert_eq!(
        report.profile.source_commit,
        "9baca5908ca28b53da085de69336fde48420ea8f"
    );
}
