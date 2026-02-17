//! Backup configuration and scheduling.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Backup configuration for a data store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Enable backups for this store
    pub enabled: bool,

    /// Backup schedule (cron expression)
    /// Examples: "0 * * * *" (hourly), "0 0 * * *" (daily)
    pub schedule: String,

    /// Maximum number of backups to retain
    pub retention_count: u32,

    /// Backup retention duration (delete older backups)
    pub retention_days: u32,

    /// Storage backend ('local', 's3', etc.)
    pub storage: String,

    /// Storage path or S3 bucket
    pub storage_path: String,

    /// Compression enabled (gzip, zstd)
    pub compression: Option<String>,

    /// Backup timeout
    pub timeout_secs: u64,

    /// Attempt to verify backup after creation
    pub verify_after_backup: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            enabled:             true,
            schedule:            "0 * * * *".to_string(), // Hourly
            retention_count:     24,                      // Keep 24 hourly backups
            retention_days:      30,
            storage:             "local".to_string(),
            storage_path:        "/var/backups/fraiseql".to_string(),
            compression:         Some("gzip".to_string()),
            timeout_secs:        600,
            verify_after_backup: true,
        }
    }
}

impl BackupConfig {
    /// Create config for PostgreSQL backups (hourly, high retention).
    pub fn postgres_default() -> Self {
        Self {
            enabled:             true,
            schedule:            "0 * * * *".to_string(), // Hourly
            retention_count:     24,
            retention_days:      30,
            storage:             "local".to_string(),
            storage_path:        "/var/backups/fraiseql/postgres".to_string(),
            compression:         Some("gzip".to_string()),
            timeout_secs:        1800,
            verify_after_backup: true,
        }
    }

    /// Create config for Redis backups (daily).
    pub fn redis_default() -> Self {
        Self {
            enabled:             true,
            schedule:            "0 0 * * *".to_string(), // Daily at midnight
            retention_count:     7,
            retention_days:      7,
            storage:             "local".to_string(),
            storage_path:        "/var/backups/fraiseql/redis".to_string(),
            compression:         Some("gzip".to_string()),
            timeout_secs:        600,
            verify_after_backup: false,
        }
    }

    /// Create config for ClickHouse backups (daily).
    pub fn clickhouse_default() -> Self {
        Self {
            enabled:             true,
            schedule:            "0 1 * * *".to_string(), // Daily at 1 AM
            retention_count:     7,
            retention_days:      7,
            storage:             "local".to_string(),
            storage_path:        "/var/backups/fraiseql/clickhouse".to_string(),
            compression:         None, // ClickHouse compression built-in
            timeout_secs:        3600,
            verify_after_backup: false,
        }
    }

    /// Create config for Elasticsearch backups (daily).
    pub fn elasticsearch_default() -> Self {
        Self {
            enabled:             true,
            schedule:            "0 2 * * *".to_string(), // Daily at 2 AM
            retention_count:     7,
            retention_days:      7,
            storage:             "local".to_string(),
            storage_path:        "/var/backups/fraiseql/elasticsearch".to_string(),
            compression:         None, // Elasticsearch snapshot built-in compression
            timeout_secs:        3600,
            verify_after_backup: true,
        }
    }

    /// Get timeout as Duration.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs)
    }
}

/// Backup status report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStatus {
    /// Name of the data store (postgres, redis, etc.)
    pub store_name: String,

    /// Whether backups are enabled
    pub enabled: bool,

    /// Last successful backup timestamp (Unix seconds)
    pub last_successful_backup: Option<i64>,

    /// Size of last backup in bytes
    pub last_backup_size: Option<u64>,

    /// Number of available backups
    pub available_backups: u32,

    /// Last error message (if any)
    pub last_error: Option<String>,

    /// Health status: "healthy", "warning", "error"
    pub status: String,
}

/// Recovery configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Data store to recover (postgres, redis, etc.)
    pub store_name: String,

    /// Backup timestamp to restore from (Unix seconds)
    pub backup_timestamp: i64,

    /// Verify data after recovery
    pub verify_after_recovery: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_postgres_default_config() {
        let config = BackupConfig::postgres_default();
        assert!(config.enabled);
        assert_eq!(config.schedule, "0 * * * *");
        assert_eq!(config.retention_count, 24);
    }

    #[test]
    fn test_redis_default_config() {
        let config = BackupConfig::redis_default();
        assert!(config.enabled);
        assert_eq!(config.schedule, "0 0 * * *");
        assert_eq!(config.retention_count, 7);
    }

    #[test]
    fn test_clickhouse_default_config() {
        let config = BackupConfig::clickhouse_default();
        assert!(config.enabled);
        assert_eq!(config.schedule, "0 1 * * *");
    }

    #[test]
    fn test_elasticsearch_default_config() {
        let config = BackupConfig::elasticsearch_default();
        assert!(config.enabled);
        assert_eq!(config.schedule, "0 2 * * *");
    }

    #[test]
    fn test_timeout_conversion() {
        let config = BackupConfig::postgres_default();
        let duration = config.timeout();
        assert_eq!(duration.as_secs(), 1800);
    }
}
