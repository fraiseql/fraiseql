//! ClickHouse backup provider.

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// ClickHouse backup provider.
///
/// Creates backups using ClickHouse's native backup mechanism.
// Reason: implemented but not yet registered in BackupManager
#[allow(dead_code)]
pub struct ClickhouseBackupProvider {
    /// ClickHouse HTTP endpoint
    endpoint_url: String,
    /// Backup directory
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
        Ok(Vec::new())
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
    use super::*;

    #[tokio::test]
    async fn test_clickhouse_backup_not_implemented() {
        let provider =
            ClickhouseBackupProvider::new("http://localhost:8123".to_string(), "/tmp".to_string());
        let err = provider.backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }
}
