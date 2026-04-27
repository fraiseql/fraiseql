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
    schema::CompiledSchema,
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
                sql_source,
                json_column,
                field_name,
                json_key,
            } => {
                write!(
                    f,
                    "query `{query_name}`: field `{field_name}` (key `{json_key}`) not found in `{sql_source}.{json_column}` sample data"
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
        }
    }
}

/// Check if a SQL data type represents a JSON column for the given database.
fn is_json_type(data_type: &str, db_type: DatabaseType) -> bool {
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

/// Convert a `camelCase` or `PascalCase` field name to `snake_case`.
///
/// This matches the convention used by FraiseQL for JSONB key extraction.
fn to_snake_case(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
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
            // L1: Check relation exists
            if !relation_exists(&schema_qualified, &unqualified, source) {
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

            // L2: Detect native columns for direct (non-auto-param) arguments.
            let direct_args: Vec<&str> = query
                .arguments
                .iter()
                .filter(|a| !AUTO_PARAM_NAMES.contains(&a.name.as_str()))
                .map(|a| a.name.as_str())
                .collect();

            if !direct_args.is_empty() {
                let mut query_native: HashMap<String, String> = HashMap::new();
                for arg_name in &direct_args {
                    if let Some(col_type) = column_map.get(*arg_name) {
                        query_native.insert((*arg_name).to_string(), col_type.clone());
                    } else {
                        warnings.push(DatabaseWarning::NativeColumnFallback {
                            query_name: query.name.clone(),
                            sql_source: source.clone(),
                            arg_name:   (*arg_name).to_string(),
                        });
                    }
                }
                if !query_native.is_empty() {
                    native_columns.insert(query.name.clone(), query_native);
                }
            }

            // L1: Check additional_views
            for view in &query.additional_views {
                if !relation_exists(&schema_qualified, &unqualified, view) {
                    warnings.push(DatabaseWarning::MissingAdditionalView {
                        query_name: query.name.clone(),
                        view_name:  view.clone(),
                    });
                }
            }
        }
    }

    // Validate mutations (L1 only)
    for mutation in &schema.mutations {
        if let Some(ref source) = mutation.sql_source {
            if !relation_exists(&schema_qualified, &unqualified, source) {
                warnings.push(DatabaseWarning::MissingRelation {
                    query_name: mutation.name.clone(),
                    sql_source: source.clone(),
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
            let json_key = to_snake_case(field_str);
            // Skip fields that are top-level columns (not from JSONB)
            // Convention: fields like "id", "pk_*", "fk_*" are columns, not JSON keys
            if field_str == "id" || field_str.starts_with("pk_") || field_str.starts_with("fk_") {
                continue;
            }
            if !all_keys.contains(&json_key) && !all_keys.contains(field_str) {
                warnings.push(DatabaseWarning::MissingJsonKey {
                    query_name: query.name.clone(),
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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        schema::{
            AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, MutationDefinition,
            QueryDefinition, TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    use super::*;

    /// Mock introspector for unit tests.
    struct MockIntrospector {
        relations:    Vec<RelationInfo>,
        columns:      HashMap<String, Vec<(String, String, bool)>>,
        json_samples: HashMap<(String, String), Vec<serde_json::Value>>,
        db_type:      DatabaseType,
    }

    impl MockIntrospector {
        fn new(db_type: DatabaseType) -> Self {
            Self {
                relations: Vec::new(),
                columns: HashMap::new(),
                json_samples: HashMap::new(),
                db_type,
            }
        }

        fn with_relation(
            mut self,
            schema: &str,
            name: &str,
            kind: fraiseql_core::db::RelationKind,
        ) -> Self {
            self.relations.push(RelationInfo {
                schema: schema.to_string(),
                name: name.to_string(),
                kind,
            });
            self
        }

        fn with_columns(mut self, table: &str, cols: Vec<(&str, &str, bool)>) -> Self {
            self.columns.insert(
                table.to_string(),
                cols.into_iter()
                    .map(|(n, t, nullable)| (n.to_string(), t.to_string(), nullable))
                    .collect(),
            );
            self
        }

        fn with_json_samples(
            mut self,
            table: &str,
            column: &str,
            samples: Vec<serde_json::Value>,
        ) -> Self {
            self.json_samples.insert((table.to_string(), column.to_string()), samples);
            self
        }
    }

    impl DatabaseIntrospector for MockIntrospector {
        async fn list_fact_tables(&self) -> fraiseql_core::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn get_columns(
            &self,
            table_name: &str,
        ) -> fraiseql_core::Result<Vec<(String, String, bool)>> {
            Ok(self.columns.get(table_name).cloned().unwrap_or_default())
        }

        async fn get_indexed_columns(
            &self,
            _table_name: &str,
        ) -> fraiseql_core::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn database_type(&self) -> DatabaseType {
            self.db_type
        }

        async fn list_relations(&self) -> fraiseql_core::Result<Vec<RelationInfo>> {
            Ok(self.relations.clone())
        }

        async fn get_sample_json_rows(
            &self,
            table_name: &str,
            column_name: &str,
            _limit: usize,
        ) -> fraiseql_core::Result<Vec<serde_json::Value>> {
            Ok(self
                .json_samples
                .get(&(table_name.to_string(), column_name.to_string()))
                .cloned()
                .unwrap_or_default())
        }
    }

    fn make_query(name: &str, return_type: &str, sql_source: &str) -> QueryDefinition {
        QueryDefinition {
            name:                name.to_string(),
            return_type:         return_type.to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some(sql_source.to_string()),
            sql_source_dispatch: None,
            description:         None,
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
        }
    }

    fn make_type(name: &str, fields: Vec<(&str, FieldType)>) -> TypeDefinition {
        TypeDefinition {
            name:                name.into(),
            fields:              fields
                .into_iter()
                .map(|(n, ft)| FieldDefinition::new(n, ft))
                .collect(),
            description:         None,
            sql_source:          "".into(),
            jsonb_column:        "data".to_string(),
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       Vec::new(),
        }
    }

    fn make_schema(types: Vec<TypeDefinition>, queries: Vec<QueryDefinition>) -> CompiledSchema {
        CompiledSchema {
            types,
            queries,
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
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_valid_schema_no_warnings() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false), ("pk_user", "bigint", false)])
            .with_json_samples(
                "v_user",
                "data",
                vec![serde_json::json!({"name": "Alice", "email": "alice@example.com"})],
            );

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![("name", FieldType::String), ("email", FieldType::String)],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(
            report.warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            report.warnings.len()
        );
    }

    #[tokio::test]
    async fn test_missing_relation() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);
        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "v_user")
        );
    }

    #[tokio::test]
    async fn test_missing_additional_view() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let mut query = make_query("users", "User", "v_user");
        query.additional_views = vec!["v_missing".to_string()];

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingAdditionalView { view_name, .. } if view_name == "v_missing")
        );
    }

    #[tokio::test]
    async fn test_missing_jsonb_column() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("pk_user", "bigint", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingJsonColumn { column_name, .. } if column_name == "data")
        );
    }

    #[tokio::test]
    async fn test_wrong_json_column_type() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "text", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::WrongJsonColumnType { actual_type, .. } if actual_type == "text")
        );
    }

    #[tokio::test]
    async fn test_sqlserver_nvarchar_no_warning() {
        let introspector = MockIntrospector::new(DatabaseType::SQLServer)
            .with_relation("dbo", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "nvarchar", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // SQL Server: nvarchar is always accepted for JSON columns
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::WrongJsonColumnType { .. }))
        );
    }

    #[tokio::test]
    async fn test_missing_cursor_column() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let mut query = make_query("users", "User", "v_user");
        query.relay = true;
        query.relay_cursor_column = Some("pk_user".to_string());

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingCursorColumn { column_name, .. } if column_name == "pk_user")));
    }

    #[tokio::test]
    async fn test_missing_json_key() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)])
            .with_json_samples("v_user", "data", vec![serde_json::json!({"name": "Alice"})]);

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![("name", FieldType::String), ("email", FieldType::String)],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingJsonKey { field_name, .. } if field_name == "email")));
    }

    #[tokio::test]
    async fn test_empty_json_sample_no_l3_warnings() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let schema = make_schema(
            vec![make_type("User", vec![("name", FieldType::String)])],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // No L3 warnings because no sample data
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingJsonKey { .. }))
        );
    }

    #[tokio::test]
    async fn test_schema_qualified_match() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("etl_log", "v_foo", fraiseql_core::db::RelationKind::View)
            .with_columns("v_foo", vec![("data", "jsonb", false)]);

        let schema = make_schema(vec![], vec![make_query("foos", "Foo", "etl_log.v_foo")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // Should match
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingRelation { .. }))
        );
    }

    #[tokio::test]
    async fn test_schema_qualified_wrong_schema() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL).with_relation(
            "public",
            "v_foo",
            fraiseql_core::db::RelationKind::View,
        );

        let schema = make_schema(vec![], vec![make_query("foos", "Foo", "etl_log.v_foo")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "etl_log.v_foo")
        );
    }

    #[tokio::test]
    async fn test_mutation_missing_sql_source() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);

        let mut schema = make_schema(vec![], vec![]);
        schema.mutations.push(MutationDefinition {
            name: "createUser".to_string(),
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "fn_create_user")
        );
    }

    #[tokio::test]
    async fn test_query_no_sql_source_skipped() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);

        let mut query = make_query("users", "User", "v_user");
        query.sql_source = None;

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_samples_merge_keys() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)])
            .with_json_samples(
                "v_user",
                "data",
                vec![
                    serde_json::json!({"name": "Alice", "email": "alice@example.com"}),
                    serde_json::json!({"email": "bob@example.com", "age": 30}),
                ],
            );

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![
                    ("name", FieldType::String),
                    ("email", FieldType::String),
                    ("age", FieldType::Int),
                ],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // All keys present across both samples
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingJsonKey { .. }))
        );
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("firstName"), "first_name");
        assert_eq!(to_snake_case("name"), "name");
        assert_eq!(to_snake_case("HTMLParser"), "h_t_m_l_parser");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_is_json_type_postgres() {
        assert!(is_json_type("jsonb", DatabaseType::PostgreSQL));
        assert!(is_json_type("json", DatabaseType::PostgreSQL));
        assert!(!is_json_type("text", DatabaseType::PostgreSQL));
    }

    #[test]
    fn test_is_json_type_mysql() {
        assert!(is_json_type("json", DatabaseType::MySQL));
        assert!(!is_json_type("varchar", DatabaseType::MySQL));
    }

    #[test]
    fn test_is_json_type_sqlite() {
        assert!(is_json_type("json", DatabaseType::SQLite));
        assert!(is_json_type("JSON", DatabaseType::SQLite));
        assert!(!is_json_type("text", DatabaseType::SQLite));
    }

    #[test]
    fn test_is_json_type_sqlserver() {
        // SQL Server always returns true
        assert!(is_json_type("nvarchar", DatabaseType::SQLServer));
        assert!(is_json_type("varchar", DatabaseType::SQLServer));
    }

    #[test]
    fn test_display_warnings() {
        let warning = DatabaseWarning::MissingRelation {
            query_name: "users".to_string(),
            sql_source: "v_user".to_string(),
        };
        assert_eq!(
            warning.to_string(),
            "query `users`: sql_source `v_user` does not exist in database"
        );
    }
}
