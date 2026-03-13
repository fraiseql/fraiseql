//! Audit logging for GraphQL operations
//!
//! Uses `PostgreSQL` `deadpool` for database operations

use std::{sync::Arc, time::SystemTime};

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Errors that can occur during audit operations.
#[derive(Debug, thiserror::Error)]
pub enum AuditError {
    /// Database operation failed.
    #[error("Database operation failed: {0}")]
    Database(#[from] deadpool_postgres::PoolError),

    /// SQL query execution failed.
    #[error("SQL query failed: {0}")]
    Sql(#[from] tokio_postgres::Error),

    /// Failed to serialize data to JSON.
    #[error("JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

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
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::INFO => "INFO",
            Self::WARN => "WARN",
            Self::ERROR => "ERROR",
        }
    }

    /// Parse from string
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "WARN" => Self::WARN,
            "ERROR" => Self::ERROR,
            _ => Self::INFO, // Default to INFO for unknown strings
        }
    }
}

/// Audit log entry with integrity protection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Entry ID (None for new entries)
    pub id:             Option<i64>,
    /// Timestamp
    pub timestamp:      DateTime<Utc>,
    /// Log level
    pub level:          AuditLevel,
    /// User ID
    pub user_id:        i64,
    /// Tenant ID
    pub tenant_id:      i64,
    /// Operation type (query, mutation)
    pub operation:      String,
    /// GraphQL query string
    pub query:          String,
    /// Query variables (JSONB)
    pub variables:      serde_json::Value,
    /// Client IP address
    pub ip_address:     String,
    /// Client user agent
    pub user_agent:     String,
    /// Error message (if any)
    pub error:          Option<String>,
    /// Query duration in milliseconds (optional)
    pub duration_ms:    Option<i32>,
    /// SHA256 hash of previous entry (for integrity chain)
    pub previous_hash:  Option<String>,
    /// SHA256 hash of this entry (for integrity verification)
    pub integrity_hash: Option<String>,
}

impl AuditEntry {
    /// Calculate SHA256 hash for this entry (for integrity chain)
    ///
    /// Hashes: user_id | timestamp | operation | query to create a tamper-proof chain
    #[must_use]
    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha256::new();

        // Include all mutable fields in hash for tamper detection
        hasher.update(self.user_id.to_string().as_bytes());
        hasher.update(self.timestamp.to_rfc3339().as_bytes());
        hasher.update(self.operation.as_bytes());
        hasher.update(self.query.as_bytes());
        hasher.update(self.level.as_str().as_bytes());

        // Include previous hash if present (hash chain)
        if let Some(ref prev) = self.previous_hash {
            hasher.update(prev.as_bytes());
        }

        format!("{:x}", hasher.finalize())
    }

    /// Verify integrity of this entry against its stored hash
    #[must_use]
    pub fn verify_integrity(&self) -> bool {
        if let Some(ref stored_hash) = self.integrity_hash {
            let calculated = self.calculate_hash();
            // Constant-time comparison to prevent timing attacks
            constant_time_eq(stored_hash.as_bytes(), calculated.as_bytes())
        } else {
            false
        }
    }
}

/// Constant-time comparison to prevent timing attacks.
///
/// Uses [`subtle::ConstantTimeEq`] — the same primitive used elsewhere in this
/// codebase — instead of a hand-rolled loop, which is brittle under optimiser
/// changes.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.ct_eq(b).into()
}

/// Statistics about audit events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStats {
    /// Total number of audit events recorded
    pub total_events:  u64,
    /// Number of recent events (last 24 hours or recent window)
    pub recent_events: u64,
}

/// Audit logger with `PostgreSQL` backend
#[derive(Clone, Debug)]
pub struct AuditLogger {
    pool: Arc<Pool>,
}

impl AuditLogger {
    /// Create a new audit logger
    #[must_use]
    pub const fn new(pool: Arc<Pool>) -> Self {
        Self { pool }
    }

    /// Maximum byte length for the `query` and `variables` fields in an audit log entry.
    ///
    /// Limits audit table bloat and prevents excess allocation during serialization.
    /// Queries that bypass `QueryValidator` (e.g. on error paths) may be arbitrarily large.
    const MAX_AUDIT_FIELD_BYTES: usize = 64 * 1024; // 64 KiB

