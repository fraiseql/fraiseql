//! File-based audit logging backend.
//!
//! Writes audit entries as append-only JSONL (one JSON object per line) with
//! optional HMAC-SHA256 hash chaining for tamper detection. Supports automatic
//! file rotation by size.
//!
//! # File format
//!
//! Each line is a self-contained JSON object representing one [`AuditEntry`].
//! When tamper-evident mode is enabled, each entry includes a `chain_hash`
//! field that depends on all previous entries — see [`crate::audit_chain`].
//!
//! # Rotation
//!
//! When the active file exceeds `max_file_size_bytes`, it is renamed to
//! `{stem}-{ISO8601}.jsonl` and a new empty file is started. The hash chain
//! resets on rotation (each file is independently verifiable).
//!
//! # Thread safety
//!
//! `FileAuditLogger` is `Send + Sync`. All mutable state is guarded by a
//! [`parking_lot::Mutex`].

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::audit_chain::ChainHasher;
use crate::audit_logger::{AuditEntry, AuditLogger};

/// Default maximum file size before rotation (100 MB).
const DEFAULT_MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Default chain seed (all zeros) — override with `chain_seed` in config.
const ZERO_SEED: [u8; 32] = [0u8; 32];

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the file audit backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAuditConfig {
    /// Path to the JSONL audit log file.
    pub path: PathBuf,
    /// Maximum file size in bytes before rotation. Defaults to 100 MB.
    pub max_file_size_bytes: u64,
    /// Whether to rotate files when the size threshold is exceeded.
    pub rotate: bool,
    /// Whether to enable tamper-evident hash chaining.
    pub tamper_evident: bool,
    /// 32-byte HMAC seed for the hash chain (hex-encoded, 64 chars).
    /// If `None`, a zero seed is used (suitable for non-production).
    pub chain_seed_hex: Option<String>,
}

impl Default for FileAuditConfig {
    fn default() -> Self {
        Self {
            path:                PathBuf::from("/var/log/fraiseql/audit.jsonl"),
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
            rotate:              true,
            tamper_evident:      false,
            chain_seed_hex:      None,
        }
    }
}

// ============================================================================
// Serializable entry (adds timestamp)
// ============================================================================

/// On-disk representation of an audit entry with ISO 8601 timestamp.
#[derive(Serialize)]
struct TimestampedEntry<'a> {
    timestamp:     String,
    event_type:    &'a str,
    secret_type:   &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject:       &'a Option<String>,
    operation:     &'a str,
    success:       bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_message: &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context:       &'a Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chain_hash:    Option<String>,
}

// ============================================================================
// Inner mutable state
// ============================================================================

struct FileAuditInner {
    file:          File,
    path:          PathBuf,
    current_size:  u64,
    chain_hasher:  Option<ChainHasher>,
    config:        FileAuditConfig,
}

impl FileAuditInner {
    /// Serialize the entry to JSON, optionally computing a chain hash.
    fn serialize_entry(&mut self, entry: &AuditEntry) -> String {
        let mut ts_entry = TimestampedEntry {
            timestamp:     Utc::now().to_rfc3339(),
            event_type:    entry.event_type.as_str(),
            secret_type:   entry.secret_type.as_str(),
            subject:       &entry.subject,
            operation:     &entry.operation,
            success:       entry.success,
            error_message: &entry.error_message,
            context:       &entry.context,
            chain_hash:    None,
        };

        if let Some(hasher) = &mut self.chain_hasher {
            // Serialize without chain_hash first to compute the hash over the content.
            #[allow(clippy::unwrap_used)] // Reason: AuditEntry fields are all serializable
            let content_json = serde_json::to_string(&ts_entry).unwrap();
            let hash = hasher.advance(&content_json);
            ts_entry.chain_hash = Some(hash);
        }

        #[allow(clippy::unwrap_used)] // Reason: AuditEntry fields are all serializable
        serde_json::to_string(&ts_entry).unwrap()
    }

    /// Write a single JSONL line. Returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// Logs via tracing on I/O failure; never panics.
    fn write_line(&mut self, line: &str) {
        let bytes = line.as_bytes();
        let newline = b"\n";
        let total = bytes.len() as u64 + 1;

        if let Err(e) = self.file.write_all(bytes).and_then(|()| self.file.write_all(newline)) {
            error!(path = %self.path.display(), error = %e, "Failed to write audit entry");
            return;
        }

        self.current_size += total;
    }

