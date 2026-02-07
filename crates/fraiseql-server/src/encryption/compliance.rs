// Phase 12.3 Cycle 8: Compliance Features (GREEN)
//! Compliance framework validation and reporting for HIPAA, PCI-DSS, GDPR, SOC 2.
//!
//! Provides compliance validators, audit trail enforcement, and compliance reporting
//! for regulated industries.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

/// Compliance framework
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComplianceFramework {
    /// HIPAA (healthcare)
    HIPAA,
    /// PCI-DSS (payment cards)
    PCIDSS,
    /// GDPR (EU data privacy)
    GDPR,
    /// SOC 2 (service controls)
    SOC2,
}

impl std::fmt::Display for ComplianceFramework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HIPAA => write!(f, "HIPAA"),
            Self::PCIDSS => write!(f, "PCI-DSS"),
            Self::GDPR => write!(f, "GDPR"),
            Self::SOC2 => write!(f, "SOC 2"),
        }
    }
}

/// Compliance requirement status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceStatus {
    /// Compliant
    Compliant,
    /// Non-compliant
    NonCompliant,
    /// Partially compliant (some requirements met)
    PartiallyCompliant,
    /// Unknown (not yet checked)
    Unknown,
}

impl std::fmt::Display for ComplianceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Compliant => write!(f, "compliant"),
            Self::NonCompliant => write!(f, "non-compliant"),
            Self::PartiallyCompliant => write!(f, "partially-compliant"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Compliance configuration for a framework
#[derive(Debug, Clone)]
pub struct ComplianceConfig {
    /// Framework
    pub framework:            ComplianceFramework,
    /// Enabled for this instance
    pub enabled:              bool,
    /// Audit log retention days
    pub audit_retention_days: i32,
    /// Encryption required
    pub encryption_required:  bool,
    /// Encryption algorithm
    pub encryption_algorithm: String,
    /// Key rotation required (days, 0 = not required)
    pub key_rotation_days:    i32,
    /// Additional settings
    pub settings:             HashMap<String, String>,
}

impl ComplianceConfig {
    /// Create new compliance config
    pub fn new(framework: ComplianceFramework) -> Self {
        let (retention, rotation, algorithm) = match framework {
            ComplianceFramework::HIPAA => (2190, 365, "aes256-gcm"), // 6 years, 1 year
            ComplianceFramework::PCIDSS => (365, 365, "aes256-gcm"), // 1 year, 1 year
            ComplianceFramework::GDPR => (2555, 0, "aes256-gcm"),    // ~7 years, as needed
            ComplianceFramework::SOC2 => (365, 0, "aes256-gcm"),     // 1 year, as needed
        };

        Self {
            framework,
            enabled: true,
            audit_retention_days: retention,
            encryption_required: true,
            encryption_algorithm: algorithm.to_string(),
            key_rotation_days: rotation,
            settings: HashMap::new(),
        }
    }

    /// Disable this framework
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set audit retention
    pub fn with_retention_days(mut self, days: i32) -> Self {
        self.audit_retention_days = days;
        self
    }

    /// Set key rotation
    pub fn with_key_rotation_days(mut self, days: i32) -> Self {
        self.key_rotation_days = days;
        self
    }

    /// Add custom setting
    pub fn with_setting(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.insert(key.into(), value.into());
        self
    }
}

/// Compliance requirement check result
#[derive(Debug, Clone)]
pub struct ComplianceCheckResult {
    /// Framework
    pub framework:   ComplianceFramework,
    /// Requirement name
    pub requirement: String,
    /// Status
    pub status:      ComplianceStatus,
    /// Description of requirement
    pub description: String,
    /// Evidence/details
    pub details:     String,
    /// Last checked
    pub checked_at:  DateTime<Utc>,
}

impl ComplianceCheckResult {
    /// Create new check result
    pub fn new(
        framework: ComplianceFramework,
        requirement: impl Into<String>,
        status: ComplianceStatus,
        description: impl Into<String>,
    ) -> Self {
        Self {
            framework,
            requirement: requirement.into(),
            status,
            description: description.into(),
            details: String::new(),
            checked_at: Utc::now(),
        }
    }

    /// Add details
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }
}

