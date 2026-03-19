#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::error::FraiseQLError;
use crate::validation::ValidationRule;

#[test]
fn test_custom_type_def_new() {
    let def = CustomTypeDef::new("Email".to_string());

    assert_eq!(def.name, "Email");
    assert_eq!(def.description, None);
    assert_eq!(def.specified_by_url, None);
    assert_eq!(def.validation_rules.len(), 0);
    assert_eq!(def.elo_expression, None);
    assert_eq!(def.base_type, None);
}

#[test]
fn test_custom_type_def_with_all_fields() {
    let def = CustomTypeDef {
        name:             "ISBN".to_string(),
        description:      Some("International Standard Book Number".to_string()),
        specified_by_url: Some("https://www.isbn-international.org/".to_string()),
        validation_rules: vec![],
        elo_expression:   Some("matches(value, /^[0-9-]{10,17}$/)".to_string()),
        base_type:        Some("String".to_string()),
    };

    assert_eq!(def.name, "ISBN");
    assert_eq!(def.description, Some("International Standard Book Number".to_string()));
    assert_eq!(def.specified_by_url, Some("https://www.isbn-international.org/".to_string()));
    assert_eq!(def.elo_expression, Some("matches(value, /^[0-9-]{10,17}$/)".to_string()));
    assert_eq!(def.base_type, Some("String".to_string()));
}

#[test]
fn test_custom_type_def_equality() {
    let def1 = CustomTypeDef::new("Email".to_string());
    let def2 = CustomTypeDef::new("Email".to_string());

    assert_eq!(def1, def2);
}

#[test]
fn test_registry_register_and_get() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("Email".to_string());

    registry.register("Email".to_string(), def.clone())
        .unwrap_or_else(|e| panic!("register should succeed for new type: {e}"));
    assert_eq!(registry.get("Email"), Some(def));
}

#[test]
fn test_registry_register_duplicate() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("Email".to_string());

    registry.register("Email".to_string(), def.clone())
        .unwrap_or_else(|e| panic!("first register should succeed: {e}"));
    let result = registry.register("Email".to_string(), def);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("already registered")),
        "expected Validation error about duplicate registration, got: {result:?}"
    );
}

#[test]
fn test_registry_exists() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("Email".to_string());

    assert!(!registry.exists("Email"));
    registry.register("Email".to_string(), def).unwrap();
    assert!(registry.exists("Email"));
}

#[test]
fn test_registry_remove() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("Email".to_string());

    registry.register("Email".to_string(), def.clone()).unwrap();
    assert_eq!(registry.remove("Email"), Some(def));
    assert!(!registry.exists("Email"));
}

#[test]
fn test_registry_count() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

    assert_eq!(registry.count(), 0);
    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();
    assert_eq!(registry.count(), 1);
    registry
        .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
        .unwrap();
    assert_eq!(registry.count(), 2);
}

#[test]
fn test_registry_list_all() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();
    registry
        .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
        .unwrap();

    let all = registry.list_all();
    assert_eq!(all.len(), 2);
    assert!(all.iter().any(|(name, _)| name == "Email"));
    assert!(all.iter().any(|(name, _)| name == "ISBN"));
}

#[test]
fn test_registry_clear() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());

    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();
    assert_eq!(registry.count(), 1);

    registry.clear();
    assert_eq!(registry.count(), 0);
}

#[test]
fn test_registry_max_scalars_limit() {
    let config = CustomTypeRegistryConfig {
        max_scalars:    Some(2),
        enable_caching: false,
    };
    let registry = CustomTypeRegistry::new(config);

    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();
    registry
        .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
        .unwrap();

    // Third registration should fail
    let result = registry.register("IBAN".to_string(), CustomTypeDef::new("IBAN".to_string()));
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("max scalars limit")),
        "expected Validation error about max scalars limit, got: {result:?}"
    );
}

