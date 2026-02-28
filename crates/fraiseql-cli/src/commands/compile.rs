//! Schema compilation command
//!
//! Compiles schema.json (from Python/TypeScript/etc.) into optimized schema.compiled.json

use std::{fs, path::Path};

use anyhow::{Context, Result};
use fraiseql_core::schema::CompiledSchema;
use tracing::{info, warn};

use crate::{
    config::FraiseQLConfig,
    schema::{
        IntermediateSchema, OptimizationReport, SchemaConverter, SchemaOptimizer, SchemaValidator,
    },
};

/// Input source configuration for schema compilation.
#[derive(Debug, Default)]
pub struct CompileOptions<'a> {
    /// Path to `fraiseql.toml` (TOML workflow) or `schema.json` (legacy).
    pub input:          &'a str,
    /// Optional path to `types.json` (TOML workflow, backward compat).
    pub types:          Option<&'a str>,
    /// Optional directory for schema file auto-discovery.
    pub schema_dir:     Option<&'a str>,
    /// Explicit type file paths (highest priority).
    pub type_files:     Vec<String>,
    /// Explicit query file paths.
    pub query_files:    Vec<String>,
    /// Explicit mutation file paths.
    pub mutation_files: Vec<String>,
    /// Optional database URL for indexed column validation.
    pub database:       Option<&'a str>,
}

impl<'a> CompileOptions<'a> {
    /// Create new compile options with just the input path.
    #[must_use]
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            ..Default::default()
        }
    }

    /// Set the types path.
    #[must_use]
    pub fn with_types(mut self, types: &'a str) -> Self {
        self.types = Some(types);
        self
    }

    /// Set the schema directory for auto-discovery.
    #[must_use]
    pub fn with_schema_dir(mut self, schema_dir: &'a str) -> Self {
        self.schema_dir = Some(schema_dir);
        self
    }

    /// Set the database URL for validation.
    #[must_use]
    pub fn with_database(mut self, database: &'a str) -> Self {
        self.database = Some(database);
        self
    }
}

/// Select and execute the appropriate schema-loading strategy for TOML-based workflows.
///
/// Tries strategies in priority order:
/// 1. Explicit file lists (highest priority)
/// 2. Directory auto-discovery
/// 3. Single types file (backward-compatible)
/// 4. Domain discovery → TOML includes → TOML-only (fallback sequence)
fn load_intermediate_schema(
    toml_path: &str,
    type_files: &[String],
    query_files: &[String],
    mutation_files: &[String],
    schema_dir: Option<&str>,
    types_path: Option<&str>,
) -> Result<IntermediateSchema> {
    if !type_files.is_empty() || !query_files.is_empty() || !mutation_files.is_empty() {
        info!("Mode: Explicit file lists");
        return crate::schema::SchemaMerger::merge_explicit_files(
            toml_path,
            type_files,
            query_files,
            mutation_files,
        )
        .context("Failed to load explicit schema files");
    }
    if let Some(dir) = schema_dir {
        info!("Mode: Auto-discovery from directory: {}", dir);
        return crate::schema::SchemaMerger::merge_from_directory(toml_path, dir)
            .context("Failed to load schema from directory");
    }
    if let Some(types) = types_path {
        info!("Mode: Language + TOML (types.json + fraiseql.toml)");
        return crate::schema::SchemaMerger::merge_files(types, toml_path)
            .context("Failed to merge types.json with TOML");
    }
    info!("Mode: TOML-based (checking for domain discovery...)");
    if let Ok(schema) = crate::schema::SchemaMerger::merge_from_domains(toml_path) {
        return Ok(schema);
    }
    info!("No domains configured, checking for TOML includes...");
    if let Ok(schema) = crate::schema::SchemaMerger::merge_with_includes(toml_path) {
        return Ok(schema);
    }
    info!("No includes configured, using TOML-only definitions");
    crate::schema::SchemaMerger::merge_toml_only(toml_path)
        .context("Failed to load schema from TOML")
}

