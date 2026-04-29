//! Schema loader for compiled GraphQL schemas.

use std::path::{Path, PathBuf};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_functions::FunctionDefinition;
use serde::Deserialize;
use tracing::{debug, info};

use crate::realtime::routes::RealtimeSchemaConfig;

/// Error loading schema.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

/// Storage configuration extracted from the `"storage"` section of a compiled schema.
///
/// This describes the *bucket policies* declared by the developer, not the storage
/// backend settings (which come from server TOML / environment variables).
///
/// ```json
/// {
///   "storage": {
///     "buckets": [
///       { "name": "avatars", "access": "private" },
///       { "name": "media", "access": "public_read", "max_object_bytes": 5242880 }
///     ]
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct SchemaStorageConfig {
    /// Bucket definitions declared in the schema.
    pub buckets: Vec<SchemaBucketDef>,
}

/// A single bucket definition from the compiled schema.
#[derive(Debug, Clone, Deserialize)]
pub struct SchemaBucketDef {
    /// Bucket name — must be a valid identifier (alphanumeric, hyphens, underscores; no spaces).
    pub name: String,

    /// Access policy: `"private"` (default) or `"public_read"`.
    #[serde(default = "default_access")]
    pub access: String,

    /// Maximum object size in bytes. `None` means unlimited.
    #[serde(default)]
    pub max_object_bytes: Option<u64>,

    /// Allowed MIME types. `None` means any MIME type is accepted.
    #[serde(default)]
    pub allowed_mime_types: Option<Vec<String>>,
}

fn default_access() -> String {
    "private".to_string()
}

/// Functions configuration extracted from the `"functions"` section of a compiled schema.
///
/// ```json
/// {
///   "functions": {
///     "module_dir": "/opt/fraiseql/functions",
///     "definitions": [
///       { "name": "on_create_user", "trigger": "after:mutation:createUser", "runtime": "Wasm" }
///     ]
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize)]
pub struct FunctionsConfig {
    /// Directory containing compiled function modules (`.wasm`, `.js`, etc.).
    pub module_dir: PathBuf,

    /// Function definitions loaded from the compiled schema.
    pub definitions: Vec<FunctionDefinition>,
}

/// A compiled schema with all optional platform extensions parsed out.
///
/// Use [`CompiledSchemaLoader::load_extended`] to obtain this type. It bundles the
/// core [`CompiledSchema`] together with optional storage, functions, and realtime
/// configurations that are embedded in the compiled schema JSON.
#[derive(Debug)]
pub struct ExtendedCompiledSchema {
    /// Core compiled GraphQL schema (types, queries, mutations, subscriptions).
    pub schema: CompiledSchema,

    /// Storage bucket configuration, if the `"storage"` key is present and non-null.
    pub storage: Option<SchemaStorageConfig>,

    /// Serverless functions configuration, if the `"functions"` key is present.
    pub functions: Option<FunctionsConfig>,

    /// Realtime broadcast observer configuration, if the `"realtime"` key is present.
    pub realtime: Option<RealtimeSchemaConfig>,
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
    /// ```no_run
    /// // Requires: schema.compiled.json file on disk.
    /// # use fraiseql_server::schema::loader::CompiledSchemaLoader;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let loader = CompiledSchemaLoader::new("schema.compiled.json");
    /// let schema = loader.load().await?;
    /// # Ok(())
    /// # }
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
    /// Returns [`SchemaLoadError::NotFound`] if the file does not exist.
    /// Returns [`SchemaLoadError::IoError`] if the file cannot be read.
    /// Returns [`SchemaLoadError::ParseError`] if the JSON is malformed.
    /// Returns [`SchemaLoadError::ValidationError`] if schema validation fails.
    ///
    /// # Example
    ///
    /// ```no_run
    /// // Requires: schema.compiled.json file on disk.
    /// # use fraiseql_server::schema::loader::CompiledSchemaLoader;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let loader = CompiledSchemaLoader::new("schema.compiled.json");
    /// let schema = loader.load().await?;
    /// # Ok(())
    /// # }
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

    /// Load schema and all optional platform extension sections from file.
    ///
    /// In addition to the core schema (types, queries, mutations, subscriptions),
    /// this method parses and validates the `"storage"`, `"functions"`, and
    /// `"realtime"` top-level keys if they are present. Unknown top-level keys are
    /// ignored for forward compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaLoadError::NotFound`] if the file does not exist.
    /// Returns [`SchemaLoadError::IoError`] if the file cannot be read.
    /// Returns [`SchemaLoadError::ParseError`] if the JSON is malformed.
    /// Returns [`SchemaLoadError::ValidationError`] if any of the following fail:
    ///   - A storage bucket name contains whitespace or is empty.
    ///   - A function trigger string does not match a recognised pattern.
    ///   - A realtime entity name does not appear in the schema's type definitions.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use fraiseql_server::schema::loader::CompiledSchemaLoader;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let loader = CompiledSchemaLoader::new("schema.compiled.json");
    /// let extended = loader.load_extended().await?;
    /// if let Some(storage) = &extended.storage {
    ///     println!("{} buckets configured", storage.buckets.len());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn load_extended(&self) -> Result<ExtendedCompiledSchema, SchemaLoadError> {
        info!(path = %self.path.display(), "Loading extended compiled schema");

