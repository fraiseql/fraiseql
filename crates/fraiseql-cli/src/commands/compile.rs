//! Schema compilation command
//!
//! Compiles schema.json (from Python/TypeScript/etc.) into optimized schema.compiled.json

use std::{fs, path::Path};

use anyhow::{Context, Result};
use fraiseql_core::schema::{CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema, FieldType};
use tracing::{info, warn};

use crate::{
    config::TomlProjectConfig,
    schema::{
        IntermediateSchema, OptimizationReport, SchemaConverter, SchemaOptimizer, SchemaValidator,
        database_validator::validate_schema_against_database,
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
#[allow(clippy::cognitive_complexity)] // Reason: multi-strategy schema discovery with fallback chain
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
#[allow(clippy::cognitive_complexity)] // Reason: end-to-end compilation pipeline with validation, introspection, and output stages
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
    // different TOML format (TomlSchema vs TomlProjectConfig) that is not compatible.
    if !is_toml && Path::new("fraiseql.toml").exists() {
        info!("Loading security configuration from fraiseql.toml...");
        match TomlProjectConfig::from_file("fraiseql.toml") {
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

    // 5a. Stamp schema format version for runtime compatibility checks.
    schema.schema_format_version = Some(CURRENT_SCHEMA_FORMAT_VERSION);

    // 5b-pre. Infer native_columns for ID/UUID-typed arguments on JSONB-backed queries.
    // DB introspection (step 5b) overrides these inferred values when `--database` is provided.
    infer_native_columns_from_arg_types(&mut schema);

    // 5b. Optional: Validate indexed columns and native columns against database.
    if let Some(db_url) = opts.database {
        info!("Validating indexed columns against database...");
        validate_indexed_columns(&schema, db_url).await?;

        info!("Validating native columns for direct query arguments...");
        let pg_introspector =
            build_postgres_introspector(db_url).context("Failed to connect for native column validation")?;
        let db_report = validate_schema_against_database(&schema, &pg_introspector).await?;
        for w in &db_report.warnings {
            warn!("{w}");
        }
        // Patch QueryDefinitions with DB-discovered native_columns, overriding inferred values.
        for query in &mut schema.queries {
            if let Some(cols) = db_report.native_columns.get(&query.name) {
                query.native_columns = cols.clone();
            }
        }
    } else {
        // Warn for queries that still have unresolved direct arguments after inference.
        // Arguments already covered by native_columns inference are not warned about.
        for query in &schema.queries {
            if query.sql_source.is_none() {
                continue;
            }
            let unresolved: Vec<_> = query
                .arguments
                .iter()
                .filter(|a| !NATIVE_COLUMN_SKIP_ARGS.contains(&a.name.as_str()))
                .filter(|a| !query.native_columns.contains_key(&a.name))
                .collect();
            if !unresolved.is_empty() {
                let names: Vec<_> = unresolved.iter().map(|a| a.name.as_str()).collect();
                warn!(
                    "query `{}`: argument(s) {:?} on `{}` could not be resolved to native \
                     columns — no --database URL provided. These filters will use JSONB \
                     extraction. Provide --database or annotate with native_columns.",
                    query.name,
                    names,
                    query.sql_source.as_deref().unwrap_or("?"),
                );
            }
        }
    }

    // 5c. Warn when SQLite is the target but the schema uses features SQLite doesn't support.
    check_sqlite_compatibility_warnings(&schema, opts.input, is_toml, opts.database);

    // 5d. Warn when mutations have wide invalidation fan-out (HOT update pressure).
    warn_wide_cascade_mutations(&schema);

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

/// Emit warnings when schema uses features that SQLite does not support.
///
/// SQLite lacks stored procedures (mutations) and relay/subscription support.
/// A compile-time warning helps catch this before runtime failures.
fn check_sqlite_compatibility_warnings(
    schema: &CompiledSchema,
    input_path: &str,
    is_toml: bool,
    database_url: Option<&str>,
) {
    let target_is_sqlite = database_url
        .is_some_and(|url| url.to_ascii_lowercase().starts_with("sqlite://"))
        || is_toml && detect_sqlite_target_in_toml(input_path);

    if !target_is_sqlite {
        return;
    }

    let mutation_count = schema.mutations.len();
    let relay_count = schema.queries.iter().filter(|q| q.relay).count();
    let subscription_count = schema.subscriptions.len();

    if mutation_count > 0 {
        warn!(
            "Schema contains {} mutation(s) but target database is SQLite. \
             Mutations are not supported on SQLite. \
             See: https://fraiseql.dev/docs/database-compatibility",
            mutation_count,
        );
    }
    if relay_count > 0 {
        warn!(
            "Schema contains {} relay query/queries but target database is SQLite. \
             Relay (keyset pagination) is not supported on SQLite. \
             See: https://fraiseql.dev/docs/database-compatibility",
            relay_count,
        );
    }
    if subscription_count > 0 {
        warn!(
            "Schema contains {} subscription(s) but target database is SQLite. \
             Subscriptions are not supported on SQLite. \
             See: https://fraiseql.dev/docs/database-compatibility",
            subscription_count,
        );
    }
}

/// Check if the TOML schema file specifies `database_target = "sqlite"`.
///
/// Reads and parses the TOML to extract the schema metadata. Returns `false`
/// on any parse error (non-fatal — warning detection is best-effort).
fn detect_sqlite_target_in_toml(toml_path: &str) -> bool {
    let Ok(content) = fs::read_to_string(toml_path) else {
        return false;
    };
    let Ok(toml_schema) = toml::from_str::<crate::config::toml_schema::TomlSchema>(&content) else {
        return false;
    };
    toml_schema.schema.database_target.to_ascii_lowercase().contains("sqlite")
}

/// Minimum distinct invalidation targets (views + fact tables) that triggers
/// the HOT-update fan-out warning.
const WIDE_FANOUT_THRESHOLD: usize = 3;

/// Return mutations whose total invalidation fan-out meets or exceeds `threshold`.
///
/// Fan-out is the count of distinct views (`invalidates_views`) plus fact tables
/// (`invalidates_fact_tables`) that a mutation touches on every successful write.
/// Used by [`warn_wide_cascade_mutations`] and exposed for unit testing.
fn wide_cascade_mutations(
    schema: &CompiledSchema,
    threshold: usize,
) -> Vec<&fraiseql_core::schema::MutationDefinition> {
    schema
        .mutations
        .iter()
        .filter(|m| m.invalidates_views.len() + m.invalidates_fact_tables.len() >= threshold)
        .collect()
}

/// Emit a warning for each mutation whose invalidation fan-out is wide enough
/// to risk exhausting PostgreSQL HOT-update page slots under high write load.
///
/// When a mutation touches many tables on every write, the free space reserved
/// on each heap page (needed for HOT updates) fills up quickly. Subsequent
/// mutations must write to a new page instead of updating in place, which
/// increases I/O and table bloat. Setting `fillfactor=70-80` on the backing
/// tables leaves 20-30 % of each page free, keeping HOT updates available.
///
/// The warning lists ready-to-run `ALTER TABLE … SET (fillfactor = 75)` statements
/// derived from the view names using FraiseQL naming conventions
/// (`tv_foo` / `v_foo` → `tb_foo`).
fn warn_wide_cascade_mutations(schema: &CompiledSchema) {
    for mutation in wide_cascade_mutations(schema, WIDE_FANOUT_THRESHOLD) {
        let total = mutation.invalidates_views.len() + mutation.invalidates_fact_tables.len();

        // Build a sorted, deduplicated target list for a stable message.
        let mut targets: Vec<&str> = mutation
            .invalidates_views
            .iter()
            .chain(mutation.invalidates_fact_tables.iter())
            .map(String::as_str)
            .collect();
        targets.sort_unstable();
        targets.dedup();

        // Derive a likely backing-table name from FraiseQL view naming conventions.
        // tv_foo → tb_foo, v_foo → tb_foo, anything else (e.g. fact tables) unchanged.
        let alter_stmts: Vec<String> = targets
            .iter()
            .map(|&name| {
                let table = name
                    .strip_prefix("tv_")
                    .or_else(|| name.strip_prefix("v_"))
                    .map_or_else(|| name.to_string(), |rest| format!("tb_{rest}"));
                format!("ALTER TABLE {table} SET (fillfactor = 75);")
            })
            .collect();

        warn!(
            "mutation '{}' has a wide invalidation fan-out ({} targets: [{}]). \
             Under high write load, HOT-update page slots on these tables may be \
             exhausted, forcing full-page writes and reducing mutation throughput. \
             Set fillfactor=70-80 on the backing tables: {}  \
             Monitor HOT efficiency: SELECT relname, \
             n_tup_hot_upd * 100 / NULLIF(n_tup_upd, 0) AS hot_pct \
             FROM pg_stat_user_tables WHERE n_tup_upd > 0 ORDER BY hot_pct;",
            mutation.name,
            total,
            targets.join(", "),
            alter_stmts.join("  "),
        );
    }
}

/// Build a PostgreSQL introspector connected to `db_url`.
///
/// Shared by `validate_indexed_columns` and the native column validation path.
///
/// # Errors
///
/// Returns error if the pool cannot be created or the connection URL is invalid.
fn build_postgres_introspector(
    db_url: &str,
) -> Result<fraiseql_core::db::postgres::PostgresIntrospector> {
    use deadpool_postgres::{Config, ManagerConfig, RecyclingMethod, Runtime};
    use tokio_postgres::NoTls;

    let mut cfg = Config::new();
    cfg.url = Some(db_url.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(2));

    let pool = cfg
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("Failed to create connection pool for database validation")?;

    Ok(fraiseql_core::db::postgres::PostgresIntrospector::new(pool))
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

/// Auto-param names excluded from `native_columns` inference and JSONB-extraction warnings.
const NATIVE_COLUMN_SKIP_ARGS: &[&str] =
    &["where", "limit", "offset", "orderBy", "first", "last", "after", "before"];

/// Infer `native_columns` for `ID`/`UUID`-typed arguments on JSONB-backed queries.
///
/// When a query reads from a JSONB table (`sql_source` + non-empty `jsonb_column`) and an
/// argument is typed [`FieldType::Id`] or [`FieldType::Uuid`], the argument name almost
/// certainly maps to a native UUID column alongside the `data` JSONB column
/// (e.g. `id UUID NOT NULL`). Emitting `WHERE id = $1::uuid` instead of
/// `WHERE data->>'id' = $1` lets the planner use the B-tree index without
/// needing a database connection at compile time.
///
/// Auto-param names (`where`, `limit`, `offset`, etc.) are skipped.
/// Arguments already present in `native_columns` are not overridden.
fn infer_native_columns_from_arg_types(schema: &mut CompiledSchema) {
    for query in &mut schema.queries {
        if query.sql_source.is_none() || query.jsonb_column.is_empty() {
            continue;
        }
        for arg in &query.arguments {
            if NATIVE_COLUMN_SKIP_ARGS.contains(&arg.name.as_str()) {
                continue;
            }
            if query.native_columns.contains_key(&arg.name) {
                continue; // already explicitly declared — don't override
            }
            if matches!(arg.arg_type, FieldType::Id | FieldType::Uuid) {
                query.native_columns.insert(arg.name.clone(), "uuid".to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        schema::{
            ArgumentDefinition, AutoParams, CompiledSchema, CursorType, FieldDefinition,
            FieldDenyPolicy, FieldType, QueryDefinition, TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    use super::infer_native_columns_from_arg_types;

    use fraiseql_core::schema::MutationDefinition;

    use super::{WIDE_FANOUT_THRESHOLD, wide_cascade_mutations};

    fn mutation_with_fanout(
        name: &str,
        views: &[&str],
        fact_tables: &[&str],
    ) -> MutationDefinition {
        let mut m = MutationDefinition::new(name, "SomeResult");
        m.invalidates_views = views.iter().map(|s| (*s).to_string()).collect();
        m.invalidates_fact_tables = fact_tables.iter().map(|s| (*s).to_string()).collect();
        m
    }

    #[test]
    fn test_wide_cascade_below_threshold_not_flagged() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout("update", &["tv_user", "tv_post"], &[])],
            ..Default::default()
        };
        assert!(
            wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD).is_empty(),
            "2 targets is below threshold of 3"
        );
    }

    #[test]
    fn test_wide_cascade_at_threshold_flagged() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout(
                "updateUserWithPosts",
                &["tv_user", "tv_post", "tv_comment"],
                &[],
            )],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].name, "updateUserWithPosts");
    }

    #[test]
    fn test_wide_cascade_views_plus_fact_tables_counted_together() {
        let schema = CompiledSchema {
            mutations: vec![mutation_with_fanout(
                "createOrder",
                &["tv_order", "tv_order_item"],
                &["tf_sales"],
            )],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1, "2 views + 1 fact table = 3 total, meets threshold");
    }

    #[test]
    fn test_wide_cascade_only_wide_mutations_flagged() {
        let schema = CompiledSchema {
            mutations: vec![
                mutation_with_fanout("narrow", &["tv_user"], &[]),
                mutation_with_fanout("wide", &["tv_user", "tv_post", "tv_comment"], &[]),
            ],
            ..Default::default()
        };
        let flagged = wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].name, "wide");
    }

    #[test]
    fn test_wide_cascade_no_mutations_no_warnings() {
        let schema = CompiledSchema::default();
        assert!(wide_cascade_mutations(&schema, WIDE_FANOUT_THRESHOLD).is_empty());
    }

    #[test]
    fn test_validate_schema_success() {
        let schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "User".into(),
                fields:              vec![
                    FieldDefinition {
                        name:           "id".into(),
                        field_type:     FieldType::Int,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        encryption:     None,
                    },
                    FieldDefinition {
                        name:           "name".into(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        encryption:     None,
                    },
                ],
                description:         Some("User type".to_string()),
                sql_source:          String::new().into(),
                jsonb_column:        String::new(),
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       Vec::new(),
            }],
            queries: vec![QueryDefinition {
                name:                "users".to_string(),
                return_type:         "User".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![],
                sql_source:          Some("v_user".to_string()),
                description:         Some("Get users".to_string()),
                auto_params:         AutoParams::default(),
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            // None is intentional here: this struct is used only for in-process
            // validation assertions and is never serialised to disk. The real
            // compile path stamps the version at compile_impl() line 220.
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        // Validation is done inside SchemaConverter::convert, not exposed separately
        // This test just verifies we can build a valid schema structure
        assert_eq!(schema.types.len(), 1);
        assert_eq!(schema.queries.len(), 1);
    }

    #[test]
    fn test_validate_schema_unknown_type() {
        let schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![QueryDefinition {
                name:                "users".to_string(),
                return_type:         "UnknownType".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![],
                sql_source:          Some("v_user".to_string()),
                description:         Some("Get users".to_string()),
                auto_params:         AutoParams::default(),
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
            }],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            // None is intentional here: this struct is used only for in-process
            // validation assertions and is never serialised to disk. The real
            // compile path stamps the version at compile_impl() line 220.
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        // Note: Validation is private to SchemaConverter
        // This test demonstrates the schema structure with an invalid type
        assert_eq!(schema.types.len(), 0);
        assert_eq!(schema.queries[0].return_type, "UnknownType");
    }

    fn make_query(
        name: &str,
        sql_source: Option<&str>,
        jsonb_column: &str,
        args: Vec<(&str, FieldType)>,
        native_columns: std::collections::HashMap<String, String>,
    ) -> QueryDefinition {
        QueryDefinition {
            name:                name.to_string(),
            return_type:         "T".to_string(),
            returns_list:        false,
            nullable:            true,
            arguments:           args
                .into_iter()
                .map(|(n, t)| ArgumentDefinition::new(n, t))
                .collect(),
            sql_source:          sql_source.map(str::to_string),
            jsonb_column:        jsonb_column.to_string(),
            native_columns,
            auto_params:         AutoParams::default(),
            ..Default::default()
        }
    }

    #[test]
    fn test_infer_id_arg_becomes_uuid_native_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert_eq!(
            schema.queries[0].native_columns.get("id").map(String::as_str),
            Some("uuid"),
            "ID-typed arg should be inferred as uuid native column"
        );
    }

    #[test]
    fn test_infer_uuid_arg_becomes_uuid_native_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("userId", FieldType::Uuid)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert_eq!(
            schema.queries[0].native_columns.get("userId").map(String::as_str),
            Some("uuid")
        );
    }

    #[test]
    fn test_infer_does_not_override_explicit_declaration() {
        let mut explicit = std::collections::HashMap::new();
        explicit.insert("id".to_string(), "text".to_string()); // explicit, non-uuid
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("id", FieldType::Id)],
                explicit,
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        // explicit "text" must not be overridden by the inferred "uuid"
        assert_eq!(
            schema.queries[0].native_columns.get("id").map(String::as_str),
            Some("text"),
            "explicit native_columns declaration must win over inference"
        );
    }

    #[test]
    fn test_infer_skips_queries_without_sql_source() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                None,
                "data",
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "queries without sql_source must not get inferred native_columns"
        );
    }

    #[test]
    fn test_infer_skips_queries_without_jsonb_column() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("v_user"),
                "", // no jsonb_column — plain column view
                vec![("id", FieldType::Id)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "queries without jsonb_column must not get inferred native_columns"
        );
    }

    #[test]
    fn test_infer_skips_non_id_types() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "user",
                Some("tv_user"),
                "data",
                vec![("username", FieldType::String), ("age", FieldType::Int)],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "String/Int args must not be inferred as native columns"
        );
    }

    #[test]
    fn test_infer_skips_auto_param_names() {
        let mut schema = CompiledSchema {
            queries: vec![make_query(
                "users",
                Some("tv_user"),
                "data",
                vec![
                    ("where", FieldType::Id),
                    ("limit", FieldType::Id),
                    ("orderBy", FieldType::Id),
                ],
                std::collections::HashMap::new(),
            )],
            ..Default::default()
        };
        infer_native_columns_from_arg_types(&mut schema);
        assert!(
            schema.queries[0].native_columns.is_empty(),
            "auto-param names must never be inferred as native columns even if typed ID"
        );
    }
}