#[test]
fn test_registry_concurrent_reads() {
    use std::{sync::Arc as StdArc, thread};

    let registry = StdArc::new(CustomTypeRegistry::new(CustomTypeRegistryConfig::default()));
    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();

    let mut handles = vec![];
    for _ in 0..5 {
        let reg = StdArc::clone(&registry);
        let handle = thread::spawn(move || {
            assert!(reg.exists("Email"));
            assert_eq!(reg.count(), 1);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_validate_unknown_scalar() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let value = serde_json::json!("some-value");

    let result = registry.validate("UnknownType", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Unknown custom scalar")),
        "expected Validation error about unknown scalar type, got: {result:?}"
    );
}

#[test]
fn test_validate_library_code_minimal() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("LibraryCode".to_string());

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("LIB-1234");
    let result = registry.validate("LibraryCode", &value);
    result.unwrap_or_else(|e| panic!("minimal LibraryCode validation should pass: {e}"));
}

#[test]
fn test_validate_student_id_minimal() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("StudentID".to_string());

    registry.register("StudentID".to_string(), def).unwrap();

    let value = serde_json::json!("STU-2024-001");
    let result = registry.validate("StudentID", &value);
    result.unwrap_or_else(|e| panic!("minimal StudentID validation should pass: {e}"));
}

#[test]
fn test_validate_patient_id_minimal() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let def = CustomTypeDef::new("PatientID".to_string());

    registry.register("PatientID".to_string(), def).unwrap();

    let value = serde_json::json!("PAT-987654");
    let result = registry.validate("PatientID", &value);
    result.unwrap_or_else(|e| panic!("minimal PatientID validation should pass: {e}"));
}

#[test]
fn test_validate_with_pattern_rule_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.validation_rules = vec![ValidationRule::Pattern {
        pattern: r"^LIB-[0-9]{4}$".to_string(),
        message: Some("Library code must be LIB-#### format".to_string()),
    }];

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("LIB-1234");
    let result = registry.validate("LibraryCode", &value);
    result.unwrap_or_else(|e| panic!("valid pattern should pass: {e}"));
}

#[test]
fn test_validate_with_pattern_rule_invalid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.validation_rules = vec![ValidationRule::Pattern {
        pattern: r"^LIB-[0-9]{4}$".to_string(),
        message: Some("Library code must be LIB-#### format".to_string()),
    }];

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("INVALID");
    let result = registry.validate("LibraryCode", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Library code must be LIB-####")),
        "expected Validation error with custom pattern message, got: {result:?}"
    );
}

#[test]
fn test_validate_with_length_rule_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("StudentID".to_string());
    def.validation_rules = vec![ValidationRule::Length {
        min: Some(5),
        max: Some(15),
    }];

    registry.register("StudentID".to_string(), def).unwrap();

    let value = serde_json::json!("STU-2024"); // 8 chars, within 5-15
    let result = registry.validate("StudentID", &value);
    result.unwrap_or_else(|e| panic!("valid length should pass: {e}"));
}

#[test]
fn test_validate_with_length_rule_too_short() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("StudentID".to_string());
    def.validation_rules = vec![ValidationRule::Length {
        min: Some(5),
        max: Some(15),
    }];

    registry.register("StudentID".to_string(), def).unwrap();

    let value = serde_json::json!("STU"); // 3 chars, below min of 5
    let result = registry.validate("StudentID", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("at least") && message.contains('5')),
        "expected Validation error about minimum length, got: {result:?}"
    );
}

#[test]
fn test_validate_with_multiple_rules() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("PatientID".to_string());
    def.validation_rules = vec![
        ValidationRule::Pattern {
            pattern: r"^PAT-[0-9]{6}$".to_string(),
            message: Some("Patient ID must be PAT-###### format".to_string()),
        },
        ValidationRule::Length {
            min: Some(10),
            max: Some(10),
        },
    ];

    registry.register("PatientID".to_string(), def).unwrap();

    // Valid: matches pattern and length
    let value_valid = serde_json::json!("PAT-123456");
    registry.validate("PatientID", &value_valid)
        .unwrap_or_else(|e| panic!("valid value should pass all rules: {e}"));

    // Invalid: wrong pattern but right length
    let value_invalid_pattern = serde_json::json!("PAT-12345X");
    let result = registry.validate("PatientID", &value_invalid_pattern);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Patient ID must be PAT-######")),
        "expected Validation error about pattern mismatch, got: {result:?}"
    );
}

