//! Compile-time database validation for schema definitions.
//!
//! Validates a compiled schema against a live database at three levels:
//! - **L1**: `sql_source` relation exists in the database
//! - **L2**: Columns and JSON column types match
//! - **L3**: JSONB keys exist in sampled rows (best-effort)
//!
//! All diagnostics are warnings — compilation never fails due to validation.

use std::{
    collections::{HashMap, HashSet},
    fmt,
};

use fraiseql_core::{
    db::{
        DatabaseType,
        introspector::{DatabaseIntrospector, RelationInfo},
    },
    schema::{CompiledSchema, FieldType},
};

/// Report containing all database validation warnings and discovered metadata.
pub struct DatabaseValidationReport {
    /// All warnings emitted during validation.
    pub warnings:       Vec<DatabaseWarning>,
    /// Native columns discovered per query during L2 validation.
    ///
    /// Key: query name. Value: map of argument name → PostgreSQL type string
    /// (e.g. `"uuid"`, `"integer"`, `"text"`).
    ///
    /// Only contains entries for queries that have at least one direct argument
    /// with a matching native column on their `sql_source`.
    pub native_columns: HashMap<String, HashMap<String, String>>,
}

/// A single database validation warning.
#[derive(Debug)]
pub enum DatabaseWarning {
    /// L1: `sql_source` relation does not exist.
    MissingRelation {
        /// Name of the query or mutation.
        query_name: String,
        /// The `sql_source` value that was not found.
        sql_source: String,
    },
    /// L1: a mutation's `sql_source` **function** does not exist.
    ///
    /// Distinct from [`MissingRelation`](Self::MissingRelation): a mutation's
    /// `sql_source` names a SQL *function* (the runtime calls
    /// `SELECT * FROM fn(...)`), not a view/table, so it is probed via `pg_proc`
    /// rather than the relation catalog.
    MissingFunction {
        /// Name of the mutation.
        mutation_name: String,
        /// The `sql_source` function that was not found.
        sql_source:    String,
    },
    /// L1: `additional_view` does not exist.
    MissingAdditionalView {
        /// Name of the query.
        query_name: String,
        /// The view name that was not found.
        view_name:  String,
    },
    /// L2: `jsonb_column` does not exist on the relation.
    MissingJsonColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The missing column name.
        column_name: String,
    },
    /// L2: `jsonb_column` exists but is not a JSON/JSONB type.
    WrongJsonColumnType {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The column name.
        column_name: String,
        /// The actual SQL data type.
        actual_type: String,
    },
    /// L2: `relay_cursor_column` does not exist on the relation.
    MissingCursorColumn {
        /// Name of the query.
        query_name:  String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The missing cursor column name.
        column_name: String,
    },
    /// L3: a JSON key path is declared but not found in sampled data.
    MissingJsonKey {
        /// Name of the query.
        query_name:  String,
        /// The GraphQL type that declares the field (where to fix it).
        type_name:   String,
        /// The `sql_source` relation.
        sql_source:  String,
        /// The JSON column being sampled.
        json_column: String,
        /// The GraphQL field name.
        field_name:  String,
        /// The snake_case key looked up in the JSON.
        json_key:    String,
    },
    /// L2: a direct query argument has no matching native column — will fall back to JSONB
    /// extraction.
    ///
    /// For best performance, consider adding a native column with the same name
    /// and an index on the `sql_source` table/view.
    NativeColumnFallback {
        /// Name of the query.
        query_name: String,
        /// The `sql_source` relation.
        sql_source: String,
        /// The argument name that has no matching native column.
        arg_name:   String,
    },
    /// L2: a direct query argument resolves to a native column whose SQL type cannot
    /// cleanly drive the predicate for the argument's GraphQL scalar type (e.g. an
    /// `Int` argument filtering a `uuid` column — `WHERE col = $N` errors or never
    /// matches).
    TypeConvertibility {
        /// Name of the query.
        query_name:   String,
        /// The `sql_source` relation.
        sql_source:   String,
        /// The argument name.
        arg_name:     String,
        /// The argument's declared GraphQL type (e.g. `Int`).
        graphql_type: String,
        /// The native column's SQL type (e.g. `uuid`).
        column_type:  String,
    },
}

