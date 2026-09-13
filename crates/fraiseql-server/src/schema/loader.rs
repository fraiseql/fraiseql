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

        // #1326: a build that cannot RUN a declared section refuses to boot, rather
        // than loading it and dropping it. See `refuse_unservable_sections`.
        refuse_unservable_sections(&raw, &gated_sections())?;

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

/// How to tell whether a declared section actually asks for something to RUN.
///
/// A section that is present but switched off is **not** a misconfiguration — the
/// operator turned it off, and refusing to boot on it would break a working deployment
/// that simply carries a fuller schema than its binary serves. Every predicate here
/// mirrors the one the serving subsystem itself applies: `if cfg.enabled`,
/// `if definitions.is_empty() { return }`, `if !sources.is_empty()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity {
    /// An object carrying `enabled: bool`. Active when it is `true`.
    EnabledFlag,
    /// An object carrying a `definitions` array. Active when it is non-empty —
    /// the same test `prepare_functions_runtime` makes before doing any work.
    NonEmptyDefinitions,
    /// A top-level array of declarations, each with its own `enabled`. Active when at
    /// least one entry is enabled, which is what the source scheduler starts pollers for.
    AnyEntryEnabled,
}

impl Activity {
    /// Whether `value` — the section as it appears in the compiled schema — asks for
    /// something to run.
    fn is_active(self, value: &serde_json::Value) -> bool {
        match self {
            Activity::EnabledFlag => {
                value.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(false)
            },
            Activity::NonEmptyDefinitions => value
                .get("definitions")
                .and_then(|d| d.as_array())
                .is_some_and(|d| !d.is_empty()),
            Activity::AnyEntryEnabled => value.as_array().is_some_and(|entries| {
                entries
                    .iter()
                    .any(|e| e.get("enabled").and_then(serde_json::Value::as_bool).unwrap_or(false))
            }),
        }
    }
}

/// A compiled-schema section that only a feature-gated subsystem can serve (#1326).
///
/// `compiled_in` is evaluated with `cfg!`, not `#[cfg]`, so both arms type-check in
/// every build and the OFF arm is never a branch nothing compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GatedSection {
    /// The top-level key in `schema.compiled.json`.
    pub name:        &'static str,
    /// The `fraiseql-server` Cargo feature whose subsystem serves it.
    pub feature:     &'static str,
    /// Whether this build has that feature.
    pub compiled_in: bool,
    /// What an operator should do — a published image tag, or the build flag.
    pub remedy:      &'static str,
    /// Whether a present declaration is actually asking for something to run.
    pub activity:    Activity,
}

