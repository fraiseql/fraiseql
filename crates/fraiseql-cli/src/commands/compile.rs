//! Schema compilation command
//!
//! Compiles schema.json (from Python/TypeScript/etc.) into optimized schema.compiled.json

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use fraiseql_core::schema::{
    CURRENT_SCHEMA_FORMAT_VERSION, CompiledSchema, FieldType, InputStyle, MutationOperation,
    NamingConvention, canonicalize_json,
};
use tracing::{info, warn};

use crate::{
    config::TomlProjectConfig,
    schema::{
        ConvertOptions, IntermediateSchema, OptimizationReport, SchemaConverter, SchemaOptimizer,
        SchemaValidator,
        database_validator::validate_schema_against_database,
        mutation_contract::{Severity, validate_mutation_contract},
        pg_catalog::PgCatalog,
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
    /// Skip embedding content hash in compiled schema (for test fixtures).
    pub skip_hash:      bool,
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

        // 2. Parse JSON into IntermediateSchema (language-agnostic format). Parse via Value first
        //    so we can detect a federation block that the SDK emitted but that failed to bind into
        //    the schema (the silent-drop class this issue fixed): the block must carry through or
        //    fail loudly, never vanish into a non-federated subgraph.
        info!("Parsing intermediate schema...");
        let raw: serde_json::Value =
            serde_json::from_str(&schema_json).context("Failed to parse schema.json")?;
        let input_has_federation = ["federation", "federation_config"].iter().any(|key| {
            raw.get(*key)
                .and_then(serde_json::Value::as_object)
                .is_some_and(|o| !o.is_empty())
        });
        let intermediate: IntermediateSchema =
            serde_json::from_value(raw).context("Failed to parse schema.json")?;
        if input_has_federation && intermediate.federation_config.is_none() {
            anyhow::bail!(
                "schema.json carries a `federation` block that did not bind into the compiled \
                 schema — refusing to compile a silently non-federated subgraph. This is a \
                 compiler bug; please report it."
            );
        }
        intermediate
    };

    // 2a. Load and apply security configuration from fraiseql.toml if it exists.
    // Skip when the input itself is a TomlSchema file: in that case the security
    // settings are embedded in the TomlSchema, and the CWD fraiseql.toml uses a
    // different TOML format (TomlSchema vs TomlProjectConfig) that is not compatible.
    // Opt-in mutation-error-union synthesis, read from [fraiseql.mutations] below.
    let mut auto_error_union = false;
    // Casing acronyms from [fraiseql.naming], added on top of the built-in defaults.
    let mut naming_acronyms: Vec<String> = Vec::new();
    // Per-operation @cost weight overrides from [fraiseql.cost_weights] (#379).
    let mut operation_cost_weights: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    // GraphQL surface convention for the legacy JSON (Workflow-B) path. Defaults
    // to camelCase (snake_case DB, camelCase client surface + input recasing);
    // [fraiseql.naming] convention = "preserve" restores the as-authored names.
    // The TomlSchema path carries its own naming_convention via the merger and is
    // left untouched (see the `if !is_toml` apply below).
    let mut naming_convention = NamingConvention::CamelCase;
    if !is_toml && Path::new("fraiseql.toml").exists() {
        info!("Loading security configuration from fraiseql.toml...");
        match TomlProjectConfig::from_file("fraiseql.toml") {
            Ok(config) => {
                info!("Validating security configuration...");
                config.validate()?;

                auto_error_union = config.fraiseql.mutations.auto_error_union;
                naming_acronyms.clone_from(&config.fraiseql.naming.acronyms);
                operation_cost_weights.clone_from(&config.fraiseql.cost_weights);
                naming_convention = config.fraiseql.naming.convention;

                info!("Applying security configuration to schema...");
                // Merge security config into intermediate schema
                let mut security_json = config.fraiseql.security.to_json();

                // Embed tenancy configuration into the security section
                if !matches!(
                    config.fraiseql.tenancy.mode,
                    crate::config::security::TenancyModeConfig::None
                ) {
                    security_json["tenancy"] = config.fraiseql.tenancy.to_json();
                }

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

    // Install the project's casing acronyms so compile-time key inference
    // (native-column resolution, DDL) agrees with the runtime. Defaults apply
    // when none are configured.
    fraiseql_core::utils::casing::set_runtime_acronyms(&naming_acronyms);

    // Apply the naming convention to the legacy JSON (Workflow-B) path. The
    // JSON schema never carries one, so without this it would fall to the enum
    // default (Preserve); Workflow-B instead defaults to CamelCase (overridable
    // via [fraiseql.naming].convention). The TomlSchema path already carries its
    // own convention from the merger, so it is left untouched.
    if !is_toml {
        intermediate.naming_convention = naming_convention;
    }

    // 2b. Validate @tenant_id annotations when tenancy mode is "row".
    // Extract tenancy config from the already-embedded security JSON.
    let tenancy_row_claim: Option<String> = intermediate.security.as_ref().and_then(|sec| {
        let tenancy = sec.get("tenancy")?;
        let mode = tenancy.get("mode").and_then(|m| m.as_str()).unwrap_or("none");
        if mode == "row" {
            Some(
                tenancy
                    .get("tenantClaim")
                    .and_then(|c| c.as_str())
                    .unwrap_or("tenant_id")
                    .to_string(),
            )
        } else {
            None
        }
    });
    if let Some(tenant_claim) = &tenancy_row_claim {
        info!("Validating @tenant_id annotations for row-isolation tenancy...");
        crate::schema::converter::tenancy::validate_tenant_annotations(
            &mut intermediate,
            tenant_claim,
        )
        .context("@tenant_id validation failed")?;
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
    let mut schema =
        SchemaConverter::convert_with_options(intermediate, &ConvertOptions { auto_error_union })
            .context("Failed to convert schema to compiled format")?;

    // Carry the project's casing acronyms into the compiled schema so the runtime
    // installs them at boot (see `fraiseql_db::utils::set_runtime_acronyms`).
    schema.naming_acronyms = naming_acronyms;

    // Carry the project's @cost weight overrides (#379) into the compiled schema so
    // the runtime per-tenant cost-budget check can apply them. Non-clobbering: only
    // overwrite when configured, leaving any value a future merger path may set.
    if !operation_cost_weights.is_empty() {
        schema.operation_cost_weights = operation_cost_weights;
    }

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
        let pg_introspector = build_postgres_introspector(db_url)
            .context("Failed to connect for native column validation")?;
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

        // Mutation call/response contract (#384 item 3: inject_params resolve to real
        // function arguments). PostgreSQL-only — the catalog reads `pg_proc`; other
        // dialects are validated structurally above. Advisory like the rest of
        // `compile --database`: findings warn, they never fail the compile.
        if db_url.starts_with("postgres") {
            info!("Validating mutation contract against the database...");
            let catalog = PgCatalog::connect(db_url)
                .context("Failed to connect for mutation-contract validation")?;
            let contract = validate_mutation_contract(&schema, &catalog).await?;
            for m in &contract.mutations {
                for v in &m.violations {
                    let kind = match v.severity() {
                        Severity::Error => "contract error",
                        Severity::Warn => "contract warning",
                    };
                    warn!("mutation `{}` (sql_source: {}): {v} [{kind}]", m.mutation, m.sql_source);
                }
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

    // 5e. Warn when a `preserve` schema would silently forward camelCase input keys
    // to snake_case SQL functions on the single-JSONB mutation path (#456).
    warn_jsonb_preserve_mismatch(&schema);

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
/// * `emit_ddl` - Optional directory to write `CREATE TABLE` DDL files (confiture format)
/// * `check_migrations` - If true, run `confiture migrate validate` after compilation
/// * `skip_hash` - Skip embedding content hash (for test fixtures)
///
/// # Errors
///
/// Returns error if:
/// - Input file doesn't exist or can't be read
/// - JSON/TOML parsing fails
/// - Schema validation fails
/// - Output file can't be written
/// - Database connection fails (when database URL is provided)
/// - DDL output directory cannot be created (when `emit_ddl` is provided)
/// - `confiture` is not installed (when `check_migrations` is true)
/// - Migration drift detected (when `check_migrations` is true)
#[allow(clippy::too_many_arguments)]
// Reason: run() is the CLI entry point that receives individual args from clap; keeping them
// separate for clarity
#[doc(hidden)] // Internal-pub: CLI entry point dispatched by runner.rs; not a stable downstream API.
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
    emit_ddl: Option<&str>,
    check_migrations: bool,
    skip_hash: bool,
) -> Result<()> {
    // Defense-in-depth: never write the compiled output over the input schema.
    // The removed `serve` command did exactly this (H23) by deriving an output
    // path identical to its input via a faulty extension swap. `--check` writes
    // nothing, so the guard only applies to a real write.
    if !check && Path::new(output) == Path::new(input) {
        anyhow::bail!(
            "Refusing to write compiled output over the input file '{input}'. \
             Use a distinct --output path (e.g. schema.compiled.json)."
        );
    }

    let opts = CompileOptions {
        input,
        types,
        schema_dir,
        type_files,
        query_files,
        mutation_files,
        database,
        skip_hash,
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
    let output_json = if skip_hash {
        serde_json::to_string_pretty(&schema).context("Failed to serialize compiled schema")?
    } else {
        use sha2::{Digest, Sha256};

        let body =
            serde_json::to_string_pretty(&schema).context("Failed to serialize compiled schema")?;
        let value: serde_json::Value = serde_json::from_str(&body)?;
        // Canonicalize (recursively sort keys) before hashing — matches from_json verifier
        let canonical = serde_json::to_string_pretty(&canonicalize_json(&value))?;
        let hash = Sha256::digest(canonical.as_bytes());
        let hash_hex = hex::encode(&hash[..16]);

        let obj = value.as_object().context("schema must serialise as JSON object")?;

        // Insert _content_hash as the first field (serde_json::Map preserves insertion order)
        let mut new_obj = serde_json::Map::new();
        new_obj.insert("_content_hash".to_string(), serde_json::Value::String(hash_hex));
        for (k, v) in obj {
            new_obj.insert(k.clone(), v.clone());
        }
        serde_json::to_string_pretty(&serde_json::Value::Object(new_obj))?
    };

    fs::write(output, output_json).context("Failed to write compiled schema")?;

    // Success message
    println!("✓ Schema compiled successfully");
    println!("  Input:  {input}");
    println!("  Output: {output}");
    println!("  Types: {}", schema.types.len());
    println!("  Queries: {}", schema.queries.len());
    println!("  Mutations: {}", schema.mutations.len());
    optimization_report.print();

    // Emit DDL to directory if requested
    if let Some(ddl_dir) = emit_ddl {
        emit_ddl_to_dir(&schema, ddl_dir)?;
    }

    // Check for migration drift if requested
    if check_migrations {
        run_check_migrations(&schema)?;
    }

    Ok(())
}

/// Emit `CREATE TABLE` DDL files for all compiled schema types to `output_dir`.
///
/// Each type produces one `<type_snake_case>.sql` file containing a `CREATE TABLE IF NOT EXISTS`
/// statement. The output directory is created if it does not already exist.
///
/// Output is compatible with confiture's `db/schema/` directory format, so that
/// `fraiseql migrate generate` can auto-detect drift between the compiled schema and the
/// current migrations.
///
/// # Errors
///
/// Returns an error if the output directory cannot be created, or if any DDL file
/// cannot be written.
pub fn emit_ddl_to_dir(schema: &CompiledSchema, output_dir: &str) -> Result<()> {
    fs::create_dir_all(output_dir)
        .context(format!("Failed to create DDL output directory: {output_dir}"))?;

    let mut count = 0;
    for type_def in &schema.types {
        let table_name = to_snake_case(type_def.name.as_str());
        let ddl = build_create_table_ddl(&table_name, type_def);

        let file_path = Path::new(output_dir).join(format!("{table_name}.sql"));
        fs::write(&file_path, ddl)
            .context(format!("Failed to write DDL for type '{}'", type_def.name))?;
        count += 1;
    }

    println!("✓ DDL emitted to {output_dir}/ ({count} table(s))");
    Ok(())
}

/// Delegate to `confiture migrate validate` for migration drift detection.
///
/// Emits DDL to a temporary directory, then invokes confiture. Exits non-zero when
/// drift is detected, printing a friendly remediation hint.
///
/// # Errors
///
/// Returns an error if confiture is not installed, if the temp directory cannot be
/// created, or if confiture reports drift or validation failures.
fn run_check_migrations(schema: &CompiledSchema) -> Result<()> {
    let tmp_dir = tempfile::tempdir().context("Failed to create temporary DDL directory")?;
    let tmp_path = tmp_dir.path().to_str().context("Temp directory path is not valid UTF-8")?;

    emit_ddl_to_dir(schema, tmp_path)?;

    info!("Running confiture migrate validate for drift detection...");

    let status = Command::new("confiture").args(["migrate", "validate"]).status();

    match status {
        Err(_) => {
            // confiture not installed — warn but don't fail the build
            eprintln!(
                "WARN: confiture is not installed; skipping migration drift check.\n\
                 Install it with: cargo install confiture"
            );
            Ok(())
        },
        Ok(s) if s.success() => {
            println!("✓ No migration drift detected.");
            Ok(())
        },
        Ok(_) => {
            eprintln!(
                "WARN: compiled schema diverges from database — run fraiseql migrate generate"
            );
            anyhow::bail!(
                "Migration drift detected. Run `fraiseql migrate generate` to create a migration."
            )
        },
    }
}

/// Convert a `PascalCase` or `camelCase` type name to `snake_case`.
pub(crate) fn to_snake_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.extend(ch.to_lowercase());
    }
    result
}

/// Generate a `CREATE TABLE IF NOT EXISTS` DDL statement for a compiled type definition.
fn build_create_table_ddl(
    table_name: &str,
    type_def: &fraiseql_core::schema::TypeDefinition,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    lines.push("-- Generated by fraiseql compile --emit-ddl".to_string());
    lines.push(format!("-- Type: {}", type_def.name));
    if let Some(desc) = &type_def.description {
        lines.push(format!("-- {desc}"));
    }
    lines.push(String::new());
    lines.push(format!("CREATE TABLE IF NOT EXISTS tb_{table_name} ("));

    let col_lines: Vec<String> = type_def
        .fields
        .iter()
        .map(|field| {
            let col_name = to_snake_case(field.name.as_str());
            let pg_type = field_type_to_pg(&field.field_type);
            let nullable = if field.nullable { "" } else { " NOT NULL" };
            format!("    {col_name} {pg_type}{nullable}")
        })
        .collect();

    let last = col_lines.len().saturating_sub(1);
    for (i, col) in col_lines.iter().enumerate() {
        if i < last {
            lines.push(format!("{col},"));
        } else {
            lines.push(col.clone());
        }
    }

    lines.push(");".to_string());
    lines.push(String::new());
    lines.join("\n")
}

/// Map a `FieldType` to its PostgreSQL column type string.
pub(crate) fn field_type_to_pg(ft: &FieldType) -> String {
    match ft {
        FieldType::String | FieldType::Scalar(_) => "TEXT".to_string(),
        FieldType::Int => "INTEGER".to_string(),
        FieldType::Float => "DOUBLE PRECISION".to_string(),
        FieldType::Boolean => "BOOLEAN".to_string(),
        FieldType::Id | FieldType::Uuid => "UUID".to_string(),
        FieldType::DateTime => "TIMESTAMPTZ".to_string(),
        FieldType::Date => "DATE".to_string(),
        FieldType::Time => "TIME".to_string(),
        FieldType::Json | FieldType::List(_) | FieldType::Object(_) => "JSONB".to_string(),
        FieldType::Decimal => "NUMERIC".to_string(),
        FieldType::Vector => "VECTOR".to_string(),
        // Use the actual Postgres enum type name so DDL matches the schema.
        FieldType::Enum(name) => name.clone(),
        FieldType::Input(_) | FieldType::Interface(_) | FieldType::Union(_) => "JSONB".to_string(),
        // FieldType is #[non_exhaustive]; future variants default to TEXT.
        _ => "TEXT".to_string(),
    }
}

/// Emit warnings when schema uses features that SQLite does not support.
///
/// SQLite executes direct-SQL Insert/Delete mutations, but lacks Update /
/// stored-procedure (`fn_*`) mutations and relay/subscription support. A
/// compile-time warning helps catch this before runtime failures.
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

    // SQLite executes direct-SQL Insert/Delete mutations via the `DirectSql` strategy;
    // only Update and custom / stored-procedure (`fn_*`) mutations are unsupported.
    let unsupported_mutation_count = schema
        .mutations
        .iter()
        .filter(|m| {
            matches!(
                m.operation,
                fraiseql_core::schema::MutationOperation::Update { .. }
                    | fraiseql_core::schema::MutationOperation::Custom
            )
        })
        .count();
    let relay_count = schema.queries.iter().filter(|q| q.relay).count();
    let subscription_count = schema.subscriptions.len();

    if unsupported_mutation_count > 0 {
        warn!(
            "Schema contains {} Update or custom mutation(s) but target database is SQLite. \
             SQLite supports only direct-SQL Insert/Delete mutations. \
             See: https://fraiseql.dev/docs/database-compatibility",
            unsupported_mutation_count,
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
pub(crate) const WIDE_FANOUT_THRESHOLD: usize = 3;

/// Return mutations whose total invalidation fan-out meets or exceeds `threshold`.
///
/// Fan-out is the count of distinct views (`invalidates_views`) plus fact tables
/// (`invalidates_fact_tables`) that a mutation touches on every successful write.
/// Used by [`warn_wide_cascade_mutations`] and exposed for unit testing.
pub(crate) fn wide_cascade_mutations(
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

/// Detect single-JSONB mutations that would silently forward `camelCase` input
/// keys to a `snake_case` SQL function under `naming_convention = "preserve"`.
///
/// Returns `(mutation_name, camelCase_field_names)` for every mutation that, on a
/// `Preserve` schema, takes one declared `input` Input type through the
/// single-JSONB path (`input_style = "jsonb"` or an `Update`) and whose Input
/// type has `camelCase`-looking field name(s). Under `Preserve` the runtime
/// forwards the payload verbatim — no `snake_case` recasing — so a function
/// reading `payload->>'snake_field'` sees NULL for every multi-word field (#456).
///
/// Pure (no I/O) so it can be unit-tested; [`warn_jsonb_preserve_mismatch`] is the
/// thin logging wrapper.
pub(crate) fn jsonb_preserve_mismatches(schema: &CompiledSchema) -> Vec<(String, Vec<String>)> {
    if schema.naming_convention != NamingConvention::Preserve {
        return Vec::new();
    }
    let mut out = Vec::new();
    for mutation in &schema.mutations {
        // The single-JSONB path: an explicit `input_style = jsonb` or an Update,
        // both of which forward the whole `input` object as one JSONB arg.
        let single_jsonb = matches!(mutation.input_style, InputStyle::Jsonb)
            || matches!(mutation.operation, MutationOperation::Update { .. });
        if !single_jsonb {
            continue;
        }
        // The single-`input`-object pattern: exactly one arg named "input" whose
        // type is a declared input type (mirrors the runtime's detection). The
        // compiler emits input-type references as `FieldType::Object`, never
        // `FieldType::Input`, so recognise an `Object` naming a registered input
        // type too — otherwise this warning is blind to every real compiled schema
        // (#456).
        let input_type_name = match mutation.arguments.as_slice() {
            [arg] if arg.name == "input" => match &arg.arg_type {
                FieldType::Input(name) => name.as_str(),
                FieldType::Object(name) if schema.find_input_type(name).is_some() => name.as_str(),
                _ => continue,
            },
            _ => continue,
        };
        let Some(input_type) = schema.find_input_type(input_type_name) else {
            continue;
        };
        // A field name with any uppercase letter is camelCase-looking — under
        // Preserve it reaches the function verbatim and won't match a snake key.
        let camel_fields: Vec<String> = input_type
            .fields
            .iter()
            .filter(|f| f.name.chars().any(|c| c.is_ascii_uppercase()))
            .map(|f| f.name.clone())
            .collect();
        if !camel_fields.is_empty() {
            out.push((mutation.name.clone(), camel_fields));
        }
    }
    out
}

/// Warn for every [`jsonb_preserve_mismatches`] hit — the silent #456
/// misconfiguration where a `preserve` schema forwards camelCase input keys to a
/// snake_case SQL function on the single-JSONB path.
fn warn_jsonb_preserve_mismatch(schema: &CompiledSchema) {
    for (mutation, fields) in jsonb_preserve_mismatches(schema) {
        warn!(
            "mutation '{mutation}' forwards its input as a single JSONB payload but the schema \
             uses naming_convention = \"preserve\", and its input type has camelCase field(s) \
             [{}]. Under 'preserve' the runtime forwards input keys verbatim (no snake_case \
             recasing), so a SQL function reading payload->>'snake_field' will receive these \
             camelCase keys and see NULL. Set naming_convention = \"camelCase\" (the default for \
             new schemas) if the function expects snake_case keys, or rename the fields. (#456)",
            fields.join(", "),
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
const NATIVE_COLUMN_SKIP_ARGS: &[&str] = &[
    "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
];

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
pub(crate) fn infer_native_columns_from_arg_types(schema: &mut CompiledSchema) {
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
