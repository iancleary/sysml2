//! Text-file-backed building blocks for SysML v2 system models.
//!
//! The crate intentionally starts with a small, explicit model graph rather
//! than claiming full conformance with the OMG SysML v2 textual grammar. It
//! covers the major definition, usage, and relationship families and stores
//! them in a deterministic TOML representation.

mod error;
mod model;
mod text;

pub use error::{ModelError, ValidationError, ValidationIssue};
pub use model::{
    Direction, Element, ElementKind, ElementRole, Model, ModelSummary, Multiplicity, Relationship,
    RelationshipKind, UsageOwnership, SCHEMA_VERSION,
};