#[test]
fn test_validate_library_code_with_elo_expression_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("LIB-1234");
    let result = registry.validate("LibraryCode", &value);
    result.unwrap_or_else(|e| panic!("valid LibraryCode with ELO should pass: {e}"));
}

#[test]
fn test_validate_library_code_with_elo_expression_invalid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("INVALID-CODE");
    let result = registry.validate("LibraryCode", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for invalid ELO expression, got: {result:?}"
    );
}

#[test]
fn test_validate_student_id_with_elo_expression_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("StudentID".to_string());
    def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

    registry.register("StudentID".to_string(), def).unwrap();

    let value = serde_json::json!("STU-2024-001");
    let result = registry.validate("StudentID", &value);
    result.unwrap_or_else(|e| panic!("valid StudentID with ELO should pass: {e}"));
}

#[test]
fn test_validate_student_id_with_elo_expression_invalid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("StudentID".to_string());
    def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

    registry.register("StudentID".to_string(), def).unwrap();

    let value = serde_json::json!("STUDENT-2024");
    let result = registry.validate("StudentID", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for invalid StudentID ELO, got: {result:?}"
    );
}

#[test]
fn test_validate_patient_id_with_elo_expression_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("PatientID".to_string());
    def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

    registry.register("PatientID".to_string(), def).unwrap();

    let value = serde_json::json!("PAT-987654");
    let result = registry.validate("PatientID", &value);
    result.unwrap_or_else(|e| panic!("valid PatientID with ELO should pass: {e}"));
}

#[test]
fn test_validate_patient_id_with_elo_expression_invalid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("PatientID".to_string());
    def.elo_expression = Some("matches(value, \"^PAT-[0-9]{6}$\")".to_string());

    registry.register("PatientID".to_string(), def).unwrap();

    let value = serde_json::json!("PATIENT123");
    let result = registry.validate("PatientID", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error for invalid PatientID ELO, got: {result:?}"
    );
}

#[test]
fn test_validate_rules_then_elo_expression_both_valid() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.validation_rules = vec![ValidationRule::Length {
        min: Some(8),
        max: Some(8),
    }];
    def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

    registry.register("LibraryCode".to_string(), def).unwrap();

    let value = serde_json::json!("LIB-1234");
    let result = registry.validate("LibraryCode", &value);
    result.unwrap_or_else(|e| panic!("both rules and ELO should pass: {e}"));
}

#[test]
fn test_validate_rules_pass_elo_fails() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("LibraryCode".to_string());
    def.validation_rules = vec![ValidationRule::Length {
        min: Some(8),
        max: Some(8),
    }];
    def.elo_expression = Some("matches(value, \"^LIB-[0-9]{4}$\")".to_string());

    registry.register("LibraryCode".to_string(), def).unwrap();

    // This passes the length rule but fails the ELO pattern
    let value = serde_json::json!("NOTVALID");
    let result = registry.validate("LibraryCode", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error when rules pass but ELO fails, got: {result:?}"
    );
}

#[test]
fn test_validate_rules_fail_elo_not_evaluated() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("StudentID".to_string());
    def.validation_rules = vec![ValidationRule::Length {
        min: Some(10),
        max: Some(10),
    }];
    def.elo_expression = Some("matches(value, \"^STU-[0-9]{4}-[0-9]{3}$\")".to_string());

    registry.register("StudentID".to_string(), def).unwrap();

    // This fails the length rule, so ELO should not be evaluated
    let value = serde_json::json!("STU-2024");
    let result = registry.validate("StudentID", &value);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("at least")),
        "expected Validation error about length rule failure, got: {result:?}"
    );
}