/// Compliance validator
pub struct ComplianceValidator {
    /// Configurations per framework
    configs: HashMap<ComplianceFramework, ComplianceConfig>,
    /// Check results
    results: Vec<ComplianceCheckResult>,
}

impl ComplianceValidator {
    /// Create new validator
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
            results: Vec::new(),
        }
    }

    /// Register framework
    pub fn register_framework(&mut self, config: ComplianceConfig) {
        self.configs.insert(config.framework, config);
    }

    /// Check if framework enabled
    pub fn is_framework_enabled(&self, framework: ComplianceFramework) -> bool {
        self.configs.get(&framework).map(|c| c.enabled).unwrap_or(false)
    }

    /// Get framework config
    pub fn get_framework_config(
        &self,
        framework: ComplianceFramework,
    ) -> Option<&ComplianceConfig> {
        self.configs.get(&framework)
    }

    /// Record check result
    pub fn record_result(&mut self, result: ComplianceCheckResult) {
        self.results.push(result);
    }

    /// Get all results
    pub fn results(&self) -> &[ComplianceCheckResult] {
        &self.results
    }

    /// Filter results by predicate
    fn filter_results<F>(&self, predicate: F) -> Vec<&ComplianceCheckResult>
    where
        F: Fn(&&ComplianceCheckResult) -> bool,
    {
        self.results.iter().filter(predicate).collect()
    }

    /// Get results for framework
    pub fn results_for_framework(
        &self,
        framework: ComplianceFramework,
    ) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.framework == framework)
    }

    /// Get results with specific status
    pub fn results_by_status(&self, status: ComplianceStatus) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.status == status)
    }

    /// Get results for framework and status
    pub fn results_for_framework_status(
        &self,
        framework: ComplianceFramework,
        status: ComplianceStatus,
    ) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.framework == framework && r.status == status)
    }

    /// Check overall compliance status for framework
    pub fn check_framework_status(&self, framework: ComplianceFramework) -> ComplianceStatus {
        if !self.is_framework_enabled(framework) {
            return ComplianceStatus::Unknown;
        }

        let results = self.results_for_framework(framework);

        if results.is_empty() {
            return ComplianceStatus::Unknown;
        }

        let all_compliant = results.iter().all(|r| r.status == ComplianceStatus::Compliant);
        let any_compliant = results.iter().any(|r| r.status == ComplianceStatus::Compliant);

        if all_compliant {
            ComplianceStatus::Compliant
        } else if any_compliant {
            ComplianceStatus::PartiallyCompliant
        } else {
            ComplianceStatus::NonCompliant
        }
    }

    /// Get enabled frameworks
    pub fn enabled_frameworks(&self) -> Vec<ComplianceFramework> {
        self.configs.iter().filter(|(_, c)| c.enabled).map(|(f, _)| *f).collect()
    }

    /// Get compliance status for all enabled frameworks
    pub fn overall_status(&self) -> ComplianceStatus {
        let enabled = self.enabled_frameworks();
        if enabled.is_empty() {
            return ComplianceStatus::Unknown;
        }

        let statuses: Vec<_> = enabled.iter().map(|f| self.check_framework_status(*f)).collect();

        let all_compliant = statuses.iter().all(|s| *s == ComplianceStatus::Compliant);
        let any_compliant = statuses.contains(&ComplianceStatus::Compliant);

        if all_compliant {
            ComplianceStatus::Compliant
        } else if any_compliant {
            ComplianceStatus::PartiallyCompliant
        } else {
            ComplianceStatus::NonCompliant
        }
    }

    /// Count results by status
    pub fn count_by_status(&self, status: ComplianceStatus) -> usize {
        self.results_by_status(status).len()
    }

    /// Get compliance summary
    pub fn get_summary(&self) -> (usize, usize, usize) {
        (
            self.count_by_status(ComplianceStatus::Compliant),
            self.count_by_status(ComplianceStatus::NonCompliant),
            self.count_by_status(ComplianceStatus::PartiallyCompliant),
        )
    }

    /// Clear old results
    pub fn cleanup_old_results(&mut self, days: i32) {
        let cutoff = Utc::now() - Duration::days(days as i64);
        self.results.retain(|r| r.checked_at > cutoff);
    }
}

