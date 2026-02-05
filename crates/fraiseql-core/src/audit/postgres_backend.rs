//! PostgreSQL audit backend
//!
//! Stores audit events in PostgreSQL with full-text search, JSONB storage,
//! and performance-optimized indexes.

use super::*;
use deadpool_postgres::Pool;

/// PostgreSQL audit backend for persistent, queryable audit logs.
///
/// Features:
/// - JSONB columns for metadata and state snapshots
/// - Optimized indexes for common query patterns
/// - Multi-tenancy support with tenant_id isolation
/// - Connection pooling via deadpool-postgres
#[derive(Clone)]
pub struct PostgresAuditBackend {
    /// Connection pool for database access
    pool: Pool,
}

impl PostgresAuditBackend {
    /// Create a new PostgreSQL audit backend.
    ///
    /// # Arguments
    ///
    /// * `pool` - Database connection pool
    ///
    /// # Errors
    ///
    /// Returns error if table creation fails
    pub async fn new(pool: Pool) -> AuditResult<Self> {
        // Ensure audit_log table exists with proper schema
        Self::ensure_table_exists(&pool).await?;
        Ok(Self { pool })
    }

    /// Ensure audit_log table exists with proper schema and indexes.
    async fn ensure_table_exists(pool: &Pool) -> AuditResult<()> {
        let client = pool
            .get()
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Failed to get connection: {}", e)))?;

        // Create audit_log table if not exists
        let create_table_sql = r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id UUID PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,
                event_type VARCHAR(255) NOT NULL,
                user_id VARCHAR(255) NOT NULL,
                username VARCHAR(255) NOT NULL,
                ip_address VARCHAR(45) NOT NULL,
                resource_type VARCHAR(255) NOT NULL,
                resource_id VARCHAR(255),
                action VARCHAR(255) NOT NULL,
                before_state JSONB,
                after_state JSONB,
                status VARCHAR(32) NOT NULL,
                error_message TEXT,
                tenant_id VARCHAR(255),
                metadata JSONB NOT NULL DEFAULT '{}'::JSONB
            )
        "#;

        client
            .execute(create_table_sql, &[])
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Failed to create table: {}", e)))?;

        // Create indexes for performance
        Self::ensure_indexes(&client).await?;

        Ok(())
    }

    /// Create performance indexes if they don't exist.
    async fn ensure_indexes(client: &deadpool_postgres::Object) -> AuditResult<()> {
        let indexes = vec![
            // Index on timestamp for time range queries (descending for recent-first)
            "CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log (timestamp DESC)",
            // Index on user_id for user-specific audits
            "CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_log (user_id)",
            // Index on event_type for event filtering
            "CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log (event_type)",
            // Partial index on tenant_id (only non-null for efficiency)
            "CREATE INDEX IF NOT EXISTS idx_audit_tenant_id ON audit_log (tenant_id) WHERE tenant_id IS NOT NULL",
            // Composite indexes for common query patterns
            "CREATE INDEX IF NOT EXISTS idx_audit_tenant_time ON audit_log (tenant_id, timestamp DESC) WHERE tenant_id IS NOT NULL",
            "CREATE INDEX IF NOT EXISTS idx_audit_user_time ON audit_log (user_id, timestamp DESC)",
            // Index on status for failure/denied queries
            "CREATE INDEX IF NOT EXISTS idx_audit_status ON audit_log (status) WHERE status != 'success'",
        ];

        for index_sql in indexes {
            client
                .execute(index_sql, &[])
                .await
                .map_err(|e| AuditError::DatabaseError(format!("Failed to create index: {}", e)))?;
        }

        Ok(())
    }

    /// Convert UUID from string to bytes for PostgreSQL UUID type.
    fn parse_uuid(id: &str) -> AuditResult<uuid::Uuid> {
        uuid::Uuid::parse_str(id)
            .map_err(|e| AuditError::DatabaseError(format!("Invalid UUID: {}", e)))
    }
}

#[async_trait::async_trait]
impl AuditBackend for PostgresAuditBackend {
    /// Log an audit event to PostgreSQL.
    async fn log_event(&self, event: AuditEvent) -> AuditResult<()> {
        // Validate event before logging
        event.validate()?;

        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Failed to get connection: {}", e)))?;

        let event_id = Self::parse_uuid(&event.id)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&event.timestamp)
            .map_err(|e| {
                AuditError::DatabaseError(format!("Invalid timestamp format: {}", e))
            })?
            .with_timezone(&chrono::Utc);

