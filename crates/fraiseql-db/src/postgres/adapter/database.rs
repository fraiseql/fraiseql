//! `DatabaseAdapter` and `SupportsMutations` implementations for `PostgresAdapter`.

use async_trait::async_trait;
use bytes::BufMut as _;
use fraiseql_error::{FraiseQLError, Result};
use tokio_postgres::Row;

use super::{PostgresAdapter, build_where_select_sql, build_where_select_sql_ordered};
use crate::{
    identifier::quote_postgres_identifier,
    traits::{DatabaseAdapter, SupportsMutations},
    types::{
        DatabaseType, JsonbValue, PoolMetrics, QueryParam,
        sql_hints::{OrderByClause, SqlProjectionHint},
    },
    where_clause::WhereClause,
};

/// PostgreSQL SQLSTATE 42703: undefined column.
const PG_UNDEFINED_COLUMN: &str = "42703";

/// A flexible SQL parameter that binds to any PostgreSQL type.
///
/// Solves the impedance mismatch between `serde_json::Value` (only accepts JSON/JSONB)
/// and `Option<String>` (only accepts text-family types) when binding function-call
/// arguments whose types are resolved at runtime from the function signature.
///
/// Serialisation strategy (binary wire format):
/// - `JSONB`: 1-byte version header (1) + UTF-8 JSON bytes
/// - `JSON`: UTF-8 JSON bytes
/// - `UUID`: 16-byte big-endian UUID
/// - `INT4`: 4-byte big-endian i32
/// - `INT8`: 8-byte big-endian i64
/// - `BOOL`: 1-byte (0 or 1)
/// - All other types: UTF-8 bytes (PostgreSQL text binary = raw UTF-8)
#[derive(Debug)]
enum FlexParam {
    /// SQL NULL — accepted by any PostgreSQL type.
    Null,
    /// A text-encoded value; binary-serialised according to the server-resolved type.
    Text(String),
}

impl tokio_postgres::types::ToSql for FlexParam {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        use tokio_postgres::types::{IsNull, Type};
        match self {
            Self::Null => Ok(IsNull::Yes),
            Self::Text(s) => {
                if *ty == Type::JSONB {
                    // JSONB binary wire format: 1-byte version (1) + JSON bytes
                    out.put_u8(1);
                    out.extend_from_slice(s.as_bytes());
                } else if *ty == Type::JSON {
                    out.extend_from_slice(s.as_bytes());
                } else if *ty == Type::UUID {
                    let uuid = uuid::Uuid::parse_str(s)?;
                    out.extend_from_slice(uuid.as_bytes());
                } else if *ty == Type::INT4 {
                    let n: i32 = s.parse()?;
                    out.put_i32(n);
                } else if *ty == Type::INT8 {
                    let n: i64 = s.parse()?;
                    out.put_i64(n);
                } else if *ty == Type::BOOL {
                    let b: bool = s.parse()?;
                    out.put_u8(u8::from(b));
                } else {
                    // TEXT, VARCHAR, BPCHAR, NAME, UNKNOWN, and any user-defined type:
                    // UTF-8 bytes are the binary wire representation for text-family types.
                    out.extend_from_slice(s.as_bytes());
                }
                Ok(IsNull::No)
            }
        }
    }

    fn accepts(_ty: &tokio_postgres::types::Type) -> bool {
        // Accepts all types; per-type serialisation is handled in `to_sql`.
        true
    }

    fn to_sql_checked(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> std::result::Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    {
        // `accepts()` returns true for all types, so the standard WrongType check is
        // unnecessary.  Delegate directly to `to_sql`.
        self.to_sql(ty, out)
    }
}

/// Enrich a `FraiseQLError::Database` error for PostgreSQL SQLSTATE 42703 (undefined column)
/// when the WHERE clause contains `NativeField` conditions.
///
/// Native columns may be inferred automatically at compile time from `ID`/`UUID`-typed
/// arguments.  If the column does not exist on the target table at runtime, the raw
/// PostgreSQL error is replaced with a diagnostic message that names the native columns
/// involved and explains how to fix the schema.
fn enrich_undefined_column_error(
    err: FraiseQLError,
    view: &str,
    where_clause: Option<&WhereClause>,
) -> FraiseQLError {
    let FraiseQLError::Database { ref sql_state, .. } = err else {
        return err;
    };
    if sql_state.as_deref() != Some(PG_UNDEFINED_COLUMN) {
        return err;
    }
    let native_cols: Vec<&str> = where_clause
        .map(|wc| wc.native_column_names())
        .unwrap_or_default();
    if native_cols.is_empty() {
        return err;
    }
    FraiseQLError::Database {
        message: format!(
            "Column(s) {:?} referenced as native column(s) on `{view}` do not exist. \
             These columns were auto-inferred from ID/UUID-typed query arguments. \
             Either add the column(s) to the table/view, or set \
             `native_columns = {{}}` explicitly in your schema to disable inference.",
            native_cols,
        ),
        sql_state: Some(PG_UNDEFINED_COLUMN.to_string()),
    }
}

