#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_rotation_status_response_creation() {
    let response = RotationStatusResponse::new(1, 365);
    assert_eq!(response.current_version, 1);
    assert_eq!(response.ttl_days, 365);
    assert_eq!(response.status, RotationStatus::Healthy);
}

#[test]
fn test_rotation_status_response_builder() {
    let now = Utc::now();
    let response = RotationStatusResponse::new(2, 365)
        .with_last_rotation(now)
        .with_status(RotationStatus::NeedsRotation);
    assert!(response.last_rotation.is_some());
    assert_eq!(response.status, RotationStatus::NeedsRotation);
}

#[test]
fn test_manual_rotation_response_success() {
    let response = ManualRotationResponse::success(1, 2, 100);
    assert_eq!(response.status, ManualRotationStatus::Success);
    assert_eq!(response.old_version, 1);
    assert_eq!(response.new_version, 2);
    assert!(response.error.is_none());
}

#[test]
fn test_manual_rotation_response_failure() {
    let response = ManualRotationResponse::failure(1, "Vault error");
    assert_eq!(response.status, ManualRotationStatus::Failed);
    assert!(response.error.is_some());
}

#[test]
fn test_rotation_history_response_creation() {
    let response = RotationHistoryResponse::new(0, 100);
    assert_eq!(response.offset, 0);
    assert_eq!(response.limit, 100);
    assert_eq!(response.total_count, 0);
}

#[test]
fn test_rotation_history_record_creation() {
    let now = Utc::now();
    let record = RotationHistoryRecord {
        timestamp:    now,
        old_version:  1,
        new_version:  2,
        reason:       Some("test".to_string()),
        duration_ms:  50,
        triggered_by: "manual".to_string(),
        user_id:      Some("user123".to_string()),
    };
    assert_eq!(record.old_version, 1);
    assert_eq!(record.new_version, 2);
}

#[test]
fn test_rotation_config_update_validation() {
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         Some(true),
        refresh_check_interval_hours: Some(24),
        refresh_threshold_percent:    Some(80),
        ttl_days:                     Some(365),
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    update.validate().unwrap_or_else(|e| panic!("expected Ok from validate: {e}"));
}

#[test]
fn test_rotation_config_update_invalid_threshold() {
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: None,
        refresh_threshold_percent:    Some(100), // Invalid
        ttl_days:                     None,
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    assert!(
        update.validate().is_err(),
        "expected Err for invalid threshold, got: {:?}",
        update.validate()
    );
}

#[test]
fn test_rotation_config_update_invalid_ttl() {
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: None,
        refresh_threshold_percent:    None,
        ttl_days:                     Some(400), // Invalid
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    assert!(
        update.validate().is_err(),
        "expected Err for invalid TTL, got: {:?}",
        update.validate()
    );
}

#[test]
fn test_rotation_schedule_response_manual() {
    let schedule = RotationScheduleResponse::manual();
    assert_eq!(schedule.schedule_type, ScheduleType::Manual);
    assert!(!schedule.enabled);
}

#[test]
fn test_rotation_schedule_response_cron() {
    let now = Utc::now();
    let schedule = RotationScheduleResponse::cron("0 2 1 * *", now);
    assert_eq!(schedule.schedule_type, ScheduleType::Cron);
    assert!(schedule.enabled);
}

#[test]
fn test_rotation_schedule_response_interval() {
    let now = Utc::now();
    let schedule = RotationScheduleResponse::interval(30, now);
    assert_eq!(schedule.schedule_type, ScheduleType::Interval);
    assert!(schedule.enabled);
}

#[test]
fn test_test_schedule_response_valid() {
    let times = vec![Utc::now(), Utc::now()];
    let response = TestScheduleResponse::valid(times);
    assert!(response.valid);
    assert_eq!(response.next_times.len(), 2);
}

#[test]
fn test_test_schedule_response_invalid() {
    let response = TestScheduleResponse::invalid("Invalid cron");
    assert!(!response.valid);
    assert!(response.error.is_some());
}

#[test]
fn test_api_error_response_creation() {
    let error = ApiErrorResponse::new("rotation_failed", "Vault unreachable");
    assert_eq!(error.error, "rotation_failed");
    assert_eq!(error.message, "Vault unreachable");
}

#[test]
fn test_api_error_response_with_code() {
    let error = ApiErrorResponse::new("rotation_failed", "Vault unreachable")
        .with_code("VAULT_UNAVAILABLE");
    assert!(error.code.is_some());
}

#[test]
fn test_config_preset_hipaa() {
    let config = ConfigPreset::Hipaa.get_config();
    assert_eq!(config.ttl_days, 365);
    assert!(config.auto_refresh_enabled);
}

