//! Rotation dashboard, metrics visualization, compliance monitoring,
//! and alert/notification systems for rotation management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Dashboard overview with all keys status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardOverview {
    /// Total number of encryption keys
    pub total_keys:      usize,
    /// Keys with <70% TTL consumed
    pub healthy_keys:    usize,
    /// Keys with 70-85% TTL consumed
    pub warning_keys:    usize,
    /// Keys with 85%+ TTL consumed
    pub urgent_keys:     usize,
    /// Average TTL consumption percentage across all keys
    pub avg_ttl_percent: u32,
    /// Overall system health status
    pub system_health:   String,
}

impl DashboardOverview {
    /// Create new dashboard overview
    pub fn new() -> Self {
        Self {
            total_keys:      0,
            healthy_keys:    0,
            warning_keys:    0,
            urgent_keys:     0,
            avg_ttl_percent: 0,
            system_health:   "healthy".to_string(),
        }
    }

    /// Calculate system health status
    pub fn recalculate_health(&mut self) {
        if self.urgent_keys > 0 {
            self.system_health = "critical".to_string();
        } else if self.warning_keys > 0 {
            self.system_health = "warning".to_string();
        } else {
            self.system_health = "healthy".to_string();
        }
    }
}

impl Default for DashboardOverview {
    fn default() -> Self {
        Self::new()
    }
}

/// Key status card for dashboard display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatusCard {
    /// Key identifier
    pub key_id:             String,
    /// Current active version
    pub current_version:    u16,
    /// TTL consumption percentage (0-100)
    pub ttl_percent:        u32,
    /// Status level: "healthy", "warning", "urgent", "overdue"
    pub status:             String,
    /// Last rotation timestamp
    pub last_rotation:      Option<DateTime<Utc>>,
    /// Estimated next rotation time
    pub next_rotation:      Option<DateTime<Utc>>,
    /// Total versions count
    pub versions_count:     usize,
    /// Urgency score (0-100)
    pub urgency_score:      u32,
    /// Recommended action
    pub recommended_action: String,
}

impl KeyStatusCard {
    /// Create new key status card
    pub fn new(key_id: impl Into<String>, current_version: u16, ttl_percent: u32) -> Self {
        let (status, urgency_score, recommended_action) = match ttl_percent {
            0..=40 => ("healthy".to_string(), 10, "Monitor key health".to_string()),
            41..=70 => ("healthy".to_string(), 30, "Monitor key health".to_string()),
            71..=85 => ("warning".to_string(), 60, "Prepare for upcoming rotation".to_string()),
            86..=99 => ("urgent".to_string(), 85, "Trigger manual rotation".to_string()),
            _ => ("overdue".to_string(), 100, "CRITICAL: Rotate immediately".to_string()),
        };

        Self {
            key_id: key_id.into(),
            current_version,
            ttl_percent,
            status,
            last_rotation: None,
            next_rotation: None,
            versions_count: 1,
            urgency_score,
            recommended_action,
        }
    }
}

/// Rotation metrics for time series visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationMetricsPoint {
    /// Timestamp for this data point
    pub timestamp:                DateTime<Utc>,
    /// Total rotations count
    pub rotations_total:          u64,
    /// Manual rotations count
    pub rotations_manual:         u64,
    /// Auto-refresh rotations count
    pub rotations_auto:           u64,
    /// Average rotation duration in milliseconds
    pub rotation_duration_avg_ms: u64,
    /// Rotation success rate percentage
    pub success_rate_percent:     u32,
}

impl RotationMetricsPoint {
    /// Create new metrics point
    pub fn new(timestamp: DateTime<Utc>) -> Self {
        Self {
            timestamp,
            rotations_total: 0,
            rotations_manual: 0,
            rotations_auto: 0,
            rotation_duration_avg_ms: 0,
            success_rate_percent: 100,
        }
    }
}

/// Time series data for metrics charts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsTimeSeries {
    /// Period: "1d", "7d", "30d", "90d"
    pub period:      String,
    /// Data points
    pub data_points: Vec<RotationMetricsPoint>,
}

impl MetricsTimeSeries {
    /// Create new time series
    pub fn new(period: impl Into<String>) -> Self {
        Self {
            period:      period.into(),
            data_points: Vec::new(),
        }
    }
}

/// Compliance framework status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComplianceStatus {
    /// All requirements met
    Compliant,
    /// Some requirements not met
    Partial,
    /// Requirements not met
    NonCompliant,
}

