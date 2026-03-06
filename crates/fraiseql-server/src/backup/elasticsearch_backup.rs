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
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
    endpoint_url: String,
    /// Backup repository name
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
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

    async fn get_backup(&self, _backup_id: &str) -> BackupResult<BackupInfo> {
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "get_backup".to_string(),
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
        Err(BackupError::NotImplemented {
            store:     "elasticsearch".to_string(),
            operation: "get_storage_usage".to_string(),
        })
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> ElasticsearchBackupProvider {
        ElasticsearchBackupProvider::new(
            "http://localhost:9200".to_string(),
            "default".to_string(),
        )
    }

    #[tokio::test]
    async fn test_elasticsearch_backup_not_implemented() {
        let err = provider().backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { store, .. } if store == "elasticsearch"));
    }

    #[tokio::test]
    async fn test_elasticsearch_restore_not_implemented() {
        let err = provider().restore("snap-1", false).await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_health_check_not_implemented() {
        let err = provider().health_check().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_list_backups_not_implemented() {
        let err = provider().list_backups().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_get_backup_not_implemented() {
        let err = provider().get_backup("snap-1").await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_delete_backup_not_implemented() {
        let err = provider().delete_backup("snap-1").await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_verify_backup_not_implemented() {
        let err = provider().verify_backup("snap-1").await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_elasticsearch_get_storage_usage_not_implemented() {
        let err = provider().get_storage_usage().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[test]
    fn test_elasticsearch_name() {
        assert_eq!(provider().name(), "elasticsearch");
    }
}
