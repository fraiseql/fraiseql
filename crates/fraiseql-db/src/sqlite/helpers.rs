//! Helper functions for SQLite row conversion and SQL generation.

use fraiseql_error::{FraiseQLError, Result};
use sqlx::sqlite::SqliteRow;

use crate::{
    identifier::quote_sqlite_identifier,
    traits::{DirectMutationContext, DirectMutationOp},
};

/// Convert a SQLite row (from `RETURNING *`) into a JSON object.
///
/// Uses type-sniffing (i32 → i64 → f64 → String → bool → Null) to convert
/// each column value. String values that look like JSON are parsed.
pub(super) fn sqlite_row_to_json(row: &SqliteRow) -> serde_json::Value {
    use sqlx::{Column as _, Row as _, TypeInfo as _, ValueRef as _};

    let mut obj = serde_json::Map::new();
    for column in row.columns() {
        let name = column.name().to_string();

        // Check for NULL first — sqlx type-sniffing can coerce NULL to default values
        let is_null = row
            .try_get_raw(name.as_str())
            .is_ok_and(|v| v.is_null() || v.type_info().name() == "NULL");

        let value: serde_json::Value = if is_null {
            serde_json::Value::Null
        } else if let Ok(v) = row.try_get::<i32, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<i64, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<f64, _>(name.as_str()) {
            serde_json::json!(v)
        } else if let Ok(v) = row.try_get::<String, _>(name.as_str()) {
            if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&v) {
                json_val
            } else {
                serde_json::json!(v)
            }
        } else if let Ok(v) = row.try_get::<bool, _>(name.as_str()) {
            serde_json::json!(v)
        } else {
            serde_json::Value::Null
        };
        obj.insert(name, value);
    }
    serde_json::Value::Object(obj)
}

/// Build the SQL statement and ordered bind values for a direct mutation.
///
/// # Errors
///
/// Returns `FraiseQLError::Validation` if the operation/column combination is invalid.
pub(super) fn build_direct_mutation_sql<'a>(
    ctx: &'a DirectMutationContext<'_>,
) -> Result<(String, Vec<&'a serde_json::Value>)> {
    // Number of client args
    let n_client = ctx.columns.len();
    let n_inject = ctx.inject_columns.len();

    match ctx.operation {
        DirectMutationOp::Insert => {
            // INSERT INTO "table" ("col1", "col2", "inject1") VALUES (?, ?, ?) RETURNING *
            // All client + inject columns, all values in order
            let all_columns: Vec<String> = ctx
                .columns
                .iter()
                .chain(ctx.inject_columns.iter())
                .map(|c| quote_sqlite_identifier(c))
                .collect();
            let placeholders: Vec<&str> = vec!["?"; n_client + n_inject];
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({}) RETURNING *",
                quote_sqlite_identifier(ctx.table),
                all_columns.join(", "),
                placeholders.join(", ")
            );
            // Bind all values in order (client args + inject args)
            let bind_values: Vec<&serde_json::Value> = ctx.values.iter().collect();
            Ok((sql, bind_values))
        },
        DirectMutationOp::Update => {
            // UPDATE "table" SET "col2" = ?, "inject1" = ? WHERE "pk_col" = ? RETURNING *
            // First client column is PK (for WHERE), rest are SET columns
            if ctx.columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "UPDATE mutation requires at least one argument (primary key)".into(),
                    path:    None,
                });
            }
            let pk_col = quote_sqlite_identifier(&ctx.columns[0]);

            // SET columns: client columns after PK + inject columns
            let set_columns: Vec<String> = ctx.columns[1..]
                .iter()
                .chain(ctx.inject_columns.iter())
                .map(|c| format!("{} = ?", quote_sqlite_identifier(c)))
                .collect();

            if set_columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "UPDATE mutation requires at least one column to update".into(),
                    path:    None,
                });
            }

            let sql = format!(
                "UPDATE {} SET {} WHERE {} = ? RETURNING *",
                quote_sqlite_identifier(ctx.table),
                set_columns.join(", "),
                pk_col
            );

            // Bind order: SET values (client[1..] + inject), then PK value (client[0])
            let mut bind_values: Vec<&serde_json::Value> = Vec::with_capacity(ctx.values.len());
            // Client args after PK (indices 1..n_client)
            for v in &ctx.values[1..n_client] {
                bind_values.push(v);
            }
            // Inject args (indices n_client..)
            for v in &ctx.values[n_client..] {
                bind_values.push(v);
            }
            // PK value last (for WHERE clause)
            bind_values.push(&ctx.values[0]);
            Ok((sql, bind_values))
        },
        DirectMutationOp::Delete => {
            // DELETE FROM "table" WHERE "pk_col" = ? [AND "inject_col" = ?] RETURNING *
            if ctx.columns.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: "DELETE mutation requires at least one argument (primary key)".into(),
                    path:    None,
                });
            }
            let pk_col = quote_sqlite_identifier(&ctx.columns[0]);

            let mut where_parts = vec![format!("{pk_col} = ?")];
            for ic in ctx.inject_columns {
                where_parts.push(format!("{} = ?", quote_sqlite_identifier(ic)));
            }

            let sql = format!(
                "DELETE FROM {} WHERE {} RETURNING *",
                quote_sqlite_identifier(ctx.table),
                where_parts.join(" AND ")
            );

            // Bind order: PK value, then inject values
            let mut bind_values: Vec<&serde_json::Value> = Vec::with_capacity(1 + n_inject);
            bind_values.push(&ctx.values[0]);
            for v in &ctx.values[n_client..] {
                bind_values.push(v);
            }
            Ok((sql, bind_values))
        },
    }
}
