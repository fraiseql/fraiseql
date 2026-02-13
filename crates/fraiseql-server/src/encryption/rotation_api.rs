// Phase 12.4 Cycle 3: Rotation API Endpoints - GREEN
//! Credential rotation REST API endpoints for status, manual rotation,
//! history retrieval, and configuration management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Rotation status levels
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RotationStatus {
    /// Less than 70% TTL consumed
    Healthy,
    /// 70-85% TTL consumed
    ExpiringSoon,
    /// 85%+ TTL consumed or refresh triggered
    NeedsRotation,
    /// Over 100% TTL consumed
    Overdue,
}

impl std::fmt::Display for RotationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::ExpiringSoon => write!(f, "expiring_soon"),
            Self::NeedsRotation => write!(f, "needs_rotation"),
            Self::Overdue => write!(f, "overdue"),
        }
    }
}

/// Rotation status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationStatusResponse {
    /// Current active version number
    pub current_version:           u16,
    /// TTL for each version in days
    pub ttl_days:                  u32,
    /// Last rotation timestamp
    pub last_rotation:             Option<DateTime<Utc>>,
    /// Estimated next rotation time
    pub next_rotation:             Option<DateTime<Utc>>,
    /// Current status level
    pub status:                    RotationStatus,
    /// Is automatic refresh enabled
    pub auto_refresh_enabled:      bool,
    /// Total versions for this key
    pub versions_total:            usize,
    /// Active versions count
    pub versions_active:           usize,
    /// Expired versions count
    pub versions_expired:          usize,
    /// Last rotation duration in milliseconds
    pub last_rotation_duration_ms: u64,
    /// Total auto-refresh checks performed
    pub auto_refresh_checks:       u64,
}

impl RotationStatusResponse {
    /// Create new rotation status response
    pub fn new(current_version: u16, ttl_days: u32) -> Self {
        Self {
            current_version,
            ttl_days,
            last_rotation: None,
            next_rotation: None,
            status: RotationStatus::Healthy,
            auto_refresh_enabled: true,
            versions_total: 1,
            versions_active: 1,
            versions_expired: 0,
            last_rotation_duration_ms: 0,
            auto_refresh_checks: 0,
        }
    }

    /// Set last rotation timestamp
    pub fn with_last_rotation(mut self, timestamp: DateTime<Utc>) -> Self {
        self.last_rotation = Some(timestamp);
        self
    }

    /// Set next rotation timestamp
    pub fn with_next_rotation(mut self, timestamp: DateTime<Utc>) -> Self {
        self.next_rotation = Some(timestamp);
        self
    }

    /// Set status level
    pub fn with_status(mut self, status: RotationStatus) -> Self {
        self.status = status;
        self
    }
}

/// Manual rotation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualRotationRequest {
    /// Key ID to rotate (optional, defaults to primary)
    pub key_id:  Option<String>,
    /// Reason for rotation
    pub reason:  Option<String>,
    /// Dry-run mode (validate without applying)
    pub dry_run: Option<bool>,
}

/// Manual rotation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManualRotationResponse {
    /// New version number
    pub new_version: u16,
    /// Old version number
    pub old_version: u16,
    /// Rotation status: "success" or "failed"
    pub status:      String,
    /// Rotation duration in milliseconds
    pub duration_ms: u64,
    /// Error message if failed
    pub error:       Option<String>,
}

impl ManualRotationResponse {
    /// Create successful rotation response
    pub fn success(old_version: u16, new_version: u16, duration_ms: u64) -> Self {
        Self {
            new_version,
            old_version,
            status: "success".to_string(),
            duration_ms,
            error: None,
        }
    }

    /// Create failed rotation response
    pub fn failure(old_version: u16, error: impl Into<String>) -> Self {
        Self {
            new_version: old_version,
            old_version,
            status: "failed".to_string(),
            duration_ms: 0,
            error: Some(error.into()),
        }
    }
}

/// Rotation history record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationHistoryRecord {
    /// When rotation occurred
    pub timestamp:    DateTime<Utc>,
    /// Previous version
    pub old_version:  u16,
    /// New version
    pub new_version:  u16,
    /// Rotation reason
    pub reason:       Option<String>,
    /// Operation duration in milliseconds
    pub duration_ms:  u64,
    /// "auto" or "manual"
    pub triggered_by: String,
    /// User ID if manually triggered
    pub user_id:      Option<String>,
}