/// Every compiled-schema section whose *only* consumers sit behind a
/// `fraiseql-server` Cargo feature, and whether this build has that feature.
///
/// Each entry was verified by reading the consumer, not by assuming from the name:
///
/// | section | serving site | gate |
/// |---|---|---|
/// | `functions` | `main.rs` `LoadedSchema.functions` → `prepare_functions_runtime` | `#[cfg(feature = "functions-runtime")]` |
/// | `sources` | `lifecycle.rs` source scheduler | `#[cfg(feature = "sources")]` |
/// | `mcp_config` | `builder.rs` `apply_compiled_config` | `#[cfg(feature = "mcp")]` |
/// | `rest_config` | `routes::rest` | `#[cfg(feature = "rest")]` |
/// | `grpc_config` | `routes::grpc` | `#[cfg(feature = "grpc")]` |
/// | `federation` | `builder.rs` `schema_subsystems` circuit breaker, `routes::api::federation` | `#[cfg(feature = "federation")]` |
/// | `observers_config` | the observer runtime | `#[cfg(feature = "observers")]` |
///
/// `reload_gate.rs` also names most of these, but it only *compares* them across a hot
/// reload — it does not serve them, so it is not a consumer for this purpose.
///
/// A new feature-gated section belongs here. `tools/check-gated-sections.py` fails when
/// the compiled-schema surface grows one that is not listed.
pub(crate) fn gated_sections() -> Vec<GatedSection> {
    vec![
        GatedSection {
            name:        "functions",
            feature:     "functions-runtime",
            compiled_in: cfg!(feature = "functions-runtime"),
            remedy:      "pull the `-platform` image tag, or rebuild with \
                          `--features functions-runtime`",
            activity:    Activity::NonEmptyDefinitions,
        },
        GatedSection {
            name:        "sources",
            feature:     "sources",
            compiled_in: cfg!(feature = "sources"),
            remedy:      "pull the `-platform` image tag, or rebuild with \
                          `--features sources`",
            activity:    Activity::AnyEntryEnabled,
        },
        GatedSection {
            name:        "mcp_config",
            feature:     "mcp",
            compiled_in: cfg!(feature = "mcp"),
            remedy:      "rebuild with `--features mcp`",
            activity:    Activity::EnabledFlag,
        },
        GatedSection {
            name:        "rest_config",
            feature:     "rest",
            compiled_in: cfg!(feature = "rest"),
            remedy:      "rebuild with `--features rest`",
            activity:    Activity::EnabledFlag,
        },
        GatedSection {
            name:        "grpc_config",
            feature:     "grpc",
            compiled_in: cfg!(feature = "grpc"),
            remedy:      "rebuild with `--features grpc`",
            activity:    Activity::EnabledFlag,
        },
        GatedSection {
            name:        "federation",
            feature:     "federation",
            compiled_in: cfg!(feature = "federation"),
            remedy:      "rebuild with `--features federation`",
            activity:    Activity::EnabledFlag,
        },
        GatedSection {
            name:        "observers_config",
            feature:     "observers",
            compiled_in: cfg!(feature = "observers"),
            remedy:      "rebuild with `--features observers`",
            activity:    Activity::EnabledFlag,
        },
    ]
}

/// Refuse a compiled schema that declares a section this build cannot serve.
///
/// # Why this lives at the READ site
///
/// The whole #1326 defect is that the *use* site is compiled out: `LoadedSchema.functions`
/// is `#[cfg(feature = "functions-runtime")]`, so a guard placed beside it disappears in
/// exactly the build that needs one. `load_extended` parses and validates the section
/// unconditionally, so it is the one place that can see a declaration a lean binary is
/// about to drop.
///
/// This is the posture `subsystems/loader.rs` already states in prose — "a declared
/// function that can never run is a misconfiguration, not something to skip silently" —
/// and the one #871 applied to `http:` triggers and #1008 to a `storage` section. The
/// lean build was simply outside it: it boots clean, logs nothing, and every declared
/// function never fires.
///
/// # Errors
///
/// Returns [`SchemaLoadError::ValidationError`] naming the section, the missing feature
/// and the remedy.
pub(crate) fn refuse_unservable_sections(
    raw: &serde_json::Value,
    sections: &[GatedSection],
) -> Result<(), SchemaLoadError> {
    for section in sections {
        if section.compiled_in {
            continue;
        }
        // A `null` is not a declaration — the same rule the `storage` refusal uses, so a
        // serializer that emits every key with an empty value does not fail the boot.
        let Some(value) = raw.get(section.name).filter(|v| !v.is_null()) else {
            continue;
        };
        // Nor is a section that is switched off, or carries nothing to run.
        if !section.activity.is_active(value) {
            continue;
        }
        return Err(SchemaLoadError::ValidationError(format!(
            "the compiled schema declares a `{name}` section, but this build was compiled \
             without the `{feature}` feature, so nothing can run it. It would otherwise be \
             loaded, validated and then silently dropped — the server would boot clean and \
             every declared entry would never fire. To run it: {remedy}. To run without it: \
             remove the `{name}` section and recompile the schema.",
            name = section.name,
            feature = section.feature,
            remedy = section.remedy,
        )));
    }
    Ok(())
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