impl std::fmt::Display for ComplianceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compliant => write!(f, "compliant"),
            Self::Partial => write!(f, "partial"),
            Self::NonCompliant => write!(f, "non_compliant"),
        }
    }
}

/// Compliance requirement check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceRequirement {
    /// Requirement name
    pub name:    String,
    /// Is requirement met
    pub met:     bool,
    /// Details about requirement
    pub details: String,
}

/// Compliance framework dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceDashboard {
    /// HIPAA compliance status
    pub hipaa:   ComplianceStatus,
    /// PCI-DSS compliance status
    pub pci_dss: ComplianceStatus,
    /// GDPR compliance status
    pub gdpr:    ComplianceStatus,
    /// SOC 2 compliance status
    pub soc2:    ComplianceStatus,
    /// Overall compliance status
    pub overall: ComplianceStatus,
}

impl ComplianceDashboard {
    /// Create new compliance dashboard
    pub fn new() -> Self {
        Self {
            hipaa:   ComplianceStatus::Compliant,
            pci_dss: ComplianceStatus::Compliant,
            gdpr:    ComplianceStatus::Compliant,
            soc2:    ComplianceStatus::Compliant,
            overall: ComplianceStatus::Compliant,
        }
    }

    /// Recalculate overall compliance
    pub fn recalculate_overall(&mut self) {
        // If any is non-compliant, overall is non-compliant
        if matches!(self.hipaa, ComplianceStatus::NonCompliant)
            || matches!(self.pci_dss, ComplianceStatus::NonCompliant)
            || matches!(self.gdpr, ComplianceStatus::NonCompliant)
            || matches!(self.soc2, ComplianceStatus::NonCompliant)
        {
            self.overall = ComplianceStatus::NonCompliant;
        } else if matches!(self.hipaa, ComplianceStatus::Partial)
            || matches!(self.pci_dss, ComplianceStatus::Partial)
            || matches!(self.gdpr, ComplianceStatus::Partial)
            || matches!(self.soc2, ComplianceStatus::Partial)
        {
            self.overall = ComplianceStatus::Partial;
        } else {
            self.overall = ComplianceStatus::Compliant;
        }
    }
}

impl Default for ComplianceDashboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Alert severity level
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    /// Informational
    Info,
    /// Warning
    Warning,
    /// Error
    Error,
    /// Critical
    Critical,
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Error => write!(f, "error"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// Alert notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert ID
    pub id:           String,
    /// Alert type
    pub alert_type:   String,
    /// Severity level
    pub severity:     AlertSeverity,
    /// Affected key ID
    pub key_id:       Option<String>,
    /// Alert timestamp
    pub timestamp:    DateTime<Utc>,
    /// Alert message
    pub message:      String,
    /// Recommended action
    pub action:       Option<String>,
    /// Is alert read/acknowledged
    pub acknowledged: bool,
}

impl Alert {
    /// Create new alert
    pub fn new(
        alert_type: impl Into<String>,
        severity: AlertSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            alert_type: alert_type.into(),
            severity,
            key_id: None,
            timestamp: Utc::now(),
            message: message.into(),
            action: None,
            acknowledged: false,
        }
    }

    /// Set affected key
    pub fn with_key_id(mut self, key_id: impl Into<String>) -> Self {
        self.key_id = Some(key_id.into());
        self
    }

    /// Set recommended action
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Mark alert as acknowledged
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

/// Dashboard alerts widget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertsWidget {
    /// Active (unacknowledged) alerts
    pub active_alerts:        Vec<Alert>,
    /// Historical alerts
    pub historical_alerts:    Vec<Alert>,
    /// Total unacknowledged count
    pub unacknowledged_count: usize,
}

impl AlertsWidget {
    /// Create new alerts widget
    pub fn new() -> Self {
        Self {
            active_alerts:        Vec::new(),
            historical_alerts:    Vec::new(),
            unacknowledged_count: 0,
        }
    }

    /// Add alert
    pub fn add_alert(&mut self, alert: Alert) {
        if !alert.acknowledged {
            self.unacknowledged_count += 1;
        }
        self.active_alerts.push(alert);
    }

    /// Clear old alerts (older than N days)
    pub fn clear_old_alerts(&mut self, days: i64) {
        let cutoff = Utc::now() - chrono::Duration::days(days);
        self.active_alerts.retain(|a| a.timestamp > cutoff || !a.acknowledged);
    }
}

