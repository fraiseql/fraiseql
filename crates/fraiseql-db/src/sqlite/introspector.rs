//! SQLite database introspection for compile-time validation.

use sqlx::{
    Row,
    sqlite::SqlitePool,
};

use fraiseql_error::{FraiseQLError, Result};

use crate::{
    introspector::{DatabaseIntrospector, RelationInfo, RelationKind},
    DatabaseType,
};

/// SQLite introspector for database metadata.
pub struct SqliteIntrospector {
    pool: SqlitePool,
}

impl SqliteIntrospector {
    /// Create new SQLite introspector from connection pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DatabaseIntrospector for SqliteIntrospector {
    async fn list_fact_tables(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master \
             WHERE type = 'table' \
               AND name LIKE 'tf_%' \
             ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to list fact tables: {e}"),
            sql_state: None,
        })?;

        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>> {
        let query = format!("SELECT name, type, \"notnull\" FROM pragma_table_info('{table_name}') ORDER BY cid");

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query column information: {e}"),
                sql_state: None,
            })?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            let name: String = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read column name: {e}"),
                sql_state: None,
            })?;
            let data_type: String = row.try_get(1).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read column type: {e}"),
                sql_state: None,
            })?;
            let notnull: bool = row.try_get(2).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read notnull flag: {e}"),
                sql_state: None,
            })?;
            columns.push((name, data_type, !notnull));
        }

        Ok(columns)
    }

    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>> {
        let query = format!(
            "SELECT DISTINCT ii.name \
             FROM sqlite_master sm, \
                  pragma_index_list(sm.name) il, \
                  pragma_index_info(il.name) ii \
             WHERE sm.name = '{table_name}'"
        );

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query index information: {e}"),
                sql_state: None,
            })?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            let name: String = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read index column name: {e}"),
                sql_state: None,
            })?;
            columns.push(name);
        }

        Ok(columns)
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLite
    }

    async fn list_relations(&self) -> Result<Vec<RelationInfo>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table', 'view') \
               AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message:   format!("Failed to list relations: {e}"),
            sql_state: None,
        })?;

        Ok(rows
            .into_iter()
            .map(|(name, kind_str)| {
                let kind = if kind_str == "view" {
                    RelationKind::View
                } else {
                    RelationKind::Table
                };
                RelationInfo {
                    schema: "main".to_string(),
                    name,
                    kind,
                }
            })
            .collect())
    }

    async fn get_sample_json_rows(
        &self,
        table_name: &str,
        column_name: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let query = format!(
            "SELECT \"{column}\" FROM \"{table}\" WHERE \"{column}\" IS NOT NULL LIMIT {limit}",
            table = table_name,
            column = column_name,
        );

        let rows = sqlx::query(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query sample JSON rows: {e}"),
                sql_state: None,
            })?;

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            let text: String = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read JSON column: {e}"),
                sql_state: None,
            })?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| FraiseQLError::Parse {
                    message:  format!("Failed to parse JSON sample: {e}"),
                    location: format!("{table_name}.{column_name}"),
                })?;
            results.push(value);
        }

        Ok(results)
    }
}
