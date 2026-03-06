//! Backup manager orchestrating all backup providers.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use super::{
    backup_config::{BackupConfig, BackupStatus},
    backup_provider::{BackupError, BackupProvider, BackupResult},
    clickhouse_backup::ClickhouseBackupProvider,
    elasticsearch_backup::ElasticsearchBackupProvider,
    postgres_backup::PostgresBackupProvider,
    redis_backup::RedisBackupProvider,
};

/// Manages backups across all data stores.
pub struct BackupManager {
    /// Registered backup providers
    providers: Arc<RwLock<HashMap<String, Arc<dyn BackupProvider>>>>,

    /// Backup status cache
    status_cache: Arc<RwLock<HashMap<String, BackupStatus>>>,

    /// Backup configs
    configs: HashMap<String, BackupConfig>,
}

impl BackupManager {
    /// Create new backup manager.
    pub fn new(configs: HashMap<String, BackupConfig>) -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            status_cache: Arc::new(RwLock::new(HashMap::new())),
            configs,
        }
    }

    /// Register a backup provider and seed its initial status entry.
    pub async fn register_provider(
        &self,
        name: String,
        provider: Arc<dyn BackupProvider>,
    ) -> Result<(), String> {
        let mut providers = self.providers.write().await;

        if providers.contains_key(&name) {
            return Err(format!("Provider '{}' already registered", name));
        }

        providers.insert(name.clone(), provider);

        // Seed an initial status so get_status() lists all registered providers
        // even before the first backup has run.
        let enabled = self.configs.get(&name).map(|c| c.enabled).unwrap_or(false);
        let mut cache = self.status_cache.write().await;
        cache.entry(name.clone()).or_insert_with(|| BackupStatus {
            store_name:             name,
            enabled,
            last_successful_backup: None,
            last_backup_size:       None,
            available_backups:      0,
            last_error:             None,
            status:                 "registered".to_string(),
        });

        Ok(())
    }

    /// Start backup scheduler and register all known providers.
    ///
    /// Only providers whose [`BackupProvider::is_implemented`] returns `true`
    /// are registered.  Stub providers (all operations return `NotImplemented`)
    /// are skipped and logged at DEBUG level so they never appear in health
    /// checks or produce user-visible errors.
    pub async fn start(&self) -> Result<(), String> {
        let providers: &[(&str, Arc<dyn BackupProvider>)] = &[
            (
                "postgres",
                Arc::new(PostgresBackupProvider::new(String::new(), String::new())),
            ),
            (
                "redis",
                Arc::new(RedisBackupProvider::new(String::new(), String::new())),
            ),
            (
                "clickhouse",
                Arc::new(ClickhouseBackupProvider::new(String::new(), String::new())),
            ),
            (
                "elasticsearch",
                Arc::new(ElasticsearchBackupProvider::new(String::new(), String::new())),
            ),
        ];

        for (name, provider) in providers {
            if !provider.is_implemented() {
                tracing::debug!(
                    provider = %name,
                    "Backup provider skipped — not yet implemented"
                );
                continue;
            }

            // register_provider is idempotent — skip if already registered.
            let result = self.register_provider((*name).to_string(), Arc::clone(provider)).await;
            if result.is_ok() {
                tracing::info!(provider = %name, "Backup provider registered");
            }
        }

        Ok(())
    }

    /// Create a backup for a specific provider.
    pub async fn backup(&self, provider_name: &str) -> BackupResult<()> {
        let providers = self.providers.read().await;

        let provider = providers.get(provider_name).ok_or_else(|| BackupError::BackupFailed {
            store:   provider_name.to_string(),
            message: "Provider not registered".to_string(),
        })?;

        let config = self.configs.get(provider_name).ok_or_else(|| BackupError::BackupFailed {
            store:   provider_name.to_string(),
            message: "No configuration found".to_string(),
        })?;

        if !config.enabled {
            return Err(BackupError::BackupFailed {
                store:   provider_name.to_string(),
                message: "Backups disabled".to_string(),
            });
        }

        // Execute backup with timeout
        let backup_future = provider.backup();
        let timeout_duration = config.timeout();

        let result = tokio::time::timeout(timeout_duration, backup_future).await.map_err(|_| {
            BackupError::Timeout {
                store: provider_name.to_string(),
            }
        })?;

        match result {
            Ok(backup_info) => {
                // Update status cache
                let mut cache = self.status_cache.write().await;
                cache.insert(
                    provider_name.to_string(),
                    BackupStatus {
                        store_name:             provider_name.to_string(),
                        enabled:                config.enabled,
                        last_successful_backup: Some(backup_info.timestamp),
                        last_backup_size:       Some(backup_info.size_bytes),
                        available_backups:      1, // Would count in production
                        last_error:             None,
                        status:                 "healthy".to_string(),
                    },
                );
                Ok(())
            },
            Err(e) => {
                // Update status cache with error
                let mut cache = self.status_cache.write().await;
                cache.insert(
                    provider_name.to_string(),
                    BackupStatus {
                        store_name:             provider_name.to_string(),
                        enabled:                config.enabled,
                        last_successful_backup: None,
                        last_backup_size:       None,
                        available_backups:      0,
                        last_error:             Some(e.to_string()),
                        status:                 "error".to_string(),
                    },
                );
                Err(e)
            },
        }
    }

    /// Get backup status for all providers.
    pub async fn get_status(&self) -> HashMap<String, BackupStatus> {
        self.status_cache.read().await.clone()
    }

    /// Get backup status for a specific provider.
    pub async fn get_provider_status(&self, provider_name: &str) -> Option<BackupStatus> {
        self.status_cache.read().await.get(provider_name).cloned()
    }

    /// Restore from a backup.
    pub async fn restore(&self, provider_name: &str, backup_id: &str) -> BackupResult<()> {
        let providers = self.providers.read().await;

        let provider = providers.get(provider_name).ok_or_else(|| BackupError::RestoreFailed {
            store:   provider_name.to_string(),
            message: "Provider not registered".to_string(),
        })?;

        let config = self.configs.get(provider_name).ok_or_else(|| BackupError::RestoreFailed {
            store:   provider_name.to_string(),
            message: "No configuration found".to_string(),
        })?;

        provider.restore(backup_id, config.verify_after_backup).await
    }

    /// List backups for a provider.
    pub async fn list_backups(&self, provider_name: &str) -> BackupResult<Vec<String>> {
        let providers = self.providers.read().await;

        let provider = providers.get(provider_name).ok_or_else(|| BackupError::BackupFailed {
            store:   provider_name.to_string(),
            message: "Provider not registered".to_string(),
        })?;

        let backups = provider.list_backups().await?;
        Ok(backups.iter().map(|b| b.backup_id.clone()).collect())
    }
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::backup_provider::{BackupInfo, StorageUsage};

    /// Mock backup provider for testing
    struct MockBackupProvider {
        name: String,
    }

    #[async_trait::async_trait]
    impl BackupProvider for MockBackupProvider {
        fn name(&self) -> &str {
            &self.name
        }

        async fn health_check(&self) -> BackupResult<()> {
            Ok(())
        }

        async fn backup(&self) -> BackupResult<BackupInfo> {
            Ok(BackupInfo {
                backup_id:   format!("{}-backup-1", self.name),
                store_name:  self.name.clone(),
                timestamp:   1_000_000,
                size_bytes:  1024 * 1024,
                verified:    true,
                compression: Some("gzip".to_string()),
                metadata:    Default::default(),
            })
        }

        async fn restore(&self, _backup_id: &str, _verify: bool) -> BackupResult<()> {
            Ok(())
        }

        async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
            Ok(vec![BackupInfo {
                backup_id:   format!("{}-backup-1", self.name),
                store_name:  self.name.clone(),
                timestamp:   1_000_000,
                size_bytes:  1024 * 1024,
                verified:    true,
                compression: Some("gzip".to_string()),
                metadata:    Default::default(),
            }])
        }

        async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
            Ok(BackupInfo {
                backup_id:   backup_id.to_string(),
                store_name:  self.name.clone(),
                timestamp:   1_000_000,
                size_bytes:  1024 * 1024,
                verified:    true,
                compression: Some("gzip".to_string()),
                metadata:    Default::default(),
            })
        }

        async fn delete_backup(&self, _backup_id: &str) -> BackupResult<()> {
            Ok(())
        }

        async fn verify_backup(&self, _backup_id: &str) -> BackupResult<()> {
            Ok(())
        }

        async fn get_storage_usage(&self) -> BackupResult<StorageUsage> {
            Ok(StorageUsage {
                total_bytes:             1024 * 1024 * 100,
                backup_count:            7,
                oldest_backup_timestamp: Some(999_999),
                newest_backup_timestamp: Some(1_000_000),
            })
        }
    }

    #[tokio::test]
    async fn test_register_provider() {
        let configs = HashMap::new();
        let manager = BackupManager::new(configs);

        let provider = Arc::new(MockBackupProvider {
            name: "postgres".to_string(),
        });

        assert!(manager.register_provider("postgres".to_string(), provider).await.is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_provider() {
        let configs = HashMap::new();
        let manager = BackupManager::new(configs);

        let provider = Arc::new(MockBackupProvider {
            name: "postgres".to_string(),
        });

        manager.register_provider("postgres".to_string(), provider.clone()).await.ok();
        let result = manager.register_provider("postgres".to_string(), provider).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_backup_updates_status() {
        let mut configs = HashMap::new();
        configs.insert("postgres".to_string(), BackupConfig::postgres_default());

        let manager = BackupManager::new(configs);

        let provider = Arc::new(MockBackupProvider {
            name: "postgres".to_string(),
        });

        manager.register_provider("postgres".to_string(), provider).await.unwrap();

        manager.backup("postgres").await.unwrap();

        let status = manager.get_provider_status("postgres").await;
        assert!(status.is_some());
        assert_eq!(status.unwrap().status, "healthy");
    }

    #[tokio::test]
    async fn test_list_backups() {
        let configs = HashMap::new();
        let manager = BackupManager::new(configs);

        let provider = Arc::new(MockBackupProvider {
            name: "postgres".to_string(),
        });

        manager.register_provider("postgres".to_string(), provider).await.unwrap();

        let backups = manager.list_backups("postgres").await.unwrap();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].contains("backup-1"));
    }

    /// A mock provider that reports itself as implemented.
    struct ImplementedMockProvider {
        name: String,
    }

    #[async_trait::async_trait]
    impl BackupProvider for ImplementedMockProvider {
        fn name(&self) -> &str { &self.name }

        fn is_implemented(&self) -> bool { true }

        async fn health_check(&self) -> BackupResult<()> { Ok(()) }

        async fn backup(&self) -> BackupResult<BackupInfo> {
            Ok(BackupInfo {
                backup_id:   format!("{}-backup-1", self.name),
                store_name:  self.name.clone(),
                timestamp:   1_000_000,
                size_bytes:  1024,
                verified:    true,
                compression: None,
                metadata:    Default::default(),
            })
        }

        async fn restore(&self, _: &str, _: bool) -> BackupResult<()> { Ok(()) }
        async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> { Ok(vec![]) }
        async fn get_backup(&self, id: &str) -> BackupResult<BackupInfo> {
            Ok(BackupInfo {
                backup_id:   id.to_string(),
                store_name:  self.name.clone(),
                timestamp:   1_000_000,
                size_bytes:  1024,
                verified:    true,
                compression: None,
                metadata:    Default::default(),
            })
        }
        async fn delete_backup(&self, _: &str) -> BackupResult<()> { Ok(()) }
        async fn verify_backup(&self, _: &str) -> BackupResult<()> { Ok(()) }
        async fn get_storage_usage(&self) -> BackupResult<StorageUsage> {
            Ok(StorageUsage {
                total_bytes:             0,
                backup_count:            0,
                oldest_backup_timestamp: None,
                newest_backup_timestamp: None,
            })
        }
    }

    // =========================================================================
    // Provider registration tests
    // =========================================================================

    #[tokio::test]
    async fn test_start_skips_unimplemented_providers() {
        // start() must skip all four built-in stubs because none have
        // is_implemented() = true.
        let manager = BackupManager::new(HashMap::new());
        manager.start().await.expect("start must not fail");
        assert_eq!(
            manager.get_status().await.len(),
            0,
            "no unimplemented providers should appear in status"
        );
    }

    #[tokio::test]
    async fn test_start_is_idempotent() {
        let manager = BackupManager::new(HashMap::new());
        manager.start().await.unwrap();
        // Calling start() a second time must not panic or error.
        manager.start().await.unwrap();
        assert_eq!(manager.get_status().await.len(), 0);
    }

    #[tokio::test]
    async fn test_implemented_provider_gets_registered() {
        let manager = BackupManager::new(HashMap::new());
        let provider = Arc::new(ImplementedMockProvider {
            name: "custom".to_string(),
        });
        manager.register_provider("custom".to_string(), provider).await.unwrap();
        assert!(manager.get_status().await.contains_key("custom"));
    }

    #[tokio::test]
    async fn test_register_provider_seeds_initial_status() {
        let manager = BackupManager::new(HashMap::new());
        let provider = Arc::new(MockBackupProvider {
            name: "redis".to_string(),
        });
        manager.register_provider("redis".to_string(), provider).await.unwrap();

        let status = manager.get_status().await;
        assert!(status.contains_key("redis"));
        assert_eq!(status["redis"].store_name, "redis");
        assert_eq!(status["redis"].status, "registered");
    }
}