impl Default for AlertsWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// Trend analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    /// Rotation frequency trend: "increasing", "stable", "decreasing"
    pub rotation_frequency: String,
    /// Duration trend: "increasing", "stable", "decreasing"
    pub rotation_duration:  String,
    /// Failure rate trend: "increasing", "stable", "decreasing"
    pub failure_rate:       String,
    /// Compliance trend: "improving", "stable", "degrading"
    pub compliance_status:  String,
}

impl TrendAnalysis {
    /// Create new trend analysis
    pub fn new() -> Self {
        Self {
            rotation_frequency: "stable".to_string(),
            rotation_duration:  "stable".to_string(),
            failure_rate:       "stable".to_string(),
            compliance_status:  "stable".to_string(),
        }
    }
}

impl Default for TrendAnalysis {
    fn default() -> Self {
        Self::new()
    }
}

/// Dashboard export format options
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ExportFormat {
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// PDF report format
    Pdf,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json => write!(f, "json"),
            Self::Csv => write!(f, "csv"),
            Self::Pdf => write!(f, "pdf"),
        }
    }
}

/// Dashboard snapshot for export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    /// Snapshot timestamp
    pub snapshot_time:       DateTime<Utc>,
    /// Dashboard overview at this time
    pub overview:            DashboardOverview,
    /// Key status cards
    pub key_cards:           Vec<KeyStatusCard>,
    /// Alert summary
    pub active_alerts_count: usize,
    /// Compliance status
    pub compliance:          ComplianceDashboard,
}

impl DashboardSnapshot {
    /// Create new dashboard snapshot
    pub fn new(
        overview: DashboardOverview,
        key_cards: Vec<KeyStatusCard>,
        compliance: ComplianceDashboard,
        active_alerts_count: usize,
    ) -> Self {
        Self {
            snapshot_time: Utc::now(),
            overview,
            key_cards,
            active_alerts_count,
            compliance,
        }
    }

    /// Get urgency summary (count of keys by status)
    pub fn urgency_summary(&self) -> (usize, usize, usize, usize) {
        let mut healthy = 0;
        let mut warning = 0;
        let mut urgent = 0;
        let mut overdue = 0;

        for card in &self.key_cards {
            match card.status.as_str() {
                "healthy" => healthy += 1,
                "warning" => warning += 1,
                "urgent" => urgent += 1,
                "overdue" => overdue += 1,
                _ => {},
            }
        }

        (healthy, warning, urgent, overdue)
    }

    /// Calculate average urgency score across all keys
    pub fn average_urgency_score(&self) -> u32 {
        if self.key_cards.is_empty() {
            return 0;
        }

        let total: u32 = self.key_cards.iter().map(|c| c.urgency_score).sum();
        total / self.key_cards.len() as u32
    }
}

/// Alert filter for querying and filtering alerts
#[derive(Debug, Clone)]
pub struct AlertFilter {
    /// Filter by severity
    pub severity:     Option<AlertSeverity>,
    /// Filter by alert type
    pub alert_type:   Option<String>,
    /// Filter by key ID
    pub key_id:       Option<String>,
    /// Filter acknowledged status
    pub acknowledged: Option<bool>,
}

impl AlertFilter {
    /// Create new alert filter
    pub fn new() -> Self {
        Self {
            severity:     None,
            alert_type:   None,
            key_id:       None,
            acknowledged: None,
        }
    }