impl Default for ComplianceValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Compliance report
#[derive(Debug, Clone)]
pub struct ComplianceReport {
    /// Report generation time
    pub generated_at:        DateTime<Utc>,
    /// Framework
    pub framework:           ComplianceFramework,
    /// Overall status
    pub overall_status:      ComplianceStatus,
    /// Check results
    pub results:             Vec<ComplianceCheckResult>,
    /// Summary statistics
    pub compliant_count:     usize,
    pub non_compliant_count: usize,
    pub partial_count:       usize,
}

impl ComplianceReport {
    /// Create new report
    pub fn new(framework: ComplianceFramework) -> Self {
        Self {
            generated_at: Utc::now(),
            framework,
            overall_status: ComplianceStatus::Unknown,
            results: Vec::new(),
            compliant_count: 0,
            non_compliant_count: 0,
            partial_count: 0,
        }
    }

    /// Add results
    pub fn with_results(mut self, results: Vec<ComplianceCheckResult>) -> Self {
        self.compliant_count =
            results.iter().filter(|r| r.status == ComplianceStatus::Compliant).count();
        self.non_compliant_count =
            results.iter().filter(|r| r.status == ComplianceStatus::NonCompliant).count();
        self.partial_count = results
            .iter()
            .filter(|r| r.status == ComplianceStatus::PartiallyCompliant)
            .count();

        if self.compliant_count == results.len() && !results.is_empty() {
            self.overall_status = ComplianceStatus::Compliant;
        } else if self.compliant_count + self.partial_count > 0 {
            self.overall_status = ComplianceStatus::PartiallyCompliant;
        } else if self.non_compliant_count > 0 {
            self.overall_status = ComplianceStatus::NonCompliant;
        }

        self.results = results;
        self
    }

    /// Export to JSON-like string
    pub fn to_json_like(&self) -> String {
        format!(
            r#"{{ "framework": "{}", "generated_at": "{}", "overall_status": "{}", "compliant": {}, "non_compliant": {}, "partial": {} }}"#,
            self.framework,
            self.generated_at.to_rfc3339(),
            self.overall_status,
            self.compliant_count,
            self.non_compliant_count,
            self.partial_count
        )
    }

    /// Export to CSV header
    pub fn to_csv_header() -> String {
        "Framework,Requirement,Status,Description,Details,CheckedAt".to_string()
    }