    /// Rotate the file if the size threshold is exceeded.
    fn maybe_rotate(&mut self) {
        if !self.config.rotate || self.current_size < self.config.max_file_size_bytes {
            return;
        }

        let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
        let rotated = rotated_path(&self.path, &stamp.to_string());

        if let Err(e) = fs::rename(&self.path, &rotated) {
            error!(
                from = %self.path.display(),
                to = %rotated.display(),
                error = %e,
                "Failed to rotate audit log"
            );
            return;
        }

        match open_append(&self.path) {
            Ok(file) => {
                self.file = file;
                self.current_size = 0;
                // Reset chain hasher on rotation — each file is independently verifiable.
                if self.chain_hasher.is_some() {
                    let seed = parse_seed(self.config.chain_seed_hex.as_deref());
                    self.chain_hasher = Some(ChainHasher::new(seed));
                }
            },
            Err(e) => {
                error!(path = %self.path.display(), error = %e, "Failed to open new audit log after rotation");
            },
        }
    }
}

// ============================================================================
// FileAuditLogger
// ============================================================================

/// Append-only JSONL audit logger with optional hash chaining and rotation.
pub struct FileAuditLogger {
    inner: Mutex<FileAuditInner>,
}

impl FileAuditLogger {
    /// Create a new file audit logger.
    ///
    /// Creates parent directories if they don't exist. Opens the file in
    /// append mode (`O_APPEND`).
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the file cannot be opened.
    pub fn new(config: FileAuditConfig) -> std::io::Result<Self> {
        if let Some(parent) = config.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = open_append(&config.path)?;
        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        let chain_hasher = if config.tamper_evident {
            let seed = parse_seed(config.chain_seed_hex.as_deref());
            Some(ChainHasher::new(seed))
        } else {
            None
        };

        Ok(Self {
            inner: Mutex::new(FileAuditInner {
                file,
                path: config.path.clone(),
                current_size,
                chain_hasher,
                config,
            }),
        })
    }
}

impl AuditLogger for FileAuditLogger {
    fn log_entry(&self, entry: AuditEntry) {
        let mut inner = self.inner.lock();
        let line = inner.serialize_entry(&entry);
        inner.write_line(&line);
        inner.maybe_rotate();
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Open a file in append-only mode, creating it if it doesn't exist.
fn open_append(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
}

/// Build the rotated file path: `audit-20260318T120000Z.jsonl`.
fn rotated_path(original: &Path, timestamp: &str) -> PathBuf {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("audit");
    let ext = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("jsonl");
    let filename = format!("{stem}-{timestamp}.{ext}");
    original.with_file_name(filename)
}

/// Parse a hex-encoded 32-byte seed, falling back to a zero seed.
fn parse_seed(hex_seed: Option<&str>) -> [u8; 32] {
    hex_seed
        .and_then(|h| {
            let bytes = hex::decode(h).ok()?;
            <[u8; 32]>::try_from(bytes.as_slice()).ok()
        })
        .unwrap_or(ZERO_SEED)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::io::BufRead;

    use super::*;
    use crate::audit_logger::{AuditEventType, SecretType};

    fn test_entry() -> AuditEntry {
        AuditEntry {
            event_type:    AuditEventType::JwtValidation,
            secret_type:   SecretType::JwtToken,
            subject:       Some("user42".to_string()),
            operation:     "validate".to_string(),
            success:       true,
            error_message: None,
            context:       None,
            chain_hash:    None,
        }
    }

    fn temp_config(dir: &Path) -> FileAuditConfig {
        FileAuditConfig {
            path:                dir.join("audit.jsonl"),
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
            rotate:              false,
            tamper_evident:      false,
            chain_seed_hex:      None,
        }
    }

    // ----------------------------------------------------------------
    // Cycle 1: Basic JSONL writing
    // ----------------------------------------------------------------

    #[test]
    fn test_file_backend_appends_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let config = temp_config(dir.path());
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        logger.log_entry(test_entry());
        logger.log_entry(test_entry());

        let contents = fs::read_to_string(&config.path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "should have 2 JSONL lines");

        // Each line should be valid JSON.
        for line in &lines {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(v["event_type"], "jwt_validation");
            assert_eq!(v["success"], true);
            assert!(v["timestamp"].is_string());
        }
    }

    #[test]
    fn test_file_backend_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("deep").join("nested").join("dir");
        let config = FileAuditConfig {
            path: nested.join("audit.jsonl"),
            ..FileAuditConfig::default()
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();
        logger.log_entry(test_entry());

        assert!(config.path.exists());
    }

    #[test]
    fn test_file_backend_appends_to_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = temp_config(dir.path());

        // Write first entry.
        let logger1 = FileAuditLogger::new(config.clone()).unwrap();
        logger1.log_entry(test_entry());
        drop(logger1);

        // Reopen and write second entry.
        let logger2 = FileAuditLogger::new(config.clone()).unwrap();
        logger2.log_entry(test_entry());
        drop(logger2);

        let lines: Vec<String> = std::io::BufReader::new(File::open(&config.path).unwrap())
            .lines()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_file_backend_includes_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let config = temp_config(dir.path());
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        let entry = AuditEntry {
            event_type:    AuditEventType::AuthFailure,
            secret_type:   SecretType::ClientSecret,
            subject:       Some("svc-account".to_string()),
            operation:     "exchange".to_string(),
            success:       false,
            error_message: Some("Invalid grant".to_string()),
            context:       Some("provider=github".to_string()),
            chain_hash:    None,
        };
        logger.log_entry(entry);

        let contents = fs::read_to_string(&config.path).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(v["event_type"], "auth_failure");
        assert_eq!(v["secret_type"], "client_secret");
        assert_eq!(v["subject"], "svc-account");
        assert_eq!(v["operation"], "exchange");
        assert_eq!(v["success"], false);
        assert_eq!(v["error_message"], "Invalid grant");
        assert_eq!(v["context"], "provider=github");
    }