#[test]
fn test_config_preset_gdpr() {
    let config = ConfigPreset::Gdpr.get_config();
    assert_eq!(config.ttl_days, 90);
    assert_eq!(config.refresh_threshold_percent, 75);
}

#[test]
fn test_compliance_presets_response() {
    let presets = CompliancePresetsResponse::default();
    assert_eq!(presets.presets.len(), 4);
}

#[test]
fn test_rotation_history_query_effective_limit() {
    let query1 = RotationHistoryQuery {
        limit:        Some(50),
        offset:       None,
        from:         None,
        to:           None,
        reason:       None,
        triggered_by: None,
        format:       None,
    };
    assert_eq!(query1.effective_limit(), 50);

    let query2 = RotationHistoryQuery {
        limit:        Some(5000),
        offset:       None,
        from:         None,
        to:           None,
        reason:       None,
        triggered_by: None,
        format:       None,
    };
    assert_eq!(query2.effective_limit(), 1000); // Capped
}

#[test]
fn test_rotation_status_display_healthy() {
    let status = RotationStatusResponse::new(1, 365);
    let display = status.to_display();
    assert_eq!(display.urgency_score, 10);
    assert!(display.recommended_action.contains("Monitor"));
}

#[test]
fn test_rotation_status_display_urgent() {
    let status = RotationStatusResponse::new(1, 365).with_status(RotationStatus::Overdue);
    let display = status.to_display();
    assert_eq!(display.urgency_score, 100);
    assert!(display.recommended_action.contains("CRITICAL"));
}

// ── Behavioral edge-case tests ───────────────────────────────────────────

#[test]
fn test_rotation_config_update_validates_zero_threshold() {
    // Threshold of 0 is invalid (must be 1-99)
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: None,
        refresh_threshold_percent:    Some(0),
        ttl_days:                     None,
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    assert!(
        update.validate().is_err(),
        "expected Err for zero threshold, got: {:?}",
        update.validate()
    );
}

#[test]
fn test_rotation_config_update_validates_zero_ttl() {
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: None,
        refresh_threshold_percent:    None,
        ttl_days:                     Some(0),
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    assert!(
        update.validate().is_err(),
        "expected Err for zero TTL, got: {:?}",
        update.validate()
    );
}

#[test]
fn test_rotation_config_update_validates_zero_interval() {
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: Some(0),
        refresh_threshold_percent:    None,
        ttl_days:                     None,
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    assert!(
        update.validate().is_err(),
        "expected Err for zero interval, got: {:?}",
        update.validate()
    );
}

#[test]
fn test_rotation_config_update_valid_boundary_values() {
    // Test the valid edges: threshold=1, ttl=1, interval=1
    let update = RotationConfigUpdateRequest {
        auto_refresh_enabled:         None,
        refresh_check_interval_hours: Some(1),
        refresh_threshold_percent:    Some(1),
        ttl_days:                     Some(1),
        quiet_hours_start:            None,
        quiet_hours_end:              None,
    };
    update
        .validate()
        .unwrap_or_else(|e| panic!("expected Ok from validate with boundary values: {e}"));
}

#[test]
fn test_rotation_history_query_max_limit() {
    let query = RotationHistoryQuery {
        limit:        Some(9999),
        offset:       Some(0),
        from:         None,
        to:           None,
        reason:       None,
        triggered_by: None,
        format:       None,
    };
    // effective_limit caps at 1000
    assert_eq!(query.effective_limit(), 1000);
}

#[test]
fn test_config_preset_pci_dss_key_rotation() {
    // PCI-DSS requires annual key rotation (365 days or less)
    let config = ConfigPreset::PciDss.get_config();
    assert!(config.ttl_days <= 365);
    assert!(config.auto_refresh_enabled);
}

#[test]
fn test_config_preset_hipaa_shorter_rotation_than_soc2() {
    // HIPAA is typically more aggressive on key rotation than SOC2
    let hipaa = ConfigPreset::Hipaa.get_config();
    let soc2 = ConfigPreset::Soc2.get_config();
    assert!(hipaa.ttl_days <= soc2.ttl_days);
}

#[test]
fn test_rotation_history_query_default_offset() {
    let query = RotationHistoryQuery {
        limit:        None,
        offset:       None,
        from:         None,
        to:           None,
        reason:       None,
        triggered_by: None,
        format:       None,
    };
    assert_eq!(query.effective_offset(), 0);
}

#[test]
fn test_rotation_history_query_effective_offset_with_value() {
    let query = RotationHistoryQuery {
        limit:        None,
        offset:       Some(42),
        from:         None,
        to:           None,
        reason:       None,
        triggered_by: None,
        format:       None,
    };
    assert_eq!(query.effective_offset(), 42);
}
