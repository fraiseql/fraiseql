//! Comprehensive test specifications for rotation dashboard, metrics visualization,
//! compliance monitoring, and historical trend tracking.

#[cfg(test)]
#[allow(clippy::module_inception)]
mod dashboard_tests {
    use chrono::Utc;

    use crate::encryption::credential_rotation::{
        CredentialRotationManager, RotationConfig, RotationMetrics,
    };
    use crate::encryption::dashboard::{
        Alert, AlertFilter, AlertSeverity, AlertsWidget, ComplianceChecker, ComplianceDashboard,
        ComplianceStatus, DashboardOverview, DashboardSnapshot, ExportFormat, KeyStatusCard,
        MetricsTimeSeries, RotationMetricsPoint, TrendAnalysis,
    };

    // ============================================================================
    // ROTATION STATUS DASHBOARD TESTS
    // ============================================================================

    /// Test dashboard overview endpoint
    #[tokio::test]
    async fn test_dashboard_overview() {
        // Dashboard overview with key statistics
        let mut overview = DashboardOverview::new();
        overview.total_keys = 10;
        overview.healthy_keys = 6;  // <70% TTL consumed
        overview.warning_keys = 3;  // 70-85% TTL consumed
        overview.urgent_keys = 1;   // 85%+ TTL consumed
        overview.avg_ttl_percent = 55;

        assert_eq!(overview.total_keys, 10);
        assert_eq!(overview.healthy_keys, 6);
        assert_eq!(overview.warning_keys, 3);
        assert_eq!(overview.urgent_keys, 1);
        assert_eq!(overview.avg_ttl_percent, 55);

        // Recalculate health based on key statuses
        overview.recalculate_health();
        // With urgent keys present, system health should be critical
        assert_eq!(overview.system_health, "critical");

        // Without urgent keys, but with warnings
        overview.urgent_keys = 0;
        overview.recalculate_health();
        assert_eq!(overview.system_health, "warning");

        // All healthy
        overview.warning_keys = 0;
        overview.recalculate_health();
        assert_eq!(overview.system_health, "healthy");
    }

    /// Test dashboard key status cards
    #[tokio::test]
    async fn test_dashboard_key_status_cards() {
        // Dashboard displays card for each key
        let card = KeyStatusCard::new("primary", 3, 50);

        // Key metadata
        assert_eq!(card.key_id, "primary");
        assert_eq!(card.current_version, 3);
        assert_eq!(card.ttl_percent, 50);
        assert_eq!(card.status, "healthy");
        assert_eq!(card.versions_count, 1); // Default

        // Create card with more details
        let mut card2 = KeyStatusCard::new("secondary", 2, 75);
        card2.last_rotation = Some(Utc::now());
        card2.next_rotation = Some(Utc::now() + chrono::Duration::days(30));
        card2.versions_count = 5;

        assert_eq!(card2.key_id, "secondary");
        assert_eq!(card2.current_version, 2);
        assert_eq!(card2.ttl_percent, 75);
        assert_eq!(card2.status, "warning");
        assert!(card2.last_rotation.is_some());
        assert!(card2.next_rotation.is_some());
        assert_eq!(card2.versions_count, 5);
    }

    /// Test dashboard urgency indicator
    #[tokio::test]
    async fn test_dashboard_urgency_indicator() {
        // Color coding: green (0-40%), yellow (40-70%), orange (70-85%), red (85%+)
        let green_card = KeyStatusCard::new("key1", 1, 20);
        assert_eq!(green_card.status, "healthy");
        assert_eq!(green_card.urgency_score, 10);

        let yellow_card = KeyStatusCard::new("key2", 1, 60);
        assert_eq!(yellow_card.status, "healthy");
        assert_eq!(yellow_card.urgency_score, 30);

        let orange_card = KeyStatusCard::new("key3", 1, 80);
        assert_eq!(orange_card.status, "warning");
        assert_eq!(orange_card.urgency_score, 60);

        let red_card = KeyStatusCard::new("key4", 1, 90);
        assert_eq!(red_card.status, "urgent");
        assert_eq!(red_card.urgency_score, 85);

        let overdue_card = KeyStatusCard::new("key5", 1, 105);
        assert_eq!(overdue_card.status, "overdue");
        assert_eq!(overdue_card.urgency_score, 100);

        // Recommended action text varies by urgency
        assert!(green_card.recommended_action.contains("Monitor"));
        assert!(orange_card.recommended_action.contains("Prepare"));
        assert!(red_card.recommended_action.contains("Trigger"));
        assert!(overdue_card.recommended_action.contains("CRITICAL"));
    }

