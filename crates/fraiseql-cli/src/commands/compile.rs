//! Schema compilation command
//!
//! Compiles schema.json (from Python/TypeScript/etc.) into optimized schema.compiled.json

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result};
use fraiseql_core::schema::{
    CompiledSchema, FieldType, InputStyle, MutationOperation, NamingConvention, content_hash_of,
};
use fraiseql_db::postgres::PostgresTlsConfig;
use tracing::{info, warn};

use crate::{
    config::{ConfigSource, TomlProjectConfig},
    schema::{
        CompiledArtifact, ConvertOptions, IntermediateSchema, OptimizationReport, SchemaConverter,
        SchemaOptimizer, SchemaValidator,
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
    /// Downgrade error-severity schema↔database drift findings to advisories
    /// (#384). Without this, error-severity drift fails the compile.
    pub allow_drift:    bool,
    /// The project config to compile with. `None` resolves it from `input` with
    /// [`ConfigSource::resolve`], exactly as `compile` and `run` do — never from the
    /// working directory (#1387).
    pub config:         Option<ConfigSource>,
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
///
/// # Errors
///
/// Returns an error if a **configured** schema source fails to load. Distinguishing
/// "not configured" (fall through to the next strategy) from "configured but failed"
/// (propagate) is the substance of #723 — see the fallback sequence below.
#[allow(clippy::cognitive_complexity)] // Reason: multi-strategy schema discovery with fallback chain
pub fn load_intermediate_schema(
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
    // Fallback sequence: domain discovery → TOML includes → TOML-only.
    //
    // Each step asks "is this configured?" **before** attempting it, so a failure inside a
    // configured source propagates instead of being swallowed by the next fallback (#723).
    //
    // This used to be `if let Ok(schema) = merge_from_domains(toml_path)`, twice. A bad
    // `root_dir`, an unreadable domain file or a JSON parse error was discarded and
    // compilation fell through to TOML-only definitions — producing either a schema silently
    // missing the user's domain types, or a later death with the misleading "Failed to load
    // schema from TOML" naming the wrong thing entirely. That directly contradicts the #612
    // doctrine this codebase otherwise holds to: configured input that fails must fail loud.
    let toml_schema = crate::config::TomlSchema::from_file(toml_path)
        .context(format!("Failed to load TOML from {toml_path}"))?;

    if toml_schema.domain_discovery.enabled {
        info!("Mode: TOML-based with domain discovery");
        return crate::schema::SchemaMerger::merge_from_domains(toml_path).context(
            "Failed to load schema from the configured [domain_discovery] section. Fix the \
             error above, or remove the section to compile from TOML definitions only.",
        );
    }

    if !toml_schema.includes.is_empty() {
        info!("Mode: TOML-based with schema includes");
        return crate::schema::SchemaMerger::merge_with_includes(toml_path).context(
            "Failed to load schema from the configured [includes] section. Fix the error \
             above, or remove the section to compile from TOML definitions only.",
        );
    }

    info!("No domains or includes configured, using TOML-only definitions");
    crate::schema::SchemaMerger::merge_toml_only(toml_path)
        .context("Failed to load schema from TOML")
}

/// Compile a schema to the whole `schema.compiled.json` artifact, without writing it.
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
) -> Result<(CompiledArtifact, OptimizationReport)> {
    info!("Compiling schema: {}", opts.input);

    // A removed engine's URL is refused before any work, naming the PostgreSQL-only rule
    // (#1341), rather than failing inside the connection pool at step 5b.
    if let Some(db_url) = opts.database {
        crate::connection::require_postgres(db_url)?;
    }

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

        // Refuse a security control declared under a key that does not bind (#806/#807).
        // Must run on the raw JSON: after deserialization the evidence is gone, which is
        // precisely the defect — an unread key becomes an empty default and nothing
        // downstream can distinguish "no scope declared" from "scope declared and lost".
        crate::schema::intermediate::reject_drifted_keys(&raw)
            .map_err(|e| anyhow::anyhow!("{e}"))?;

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

    // 2a. Load and apply the project config that belongs to this input (#1387).
    // A TOML input carries its settings itself (`ConfigSource::Input` names no
    // separate file): it is a TomlSchema, a different format from TomlProjectConfig.
    let config_source = match opts.config {
        Some(source) => source,
        None => ConfigSource::resolve(input_path, None)?,
    };
    info!("Project config: {config_source}");
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
    // Transport security for the `--database` checks (#1429): `[database] ssl_mode` from
    // the config this compile loaded. Unset leaves the URL's own `?sslmode=` in charge.
    let mut database_tls = if is_toml && opts.database.is_some() {
        crate::config::TomlSchema::from_file(opts.input)
            .with_context(|| format!("Failed to load TOML from {}", opts.input))?
            .database
            .postgres_tls()?
    } else {
        PostgresTlsConfig::default()
    };
    if let Some(config_path) = config_source.project_config() {
        info!("Loading security configuration from {}...", config_path.display());
        let config_file = config_path.to_str().ok_or_else(|| {
            anyhow::anyhow!("Config path is not valid UTF-8: {}", config_path.display())
        })?;
        match TomlProjectConfig::from_file(config_file) {
            Ok(config) => {
                info!("Validating security configuration...");
                config.validate()?;

                auto_error_union = config.fraiseql.mutations.auto_error_union;
                naming_acronyms.clone_from(&config.fraiseql.naming.acronyms);
                operation_cost_weights.clone_from(&config.fraiseql.cost_weights);
                naming_convention = config.fraiseql.naming.convention;
                database_tls = config.database.postgres_tls()?;

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

                // Session variables are the mechanism RLS policies read; carry them
                // through rather than leaving `schema.json` as the only way to
                // declare them (#628).
                if config.fraiseql.session_variables
                    != fraiseql_core::schema::SessionVariablesConfig::default()
                {
                    intermediate.session_variables = Some(config.fraiseql.session_variables);
                }

                // `[locale]` (#1512) is configuration, so it lives only in `fraiseql.toml`; a
                // schema document carrying a different one is an authoring conflict.
                if let Some(locale) = config.locale {
                    if intermediate.locale.as_ref().is_some_and(|l| *l != locale) {
                        anyhow::bail!(
                            "[locale] in {} disagrees with the `locale` the schema document \
                             declares; declare it once, in fraiseql.toml",
                            config_path.display()
                        );
                    }
                    intermediate.locale = Some(locale);
                }

                // `[inject_defaults]` lives in the config the SDK loaders read (#1384); a
                // copy the SDK emitted into the schema must agree with it.
                intermediate.inject_defaults =
                    crate::config::inject_defaults::InjectDefaultsToml::reconcile(
                        config.inject_defaults.as_ref(),
                        intermediate.inject_defaults.take(),
                    )?;

                info!("Security configuration applied successfully");
            },
            Err(e) => {
                anyhow::bail!(
                    "Failed to parse {}: {e}\n\
                     Fix the configuration file or remove it to use defaults.",
                    config_path.display()
                );
            },
        }
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
            // `tenant_claim`, matching `fraiseql_core::schema::TenancyConfig`. This
            // read `tenantClaim` while the runtime read `tenant_claim`, so the
            // compiler validated `@tenant_id` against one claim and the server
            // injected another (#757).
            Some(
                tenancy
                    .get("tenant_claim")
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
        // The errors themselves, not only their count: a caller of `compile_to_schema` that
        // is not a terminal (an embedder, `fraiseql run`) has nothing else to read (#1530).
        let errors: Vec<String> = validation_report
            .errors
            .iter()
            .filter(|e| e.severity == crate::schema::validator::ErrorSeverity::Error)
            .map(|e| format!("{} ({})", e.message, e.path))
            .collect();
        anyhow::bail!(
            "Schema validation failed with {} error(s):\n  - {}",
            validation_report.error_count(),
            errors.join("\n  - ")
        );
    }

    // Print warnings if any
    if validation_report.warning_count() > 0 {
        validation_report.print();
    }

    // 4. Convert to the compiled artifact (validates and normalizes). `functions` is
    // a sibling section of the compiled schema, not a field of it, so the converter
    // hands both halves back and the rest of this pipeline sharpens the schema half.
    // Which parameters each mutation receives from `[inject_defaults]` rather than declares
    // (#1385), so a contract error a default caused can name the default instead of
    // repeating once per mutation. Computed by the converter's own rule; a refusal that
    // rule raises is the converter's to report.
    let mutation_defaults =
        intermediate.inject_defaults.clone().unwrap_or_default().for_mutations();
    let default_args: std::collections::HashMap<String, Vec<String>> = intermediate
        .mutations
        .iter()
        .filter_map(|m| {
            let mut inject = m.inject.clone();
            crate::schema::intermediate::IntermediateInjectDefaults::apply_to(
                &mutation_defaults,
                &mut inject,
                &m.exclude_inject_defaults,
                &m.name,
            )
            .ok()
            .filter(|added| !added.is_empty())
            .map(|added| (m.name.clone(), added))
        })
        .collect();

    info!("Converting to compiled format...");
    let CompiledArtifact {
        mut schema,
        functions,
    } = SchemaConverter::convert_artifact(intermediate, &ConvertOptions { auto_error_union })
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

    // 5b-pre. Infer native_columns for ID/UUID-typed arguments on JSONB-backed queries.
    // DB introspection (step 5b) overrides these inferred values when `--database` is provided.
    infer_native_columns_from_arg_types(&mut schema);

    // 5b. Optional: Validate native columns against database.
    if let Some(db_url) = opts.database {
        info!("Validating native columns for direct query arguments...");
        // One pool, one connection decision, for every check below (#1429).
        let pool = crate::connection::postgres_pool(db_url, "database validation", &database_tls)
            .await
            .context("Failed to connect for database validation")?;
        let pg_introspector = fraiseql_core::db::postgres::PostgresIntrospector::new(pool.clone());
        let db_report = validate_schema_against_database(&schema, &pg_introspector).await?;

        // Error-severity drift fails the compile (#384) — collected here, raised
        // after every check has reported, so one run surfaces the full list.
        let mut drift_errors: Vec<String> = Vec::new();
        for w in &db_report.warnings {
            match w.severity() {
                Severity::Error => {
                    warn!("{w} [drift error]");
                    drift_errors.push(w.to_string());
                },
                Severity::Warn => warn!("{w}"),
            }
        }
        apply_database_report(&mut schema, &db_report);

        // Mutation call/response contract (#384 item 3: inject_params resolve to real
        // function arguments). Every URL that reached the drift check is PostgreSQL —
        // a removed engine's was refused up front — so this runs for every form of it,
        // libpq `key=value` included (#1403).
        info!("Validating mutation contract against the database...");
        let catalog = PgCatalog::from_pool(pool);
        let contract = validate_mutation_contract(&schema, &catalog).await?;
        let mut caused_by_default: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();
        for m in &contract.mutations {
            let added = default_args.get(&m.mutation).map_or(&[][..], Vec::as_slice);
            for v in &m.violations {
                let kind = match v.severity() {
                    Severity::Error => "contract error",
                    Severity::Warn => "contract warning",
                };
                warn!("mutation `{}` (sql_source: {}): {v} [{kind}]", m.mutation, m.sql_source);
                if v.severity() != Severity::Error {
                    continue;
                }
                if let Some(key) = default_caused(v, added) {
                    caused_by_default.entry(key).or_default().push(m.mutation.clone());
                } else {
                    drift_errors.push(format!(
                        "mutation `{}` (sql_source: {}): {v}",
                        m.mutation, m.sql_source
                    ));
                }
            }
        }
        for (key, mutations) in caused_by_default {
            drift_errors.push(default_collision(&key, &mutations));
        }

        // The linter must be able to FAIL (#384): a schema that names database
        // objects which do not exist — or cannot serve the declared shape —
        // does not compile. `--allow-drift` restores the advisory behaviour;
        // the findings above have already been reported either way.
        if !drift_errors.is_empty() {
            if opts.allow_drift {
                warn!(
                    "{} schema↔database drift error(s) suppressed by --allow-drift",
                    drift_errors.len()
                );
            } else {
                let list =
                    drift_errors.iter().map(|e| format!("  - {e}")).collect::<Vec<_>>().join("\n");
                anyhow::bail!(
                    "schema↔database drift: {} error(s):\n{list}\n\
                     Fix the declarations or the database objects, or pass --allow-drift \
                     to compile anyway (advisory mode).",
                    drift_errors.len()
                );
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

            // #1303: the same trade, for the page ordering. The offline answer is
            // correct — `data->>'id'` orders every type under ADR-0017 — and it is
            // the most expensive of the three, which is exactly the shape of every
            // other `--database` sharpening in this pipeline.
            if query.pagination_order == Some(fraiseql_core::schema::PaginationOrder::JsonIdentity)
            {
                warn!(
                    "query `{}`: offset pages will be ordered by `data->>'id'` — no --database \
                     URL provided, so a cheaper unique column on `{}` could not be found. \
                     Provide --database to use `pk_{}` or a native `id` column instead, or \
                     declare pagination_order.",
                    query.name,
                    query.sql_source.as_deref().unwrap_or("?"),
                    fraiseql_core::utils::to_snake_case(&query.return_type),
                );
            }
        }
    }

    // 5d. Warn when mutations have wide invalidation fan-out (HOT update pressure).
    warn_wide_cascade_mutations(&schema);

    // 5e. Warn when a `preserve` schema would silently forward camelCase input keys
    // to snake_case SQL functions on the single-JSONB mutation path (#456).
    warn_jsonb_preserve_mismatch(&schema);

    // 5f. Refuse a schema in which a mutation could invalidate nothing (#910).
    refuse_unattributable_mutations(&schema)?;

    // 5g. Refuse authorization declarations no enforcer reads (#983).
    refuse_unenforced_authz_declarations(&schema)?;

    // 5h. Refuse a schema a server would refuse to load (ruling AF 4).
    refuse_what_a_server_would_not_load(&schema)?;

    Ok((CompiledArtifact { schema, functions }, report))
}

/// Refuse a compiled schema that a server would refuse to load (ruling AF 4).
///
/// The load-time checks (`CompiledSchema::finish_load`: duplicate names, type roles and
/// injects, subscription policies and filters, `requires_scope` without a `security`
/// section, fact-table links, relationships) hold for every producer, so they live at load
/// — and until now they ran only when a server started, on an artifact the compiler had
/// already written. The schema is serialized as the writer serializes it, stamped with its
/// content hash, and loaded exactly as a server loads it; a refusal fails the compile with
/// the loader's own message.
///
/// # Errors
///
/// Returns the loader's refusal.
/// The `[inject_defaults]` key a contract error is due to, if it is due to one (#1385).
///
/// Attributed only when the arithmetic says so: the function takes exactly the arguments
/// the mutation would send without its default-supplied parameters, or the parameter the
/// call binds at a mismatching position is one a default supplied. Anything else is the
/// function's own error and is reported as such.
pub(crate) fn default_caused(
    violation: &crate::schema::mutation_contract::ContractViolation,
    added: &[String],
) -> Option<String> {
    use crate::schema::mutation_contract::ContractViolation;
    match violation {
        ContractViolation::ArityMismatch { expected, found }
            if !added.is_empty() && found.contains(&expected.saturating_sub(added.len())) =>
        {
            Some(added.join(", "))
        },
        ContractViolation::InjectNameMismatch { expected, .. } if added.contains(expected) => {
            Some(expected.clone())
        },
        _ => None,
    }
}

/// One error for every mutation a default does not fit, instead of one per mutation.
pub(crate) fn default_collision(key: &str, mutations: &[String]) -> String {
    const SHOWN: usize = 5;
    let shown = mutations.iter().take(SHOWN).map(|m| format!("`{m}`")).collect::<Vec<_>>();
    let more = mutations.len().saturating_sub(SHOWN);
    let list = if more == 0 {
        shown.join(", ")
    } else {
        format!("{}, and {more} more", shown.join(", "))
    };
    format!(
        "[inject_defaults] adds `{key}` to {} mutation(s) whose functions do not take it: \
         {list}. Exclude it on those mutations (`exclude_inject_defaults = [\"{key}\"]`), or \
         move it from the base [inject_defaults] table to [inject_defaults.queries].",
        mutations.len()
    )
}

fn refuse_what_a_server_would_not_load(schema: &CompiledSchema) -> Result<()> {
    let body = serde_json::to_string(schema).context("Failed to serialize compiled schema")?;
    let mut value: serde_json::Value = serde_json::from_str(&body)?;
    let hash = content_hash_of(&value);
    value
        .as_object_mut()
        .context("schema must serialise as JSON object")?
        .insert("_content_hash".to_string(), serde_json::Value::String(hash));
    CompiledSchema::from_json(&serde_json::to_string(&value)?, true).map_err(|e| {
        anyhow::anyhow!("the compiled schema would be refused when a server loads it: {e}")
    })?;
    Ok(())
}

/// Refuse `security.rules`, `security.field_auth` and `security.default_policy` (#983).
///
/// All three are carried by the compiled seam and read by **nothing**: field-level
/// RBAC runs on `role_definitions` + scopes, and `#677` lowered type gates onto
/// operations. An operator who writes `[[security.rules]]` in a hand-authored
/// `schema.json` gets a successful compile and zero enforcement — an authorization
/// rule that is silently not applied, which is the worst shape a security
/// declaration can take.
///
/// Refused rather than wired: an enforcer for them is `#626`'s design, and wiring
/// three keys ad hoc to make the compile stop lying would prejudge it. This is the
/// same disposition `#612` gave `security.policies`, which is why that one is
/// already unproducible.
///
/// # Errors
///
/// Returns an error naming each declaration present.
fn refuse_unenforced_authz_declarations(schema: &CompiledSchema) -> Result<()> {
    let Some(security) = schema.security.as_ref() else {
        return Ok(());
    };

    let mut declared: Vec<String> = Vec::new();
    if !security.rules.is_empty() {
        declared.push(format!("`security.rules` ({} rule(s))", security.rules.len()));
    }
    if !security.field_auth.is_empty() {
        declared.push(format!("`security.field_auth` ({} rule(s))", security.field_auth.len()));
    }
    if let Some(policy) = security.default_policy.as_deref() {
        declared.push(format!("`security.default_policy` (\"{policy}\")"));
    }
    if declared.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "authorization declaration(s) with no enforcer: {}.\n\n\
         FraiseQL enforces field-level authorization through `security.role_definitions` \
         (roles → scopes) and each field's `requires_scope`; nothing reads these keys, so a \
         rule written here is not applied to any request. Remove them, or express the same \
         intent with role definitions and `requires_scope`.\n\n\
         (A general authorization-rule engine is tracked separately; until it exists, a \
         declaration that silently enforces nothing is refused rather than accepted.)",
        declared.join(", ")
    )
}

/// Refuse a schema whose caching depends on a mutation the engine cannot attribute (#910).
///
/// A mutation's invalidation is resolved from `invalidates_views`, the return type's
/// view, the entity a payload type wraps, the `entity_type` its function stamps on
/// `mutation_response`, and its cascade envelope. The first three are knowable here;
/// the last two are not. When the knowable ones resolve to nothing, the mutation
/// *may* invalidate nothing at runtime — and for a view annotated
/// `cache_ttl_seconds = 0`, documented as "mutation-invalidated only", nothing means
/// **forever**: the write lands, every cached entry stays warm for the process
/// lifetime, and no log line says so.
///
/// The compiler knows every mutation and every `cache_ttl_seconds`, so it can make
/// the shape impossible instead of loud. A `tracing::warn!` beside a successful
/// compile is the same defect with more text; a boot-time refusal is later and
/// per-deployment. Declaring `invalidates_views` costs one line and is checkable.
///
/// Schemas that annotate no view as cacheable are unaffected — there is no entry
/// to strand.
///
/// # Errors
///
/// Returns an error naming every unattributable mutation when the schema also
/// declares at least one cacheable view.
fn refuse_unattributable_mutations(schema: &CompiledSchema) -> Result<()> {
    if !fraiseql_core::cache::declares_cacheable_views(schema) {
        return Ok(());
    }
    let unattributable = fraiseql_core::cache::unattributable_mutations(schema);
    if unattributable.is_empty() {
        return Ok(());
    }

    let cacheable: Vec<&str> = schema
        .queries
        .iter()
        .filter(|q| q.cache_ttl_seconds.is_some())
        .filter_map(|q| q.sql_source.as_deref())
        .collect();
    let list = unattributable.iter().map(|m| format!("  - {m}")).collect::<Vec<_>>().join("\n");
    anyhow::bail!(
        "mutation(s) whose cache invalidation cannot be resolved from the schema:\n{list}\n\n\
         This schema annotates {} cacheable view(s) ({}), and a mutation that resolves to no \
         view invalidates nothing when it succeeds — permanently, for any view annotated \
         `cache_ttl_seconds = 0`.\n\n\
         Fix by declaring what each mutation writes:\n\
         \x20   invalidates_views = [\"v_price\"]\n\n\
         A mutation whose return type is backed by a view, or whose payload wraps an entity \
         that is, already resolves and needs no annotation.",
        cacheable.len(),
        cacheable.join(", "),
    )
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
/// * `allow_drift` - Downgrade error-severity schema↔database drift to advisories (#384)
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
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
// Reason: run() is the CLI entry point that receives individual args from clap; keeping them
// separate for clarity — each bool is a distinct clap flag
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
    allow_drift: bool,
    config: Option<&str>,
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

    // Which config an artifact was compiled with is not answerable from the artifact,
    // so every compile says it, before the work that could fail (#1387).
    let config = ConfigSource::resolve(Path::new(input), config.map(Path::new))?;
    println!("Config: {config}");

    let opts = CompileOptions {
        input,
        types,
        schema_dir,
        type_files,
        query_files,
        mutation_files,
        database,
        skip_hash,
        allow_drift,
        config: Some(config),
    };
    let (artifact, optimization_report) = compile_to_schema(opts).await?;
    let schema = &artifact.schema;

    // If check-only mode, stop here
    if check {
        println!("✓ Schema is valid");
        println!("  Types: {}", schema.types.len());
        println!("  Queries: {}", schema.queries.len());
        println!("  Mutations: {}", schema.mutations.len());
        optimization_report.print();
        print!("{}", localized_index_report_text(schema));
        return Ok(());
    }

    // Write compiled schema
    info!("Writing compiled schema to: {output}");
    // One routine assembles the file — schema plus every sibling section — so the
    // `_content_hash` below covers all of it (#899/#1325).
    let value = artifact.to_json_value()?;

    let output_json = if skip_hash {
        serde_json::to_string_pretty(&value).context("Failed to serialize compiled schema")?
    } else {
        // One routine, shared with the `from_json` verifier, so writer and reader
        // cannot drift (#899).
        let hash_hex = content_hash_of(&value);

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
    print!("{}", localized_index_report_text(schema));

    // Emit DDL to directory if requested
    if let Some(ddl_dir) = emit_ddl {
        emit_ddl_to_dir(schema, ddl_dir)?;
    }

    // Check for migration drift if requested
    if check_migrations {
        run_check_migrations(schema)?;
    }

    Ok(())
}

/// The expression indexes a filter or sort on a localized field reads (#1513), as printed.
///
/// One per (field, allowed locale), each on exactly the key the query builds. Empty when the
/// schema has no localized field.
///
/// A view gets the advice and no DDL: an index belongs on its base table, which the schema
/// does not name.
#[must_use]
pub fn localized_index_report_text(schema: &fraiseql_core::schema::CompiledSchema) -> String {
    use std::fmt::Write as _;
    let report = schema.localized_index_report();
    if report.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "\nLocalized field indexes (one per field and allowed locale; a filter or sort on the \
         field reads it):\n",
    );
    for advice in &report {
        let subject = format!(
            "{}.{} [{}] on {}",
            advice.type_name, advice.field, advice.locale, advice.table
        );
        // fmt::Write for String is infallible.
        let _ = match &advice.index {
            Some(index) => writeln!(text, "  {subject}:\n    {}", index.ddl),
            None => writeln!(
                text,
                "  {subject}: a view; create the index on its base table, on the expression the \
                 query reads"
            ),
        };
    }
    text
}

/// The `--emit-ddl` artifact's format version (#965), the first line of every file it
/// writes. Raised when a consumer reading the old format would misread the new one.
pub const EMIT_DDL_FORMAT: u32 = 1;

/// Emit `CREATE TABLE` DDL files for all compiled schema types to `output_dir`.
///
/// Each type produces one `<type_snake_case>.sql` file containing a `CREATE TABLE IF NOT EXISTS`
/// statement. The output directory is created if it does not already exist.
///
/// The directory is a contract (#965; `docs/operations/emit-ddl.md`): Confiture's `migrate diff
/// --to <dir>` reads it. Every file opens with the format version ([`EMIT_DDL_FORMAT`]) and the
/// compiler that wrote it, and the bytes are a function of the schema alone: the same schema
/// gives the same files whatever order its types arrived in.
///
/// # Errors
///
/// Returns an error if two types would write one file (their table names coincide), if the
/// output directory cannot be created, or if any DDL file cannot be written. A refusal writes
/// nothing.
pub fn emit_ddl_to_dir(schema: &CompiledSchema, output_dir: &str) -> Result<()> {
    // Every file built before any is written, so a refusal leaves the directory as it was.
    let mut files: std::collections::BTreeMap<String, (&str, String)> =
        std::collections::BTreeMap::new();
    for type_def in &schema.types {
        let table_name = to_snake_case(type_def.name.as_str());
        let ddl = build_create_table_ddl(&table_name, type_def)?;
        if let Some((other, _)) = files.get(&table_name) {
            let (first, second) = if *other < type_def.name.as_str() {
                (*other, type_def.name.as_str())
            } else {
                (type_def.name.as_str(), *other)
            };
            anyhow::bail!(
                "--emit-ddl: types '{first}' and '{second}' both map to the table \
                 `tb_{table_name}` and the file `{table_name}.sql`; rename one of them"
            );
        }
        files.insert(table_name, (type_def.name.as_str(), ddl));
    }

    fs::create_dir_all(output_dir)
        .context(format!("Failed to create DDL output directory: {output_dir}"))?;
    for (table_name, (type_name, ddl)) in &files {
        let file_path = Path::new(output_dir).join(format!("{table_name}.sql"));
        fs::write(&file_path, ddl)
            .context(format!("Failed to write DDL for type '{type_name}'"))?;
    }

    println!("✓ DDL emitted to {output_dir}/ ({} table(s))", files.len());
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
///
/// Delegates to the pipeline's one namer rather than reimplementing it. This used to
/// insert a separator before every uppercase character, so `HTTPServer` became
/// `h_t_t_p_server` in emitted DDL while JSONB key derivation — which already used
/// `fraiseql_core::utils::casing` — produced `http_server`. Two casing systems that
/// disagree about the same name produce DDL for a table the runtime never looks in.
pub(crate) fn to_snake_case(name: &str) -> String {
    fraiseql_core::utils::to_snake_case(name)
}

/// Generate a `CREATE TABLE IF NOT EXISTS` DDL statement for a compiled type definition.
///
/// # Errors
///
/// Returns an error when a vector field declares an index the metric has no
/// pgvector operator class for — DDL `CREATE INDEX` would refuse (#959).
fn build_create_table_ddl(
    table_name: &str,
    type_def: &fraiseql_core::schema::TypeDefinition,
) -> Result<String> {
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("-- fraiseql emit-ddl format {EMIT_DDL_FORMAT}"));
    lines.push(format!("-- Generated by fraiseql-cli {}", env!("CARGO_PKG_VERSION")));
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
            // #386: a dimensioned `vector(N)` column — a bare `vector` column
            // cannot carry an HNSW/IVFFlat index, so a dimension-less emission
            // silently produced unindexable DDL. #959: the same for `bit(N)`,
            // where a missing width is worse still — bare `bit` is `bit(1)`.
            let pg_type =
                if field.field_type.is_searchable_vector() && field.vector_config.is_some() {
                    field.field_type.to_sql_type(field.vector_config.as_ref())
                } else {
                    field_type_to_pg(&field.field_type)
                };
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

    // #386: vector columns need the extension and their declared ANN index.
    // Binary vectors need it too — `bit(N)` is a PostgreSQL type, but `<~>`,
    // `<%>` and the `bit_*_ops` operator classes come from pgvector (#959).
    let vector_fields: Vec<_> = type_def
        .fields
        .iter()
        .filter_map(|f| f.vector_config.as_ref().map(|c| (f, c)))
        .collect();
    if !vector_fields.is_empty() {
        lines.push(String::new());
        lines.push("CREATE EXTENSION IF NOT EXISTS vector;".to_string());
        for (field, config) in vector_fields {
            let col_name = to_snake_case(field.name.as_str());
            let index_ddl = config
                .index_type
                .index_sql(
                    &format!("tb_{table_name}"),
                    &col_name,
                    &field.field_type,
                    config.distance_metric,
                )
                .with_context(|| format!("field '{}.{}'", type_def.name, field.name.as_str()))?;
            if let Some(index_ddl) = index_ddl {
                lines.push(format!("{index_ddl};"));
            }
        }
    }

    lines.push(String::new());
    Ok(lines.join("\n"))
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
        // Width-less on purpose: a `BitVector` field always carries a
        // `vector_config`, so the dimensioned `bit(N)` form is what the DDL
        // emitter uses; this branch is the shape a config-less one would take,
        // and `varbit` is the only bit type that does not silently truncate.
        FieldType::BitVector => "VARBIT".to_string(),
        // Width-less for the same reason as `BitVector`: a vector field always
        // carries a `vector_config`, so the dimensioned form is what the DDL
        // emitter uses.
        FieldType::HalfVector => "HALFVEC".to_string(),
        FieldType::SparseVector => "SPARSEVEC".to_string(),
        // Use the actual Postgres enum type name so DDL matches the schema.
        FieldType::Enum(name) => name.clone(),
        FieldType::Input(_) | FieldType::Interface(_) | FieldType::Union(_) => "JSONB".to_string(),
        // FieldType is #[non_exhaustive]; future variants default to TEXT.
        _ => "TEXT".to_string(),
    }
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
/// # Errors
///
/// Returns error if the connection URL is invalid, the server cannot be reached, or
/// it is older than PostgreSQL 18.
pub(crate) async fn build_postgres_introspector(
    db_url: &str,
    tls: &PostgresTlsConfig,
) -> Result<fraiseql_core::db::postgres::PostgresIntrospector> {
    let pool = crate::connection::postgres_pool(db_url, "database validation", tls).await?;
    Ok(fraiseql_core::db::postgres::PostgresIntrospector::new(pool))
}

/// Auto-param names excluded from `native_columns` inference and JSONB-extraction warnings.
const NATIVE_COLUMN_SKIP_ARGS: &[&str] = &[
    "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
];

/// Fold what introspection discovered back into the compiled schema.
///
/// Both halves are *sharpenings*: the schema is already correct without a
/// database, and a `--database` compile makes it cheaper. Split out of the
/// pipeline so the fold is testable without one — the patch loop it replaced was
/// reachable only from a live connection, which is the shape a discovery that
/// silently stops being applied hides in.
///
/// An authored `pagination_order` is protected upstream, in the validator, which
/// only reports a sharpening for a query whose ordering the compiler derived. It
/// is stated here too because this is where the overwrite happens.
pub(crate) fn apply_database_report(
    schema: &mut fraiseql_core::schema::CompiledSchema,
    report: &crate::schema::database_validator::DatabaseValidationReport,
) {
    for query in &mut schema.queries {
        if let Some(cols) = report.native_columns.get(&query.name) {
            query.native_columns = cols.clone();
        }
        if let Some(order) = report.pagination_orders.get(&query.name) {
            query.pagination_order = Some(order.clone());
        }
    }
}

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
