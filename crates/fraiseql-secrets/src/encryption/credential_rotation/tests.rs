#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
    let returned_version = result.unwrap_or_else(|e| panic!("expected Ok from add_version: {e}"));
    assert_eq!(returned_version, 1);
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
    result.unwrap_or_else(|e| panic!("expected Ok from mark_version_compromised: {e}"));
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
