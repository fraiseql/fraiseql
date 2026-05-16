#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
    let config =
        ComplianceConfig::new(ComplianceFramework::HIPAA).with_setting("data_handler", "medical");
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
    validator.register_framework(config);
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
