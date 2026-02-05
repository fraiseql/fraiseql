//! Backup and Disaster Recovery System
//!
//! Provides automated backups for all data stores with configurable schedules and retention
//! policies.
//!
//! # Architecture
//!
//! - **BackupManager**: Orchestrates backups across all databases
//! - **Database-specific providers**: PostgreSQL, Redis, ClickHouse, Elasticsearch
//! - **Storage backends**: Local filesystem, S3-compatible
//! - **Recovery utilities**: Restore from backups
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_server::backup::BackupManager;
//!
//! let manager = BackupManager::new(config).await?;
//! manager.start().await?;  // Starts background backup scheduler
//! ```

pub mod backup_config;
pub mod backup_manager;
pub mod backup_provider;
pub mod clickhouse_backup;
pub mod elasticsearch_backup;
pub mod postgres_backup;
pub mod recovery;
pub mod redis_backup;
pub mod storage;

pub use backup_config::BackupConfig;
pub use backup_manager::BackupManager;
pub use backup_provider::BackupProvider;
pub use recovery::{RecoveryChecklist, RecoveryStatus};
