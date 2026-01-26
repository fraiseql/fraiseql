//! Backup provider trait.

use serde::{Deserialize, Serialize};

/// Result type for backup operations.
pub type BackupResult<T> = Result<T, BackupError>;

/// Backup operation errors.
#[derive(Debug, Clone)]
pub enum BackupError {
    /// Connection failed
    ConnectionFailed { store: String, message: String },
    /// Backup failed
    BackupFailed { store: String, message: String },
    /// Restore failed
    RestoreFailed { store: String, message: String },
    /// Verification failed
    VerificationFailed { store: String, message: String },
    /// Storage error
    StorageError { message: String },
    /// Not found
    NotFound {
        store:     String,
        backup_id: String,
    },
    /// Timeout
    Timeout { store: String },
}

impl std::fmt::Display for BackupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed { store, message } => {
                write!(f, "Failed to connect to {}: {}", store, message)
            },
            Self::BackupFailed { store, message } => {
                write!(f, "Backup failed for {}: {}", store, message)
            },
            Self::RestoreFailed { store, message } => {
                write!(f, "Restore failed for {}: {}", store, message)
            },
            Self::VerificationFailed { store, message } => {
                write!(f, "Verification failed for {}: {}", store, message)
            },
            Self::StorageError { message } => write!(f, "Storage error: {}", message),
            Self::NotFound { store, backup_id } => {
                write!(f, "Backup not found for {}: {}", store, backup_id)
            },
            Self::Timeout { store } => write!(f, "Backup timeout for {}", store),
        }
    }
}

impl std::error::Error for BackupError {}

/// Backup information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Unique backup identifier
    pub backup_id: String,

    /// Data store name (postgres, redis, etc.)
    pub store_name: String,

    /// Backup timestamp (Unix seconds)
    pub timestamp: i64,

    /// Backup size in bytes
    pub size_bytes: u64,

    /// Whether backup is verified
    pub verified: bool,

    /// Compression algorithm (if any)
    pub compression: Option<String>,

    /// Additional metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl BackupInfo {
    /// Get human-readable timestamp.
    pub fn timestamp_display(&self) -> String {
        let secs = self.timestamp;
        let duration = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64);
        match duration.elapsed() {
            Ok(_) => format_timestamp(secs),
            Err(_) => format_timestamp(secs),
        }
    }

    /// Get human-readable size.
    pub fn size_display(&self) -> String {
        format_size_bytes(self.size_bytes)
    }
}

/// Backup provider trait for each data store.
#[async_trait::async_trait]
pub trait BackupProvider: Send + Sync {
    /// Get the name of this provider (e.g., "postgres", "redis").
    fn name(&self) -> &str;

    /// Check if provider is healthy and connected.
    async fn health_check(&self) -> BackupResult<()>;

    /// Create a new backup.
    ///
    /// Returns backup info on success.
    async fn backup(&self) -> BackupResult<BackupInfo>;

    /// Restore from a backup.
    ///
    /// # Arguments
    /// * `backup_id` - The backup to restore from
    /// * `verify` - Whether to verify after restore
    async fn restore(&self, backup_id: &str, verify: bool) -> BackupResult<()>;

    /// List all available backups.
    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>>;

    /// Get a specific backup by ID.
    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo>;

    /// Delete a backup.
    async fn delete_backup(&self, backup_id: &str) -> BackupResult<()>;

    /// Verify a backup is restorable.
    async fn verify_backup(&self, backup_id: &str) -> BackupResult<()>;

    /// Get storage usage.
    async fn get_storage_usage(&self) -> BackupResult<StorageUsage>;
}

/// Storage usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsage {
    /// Total backup size in bytes
    pub total_bytes: u64,

    /// Number of backups
    pub backup_count: u32,

    /// Oldest backup timestamp (Unix seconds)
    pub oldest_backup_timestamp: Option<i64>,

    /// Newest backup timestamp (Unix seconds)
    pub newest_backup_timestamp: Option<i64>,
}

// Helper functions

fn format_timestamp(secs: i64) -> String {
    // Simple formatting - in production would use chrono or similar
    format!("{}", secs)
}

fn format_size_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size_bytes(100), "100.00 B");
        assert_eq!(format_size_bytes(1024), "1.00 KB");
        assert_eq!(format_size_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_size_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_backup_info_display() {
        let info = BackupInfo {
            backup_id:   "backup-123".to_string(),
            store_name:  "postgres".to_string(),
            timestamp:   1000000,
            size_bytes:  1024 * 1024,
            verified:    true,
            compression: Some("gzip".to_string()),
            metadata:    Default::default(),
        };

        assert_eq!(info.size_display(), "1.00 MB");
        assert!(!info.timestamp_display().is_empty());
    }

    #[test]
    fn test_backup_error_display() {
        let err = BackupError::BackupFailed {
            store:   "postgres".to_string(),
            message: "Connection timeout".to_string(),
        };
        assert!(err.to_string().contains("postgres"));
        assert!(err.to_string().contains("Connection timeout"));
    }
}
