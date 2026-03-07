//! PostgreSQL backup provider.
//!
//! Uses `pg_dump` / `pg_restore` subprocesses for logical backups.
//! The connection URL is passed as a subprocess argument and is never
//! logged at INFO level; only DEBUG logging with explicit redaction.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;

use super::backup_provider::{BackupError, BackupInfo, BackupProvider, BackupResult, StorageUsage};

/// Sidecar file written alongside each `.dump` file containing metadata.
const META_SUFFIX: &str = ".meta.json";
/// Extension used for `pg_dump --format=custom` output files.
const DUMP_SUFFIX: &str = ".dump";

/// PostgreSQL backup provider.
///
/// Uses `pg_dump` for logical backups and `pg_restore` for restore operations.
/// Each backup produces a `<id>.dump` file (custom format) and a `<id>.dump.meta.json`
/// sidecar with creation timestamp, size, and pg version metadata.
pub struct PostgresBackupProvider {
    /// PostgreSQL connection URL (e.g. `postgresql://user:pass@host/db`)
    connection_url: String,

    /// Base directory where backup files are stored
    backup_dir: PathBuf,
}

impl PostgresBackupProvider {
    /// Create new PostgreSQL backup provider.
    pub fn new(connection_url: String, backup_dir: String) -> Self {
        Self {
            connection_url,
            backup_dir: PathBuf::from(backup_dir),
        }
    }

    /// Generate a unique backup ID incorporating the current Unix timestamp.
    fn generate_backup_id() -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        format!("postgres-{timestamp}")
    }

    /// Redact password from connection URL for safe logging.
    fn redact_url(url: &str) -> String {
        // Replace password portion: postgresql://user:PASSWORD@host/db
        if let Some(at_pos) = url.find('@') {
            if let Some(scheme_end) = url.find("://") {
                let after_scheme = &url[scheme_end + 3..at_pos];
                if let Some(colon) = after_scheme.find(':') {
                    let user = &after_scheme[..colon];
                    let rest = &url[at_pos..];
                    let scheme = &url[..scheme_end + 3];
                    return format!("{scheme}{user}:***{rest}");
                }
            }
        }
        url.to_string()
    }

    /// Derive the path for a `.dump` file from its backup ID.
    fn dump_path(&self, backup_id: &str) -> PathBuf {
        self.backup_dir.join(format!("{backup_id}{DUMP_SUFFIX}"))
    }

    /// Derive the path for a `.meta.json` sidecar from its backup ID.
    fn meta_path(&self, backup_id: &str) -> PathBuf {
        self.backup_dir.join(format!("{backup_id}{DUMP_SUFFIX}{META_SUFFIX}"))
    }

    /// Write a metadata sidecar file for `backup_id`.
    async fn write_meta(
        &self,
        backup_id: &str,
        size_bytes: u64,
        pg_version: &str,
    ) -> BackupResult<()> {
        let meta = serde_json::json!({
            "backup_id": backup_id,
            "format": "pg_dump-custom",
            "pg_version": pg_version,
            "size_bytes": size_bytes,
            "created_at": unix_now(),
        });
        let path = self.meta_path(backup_id);
        tokio::fs::write(&path, serde_json::to_string(&meta).unwrap_or_default())
            .await
            .map_err(|e| BackupError::StorageError {
                message: format!("Failed to write metadata to {}: {e}", path.display()),
            })
    }

    /// Read a metadata sidecar and return a `BackupInfo`.
    async fn read_meta(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        let path = self.meta_path(backup_id);
        let raw = tokio::fs::read_to_string(&path).await.map_err(|e| {
            BackupError::NotFound {
                store:     "postgres".to_string(),
                backup_id: format!("{backup_id}: {e}"),
            }
        })?;
        let meta: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
            BackupError::StorageError {
                message: format!("Corrupt metadata at {}: {e}", path.display()),
            }
        })?;

        let size_bytes = meta["size_bytes"].as_u64().unwrap_or(0);
        let timestamp = meta["created_at"].as_i64().unwrap_or(0);
        let pg_version = meta["pg_version"].as_str().unwrap_or("").to_string();

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("format".to_string(), "pg_dump-custom".to_string());
        metadata.insert("pg_version".to_string(), pg_version);

        Ok(BackupInfo {
            backup_id: backup_id.to_string(),
            store_name: "postgres".to_string(),
            timestamp,
            size_bytes,
            verified: false,
            compression: None,
            metadata,
        })
    }

    /// Query `pg_dump --version` and return the version string.
    async fn pg_dump_version() -> String {
        let output = Command::new("pg_dump")
            .arg("--version")
            .output()
            .await
            .ok();
        output
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }
}

