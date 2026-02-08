//! Credential rotation and key lifecycle management including versioning,
//! TTL tracking, automatic refresh, and multi-version decryption support.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use chrono::{DateTime, Duration, Utc};

/// Key version identifier (0 = unversioned/legacy, 1-65535 = versioned)
pub type KeyVersion = u16;

/// Status of a key version in its lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVersionStatus {
    /// Active and available for encryption and decryption
    Active,
    /// Approaching expiry, cannot encrypt but can decrypt
    Expiring,
    /// Expired, cannot encrypt but can decrypt (archival)
    Expired,
    /// Compromised, should not be used but retained for decryption
    Compromised,
}

impl std::fmt::Display for KeyVersionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Expiring => write!(f, "expiring"),
            Self::Expired => write!(f, "expired"),
            Self::Compromised => write!(f, "compromised"),
        }
    }
}

/// Metadata for a versioned encryption key
#[derive(Debug, Clone)]
pub struct KeyVersionMetadata {
    /// Version identifier
    pub version:           KeyVersion,
    /// When this version was issued
    pub issued_at:         DateTime<Utc>,
    /// When this version expires (TTL)
    pub expires_at:        DateTime<Utc>,
    /// Current status in lifecycle
    pub status:            KeyVersionStatus,
    /// Is this the current version for new encryptions?
    pub is_current:        bool,
    /// Reason for compromised status (if applicable)
    pub compromise_reason: Option<String>,
}

impl KeyVersionMetadata {
    /// Create new key version metadata
    pub fn new(version: KeyVersion, ttl_days: u32) -> Self {
        let now = Utc::now();
        Self {
            version,
            issued_at: now,
            expires_at: now + Duration::days(ttl_days as i64),
            status: KeyVersionStatus::Active,
            is_current: false,
            compromise_reason: None,
        }
    }

    /// Check if version is expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Check if version is expiring soon (< 14 days)
    pub fn is_expiring_soon(&self) -> bool {
        let remaining = self.expires_at - Utc::now();
        remaining < Duration::days(14) && !self.is_expired()
    }

    /// Get time until expiration
    pub fn time_until_expiry(&self) -> Duration {
        self.expires_at - Utc::now()
    }

    /// Get percentage of TTL consumed
    pub fn ttl_consumed_percent(&self) -> u32 {
        let total_ttl = self.expires_at - self.issued_at;
        let elapsed = Utc::now() - self.issued_at;
        if total_ttl.num_seconds() <= 0 {
            100
        } else {
            let percent = (elapsed.num_seconds() as f64 / total_ttl.num_seconds() as f64) * 100.0;
            percent.min(100.0) as u32
        }
    }

    /// Check if refresh should trigger (80% of TTL consumed)
    pub fn should_refresh(&self) -> bool {
        self.status == KeyVersionStatus::Active && self.ttl_consumed_percent() >= 80
    }

    /// Update status based on current time
    pub fn update_status(&mut self) {
        match self.status {
            KeyVersionStatus::Compromised => {}, // Never change compromised status
            KeyVersionStatus::Active => {
                if self.is_expired() {
                    self.status = KeyVersionStatus::Expired;
                } else if self.is_expiring_soon() {
                    self.status = KeyVersionStatus::Expiring;
                }
            },
            KeyVersionStatus::Expiring => {
                if self.is_expired() {
                    self.status = KeyVersionStatus::Expired;
                }
            },
            KeyVersionStatus::Expired => {}, // Remains expired
        }
    }

    /// Mark key as compromised
    pub fn mark_compromised(&mut self, reason: impl Into<String>) {
        self.status = KeyVersionStatus::Compromised;
        self.compromise_reason = Some(reason.into());
    }
}

/// Rotation schedule configuration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RotationSchedule {
    /// Manual rotation only (no automatic schedule)
    Manual,
    /// Automatic rotation at cron expression (e.g., "0 2 1 * *" for monthly)
    Cron(String),
    /// Automatic rotation every N days
    Interval(u32),
}

impl std::fmt::Display for RotationSchedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::Cron(expr) => write!(f, "cron: {}", expr),
            Self::Interval(days) => write!(f, "every {} days", days),
        }
    }
}

/// Credential rotation configuration
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// TTL for each key version (days)
    pub ttl_days:                  u32,
    /// When to trigger refresh (percentage of TTL consumed)
    pub refresh_threshold_percent: u32,
    /// Rotation schedule
    pub schedule:                  RotationSchedule,
    /// Maximum number of historical versions to retain
    pub max_retained_versions:     usize,
}

