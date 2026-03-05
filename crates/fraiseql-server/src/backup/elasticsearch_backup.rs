//! Elasticsearch backup provider.

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// Elasticsearch backup provider.
///
/// Uses Elasticsearch snapshot/restore API.
///
/// # Note
///
/// All operations currently return `BackupError::NotImplemented`.
/// Register this provider with `BackupManager::register_provider` when real
/// Elasticsearch backup support is added.
pub struct ElasticsearchBackupProvider {
    /// Elasticsearch endpoint URL
    endpoint_url: String,
    /// Backup repository name
    repository:   String,
}

impl ElasticsearchBackupProvider {
    /// Create new Elasticsearch backup provider.
    pub fn new(endpoint_url: String, repository: String) -> Self {
        Self {
            endpoint_url,
            repository,
        }
    }
}

#[async_trait::async_trait]
impl BackupProvider for ElasticsearchBackupProvider {
    fn name(&self) -> &'static str {
        "elasticsearch"
    }

    async fn health_check(&self) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "health_check".to_string(),
        })
    }

    async fn backup(&self) -> BackupResult<BackupInfo> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "backup".to_string(),
        })
    }

    async fn restore(&self, _backup_id: &str, _verify: bool) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "restore".to_string(),
        })
    }

    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "list_backups".to_string(),
        })
    }

    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        Err(BackupError::NotFound {
            store:     "elasticsearch".to_string(),
            backup_id: backup_id.to_string(),
        })
    }

    async fn delete_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "delete_backup".to_string(),
        })
    }

    async fn verify_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
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
    async fn test_elasticsearch_backup_not_implemented() {
        let provider = ElasticsearchBackupProvider::new(
            "http://localhost:9200".to_string(),
            "default".to_string(),
        );
        let err = provider.backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }
}
