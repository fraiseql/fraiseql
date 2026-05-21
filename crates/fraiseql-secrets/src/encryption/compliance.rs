//! Compliance framework validation and reporting for HIPAA, PCI-DSS, GDPR, SOC 2.
//!
//! Provides compliance validators, audit trail enforcement, and compliance reporting
//! for regulated industries.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

/// Compliance framework
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
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
#[non_exhaustive]
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
    #[must_use]
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
    #[must_use]
    pub const fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Set audit retention
    #[must_use]
    pub const fn with_retention_days(mut self, days: i32) -> Self {
        self.audit_retention_days = days;
        self
    }

    /// Set key rotation
    #[must_use]
    pub const fn with_key_rotation_days(mut self, days: i32) -> Self {
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
    #[must_use]
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
    #[must_use]
    pub fn is_framework_enabled(&self, framework: ComplianceFramework) -> bool {
        self.configs.get(&framework).is_some_and(|c| c.enabled)
    }

    /// Get framework config
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn results_for_framework(
        &self,
        framework: ComplianceFramework,
    ) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.framework == framework)
    }

    /// Get results with specific status
    #[must_use]
    pub fn results_by_status(&self, status: ComplianceStatus) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.status == status)
    }

    /// Get results for framework and status
    #[must_use]
    pub fn results_for_framework_status(
        &self,
        framework: ComplianceFramework,
        status: ComplianceStatus,
    ) -> Vec<&ComplianceCheckResult> {
        self.filter_results(|r| r.framework == framework && r.status == status)
    }

    /// Check overall compliance status for framework
    #[must_use]
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
    #[must_use]
    pub fn enabled_frameworks(&self) -> Vec<ComplianceFramework> {
        self.configs.iter().filter(|(_, c)| c.enabled).map(|(f, _)| *f).collect()
    }

    /// Get compliance status for all enabled frameworks
    #[must_use]
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
    #[must_use]
    pub fn count_by_status(&self, status: ComplianceStatus) -> usize {
        self.results_by_status(status).len()
    }

    /// Get compliance summary
    #[must_use]
    pub fn get_summary(&self) -> (usize, usize, usize) {
        (
            self.count_by_status(ComplianceStatus::Compliant),
            self.count_by_status(ComplianceStatus::NonCompliant),
            self.count_by_status(ComplianceStatus::PartiallyCompliant),
        )
    }

    /// Clear old results
    pub fn cleanup_old_results(&mut self, days: i32) {
        let cutoff = Utc::now() - Duration::days(i64::from(days));
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
    /// Number of requirements that passed this report period.
    pub compliant_count:     usize,
    /// Number of requirements that failed this report period.
    pub non_compliant_count: usize,
    /// Number of requirements that only partially passed this report period.
    pub partial_count:       usize,
}

impl ComplianceReport {
    /// Create new report
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn to_csv_header() -> String {
        "Framework,Requirement,Status,Description,Details,CheckedAt".to_string()
    }

    /// Export to CSV row
    #[must_use]
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
mod tests;