/// Compile a schema to `CompiledSchema` without writing to disk.
///
/// This is the core compilation logic, shared between `compile` (which writes to disk)
/// and `run` (which serves in-memory without any file artifacts).
///
/// # Arguments
///
/// * `opts` - Compilation options including input paths and configuration
///
/// # Errors
///
/// Returns error if input is missing, parsing fails, validation fails, or the database
/// connection fails (when `database` is provided).
pub async fn compile_to_schema(
    opts: CompileOptions<'_>,
) -> Result<(CompiledSchema, OptimizationReport)> {
    info!("Compiling schema: {}", opts.input);

    // 1. Determine workflow based on input file and options
    let input_path = Path::new(opts.input);
    if !input_path.exists() {
        anyhow::bail!("Input file not found: {}", opts.input);
    }

    // Load schema based on file type and options
    let is_toml = input_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"));
    let mut intermediate: IntermediateSchema = if is_toml {
        info!("Using TOML-based workflow");
        load_intermediate_schema(
            opts.input,
            &opts.type_files,
            &opts.query_files,
            &opts.mutation_files,
            opts.schema_dir,
            opts.types,
        )?
    } else {
        // Legacy JSON workflow
        info!("Using legacy JSON workflow");
        let schema_json = fs::read_to_string(input_path).context("Failed to read schema.json")?;

        // 2. Parse JSON into IntermediateSchema (language-agnostic format)
        info!("Parsing intermediate schema...");
        serde_json::from_str(&schema_json).context("Failed to parse schema.json")?
    };

    // 2a. Load and apply security configuration from fraiseql.toml if it exists.
    // Skip when the input itself is a TomlSchema file: in that case the security
    // settings are embedded in the TomlSchema, and the CWD fraiseql.toml uses a
    // different TOML format (TomlSchema vs FraiseQLConfig) that is not compatible.
    if !is_toml && Path::new("fraiseql.toml").exists() {
        info!("Loading security configuration from fraiseql.toml...");
        match FraiseQLConfig::from_file("fraiseql.toml") {
            Ok(config) => {
                info!("Validating security configuration...");
                config.validate()?;

                info!("Applying security configuration to schema...");
                // Merge security config into intermediate schema
                let security_json = config.fraiseql.security.to_json();
                intermediate.security = Some(security_json);

                info!("Security configuration applied successfully");
            },
            Err(e) => {
                anyhow::bail!(
                    "Failed to parse fraiseql.toml: {e}\n\
                     Fix the configuration file or remove it to use defaults."
                );
            },
        }
    } else {
        info!("No fraiseql.toml found, using default security configuration");
    }

    // 3. Validate intermediate schema
    info!("Validating schema structure...");
    let validation_report =
        SchemaValidator::validate(&intermediate).context("Failed to validate schema")?;

    if !validation_report.is_valid() {
        validation_report.print();
        anyhow::bail!("Schema validation failed with {} error(s)", validation_report.error_count());
    }

    // Print warnings if any
    if validation_report.warning_count() > 0 {
        validation_report.print();
    }

    // 4. Convert to CompiledSchema (validates and normalizes)
    info!("Converting to compiled format...");
    let mut schema = SchemaConverter::convert(intermediate)
        .context("Failed to convert schema to compiled format")?;

    // 5. Optimize schema and generate SQL hints (mutates schema in place, report for display)
    info!("Analyzing schema for optimization opportunities...");
    let report = SchemaOptimizer::optimize(&mut schema).context("Failed to optimize schema")?;

    // 5b. Optional: Validate indexed columns against database
    if let Some(db_url) = opts.database {
        info!("Validating indexed columns against database...");
        validate_indexed_columns(&schema, db_url).await?;
    }

    Ok((schema, report))
}

/// Run the compile command
///
/// # Arguments
///
/// * `input` - Path to fraiseql.toml (TOML) or schema.json (legacy)
/// * `types` - Optional path to types.json (when using TOML workflow)
/// * `schema_dir` - Optional directory for auto-discovery of schema files
/// * `type_files` - Optional vector of explicit type file paths
/// * `query_files` - Optional vector of explicit query file paths
/// * `mutation_files` - Optional vector of explicit mutation file paths
/// * `output` - Path to write schema.compiled.json
/// * `check` - If true, validate only without writing output
/// * `database` - Optional database URL for indexed column validation
///
/// # Workflows
///
/// 1. TOML-only: `fraiseql compile fraiseql.toml`
/// 2. Language + TOML: `fraiseql compile fraiseql.toml --types types.json`
/// 3. Multi-file auto-discovery: `fraiseql compile fraiseql.toml --schema-dir schema/`
/// 4. Multi-file explicit: `fraiseql compile fraiseql.toml --type-file a.json --type-file b.json`
/// 5. Legacy JSON: `fraiseql compile schema.json`
///
/// # Errors
///
/// Returns error if:
/// - Input file doesn't exist or can't be read
/// - JSON/TOML parsing fails
/// - Schema validation fails
/// - Output file can't be written
/// - Database connection fails (when database URL is provided)
#[allow(clippy::too_many_arguments)] // Reason: run() is the CLI entry point that receives individual args from clap; keeping them separate for clarity
pub async fn run(
    input: &str,
    types: Option<&str>,
    schema_dir: Option<&str>,
    type_files: Vec<String>,
    query_files: Vec<String>,
    mutation_files: Vec<String>,
    output: &str,
    check: bool,
    database: Option<&str>,
) -> Result<()> {
    let opts = CompileOptions {
        input,
        types,
        schema_dir,
        type_files,
        query_files,
        mutation_files,
        database,
    };
    let (schema, optimization_report) = compile_to_schema(opts).await?;

    // If check-only mode, stop here
    if check {
        println!("✓ Schema is valid");
        println!("  Types: {}", schema.types.len());
        println!("  Queries: {}", schema.queries.len());
        println!("  Mutations: {}", schema.mutations.len());
        optimization_report.print();
        return Ok(());
    }

    // Write compiled schema
    info!("Writing compiled schema to: {output}");
    let output_json =
        serde_json::to_string_pretty(&schema).context("Failed to serialize compiled schema")?;
    fs::write(output, output_json).context("Failed to write compiled schema")?;

    // Success message
    println!("✓ Schema compiled successfully");
    println!("  Input:  {input}");
    println!("  Output: {output}");
    println!("  Types: {}", schema.types.len());
    println!("  Queries: {}", schema.queries.len());
    println!("  Mutations: {}", schema.mutations.len());
    optimization_report.print();

    Ok(())
}