#[test]
fn test_validate_complex_elo_expression_with_multiple_conditions() {
    let registry = CustomTypeRegistry::new(CustomTypeRegistryConfig::default());
    let mut def = CustomTypeDef::new("PatientID".to_string());
    // Match pattern OR contains substring
    def.elo_expression =
        Some("matches(value, \"^PAT-[0-9]{6}$\") || contains(value, \"URGENT\")".to_string());

    registry.register("PatientID".to_string(), def).unwrap();

    // Valid: matches pattern
    let value1 = serde_json::json!("PAT-123456");
    registry.validate("PatientID", &value1)
        .unwrap_or_else(|e| panic!("pattern match should pass: {e}"));

    // Valid: contains substring (even though doesn't match pattern)
    let value2 = serde_json::json!("URGENT-CASE");
    registry.validate("PatientID", &value2)
        .unwrap_or_else(|e| panic!("contains substring should pass: {e}"));

    // Invalid: neither matches pattern nor contains substring
    let value3 = serde_json::json!("INVALID");
    let result = registry.validate("PatientID", &value3);
    assert!(
        matches!(result, Err(FraiseQLError::Validation { .. })),
        "expected Validation error when no ELO condition matches, got: {result:?}"
    );
}

#[test]
fn test_with_builtin_rich_scalars_count() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();
    assert_eq!(registry.count(), 51);
}

#[test]
fn test_with_builtin_rich_scalars_contact_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Contact/Communication scalars
    assert!(registry.exists("Email"));
    assert!(registry.exists("PhoneNumber"));
    assert!(registry.exists("URL"));
    assert!(registry.exists("DomainName"));
    assert!(registry.exists("Hostname"));
}

#[test]
fn test_with_builtin_rich_scalars_location_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Location/Address scalars
    assert!(registry.exists("PostalCode"));
    assert!(registry.exists("Latitude"));
    assert!(registry.exists("Longitude"));
    assert!(registry.exists("Coordinates"));
    assert!(registry.exists("Timezone"));
    assert!(registry.exists("LocaleCode"));
    assert!(registry.exists("LanguageCode"));
    assert!(registry.exists("CountryCode"));
}

#[test]
fn test_with_builtin_rich_scalars_financial_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Financial scalars
    assert!(registry.exists("IBAN"));
    assert!(registry.exists("CUSIP"));
    assert!(registry.exists("ISIN"));
    assert!(registry.exists("SEDOL"));
    assert!(registry.exists("LEI"));
    assert!(registry.exists("MIC"));
    assert!(registry.exists("CurrencyCode"));
    assert!(registry.exists("Money"));
    assert!(registry.exists("ExchangeCode"));
    assert!(registry.exists("ExchangeRate"));
    assert!(registry.exists("StockSymbol"));
}

#[test]
fn test_with_builtin_rich_scalars_identifier_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Identifier scalars
    assert!(registry.exists("Slug"));
    assert!(registry.exists("SemanticVersion"));
    assert!(registry.exists("HashSHA256"));
    assert!(registry.exists("APIKey"));
    assert!(registry.exists("LicensePlate"));
    assert!(registry.exists("VIN"));
    assert!(registry.exists("TrackingNumber"));
    assert!(registry.exists("ContainerNumber"));
}

#[test]
fn test_with_builtin_rich_scalars_networking_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Networking scalars
    assert!(registry.exists("IPAddress"));
    assert!(registry.exists("IPv4"));
    assert!(registry.exists("IPv6"));
    assert!(registry.exists("MACAddress"));
    assert!(registry.exists("CIDR"));
    assert!(registry.exists("Port"));
}

#[test]
fn test_with_builtin_rich_scalars_transportation_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Transportation scalars
    assert!(registry.exists("AirportCode"));
    assert!(registry.exists("PortCode"));
    assert!(registry.exists("FlightNumber"));
}

#[test]
fn test_with_builtin_rich_scalars_content_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Content scalars
    assert!(registry.exists("Markdown"));
    assert!(registry.exists("HTML"));
    assert!(registry.exists("MimeType"));
    assert!(registry.exists("Color"));
    assert!(registry.exists("Image"));
    assert!(registry.exists("File"));
}

#[test]
fn test_with_builtin_rich_scalars_database_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Database/PostgreSQL scalars
    assert!(registry.exists("LTree"));
}