/// Rotation history response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationHistoryResponse {
    /// Pagination: total count
    pub total_count: usize,
    /// Pagination: offset used
    pub offset:      usize,
    /// Pagination: limit used
    pub limit:       usize,
    /// History records
    pub records:     Vec<RotationHistoryRecord>,
}

impl RotationHistoryResponse {
    /// Create new history response
    pub fn new(offset: usize, limit: usize) -> Self {
        Self {
            total_count: 0,
            offset,
            limit,
            records: Vec::new(),
        }
    }

    /// Add record to history
    pub fn with_record(mut self, record: RotationHistoryRecord) -> Self {
        self.records.push(record);
        self
    }

    /// Set total count
    pub fn with_total_count(mut self, count: usize) -> Self {
        self.total_count = count;
        self
    }
}

/// Rotation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfigResponse {
    /// Is auto-refresh enabled
    pub auto_refresh_enabled: bool,
    /// Check interval in hours
    pub refresh_check_interval_hours: u32,
    /// Refresh threshold percentage
    pub refresh_threshold_percent: u32,
    /// TTL in days
    pub ttl_days: u32,
    /// Quiet hours start (0-23, None = disabled)
    pub quiet_hours_start: Option<u32>,
    /// Quiet hours end (0-23)
    pub quiet_hours_end: Option<u32>,
    /// Manual rotation cooldown in minutes
    pub manual_rotation_cooldown_minutes: u32,
}

impl RotationConfigResponse {
    /// Create default config
    pub fn default() -> Self {
        Self {
            auto_refresh_enabled: true,
            refresh_check_interval_hours: 24,
            refresh_threshold_percent: 80,
            ttl_days: 365,
            quiet_hours_start: None,
            quiet_hours_end: None,
            manual_rotation_cooldown_minutes: 60,
        }
    }
}

/// Rotation config update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfigUpdateRequest {
    /// Is auto-refresh enabled
    pub auto_refresh_enabled:         Option<bool>,
    /// Check interval in hours
    pub refresh_check_interval_hours: Option<u32>,
    /// Refresh threshold percentage
    pub refresh_threshold_percent:    Option<u32>,
    /// TTL in days
    pub ttl_days:                     Option<u32>,
    /// Quiet hours start (0-23)
    pub quiet_hours_start:            Option<u32>,
    /// Quiet hours end (0-23)
    pub quiet_hours_end:              Option<u32>,
}

impl RotationConfigUpdateRequest {
    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if let Some(threshold) = self.refresh_threshold_percent {
            if threshold < 1 || threshold > 99 {
                return Err("Threshold must be 1-99".to_string());
            }
        }

        if let Some(ttl) = self.ttl_days {
            if ttl < 1 || ttl > 365 {
                return Err("TTL must be 1-365 days".to_string());
            }
        }

        if let Some(interval) = self.refresh_check_interval_hours {
            if interval < 1 || interval > 720 {
                return Err("Interval must be 1-720 hours".to_string());
            }
        }

        Ok(())
    }
}

/// Rotation schedule types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleType {
    /// Manual rotation only
    Manual,
    /// Cron-based schedule
    Cron,
    /// Interval-based (every N days)
    Interval,
}

impl std::fmt::Display for ScheduleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::Cron => write!(f, "cron"),
            Self::Interval => write!(f, "interval"),
        }
    }
}

/// Rotation schedule response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationScheduleResponse {
    /// Schedule type
    pub schedule_type:       ScheduleType,
    /// Schedule value (cron expression or interval in days)
    pub schedule_value:      String,
    /// Next scheduled rotation time
    pub next_scheduled_time: Option<DateTime<Utc>>,
    /// Is schedule enabled
    pub enabled:             bool,
}

impl RotationScheduleResponse {
    /// Create manual schedule (default)
    pub fn manual() -> Self {
        Self {
            schedule_type:       ScheduleType::Manual,
            schedule_value:      "manual".to_string(),
            next_scheduled_time: None,
            enabled:             false,
        }
    }

