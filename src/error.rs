use std::error::Error;
use std::fmt;
use std::io;
use std::path::PathBuf;

/// A single model validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// The element, relationship, or field associated with the failure.
    pub location: String,
    /// A human-readable explanation of the failure.
    pub message: String,
}

impl ValidationIssue {
    pub(crate) fn new(location: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            location: location.into(),
            message: message.into(),
        }
    }
}

/// All validation failures found in one model pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The collected validation failures.
    pub issues: Vec<ValidationIssue>,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "model validation failed with {} issue(s):",
            self.issues.len()
        )?;
        for issue in &self.issues {
            writeln!(formatter, "- {}: {}", issue.location, issue.message)?;
        }
        Ok(())
    }
}

impl Error for ValidationError {}

/// Errors returned while reading, writing, or validating a model.
#[derive(Debug)]
pub enum ModelError {
    /// The model could not be read from or written to a file.
    Io { path: PathBuf, source: io::Error },
    /// The TOML document could not be parsed.
    Parse(toml::de::Error),
    /// The model could not be serialized to TOML.
    Serialize(toml::ser::Error),
    /// The parsed or constructed model is invalid.
    Validation(ValidationError),
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "failed to access {}: {source}", path.display())
            }
            Self::Parse(source) => write!(formatter, "failed to parse model TOML: {source}"),
            Self::Serialize(source) => {
                write!(formatter, "failed to serialize model TOML: {source}")
            }
            Self::Validation(source) => source.fmt(formatter),
        }
    }
}

impl Error for ModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse(source) => Some(source),
            Self::Serialize(source) => Some(source),
            Self::Validation(source) => Some(source),
        }
    }
}

impl From<toml::de::Error> for ModelError {
    fn from(source: toml::de::Error) -> Self {
        Self::Parse(source)
    }
}

impl From<toml::ser::Error> for ModelError {
    fn from(source: toml::ser::Error) -> Self {
        Self::Serialize(source)
    }
}

impl From<ValidationError> for ModelError {
    fn from(source: ValidationError) -> Self {
        Self::Validation(source)
    }
}