/// Convert a single `tokio_postgres::Row` into a `HashMap<String, serde_json::Value>`.
///
/// Tries each PostgreSQL type in priority order; falls back to `Null` for
/// types that cannot be represented as JSON.
fn row_to_map(row: &Row) -> std::collections::HashMap<String, serde_json::Value> {
    let mut map = std::collections::HashMap::new();
    for (idx, column) in row.columns().iter().enumerate() {
        let column_name = column.name().to_string();
        let value: serde_json::Value = if let Ok(v) = row.try_get::<_, i32>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, i64>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, f64>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, String>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, bool>(idx) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<_, serde_json::Value>(idx) {
            v
        } else {
            serde_json::Value::Null
        };
        map.insert(column_name, value);
    }
    map
}

// Reason: DatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl DatabaseAdapter for PostgresAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_with_projection_impl(view, projection, where_clause, limit, offset, order_by)
            .await
    }

    async fn execute_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
        order_by: Option<&[OrderByClause]>,
    ) -> Result<Vec<JsonbValue>> {
        let (sql, typed_params) =
            build_where_select_sql_ordered(view, where_clause, limit, offset, order_by)?;

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        self.execute_raw(&sql, &param_refs).await.map_err(|e| {
            enrich_undefined_column_error(e, view, where_clause)
        })
    }

    async fn explain_where_query(
        &self,
        view: &str,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<serde_json::Value> {
        let (select_sql, typed_params) = build_where_select_sql(view, where_clause, limit, offset)?;
        // Defense-in-depth: compiler-generated SQL should never contain a
        // semicolon, but guard against it to prevent statement injection.
        if select_sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {select_sql}");

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = typed_params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let client = self.acquire_connection_with_retry().await?;
        let rows = client.query(explain_sql.as_str(), &param_refs).await.map_err(|e| {
            FraiseQLError::Database {
                message:   format!("EXPLAIN ANALYZE failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            }
        })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to parse EXPLAIN output: {e}"),
                sql_state: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        // Use retry logic for health check to avoid false negatives during pool exhaustion
        let client = self.acquire_connection_with_retry().await?;

        client.query("SELECT 1", &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Health check failed: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        Ok(())
    }

    #[allow(clippy::cast_possible_truncation)] // Reason: value is bounded; truncation cannot occur in practice
    fn pool_metrics(&self) -> PoolMetrics {
        let status = self.pool.status();

        PoolMetrics {
            total_connections:  status.size as u32,
            idle_connections:   status.available as u32,
            active_connections: (status.size - status.available) as u32,
            waiting_requests:   status.waiting as u32,
        }
    }

    /// # Security
    ///
    /// `sql` **must** be compiler-generated. Never pass user-supplied strings
    /// directly — doing so would open SQL-injection vulnerabilities.
    async fn execute_raw_query(
        &self,
        sql: &str,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Use retry logic for connection acquisition
        let client = self.acquire_connection_with_retry().await?;

        let rows: Vec<Row> = client.query(sql, &[]).await.map_err(|e| FraiseQLError::Database {
            message:   format!("Query execution failed: {e}"),
            sql_state: e.code().map(|c| c.code().to_string()),
        })?;

        // Convert each row to HashMap<String, Value>
        let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
            rows.iter().map(row_to_map).collect();

        Ok(results)
    }

    async fn execute_parameterized_aggregate(
        &self,
        sql: &str,
        params: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Convert serde_json::Value params to QueryParam so that strings are bound
        // as TEXT (not JSONB), which is required for correct WHERE comparisons against
        // data->>'field' expressions that return TEXT.
        let typed: Vec<QueryParam> = params.iter().cloned().map(QueryParam::from).collect();
        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            typed.iter().map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> =
            client.query(sql, &param_refs).await.map_err(|e| FraiseQLError::Database {
                message:   format!("Parameterized aggregate query failed: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

        let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
            rows.iter().map(row_to_map).collect();

        Ok(results)
    }

    async fn execute_function_call(
        &self,
        function_name: &str,
        args: &[serde_json::Value],
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>> {
        // Build: SELECT * FROM "fn_name"($1, $2, ...)
        // Use the standard identifier quoting utility so that schema-qualified
        // names like "benchmark.fn_update_user" are correctly split into
        // "benchmark"."fn_update_user" instead of being wrapped as a single
        // identifier.
        let quoted_fn = quote_postgres_identifier(function_name);
        let placeholders: Vec<String> = (1..=args.len()).map(|i| format!("${i}")).collect();
        let sql = format!("SELECT * FROM {quoted_fn}({})", placeholders.join(", "));

        let mut client = self.acquire_connection_with_retry().await?;

        // Convert serde_json::Value arguments to FlexParam for binding.
        //
        // serde_json::Value only accepts JSON/JSONB types; Option<String> only accepts
        // text-family types.  Neither works universally when the function signature
        // contains a mix of JSONB, UUID, INT4, and TEXT parameters.  FlexParam accepts
        // all PostgreSQL types and serialises each value in the correct binary wire
        // format for the server-resolved parameter type.
        let flex_args: Vec<FlexParam> = args
            .iter()
            .map(|v| match v {
                serde_json::Value::Null => FlexParam::Null,
                serde_json::Value::String(s) => FlexParam::Text(s.clone()),
                _ => FlexParam::Text(v.to_string()),
            })
            .collect();
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = flex_args
            .iter()
            .map(|v| v as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        if self.mutation_timing_enabled {
            // Wrap in a transaction so SET LOCAL scopes the variable to this call only.
            // `set_config(name, value, is_local)` with is_local=true is equivalent to
            // SET LOCAL and is parameterized to avoid SQL injection.
            let txn =
                client.build_transaction().start().await.map_err(|e| FraiseQLError::Database {
                    message:   format!("Failed to start mutation timing transaction: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                })?;

            txn.execute(
                "SELECT set_config($1, clock_timestamp()::text, true)",
                &[&self.timing_variable_name],
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to set mutation timing variable: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

            let rows: Vec<Row> = txn.query(sql.as_str(), params.as_slice()).await.map_err(|e| {
                let detail = e.as_db_error().map(|d| d.message()).unwrap_or("");
                FraiseQLError::Database {
                    message:   format!("Function call {function_name} failed: {e}: {detail}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                }
            })?;

            txn.commit().await.map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to commit mutation timing transaction: {e}"),
                sql_state: e.code().map(|c| c.code().to_string()),
            })?;

            let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
                rows.iter().map(row_to_map).collect();

            Ok(results)
        } else {
            let rows: Vec<Row> =
                client.query(sql.as_str(), params.as_slice()).await.map_err(|e| {
                    let detail = e.as_db_error().map(|d| d.message()).unwrap_or("");
                    FraiseQLError::Database {
                        message:   format!("Function call {function_name} failed: {e}: {detail}"),
                        sql_state: e.code().map(|c| c.code().to_string()),
                    }
                })?;

            let results: Vec<std::collections::HashMap<String, serde_json::Value>> =
                rows.iter().map(row_to_map).collect();

            Ok(results)
        }
    }

    async fn set_session_variables(&self, variables: &[(&str, &str)]) -> Result<()> {
        if variables.is_empty() {
            return Ok(());
        }
        let client = self.acquire_connection_with_retry().await?;
        for (name, value) in variables {
            client
                .execute("SELECT set_config($1, $2, true)", &[name, value])
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("set_config({name:?}) failed: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                })?;
        }
        Ok(())
    }

    async fn explain_query(
        &self,
        sql: &str,
        _params: &[serde_json::Value],
    ) -> Result<serde_json::Value> {
        // Defense-in-depth: reject multi-statement input even though this SQL is
        // compiler-generated. A semicolon would allow a second statement to be
        // appended to the EXPLAIN prefix.
        if sql.contains(';') {
            return Err(FraiseQLError::Validation {
                message: "EXPLAIN SQL must be a single statement".into(),
                path:    None,
            });
        }
        let explain_sql = format!("EXPLAIN (ANALYZE false, FORMAT JSON) {sql}");
        let client = self.acquire_connection_with_retry().await?;
        let rows: Vec<Row> =
            client
                .query(explain_sql.as_str(), &[])
                .await
                .map_err(|e| FraiseQLError::Database {
                    message:   format!("EXPLAIN failed: {e}"),
                    sql_state: e.code().map(|c| c.code().to_string()),
                })?;

        if let Some(row) = rows.first() {
            let plan: serde_json::Value = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to parse EXPLAIN output: {e}"),
                sql_state: None,
            })?;
            Ok(plan)
        } else {
            Ok(serde_json::Value::Null)
        }
    }
}

impl SupportsMutations for PostgresAdapter {}