    /// Filter alerts based on criteria
    pub fn apply(&self, alerts: &[Alert]) -> Vec<Alert> {
        alerts
            .iter()
            .filter(|alert| {
                if let Some(sev) = self.severity {
                    if alert.severity != sev {
                        return false;
                    }
                }
                if let Some(ref alert_type) = self.alert_type {
                    if alert.alert_type != *alert_type {
                        return false;
                    }
                }
                if let Some(ref key_id) = self.key_id {
                    if alert.key_id.as_ref() != Some(key_id) {
                        return false;
                    }
                }
                if let Some(ack) = self.acknowledged {
                    if alert.acknowledged != ack {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }
}

impl Default for AlertFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Compliance requirement checker
#[derive(Debug, Clone)]
pub struct ComplianceChecker {
    /// Framework name
    pub framework:                     String,
    /// Required TTL in days
    pub required_ttl_days:             u32,
    /// Required check interval in hours
    pub required_check_interval_hours: u32,
}

impl ComplianceChecker {
    /// Create checker for HIPAA compliance
    pub fn hipaa() -> Self {
        Self {
            framework:                     "HIPAA".to_string(),
            required_ttl_days:             365,
            required_check_interval_hours: 24,
        }
    }

    /// Create checker for PCI-DSS compliance
    pub fn pci_dss() -> Self {
        Self {
            framework:                     "PCI-DSS".to_string(),
            required_ttl_days:             365,
            required_check_interval_hours: 24,
        }
    }

    /// Create checker for GDPR compliance
    pub fn gdpr() -> Self {
        Self {
            framework:                     "GDPR".to_string(),
            required_ttl_days:             90,
            required_check_interval_hours: 24,
        }
    }

    /// Create checker for SOC 2 compliance
    pub fn soc2() -> Self {
        Self {
            framework:                     "SOC 2".to_string(),
            required_ttl_days:             365,
            required_check_interval_hours: 24,
        }
    }

    /// Check if card meets compliance requirements
    pub fn check_compliance(&self, card: &KeyStatusCard) -> bool {
        // For compliance, key should not be urgent or overdue
        !matches!(card.status.as_str(), "urgent" | "overdue")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dashboard_overview_creation() {
        let overview = DashboardOverview::new();
        assert_eq!(overview.total_keys, 0);
        assert_eq!(overview.system_health, "healthy");
    }

    #[test]
    fn test_dashboard_overview_health_critical() {
        let mut overview = DashboardOverview {
            total_keys:      1,
            healthy_keys:    0,
            warning_keys:    0,
            urgent_keys:     1,
            avg_ttl_percent: 95,
            system_health:   "healthy".to_string(),
        };
        overview.recalculate_health();
        assert_eq!(overview.system_health, "critical");
    }

    #[test]
    fn test_key_status_card_healthy() {
        let card = KeyStatusCard::new("primary", 1, 50);
        assert_eq!(card.status, "healthy");
        assert_eq!(card.urgency_score, 30);
    }

    #[test]
    fn test_key_status_card_urgent() {
        let card = KeyStatusCard::new("primary", 1, 90);
        assert_eq!(card.status, "urgent");
        assert_eq!(card.urgency_score, 85);
    }

    #[test]
    fn test_key_status_card_overdue() {
        let card = KeyStatusCard::new("primary", 1, 105);
        assert_eq!(card.status, "overdue");
        assert_eq!(card.urgency_score, 100);
    }

    #[test]
    fn test_rotation_metrics_point_creation() {
        let now = Utc::now();
        let point = RotationMetricsPoint::new(now);
        assert_eq!(point.timestamp, now);
        assert_eq!(point.rotations_total, 0);
        assert_eq!(point.success_rate_percent, 100);
    }

    #[test]
    fn test_metrics_time_series_creation() {
        let series = MetricsTimeSeries::new("30d");
        assert_eq!(series.period, "30d");
        assert_eq!(series.data_points.len(), 0);
    }

    #[test]
    fn test_compliance_dashboard_creation() {
        let dashboard = ComplianceDashboard::new();
        assert_eq!(dashboard.overall, ComplianceStatus::Compliant);
    }

    #[test]
    fn test_compliance_dashboard_recalculate_non_compliant() {
        let mut dashboard = ComplianceDashboard::new();
        dashboard.hipaa = ComplianceStatus::NonCompliant;
        dashboard.recalculate_overall();
        assert_eq!(dashboard.overall, ComplianceStatus::NonCompliant);
    }

    #[test]
    fn test_compliance_dashboard_recalculate_partial() {
        let mut dashboard = ComplianceDashboard::new();
        dashboard.gdpr = ComplianceStatus::Partial;
        dashboard.recalculate_overall();
        assert_eq!(dashboard.overall, ComplianceStatus::Partial);
    }

    #[test]
    fn test_alert_creation() {
        let alert = Alert::new("rotation_failed", AlertSeverity::Critical, "Rotation failed");
        assert_eq!(alert.alert_type, "rotation_failed");
        assert_eq!(alert.severity, AlertSeverity::Critical);
        assert!(!alert.acknowledged);
    }

    #[test]
    fn test_alert_with_key_id() {
        let alert = Alert::new("rotation_failed", AlertSeverity::Error, "Rotation failed")
            .with_key_id("primary");
        assert_eq!(alert.key_id, Some("primary".to_string()));
    }

    #[test]
    fn test_alert_acknowledge() {
        let mut alert = Alert::new("rotation_failed", AlertSeverity::Error, "Rotation failed");
        assert!(!alert.acknowledged);
        alert.acknowledge();
        assert!(alert.acknowledged);
    }

    #[test]
    fn test_alerts_widget_creation() {
        let widget = AlertsWidget::new();
        assert_eq!(widget.active_alerts.len(), 0);
        assert_eq!(widget.unacknowledged_count, 0);
    }

    #[test]
    fn test_alerts_widget_add_alert() {
        let mut widget = AlertsWidget::new();
        let alert = Alert::new("rotation_failed", AlertSeverity::Error, "Failed");
        widget.add_alert(alert);
        assert_eq!(widget.active_alerts.len(), 1);
        assert_eq!(widget.unacknowledged_count, 1);
    }

    #[test]
    fn test_trend_analysis_creation() {
        let trend = TrendAnalysis::new();
        assert_eq!(trend.rotation_frequency, "stable");
        assert_eq!(trend.compliance_status, "stable");
    }

    #[test]
    fn test_dashboard_snapshot_creation() {
        let overview = DashboardOverview::new();
        let key_cards = vec![
            KeyStatusCard::new("primary", 1, 50),
            KeyStatusCard::new("secondary", 1, 80),
        ];
        let compliance = ComplianceDashboard::new();
        let snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 0);
        assert_eq!(snapshot.key_cards.len(), 2);
    }

    #[test]
    fn test_dashboard_snapshot_urgency_summary() {
        let overview = DashboardOverview::new();
        let key_cards = vec![
            KeyStatusCard::new("primary", 1, 30),    // healthy
            KeyStatusCard::new("secondary", 1, 50),  // healthy
            KeyStatusCard::new("tertiary", 1, 75),   // warning
            KeyStatusCard::new("quaternary", 1, 90), // urgent
        ];
        let compliance = ComplianceDashboard::new();
        let snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 0);
        let (healthy, warning, urgent, overdue) = snapshot.urgency_summary();
        assert_eq!(healthy, 2);
        assert_eq!(warning, 1);
        assert_eq!(urgent, 1);
        assert_eq!(overdue, 0);
    }

    #[test]
    fn test_dashboard_snapshot_average_urgency_score() {
        let overview = DashboardOverview::new();
        let key_cards = vec![
            KeyStatusCard::new("primary", 1, 50),   // urgency_score: 30
            KeyStatusCard::new("secondary", 1, 80), // urgency_score: 60
        ];
        let compliance = ComplianceDashboard::new();
        let snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 0);
        assert_eq!(snapshot.average_urgency_score(), 45);
    }

    #[test]
    fn test_alert_filter_by_severity() {
        let mut filter = AlertFilter::new();
        filter.severity = Some(AlertSeverity::Critical);

        let alerts = vec![
            Alert::new("rotation_failed", AlertSeverity::Critical, "Failed"),
            Alert::new("rotation_warning", AlertSeverity::Warning, "Warning"),
        ];

        let filtered = filter.apply(&alerts);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].severity, AlertSeverity::Critical);
    }

