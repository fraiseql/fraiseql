//! Schema loader for compiled GraphQL schemas.

use std::path::{Path, PathBuf};

use fraiseql_core::schema::CompiledSchema;
use fraiseql_functions::FunctionDefinition;
use serde::Deserialize;
use tracing::{debug, info, warn};

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

    /// Which dead-letter store backs function dispatch (#598): `"memory"` (the
    /// default — dead-letters vanish on restart) or `"postgres"` (durable, survives
    /// a restart; requires a database pool). Overridable by the
    /// `FRAISEQL_FUNCTIONS_DLQ_STORE` env var. Absent ⇒ memory.
    #[serde(default)]
    pub dlq_store: Option<String>,
}

/// A compiled schema with all optional platform extensions parsed out.
///
/// Use [`CompiledSchemaLoader::load_extended`] to obtain this type. It bundles the
/// core [`CompiledSchema`] together with the optional `functions` configuration
/// embedded in the compiled schema JSON.
#[derive(Debug)]
pub struct ExtendedCompiledSchema {
    /// Core compiled GraphQL schema (types, queries, mutations, subscriptions).
    pub schema: CompiledSchema,

    /// Serverless functions configuration, if the `"functions"` key is present.
    pub functions: Option<FunctionsConfig>,
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
        let schema = CompiledSchema::from_json(&contents, false)
            .map_err(|e| SchemaLoadError::ValidationError(e.to_string()))?;

        info!(path = %self.path.display(), "Schema loaded successfully");

        Ok(schema)
    }

    /// Load schema and all optional platform extension sections from file.
    ///
    /// In addition to the core schema (types, queries, mutations, subscriptions),
    /// this method parses and validates the `"functions"` top-level key if it is
    /// present. A `"storage"` key is refused (#1008 — nothing reads it; the working
    /// surface is `[storage]` in the server config file). A legacy `"realtime"` key
    /// is ignored with a warning (the subsystem was removed in #605). Unknown
    /// top-level keys are ignored for forward compatibility.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaLoadError::NotFound`] if the file does not exist.
    /// Returns [`SchemaLoadError::IoError`] if the file cannot be read.
    /// Returns [`SchemaLoadError::ParseError`] if the JSON is malformed.
    /// Returns [`SchemaLoadError::ValidationError`] if any of the following fail:
    ///   - A non-null `"storage"` key is present.
    ///   - A function trigger string does not match a recognised pattern.
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
        let schema = CompiledSchema::from_json(&contents, false)
            .map_err(|e| SchemaLoadError::ValidationError(e.to_string()))?;

        // The compiled-schema `storage` section is refused rather than parsed (#1008).
        //
        // It used to be deserialized, validated, and stored on
        // `ExtendedCompiledSchema.storage` — where nothing read it. `main.rs` takes
        // `.schema` and `.functions`; the storage backend is built from `[storage]` in
        // the *server config file*. So an author who read "configuration is embedded in
        // the compiled schema" and declared buckets here got a clean compile, a clean
        // boot, and either no storage backend or an unrelated one. Parsing and
        // validating it is what made it look honoured.
        //
        // Refused rather than warned-and-ignored, unlike the `realtime` key below: that
        // one names a subsystem that no longer exists, so an author can only recompile,
        // while this one names a live subsystem configured elsewhere. Naming the working
        // surface is the difference between a refusal and a usable one (#612).
        if raw.get("storage").is_some_and(|v| !v.is_null()) {
            return Err(SchemaLoadError::ValidationError(
                "the compiled schema declares a `storage` section, which no part of the \
                 server reads: the storage backend is built from `[storage]` in the server \
                 config file (or its FRAISEQL_STORAGE_* environment overrides). Move the \
                 bucket configuration there and remove this section, which would otherwise \
                 be silently dropped at boot."
                    .to_string(),
            ));
        }

        // Parse and validate the optional sections.
        let functions = raw
            .get("functions")
            .filter(|v| !v.is_null())
            .map(|v| {
                let cfg: FunctionsConfig = serde_json::from_value(v.clone())?;
                validate_functions_config(&cfg)?;
                Ok::<_, SchemaLoadError>(cfg)
            })
            .transpose()?;

        // The compiled-schema `"realtime"` section is no longer supported (#605): the
        // dormant `/realtime/v1` subsystem was removed. fraiseql-cli never emitted this
        // section (cli and server are version-locked on the format), so only a
        // hand-authored or stale schema could contain one — warn and ignore rather than
        // fail, keeping boot resilient while still surfacing the staleness.
        if raw.get("realtime").is_some_and(|v| !v.is_null()) {
            warn!(
                path = %self.path.display(),
                "compiled-schema `realtime` section is no longer supported and is ignored; \
                 recompile with the current fraiseql-cli"
            );
        }

        info!(
            path = %self.path.display(),
            has_functions = functions.is_some(),
            "Extended schema loaded successfully"
        );

        Ok(ExtendedCompiledSchema { schema, functions })
    }

    /// Get the path to the schema file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Valid trigger prefixes recognised by the trigger system.
const VALID_TRIGGER_PREFIXES: &[&str] = &[
    "after:mutation:",
    "before:mutation:",
    "after:storage:",
    "cron:",
    "http:",
];

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