    /// Create cron schedule
    pub fn cron(expression: impl Into<String>, next_time: DateTime<Utc>) -> Self {
        Self {
            schedule_type:       ScheduleType::Cron,
            schedule_value:      expression.into(),
            next_scheduled_time: Some(next_time),
            enabled:             true,
        }
    }

    /// Create interval schedule
    pub fn interval(days: u32, next_time: DateTime<Utc>) -> Self {
        Self {
            schedule_type:       ScheduleType::Interval,
            schedule_value:      format!("{} days", days),
            next_scheduled_time: Some(next_time),
            enabled:             true,
        }
    }
}

/// Rotation schedule update request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationScheduleUpdateRequest {
    /// Schedule type
    pub schedule_type:  ScheduleType,
    /// Schedule value
    pub schedule_value: String,
}

/// Test schedule response (next N times)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestScheduleResponse {
    /// Validation status
    pub valid:      bool,
    /// Error message if invalid
    pub error:      Option<String>,
    /// Next 10 scheduled times
    pub next_times: Vec<DateTime<Utc>>,
}

impl TestScheduleResponse {
    /// Create valid schedule test
    pub fn valid(next_times: Vec<DateTime<Utc>>) -> Self {
        Self {
            valid: true,
            error: None,
            next_times,
        }
    }

    /// Create invalid schedule test
    pub fn invalid(error: impl Into<String>) -> Self {
        Self {
            valid:      false,
            error:      Some(error.into()),
            next_times: Vec::new(),
        }
    }
}

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    /// Error code
    pub error:   String,
    /// Error message
    pub message: String,
    /// Additional error details
    pub code:    Option<String>,
}

impl ApiErrorResponse {
    /// Create new error response
    pub fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error:   error.into(),
            message: message.into(),
            code:    None,
        }
    }

    /// Add error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }
}

// ========== REFACTOR ENHANCEMENTS ==========

/// Configuration preset type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigPreset {
    /// HIPAA compliance preset
    Hipaa,
    /// PCI-DSS compliance preset
    PciDss,
    /// GDPR compliance preset
    Gdpr,
    /// SOC 2 compliance preset
    Soc2,
    /// Custom preset
    Custom,
}

impl std::fmt::Display for ConfigPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hipaa => write!(f, "hipaa"),
            Self::PciDss => write!(f, "pci_dss"),
            Self::Gdpr => write!(f, "gdpr"),
            Self::Soc2 => write!(f, "soc2"),
            Self::Custom => write!(f, "custom"),
        }
    }
}

impl ConfigPreset {
    /// Get default config for this preset
    pub fn get_config(&self) -> RotationConfigResponse {
        match self {
            Self::Hipaa => RotationConfigResponse {
                auto_refresh_enabled: true,
                refresh_check_interval_hours: 24,
                refresh_threshold_percent: 80,
                ttl_days: 365,
                quiet_hours_start: Some(2),
                quiet_hours_end: Some(4),
                manual_rotation_cooldown_minutes: 60,
            },
            Self::PciDss => RotationConfigResponse {
                auto_refresh_enabled: true,
                refresh_check_interval_hours: 24,
                refresh_threshold_percent: 80,
                ttl_days: 365,
                quiet_hours_start: Some(2),
                quiet_hours_end: Some(4),
                manual_rotation_cooldown_minutes: 60,
            },
            Self::Gdpr => RotationConfigResponse {
                auto_refresh_enabled: true,
                refresh_check_interval_hours: 24,
                refresh_threshold_percent: 75,
                ttl_days: 90,
                quiet_hours_start: None,
                quiet_hours_end: None,
                manual_rotation_cooldown_minutes: 30,
            },
            Self::Soc2 => RotationConfigResponse {
                auto_refresh_enabled: true,
                refresh_check_interval_hours: 24,
                refresh_threshold_percent: 80,
                ttl_days: 365,
                quiet_hours_start: Some(2),
                quiet_hours_end: Some(4),
                manual_rotation_cooldown_minutes: 60,
            },
            Self::Custom => RotationConfigResponse::default(),
        }
    }
}

/// Compliance preset list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompliancePresetsResponse {
    /// Available presets
    pub presets: Vec<PresetInfo>,
}

