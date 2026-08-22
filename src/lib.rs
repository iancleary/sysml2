//! CLI-first tooling for standard SysML v2 textual models.
//!
//! The syntax frontend checks standard textual models. Semantic validation is
//! available only through an explicitly selected, bounded profile. Neither
//! surface claims complete OMG SysML v2 conformance.

mod check;
mod project;
mod validate;

pub use check::{
    check_paths, CheckDiagnostic, CheckError, CheckFileReport, CheckReport, CheckSpan,
    CHECK_REPORT_SCHEMA_VERSION,
};
pub use validate::{
    validate_paths, ValidationProfile, ValidationProfileId, ValidationReport,
    REQUIREMENTS_STRUCTURE_PROFILE_ID, VALIDATION_REPORT_SCHEMA_VERSION,
};