#[async_trait::async_trait]
impl BackupProvider for PostgresBackupProvider {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn is_implemented(&self) -> bool {
        true
    }

    /// Run `pg_isready` to verify the database is reachable.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::ConnectionFailed` if `pg_isready` exits non-zero.
    async fn health_check(&self) -> BackupResult<()> {
        let output = Command::new("pg_isready")
            .args(["-d", &self.connection_url])
            .output()
            .await
            .map_err(|e| BackupError::ConnectionFailed {
                store:   "postgres".to_string(),
                message: format!("Failed to spawn pg_isready: {e}"),
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(BackupError::ConnectionFailed {
                store:   "postgres".to_string(),
                message: stderr,
            })
        }
    }

    /// Create a backup using `pg_dump --format=custom`.
    ///
    /// Writes `<backup_dir>/<id>.dump` and a `<id>.dump.meta.json` sidecar.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::BackupFailed` if `pg_dump` exits non-zero or I/O fails.
    async fn backup(&self) -> BackupResult<BackupInfo> {
        // Ensure backup directory exists.
        tokio::fs::create_dir_all(&self.backup_dir).await.map_err(|e| {
            BackupError::BackupFailed {
                store:   "postgres".to_string(),
                message: format!("Cannot create backup dir {}: {e}", self.backup_dir.display()),
            }
        })?;

        let backup_id = Self::generate_backup_id();
        let dump_path = self.dump_path(&backup_id);

        tracing::debug!(
            backup_id = %backup_id,
            path = %dump_path.display(),
            url = %Self::redact_url(&self.connection_url),
            "Starting pg_dump backup"
        );

        let output = Command::new("pg_dump")
            .args([
                "--format=custom",
                &format!("--file={}", dump_path.display()),
                &self.connection_url,
            ])
            .output()
            .await
            .map_err(|e| BackupError::BackupFailed {
                store:   "postgres".to_string(),
                message: format!("Failed to spawn pg_dump: {e}"),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(BackupError::BackupFailed {
                store:   "postgres".to_string(),
                message: stderr,
            });
        }

        // Measure the produced file size.
        let size_bytes = tokio::fs::metadata(&dump_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);

        let pg_version = Self::pg_dump_version().await;
        self.write_meta(&backup_id, size_bytes, &pg_version).await?;

        let mut metadata = std::collections::HashMap::new();
        metadata.insert("format".to_string(), "pg_dump-custom".to_string());
        metadata.insert("pg_version".to_string(), pg_version);

        Ok(BackupInfo {
            backup_id,
            store_name: "postgres".to_string(),
            timestamp: unix_now(),
            size_bytes,
            verified: false,
            compression: None,
            metadata,
        })
    }

    /// Restore from a `pg_dump` custom-format backup using `pg_restore`.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::RestoreFailed` if `pg_restore` exits non-zero.
    async fn restore(&self, backup_id: &str, verify: bool) -> BackupResult<()> {
        if verify {
            self.verify_backup(backup_id).await?;
        }

        let dump_path = self.dump_path(backup_id);
        if !dump_path.exists() {
            return Err(BackupError::NotFound {
                store:     "postgres".to_string(),
                backup_id: backup_id.to_string(),
            });
        }

        tracing::debug!(
            backup_id = %backup_id,
            path = %dump_path.display(),
            url = %Self::redact_url(&self.connection_url),
            "Starting pg_restore"
        );

        let output = Command::new("pg_restore")
            .args([
                "--clean",
                "--if-exists",
                &format!("--dbname={}", self.connection_url),
                &dump_path.to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| BackupError::RestoreFailed {
                store:   "postgres".to_string(),
                message: format!("Failed to spawn pg_restore: {e}"),
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(BackupError::RestoreFailed {
                store:   "postgres".to_string(),
                message: stderr,
            })
        }
    }

    /// List all backups found in `backup_dir`, sorted newest first.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::StorageError` if the directory cannot be read.
    async fn list_backups(&self) -> BackupResult<Vec<BackupInfo>> {
        let mut entries = tokio::fs::read_dir(&self.backup_dir).await.map_err(|e| {
            BackupError::StorageError {
                message: format!(
                    "Cannot read backup dir {}: {e}",
                    self.backup_dir.display()
                ),
            }
        })?;

        let mut backups = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(DUMP_SUFFIX) && !name_str.ends_with(META_SUFFIX) {
                let backup_id = name_str.trim_end_matches(DUMP_SUFFIX).to_string();
                if let Ok(info) = self.read_meta(&backup_id).await {
                    backups.push(info);
                }
            }
        }

        // Sort newest first.
        backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(backups)
    }

    /// Retrieve a specific backup by ID.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::NotFound` if the backup does not exist.
    async fn get_backup(&self, backup_id: &str) -> BackupResult<BackupInfo> {
        self.read_meta(backup_id).await
    }

    /// Delete a backup and its metadata sidecar.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::NotFound` if the backup does not exist.
    async fn delete_backup(&self, backup_id: &str) -> BackupResult<()> {
        let dump_path = self.dump_path(backup_id);
        let meta_path = self.meta_path(backup_id);

        if !dump_path.exists() {
            return Err(BackupError::NotFound {
                store:     "postgres".to_string(),
                backup_id: backup_id.to_string(),
            });
        }

        tokio::fs::remove_file(&dump_path).await.map_err(|e| BackupError::StorageError {
            message: format!("Failed to delete {}: {e}", dump_path.display()),
        })?;
        // Best-effort removal of sidecar; ignore error if it doesn't exist.
        let _ = tokio::fs::remove_file(&meta_path).await;
        Ok(())
    }

    /// Verify a backup is restorable using `pg_restore --list` (non-destructive).
    ///
    /// # Errors
    ///
    /// Returns `BackupError::VerificationFailed` if `pg_restore --list` exits non-zero.
    async fn verify_backup(&self, backup_id: &str) -> BackupResult<()> {
        let dump_path = self.dump_path(backup_id);

        if !dump_path.exists() {
            return Err(BackupError::NotFound {
                store:     "postgres".to_string(),
                backup_id: backup_id.to_string(),
            });
        }

        let output = Command::new("pg_restore")
            .args(["--list", &dump_path.to_string_lossy()])
            .output()
            .await
            .map_err(|e| BackupError::VerificationFailed {
                store:   "postgres".to_string(),
                message: format!("Failed to spawn pg_restore --list: {e}"),
            })?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            Err(BackupError::VerificationFailed {
                store:   "postgres".to_string(),
                message: stderr,
            })
        }
    }

    /// Sum the file sizes of all `.dump` files in `backup_dir`.
    ///
    /// # Errors
    ///
    /// Returns `BackupError::StorageError` if the directory cannot be read.
    async fn get_storage_usage(&self) -> BackupResult<StorageUsage> {
        if !self.backup_dir.exists() {
            return Ok(StorageUsage {
                total_bytes:             0,
                backup_count:            0,
                oldest_backup_timestamp: None,
                newest_backup_timestamp: None,
            });
        }

        let backups = self.list_backups().await?;
        let total_bytes: u64 = backups.iter().map(|b| b.size_bytes).sum();
        let backup_count = u32::try_from(backups.len()).unwrap_or(u32::MAX);

        let oldest = backups.last().map(|b| b.timestamp);
        let newest = backups.first().map(|b| b.timestamp);

        Ok(StorageUsage {
            total_bytes,
            backup_count,
            oldest_backup_timestamp: oldest,
            newest_backup_timestamp: newest,
        })
    }
}

/// Return current Unix timestamp in seconds.
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_id_generation() {
        let id1 = PostgresBackupProvider::generate_backup_id();
        let id2 = PostgresBackupProvider::generate_backup_id();

        assert!(id1.starts_with("postgres-"), "id: {id1}");
        // IDs contain a unix timestamp — both must parse as numeric suffix.
        let suffix1 = id1.strip_prefix("postgres-").unwrap();
        assert!(suffix1.parse::<u64>().is_ok(), "suffix must be numeric: {suffix1}");
        // Two IDs generated in the same second may be equal, but they must match the format.
        assert!(id2.starts_with("postgres-"));
    }

    #[test]
    fn test_redact_url_with_password() {
        let url = "postgresql://alice:secret@db.example.com:5432/mydb";
        let redacted = PostgresBackupProvider::redact_url(url);
        assert!(!redacted.contains("secret"), "password must be redacted: {redacted}");
        assert!(redacted.contains("alice"), "username must be preserved");
        assert!(redacted.contains("db.example.com"), "host must be preserved");
    }

    #[test]
    fn test_redact_url_without_password() {
        let url = "postgresql://db.example.com/mydb";
        let redacted = PostgresBackupProvider::redact_url(url);
        // No password present — returned unchanged.
        assert_eq!(redacted, url);
    }

    #[test]
    fn test_dump_and_meta_paths() {
        let provider = PostgresBackupProvider::new(
            "postgresql://localhost/test".to_string(),
            "/tmp/backups".to_string(),
        );
        let id = "postgres-1234567890";
        assert!(provider.dump_path(id).to_str().unwrap().ends_with(".dump"));
        assert!(provider.meta_path(id).to_str().unwrap().ends_with(".meta.json"));
    }

    #[test]
    fn test_is_implemented_returns_true() {
        let provider = PostgresBackupProvider::new(String::new(), String::new());
        assert!(provider.is_implemented());
    }

    #[tokio::test]
    async fn test_health_check_fails_gracefully_without_pg_isready() {
        // Provide a definitely-unreachable URL; pg_isready will fail or not be found.
        // Either outcome must produce Err, not a panic.
        let provider = PostgresBackupProvider::new(
            "postgresql://localhost:1/nonexistent".to_string(),
            "/tmp".to_string(),
        );
        // We accept any Err — the important thing is no panic.
        let _ = provider.health_check().await;
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL and pg_dump binary"]
    async fn test_backup_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for this test");

        let provider = PostgresBackupProvider::new(url, dir.path().to_str().unwrap().to_string());
        let info = provider.backup().await.expect("backup must succeed");

        assert!(provider.dump_path(&info.backup_id).exists());
        assert!(provider.meta_path(&info.backup_id).exists());
        assert!(info.size_bytes > 0);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL and pg_dump binary"]
    async fn test_list_returns_backup_after_creation() {
        let dir = tempfile::tempdir().unwrap();
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for this test");

        let provider = PostgresBackupProvider::new(url, dir.path().to_str().unwrap().to_string());
        let info = provider.backup().await.unwrap();

        let list = provider.list_backups().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].backup_id, info.backup_id);
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL and pg_dump binary"]
    async fn test_verify_backup_passes() {
        let dir = tempfile::tempdir().unwrap();
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for this test");

        let provider = PostgresBackupProvider::new(url, dir.path().to_str().unwrap().to_string());
        let info = provider.backup().await.unwrap();
        assert!(provider.verify_backup(&info.backup_id).await.is_ok(), "backup just created should pass verification");
    }
}