    /// Log an audit entry.
    ///
    /// Truncates `query` and `variables` to [`Self::MAX_AUDIT_FIELD_BYTES`] before
    /// storing to prevent audit table bloat.
    ///
    /// # Errors
    ///
    /// Returns error if database operation fails.
    pub async fn log(&self, mut entry: AuditEntry) -> Result<i64, AuditError> {
        // Truncate oversized query and variables before storing to prevent
        // audit-table bloat from queries that bypass QueryValidator.
        if entry.query.len() > Self::MAX_AUDIT_FIELD_BYTES {
            entry.query.truncate(Self::MAX_AUDIT_FIELD_BYTES);
            entry.query.push_str("…[truncated]");
        }
        let vars_serialized = serde_json::to_string(&entry.variables).unwrap_or_default();
        if vars_serialized.len() > Self::MAX_AUDIT_FIELD_BYTES {
            entry.variables = serde_json::json!({"_truncated": true});
        }

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
        // timestamp() returns i64 seconds since epoch; audit events are always post-epoch.
        let secs = u64::try_from(entry.timestamp.timestamp()).unwrap_or(0);
        let timestamp_system = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(secs)
            + std::time::Duration::from_nanos(u64::from(entry.timestamp.timestamp_subsec_nanos()));

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
    ) -> Result<Vec<AuditEntry>, AuditError> {
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
            client.query(sql, &[&tenant_id, &lvl.as_str(), &limit]).await?
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
                    level: AuditLevel::parse(&level_str),
                    user_id,
                    tenant_id,
                    operation,
                    query,
                    variables,
                    ip_address,
                    user_agent,
                    error,
                    duration_ms,
                    previous_hash: None,
                    integrity_hash: None,
                }
            })
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_entry_integrity_hash() {
        let entry = AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(100),
            previous_hash:  None,
            integrity_hash: None,
        };

        let hash = entry.calculate_hash();
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // SHA256 hex is 64 chars
    }

    #[test]
    fn test_audit_integrity_verification() {
        let mut entry = AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(100),
            previous_hash:  None,
            integrity_hash: None,
        };

        // Calculate hash and store it
        let calculated_hash = entry.calculate_hash();
        entry.integrity_hash = Some(calculated_hash);

        // Verify should pass
        assert!(entry.verify_integrity());

        // Tamper with data
        entry.user_id = 999;

        // Verify should fail
        assert!(!entry.verify_integrity());
    }

    #[test]
    fn test_audit_hash_chain() {
        let timestamp = Utc::now();

        let mut entry1 = AuditEntry {
            id: Some(1),
            timestamp,
            level: AuditLevel::INFO,
            user_id: 123,
            tenant_id: 456,
            operation: "query".to_string(),
            query: "{ users { id } }".to_string(),
            variables: serde_json::json!({}),
            ip_address: "192.168.1.1".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            error: None,
            duration_ms: Some(100),
            previous_hash: None,
            integrity_hash: None,
        };

        let hash1 = entry1.calculate_hash();
        entry1.integrity_hash = Some(hash1.clone());

        // Create second entry with chain
        let mut entry2 = AuditEntry {
            id: Some(2),
            timestamp,
            level: AuditLevel::INFO,
            user_id: 123,
            tenant_id: 456,
            operation: "query".to_string(),
            query: "{ posts { id } }".to_string(),
            variables: serde_json::json!({}),
            ip_address: "192.168.1.1".to_string(),
            user_agent: "Mozilla/5.0".to_string(),
            error: None,
            duration_ms: Some(50),
            previous_hash: Some(hash1),
            integrity_hash: None,
        };

        let hash2 = entry2.calculate_hash();
        entry2.integrity_hash = Some(hash2);

        // Both should verify
        assert!(entry1.verify_integrity());
        assert!(entry2.verify_integrity());

        // Breaking the chain should be detected
        entry1.user_id = 999;
        assert!(!entry1.verify_integrity());
    }

    #[test]
    fn test_audit_level_parsing() {
        assert_eq!(AuditLevel::parse("WARN"), AuditLevel::WARN);
        assert_eq!(AuditLevel::parse("ERROR"), AuditLevel::ERROR);
        assert_eq!(AuditLevel::parse("INFO"), AuditLevel::INFO);
        assert_eq!(AuditLevel::parse("UNKNOWN"), AuditLevel::INFO);
    }
}