        if !self.path.exists() {
            return Err(SchemaLoadError::NotFound(self.path.clone()));
        }

        let contents =
            tokio::fs::read_to_string(&self.path).await.map_err(SchemaLoadError::IoError)?;

        debug!(
            path = %self.path.display(),
            size_bytes = contents.len(),
            "Schema file read for extended loading"
        );

        // Parse once as a raw JSON value so we can extract platform sections without
        // touching the CompiledSchema deserialization path.
        let raw: serde_json::Value = serde_json::from_str(&contents)?;

        // Core schema (always required).
        let schema = CompiledSchema::from_json(&contents)
            .map_err(|e| SchemaLoadError::ValidationError(e.to_string()))?;

        // Collect type names for cross-validation.
        let type_names: std::collections::HashSet<String> =
            schema.types.iter().map(|t| t.name.as_str().to_owned()).collect();

        // Parse and validate the optional sections.
        let storage = raw
            .get("storage")
            .filter(|v| !v.is_null())
            .map(|v| {
                let cfg: SchemaStorageConfig = serde_json::from_value(v.clone())?;
                validate_storage_config(&cfg)?;
                Ok::<_, SchemaLoadError>(cfg)
            })
            .transpose()?;

        let functions = raw
            .get("functions")
            .filter(|v| !v.is_null())
            .map(|v| {
                let cfg: FunctionsConfig = serde_json::from_value(v.clone())?;
                validate_functions_config(&cfg)?;
                Ok::<_, SchemaLoadError>(cfg)
            })
            .transpose()?;

        let realtime = raw
            .get("realtime")
            .filter(|v| !v.is_null())
            .map(|v| {
                let cfg: RealtimeSchemaConfig = serde_json::from_value(v.clone())?;
                validate_realtime_config(&cfg, &type_names)?;
                Ok::<_, SchemaLoadError>(cfg)
            })
            .transpose()?;

        info!(
            path = %self.path.display(),
            has_storage = storage.is_some(),
            has_functions = functions.is_some(),
            has_realtime = realtime.is_some(),
            "Extended schema loaded successfully"
        );

        Ok(ExtendedCompiledSchema { schema, storage, functions, realtime })
    }

    /// Get the path to the schema file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Validate storage bucket configurations.
///
/// # Errors
///
/// Returns `ValidationError` if any bucket name is empty or contains whitespace.
fn validate_storage_config(config: &SchemaStorageConfig) -> Result<(), SchemaLoadError> {
    for bucket in &config.buckets {
        if bucket.name.is_empty() {
            return Err(SchemaLoadError::ValidationError(
                "storage bucket name must not be empty".to_string(),
            ));
        }
        if bucket.name.chars().any(char::is_whitespace) {
            return Err(SchemaLoadError::ValidationError(format!(
                "storage bucket name {:?} must not contain whitespace",
                bucket.name
            )));
        }
    }
    Ok(())
}

/// Valid trigger prefixes recognised by the trigger system.
const VALID_TRIGGER_PREFIXES: &[&str] =
    &["after:mutation:", "before:mutation:", "after:storage:", "cron:", "http:"];

/// Validate function definitions.
///
/// # Errors
///
/// Returns `ValidationError` if any function definition has an unrecognised trigger format.
fn validate_functions_config(config: &FunctionsConfig) -> Result<(), SchemaLoadError> {
    for def in &config.definitions {
        let known = VALID_TRIGGER_PREFIXES.iter().any(|prefix| def.trigger.starts_with(prefix));
        if !known {
            return Err(SchemaLoadError::ValidationError(format!(
                "function {:?} has unrecognised trigger format {:?}; \
                 expected one of: after:mutation:<name>, before:mutation:<name>, \
                 after:storage:<bucket>:<op>, cron:<expr>, http:<method>:<path>",
                def.name, def.trigger
            )));
        }
    }
    Ok(())
}

/// Validate that realtime entities exist in the schema's type definitions.
///
/// # Errors
///
/// Returns `ValidationError` if any entity name is not present in `type_names`.
fn validate_realtime_config(
    config: &RealtimeSchemaConfig,
    type_names: &std::collections::HashSet<String>,
) -> Result<(), SchemaLoadError> {
    for entity in &config.entities {
        if !type_names.contains(entity) {
            return Err(SchemaLoadError::ValidationError(format!(
                "realtime entity {entity:?} is not defined in schema types; \
                 add a @fraiseql.type decorated class named {entity:?} or remove it from the realtime entities list"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable
    #![allow(clippy::cast_precision_loss)] // Reason: test metrics reporting
    #![allow(clippy::cast_sign_loss)] // Reason: test data uses small positive integers
    #![allow(clippy::cast_possible_truncation)] // Reason: test data values are bounded
    #![allow(clippy::cast_possible_wrap)] // Reason: test data values are bounded
    #![allow(clippy::missing_panics_doc)] // Reason: test helpers
    #![allow(clippy::missing_errors_doc)] // Reason: test helpers
    #![allow(missing_docs)] // Reason: test code
    #![allow(clippy::items_after_statements)] // Reason: test helpers defined near use site

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