    /// Export to CSV row
    pub fn to_csv_rows(&self) -> Vec<String> {
        self.results
            .iter()
            .map(|r| {
                format!(
                    "{},{},{},{},{}",
                    r.framework,
                    r.requirement.replace(',', ";"),
                    r.status,
                    r.description.replace(',', ";"),
                    r.checked_at.to_rfc3339()
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compliance_framework_display() {
        assert_eq!(ComplianceFramework::HIPAA.to_string(), "HIPAA");
        assert_eq!(ComplianceFramework::PCIDSS.to_string(), "PCI-DSS");
        assert_eq!(ComplianceFramework::GDPR.to_string(), "GDPR");
        assert_eq!(ComplianceFramework::SOC2.to_string(), "SOC 2");
    }

    #[test]
    fn test_compliance_status_display() {
        assert_eq!(ComplianceStatus::Compliant.to_string(), "compliant");
        assert_eq!(ComplianceStatus::NonCompliant.to_string(), "non-compliant");
        assert_eq!(ComplianceStatus::PartiallyCompliant.to_string(), "partially-compliant");
        assert_eq!(ComplianceStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_compliance_config_hipaa() {
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        assert!(config.enabled);
        assert!(config.encryption_required);
        assert_eq!(config.audit_retention_days, 2190); // 6 years
        assert_eq!(config.key_rotation_days, 365);
    }

    #[test]
    fn test_compliance_config_pcidss() {
        let config = ComplianceConfig::new(ComplianceFramework::PCIDSS);
        assert_eq!(config.audit_retention_days, 365); // 1 year
        assert_eq!(config.key_rotation_days, 365);
    }

    #[test]
    fn test_compliance_config_gdpr() {
        let config = ComplianceConfig::new(ComplianceFramework::GDPR);
        assert_eq!(config.audit_retention_days, 2555); // ~7 years
        assert_eq!(config.key_rotation_days, 0); // As needed
    }

    #[test]
    fn test_compliance_config_soc2() {
        let config = ComplianceConfig::new(ComplianceFramework::SOC2);
        assert_eq!(config.audit_retention_days, 365);
    }

    #[test]
    fn test_compliance_config_disabled() {
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA).disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_compliance_config_with_retention() {
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA).with_retention_days(1000);
        assert_eq!(config.audit_retention_days, 1000);
    }

    #[test]
    fn test_compliance_config_with_key_rotation() {
        let config = ComplianceConfig::new(ComplianceFramework::GDPR).with_key_rotation_days(180);
        assert_eq!(config.key_rotation_days, 180);
    }

    #[test]
    fn test_compliance_config_with_setting() {
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA)
            .with_setting("data_handler", "medical");
        assert_eq!(config.settings.get("data_handler"), Some(&"medical".to_string()));
    }

    #[test]
    fn test_compliance_check_result_creation() {
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption_at_rest",
            ComplianceStatus::Compliant,
            "PHI encrypted with AES-256-GCM",
        );
        assert_eq!(result.framework, ComplianceFramework::HIPAA);
        assert_eq!(result.requirement, "encryption_at_rest");
        assert_eq!(result.status, ComplianceStatus::Compliant);
    }

    #[test]
    fn test_compliance_check_result_with_details() {
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption_at_rest",
            ComplianceStatus::Compliant,
            "PHI encrypted",
        )
        .with_details("All 45 PHI fields encrypted");
        assert_eq!(result.details, "All 45 PHI fields encrypted");
    }

    #[test]
    fn test_compliance_validator_creation() {
        let validator = ComplianceValidator::new();
        assert_eq!(validator.enabled_frameworks().len(), 0);
    }

    #[test]
    fn test_compliance_validator_register_framework() {
        let mut validator = ComplianceValidator::new();
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        validator.register_framework(config);
        assert!(validator.is_framework_enabled(ComplianceFramework::HIPAA));
    }

    #[test]
    fn test_compliance_validator_get_framework_config() {
        let mut validator = ComplianceValidator::new();
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        validator.register_framework(config.clone());
        let retrieved = validator.get_framework_config(ComplianceFramework::HIPAA);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().framework, ComplianceFramework::HIPAA);
    }

    #[test]
    fn test_compliance_validator_record_result() {
        let mut validator = ComplianceValidator::new();
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        validator.record_result(result);
        assert_eq!(validator.results().len(), 1);
    }

    #[test]
    fn test_compliance_validator_results_for_framework() {
        let mut validator = ComplianceValidator::new();
        let hipaa_result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let pcidss_result = ComplianceCheckResult::new(
            ComplianceFramework::PCIDSS,
            "key_rotation",
            ComplianceStatus::Compliant,
            "Keys rotated",
        );
        validator.record_result(hipaa_result);
        validator.record_result(pcidss_result);

        let hipaa_results = validator.results_for_framework(ComplianceFramework::HIPAA);
        assert_eq!(hipaa_results.len(), 1);
    }

    #[test]
    fn test_compliance_validator_check_framework_status_compliant() {
        let mut validator = ComplianceValidator::new();
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        validator.register_framework(config);
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        validator.record_result(result);

        let status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(status, ComplianceStatus::Compliant);
    }

    #[test]
    fn test_compliance_validator_check_framework_status_non_compliant() {
        let mut validator = ComplianceValidator::new();
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        validator.register_framework(config);
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::NonCompliant,
            "Not encrypted",
        );
        validator.record_result(result);

        let status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(status, ComplianceStatus::NonCompliant);
    }

    #[test]
    fn test_compliance_validator_check_framework_status_partial() {
        let mut validator = ComplianceValidator::new();
        let config = ComplianceConfig::new(ComplianceFramework::HIPAA);
        validator.register_framework(config);
        let result1 = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let result2 = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "audit_trail",
            ComplianceStatus::NonCompliant,
            "Incomplete",
        );
        validator.record_result(result1);
        validator.record_result(result2);