impl fmt::Display for DatabaseWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRelation {
                query_name,
                sql_source,
            } => {
                write!(
                    f,
                    "query `{query_name}`: sql_source `{sql_source}` does not exist in database"
                )
            },
            Self::MissingFunction {
                mutation_name,
                sql_source,
            } => {
                write!(
                    f,
                    "mutation `{mutation_name}`: sql_source function `{sql_source}` does not exist in database"
                )
            },
            Self::MissingAdditionalView {
                query_name,
                view_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: additional_view `{view_name}` does not exist in database"
                )
            },
            Self::MissingJsonColumn {
                query_name,
                sql_source,
                column_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: column `{column_name}` not found on `{sql_source}`"
                )
            },
            Self::WrongJsonColumnType {
                query_name,
                sql_source,
                column_name,
                actual_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: column `{column_name}` on `{sql_source}` is `{actual_type}`, expected json/jsonb"
                )
            },
            Self::MissingCursorColumn {
                query_name,
                sql_source,
                column_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: relay cursor column `{column_name}` not found on `{sql_source}`"
                )
            },
            Self::MissingJsonKey {
                query_name,
                type_name,
                sql_source,
                json_column,
                field_name,
                json_key,
            } => {
                write!(
                    f,
                    "query `{query_name}`: field `{type_name}.{field_name}` (key `{json_key}`) not found in `{sql_source}.{json_column}` sample data"
                )
            },
            Self::NativeColumnFallback {
                query_name,
                sql_source,
                arg_name,
            } => {
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` will use JSONB extraction \
                     (`{sql_source}.data->>''{arg_name}''`) — no native column `{arg_name}` found on \
                     `{sql_source}`. Add a native column with an index for O(log n) lookup."
                )
            },
            Self::TypeConvertibility {
                query_name,
                sql_source,
                arg_name,
                graphql_type,
                column_type,
            } => {
                write!(
                    f,
                    "query `{query_name}`: argument `{arg_name}` is `{graphql_type}` but the native column \
                     `{sql_source}.{arg_name}` is `{column_type}` — the predicate may error or never match. \
                     Align the argument type with the column, or store the field in `data` for JSONB extraction."
                )
            },
        }
    }
}

/// Check if a SQL data type represents a JSON column for the given database.
pub(crate) fn is_json_type(data_type: &str, db_type: DatabaseType) -> bool {
    let lower = data_type.to_lowercase();
    match db_type {
        DatabaseType::PostgreSQL => lower == "jsonb" || lower == "json",
        DatabaseType::MySQL => lower == "json",
        DatabaseType::SQLite => lower.contains("json"),
        // SQL Server has no native JSON type — always attempt JSON
        // validation for the configured jsonb_column
        DatabaseType::SQLServer => true,
    }
}

/// Coarse SQL type families, used to flag argument↔column type mismatches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SqlFamily {
    /// Integer/decimal/float numeric types.
    Numeric,
    /// Boolean.
    Boolean,
    /// UUID / `uniqueidentifier`.
    Uuid,
    /// Character/string types.
    Text,
    /// Date/time/interval types.
    Temporal,
    /// `json` / `jsonb`.
    Json,
    /// Anything else (custom domains, enums, arrays, geometry, …) — never flagged.
    Other,
}

/// Classify a (any-dialect) SQL type string into a coarse [`SqlFamily`].
///
/// Strips length/precision and multi-word suffixes (`character varying`,
/// `timestamp with time zone`, `double precision`) down to the leading base word.
fn sql_type_family(sql_type: &str) -> SqlFamily {
    let lower = sql_type.trim().to_lowercase();
    let base = lower.split(['(', ' ', '[']).next().unwrap_or(lower.as_str());
    match base {
        "smallint" | "integer" | "int" | "int2" | "int4" | "int8" | "bigint" | "serial"
        | "bigserial" | "smallserial" | "numeric" | "decimal" | "real" | "double" | "float"
        | "float4" | "float8" | "money" | "mediumint" | "tinyint" => SqlFamily::Numeric,
        "boolean" | "bool" | "bit" => SqlFamily::Boolean,
        "uuid" | "uniqueidentifier" => SqlFamily::Uuid,
        "text" | "varchar" | "char" | "bpchar" | "name" | "citext" | "nvarchar" | "nchar"
        | "character" | "clob" | "string" => SqlFamily::Text,
        "date" | "time" | "timestamp" | "timestamptz" | "timetz" | "datetime" | "datetime2"
        | "interval" | "smalldatetime" => SqlFamily::Temporal,
        "json" | "jsonb" => SqlFamily::Json,
        _ => SqlFamily::Other,
    }
}

/// Whether a query argument's GraphQL type can cleanly drive an equality predicate
/// against a native column of `sql_type`.
///
/// Conservative by design: only the strongly-typed scalars (`Int`, `Float`,
/// `Decimal`, `Boolean`, `UUID`) can flag a mismatch, and only against a *known*
/// incompatible column family. Permissive scalars (`String`, `ID`, date/time, `JSON`,
/// custom scalars) and non-scalar references never warn — the runtime binds them as
/// text-coercible parameters, and `ID` intentionally spans uuid / integer / text keys.
fn arg_type_convertible(field_type: &FieldType, sql_type: &str) -> bool {
    // A list filter (`[ID!]`) drives the predicate with its element type.
    let base = match field_type {
        FieldType::List(inner) => inner.as_ref(),
        other => other,
    };
    let family = sql_type_family(sql_type);
    // Unknown column family → don't second-guess it.
    if family == SqlFamily::Other {
        return true;
    }
    match base {
        FieldType::Int | FieldType::Float | FieldType::Decimal => family == SqlFamily::Numeric,
        FieldType::Boolean => family == SqlFamily::Boolean,
        FieldType::Uuid => matches!(family, SqlFamily::Uuid | SqlFamily::Text),
        _ => true,
    }
}

/// Split a potentially schema-qualified name into (optional_schema, name).
fn split_schema_qualified(sql_source: &str) -> (Option<&str>, &str) {
    match sql_source.split_once('.') {
        Some((schema, table)) => (Some(schema), table),
        None => (None, sql_source),
    }
}

/// Check if a relation exists in the relation lookup maps.
fn relation_exists(
    schema_qualified: &HashMap<(String, String), RelationInfo>,
    unqualified: &HashMap<String, Vec<String>>,
    sql_source: &str,
) -> bool {
    let (schema, name) = split_schema_qualified(sql_source);
    if let Some(s) = schema {
        schema_qualified.contains_key(&(s.to_string(), name.to_string()))
    } else {
        unqualified.contains_key(name)
    }
}

/// L1 relation existence with runtime-faithful resolution.
///
/// For a **schema-qualified** source, probes via `to_regclass` on the
/// case-sensitively-quoted identifier (search_path-independent — exactly how the
/// runtime resolves it), so a mixed-case relation in an off-`search_path` schema
/// is not falsely flagged. Falls back to the relation map for **bare** names
/// (search_path-scoped, also matching the runtime) or when the connector cannot
/// probe directly (non-Postgres).
async fn relation_source_exists(
    introspector: &impl DatabaseIntrospector,
    schema_qualified: &HashMap<(String, String), RelationInfo>,
    unqualified: &HashMap<String, Vec<String>>,
    sql_source: &str,
) -> fraiseql_core::Result<bool> {
    let (schema, name) = split_schema_qualified(sql_source);
    if let Some(s) = schema {
        if let Some(exists) = introspector.qualified_relation_exists(s, name).await? {
            return Ok(exists);
        }
    }
    // Bare name, or a connector that can't probe → relation-map lookup.
    Ok(relation_exists(schema_qualified, unqualified, sql_source))
}

/// L1 function existence for a mutation's `sql_source`.
///
/// A mutation's `sql_source` names a SQL *function*, so it is probed via `pg_proc`
/// (not the relation catalog). When the connector cannot probe functions
/// (non-Postgres), returns `true` — the check is skipped rather than false-failing.
async fn function_source_exists(
    introspector: &impl DatabaseIntrospector,
    sql_source: &str,
) -> fraiseql_core::Result<bool> {
    let (schema, name) = split_schema_qualified(sql_source);
    Ok(introspector.function_exists(schema, name).await?.unwrap_or(true))
}

/// Validate a compiled schema against a live database.
///
/// Performs three levels of validation:
/// - **L1**: Checks that `sql_source` relations exist
/// - **L2**: Checks column existence and JSON column types
/// - **L3**: Checks JSONB key existence via sampling
///
/// All diagnostics are warnings — the report never causes compilation to fail.
///
/// # Errors
///
/// Returns `FraiseQLError` if database introspection queries fail.
pub async fn validate_schema_against_database(
    schema: &CompiledSchema,
    introspector: &impl DatabaseIntrospector,
) -> fraiseql_core::Result<DatabaseValidationReport> {
    // Auto-wired argument names excluded from direct-arg native column detection.
    // Must stay in sync with AUTO_PARAM_NAMES in fraiseql-core/runtime/executor/query.rs.
    const AUTO_PARAM_NAMES: &[&str] = &[
        "where", "limit", "offset", "orderBy", "first", "last", "after", "before",
    ];

    let mut warnings = Vec::new();
    let mut native_columns: HashMap<String, HashMap<String, String>> = HashMap::new();
    let db_type = introspector.database_type();

    // L1: Build relation lookup maps
    let relations = introspector.list_relations().await?;
    let (schema_qualified, unqualified) = build_relation_maps(&relations);

    // Validate queries
    for query in &schema.queries {
        if let Some(ref source) = query.sql_source {
            // L1: Check relation exists (qualified → to_regclass verbatim, bare → map).
            if !relation_source_exists(introspector, &schema_qualified, &unqualified, source)
                .await?
            {
                warnings.push(DatabaseWarning::MissingRelation {
                    query_name: query.name.clone(),
                    sql_source: source.clone(),
                });
                continue; // Skip L2/L3 if relation doesn't exist
            }

            // L2: Get columns for the relation.
            // Pass the full source (possibly schema-qualified like "benchmark.tv_post") so
            // the introspector can use the explicit schema when present.
            let columns = introspector.get_columns(source).await?;
            let column_map: HashMap<String, String> =
                columns.into_iter().map(|(name, dtype, _)| (name, dtype)).collect();

            // L2: Check jsonb_column
            let jsonb_col = &query.jsonb_column;
            if !jsonb_col.is_empty() {
                if let Some(actual_type) = column_map.get(jsonb_col) {
                    if !is_json_type(actual_type, db_type) {
                        warnings.push(DatabaseWarning::WrongJsonColumnType {
                            query_name:  query.name.clone(),
                            sql_source:  source.clone(),
                            column_name: jsonb_col.clone(),
                            actual_type: actual_type.clone(),
                        });
                    }
                } else {
                    warnings.push(DatabaseWarning::MissingJsonColumn {
                        query_name:  query.name.clone(),
                        sql_source:  source.clone(),
                        column_name: jsonb_col.clone(),
                    });
                }
            }

            // L2: Check relay_cursor_column
            if query.relay {
                if let Some(ref cursor_col) = query.relay_cursor_column {
                    if !column_map.contains_key(cursor_col) {
                        warnings.push(DatabaseWarning::MissingCursorColumn {
                            query_name:  query.name.clone(),
                            sql_source:  source.clone(),
                            column_name: cursor_col.clone(),
                        });
                    }
                }
            }

            // L3: Sample JSON keys if jsonb_column is valid JSON type
            if !jsonb_col.is_empty() {
                let json_type_ok =
                    column_map.get(jsonb_col).is_some_and(|t| is_json_type(t, db_type));

                if json_type_ok {
                    validate_json_keys(
                        schema,
                        query,
                        source,
                        jsonb_col,
                        introspector,
                        source, // pass full schema-qualified source for sample queries
                        &mut warnings,
                    )
                    .await?;
                }
            }

            // L2: Detect native columns for direct (non-auto-param) arguments AND
            // inject params. Both are filtered against the view at runtime, so a name
            // that matches a real column must use the native-column path
            // (`WHERE col = $N`) rather than the JSONB fallback (`data->>'name'`).
            let direct_args: Vec<&str> = query
                .arguments
                .iter()
                .filter(|a| !AUTO_PARAM_NAMES.contains(&a.name.as_str()))
                .map(|a| a.name.as_str())
                .collect();

            let (query_native, arg_fallbacks) = detect_query_native_columns(
                &direct_args,
                query.inject_params.keys().map(String::as_str),
                &column_map,
            );
            for arg_name in arg_fallbacks {
                warnings.push(DatabaseWarning::NativeColumnFallback {
                    query_name: query.name.clone(),
                    sql_source: source.clone(),
                    arg_name,
                });
            }

            // L2 (type-convertibility): a direct argument that resolved to a native
            // column whose SQL type cannot cleanly drive the predicate is a likely
            // authoring bug (e.g. an `Int` argument filtering a `uuid` column).
            for arg in &query.arguments {
                if let Some(col_type) = query_native.get(&arg.name) {
                    if !arg_type_convertible(&arg.arg_type, col_type) {
                        warnings.push(DatabaseWarning::TypeConvertibility {
                            query_name:   query.name.clone(),
                            sql_source:   source.clone(),
                            arg_name:     arg.name.clone(),
                            graphql_type: arg.arg_type.to_graphql_string(),
                            column_type:  col_type.clone(),
                        });
                    }
                }
            }

            if !query_native.is_empty() {
                native_columns.insert(query.name.clone(), query_native);
            }

            // L1: Check additional_views (relations, same resolution as sql_source).
            for view in &query.additional_views {
                if !relation_source_exists(introspector, &schema_qualified, &unqualified, view)
                    .await?
                {
                    warnings.push(DatabaseWarning::MissingAdditionalView {
                        query_name: query.name.clone(),
                        view_name:  view.clone(),
                    });
                }
            }
        }
    }

    // Validate mutations (L1 only). A mutation's `sql_source` is a FUNCTION, not a
    // relation — probe `pg_proc`, not the relation catalog (#485). Mirrors the
    // shared `fraiseql_core::schema::sql_source_probes` Relation/Function split.
    for mutation in &schema.mutations {
        if let Some(ref source) = mutation.sql_source {
            if !function_source_exists(introspector, source).await? {
                warnings.push(DatabaseWarning::MissingFunction {
                    mutation_name: mutation.name.clone(),
                    sql_source:    source.clone(),
                });
            }
        }
    }

    Ok(DatabaseValidationReport {
        warnings,
        native_columns,
    })
}

/// Build lookup maps from the list of relations.
fn build_relation_maps(
    relations: &[RelationInfo],
) -> (HashMap<(String, String), RelationInfo>, HashMap<String, Vec<String>>) {
    let mut schema_qualified = HashMap::new();
    let mut unqualified: HashMap<String, Vec<String>> = HashMap::new();

    for rel in relations {
        schema_qualified.insert((rel.schema.clone(), rel.name.clone()), rel.clone());
        unqualified.entry(rel.name.clone()).or_default().push(rel.schema.clone());
    }

    (schema_qualified, unqualified)
}

/// Validate JSON keys in sampled data for L3 checking.
async fn validate_json_keys(
    schema: &CompiledSchema,
    query: &fraiseql_core::schema::QueryDefinition,
    source: &str,
    jsonb_col: &str,
    introspector: &impl DatabaseIntrospector,
    table_name: &str,
    warnings: &mut Vec<DatabaseWarning>,
) -> fraiseql_core::Result<()> {
    let samples = introspector.get_sample_json_rows(table_name, jsonb_col, 5).await?;

    if samples.is_empty() {
        return Ok(());
    }

    // Merge all top-level keys from sampled rows
    let mut all_keys = HashSet::new();
    for sample in &samples {
        if let serde_json::Value::Object(map) = sample {
            for key in map.keys() {
                all_keys.insert(key.clone());
            }
        }
    }

    if all_keys.is_empty() {
        return Ok(());
    }

    // Find the type definition for this query's return type
    let type_def = schema.types.iter().find(|t| t.name.as_str() == query.return_type);

    if let Some(type_def) = type_def {
        for field in &type_def.fields {
            let field_str = field.name.as_str();
            // Use the canonical acronym/digit-aware caser the runtime queries with
            // (`httpResponse` → `http_response`, not `h_t_t_p_response`), so the
            // L3 JSON-key check never cries wolf on acronym/digit fields (#485).
            let json_key = fraiseql_core::utils::to_snake_case(field_str);
            // Skip fields that are top-level columns (not from JSONB)
            // Convention: fields like "id", "pk_*", "fk_*" are columns, not JSON keys
            if field_str == "id" || field_str.starts_with("pk_") || field_str.starts_with("fk_") {
                continue;
            }
            if !all_keys.contains(&json_key) && !all_keys.contains(field_str) {
                warnings.push(DatabaseWarning::MissingJsonKey {
                    query_name: query.name.clone(),
                    type_name: type_def.name.as_str().to_string(),
                    sql_source: source.to_string(),
                    json_column: jsonb_col.to_string(),
                    field_name: field_str.to_string(),
                    json_key,
                });
            }
        }
    }

    Ok(())
}

/// Enum dispatch for database introspectors.
///
/// Uses enum dispatch instead of `Box<dyn DatabaseIntrospector>` because the
/// trait uses `async_fn_in_trait` and cannot be object-safe.
pub enum AnyIntrospector {
    /// PostgreSQL introspector.
    Postgres(fraiseql_core::db::PostgresIntrospector),
    #[cfg(feature = "mysql")]
    /// MySQL introspector.
    MySql(fraiseql_core::db::MySqlIntrospector),
    #[cfg(feature = "sqlite")]
    /// SQLite introspector.
    Sqlite(fraiseql_core::db::SqliteIntrospector),
    #[cfg(feature = "sqlserver")]
    /// SQL Server introspector.
    SqlServer(fraiseql_core::db::SqlServerIntrospector),
}

impl DatabaseIntrospector for AnyIntrospector {
    async fn list_fact_tables(&self) -> fraiseql_core::Result<Vec<String>> {
        match self {
            Self::Postgres(i) => i.list_fact_tables().await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.list_fact_tables().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.list_fact_tables().await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.list_fact_tables().await,
        }
    }

    async fn get_columns(
        &self,
        table_name: &str,
    ) -> fraiseql_core::Result<Vec<(String, String, bool)>> {
        match self {
            Self::Postgres(i) => i.get_columns(table_name).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.get_columns(table_name).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.get_columns(table_name).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.get_columns(table_name).await,
        }
    }

    async fn get_indexed_columns(&self, table_name: &str) -> fraiseql_core::Result<Vec<String>> {
        match self {
            Self::Postgres(i) => i.get_indexed_columns(table_name).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.get_indexed_columns(table_name).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.get_indexed_columns(table_name).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.get_indexed_columns(table_name).await,
        }
    }

    fn database_type(&self) -> DatabaseType {
        match self {
            Self::Postgres(i) => i.database_type(),
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.database_type(),
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.database_type(),
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.database_type(),
        }
    }

    async fn get_sample_jsonb(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> fraiseql_core::Result<Option<serde_json::Value>> {
        match self {
            Self::Postgres(i) => i.get_sample_jsonb(table_name, column_name).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.get_sample_jsonb(table_name, column_name).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.get_sample_jsonb(table_name, column_name).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.get_sample_jsonb(table_name, column_name).await,
        }
    }

    async fn list_relations(&self) -> fraiseql_core::Result<Vec<fraiseql_core::db::RelationInfo>> {
        match self {
            Self::Postgres(i) => i.list_relations().await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.list_relations().await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.list_relations().await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.list_relations().await,
        }
    }

    async fn get_sample_json_rows(
        &self,
        table_name: &str,
        column_name: &str,
        limit: usize,
    ) -> fraiseql_core::Result<Vec<serde_json::Value>> {
        match self {
            Self::Postgres(i) => i.get_sample_json_rows(table_name, column_name, limit).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.get_sample_json_rows(table_name, column_name, limit).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.get_sample_json_rows(table_name, column_name, limit).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.get_sample_json_rows(table_name, column_name, limit).await,
        }
    }

    async fn function_exists(
        &self,
        schema: Option<&str>,
        name: &str,
    ) -> fraiseql_core::Result<Option<bool>> {
        // Must delegate (not inherit the trait default) so the Postgres impl is
        // actually used — otherwise every variant would return `None`.
        match self {
            Self::Postgres(i) => i.function_exists(schema, name).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.function_exists(schema, name).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.function_exists(schema, name).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.function_exists(schema, name).await,
        }
    }

    async fn qualified_relation_exists(
        &self,
        schema: &str,
        name: &str,
    ) -> fraiseql_core::Result<Option<bool>> {
        match self {
            Self::Postgres(i) => i.qualified_relation_exists(schema, name).await,
            #[cfg(feature = "mysql")]
            Self::MySql(i) => i.qualified_relation_exists(schema, name).await,
            #[cfg(feature = "sqlite")]
            Self::Sqlite(i) => i.qualified_relation_exists(schema, name).await,
            #[cfg(feature = "sqlserver")]
            Self::SqlServer(i) => i.qualified_relation_exists(schema, name).await,
        }
    }
}

/// Create an introspector from a database URL.
///
/// Detects the database type from the URL scheme and creates the appropriate
/// introspector with a connection pool.
///
/// # Errors
///
/// Returns error if the URL scheme is unrecognized or the connection pool
/// cannot be created.
#[allow(clippy::unused_async)] // Reason: callers always .await this; feature-gated branches do use await
pub async fn create_introspector(db_url: &str) -> anyhow::Result<AnyIntrospector> {
    if db_url.starts_with("postgres") {
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
            .map_err(|e| anyhow::anyhow!("Failed to create PostgreSQL pool: {e}"))?;

        Ok(AnyIntrospector::Postgres(fraiseql_core::db::PostgresIntrospector::new(pool)))
    } else if db_url.starts_with("mysql") || db_url.starts_with("mariadb") {
        #[cfg(feature = "mysql")]
        {
            use sqlx::mysql::MySqlPool;

            let pool = MySqlPool::connect(db_url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create MySQL pool: {e}"))?;

            Ok(AnyIntrospector::MySql(fraiseql_core::db::MySqlIntrospector::new(pool)))
        }
        #[cfg(not(feature = "mysql"))]
        {
            anyhow::bail!("MySQL support not compiled in. Rebuild with `--features mysql`.")
        }
    } else if db_url.starts_with("sqlite")
        || std::path::Path::new(db_url)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("db") || ext.eq_ignore_ascii_case("sqlite"))
    {
        #[cfg(feature = "sqlite")]
        {
            use sqlx::sqlite::SqlitePool;

            let pool = SqlitePool::connect(db_url)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create SQLite pool: {e}"))?;

            Ok(AnyIntrospector::Sqlite(fraiseql_core::db::SqliteIntrospector::new(pool)))
        }
        #[cfg(not(feature = "sqlite"))]
        {
            anyhow::bail!("SQLite support not compiled in. Rebuild with `--features sqlite`.")
        }
    } else if db_url.starts_with("mssql") || db_url.starts_with("server=") {
        #[cfg(feature = "sqlserver")]
        {
            use bb8::Pool;
            use bb8_tiberius::ConnectionManager;
            use tiberius::Config;

            let config = Config::from_ado_string(db_url).map_err(|e| {
                anyhow::anyhow!("Failed to parse SQL Server connection string: {e}")
            })?;
            let mgr = ConnectionManager::build(config).map_err(|e| {
                anyhow::anyhow!("Failed to build SQL Server connection manager: {e}")
            })?;
            let pool = Pool::builder()
                .max_size(2)
                .build(mgr)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create SQL Server pool: {e}"))?;

            Ok(AnyIntrospector::SqlServer(fraiseql_core::db::SqlServerIntrospector::new(pool)))
        }
        #[cfg(not(feature = "sqlserver"))]
        {
            anyhow::bail!(
                "SQL Server support not compiled in. Rebuild with `--features sqlserver`."
            )
        }
    } else {
        anyhow::bail!("Unrecognized database URL scheme: {db_url}")
    }
}

/// Build a query's native-column map from its explicit (non-auto-param) arguments
/// and its inject-param names, consulting the introspected `column_map`.
///
/// A name that matches a real view column is rendered as a native-column predicate
/// (`WHERE col = $N`) at runtime; a name with no matching column falls back to the
/// JSONB path (`data->>'name'`). inject params were previously omitted here, so an
/// inject-scoped list query against a view that keeps e.g. `tenant_id` as a real
/// column (and not inside `data`) rendered `data->>'tenant_id'` and matched no rows.
///
/// Returns the native-column map plus the explicit-arg names that did NOT resolve to
/// a column — the caller emits a `NativeColumnFallback` warning for each. Inject-param
/// misses are intentionally silent: a claim may legitimately live in the `data` JSONB.
fn detect_query_native_columns<'a>(
    direct_arg_names: &[&str],
    inject_param_names: impl Iterator<Item = &'a str>,
    column_map: &HashMap<String, String>,
) -> (HashMap<String, String>, Vec<String>) {
    let mut native: HashMap<String, String> = HashMap::new();
    let mut arg_fallbacks: Vec<String> = Vec::new();

    for arg in direct_arg_names {
        if let Some(col_type) = column_map.get(*arg) {
            native.insert((*arg).to_string(), col_type.clone());
        } else {
            arg_fallbacks.push((*arg).to_string());
        }
    }

    for name in inject_param_names {
        if let Some(col_type) = column_map.get(name) {
            native.insert(name.to_string(), col_type.clone());
        }
    }

    (native, arg_fallbacks)
}

#[cfg(test)]
#[path = "database_validator_tests.rs"]
mod database_validator_tests;
