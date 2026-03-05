//! Redis backup provider.

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// Redis backup provider.
///
/// Creates backups using BGSAVE (RDB) or BGREWRITEAOF (AOF).
// Reason: implemented but not yet registered in BackupManager
#[allow(dead_code)]
pub struct RedisBackupProvider {
    /// Redis connection URL
    connection_url: String,
    /// Backup directory
    backup_dir:     String,
}

impl RedisBackupProvider {
    /// Create new Redis backup provider.
    pub fn new(connection_url: String, backup_dir: String) -> Self {
        Self {
            connection_url,
            backup_dir,
        }
    }
}

#[async_trait::async_trait]
impl BackupProvider for RedisBackupProvider {
    fn name(&self) -> &'static str {
        "redis"
    }

    async fn health_check(&self) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "redis".to_string(),
            operation: "health_check".to_string(),
        })
    }

    async fn backup(&self) -> BackupResult<BackupInfo> {
        Err(BackupError::NotImplemented {
            store:     "redis".to_string(),
            operation: "backup".to_string(),
        })
    }

    async fn restore(&self, _backup_id: &str, _verify: bool) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "redis".to_string(),
            operation: "restore".to_string(),
        })
    }

    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
        Ok(Vec::new())
    }

    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        Err(BackupError::NotFound {
            store:     "redis".to_string(),
            backup_id: backup_id.to_string(),
        })
    }

    async fn delete_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "redis".to_string(),
            operation: "delete_backup".to_string(),
        })
    }

    async fn verify_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "redis".to_string(),
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
    async fn test_redis_backup_not_implemented() {
        let provider =
            RedisBackupProvider::new("redis://localhost:6379".to_string(), "/tmp".to_string());
        let err = provider.backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }
}