    #[test]
    fn test_alert_filter_by_acknowledged() {
        let mut filter = AlertFilter::new();
        filter.acknowledged = Some(false);

        let alert1 = Alert::new("rotation_failed", AlertSeverity::Error, "Failed");
        let mut alert2 = Alert::new("rotation_warning", AlertSeverity::Warning, "Warning");
        alert2.acknowledge();

        let alerts = vec![alert1, alert2];
        let filtered = filter.apply(&alerts);
        assert_eq!(filtered.len(), 1);
        assert!(!filtered[0].acknowledged);
    }

    #[test]
    fn test_compliance_checker_hipaa() {
        let checker = ComplianceChecker::hipaa();
        assert_eq!(checker.framework, "HIPAA");
        assert_eq!(checker.required_ttl_days, 365);
    }

    #[test]
    fn test_compliance_checker_gdpr() {
        let checker = ComplianceChecker::gdpr();
        assert_eq!(checker.framework, "GDPR");
        assert_eq!(checker.required_ttl_days, 90);
    }

    #[test]
    fn test_compliance_checker_validates_key_status() {
        let checker = ComplianceChecker::hipaa();
        let healthy_card = KeyStatusCard::new("primary", 1, 50);
        let urgent_card = KeyStatusCard::new("primary", 1, 90);

        assert!(checker.check_compliance(&healthy_card));
        assert!(!checker.check_compliance(&urgent_card));
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(ExportFormat::Json.to_string(), "json");
        assert_eq!(ExportFormat::Csv.to_string(), "csv");
        assert_eq!(ExportFormat::Pdf.to_string(), "pdf");
    }
}
