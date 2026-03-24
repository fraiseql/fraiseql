//! SQL Server database introspection for compile-time validation.

use bb8::Pool;
use bb8_tiberius::ConnectionManager;
use fraiseql_error::{FraiseQLError, Result};

use crate::{
    DatabaseType,
    introspector::{DatabaseIntrospector, RelationInfo, RelationKind},
};

/// SQL Server introspector for database metadata.
pub struct SqlServerIntrospector {
    pool: Pool<ConnectionManager>,
}

impl SqlServerIntrospector {
    /// Create new SQL Server introspector from connection pool.
    #[must_use]
    pub const fn new(pool: Pool<ConnectionManager>) -> Self {
        Self { pool }
    }
}

impl DatabaseIntrospector for SqlServerIntrospector {
    async fn list_fact_tables(&self) -> Result<Vec<String>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let rows = conn
            .simple_query(
                "SELECT TABLE_NAME FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE = 'BASE TABLE' \
                   AND TABLE_NAME LIKE 'tf_%' \
                 ORDER BY TABLE_NAME",
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to list fact tables: {e}"),
                sql_state: None,
            })?
            .into_first_result()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read fact table results: {e}"),
                sql_state: None,
            })?;

        let mut tables = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(name) = row.get::<&str, _>(0) {
                tables.push(name.to_string());
            }
        }

        Ok(tables)
    }

    async fn get_columns(&self, table_name: &str) -> Result<Vec<(String, String, bool)>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let query = format!(
            "SELECT COLUMN_NAME, DATA_TYPE, IS_NULLABLE \
             FROM INFORMATION_SCHEMA.COLUMNS \
             WHERE TABLE_NAME = '{table_name}' \
             ORDER BY ORDINAL_POSITION"
        );

        let rows = conn
            .simple_query(&query)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query column information: {e}"),
                sql_state: None,
            })?
            .into_first_result()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read column results: {e}"),
                sql_state: None,
            })?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            let name = row.get::<&str, _>(0).unwrap_or_default().to_string();
            let data_type = row.get::<&str, _>(1).unwrap_or_default().to_string();
            let nullable = row.get::<&str, _>(2).unwrap_or("NO") == "YES";
            columns.push((name, data_type, nullable));
        }

        Ok(columns)
    }

    async fn get_indexed_columns(&self, table_name: &str) -> Result<Vec<String>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let query = format!(
            "SELECT DISTINCT c.name \
             FROM sys.indexes i \
             JOIN sys.index_columns ic ON i.object_id = ic.object_id AND i.index_id = ic.index_id \
             JOIN sys.columns c ON ic.object_id = c.object_id AND ic.column_id = c.column_id \
             WHERE i.object_id = OBJECT_ID('{table_name}') \
             ORDER BY c.name"
        );

        let rows = conn
            .simple_query(&query)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query index information: {e}"),
                sql_state: None,
            })?
            .into_first_result()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read index results: {e}"),
                sql_state: None,
            })?;

        let mut columns = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(name) = row.get::<&str, _>(0) {
                columns.push(name.to_string());
            }
        }

        Ok(columns)
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::SQLServer
    }

    async fn list_relations(&self) -> Result<Vec<RelationInfo>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let rows = conn
            .simple_query(
                "SELECT TABLE_SCHEMA, TABLE_NAME, \
                        CASE TABLE_TYPE \
                            WHEN 'BASE TABLE' THEN 'table' \
                            WHEN 'VIEW' THEN 'view' \
                        END AS kind \
                 FROM INFORMATION_SCHEMA.TABLES \
                 WHERE TABLE_TYPE IN ('BASE TABLE', 'VIEW') \
                 ORDER BY TABLE_SCHEMA, TABLE_NAME",
            )
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to list relations: {e}"),
                sql_state: None,
            })?
            .into_first_result()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read relation results: {e}"),
                sql_state: None,
            })?;

        let mut relations = Vec::with_capacity(rows.len());
        for row in &rows {
            let schema = row.get::<&str, _>(0).unwrap_or("dbo").to_string();
            let name = row.get::<&str, _>(1).unwrap_or_default().to_string();
            let kind_str = row.get::<&str, _>(2).unwrap_or("table");
            let kind = if kind_str == "view" {
                RelationKind::View
            } else {
                RelationKind::Table
            };
            relations.push(RelationInfo { schema, name, kind });
        }

        Ok(relations)
    }

    async fn get_sample_json_rows(
        &self,
        table_name: &str,
        column_name: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let mut conn = self.pool.get().await.map_err(|e| FraiseQLError::ConnectionPool {
            message: format!("Failed to acquire connection: {e}"),
        })?;

        let query = format!(
            "SELECT TOP({limit}) [{column}] FROM [{table}] WHERE [{column}] IS NOT NULL",
            table = table_name,
            column = column_name,
        );

        let rows = conn
            .simple_query(&query)
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to query sample JSON rows: {e}"),
                sql_state: None,
            })?
            .into_first_result()
            .await
            .map_err(|e| FraiseQLError::Database {
                message:   format!("Failed to read sample JSON results: {e}"),
                sql_state: None,
            })?;

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            if let Some(text) = row.get::<&str, _>(0) {
                let value: serde_json::Value =
                    serde_json::from_str(text).map_err(|e| FraiseQLError::Parse {
                        message:  format!("Failed to parse JSON sample: {e}"),
                        location: format!("{table_name}.{column_name}"),
                    })?;
                results.push(value);
            }
        }

        Ok(results)
    }
}