        let insert_sql = r#"
            INSERT INTO audit_log (
                id, timestamp, event_type, user_id, username, ip_address,
                resource_type, resource_id, action, before_state, after_state,
                status, error_message, tenant_id, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11,
                $12, $13, $14, $15
            )
        "#;

        client
            .execute(
                insert_sql,
                &[
                    &event_id,
                    &timestamp,
                    &event.event_type,
                    &event.user_id,
                    &event.username,
                    &event.ip_address,
                    &event.resource_type,
                    &event.resource_id,
                    &event.action,
                    &event.before_state,
                    &event.after_state,
                    &event.status,
                    &event.error_message,
                    &event.tenant_id,
                    &event.metadata,
                ],
            )
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Failed to insert event: {}", e)))?;

        Ok(())
    }

    /// Query audit events from PostgreSQL with filters.
    async fn query_events(&self, filters: AuditQueryFilters) -> AuditResult<Vec<AuditEvent>> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Failed to get connection: {}", e)))?;

        // Start with base query
        let mut query = "SELECT id, timestamp, event_type, user_id, username, ip_address, \
                         resource_type, resource_id, action, before_state, after_state, \
                         status, error_message, tenant_id, metadata \
                         FROM audit_log"
            .to_string();

        // Build WHERE clause with filters
        let mut where_parts = vec![];

        if filters.event_type.is_some() {
            where_parts.push("event_type = $1".to_string());
        }
        if filters.user_id.is_some() {
            where_parts.push(format!("user_id = ${}", where_parts.len() + 1));
        }
        if filters.resource_type.is_some() {
            where_parts.push(format!("resource_type = ${}", where_parts.len() + 1));
        }
        if filters.status.is_some() {
            where_parts.push(format!("status = ${}", where_parts.len() + 1));
        }
        if filters.tenant_id.is_some() {
            where_parts.push(format!("tenant_id = ${}", where_parts.len() + 1));
        }
        if filters.start_time.is_some() {
            where_parts.push(format!("timestamp >= ${}", where_parts.len() + 1));
        }
        if filters.end_time.is_some() {
            where_parts.push(format!("timestamp <= ${}", where_parts.len() + 1));
        }

        if !where_parts.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&where_parts.join(" AND "));
        }

        query.push_str(" ORDER BY timestamp DESC");

        let limit = filters.limit.unwrap_or(100);
        let offset = filters.offset.unwrap_or(0);
        query.push_str(&format!(" LIMIT {} OFFSET {}", limit, offset));

        // Build parameter vector in correct order
        let mut param_strs: Vec<String> = vec![];

        if let Some(ref val) = filters.event_type {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.user_id {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.resource_type {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.status {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.tenant_id {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.start_time {
            param_strs.push(val.clone());
        }
        if let Some(ref val) = filters.end_time {
            param_strs.push(val.clone());
        }

        // Convert owned strings to references for query parameters
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            param_strs.iter().map(|s| s as &(dyn tokio_postgres::types::ToSql + Sync)).collect();

        let rows = client
            .query(query.as_str(), params.as_slice())
            .await
            .map_err(|e| AuditError::DatabaseError(format!("Query failed: {}", e)))?;

        let mut events = vec![];
        for row in rows {
            let id: uuid::Uuid = row.get(0);
            let timestamp: chrono::DateTime<chrono::Utc> = row.get(1);
            let event_type: String = row.get(2);
            let user_id: String = row.get(3);
            let username: String = row.get(4);
            let ip_address: String = row.get(5);
            let resource_type: String = row.get(6);
            let resource_id: Option<String> = row.get(7);
            let action: String = row.get(8);
            let before_state: Option<serde_json::Value> = row.get(9);
            let after_state: Option<serde_json::Value> = row.get(10);
            let status: String = row.get(11);
            let error_message: Option<String> = row.get(12);
            let tenant_id: Option<String> = row.get(13);
            let metadata: serde_json::Value = row.get(14);

            events.push(AuditEvent {
                id: id.to_string(),
                timestamp: timestamp.to_rfc3339(),
                event_type,
                user_id,
                username,
                ip_address,
                resource_type,
                resource_id,
                action,
                before_state,
                after_state,
                status,
                error_message,
                tenant_id,
                metadata,
            });
        }

        Ok(events)
    }
}

// Re-export for convenience
pub use super::AuditBackend;
