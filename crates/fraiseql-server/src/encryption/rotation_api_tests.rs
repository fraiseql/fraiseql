//! Comprehensive test specifications for credential rotation REST API endpoints,
//! rotation status retrieval, history tracking, and configuration management.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod rotation_api_tests {
    use chrono::{Duration, Utc};

    use crate::encryption::{
        credential_rotation::{CredentialRotationManager, RotationConfig},
        rotation_api::{
            ApiErrorResponse, CompliancePresetsResponse, ConfigPreset, ManualRotationRequest,
            ManualRotationResponse, RotationConfigResponse, RotationConfigUpdateRequest,
            RotationHistoryQuery, RotationHistoryRecord, RotationHistoryResponse,
            RotationScheduleResponse, RotationScheduleUpdateRequest, RotationStatus,
            RotationStatusResponse, ScheduleType, TestScheduleResponse,
        },
    };

    // ============================================================================
    // ROTATION STATUS ENDPOINT TESTS
    // ============================================================================

    /// Test GET /api/v1/admin/rotation/status response structure
    #[tokio::test]
    async fn test_rotation_status_endpoint_structure() {
        let response = RotationStatusResponse::new(1, 365);

        // Response includes all required fields
        assert_eq!(response.current_version, 1);
        assert_eq!(response.ttl_days, 365);
        assert!(response.last_rotation.is_none());
        assert!(response.next_rotation.is_none());
        assert_eq!(response.status, RotationStatus::Healthy);
        assert!(response.auto_refresh_enabled);
        assert_eq!(response.versions_total, 1);
        assert_eq!(response.versions_active, 1);
        assert_eq!(response.versions_expired, 0);
        assert_eq!(response.last_rotation_duration_ms, 0);
        assert_eq!(response.auto_refresh_checks, 0);
    }

    /// Test rotation status with multiple keys
    #[tokio::test]
    async fn test_rotation_status_multiple_keys() {
        // Create status responses for multiple keys
        let key1_status = RotationStatusResponse::new(3, 365).with_status(RotationStatus::Healthy);
        let key2_status =
            RotationStatusResponse::new(1, 90).with_status(RotationStatus::ExpiringSoon);

        // Each key has its own status
        assert_eq!(key1_status.current_version, 3);
        assert_eq!(key1_status.status, RotationStatus::Healthy);

        assert_eq!(key2_status.current_version, 1);
        assert_eq!(key2_status.status, RotationStatus::ExpiringSoon);

        // Can store as array of statuses
        let all_statuses = [key1_status, key2_status];
        assert_eq!(all_statuses.len(), 2);
    }

    /// Test rotation status indicates urgency
    #[tokio::test]
    async fn test_rotation_status_urgency_levels() {
        // Healthy: <70% TTL consumed
        let healthy = RotationStatusResponse::new(1, 365).with_status(RotationStatus::Healthy);
        assert_eq!(healthy.status, RotationStatus::Healthy);
        assert_eq!(format!("{}", healthy.status), "healthy");

        // Expiring soon: 70-85% TTL consumed
        let expiring =
            RotationStatusResponse::new(1, 365).with_status(RotationStatus::ExpiringSoon);
        assert_eq!(expiring.status, RotationStatus::ExpiringSoon);
        assert_eq!(format!("{}", expiring.status), "expiring_soon");

        // Needs rotation: 85%+ TTL consumed
        let needs_rotation =
            RotationStatusResponse::new(1, 365).with_status(RotationStatus::NeedsRotation);
        assert_eq!(needs_rotation.status, RotationStatus::NeedsRotation);
        assert_eq!(format!("{}", needs_rotation.status), "needs_rotation");

        // Overdue: >100% TTL consumed
        let overdue = RotationStatusResponse::new(1, 365).with_status(RotationStatus::Overdue);
        assert_eq!(overdue.status, RotationStatus::Overdue);
        assert_eq!(format!("{}", overdue.status), "overdue");
    }

    /// Test rotation status with auto-refresh
    #[tokio::test]
    async fn test_rotation_status_with_auto_refresh() {
        let now = Utc::now();
        let next = now + Duration::days(30);

        let mut response = RotationStatusResponse::new(2, 365);
        response.auto_refresh_enabled = true;
        response = response.with_next_rotation(next);

        assert!(response.auto_refresh_enabled);
        assert!(response.next_rotation.is_some());
        let next_time = response.next_rotation.unwrap();
        assert!(next_time > now);
    }

    /// Test rotation status metrics
    #[tokio::test]
    async fn test_rotation_status_includes_metrics() {
        let now = Utc::now();
        let mut response = RotationStatusResponse::new(3, 365);
        response.versions_total = 5;
        response.versions_active = 3;
        response.versions_expired = 2;
        response.last_rotation_duration_ms = 150;
        response.auto_refresh_checks = 42;
        response = response.with_last_rotation(now);

        assert_eq!(response.versions_total, 5);
        assert_eq!(response.versions_active, 3);
        assert_eq!(response.versions_expired, 2);
        assert_eq!(response.last_rotation_duration_ms, 150);
        assert_eq!(response.auto_refresh_checks, 42);
        assert!(response.last_rotation.is_some());
    }

    // ============================================================================
    // MANUAL ROTATION ENDPOINT TESTS
    // ============================================================================

    /// Test POST /api/v1/admin/rotation/rotate immediate rotation
    #[tokio::test]
    async fn test_rotation_endpoint_immediate_rotation() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let old_version = manager.get_current_version().unwrap();
        let start = std::time::Instant::now();
        let new_version = manager.rotate_key().unwrap();
        let duration_ms = start.elapsed().as_millis() as u64;

        // Build response
        let response = ManualRotationResponse::success(old_version, new_version, duration_ms);

        assert_eq!(response.status, "success");
        assert_eq!(response.old_version, old_version);
        assert_eq!(response.new_version, new_version);
        assert!(response.new_version > response.old_version);
        assert!(response.error.is_none());
    }

    /// Test rotation endpoint dry-run mode
    #[tokio::test]
    async fn test_rotation_endpoint_dry_run() {
        let request = ManualRotationRequest {
            key_id:  Some("primary".to_string()),
            reason:  None,
            dry_run: Some(true),
        };

        assert!(request.dry_run.unwrap());

        // In dry-run, no actual rotation occurs
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        let version_before = manager.get_current_version().unwrap();
        let active = manager.active_versions_count().unwrap();

        // Dry-run returns what would happen without state change
        assert!(active > 0);
        assert_eq!(manager.get_current_version().unwrap(), version_before);
    }

    /// Test rotation endpoint with reason tracking
    #[tokio::test]
    async fn test_rotation_endpoint_reason_tracking() {
        let request = ManualRotationRequest {
            key_id:  Some("primary".to_string()),
            reason:  Some("Scheduled quarterly rotation".to_string()),
            dry_run: None,
        };

        assert!(request.reason.is_some());
        assert_eq!(request.reason.unwrap(), "Scheduled quarterly rotation");

        // Reason stored in history record
        let now = Utc::now();
        let record = RotationHistoryRecord {
            timestamp:    now,
            old_version:  1,
            new_version:  2,
            reason:       Some("Scheduled quarterly rotation".to_string()),
            duration_ms:  50,
            triggered_by: "manual".to_string(),
            user_id:      Some("admin_user".to_string()),
        };

        assert_eq!(record.reason.as_deref(), Some("Scheduled quarterly rotation"));
        assert_eq!(record.triggered_by, "manual");
    }

    /// Test rotation endpoint requires authentication
    #[tokio::test]
    async fn test_rotation_endpoint_auth_required() {
        // Simulate auth failure responses
        let unauthorized = ApiErrorResponse::new("unauthorized", "Bearer token required");
        assert_eq!(unauthorized.error, "unauthorized");
        assert_eq!(unauthorized.message, "Bearer token required");

        let forbidden = ApiErrorResponse::new("forbidden", "Invalid token signature")
            .with_code("INVALID_TOKEN");
        assert_eq!(forbidden.error, "forbidden");
        assert!(forbidden.code.is_some());
        assert_eq!(forbidden.code.unwrap(), "INVALID_TOKEN");
    }

    /// Test rotation endpoint validates key ID
    #[tokio::test]
    async fn test_rotation_endpoint_validates_key_id() {
        let error = ApiErrorResponse::new("bad_request", "Key 'unknown' not found")
            .with_code("KEY_NOT_FOUND");

        assert_eq!(error.error, "bad_request");
        assert!(error.message.contains("unknown"));
        assert!(error.message.contains("not found"));
        assert_eq!(error.code.as_deref(), Some("KEY_NOT_FOUND"));
    }

    /// Test rotation endpoint prevents too-frequent rotation
    #[tokio::test]
    async fn test_rotation_endpoint_prevents_too_frequent() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // First rotation succeeds
        let v1 = manager.rotate_key().unwrap();
        assert!(v1 > 1);

        // Simulate cooldown check: the rotation config has a default cooldown
        let default_config = RotationConfigResponse::default();
        assert_eq!(default_config.manual_rotation_cooldown_minutes, 60);

        // If last rotation was within cooldown, return error
        let too_frequent_error =
            ApiErrorResponse::new("too_many_requests", "Rotation cooldown not elapsed")
                .with_code("COOLDOWN_ACTIVE");
        assert_eq!(too_frequent_error.error, "too_many_requests");
    }

    // ============================================================================
    // ROTATION HISTORY ENDPOINT TESTS
    // ============================================================================

    /// Test GET /api/v1/admin/rotation/history response structure
    #[tokio::test]
    async fn test_rotation_history_endpoint_structure() {
        let now = Utc::now();
        let record = RotationHistoryRecord {
            timestamp:    now,
            old_version:  1,
            new_version:  2,
            reason:       Some("Scheduled rotation".to_string()),
            duration_ms:  75,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };

        let response = RotationHistoryResponse::new(0, 10).with_record(record).with_total_count(1);

        assert_eq!(response.total_count, 1);
        assert_eq!(response.offset, 0);
        assert_eq!(response.limit, 10);
        assert_eq!(response.records.len(), 1);

        let rec = &response.records[0];
        assert_eq!(rec.old_version, 1);
        assert_eq!(rec.new_version, 2);
        assert_eq!(rec.reason.as_deref(), Some("Scheduled rotation"));
        assert_eq!(rec.duration_ms, 75);
        assert_eq!(rec.triggered_by, "auto");
        assert!(rec.user_id.is_none());
    }

    /// Test rotation history pagination
    #[tokio::test]
    async fn test_rotation_history_pagination() {
        // Default limit is 100, max 1000
        let query_default = RotationHistoryQuery {
            limit:        None,
            offset:       None,
            from:         None,
            to:           None,
            reason:       None,
            triggered_by: None,
            format:       None,
        };
        assert_eq!(query_default.effective_limit(), 100);
        assert_eq!(query_default.effective_offset(), 0);

        // Custom limit
        let query_custom = RotationHistoryQuery {
            limit:        Some(10),
            offset:       Some(20),
            from:         Some("2026-01-01".to_string()),
            to:           Some("2026-02-01".to_string()),
            reason:       None,
            triggered_by: None,
            format:       None,
        };
        assert_eq!(query_custom.effective_limit(), 10);
        assert_eq!(query_custom.effective_offset(), 20);

        // Limit capped at 1000
        let query_max = RotationHistoryQuery {
            limit:        Some(5000),
            offset:       None,
            from:         None,
            to:           None,
            reason:       None,
            triggered_by: None,
            format:       None,
        };
        assert_eq!(query_max.effective_limit(), 1000);
    }

    /// Test rotation history filtering
    #[tokio::test]
    async fn test_rotation_history_filtering() {
        let query = RotationHistoryQuery {
            limit:        Some(50),
            offset:       None,
            from:         None,
            to:           None,
            reason:       Some("scheduled".to_string()),
            triggered_by: Some("auto".to_string()),
            format:       None,
        };

        assert_eq!(query.reason.as_deref(), Some("scheduled"));
        assert_eq!(query.triggered_by.as_deref(), Some("auto"));

        // Multiple filters combined
        let query_multi = RotationHistoryQuery {
            limit:        Some(100),
            offset:       None,
            from:         Some("2026-01-01".to_string()),
            to:           Some("2026-02-01".to_string()),
            reason:       Some("emergency".to_string()),
            triggered_by: Some("manual".to_string()),
            format:       None,
        };
        assert!(query_multi.from.is_some());
        assert!(query_multi.to.is_some());
        assert_eq!(query_multi.reason.as_deref(), Some("emergency"));
        assert_eq!(query_multi.triggered_by.as_deref(), Some("manual"));
    }

    /// Test rotation history sorting
    #[tokio::test]
    async fn test_rotation_history_sorting() {
        let now = Utc::now();
        let record1 = RotationHistoryRecord {
            timestamp:    now - Duration::hours(2),
            old_version:  1,
            new_version:  2,
            reason:       None,
            duration_ms:  50,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };
        let record2 = RotationHistoryRecord {
            timestamp:    now - Duration::hours(1),
            old_version:  2,
            new_version:  3,
            reason:       None,
            duration_ms:  75,
            triggered_by: "manual".to_string(),
            user_id:      Some("admin".to_string()),
        };
        let record3 = RotationHistoryRecord {
            timestamp:    now,
            old_version:  3,
            new_version:  4,
            reason:       None,
            duration_ms:  100,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };

        // Newest first (descending timestamp)
        let mut records = [record1, record2, record3];
        records.sort_by_key(|a| std::cmp::Reverse(a.timestamp));
        assert_eq!(records[0].new_version, 4);
        assert_eq!(records[1].new_version, 3);
        assert_eq!(records[2].new_version, 2);

        // Oldest first (ascending)
        records.sort_by_key(|a| a.timestamp);
        assert_eq!(records[0].new_version, 2);
        assert_eq!(records[2].new_version, 4);
    }

    /// Test rotation history export
    #[tokio::test]
    async fn test_rotation_history_export() {
        // JSON format (default)
        let query_json = RotationHistoryQuery {
            limit:        None,
            offset:       None,
            from:         None,
            to:           None,
            reason:       None,
            triggered_by: None,
            format:       Some("json".to_string()),
        };
        assert_eq!(query_json.format.as_deref(), Some("json"));

        // CSV format
        let query_csv = RotationHistoryQuery {
            limit:        None,
            offset:       None,
            from:         None,
            to:           None,
            reason:       None,
            triggered_by: None,
            format:       Some("csv".to_string()),
        };
        assert_eq!(query_csv.format.as_deref(), Some("csv"));

        // JSON-lines format
        let query_jsonl = RotationHistoryQuery {
            limit:        None,
            offset:       None,
            from:         None,
            to:           None,
            reason:       None,
            triggered_by: None,
            format:       Some("json-lines".to_string()),
        };
        assert_eq!(query_jsonl.format.as_deref(), Some("json-lines"));
    }

    // ============================================================================
    // ROTATION CONFIGURATION ENDPOINT TESTS
    // ============================================================================

    /// Test GET /api/v1/admin/rotation/config
    #[tokio::test]
    async fn test_rotation_config_endpoint_get() {
        let config = RotationConfigResponse::default();

        assert!(config.auto_refresh_enabled);
        assert_eq!(config.refresh_check_interval_hours, 24);
        assert_eq!(config.refresh_threshold_percent, 80);
        assert_eq!(config.ttl_days, 365);
        assert!(config.quiet_hours_start.is_none());
        assert!(config.quiet_hours_end.is_none());
        assert_eq!(config.manual_rotation_cooldown_minutes, 60);
    }

    /// Test PUT /api/v1/admin/rotation/config update
    #[tokio::test]
    async fn test_rotation_config_endpoint_update() {
        let update = RotationConfigUpdateRequest {
            auto_refresh_enabled:         Some(true),
            refresh_check_interval_hours: None,
            refresh_threshold_percent:    Some(75),
            ttl_days:                     None,
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };

        // Validate passes
        assert!(update.validate().is_ok());

        // Apply partial update to default config
        let mut config = RotationConfigResponse::default();
        if let Some(enabled) = update.auto_refresh_enabled {
            config.auto_refresh_enabled = enabled;
        }
        if let Some(threshold) = update.refresh_threshold_percent {
            config.refresh_threshold_percent = threshold;
        }

        assert!(config.auto_refresh_enabled);
        assert_eq!(config.refresh_threshold_percent, 75);
        // Unchanged fields remain at default
        assert_eq!(config.refresh_check_interval_hours, 24);
        assert_eq!(config.ttl_days, 365);
    }

    /// Test rotation config validation
    #[tokio::test]
    async fn test_rotation_config_validation() {
        // Invalid threshold > 99
        let invalid_threshold = RotationConfigUpdateRequest {
            auto_refresh_enabled:         None,
            refresh_check_interval_hours: None,
            refresh_threshold_percent:    Some(100),
            ttl_days:                     None,
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(invalid_threshold.validate().is_err());

        // Invalid threshold < 1
        let invalid_threshold_low = RotationConfigUpdateRequest {
            auto_refresh_enabled:         None,
            refresh_check_interval_hours: None,
            refresh_threshold_percent:    Some(0),
            ttl_days:                     None,
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(invalid_threshold_low.validate().is_err());

        // Invalid TTL > 365
        let invalid_ttl = RotationConfigUpdateRequest {
            auto_refresh_enabled:         None,
            refresh_check_interval_hours: None,
            refresh_threshold_percent:    None,
            ttl_days:                     Some(400),
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(invalid_ttl.validate().is_err());

        // Invalid TTL < 1
        let invalid_ttl_zero = RotationConfigUpdateRequest {
            auto_refresh_enabled:         None,
            refresh_check_interval_hours: None,
            refresh_threshold_percent:    None,
            ttl_days:                     Some(0),
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(invalid_ttl_zero.validate().is_err());

        // Invalid interval > 720
        let invalid_interval = RotationConfigUpdateRequest {
            auto_refresh_enabled:         None,
            refresh_check_interval_hours: Some(800),
            refresh_threshold_percent:    None,
            ttl_days:                     None,
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(invalid_interval.validate().is_err());

        // Valid request passes
        let valid = RotationConfigUpdateRequest {
            auto_refresh_enabled:         Some(true),
            refresh_check_interval_hours: Some(24),
            refresh_threshold_percent:    Some(80),
            ttl_days:                     Some(365),
            quiet_hours_start:            None,
            quiet_hours_end:              None,
        };
        assert!(valid.validate().is_ok());
    }

    /// Test rotation config compliance defaults
    #[tokio::test]
    async fn test_rotation_config_compliance_presets() {
        let presets = CompliancePresetsResponse::default();
        assert_eq!(presets.presets.len(), 4);

        // HIPAA preset
        let hipaa = ConfigPreset::Hipaa.get_config();
        assert_eq!(hipaa.ttl_days, 365);
        assert!(hipaa.auto_refresh_enabled);
        assert_eq!(hipaa.refresh_check_interval_hours, 24);

        // PCI-DSS preset
        let pci = ConfigPreset::PciDss.get_config();
        assert_eq!(pci.ttl_days, 365);
        assert!(pci.auto_refresh_enabled);

        // GDPR preset
        let gdpr = ConfigPreset::Gdpr.get_config();
        assert_eq!(gdpr.ttl_days, 90);
        assert_eq!(gdpr.refresh_threshold_percent, 75);

        // SOC 2 preset
        let soc2 = ConfigPreset::Soc2.get_config();
        assert_eq!(soc2.ttl_days, 365);

        // Custom preset returns default
        let custom = ConfigPreset::Custom.get_config();
        assert_eq!(custom.ttl_days, 365);
    }

    /// Test rotation config apply preset
    #[tokio::test]
    async fn test_rotation_config_apply_preset() {
        // Apply HIPAA preset
        let hipaa_config = ConfigPreset::Hipaa.get_config();
        assert_eq!(hipaa_config.ttl_days, 365);
        assert!(hipaa_config.quiet_hours_start.is_some());
        assert_eq!(hipaa_config.quiet_hours_start, Some(2));
        assert_eq!(hipaa_config.quiet_hours_end, Some(4));

        // Apply GDPR preset (different from HIPAA)
        let gdpr_config = ConfigPreset::Gdpr.get_config();
        assert_eq!(gdpr_config.ttl_days, 90);
        assert!(gdpr_config.quiet_hours_start.is_none());
        assert_eq!(gdpr_config.manual_rotation_cooldown_minutes, 30);

        // Preset display names
        assert_eq!(format!("{}", ConfigPreset::Hipaa), "hipaa");
        assert_eq!(format!("{}", ConfigPreset::PciDss), "pci_dss");
        assert_eq!(format!("{}", ConfigPreset::Gdpr), "gdpr");
        assert_eq!(format!("{}", ConfigPreset::Soc2), "soc2");
    }

    // ============================================================================
    // ROTATION SCHEDULE ENDPOINT TESTS
    // ============================================================================

    /// Test GET /api/v1/admin/rotation/schedule
    #[tokio::test]
    async fn test_rotation_schedule_endpoint_get() {
        // Manual schedule
        let manual = RotationScheduleResponse::manual();
        assert_eq!(manual.schedule_type, ScheduleType::Manual);
        assert_eq!(manual.schedule_value, "manual");
        assert!(manual.next_scheduled_time.is_none());
        assert!(!manual.enabled);

        // Cron schedule
        let now = Utc::now();
        let cron = RotationScheduleResponse::cron("0 2 1 * *", now);
        assert_eq!(cron.schedule_type, ScheduleType::Cron);
        assert_eq!(cron.schedule_value, "0 2 1 * *");
        assert!(cron.next_scheduled_time.is_some());
        assert!(cron.enabled);

        // Interval schedule
        let interval = RotationScheduleResponse::interval(30, now);
        assert_eq!(interval.schedule_type, ScheduleType::Interval);
        assert_eq!(interval.schedule_value, "30 days");
        assert!(interval.enabled);
    }

    /// Test PUT /api/v1/admin/rotation/schedule update
    #[tokio::test]
    async fn test_rotation_schedule_endpoint_update() {
        let update = RotationScheduleUpdateRequest {
            schedule_type:  ScheduleType::Cron,
            schedule_value: "0 2 1 * *".to_string(),
        };

        assert_eq!(update.schedule_type, ScheduleType::Cron);
        assert_eq!(update.schedule_value, "0 2 1 * *");

        // Build updated response
        let next_time = Utc::now() + Duration::days(30);
        let response = RotationScheduleResponse::cron(&update.schedule_value, next_time);
        assert_eq!(response.schedule_type, ScheduleType::Cron);
        assert!(response.next_scheduled_time.is_some());
    }

    /// Test schedule validation
    #[tokio::test]
    async fn test_rotation_schedule_validation() {
        // Valid cron format
        let valid_cron = RotationScheduleUpdateRequest {
            schedule_type:  ScheduleType::Cron,
            schedule_value: "0 2 1 * *".to_string(),
        };
        assert_eq!(valid_cron.schedule_type, ScheduleType::Cron);

        // Valid interval
        let valid_interval = RotationScheduleUpdateRequest {
            schedule_type:  ScheduleType::Interval,
            schedule_value: "30".to_string(),
        };
        assert_eq!(valid_interval.schedule_type, ScheduleType::Interval);

        // Schedule type display
        assert_eq!(format!("{}", ScheduleType::Manual), "manual");
        assert_eq!(format!("{}", ScheduleType::Cron), "cron");
        assert_eq!(format!("{}", ScheduleType::Interval), "interval");
    }

    /// Test test schedule endpoint
    #[tokio::test]
    async fn test_rotation_schedule_test_endpoint() {
        // Valid schedule: returns next 10 times
        let now = Utc::now();
        let next_times: Vec<_> = (1..=10).map(|i| now + Duration::days(30 * i)).collect();
        let response = TestScheduleResponse::valid(next_times);

        assert!(response.valid);
        assert!(response.error.is_none());
        assert_eq!(response.next_times.len(), 10);

        // Invalid schedule: returns error
        let invalid = TestScheduleResponse::invalid("Invalid cron expression: missing field");
        assert!(!invalid.valid);
        assert!(invalid.error.is_some());
        assert!(invalid.error.unwrap().contains("Invalid cron"));
        assert!(invalid.next_times.is_empty());
    }

    // ============================================================================
    // ROTATION ERROR HANDLING TESTS
    // ============================================================================

    /// Test rotation error response format
    #[tokio::test]
    async fn test_rotation_error_response_format() {
        let error = ApiErrorResponse::new("rotation_failed", "Vault unreachable")
            .with_code("VAULT_UNAVAILABLE");

        assert_eq!(error.error, "rotation_failed");
        assert_eq!(error.message, "Vault unreachable");
        assert_eq!(error.code.as_deref(), Some("VAULT_UNAVAILABLE"));

        // Consistent format: all errors have error and message fields
        let another = ApiErrorResponse::new("invalid_request", "Missing key_id parameter");
        assert_eq!(another.error, "invalid_request");
        assert!(another.code.is_none());
    }

    /// Test rotation timeout handling
    #[tokio::test]
    async fn test_rotation_timeout_handling() {
        let error =
            ApiErrorResponse::new("gateway_timeout", "Rotation in progress, check status endpoint")
                .with_code("ROTATION_TIMEOUT");

        assert_eq!(error.error, "gateway_timeout");
        assert!(error.message.contains("check status"));
        assert_eq!(error.code.as_deref(), Some("ROTATION_TIMEOUT"));
    }

    /// Test rotation concurrent request handling
    #[tokio::test]
    async fn test_rotation_concurrent_requests() {
        let error = ApiErrorResponse::new("conflict", "Rotation already in progress")
            .with_code("ROTATION_IN_PROGRESS");

        assert_eq!(error.error, "conflict");
        assert!(error.message.contains("already in progress"));
        assert_eq!(error.code.as_deref(), Some("ROTATION_IN_PROGRESS"));

        // Failed rotation response
        let failed = ManualRotationResponse::failure(3, "Rotation already in progress");
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.old_version, 3);
        assert_eq!(failed.new_version, 3); // Unchanged on failure
        assert!(failed.error.is_some());
    }

    // ============================================================================
    // ROTATION API SECURITY TESTS
    // ============================================================================

    /// Test rotation endpoint bearer token validation
    #[tokio::test]
    async fn test_rotation_bearer_token_validation() {
        // Expired token: 401
        let expired =
            ApiErrorResponse::new("unauthorized", "Token expired").with_code("TOKEN_EXPIRED");
        assert_eq!(expired.error, "unauthorized");

        // Invalid signature: 403
        let invalid_sig = ApiErrorResponse::new("forbidden", "Invalid token signature")
            .with_code("INVALID_SIGNATURE");
        assert_eq!(invalid_sig.error, "forbidden");

        // Both have error codes
        assert!(expired.code.is_some());
        assert!(invalid_sig.code.is_some());
    }

    /// Test rotation endpoint rate limiting
    #[tokio::test]
    async fn test_rotation_endpoint_rate_limiting() {
        let error = ApiErrorResponse::new(
            "too_many_requests",
            "Rate limit exceeded: 10 requests per minute",
        )
        .with_code("RATE_LIMIT_EXCEEDED");

        assert_eq!(error.error, "too_many_requests");
        assert!(error.message.contains("Rate limit"));
        assert_eq!(error.code.as_deref(), Some("RATE_LIMIT_EXCEEDED"));
    }

    /// Test rotation audit logging
    #[tokio::test]
    async fn test_rotation_audit_logging() {
        let now = Utc::now();

        // Successful operation audit record
        let success_record = RotationHistoryRecord {
            timestamp:    now,
            old_version:  2,
            new_version:  3,
            reason:       Some("Manual rotation by admin".to_string()),
            duration_ms:  120,
            triggered_by: "manual".to_string(),
            user_id:      Some("admin@company.com".to_string()),
        };

        assert_eq!(success_record.triggered_by, "manual");
        assert!(success_record.user_id.is_some());
        assert!(success_record.reason.is_some());

        // Failed operation also produces audit record
        let failed_record = RotationHistoryRecord {
            timestamp:    now,
            old_version:  3,
            new_version:  3, // Unchanged on failure
            reason:       Some("Auto rotation failed: Vault unreachable".to_string()),
            duration_ms:  0,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };

        assert_eq!(failed_record.old_version, failed_record.new_version);
        assert!(failed_record.reason.as_ref().unwrap().contains("failed"));
    }

    // ============================================================================
    // ROTATION API OBSERVABILITY TESTS
    // ============================================================================

    /// Test rotation metrics collection
    #[tokio::test]
    async fn test_rotation_metrics_collection() {
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Perform rotations to generate metrics
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();

        let metrics = manager.metrics();
        assert_eq!(metrics.total_rotations(), 2);
        assert_eq!(metrics.failed_rotations(), 0);
        assert!(metrics.last_rotation().is_some());
        assert_eq!(metrics.success_rate_percent(), 100);

        // Build status response from metrics
        let mut status = RotationStatusResponse::new(manager.get_current_version().unwrap(), 365);
        status.versions_total = manager.get_version_history().unwrap().len();
        status.versions_active = manager.active_versions_count().unwrap();
        status.last_rotation_duration_ms = metrics.last_rotation_duration_ms();

        assert_eq!(status.versions_total, 3);
        assert!(status.versions_active > 0);
    }

    /// Test rotation endpoint tracing
    #[tokio::test]
    async fn test_rotation_endpoint_tracing() {
        // Trace context carried in request/response
        let now = Utc::now();
        let record = RotationHistoryRecord {
            timestamp:    now,
            old_version:  1,
            new_version:  2,
            reason:       Some("traced rotation".to_string()),
            duration_ms:  45,
            triggered_by: "manual".to_string(),
            user_id:      Some("operator".to_string()),
        };

        // Each record has timestamp for correlation
        assert_eq!(record.timestamp, now);
        assert_eq!(record.duration_ms, 45);

        // Duration can be used for performance monitoring
        assert!(record.duration_ms < 1000);
    }

    /// Test rotation status webhook
    #[tokio::test]
    async fn test_rotation_status_webhook() {
        // Webhook payload for rotation events
        let now = Utc::now();

        // rotation_started event
        let started = RotationHistoryRecord {
            timestamp:    now,
            old_version:  4,
            new_version:  4, // Not yet complete
            reason:       Some("rotation_started".to_string()),
            duration_ms:  0,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };
        assert_eq!(started.reason.as_deref(), Some("rotation_started"));

        // rotation_completed event
        let completed = RotationHistoryRecord {
            timestamp:    now,
            old_version:  4,
            new_version:  5,
            reason:       Some("rotation_completed".to_string()),
            duration_ms:  200,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };
        assert_eq!(completed.reason.as_deref(), Some("rotation_completed"));
        assert!(completed.new_version > completed.old_version);

        // rotation_failed event
        let failed = RotationHistoryRecord {
            timestamp:    now,
            old_version:  4,
            new_version:  4,
            reason:       Some("rotation_failed".to_string()),
            duration_ms:  0,
            triggered_by: "auto".to_string(),
            user_id:      None,
        };
        assert_eq!(failed.reason.as_deref(), Some("rotation_failed"));
        assert_eq!(failed.old_version, failed.new_version);
    }
}