#[test]
fn test_with_builtin_rich_scalars_range_types() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Range scalars
    assert!(registry.exists("DateRange"));
    assert!(registry.exists("Duration"));
    assert!(registry.exists("Percentage"));
}

#[test]
fn test_with_builtin_rich_scalars_has_base_type() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // All rich scalars should have base_type = String
    let email = registry.get("Email").unwrap();
    assert_eq!(email.base_type, Some("String".to_string()));

    let iban = registry.get("IBAN").unwrap();
    assert_eq!(iban.base_type, Some("String".to_string()));

    let percentage = registry.get("Percentage").unwrap();
    assert_eq!(percentage.base_type, Some("String".to_string()));
}

#[test]
fn test_with_builtin_rich_scalars_no_validation_rules() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Rich scalars should not have validation rules
    let email = registry.get("Email").unwrap();
    assert_eq!(email.validation_rules.len(), 0);

    let iban = registry.get("IBAN").unwrap();
    assert_eq!(iban.validation_rules.len(), 0);
}

#[test]
fn test_with_builtin_rich_scalars_backward_compatibility() {
    let registry = CustomTypeRegistry::with_builtin_rich_scalars();

    // Verify all scalars from the old RICH_SCALARS constant are present
    let all_rich_scalar_names = [
        "Email",
        "PhoneNumber",
        "URL",
        "DomainName",
        "Hostname",
        "PostalCode",
        "Latitude",
        "Longitude",
        "Coordinates",
        "Timezone",
        "LocaleCode",
        "LanguageCode",
        "CountryCode",
        "IBAN",
        "CUSIP",
        "ISIN",
        "SEDOL",
        "LEI",
        "MIC",
        "CurrencyCode",
        "Money",
        "ExchangeCode",
        "ExchangeRate",
        "StockSymbol",
        "Slug",
        "SemanticVersion",
        "HashSHA256",
        "APIKey",
        "LicensePlate",
        "VIN",
        "TrackingNumber",
        "ContainerNumber",
        "IPAddress",
        "IPv4",
        "IPv6",
        "MACAddress",
        "CIDR",
        "Port",
        "AirportCode",
        "PortCode",
        "FlightNumber",
        "Markdown",
        "HTML",
        "MimeType",
        "Color",
        "Image",
        "File",
        "LTree",
        "DateRange",
        "Duration",
        "Percentage",
    ];

    for scalar_name in &all_rich_scalar_names {
        assert!(
            registry.exists(scalar_name),
            "Rich scalar '{}' should be registered",
            scalar_name
        );
    }
}

/// Structural test for the poisoning-recovery code paths.
///
/// True `RwLock` poisoning requires a thread to panic while holding the lock,
/// which cannot be triggered without unsafe code or spawning panicking threads.
/// This test validates the happy-path behaviour of all methods that have
/// poisoning-recovery logic, ensuring they return correct results in the
/// non-poisoned case and that the recovery branches compile correctly.
#[test]
fn registry_lock_recovery_code_paths_compile_and_return_correct_values() {
    use std::sync::Arc as StdArc;
    let registry = StdArc::new(CustomTypeRegistry::new(CustomTypeRegistryConfig::default()));
    registry
        .register("Email".to_string(), CustomTypeDef::new("Email".to_string()))
        .unwrap();

    // All read-path methods should return correct values (exercising the
    // unwrap_or_else branches in non-poisoned state).
    assert!(registry.exists("Email"));
    assert!(!registry.exists("Unknown"));
    assert_eq!(registry.count(), 1);
    assert!(registry.get("Email").is_some());
    assert_eq!(registry.list_all().len(), 1);

    // Write-path methods should also work correctly.
    registry
        .register("ISBN".to_string(), CustomTypeDef::new("ISBN".to_string()))
        .unwrap();
    assert_eq!(registry.count(), 2);
    let removed = registry.remove("ISBN");
    assert!(removed.is_some());
    assert_eq!(registry.count(), 1);
    registry.clear();
    assert_eq!(registry.count(), 0);
}