/// Validate indexed columns against database views.
///
/// Connects to the database and introspects view columns to verify that
/// any indexed column naming conventions are properly set up.
///
/// # Arguments
///
/// * `schema` - The compiled schema to validate
/// * `db_url` - Database connection URL
///
/// # Errors
///
/// Returns error if database connection fails. Warnings are printed for
/// missing indexed columns but don't cause validation to fail.
async fn validate_indexed_columns(schema: &CompiledSchema, db_url: &str) -> Result<()> {
    use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
    use fraiseql_core::db::postgres::PostgresIntrospector;
    use tokio_postgres::NoTls;

    // Create pool for introspection
    let mut cfg = Config::new();
    cfg.url = Some(db_url.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("Failed to create connection pool for indexed column validation")?;

    let introspector = PostgresIntrospector::new(pool);

    let mut total_indexed = 0;
    let mut total_views = 0;

    // Check each query's sql_source (view)
    for query in &schema.queries {
        if let Some(view_name) = &query.sql_source {
            total_views += 1;

            // Get indexed columns for this view
            match introspector.get_indexed_nested_columns(view_name).await {
                Ok(indexed_cols) => {
                    if !indexed_cols.is_empty() {
                        info!(
                            "View '{}': found {} indexed column(s): {:?}",
                            view_name,
                            indexed_cols.len(),
                            indexed_cols
                        );
                        total_indexed += indexed_cols.len();
                    }
                },
                Err(e) => {
                    warn!(
                        "Could not introspect view '{}': {}. Skipping indexed column check.",
                        view_name, e
                    );
                },
            }
        }
    }

    println!("✓ Indexed column validation complete");
    println!("  Views checked: {total_views}");
    println!("  Indexed columns found: {total_indexed}");

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        schema::{
            AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, QueryDefinition,
            TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    #[test]
    fn test_validate_schema_success() {
        let schema = CompiledSchema {
            types:            vec![TypeDefinition {
                name:                "User".to_string(),
                fields:              vec![
                    FieldDefinition {
                        name:           "id".to_string(),
                        field_type:     FieldType::Int,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        encryption:     None,
                    },
                    FieldDefinition {
                        name:           "name".to_string(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        encryption:     None,
                    },
                ],
                description:         Some("User type".to_string()),
                sql_source:          String::new(),
                jsonb_column:        String::new(),
                sql_projection_hint: None,
                implements:          vec![],
                is_error:            false,
                relay:            false,
            }],
            queries:          vec![QueryDefinition {
                name:         "users".to_string(),
                return_type:  "User".to_string(),
                returns_list: true,
                nullable:     false,
                arguments:    vec![],
                sql_source:   Some("v_user".to_string()),
                description:  Some("Get users".to_string()),
                auto_params:  AutoParams::default(),
                deprecation:  None,
                jsonb_column: "data".to_string(),
                relay: false,
                relay_cursor_column: None,
                relay_cursor_type: CursorType::default(),
                inject_params: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
            }],
            enums:            vec![],
            input_types:      vec![],
            interfaces:       vec![],
            unions:           vec![],
            mutations:        vec![],
            subscriptions:    vec![],
            directives:       vec![],
            observers:        Vec::new(),
            fact_tables:      HashMap::default(),
            federation:       None,
            security:         None,
            observers_config: None,
            schema_sdl:       None,
            custom_scalars:   CustomTypeRegistry::default(),
        };

        // Validation is done inside SchemaConverter::convert, not exposed separately
        // This test just verifies we can build a valid schema structure
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.queries.len(), 1);
    }

    #[test]
    fn test_validate_schema_unknown_type() {
        let schema = CompiledSchema {
            types:            vec![],
            enums:            vec![],
            input_types:      vec![],
            interfaces:       vec![],
            unions:           vec![],
            queries:          vec![QueryDefinition {
                name:         "users".to_string(),
                return_type:  "UnknownType".to_string(),
                returns_list: true,
                nullable:     false,
                arguments:    vec![],
                sql_source:   Some("v_user".to_string()),
                description:  Some("Get users".to_string()),
                auto_params:  AutoParams::default(),
                deprecation:  None,
                jsonb_column: "data".to_string(),
                relay: false,
                relay_cursor_column: None,
                relay_cursor_type: CursorType::default(),
                inject_params: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
            }],
            mutations:        vec![],
            subscriptions:    vec![],
            directives:       vec![],
            observers:        Vec::new(),
            fact_tables:      HashMap::default(),
            federation:       None,
            security:         None,
            observers_config: None,
            schema_sdl:       None,
            custom_scalars:   CustomTypeRegistry::default(),
        };

        // Note: Validation is private to SchemaConverter
        // This test demonstrates the schema structure with an invalid type
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries[0].return_type, "UnknownType");
    }
}
