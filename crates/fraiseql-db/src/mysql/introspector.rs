//! MySQL database introspection for compile-time validation.

use fraiseql_error::{FraiseQLError, Result};
use sqlx::{Row, mysql::MySqlPool};

use crate::{
    DatabaseType,
    introspector::{DatabaseIntrospector, RelationInfo, RelationKind},
};

/// MySQL introspector for database metadata.
pub struct MySqlIntrospector {
    pool: MySqlPool,
}

impl MySqlIntrospector {
    /// Create new MySQL introspector from connection pool.
    #[must_use]
    pub const fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

impl DatabaseIntrospector for MySqlIntrospector {
    async fn list_fact_tables(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = DATABASE() \
               AND table_type = 'BASE TABLE' \
               AND table_name LIKE 'tf_%' \
             ORDER BY table_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message: format!("Failed to list fact tables: {e}"),
            sql_state: None,
        })?;

        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT column_name, data_type, is_nullable \
             FROM information_schema.columns \
             WHERE table_name = ? \
               AND table_schema = DATABASE() \
             ORDER BY ordinal_position",
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message: format!("Failed to query column information: {e}"),
            sql_state: None,
        })?;

        Ok(rows
            .into_iter()
            .map(|(name, data_type, nullable)| (name, data_type, nullable == "YES"))
            .collect())
    }

    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT column_name \
             FROM information_schema.statistics \
             WHERE table_name = ? \
               AND table_schema = DATABASE() \
             ORDER BY column_name",
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message: format!("Failed to query index information: {e}"),
            sql_state: None,
        })?;

        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::MySQL
    }

    async fn list_relations(&self) -> Result<Vec<RelationInfo>> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT table_schema, table_name, \
                    CASE table_type \
                        WHEN 'BASE TABLE' THEN 'table' \
                        WHEN 'VIEW' THEN 'view' \
                        WHEN 'SYSTEM VIEW' THEN 'view' \
                    END AS kind \
             FROM information_schema.tables \
             WHERE table_schema = DATABASE() \
               AND table_type IN ('BASE TABLE', 'VIEW', 'SYSTEM VIEW') \
             ORDER BY table_name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| FraiseQLError::Database {
            message: format!("Failed to list relations: {e}"),
            sql_state: None,
        })?;

        Ok(rows
            .into_iter()
            .map(|(schema, name, kind_str)| {
                let kind = if kind_str == "view" {
                    RelationKind::View
                } else {
                    RelationKind::Table
                };
                RelationInfo { schema, name, kind }
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
            "SELECT `{column}` FROM `{table}` WHERE `{column}` IS NOT NULL LIMIT {limit}",
            table = table_name,
            column = column_name,
        );

        let rows = sqlx::query(&query).fetch_all(&self.pool).await.map_err(|e| {
            FraiseQLError::Database {
                message: format!("Failed to query sample JSON rows: {e}"),
                sql_state: None,
            }
        })?;

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            let text: String = row.try_get(0).map_err(|e| FraiseQLError::Database {
                message: format!("Failed to read JSON column: {e}"),
                sql_state: None,
            })?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| FraiseQLError::Parse {
                    message: format!("Failed to parse JSON sample: {e}"),
                    location: format!("{table_name}.{column_name}"),
                })?;
            results.push(value);
        }

        Ok(results)
    }
}
