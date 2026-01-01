//! Audit logging for GraphQL operations
//!
//! Uses PostgreSQL deadpool for database operations

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

/// Audit log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditLevel {
    /// Informational messages
    INFO,
    /// Warnings
    WARN,
    /// Errors
    ERROR,
}

impl AuditLevel {
    /// Convert to string for database storage
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditLevel::INFO => "INFO",
            AuditLevel::WARN => "WARN",
            AuditLevel::ERROR => "ERROR",
        }
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s {
            "INFO" => AuditLevel::INFO,
            "WARN" => AuditLevel::WARN,
            "ERROR" => AuditLevel::ERROR,
            _ => AuditLevel::INFO,
        }
    }
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry ID (None for new entries)
    pub id: Option<i64>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Log level
    pub level: AuditLevel,
    /// User ID
    pub user_id: i64,
    /// Tenant ID
    pub tenant_id: i64,
    /// Operation type (query, mutation)
    pub operation: String,
    /// GraphQL query string
    pub query: String,
    /// Query variables (JSONB)
    pub variables: serde_json::Value,
    /// Client IP address
    pub ip_address: String,
    /// Client user agent
    pub user_agent: String,
    /// Error message (if any)
    pub error: Option<String>,
    /// Query duration in milliseconds (optional)
    pub duration_ms: Option<i32>,
}

/// Audit logger with PostgreSQL backend
#[derive(Clone)]
pub struct AuditLogger {
    pool: Arc<Pool>,
}

impl AuditLogger {
    /// Create a new audit logger
    pub fn new(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Log an audit entry
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn log(&self, entry: AuditEntry) -> Result<i64, anyhow::Error> {
        let sql = r"
            INSERT INTO fraiseql_audit_logs (
                timestamp,
                level,
                user_id,
                tenant_id,
                operation,
                query,
                variables,
                ip_address,
                user_agent,
                error,
                duration_ms
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
        ";

        let client = self.pool.get().await?;
        let variables_json = serde_json::to_value(&entry.variables)?;

        // Convert DateTime<Utc> to SystemTime for PostgreSQL
        let timestamp_system = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(entry.timestamp.timestamp() as u64)
            + std::time::Duration::from_nanos(entry.timestamp.timestamp_subsec_nanos() as u64);

        let row = client
            .query_one(
                sql,
                &[
                    &timestamp_system,
                    &entry.level.as_str(),
                    &entry.user_id,
                    &entry.tenant_id,
                    &entry.operation,
                    &entry.query,
                    &variables_json,
                    &entry.ip_address,
                    &entry.user_agent,
                    &entry.error,
                    &entry.duration_ms,
                ],
            )
            .await?;

        let id: i64 = row.get(0);
        Ok(id)
    }

    /// Get recent logs for a tenant
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails
    pub async fn get_recent_logs(
        &self,
        tenant_id: i64,
        level: Option<AuditLevel>,
        limit: i64,
    ) -> Result<Vec<AuditEntry>, anyhow::Error> {
        let client = self.pool.get().await?;

        let rows = if let Some(lvl) = level {
            let sql = r"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error, duration_ms
                FROM fraiseql_audit_logs
                WHERE tenant_id = $1 AND level = $2
                ORDER BY timestamp DESC
                LIMIT $3
            ";
            client
                .query(sql, &[&tenant_id, &lvl.as_str(), &limit])
                .await?
        } else {
            let sql = r"
                SELECT id, timestamp, level, user_id, tenant_id, operation,
                       query, variables, ip_address, user_agent, error, duration_ms
                FROM fraiseql_audit_logs
                WHERE tenant_id = $1
                ORDER BY timestamp DESC
                LIMIT $2
            ";
            client.query(sql, &[&tenant_id, &limit]).await?
        };

        let entries: Vec<AuditEntry> = rows
            .into_iter()
            .map(|row| {
                let id: Option<i64> = row.get(0);
                let timestamp_system: SystemTime = row.get(1);
                let level_str: String = row.get(2);
                let user_id: i64 = row.get(3);
                let tenant_id: i64 = row.get(4);
                let operation: String = row.get(5);
                let query: String = row.get(6);
                let variables: serde_json::Value = row.get(7);
                let ip_address: String = row.get(8);
                let user_agent: String = row.get(9);
                let error: Option<String> = row.get(10);
                let duration_ms: Option<i32> = row.get(11);

                // Convert SystemTime to DateTime<Utc>
                let timestamp = DateTime::<Utc>::from(timestamp_system);

                AuditEntry {
                    id,
                    timestamp,
                    level: AuditLevel::from_str(&level_str),
                    user_id,
                    tenant_id,
                    operation,
                    query,
                    variables,
                    ip_address,
                    user_agent,
                    error,
                    duration_ms,
                }
            })
            .collect();

        Ok(entries)
    }
}
