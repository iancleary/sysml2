use crate::{Model, ModelError};
use std::fs;
use std::path::Path;

impl Model {
    /// Parse and validate a model from the crate's TOML representation.
    pub fn from_toml_str(source: &str) -> Result<Self, ModelError> {
        let model: Self = toml::from_str(source)?;
        model.validate()?;
        Ok(model)
    }

    /// Serialize a valid model to deterministic, human-readable TOML.
    pub fn to_toml_string(&self) -> Result<String, ModelError> {
        self.validate()?;
        Ok(toml::to_string_pretty(self)?)
    }

    /// Load and validate a TOML model file.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ModelError> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| ModelError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml_str(&source)
    }

    /// Validate and save a model as TOML.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ModelError> {
        let path = path.as_ref();
        let source = self.to_toml_string()?;
        fs::write(path, source).map_err(|source| ModelError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}