        let status = validator.check_framework_status(ComplianceFramework::HIPAA);
        assert_eq!(status, ComplianceStatus::PartiallyCompliant);
    }

    #[test]
    fn test_compliance_validator_enabled_frameworks() {
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::PCIDSS).disabled());

        let enabled = validator.enabled_frameworks();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0], ComplianceFramework::HIPAA);
    }

    #[test]
    fn test_compliance_report_creation() {
        let report = ComplianceReport::new(ComplianceFramework::HIPAA);
        assert_eq!(report.framework, ComplianceFramework::HIPAA);
        assert_eq!(report.overall_status, ComplianceStatus::Unknown);
    }

    #[test]
    fn test_compliance_report_with_results() {
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(vec![result]);
        assert_eq!(report.overall_status, ComplianceStatus::Compliant);
        assert_eq!(report.compliant_count, 1);
    }

    #[test]
    fn test_compliance_report_to_json_like() {
        let report = ComplianceReport::new(ComplianceFramework::HIPAA);
        let json = report.to_json_like();
        assert!(json.contains("HIPAA"));
        assert!(json.contains("unknown"));
    }

    #[test]
    fn test_compliance_report_to_csv() {
        let result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let report = ComplianceReport::new(ComplianceFramework::HIPAA).with_results(vec![result]);
        let header = ComplianceReport::to_csv_header();
        assert!(header.contains("Framework"));
        let rows = report.to_csv_rows();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_compliance_validator_results_by_status() {
        let mut validator = ComplianceValidator::new();
        let compliant = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let non_compliant = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "audit_trail",
            ComplianceStatus::NonCompliant,
            "Not logged",
        );
        validator.record_result(compliant);
        validator.record_result(non_compliant);

        let compliant_results = validator.results_by_status(ComplianceStatus::Compliant);
        assert_eq!(compliant_results.len(), 1);
    }

    #[test]
    fn test_compliance_validator_results_for_framework_status() {
        let mut validator = ComplianceValidator::new();
        let hipaa_compliant = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let pcidss_compliant = ComplianceCheckResult::new(
            ComplianceFramework::PCIDSS,
            "key_rotation",
            ComplianceStatus::Compliant,
            "Rotated",
        );
        validator.record_result(hipaa_compliant);
        validator.record_result(pcidss_compliant);

        let results = validator
            .results_for_framework_status(ComplianceFramework::HIPAA, ComplianceStatus::Compliant);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_compliance_validator_overall_status() {
        let mut validator = ComplianceValidator::new();
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::HIPAA));
        validator.register_framework(ComplianceConfig::new(ComplianceFramework::PCIDSS));

        let hipaa_result = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let pcidss_result = ComplianceCheckResult::new(
            ComplianceFramework::PCIDSS,
            "key_rotation",
            ComplianceStatus::Compliant,
            "Rotated",
        );
        validator.record_result(hipaa_result);
        validator.record_result(pcidss_result);

        let status = validator.overall_status();
        assert_eq!(status, ComplianceStatus::Compliant);
    }

    #[test]
    fn test_compliance_validator_count_by_status() {
        let mut validator = ComplianceValidator::new();
        let compliant1 = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let compliant2 = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "audit",
            ComplianceStatus::Compliant,
            "Logged",
        );
        validator.record_result(compliant1);
        validator.record_result(compliant2);

        assert_eq!(validator.count_by_status(ComplianceStatus::Compliant), 2);
        assert_eq!(validator.count_by_status(ComplianceStatus::NonCompliant), 0);
    }

    #[test]
    fn test_compliance_validator_get_summary() {
        let mut validator = ComplianceValidator::new();
        let compliant = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "encryption",
            ComplianceStatus::Compliant,
            "Encrypted",
        );
        let non_compliant = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "audit_trail",
            ComplianceStatus::NonCompliant,
            "Not logged",
        );
        let partial = ComplianceCheckResult::new(
            ComplianceFramework::HIPAA,
            "key_rotation",
            ComplianceStatus::PartiallyCompliant,
            "Partial",
        );
        validator.record_result(compliant);
        validator.record_result(non_compliant);
        validator.record_result(partial);

        let (compliant_count, non_compliant_count, partial_count) = validator.get_summary();
        assert_eq!(compliant_count, 1);
        assert_eq!(non_compliant_count, 1);
        assert_eq!(partial_count, 1);
    }
}