impl RotationConfig {
    /// Create default rotation config (annual rotation, 80% refresh)
    pub fn new() -> Self {
        Self {
            ttl_days:                  365,
            refresh_threshold_percent: 80,
            schedule:                  RotationSchedule::Manual,
            max_retained_versions:     10,
        }
    }

    /// Set TTL in days
    pub fn with_ttl_days(mut self, days: u32) -> Self {
        self.ttl_days = days;
        self
    }

    /// Set refresh threshold percentage
    pub fn with_refresh_threshold(mut self, percent: u32) -> Self {
        self.refresh_threshold_percent = percent.min(99);
        self
    }

    /// Set rotation schedule
    pub fn with_schedule(mut self, schedule: RotationSchedule) -> Self {
        self.schedule = schedule;
        self
    }
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Metrics for credential rotation tracking
#[derive(Debug, Clone)]
pub struct RotationMetrics {
    /// Total number of rotations
    total_rotations:           Arc<AtomicU64>,
    /// Number of failed rotations
    failed_rotations:          Arc<AtomicU64>,
    /// Last rotation timestamp
    last_rotation:             Arc<std::sync::Mutex<Option<DateTime<Utc>>>>,
    /// Rotation duration (milliseconds)
    last_rotation_duration_ms: Arc<AtomicU64>,
}

impl RotationMetrics {
    /// Create new rotation metrics
    pub fn new() -> Self {
        Self {
            total_rotations:           Arc::new(AtomicU64::new(0)),
            failed_rotations:          Arc::new(AtomicU64::new(0)),
            last_rotation:             Arc::new(std::sync::Mutex::new(None)),
            last_rotation_duration_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record successful rotation
    pub fn record_rotation(&self, duration_ms: u64) {
        self.total_rotations.fetch_add(1, Ordering::Relaxed);
        self.last_rotation_duration_ms.store(duration_ms, Ordering::Relaxed);
        if let Ok(mut last) = self.last_rotation.lock() {
            *last = Some(Utc::now());
        }
    }

    /// Record failed rotation
    pub fn record_failure(&self) {
        self.failed_rotations.fetch_add(1, Ordering::Relaxed);
    }

    /// Get total rotations count
    pub fn total_rotations(&self) -> u64 {
        self.total_rotations.load(Ordering::Relaxed)
    }

    /// Get failed rotations count
    pub fn failed_rotations(&self) -> u64 {
        self.failed_rotations.load(Ordering::Relaxed)
    }

    /// Get success rate percentage
    pub fn success_rate_percent(&self) -> u32 {
        let total = self.total_rotations();
        if total == 0 {
            100
        } else {
            let failed = self.failed_rotations();
            let successful = total - failed;
            ((successful as f64 / total as f64) * 100.0) as u32
        }
    }

    /// Get last rotation timestamp
    pub fn last_rotation(&self) -> Option<DateTime<Utc>> {
        if let Ok(last) = self.last_rotation.lock() {
            *last
        } else {
            None
        }
    }

    /// Get last rotation duration in milliseconds
    pub fn last_rotation_duration_ms(&self) -> u64 {
        self.last_rotation_duration_ms.load(Ordering::Relaxed)
    }
}

impl Default for RotationMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Versioned encryption key storage
#[derive(Debug, Clone)]
pub struct VersionedKeyStorage {
    /// Map of version ID to key metadata
    versions:        Arc<std::sync::Mutex<HashMap<KeyVersion, KeyVersionMetadata>>>,
    /// Current active version
    current_version: Arc<std::sync::Mutex<KeyVersion>>,
    /// Next version number to assign
    next_version:    Arc<AtomicU64>,
}

impl VersionedKeyStorage {
    /// Create new versioned key storage
    pub fn new() -> Self {
        Self {
            versions:        Arc::new(std::sync::Mutex::new(HashMap::new())),
            current_version: Arc::new(std::sync::Mutex::new(0)),
            next_version:    Arc::new(AtomicU64::new(1)),
        }
    }

    /// Add a new key version
    pub fn add_version(&self, metadata: KeyVersionMetadata) -> Result<KeyVersion, String> {
        let mut versions =
            self.versions.lock().map_err(|e| format!("Failed to lock versions: {}", e))?;

        let version = metadata.version;
        versions.insert(version, metadata);
        Ok(version)
    }

    /// Set current version
    pub fn set_current_version(&self, version: KeyVersion) -> Result<(), String> {
        let versions =
            self.versions.lock().map_err(|e| format!("Failed to lock versions: {}", e))?;

        if !versions.contains_key(&version) {
            return Err(format!("Version {} not found", version));
        }

        let mut current = self
            .current_version
            .lock()
            .map_err(|e| format!("Failed to lock current version: {}", e))?;
        *current = version;
        Ok(())
    }

    /// Get current version
    pub fn get_current_version(&self) -> Result<KeyVersion, String> {
        let current = self
            .current_version
            .lock()
            .map_err(|e| format!("Failed to lock current version: {}", e))?;
        Ok(*current)
    }

    /// Get version metadata by ID
    pub fn get_version(&self, version: KeyVersion) -> Result<Option<KeyVersionMetadata>, String> {
        let versions =
            self.versions.lock().map_err(|e| format!("Failed to lock versions: {}", e))?;
        Ok(versions.get(&version).cloned())
    }

    /// Get all versions sorted by issue date (newest first)
    pub fn get_all_versions(&self) -> Result<Vec<KeyVersionMetadata>, String> {
        let versions =
            self.versions.lock().map_err(|e| format!("Failed to lock versions: {}", e))?;

        let mut all_versions: Vec<_> = versions.values().cloned().collect();
        all_versions.sort_by_key(|v| std::cmp::Reverse(v.issued_at));
        Ok(all_versions)
    }

    /// Get next version number
    pub fn next_version_number(&self) -> KeyVersion {
        let next = self.next_version.fetch_add(1, Ordering::Relaxed);
        next as KeyVersion
    }
}

impl Default for VersionedKeyStorage {
    fn default() -> Self {
        Self::new()
    }
}

/// Credential rotation manager for key lifecycle
#[derive(Debug, Clone)]
pub struct CredentialRotationManager {
    /// Rotation configuration
    config:  Arc<RotationConfig>,
    /// Versioned key storage
    storage: Arc<VersionedKeyStorage>,
    /// Rotation metrics
    metrics: Arc<RotationMetrics>,
}

impl CredentialRotationManager {
    /// Create new credential rotation manager
    pub fn new(config: RotationConfig) -> Self {
        Self {
            config:  Arc::new(config),
            storage: Arc::new(VersionedKeyStorage::new()),
            metrics: Arc::new(RotationMetrics::new()),
        }
    }

    /// Initialize with first key version
    pub fn initialize_key(&self) -> Result<KeyVersion, String> {
        let version = self.storage.next_version_number();
        let metadata = KeyVersionMetadata::new(version, self.config.ttl_days);
        self.storage.add_version(metadata)?;
        self.storage.set_current_version(version)?;
        Ok(version)
    }

    /// Trigger key rotation
    pub fn rotate_key(&self) -> Result<KeyVersion, String> {
        let start = std::time::Instant::now();

        let new_version = self.storage.next_version_number();
        let mut metadata = KeyVersionMetadata::new(new_version, self.config.ttl_days);

        // New version is immediately current
        metadata.is_current = true;
        self.storage.add_version(metadata)?;
        self.storage.set_current_version(new_version)?;

        let duration_ms = start.elapsed().as_millis() as u64;
        self.metrics.record_rotation(duration_ms);

        Ok(new_version)
    }

    /// Get current version number
    pub fn get_current_version(&self) -> Result<KeyVersion, String> {
        self.storage.get_current_version()
    }

    /// Check if refresh is needed for any version
    pub fn needs_refresh(&self) -> Result<bool, String> {
        let current_version = self.storage.get_current_version()?;
        if let Some(metadata) = self.storage.get_version(current_version)? {
            Ok(metadata.should_refresh())
        } else {
            Ok(false)
        }
    }

    /// Get current version metadata
    pub fn get_current_metadata(&self) -> Result<Option<KeyVersionMetadata>, String> {
        let current_version = self.storage.get_current_version()?;
        self.storage.get_version(current_version)
    }

    /// Get version from ciphertext (first 2 bytes as big-endian u16)
    pub fn extract_version_from_ciphertext(ciphertext: &[u8]) -> Result<KeyVersion, String> {
        if ciphertext.len() < 2 {
            return Err("Ciphertext too short for version".to_string());
        }
        let version = u16::from_be_bytes([ciphertext[0], ciphertext[1]]);
        Ok(version)
    }

    /// Check if version exists and is usable for decryption
    pub fn can_decrypt_with_version(&self, version: KeyVersion) -> Result<bool, String> {
        if let Some(metadata) = self.storage.get_version(version)? {
            // Can decrypt with any non-compromised version
            Ok(metadata.status != KeyVersionStatus::Compromised)
        } else {
            Ok(false)
        }
    }

    /// Get rotation metrics
    pub fn metrics(&self) -> Arc<RotationMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get all version history
    pub fn get_version_history(&self) -> Result<Vec<KeyVersionMetadata>, String> {
        self.storage.get_all_versions()
    }

    // ========== REFACTOR ENHANCEMENTS ==========

    /// Check if any version needs attention (expiring or expired)
    pub fn has_versions_needing_attention(&self) -> Result<bool, String> {
        let history = self.get_version_history()?;
        Ok(history
            .iter()
            .any(|m| m.is_expiring_soon() || m.status == KeyVersionStatus::Compromised))
    }

    /// Get active versions count
    pub fn active_versions_count(&self) -> Result<usize, String> {
        let history = self.get_version_history()?;
        Ok(history.iter().filter(|m| m.status == KeyVersionStatus::Active).count())
    }

    /// Get expired versions count
    pub fn expired_versions_count(&self) -> Result<usize, String> {
        let history = self.get_version_history()?;
        Ok(history.iter().filter(|m| m.status == KeyVersionStatus::Expired).count())
    }

    /// Get compromised versions count
    pub fn compromised_versions_count(&self) -> Result<usize, String> {
        let history = self.get_version_history()?;
        Ok(history.iter().filter(|m| m.status == KeyVersionStatus::Compromised).count())
    }

    /// Check if current version needs refresh
    pub fn current_version_needs_refresh(&self) -> Result<bool, String> {
        let current_version = self.get_current_version()?;
        if let Some(metadata) = self.storage.get_version(current_version)? {
            Ok(metadata.should_refresh())
        } else {
            Ok(false)
        }
    }

    /// Perform emergency rotation due to compromise
    pub fn emergency_rotate(&self, reason: impl Into<String>) -> Result<KeyVersion, String> {
        let current_version = self.get_current_version()?;
        if let Some(mut metadata) = self.storage.get_version(current_version)? {
            metadata.mark_compromised(reason);
            self.storage.add_version(metadata)?;
        }

        // Trigger immediate rotation
        self.rotate_key()
    }

    /// Get next scheduled rotation time (for manual schedule)
    pub fn last_rotation_time(&self) -> Option<DateTime<Utc>> {
        self.metrics.last_rotation()
    }

    /// Get time since last rotation
    pub fn time_since_last_rotation(&self) -> Option<Duration> {
        self.metrics.last_rotation().map(|last| Utc::now() - last)
    }

    /// Mark specific version as compromised
    pub fn mark_version_compromised(
        &self,
        version: KeyVersion,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        if let Some(mut metadata) = self.storage.get_version(version)? {
            metadata.mark_compromised(reason);
            // Update in storage
            self.storage.add_version(metadata)?;
            Ok(())
        } else {
            Err(format!("Version {} not found", version))
        }
    }

    /// Check compliance for HIPAA (annual rotation)
    pub fn check_hipaa_compliance(&self) -> Result<bool, String> {
        let metadata = self.get_current_metadata()?;
        if let Some(m) = metadata {
            // HIPAA requires rotation at least annually
            Ok(m.ttl_consumed_percent() < 100)
        } else {
            Ok(false)
        }
    }

    /// Check compliance for PCI-DSS (annual rotation)
    pub fn check_pci_compliance(&self) -> Result<bool, String> {
        let metadata = self.get_current_metadata()?;
        if let Some(m) = metadata {
            // PCI-DSS requires rotation at least annually
            Ok(m.ttl_consumed_percent() < 100)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_version_metadata_creation() {
        let metadata = KeyVersionMetadata::new(1, 365);
        assert_eq!(metadata.version, 1);
        assert_eq!(metadata.status, KeyVersionStatus::Active);
        assert!(!metadata.is_current);
        assert!(!metadata.is_expired());
    }

    #[test]
    fn test_key_version_is_expiring_soon() {
        let mut metadata = KeyVersionMetadata::new(1, 365);
        // Manually set expires_at to near future for testing
        metadata.expires_at = Utc::now() + Duration::days(7);
        assert!(metadata.is_expiring_soon());
    }

    #[test]
    fn test_key_version_ttl_consumed() {
        let now = Utc::now();
        let mut metadata = KeyVersionMetadata::new(1, 10);
        // Simulate time passage: 8 days out of 10 = 80% consumed
        metadata.issued_at = now - Duration::days(8);
        metadata.expires_at = now + Duration::days(2);
        let percent = metadata.ttl_consumed_percent();
        assert!(percent >= 75);
    }

    #[test]
    fn test_key_version_should_refresh() {
        let now = Utc::now();
        let mut metadata = KeyVersionMetadata::new(1, 100);
        // Simulate 81 days out of 100 = 81% consumed
        metadata.issued_at = now - Duration::days(81);
        metadata.expires_at = now + Duration::days(19);
        assert!(metadata.should_refresh());
    }

    #[test]
    fn test_key_version_mark_compromised() {
        let mut metadata = KeyVersionMetadata::new(1, 365);
        metadata.mark_compromised("Leaked in incident");
        assert_eq!(metadata.status, KeyVersionStatus::Compromised);
        assert!(metadata.compromise_reason.is_some());
    }

    #[test]
    fn test_rotation_config_default() {
        let config = RotationConfig::new();
        assert_eq!(config.ttl_days, 365);
        assert_eq!(config.refresh_threshold_percent, 80);
        assert_eq!(config.schedule, RotationSchedule::Manual);
    }

    #[test]
    fn test_rotation_config_builder() {
        let config = RotationConfig::new().with_ttl_days(90).with_refresh_threshold(75);
        assert_eq!(config.ttl_days, 90);
        assert_eq!(config.refresh_threshold_percent, 75);
    }

    #[test]
    fn test_rotation_metrics_record() {
        let metrics = RotationMetrics::new();
        metrics.record_rotation(100);
        assert_eq!(metrics.total_rotations(), 1);
        assert_eq!(metrics.failed_rotations(), 0);
        assert_eq!(metrics.success_rate_percent(), 100);
    }

    #[test]
    fn test_rotation_metrics_failure() {
        let metrics = RotationMetrics::new();
        metrics.record_rotation(100);
        metrics.record_rotation(100);
        metrics.record_failure();
        assert_eq!(metrics.total_rotations(), 2);
        assert_eq!(metrics.failed_rotations(), 1);
        assert_eq!(metrics.success_rate_percent(), 50);
    }

    #[test]
    fn test_versioned_key_storage_add_version() {
        let storage = VersionedKeyStorage::new();
        let metadata = KeyVersionMetadata::new(1, 365);
        let result = storage.add_version(metadata);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }

    #[test]
    fn test_versioned_key_storage_current_version() {
        let storage = VersionedKeyStorage::new();
        let metadata = KeyVersionMetadata::new(1, 365);
        storage.add_version(metadata).unwrap();
        storage.set_current_version(1).unwrap();
        assert_eq!(storage.get_current_version().unwrap(), 1);
    }

    #[test]
    fn test_credential_rotation_manager_initialize() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        let version = manager.initialize_key().unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_credential_rotation_manager_rotate() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let new_version = manager.rotate_key().unwrap();
        assert_eq!(new_version, 2);
        assert_eq!(manager.get_current_version().unwrap(), 2);
    }

    #[test]
    fn test_credential_rotation_manager_extract_version() {
        let version_bytes = [0u8, 5u8]; // Version 5 in big-endian
        let version =
            CredentialRotationManager::extract_version_from_ciphertext(&version_bytes).unwrap();
        assert_eq!(version, 5);
    }

    #[test]
    fn test_credential_rotation_manager_extract_version_short() {
        let version_bytes = [0u8]; // Too short
        let result = CredentialRotationManager::extract_version_from_ciphertext(&version_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_credential_rotation_manager_active_versions_count() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();
        let count = manager.active_versions_count().unwrap();
        assert!(count > 0);
    }

    #[test]
    fn test_credential_rotation_manager_current_needs_refresh() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let needs_refresh = manager.current_version_needs_refresh().unwrap();
        assert!(!needs_refresh); // New key shouldn't need refresh
    }

    #[test]
    fn test_credential_rotation_manager_emergency_rotate() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let old_version = manager.get_current_version().unwrap();
        let new_version = manager.emergency_rotate("Suspected compromise").unwrap();
        assert!(new_version > old_version);
    }

    #[test]
    fn test_credential_rotation_manager_mark_compromised() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let version = manager.get_current_version().unwrap();
        let result = manager.mark_version_compromised(version, "Test compromise");
        assert!(result.is_ok());
    }

    #[test]
    fn test_credential_rotation_manager_hipaa_compliance() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let compliant = manager.check_hipaa_compliance().unwrap();
        assert!(compliant); // New key should be compliant
    }

    #[test]
    fn test_credential_rotation_manager_pci_compliance() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let compliant = manager.check_pci_compliance().unwrap();
        assert!(compliant); // New key should be compliant
    }

    #[test]
    fn test_credential_rotation_manager_versions_needing_attention() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();
        let needs_attention = manager.has_versions_needing_attention().unwrap();
        assert!(!needs_attention); // New key shouldn't need attention
    }
}