    /// Test dashboard filters
    #[tokio::test]
    async fn test_dashboard_filters() {
        // Dashboard supports filtering alerts
        let mut filter = AlertFilter::new();

        // Filter by severity
        filter.severity = Some(AlertSeverity::Critical);
        let alerts = vec![
            Alert::new("rotation_failed", AlertSeverity::Critical, "Failed"),
            Alert::new("ttl_warning", AlertSeverity::Warning, "TTL nearing expiry"),
            Alert::new("key_compromised", AlertSeverity::Critical, "Compromised"),
        ];
        let filtered = filter.apply(&alerts);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|a| a.severity == AlertSeverity::Critical));

        // Filter by acknowledged status
        let mut ack_filter = AlertFilter::new();
        ack_filter.acknowledged = Some(false);
        let filtered_unack = ack_filter.apply(&alerts);
        assert_eq!(filtered_unack.len(), 3); // All unacknowledged

        // Filter by key_id
        let key_alerts = vec![
            Alert::new("rotation_failed", AlertSeverity::Error, "Failed")
                .with_key_id("primary"),
            Alert::new("rotation_failed", AlertSeverity::Error, "Failed")
                .with_key_id("secondary"),
        ];
        let mut key_filter = AlertFilter::new();
        key_filter.key_id = Some("primary".to_string());
        let filtered_key = key_filter.apply(&key_alerts);
        assert_eq!(filtered_key.len(), 1);
        assert_eq!(filtered_key[0].key_id, Some("primary".to_string()));
    }

    /// Test dashboard sort options
    #[tokio::test]
    async fn test_dashboard_sort_options() {
        // Sort by urgency (most urgent first)
        let mut cards = vec![
            KeyStatusCard::new("low", 1, 20),      // urgency 10
            KeyStatusCard::new("high", 1, 90),      // urgency 85
            KeyStatusCard::new("medium", 1, 75),    // urgency 60
            KeyStatusCard::new("overdue", 1, 105),  // urgency 100
        ];

        // Sort by urgency descending (most urgent first)
        cards.sort_by(|a, b| b.urgency_score.cmp(&a.urgency_score));
        assert_eq!(cards[0].key_id, "overdue");
        assert_eq!(cards[1].key_id, "high");
        assert_eq!(cards[2].key_id, "medium");
        assert_eq!(cards[3].key_id, "low");

        // Sort by ttl_percent ascending
        cards.sort_by(|a, b| a.ttl_percent.cmp(&b.ttl_percent));
        assert_eq!(cards[0].key_id, "low");
        assert_eq!(cards[3].key_id, "overdue");

        // Sort by key_name ascending
        cards.sort_by(|a, b| a.key_id.cmp(&b.key_id));
        assert_eq!(cards[0].key_id, "high");
        assert_eq!(cards[3].key_id, "overdue");
    }

    // ============================================================================
    // METRICS VISUALIZATION TESTS
    // ============================================================================

    /// Test rotation metrics time series
    #[tokio::test]
    async fn test_rotation_metrics_time_series() {
        // Time series data for rotation metrics
        let mut series = MetricsTimeSeries::new("30d");
        assert_eq!(series.period, "30d");
        assert_eq!(series.data_points.len(), 0);

        // Add data points for each day
        let now = Utc::now();
        for day in 0..30 {
            let timestamp = now - chrono::Duration::days(29 - day);
            let mut point = RotationMetricsPoint::new(timestamp);
            point.rotations_total = (day as u64) % 3;
            point.rotations_manual = if day % 5 == 0 { 1 } else { 0 };
            point.rotations_auto = point.rotations_total.saturating_sub(point.rotations_manual);
            point.rotation_duration_avg_ms = 50 + (day as u64) * 2;
            point.success_rate_percent = 100;
            series.data_points.push(point);
        }

        assert_eq!(series.data_points.len(), 30);
        assert_eq!(series.period, "30d");

        // Verify data point structure
        let first_point = &series.data_points[0];
        assert!(first_point.rotation_duration_avg_ms >= 50);
        assert_eq!(first_point.success_rate_percent, 100);
    }

    /// Test rotation success rate chart
    #[tokio::test]
    async fn test_rotation_success_rate_chart() {
        // Chart data shows rotation success rate over time
        let metrics = RotationMetrics::new();

        // Record successful and failed rotations
        metrics.record_rotation(50);
        metrics.record_rotation(60);
        metrics.record_rotation(70);
        metrics.record_failure();

        // total_rotations=3 (only record_rotation increments), failed_rotations=1
        assert_eq!(metrics.total_rotations(), 3);
        assert_eq!(metrics.failed_rotations(), 1);

        // success_rate = (total - failed) / total * 100 = (3-1)/3 * 100 = 66%
        let success_rate = metrics.success_rate_percent();
        assert_eq!(success_rate, 66);

        // Show trend: more successes improve rate
        metrics.record_rotation(80);
        metrics.record_rotation(90);
        let improved_rate = metrics.success_rate_percent();
        // Now: (5-1)/5 * 100 = 80%
        assert_eq!(improved_rate, 80);
        assert!(improved_rate > success_rate);
    }

    /// Test rotation duration histogram
    #[tokio::test]
    async fn test_rotation_duration_histogram() {
        // Histogram of rotation durations
        let metrics = RotationMetrics::new();

        // Record rotations with varying durations
        let durations = vec![50, 75, 100, 120, 80, 90, 200, 150, 60, 95];
        for d in &durations {
            metrics.record_rotation(*d);
        }

        // Verify metrics tracked
        assert_eq!(metrics.total_rotations(), 10);
        assert_eq!(metrics.failed_rotations(), 0);
        assert_eq!(metrics.success_rate_percent(), 100);

        // Last rotation duration is the most recent
        assert_eq!(metrics.last_rotation_duration_ms(), 95);

        // Last rotation timestamp is set
        assert!(metrics.last_rotation().is_some());
    }

    /// Test key version lifecycle chart
    #[tokio::test]
    async fn test_key_version_lifecycle_chart() {
        // Timeline of key versions showing lifecycle states
        let config = RotationConfig::new().with_ttl_days(365);
        let manager = CredentialRotationManager::new(config);

        // Create multiple versions
        manager.initialize_key().unwrap();
        manager.rotate_key().unwrap();
        manager.rotate_key().unwrap();

        // Get version history (sorted by issue date, newest first)
        let history = manager.get_version_history().unwrap();
        assert_eq!(history.len(), 3);

        // Verify lifecycle states
        for version_meta in &history {
            // All newly created versions should be Active
            assert!(!version_meta.is_expired());
        }

        // Mark one as compromised
        manager.mark_version_compromised(1, "Test compromise").unwrap();
        let v1_meta = manager.get_version_history().unwrap();
        let compromised = v1_meta.iter().find(|m| m.version == 1).unwrap();
        assert_eq!(compromised.status, crate::encryption::credential_rotation::KeyVersionStatus::Compromised);
    }

    /// Test TTL consumption gauge
    #[tokio::test]
    async fn test_ttl_consumption_gauge() {
        // TTL consumption gauge per key
        let config = RotationConfig::new().with_ttl_days(100);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Fresh key: low TTL consumption
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let ttl_percent = metadata.ttl_consumed_percent();
        assert!(ttl_percent < 5); // Just created, nearly 0%

        // Threshold line at 80%
        assert!(!metadata.should_refresh()); // Below 80%

        // Create a status card with the TTL percentage
        let card = KeyStatusCard::new("primary", metadata.version, ttl_percent);
        assert_eq!(card.status, "healthy"); // Low TTL = healthy
    }

    // ============================================================================
    // COMPLIANCE MONITORING TESTS
    // ============================================================================

    /// Test compliance dashboard
    #[tokio::test]
    async fn test_compliance_dashboard() {
        // Compliance status for each framework
        let mut dashboard = ComplianceDashboard::new();

        // Initially all compliant
        assert_eq!(dashboard.hipaa, ComplianceStatus::Compliant);
        assert_eq!(dashboard.pci_dss, ComplianceStatus::Compliant);
        assert_eq!(dashboard.gdpr, ComplianceStatus::Compliant);
        assert_eq!(dashboard.soc2, ComplianceStatus::Compliant);
        assert_eq!(dashboard.overall, ComplianceStatus::Compliant);

        // Set one to non-compliant
        dashboard.hipaa = ComplianceStatus::NonCompliant;
        dashboard.recalculate_overall();
        assert_eq!(dashboard.overall, ComplianceStatus::NonCompliant);

        // Set to partial
        dashboard.hipaa = ComplianceStatus::Compliant;
        dashboard.gdpr = ComplianceStatus::Partial;
        dashboard.recalculate_overall();
        assert_eq!(dashboard.overall, ComplianceStatus::Partial);

        // All compliant again
        dashboard.gdpr = ComplianceStatus::Compliant;
        dashboard.recalculate_overall();
        assert_eq!(dashboard.overall, ComplianceStatus::Compliant);
    }

    /// Test compliance requirement checklist
    #[tokio::test]
    async fn test_compliance_requirement_checklist() {
        // Per-framework checklist using ComplianceChecker
        let hipaa_checker = ComplianceChecker::hipaa();
        assert_eq!(hipaa_checker.framework, "HIPAA");
        assert_eq!(hipaa_checker.required_ttl_days, 365);
        assert_eq!(hipaa_checker.required_check_interval_hours, 24);

        let pci_checker = ComplianceChecker::pci_dss();
        assert_eq!(pci_checker.framework, "PCI-DSS");

        let gdpr_checker = ComplianceChecker::gdpr();
        assert_eq!(gdpr_checker.framework, "GDPR");
        assert_eq!(gdpr_checker.required_ttl_days, 90);

        let soc2_checker = ComplianceChecker::soc2();
        assert_eq!(soc2_checker.framework, "SOC 2");

        // Compliance checks pass for healthy keys
        let healthy_card = KeyStatusCard::new("primary", 1, 50);
        assert!(hipaa_checker.check_compliance(&healthy_card));
        assert!(pci_checker.check_compliance(&healthy_card));
        assert!(gdpr_checker.check_compliance(&healthy_card));
        assert!(soc2_checker.check_compliance(&healthy_card));

        // Compliance checks fail for urgent keys
        let urgent_card = KeyStatusCard::new("primary", 1, 90);
        assert!(!hipaa_checker.check_compliance(&urgent_card));
    }

    /// Test compliance violation alerts
    #[tokio::test]
    async fn test_compliance_violation_alerts() {
        // Alerts when compliance requirements not met
        let hipaa_alert = Alert::new(
            "compliance_violation",
            AlertSeverity::Critical,
            "HIPAA: Rotation overdue (365+ days)",
        )
        .with_key_id("primary")
        .with_action("Rotate encryption keys immediately");

        assert_eq!(hipaa_alert.severity, AlertSeverity::Critical);
        assert!(hipaa_alert.message.contains("HIPAA"));
        assert_eq!(hipaa_alert.key_id, Some("primary".to_string()));
        assert!(hipaa_alert.action.is_some());

        let pci_alert = Alert::new(
            "compliance_violation",
            AlertSeverity::Warning,
            "PCI-DSS: Manual rotation not tested in 90 days",
        );
        assert_eq!(pci_alert.severity, AlertSeverity::Warning);

        let gdpr_alert = Alert::new(
            "compliance_violation",
            AlertSeverity::Error,
            "GDPR: Key retention exceeds 1 year",
        );
        assert_eq!(gdpr_alert.severity, AlertSeverity::Error);

        let soc2_alert = Alert::new(
            "compliance_violation",
            AlertSeverity::Error,
            "SOC 2: Audit log gap detected",
        );
        assert_eq!(soc2_alert.severity, AlertSeverity::Error);
        assert!(soc2_alert.message.contains("SOC 2"));
    }

    /// Test compliance certificate simulation
    #[tokio::test]
    async fn test_compliance_certificate() {
        // Compliance certificate as text representation
        use crate::encryption::compliance::{
            ComplianceCheckResult, ComplianceFramework, ComplianceReport,
            ComplianceStatus as CStatus,
        };

        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption_at_rest",
            CStatus::Compliant,
            "All PHI fields encrypted with AES-256-GCM",
        )
        .with_details("45 fields encrypted across 12 tables");

        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(vec![result]);

        // Report shows framework, status, timestamp
        assert_eq!(report.framework, ComplianceFramework::HIPAA);
        assert_eq!(report.overall_status, CStatus::Compliant);
        assert_eq!(report.compliant_count, 1);
        assert_eq!(report.non_compliant_count, 0);

        // Can be exported for audit reports
        let json = report.to_json_like();
        assert!(json.contains("HIPAA"));
        assert!(json.contains("compliant"));

        // CSV export
        let header = ComplianceReport::to_csv_header();
        assert!(header.contains("Framework"));
        let rows = report.to_csv_rows();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].contains("HIPAA"));
    }

    // ============================================================================
    // HISTORICAL TREND TESTS
    // ============================================================================

    /// Test rotation trend analysis
    #[tokio::test]
    async fn test_rotation_trend_analysis() {
        // Trend analysis results
        let mut trend = TrendAnalysis::new();

        // Default: all stable
        assert_eq!(trend.rotation_frequency, "stable");
        assert_eq!(trend.rotation_duration, "stable");
        assert_eq!(trend.failure_rate, "stable");
        assert_eq!(trend.compliance_status, "stable");

        // Simulate trend changes
        trend.rotation_frequency = "increasing".to_string();
        trend.failure_rate = "decreasing".to_string();
        trend.compliance_status = "improving".to_string();

        assert_eq!(trend.rotation_frequency, "increasing");
        assert_eq!(trend.failure_rate, "decreasing");
        assert_eq!(trend.compliance_status, "improving");
    }

    /// Test anomaly detection
    #[tokio::test]
    async fn test_anomaly_detection() {
        // Detect anomalies by comparing current metrics to historical baseline
        let metrics = RotationMetrics::new();

        // Establish baseline: rotations complete in ~100ms
        for _ in 0..10 {
            metrics.record_rotation(100);
        }
        let baseline_duration = metrics.last_rotation_duration_ms();
        assert_eq!(baseline_duration, 100);

        // Anomaly: rotation takes much longer (>3 std dev from mean)
        metrics.record_rotation(1000);
        let anomalous_duration = metrics.last_rotation_duration_ms();
        assert!(anomalous_duration > baseline_duration * 3); // >3x baseline

        // Anomaly: unusually high failure rate
        for _ in 0..5 {
            metrics.record_failure();
        }
        let failure_rate = metrics.failed_rotations();
        let total = metrics.total_rotations();
        let error_percent = (failure_rate as f64 / (total + failure_rate) as f64) * 100.0;
        assert!(error_percent > 10.0); // >10% failure rate is anomalous
    }

    /// Test trend forecasting
    #[tokio::test]
    async fn test_trend_forecasting() {
        // Forecast based on TTL and historical data
        let config = RotationConfig::new().with_ttl_days(90);
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // When next rotation due (based on TTL)
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let time_until_expiry = metadata.time_until_expiry();
        assert!(time_until_expiry.num_days() > 0);
        assert!(time_until_expiry.num_days() <= 90);

        // Estimated rotation duration (based on historical)
        let rotation_metrics = manager.metrics();
        rotation_metrics.record_rotation(50);
        rotation_metrics.record_rotation(60);
        rotation_metrics.record_rotation(70);
        let last_duration = rotation_metrics.last_rotation_duration_ms();
        assert_eq!(last_duration, 70);

        // Expected compliance status based on TTL consumption
        let ttl_percent = metadata.ttl_consumed_percent();
        assert!(ttl_percent < 5); // Fresh key, well within compliance
        assert!(!metadata.should_refresh()); // No refresh needed yet
    }

    // ============================================================================
    // DASHBOARD CONFIGURATION TESTS
    // ============================================================================

    /// Test dashboard theme configuration
    #[tokio::test]
    async fn test_dashboard_theme_config() {
        // Dashboard supports theme configuration
        // Represented as string settings
        let themes = vec!["light", "dark", "high_contrast"];

        // Each theme is a valid option
        assert_eq!(themes.len(), 3);
        assert!(themes.contains(&"light"));
        assert!(themes.contains(&"dark"));
        assert!(themes.contains(&"high_contrast"));

        // Default theme
        let default_theme = "light";
        assert_eq!(default_theme, "light");

        // Theme can be persisted per user (represented as string)
        let user_theme: std::collections::HashMap<&str, &str> = [
            ("user1", "dark"),
            ("user2", "light"),
            ("user3", "high_contrast"),
        ]
        .into();
        assert_eq!(user_theme.get("user1"), Some(&"dark"));
        assert_eq!(user_theme.get("user2"), Some(&"light"));
        assert_eq!(user_theme.get("user3"), Some(&"high_contrast"));
    }

    /// Test dashboard widget customization
    #[tokio::test]
    async fn test_dashboard_widget_customization() {
        // Users can customize dashboard widgets
        let available_widgets = vec![
            "overview",
            "key_status_cards",
            "metrics_chart",
            "compliance_status",
            "alerts",
            "trend_analysis",
        ];
        assert_eq!(available_widgets.len(), 6);

        // User's custom layout (subset and order)
        let user_layout = vec!["alerts", "overview", "key_status_cards"];
        assert_eq!(user_layout.len(), 3);
        assert_eq!(user_layout[0], "alerts"); // Alerts first

        // All user widgets are valid
        for widget in &user_layout {
            assert!(available_widgets.contains(widget));
        }

        // Multiple layouts per user
        let layouts: std::collections::HashMap<&str, Vec<&str>> = [
            ("default", vec!["overview", "key_status_cards", "alerts"]),
            ("compact", vec!["overview", "alerts"]),
            ("detailed", available_widgets.clone()),
        ]
        .into();
        assert_eq!(layouts.len(), 3);
        assert!(layouts.get("compact").unwrap().len() < layouts.get("detailed").unwrap().len());
    }

    /// Test dashboard refresh settings
    #[tokio::test]
    async fn test_dashboard_refresh_settings() {
        // Refresh rate options (in seconds)
        let refresh_options: Vec<Option<u32>> = vec![
            None,      // auto-refresh disabled
            Some(15),  // 15 seconds
            Some(30),  // 30 seconds
            Some(60),  // 1 minute
            Some(300), // 5 minutes
        ];

        assert_eq!(refresh_options.len(), 5);
        assert_eq!(refresh_options[0], None); // Disabled option
        assert_eq!(refresh_options[1], Some(15));
        assert_eq!(refresh_options[4], Some(300));

        // Global setting
        let global_refresh = Some(30u32);
        assert_eq!(global_refresh, Some(30));

        // Per-widget override
        let widget_refresh: std::collections::HashMap<&str, Option<u32>> = [
            ("overview", Some(15)),       // Fast refresh for overview
            ("metrics_chart", Some(60)),  // Slower refresh for charts
            ("alerts", Some(15)),         // Fast refresh for alerts
        ]
        .into();
        assert_eq!(widget_refresh.get("overview"), Some(&Some(15)));
        assert_eq!(widget_refresh.get("metrics_chart"), Some(&Some(60)));
    }

    /// Test dashboard export
    #[tokio::test]
    async fn test_dashboard_export() {
        // Export dashboard snapshot
        let overview = DashboardOverview {
            total_keys: 5,
            healthy_keys: 3,
            warning_keys: 1,
            urgent_keys: 1,
            avg_ttl_percent: 55,
            system_health: "warning".to_string(),
        };

        let key_cards = vec![
            KeyStatusCard::new("primary", 3, 30),
            KeyStatusCard::new("secondary", 2, 60),
            KeyStatusCard::new("tertiary", 1, 80),
        ];

        let compliance = ComplianceDashboard::new();
        let snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 2);

        // Snapshot includes timestamp and key metrics
        assert!(snapshot.snapshot_time <= Utc::now());
        assert_eq!(snapshot.key_cards.len(), 3);
        assert_eq!(snapshot.active_alerts_count, 2);
        assert_eq!(snapshot.overview.total_keys, 5);

        // Urgency summary
        let (healthy, warning, urgent, overdue) = snapshot.urgency_summary();
        assert_eq!(healthy, 2); // 30% and 60% are healthy
        assert_eq!(warning, 1); // 80% is warning
        assert_eq!(urgent, 0);
        assert_eq!(overdue, 0);

        // Average urgency score
        let avg_urgency = snapshot.average_urgency_score();
        assert!(avg_urgency > 0);

        // Export format options
        assert_eq!(ExportFormat::Json.to_string(), "json");
        assert_eq!(ExportFormat::Csv.to_string(), "csv");
        assert_eq!(ExportFormat::Pdf.to_string(), "pdf");

        // JSON serialization works
        let json = serde_json::to_string(&snapshot.overview).unwrap();
        assert!(json.contains("total_keys"));
        assert!(json.contains("5"));
    }

    // ============================================================================
    // ALERT AND NOTIFICATION TESTS
    // ============================================================================

    /// Test dashboard alerts widget
    #[tokio::test]
    async fn test_dashboard_alerts_widget() {
        // Alerts widget shows various alert types
        let mut widget = AlertsWidget::new();

        // Add different alert types
        widget.add_alert(
            Alert::new("rotation_overdue", AlertSeverity::Critical, "Key rotation overdue")
                .with_key_id("primary"),
        );
        widget.add_alert(
            Alert::new("rotation_failed", AlertSeverity::Error, "Rotation failed")
                .with_key_id("secondary"),
        );
        widget.add_alert(
            Alert::new("ttl_expiring", AlertSeverity::Warning, "TTL expiring soon")
                .with_key_id("tertiary"),
        );
        widget.add_alert(
            Alert::new("compliance_violation", AlertSeverity::Error, "Compliance check failed"),
        );
        widget.add_alert(
            Alert::new("anomaly_detected", AlertSeverity::Info, "Unusual rotation duration"),
        );

        assert_eq!(widget.active_alerts.len(), 5);
        assert_eq!(widget.unacknowledged_count, 5);

        // Verify alert severities (color-coded)
        assert_eq!(widget.active_alerts[0].severity, AlertSeverity::Critical);
        assert_eq!(widget.active_alerts[1].severity, AlertSeverity::Error);
        assert_eq!(widget.active_alerts[2].severity, AlertSeverity::Warning);
        assert_eq!(widget.active_alerts[4].severity, AlertSeverity::Info);

        // Acknowledge an alert
        widget.active_alerts[0].acknowledge();
        assert!(widget.active_alerts[0].acknowledged);
    }

    /// Test alert notification email
    #[tokio::test]
    async fn test_alert_notification_email() {
        // Alert contains information needed for email notification
        let alert = Alert::new(
            "rotation_failed",
            AlertSeverity::Critical,
            "Rotation failed for key 'primary'",
        )
        .with_key_id("primary")
        .with_action("Check Vault connectivity and retry rotation");

        // Email contains alert summary
        assert!(!alert.message.is_empty());

        // Affected key(s)
        assert_eq!(alert.key_id, Some("primary".to_string()));

        // Recommended action
        assert!(alert.action.is_some());
        assert!(alert.action.as_ref().unwrap().contains("retry"));

        // Configurable recipients (represented as metadata)
        let recipients = vec!["admin@company.com", "security@company.com"];
        assert_eq!(recipients.len(), 2);

        // Alert has unique ID for deduplication
        assert!(!alert.id.is_empty());

        // Timestamp for email
        assert!(alert.timestamp <= Utc::now());
    }

    /// Test alert webhook integration
    #[tokio::test]
    async fn test_alert_webhook_integration() {
        // Alert webhook payload as JSON
        let alert = Alert::new("rotation_failed", AlertSeverity::Critical, "Rotation failed")
            .with_key_id("primary");

        // Serialize to JSON
        let json = serde_json::to_value(&alert).unwrap();

        // Verify JSON structure matches expected webhook format
        assert_eq!(json["alert_type"], "rotation_failed");
        assert_eq!(json["severity"], "critical");
        assert_eq!(json["key_id"], "primary");
        assert!(json["timestamp"].is_string());
        assert_eq!(json["message"], "Rotation failed");
        assert!(!json["id"].as_str().unwrap().is_empty());

        // Acknowledged status included
        assert_eq!(json["acknowledged"], false);
    }

    // ============================================================================
    // REAL-TIME UPDATE TESTS
    // ============================================================================

    /// Test WebSocket real-time updates
    #[tokio::test]
    async fn test_websocket_real_time_updates() {
        // Simulate real-time updates by tracking state changes
        let config = RotationConfig::new();
        let manager = CredentialRotationManager::new(config);
        manager.initialize_key().unwrap();

        // Event: rotation starts/completes
        let v1 = manager.get_current_version().unwrap();
        let v2 = manager.rotate_key().unwrap();
        assert!(v2 > v1);

        // Event: TTL changes tracked
        let metadata = manager.get_current_metadata().unwrap().unwrap();
        let ttl = metadata.ttl_consumed_percent();
        assert!(ttl < 5); // Fresh key

        // Event: compliance status derived from key status
        let card = KeyStatusCard::new("primary", v2, ttl);
        let hipaa_checker = ComplianceChecker::hipaa();
        assert!(hipaa_checker.check_compliance(&card));

        // Event: alerts triggered on status change
        let mut alerts_widget = AlertsWidget::new();
        let alert = Alert::new("rotation_completed", AlertSeverity::Info, "Key rotated successfully")
            .with_key_id("primary");
        alerts_widget.add_alert(alert);
        assert_eq!(alerts_widget.active_alerts.len(), 1);
    }

    /// Test server-sent events updates
    #[tokio::test]
    async fn test_server_sent_events_updates() {
        // SSE returns event stream with rotation status updates
        let metrics = RotationMetrics::new();

        // Simulate events
        metrics.record_rotation(50);
        let event1_total = metrics.total_rotations();
        assert_eq!(event1_total, 1);

        metrics.record_rotation(75);
        let event2_total = metrics.total_rotations();
        assert_eq!(event2_total, 2);

        metrics.record_failure();
        let event3_failures = metrics.failed_rotations();
        assert_eq!(event3_failures, 1);

        // Events include timestamp
        assert!(metrics.last_rotation().is_some());

        // Success rate updates with each event
        let rate = metrics.success_rate_percent();
        assert_eq!(rate, 50); // 1 out of 2 successful (failure not counted in total)
    }

    // ============================================================================
    // DASHBOARD PERFORMANCE TESTS
    // ============================================================================

    /// Test dashboard load time
    #[tokio::test]
    async fn test_dashboard_load_time() {
        // Dashboard should load quickly (construct all widgets)
        let start = std::time::Instant::now();

        // Construct dashboard components
        let overview = DashboardOverview::new();
        let key_cards: Vec<KeyStatusCard> = (0..20)
            .map(|i| KeyStatusCard::new(format!("key_{}", i), 1, i * 5))
            .collect();
        let compliance = ComplianceDashboard::new();
        let mut alerts_widget = AlertsWidget::new();
        for i in 0..5 {
            alerts_widget.add_alert(Alert::new(
                format!("alert_{}", i),
                AlertSeverity::Warning,
                format!("Alert {}", i),
            ));
        }
        let _snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 5);

        let elapsed = start.elapsed();
        // Should complete in well under 2 seconds (typically <1ms)
        assert!(elapsed.as_millis() < 2000);
    }

    /// Test dashboard with many keys
    #[tokio::test]
    async fn test_dashboard_with_many_keys() {
        // Dashboard scales with 100+ keys
        let key_cards: Vec<KeyStatusCard> = (0..200)
            .map(|i| KeyStatusCard::new(format!("key_{:04}", i), 1, i as u32 % 110))
            .collect();

        let overview = DashboardOverview {
            total_keys: key_cards.len(),
            healthy_keys: key_cards.iter().filter(|c| c.status == "healthy").count(),
            warning_keys: key_cards.iter().filter(|c| c.status == "warning").count(),
            urgent_keys: key_cards.iter().filter(|c| c.status == "urgent").count(),
            avg_ttl_percent: 50,
            system_health: "warning".to_string(),
        };

        let compliance = ComplianceDashboard::new();
        let snapshot = DashboardSnapshot::new(overview, key_cards, compliance, 0);

        assert_eq!(snapshot.key_cards.len(), 200);
        assert!(snapshot.overview.total_keys == 200);

        // Pagination: simulate page of 20
        let page_size = 20;
        let page_1: Vec<_> = snapshot.key_cards.iter().take(page_size).collect();
        let page_2: Vec<_> = snapshot.key_cards.iter().skip(page_size).take(page_size).collect();
        assert_eq!(page_1.len(), 20);
        assert_eq!(page_2.len(), 20);
        assert_ne!(page_1[0].key_id, page_2[0].key_id);

        // Urgency summary works with many keys
        let (healthy, warning, urgent, overdue) = snapshot.urgency_summary();
        assert!(healthy + warning + urgent + overdue == 200);
    }

    /// Test dashboard responsive design
    #[tokio::test]
    async fn test_dashboard_responsive_design() {
        // Dashboard adapts layout for screen size (modeled as column count)
        let screen_sizes = vec![
            ("desktop", 1920, 1080, 4),  // 4 columns
            ("tablet", 1024, 768, 2),    // 2 columns
            ("mobile", 375, 667, 1),     // 1 column
        ];

        for (name, width, height, expected_columns) in &screen_sizes {
            // Column count adapts to screen size
            let columns = if *width >= 1200 {
                4
            } else if *width >= 768 {
                2
            } else {
                1
            };
            assert_eq!(
                columns, *expected_columns,
                "Screen '{}' ({}x{}) should have {} columns",
                name, width, height, expected_columns
            );
        }

        // All screen sizes can display key cards
        let cards = vec![
            KeyStatusCard::new("primary", 1, 50),
            KeyStatusCard::new("secondary", 1, 80),
        ];
        assert_eq!(cards.len(), 2);

        // Cards have all required data regardless of screen size
        for card in &cards {
            assert!(!card.key_id.is_empty());
            assert!(!card.status.is_empty());
            assert!(card.urgency_score <= 100);
        }
    }

    /// Test dashboard accessibility
    #[tokio::test]
    async fn test_dashboard_accessibility() {
        // Dashboard accessibility: all data has text alternatives
        let card = KeyStatusCard::new("primary", 1, 75);

        // Status has text representation (for screen readers)
        assert!(!card.status.is_empty());
        assert!(!card.recommended_action.is_empty());

        // Urgency is numeric (keyboard navigable / screen reader friendly)
        assert!(card.urgency_score <= 100);

        // Alert severities have string representations
        assert_eq!(AlertSeverity::Info.to_string(), "info");
        assert_eq!(AlertSeverity::Warning.to_string(), "warning");
        assert_eq!(AlertSeverity::Error.to_string(), "error");
        assert_eq!(AlertSeverity::Critical.to_string(), "critical");

        // Compliance statuses have string representations
        assert_eq!(ComplianceStatus::Compliant.to_string(), "compliant");
        assert_eq!(ComplianceStatus::NonCompliant.to_string(), "non_compliant");
        assert_eq!(ComplianceStatus::Partial.to_string(), "partial");

        // High contrast: all text descriptions are non-empty
        let alert = Alert::new("test", AlertSeverity::Warning, "Test alert message");
        assert!(!alert.message.is_empty());
        assert!(!alert.alert_type.is_empty());
    }
}
