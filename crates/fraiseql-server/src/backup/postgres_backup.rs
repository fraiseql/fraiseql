//! PostgreSQL backup provider.

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// PostgreSQL backup provider.
///
/// Uses pg_dump for logical backups and WAL archiving for point-in-time recovery.
pub struct PostgresBackupProvider {
    /// PostgreSQL connection URL
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
    connection_url: String,

    /// Base backup directory
    // Reason: stub field reserved for future implementation
    #[allow(dead_code)]
    backup_dir: String,
}

impl PostgresBackupProvider {
    /// Create new PostgreSQL backup provider.
    pub fn new(connection_url: String, backup_dir: String) -> Self {
        Self {
            connection_url,
            backup_dir,
        }
    }

    /// Generate backup ID with timestamp.
    // Reason: stub method reserved for future implementation
    #[allow(dead_code)]
    fn generate_backup_id() -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("postgres-{}", timestamp)
    }
}

#[async_trait::async_trait]
impl BackupProvider for PostgresBackupProvider {
    fn name(&self) -> &'static str {
        "postgres"
    }

    async fn health_check(&self) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
            operation: "health_check".to_string(),
        })
    }

    async fn backup(&self) -> BackupResult<BackupInfo> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
            operation: "backup".to_string(),
        })
    }

    async fn restore(&self, _backup_id: &str, _verify: bool) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
            operation: "restore".to_string(),
        })
    }

    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
            operation: "list_backups".to_string(),
        })
    }

    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        Err(BackupError::NotFound {
            store:     "postgres".to_string(),
            backup_id: backup_id.to_string(),
        })
    }

    async fn delete_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
            operation: "delete_backup".to_string(),
        })
    }

    async fn verify_backup(&self, _backup_id: &str) -> BackupResult<()> {
        Err(BackupError::NotImplemented {
            store:     "postgres".to_string(),
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

    #[test]
    fn test_backup_id_generation() {
        let id1 = PostgresBackupProvider::generate_backup_id();

        assert!(id1.starts_with("postgres-"));
        assert!(id1.len() > "postgres-".len());

        // Check format: postgres-<timestamp>
        let parts: Vec<&str> = id1.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert!(parts[1].parse::<u64>().is_ok()); // Second part should be valid timestamp
    }

    #[tokio::test]
    async fn test_health_check_not_implemented() {
        let provider = PostgresBackupProvider::new(
            "postgresql://localhost/test".to_string(),
            "/tmp/backups".to_string(),
        );
        let err = provider.health_check().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }

    #[tokio::test]
    async fn test_backup_not_implemented() {
        let provider = PostgresBackupProvider::new(
            "postgresql://localhost/test".to_string(),
            "/tmp/backups".to_string(),
        );
        let err = provider.backup().await.unwrap_err();
        assert!(matches!(err, BackupError::NotImplemented { .. }));
    }
}
