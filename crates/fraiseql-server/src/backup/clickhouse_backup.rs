//! ClickHouse backup provider.

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// ClickHouse backup provider.
///
/// Creates backups using ClickHouse's native backup mechanism.
///
/// # Note
///
/// All operations currently return `BackupError::NotImplemented`.
/// Register this provider with `BackupManager::register_provider` when real
/// ClickHouse backup support is added.
pub struct ClickhouseBackupProvider {
    /// ClickHouse HTTP endpoint
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
    endpoint_url: String,
    /// Backup directory
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
    backup_dir:   String,
}

impl ClickhouseBackupProvider {
    /// Create new ClickHouse backup provider.
    pub fn new(endpoint_url: String, backup_dir: String) -> Self {
        Self {
            endpoint_url,
            backup_dir,
        }
    }
}

#[async_trait::async_trait]
impl BackupProvider for ClickhouseBackupProvider {
    fn name(&self) -> &'static str {
        "clickhouse"
    }

    async fn health_check(&self) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "health_check".to_string(),
        })
    }

    async fn backup(&self) -> BackupResult<BackupInfo> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "backup".to_string(),
        })
    }

    async fn restore(&self, _backup_id: &str, _verify: bool) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "restore".to_string(),
        })
    }

    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "list_backups".to_string(),
        })
    }

    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        Err(BackupError::NotFound {
            store:     "clickhouse".to_string(),
            backup_id: backup_id.to_string(),
        })
    }

    async fn delete_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "delete_backup".to_string(),
        })
    }

    async fn verify_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "clickhouse".to_string(),
            operation: "verify_backup".to_string(),
        })
    }

    async fn get_storage_usage(&self) -> BackupResult<StorageUsage> {
        Ok(StorageUsage {
            total_bytes:             0,
            backup_count:            0,
            oldest_backup_timestamp: None,
            newest_backup_timestamp: None,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;

    #[tokio::test]
    async fn test_clickhouse_backup_not_implemented() {
        let provider =
            ClickhouseBackupProvider::new("http://localhost:8123".to_string(), "/tmp".to_string());
        let err = provider.backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }
}