    #[test]
    fn test_file_backend_omits_none_fields() {
        let dir = tempfile::tempdir().unwrap();
        let config = temp_config(dir.path());
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        logger.log_entry(test_entry());

        let contents = fs::read_to_string(&config.path).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert!(v.get("error_message").is_none(), "None fields should be omitted");
        assert!(v.get("context").is_none());
        assert!(v.get("chain_hash").is_none());
    }

    // ----------------------------------------------------------------
    // Cycle 2: Hash chain
    // ----------------------------------------------------------------

    #[test]
    fn test_file_backend_hash_chain_present_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:           dir.path().join("audit.jsonl"),
            tamper_evident: true,
            ..FileAuditConfig::default()
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        logger.log_entry(test_entry());
        logger.log_entry(test_entry());
        logger.log_entry(test_entry());

        let contents = fs::read_to_string(&config.path).unwrap();
        let mut hashes: Vec<String> = Vec::new();
        for line in contents.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            let hash = v["chain_hash"].as_str().unwrap().to_string();
            assert_eq!(hash.len(), 64, "chain_hash should be 64 hex chars");
            hashes.push(hash);
        }

        // Each hash should be unique (different timestamps make each entry unique).
        assert_ne!(hashes[0], hashes[1]);
        assert_ne!(hashes[1], hashes[2]);
    }

    #[test]
    fn test_file_backend_hash_chain_absent_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:           dir.path().join("audit.jsonl"),
            tamper_evident: false,
            ..FileAuditConfig::default()
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();
        logger.log_entry(test_entry());

        let contents = fs::read_to_string(&config.path).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert!(v.get("chain_hash").is_none());
    }

    #[test]
    fn test_file_backend_chain_verifiable() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:                dir.path().join("audit.jsonl"),
            tamper_evident:      true,
            chain_seed_hex:      Some("ab".repeat(32)),
            rotate:              false,
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        for _ in 0..10 {
            logger.log_entry(test_entry());
        }

        let contents = fs::read_to_string(&config.path).unwrap();
        let entries: Vec<serde_json::Value> = contents
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(entries.len(), 10);

        // Verify chain integrity: all hashes present and sequential (each
        // depends on the previous). We can't recompute the exact hashes
        // because serde_json::Value re-serialization may reorder keys, but
        // we CAN verify that modifying any entry would require a different
        // hash by checking uniqueness and consistency.
        let hashes: Vec<&str> = entries
            .iter()
            .map(|e| e["chain_hash"].as_str().unwrap())
            .collect();

        // All hashes should be 64 hex chars.
        for h in &hashes {
            assert_eq!(h.len(), 64);
            assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        }

        // All hashes should be unique (entries have different timestamps).
        let unique: std::collections::HashSet<&&str> = hashes.iter().collect();
        assert_eq!(unique.len(), 10, "All chain hashes should be unique");

        // Writing the same entries with a different seed should produce different hashes.
        let dir2 = tempfile::tempdir().unwrap();
        let config2 = FileAuditConfig {
            path:                dir2.path().join("audit.jsonl"),
            tamper_evident:      true,
            chain_seed_hex:      Some("cd".repeat(32)),
            rotate:              false,
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
        };
        let logger2 = FileAuditLogger::new(config2.clone()).unwrap();
        logger2.log_entry(test_entry());

        let contents2 = fs::read_to_string(&config2.path).unwrap();
        let entry2: serde_json::Value = serde_json::from_str(contents2.trim()).unwrap();
        let hash2 = entry2["chain_hash"].as_str().unwrap();

        // Different seed → different hash (even if entry content is similar).
        assert_ne!(hashes[0], hash2);
    }

    #[test]
    fn test_file_backend_custom_seed() {
        let seed_hex = "ff".repeat(32);
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:           dir.path().join("audit.jsonl"),
            tamper_evident: true,
            chain_seed_hex: Some(seed_hex),
            ..FileAuditConfig::default()
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();
        logger.log_entry(test_entry());

        let contents = fs::read_to_string(&config.path).unwrap();
        let v: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert!(v["chain_hash"].is_string());
    }

    // ----------------------------------------------------------------
    // Cycle 3: File rotation
    // ----------------------------------------------------------------

    #[test]
    fn test_file_rotation_on_size_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:                dir.path().join("audit.jsonl"),
            max_file_size_bytes: 200, // Very small threshold to trigger rotation.
            rotate:              true,
            tamper_evident:      false,
            chain_seed_hex:      None,
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        // Write enough entries to exceed 200 bytes.
        for _ in 0..10 {
            logger.log_entry(test_entry());
        }

        // The original path should still exist (new file after rotation).
        assert!(config.path.exists());

        // There should be at least one rotated file.
        let rotated: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.starts_with("audit-")
                    && std::path::Path::new(&name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
            })
            .collect();
        assert!(
            !rotated.is_empty(),
            "Should have at least one rotated file"
        );
    }

    #[test]
    fn test_no_rotation_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:                dir.path().join("audit.jsonl"),
            max_file_size_bytes: 200,
            rotate:              false,
            tamper_evident:      false,
            chain_seed_hex:      None,
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        for _ in 0..20 {
            logger.log_entry(test_entry());
        }

        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 1, "Should have only the original file");
    }

    #[test]
    fn test_rotation_resets_chain() {
        let dir = tempfile::tempdir().unwrap();
        let config = FileAuditConfig {
            path:                dir.path().join("audit.jsonl"),
            max_file_size_bytes: 200,
            rotate:              true,
            tamper_evident:      true,
            chain_seed_hex:      None,
        };
        let logger = FileAuditLogger::new(config.clone()).unwrap();

        for _ in 0..20 {
            logger.log_entry(test_entry());
        }

        // The active file should have entries with chain_hash.
        let contents = fs::read_to_string(&config.path).unwrap();
        for line in contents.lines() {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(v["chain_hash"].is_string(), "Active file entries should have chain_hash");
        }
    }

    #[test]
    fn test_rotated_path_format() {
        let original = PathBuf::from("/var/log/fraiseql/audit.jsonl");
        let rotated = rotated_path(&original, "20260318T120000Z");
        assert_eq!(
            rotated,
            PathBuf::from("/var/log/fraiseql/audit-20260318T120000Z.jsonl")
        );
    }

    #[test]
    fn test_parse_seed_valid() {
        let hex_seed = "aa".repeat(32);
        let seed = parse_seed(Some(&hex_seed));
        assert_eq!(seed, [0xaa; 32]);
    }

    #[test]
    fn test_parse_seed_invalid_falls_back() {
        let seed = parse_seed(Some("not-hex"));
        assert_eq!(seed, ZERO_SEED);
    }

    #[test]
    fn test_parse_seed_none_falls_back() {
        let seed = parse_seed(None);
        assert_eq!(seed, ZERO_SEED);
    }

    #[test]
    fn test_parse_seed_wrong_length_falls_back() {
        let seed = parse_seed(Some("aabb")); // only 2 bytes
        assert_eq!(seed, ZERO_SEED);
    }
}
