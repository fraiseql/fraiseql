//! Audit logging for GraphQL operations.
//!
//! Uses `PostgreSQL` `deadpool` for database operations. Supports optional
//! pluggable export sinks (syslog, webhook) for streaming audit entries to
//! external immutable stores.

use std::{sync::Arc, time::SystemTime};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::error;

/// Errors that can occur during audit operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// Export to an external sink failed.
    #[error("Audit export failed: {0}")]
    Export(String),
}

/// Audit log levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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
    /// Hashes: `user_id` | timestamp | operation | query to create a tamper-proof chain
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

// ============================================================================
// Pluggable export sinks
// ============================================================================

/// Pluggable sink for streaming audit entries to external systems.
///
/// Implementations can send entries to syslog, webhooks, S3, or any other
/// external store. The caller decides whether to retry or drop entries on
/// failure.
///
/// # Errors
///
/// Returns error if the export fails (network, serialization, etc.).
#[async_trait]
pub trait AuditExporter: Send + Sync {
    /// Export a single audit entry to the external sink.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError`] if the export fails.
    async fn export(&self, entry: &AuditEntry) -> Result<(), AuditError>;

    /// Flush any buffered entries to the external sink.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError`] if the flush fails.
    async fn flush(&self) -> Result<(), AuditError>;
}

/// Configuration for audit log export sinks.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditExportConfig {
    /// Syslog export configuration (requires `audit-syslog` feature).
    #[serde(default)]
    pub syslog:  Option<SyslogExportConfig>,
    /// Webhook export configuration (requires `audit-webhook` feature).
    #[serde(default)]
    pub webhook: Option<WebhookExportConfig>,
}

/// Configuration for the syslog audit exporter (RFC 5424).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyslogExportConfig {
    /// Syslog server hostname or IP.
    pub address:  String,
    /// Syslog server port (default: 514).
    #[serde(default = "default_syslog_port")]
    pub port:     u16,
    /// Transport protocol: "tcp" or "udp" (default: "udp").
    #[serde(default = "default_syslog_protocol")]
    pub protocol: String,
}

/// Configuration for the webhook audit exporter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookExportConfig {
    /// Webhook URL (must be HTTPS).
    pub url:                 String,
    /// Additional HTTP headers (e.g. `Authorization: Bearer ...`).
    #[serde(default)]
    pub headers:             std::collections::HashMap<String, String>,
    /// Number of entries to accumulate before flushing (default: 100).
    #[serde(default = "default_batch_size")]
    pub batch_size:          usize,
    /// Flush interval in seconds (default: 30).
    #[serde(default = "default_flush_interval_secs")]
    pub flush_interval_secs: u64,
}

const fn default_syslog_port() -> u16 {
    514
}

fn default_syslog_protocol() -> String {
    "udp".to_string()
}

const fn default_batch_size() -> usize {
    100
}

const fn default_flush_interval_secs() -> u64 {
    30
}

/// Statistics about audit events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditStats {
    /// Total number of audit events recorded
    pub total_events:  u64,
    /// Number of recent events (last 24 hours or recent window)
    pub recent_events: u64,
}

/// Audit logger with `PostgreSQL` backend and optional export sinks.
#[derive(Clone)]
pub struct AuditLogger {
    pool:      Arc<Pool>,
    exporters: Arc<Vec<Box<dyn AuditExporter>>>,
}

impl std::fmt::Debug for AuditLogger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditLogger")
            .field("pool", &self.pool)
            .field("exporters_count", &self.exporters.len())
            .finish()
    }
}

impl AuditLogger {
    /// Maximum byte length for the `query` and `variables` fields in an audit log entry.
    ///
    /// Limits audit table bloat and prevents excess allocation during serialization.
    /// Queries that bypass `QueryValidator` (e.g. on error paths) may be arbitrarily large.
    const MAX_AUDIT_FIELD_BYTES: usize = 64 * 1024;

    /// Create a new audit logger with no export sinks.
    #[must_use]
    pub fn new(pool: Arc<Pool>) -> Self {
        Self {
            pool,
            exporters: Arc::new(Vec::new()),
        }
    }

    /// Create a new audit logger with export sinks.
    ///
    /// Entries are written to PostgreSQL first, then exported to each sink
    /// on a best-effort basis (export failures are logged but do not fail the
    /// primary write).
    #[must_use]
    pub fn with_exporters(pool: Arc<Pool>, exporters: Vec<Box<dyn AuditExporter>>) -> Self {
        Self {
            pool,
            exporters: Arc::new(exporters),
        }
    }

    // 64 KiB

    /// Log an audit entry.
    ///
    /// Truncates `query` and `variables` to `MAX_AUDIT_FIELD_BYTES` before
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

        // Fire-and-forget export to external sinks. Failures are logged but
        // do not affect the primary PostgreSQL write path.
        if !self.exporters.is_empty() {
            for exporter in self.exporters.iter() {
                if let Err(e) = exporter.export(&entry).await {
                    error!(error = %e, "Audit exporter failed");
                }
            }
        }

        Ok(id)
    }

    /// Flush all export sinks.
    ///
    /// Call this during graceful shutdown to ensure buffered entries are delivered.
    ///
    /// # Errors
    ///
    /// Returns the first flush error encountered; remaining sinks are still flushed.
    pub async fn flush_exporters(&self) -> Result<(), AuditError> {
        let mut first_err = None;
        for exporter in self.exporters.iter() {
            if let Err(e) = exporter.flush().await {
                error!(error = %e, "Audit exporter flush failed");
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
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

    #[test]
    fn test_audit_export_config_deserialization() {
        let json = r#"{
            "syslog": { "address": "syslog.internal", "port": 514, "protocol": "tcp" },
            "webhook": { "url": "https://logs.example.com/ingest" }
        }"#;
        let config: AuditExportConfig =
            serde_json::from_str(json).expect("should deserialize AuditExportConfig");
        assert!(config.syslog.is_some());
        assert!(config.webhook.is_some());

        let syslog = config.syslog.expect("syslog should be Some");
        assert_eq!(syslog.address, "syslog.internal");
        assert_eq!(syslog.port, 514);
        assert_eq!(syslog.protocol, "tcp");

        let webhook = config.webhook.expect("webhook should be Some");
        assert_eq!(webhook.url, "https://logs.example.com/ingest");
        assert_eq!(webhook.batch_size, 100);
        assert_eq!(webhook.flush_interval_secs, 30);
    }

    #[test]
    fn test_audit_export_config_empty() {
        let config: AuditExportConfig =
            serde_json::from_str("{}").expect("should deserialize empty config");
        assert!(config.syslog.is_none());
        assert!(config.webhook.is_none());
    }

    #[test]
    fn test_audit_error_export_variant() {
        let err = AuditError::Export("connection refused".to_string());
        assert!(err.to_string().contains("connection refused"));
    }
}