/// Preset information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetInfo {
    /// Preset name
    pub name:                 ConfigPreset,
    /// Description
    pub description:          String,
    /// TTL days for this preset
    pub ttl_days:             u32,
    /// Refresh check interval hours
    pub check_interval_hours: u32,
    /// Refresh threshold percentage
    pub threshold_percent:    u32,
}

impl CompliancePresetsResponse {
    /// Create default presets response
    pub fn default() -> Self {
        Self {
            presets: vec![
                PresetInfo {
                    name:                 ConfigPreset::Hipaa,
                    description:          "HIPAA compliance requirements".to_string(),
                    ttl_days:             365,
                    check_interval_hours: 24,
                    threshold_percent:    80,
                },
                PresetInfo {
                    name:                 ConfigPreset::PciDss,
                    description:          "PCI-DSS compliance requirements".to_string(),
                    ttl_days:             365,
                    check_interval_hours: 24,
                    threshold_percent:    80,
                },
                PresetInfo {
                    name:                 ConfigPreset::Gdpr,
                    description:          "GDPR data minimization requirements".to_string(),
                    ttl_days:             90,
                    check_interval_hours: 24,
                    threshold_percent:    75,
                },
                PresetInfo {
                    name:                 ConfigPreset::Soc2,
                    description:          "SOC 2 compliance requirements".to_string(),
                    ttl_days:             365,
                    check_interval_hours: 24,
                    threshold_percent:    80,
                },
            ],
        }
    }
}

/// Query parameters for history endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationHistoryQuery {
    /// Limit (default 100, max 1000)
    pub limit:        Option<usize>,
    /// Offset for pagination
    pub offset:       Option<usize>,
    /// From date filter (ISO format)
    pub from:         Option<String>,
    /// To date filter (ISO format)
    pub to:           Option<String>,
    /// Reason filter
    pub reason:       Option<String>,
    /// Triggered by filter (auto or manual)
    pub triggered_by: Option<String>,
    /// Export format (json, csv, json-lines)
    pub format:       Option<String>,
}

impl RotationHistoryQuery {
    /// Get effective limit (capped at 1000)
    pub fn effective_limit(&self) -> usize {
        self.limit.unwrap_or(100).min(1000)
    }

    /// Get effective offset
    pub fn effective_offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

/// Rotation status display with urgency indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationStatusDisplay {
    /// Status response
    pub status:             RotationStatusResponse,
    /// Urgency score (0-100)
    pub urgency_score:      u32,
    /// Recommended action
    pub recommended_action: String,
}

impl RotationStatusDisplay {
    /// Create new display from status
    pub fn from_status(status: RotationStatusResponse) -> Self {
        // Calculate urgency based on TTL consumed
        let urgency_score = match status.status {
            RotationStatus::Healthy => 10,
            RotationStatus::ExpiringSoon => 50,
            RotationStatus::NeedsRotation => 85,
            RotationStatus::Overdue => 100,
        };

        let recommended_action = match status.status {
            RotationStatus::Healthy => "Monitor key health".to_string(),
            RotationStatus::ExpiringSoon => "Prepare for upcoming rotation".to_string(),
            RotationStatus::NeedsRotation => "Trigger manual rotation immediately".to_string(),
            RotationStatus::Overdue => "CRITICAL: Rotate immediately to prevent expiry".to_string(),
        };

        Self {
            status,
            urgency_score,
            recommended_action,
        }
    }
}

impl RotationStatusResponse {
    /// Convert to display with urgency
    pub fn to_display(&self) -> RotationStatusDisplay {
        RotationStatusDisplay::from_status(self.clone())
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(response.status, "success");
        assert_eq!(response.old_version, 1);
        assert_eq!(response.new_version, 2);
        assert!(response.error.is_none());
    }

    #[test]
    fn test_manual_rotation_response_failure() {
        let response = ManualRotationResponse::failure(1, "Vault error");
        assert_eq!(response.status, "failed");
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
        assert!(update.validate().is_ok());
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
        assert!(update.validate().is_err());
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
        assert!(update.validate().is_err());
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
        let response = TestScheduleResponse::valid(times.clone());
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

    // ========== REFACTOR ENHANCEMENT TESTS ==========

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
}
