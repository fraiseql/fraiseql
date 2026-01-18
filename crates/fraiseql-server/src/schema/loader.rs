//! Schema loader for compiled GraphQL schemas.

use std::path::{Path, PathBuf};

use fraiseql_core::schema::CompiledSchema;
use tracing::{debug, info};

/// Error loading schema.
#[derive(Debug, thiserror::Error)]
pub enum SchemaLoadError {
    /// Schema file not found.
    #[error("Schema file not found: {0}")]
    NotFound(PathBuf),

    /// IO error reading file.
    #[error("Failed to read schema file: {0}")]
    IoError(#[from] std::io::Error),

    /// JSON parsing error.
    #[error("Failed to parse schema JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    /// Schema validation error.
    #[error("Invalid schema: {0}")]
    ValidationError(String),
}

/// Loader for compiled GraphQL schemas from JSON files.
///
/// Loads and caches a compiled schema from a JSON file on disk.
/// Used during server startup to prepare the schema for query execution.
#[derive(Debug, Clone)]
pub struct CompiledSchemaLoader {
    /// Path to the compiled schema JSON file.
    path: PathBuf,
}

impl CompiledSchemaLoader {
    /// Create a new schema loader pointing to a schema file.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the compiled schema JSON file
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let loader = CompiledSchemaLoader::new("schema.compiled.json");
    /// let schema = loader.load().await?;
    /// ```
    #[must_use]
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    /// Load schema from file.
    ///
    /// Reads the schema JSON file, parses it, and returns a `CompiledSchema`.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - File does not exist
    /// - File cannot be read
    /// - JSON is invalid
    /// - Schema validation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let loader = CompiledSchemaLoader::new("schema.compiled.json");
    /// let schema = loader.load().await?;
    /// ```
    pub async fn load(&self) -> Result<CompiledSchema, SchemaLoadError> {
        info!(path = %self.path.display(), "Loading compiled schema");

        // Check if file exists
        if !self.path.exists() {
            return Err(SchemaLoadError::NotFound(self.path.clone()));
        }

        // Read file asynchronously
        let contents =
            tokio::fs::read_to_string(&self.path).await.map_err(SchemaLoadError::IoError)?;

        debug!(
            path = %self.path.display(),
            size_bytes = contents.len(),
            "Schema file read successfully"
        );

        // Parse JSON and validate it's valid JSON first
        serde_json::from_str::<serde_json::Value>(&contents)?;

        // Create CompiledSchema from JSON string
        let schema = CompiledSchema::from_json(&contents)
            .map_err(|e| SchemaLoadError::ValidationError(e.to_string()))?;

        info!(path = %self.path.display(), "Schema loaded successfully");

        Ok(schema)
    }

    /// Get the path to the schema file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    async fn test_loader_not_found() {
        let loader = CompiledSchemaLoader::new("/nonexistent/path/schema.json");
        let result = loader.load().await;
        assert!(matches!(result, Err(SchemaLoadError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_loader_invalid_json() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "{{invalid json").unwrap();
        file.flush().unwrap();

        let loader = CompiledSchemaLoader::new(file.path());
        let result = loader.load().await;
        assert!(matches!(result, Err(SchemaLoadError::ParseError(_))));
    }
}
