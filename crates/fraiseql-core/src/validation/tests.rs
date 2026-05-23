//! Tests for `validation/` modules.
//! Re-export commonly-needed items from sibling modules so submodules can reach them
//! via `use super::*` after the `use super::super::*` wildcard import.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience

pub use std::{collections::HashMap, sync::Arc, time::Duration};

pub use fraiseql_error::{FraiseQLError, ValidationFieldError};
pub use serde_json::{Value, json};

pub use crate::{
    utils::clock::Clock,
    validation::{
        custom_scalar::CustomScalarResult,
        error_responses::GraphQLValidationResponse,
        id_policy::{
            IdValidator, NumericIdValidator, OpaqueIdValidator, UlidIdValidator, UuidIdValidator,
            validate_ids,
        },
        validators::{RequiredValidator, create_validator_from_rule},
    },
};

mod async_validators_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // ── EmailFormatValidator ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_email_valid_simple() {
        let v = EmailFormatValidator::new();
        v.validate_async("user@example.com", "email")
            .await
            .unwrap_or_else(|e| panic!("valid simple email should pass: {e}"));
    }

    #[tokio::test]
    async fn test_email_valid_subdomain() {
        let v = EmailFormatValidator::new();
        v.validate_async("user@mail.example.co.uk", "email")
            .await
            .unwrap_or_else(|e| panic!("valid subdomain email should pass: {e}"));
    }

    #[tokio::test]
    async fn test_email_valid_plus_addressing() {
        let v = EmailFormatValidator::new();
        v.validate_async("user+tag@example.com", "email")
            .await
            .unwrap_or_else(|e| panic!("valid plus-addressed email should pass: {e}"));
    }

    #[tokio::test]
    async fn test_email_valid_corporate_domain() {
        let v = EmailFormatValidator::new();
        // Must accept any valid domain, not a hardcoded allowlist
        v.validate_async("alice@my-company.io", "email")
            .await
            .unwrap_or_else(|e| panic!("valid corporate email should pass: {e}"));
        v.validate_async("bob@university.edu", "email")
            .await
            .unwrap_or_else(|e| panic!("valid edu email should pass: {e}"));
    }

    #[tokio::test]
    async fn test_email_invalid_no_at() {
        let v = EmailFormatValidator::new();
        let result = v.validate_async("notanemail", "email").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid email format")),
            "expected Validation error about invalid email format, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_email_invalid_no_tld() {
        let v = EmailFormatValidator::new();
        // Single label after @ has no dot — rejected
        let result = v.validate_async("user@localhost", "email").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid email format")),
            "expected Validation error about invalid email format, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_email_invalid_empty() {
        let v = EmailFormatValidator::new();
        let result = v.validate_async("", "email").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid email format")),
            "expected Validation error about invalid email format, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_email_error_message_contains_field() {
        let v = EmailFormatValidator::new();
        let err = v.validate_async("bad", "contact_email").await.unwrap_err();
        assert!(err.to_string().contains("contact_email"));
    }

    // ── PhoneE164Validator ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_phone_valid_us() {
        let v = PhoneE164Validator::new();
        v.validate_async("+14155552671", "phone")
            .await
            .unwrap_or_else(|e| panic!("valid US phone should pass: {e}"));
    }

    #[tokio::test]
    async fn test_phone_valid_uk() {
        let v = PhoneE164Validator::new();
        v.validate_async("+447911123456", "phone")
            .await
            .unwrap_or_else(|e| panic!("valid UK phone should pass: {e}"));
    }

    #[tokio::test]
    async fn test_phone_valid_any_country_code() {
        let v = PhoneE164Validator::new();
        // Must accept all country codes, not a hardcoded subset
        v.validate_async("+819012345678", "phone")
            .await
            .unwrap_or_else(|e| panic!("valid Japan phone should pass: {e}"));
        v.validate_async("+5511987654321", "phone")
            .await
            .unwrap_or_else(|e| panic!("valid Brazil phone should pass: {e}"));
        v.validate_async("+27821234567", "phone")
            .await
            .unwrap_or_else(|e| panic!("valid South Africa phone should pass: {e}"));
    }

    #[tokio::test]
    async fn test_phone_invalid_missing_plus() {
        let v = PhoneE164Validator::new();
        let result = v.validate_async("14155552671", "phone").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid E.164")),
            "expected Validation error about invalid E.164 phone, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_phone_invalid_too_short() {
        let v = PhoneE164Validator::new();
        // 5 digits after + — below E.164 minimum of 7
        let result = v.validate_async("+12345", "phone").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid E.164")),
            "expected Validation error about invalid E.164 phone, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_phone_invalid_too_long() {
        let v = PhoneE164Validator::new();
        // 16 digits after + — above E.164 maximum of 15
        let result = v.validate_async("+1234567890123456", "phone").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid E.164")),
            "expected Validation error about invalid E.164 phone, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_phone_invalid_leading_zero_country_code() {
        let v = PhoneE164Validator::new();
        let result = v.validate_async("+0441234567890", "phone").await;
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Invalid E.164")),
            "expected Validation error about invalid E.164 phone, got: {result:?}"
        );
    }

    #[tokio::test]
    async fn test_phone_error_message_contains_field() {
        let v = PhoneE164Validator::new();
        let err = v.validate_async("bad", "mobile_number").await.unwrap_err();
        assert!(err.to_string().contains("mobile_number"));
    }

    // ── AsyncValidatorConfig ──────────────────────────────────────────────────

    #[test]
    fn test_async_validator_config() {
        let config = AsyncValidatorConfig::new(AsyncValidatorProvider::EmailFormatCheck, 5000)
            .with_cache_ttl(3600)
            .with_field_pattern("*.email");

        assert_eq!(config.provider, AsyncValidatorProvider::EmailFormatCheck);
        assert_eq!(config.timeout, Duration::from_secs(5));
        assert_eq!(config.cache_ttl_secs, 3600);
        assert_eq!(config.field_pattern, "*.email");
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(AsyncValidatorProvider::EmailFormatCheck.to_string(), "email_format_check");
        assert_eq!(AsyncValidatorProvider::PhoneE164Check.to_string(), "phone_e164_check");
    }

    #[test]
    fn test_email_validator_timeout_is_max() {
        // Duration::MAX signals no-timeout for local-only (regex) validators
        let v = EmailFormatValidator::new();
        assert_eq!(v.timeout(), Duration::MAX);
    }

    #[test]
    fn test_phone_validator_timeout_is_max() {
        // Duration::MAX signals no-timeout for local-only (regex) validators
        let v = PhoneE164Validator::new();
        assert_eq!(v.timeout(), Duration::MAX);
    }
}

mod checksum_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // Luhn tests
    #[test]
    fn test_luhn_valid_visa() {
        assert!(LuhnValidator::validate("4532015112830366"));
    }

    #[test]
    fn test_luhn_valid_another_visa() {
        assert!(LuhnValidator::validate("4111111111111111"));
    }

    #[test]
    fn test_luhn_invalid_checksum() {
        assert!(!LuhnValidator::validate("4532015112830367"));
    }

    #[test]
    fn test_luhn_invalid_non_digits() {
        assert!(!LuhnValidator::validate("4532-0151-1283-0366"));
    }

    #[test]
    fn test_luhn_empty_string() {
        assert!(!LuhnValidator::validate(""));
    }

    #[test]
    fn test_luhn_single_digit() {
        assert!(LuhnValidator::validate("0"));
    }

    #[test]
    fn test_luhn_all_zeros() {
        assert!(LuhnValidator::validate("0000000000000000"));
    }

    // MOD-97 tests
    #[test]
    fn test_mod97_valid_iban_gb() {
        assert!(Mod97Validator::validate("GB82WEST12345698765432"));
    }

    #[test]
    fn test_mod97_valid_iban_de() {
        assert!(Mod97Validator::validate("DE89370400440532013000"));
    }

    #[test]
    fn test_mod97_invalid_checksum() {
        assert!(!Mod97Validator::validate("GB82WEST12345698765433"));
    }

    #[test]
    fn test_mod97_invalid_too_short() {
        assert!(!Mod97Validator::validate("GB8"));
    }

    #[test]
    fn test_mod97_invalid_special_chars() {
        assert!(!Mod97Validator::validate("GB82-WEST-1234"));
    }

    #[test]
    fn test_mod97_lowercase_conversion() {
        // Should handle lowercase by converting to uppercase
        assert!(Mod97Validator::validate("gb82west12345698765432"));
    }

    #[test]
    fn test_mod97_error_message() {
        assert_eq!(Mod97Validator::error_message(), "Invalid checksum (MOD-97 algorithm)");
    }

    #[test]
    fn test_luhn_error_message() {
        assert_eq!(LuhnValidator::error_message(), "Invalid checksum (Luhn algorithm)");
    }

    // ── Length-guard tests ────────────────────────────────────────────────────

    #[test]
    fn test_luhn_exactly_25_digits_accepted() {
        // 25 digits that pass Luhn (craft a valid one: 24 zeros + check digit 0)
        let value = "0".repeat(25);
        // All-zeros passes Luhn (sum = 0, 0 % 10 == 0)
        assert!(LuhnValidator::validate(&value));
    }

    #[test]
    fn test_luhn_26_digits_rejected_by_length_guard() {
        let value = "0".repeat(26);
        assert!(!LuhnValidator::validate(&value), "26-digit string must be rejected");
    }

    #[test]
    fn test_mod97_exactly_34_chars_accepted_structure() {
        // GB IBAN is 22 chars; build a syntactically valid 34-char string
        // (all A's — will fail mod-97 checksum but must NOT be rejected by length guard)
        let value = "A".repeat(34);
        // Will be false (checksum fails) but must not panic; length guard allows it through
        let _ = Mod97Validator::validate(&value);
        // Just verify 34-char input is not immediately rejected (returns false for checksum, not
        // length) We can't check internal path from outside, so we verify no panic occurs
        // and the function runs.
    }

    #[test]
    fn test_mod97_35_chars_rejected_by_length_guard() {
        let value = "A".repeat(35);
        assert!(
            !Mod97Validator::validate(&value),
            "35-char string must be rejected by length guard"
        );
    }

    #[test]
    fn test_mod97_valid_iban_within_length_limit() {
        // Verify an actual valid IBAN still passes after adding the length guard.
        assert!(Mod97Validator::validate("GB82WEST12345698765432")); // 22 chars — well within limit
    }
}

mod compile_time_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    fn create_test_context() -> SchemaContext {
        let mut types = HashMap::new();
        let mut fields = HashMap::new();

        // Create User type
        types.insert(
            "User".to_string(),
            TypeDef {
                name:   "User".to_string(),
                fields: vec![
                    "email".to_string(),
                    "age".to_string(),
                    "birthDate".to_string(),
                    "verified".to_string(),
                ],
            },
        );

        fields.insert(("User".to_string(), "email".to_string()), FieldType::String);
        fields.insert(("User".to_string(), "age".to_string()), FieldType::Integer);
        fields.insert(("User".to_string(), "birthDate".to_string()), FieldType::Date);
        fields.insert(("User".to_string(), "verified".to_string()), FieldType::Boolean);

        // Create DateRange type
        types.insert(
            "DateRange".to_string(),
            TypeDef {
                name:   "DateRange".to_string(),
                fields: vec!["startDate".to_string(), "endDate".to_string()],
            },
        );

        fields.insert(("DateRange".to_string(), "startDate".to_string()), FieldType::Date);
        fields.insert(("DateRange".to_string(), "endDate".to_string()), FieldType::Date);

        SchemaContext { types, fields }
    }

    // ========== CROSS-FIELD RULE VALIDATION ==========

    #[test]
    fn test_valid_cross_field_comparison() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<", "endDate");

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.sql_constraint.is_some());
    }

    #[test]
    fn test_cross_field_type_mismatch() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "age", "<", "verified");

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].field, "age < verified");
    }

    #[test]
    fn test_cross_field_left_field_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "nonexistent", "<", "age");

        assert!(!result.valid);
        assert_eq!(result.errors[0].field, "nonexistent");
        assert!(result.errors[0].message.contains("not found"));
    }

    #[test]
    fn test_cross_field_right_field_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "age", "<", "nonexistent");

        assert!(!result.valid);
        assert_eq!(result.errors[0].field, "nonexistent");
    }

    #[test]
    fn test_cross_field_type_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("NonexistentType", "field", "<", "field2");

        assert!(!result.valid);
        assert!(result.errors[0].message.contains("not found"));
    }

    // ========== TYPE COMPATIBILITY ==========

    #[test]
    fn test_same_types_compatible() {
        let left = FieldType::Integer;
        let right = FieldType::Integer;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_numeric_types_compatible() {
        let left = FieldType::Integer;
        let right = FieldType::Float;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_date_datetime_compatible() {
        let left = FieldType::Date;
        let right = FieldType::DateTime;
        assert!(left.is_comparable_with(&right));
    }

    #[test]
    fn test_string_number_incompatible() {
        let left = FieldType::String;
        let right = FieldType::Integer;
        assert!(!left.is_comparable_with(&right));
    }

    #[test]
    fn test_boolean_incompatible_with_numbers() {
        let left = FieldType::Boolean;
        let right = FieldType::Integer;
        assert!(!left.is_comparable_with(&right));
    }

    // ========== SQL CONSTRAINT GENERATION ==========

    #[test]
    fn test_sql_constraint_generated() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<=", "endDate");

        assert!(result.valid);
        assert!(result.sql_constraint.is_some());
        let sql = result
            .sql_constraint
            .expect("sql_constraint must be Some when result.valid is true");
        assert!(sql.contains("CHECK"));
        assert!(sql.contains("startDate"));
        assert!(sql.contains("<="));
        assert!(sql.contains("endDate"));
    }

    #[test]
    fn test_sql_constraint_with_different_operators() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let operators = vec!["<", ">", "<=", ">=", "==", "!="];
        for op in operators {
            let result =
                validator.validate_cross_field_rule("DateRange", "startDate", op, "endDate");

            assert!(result.valid);
            let sql =
                result.sql_constraint.expect("sql_constraint must be Some for valid operator");
            assert!(sql.contains(op) || op == "==" && sql.contains('='));
        }
    }

    // ========== ELO EXPRESSION VALIDATION ==========

    #[test]
    fn test_valid_elo_expression() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "age >= 18 && verified == true");

        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_elo_expression_unknown_field() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "nonexistent >= 18");

        assert!(!result.valid);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_elo_expression_type_not_found() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("NonexistentType", "age >= 18");

        assert!(!result.valid);
    }

    #[test]
    fn test_elo_field_reference_extraction() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let fields = validator.extract_field_references("age >= 18 && verified == true");

        assert!(fields.contains(&"age".to_string()));
        assert!(fields.contains(&"verified".to_string()));
    }

    #[test]
    fn test_elo_field_extraction_with_strings() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let fields = validator.extract_field_references("email matches \"pattern\" && age > 10");

        assert!(fields.contains(&"email".to_string()));
        assert!(fields.contains(&"age".to_string()));
        assert!(!fields.contains(&"pattern".to_string())); // Inside quotes
    }

    // ========== REAL-WORLD PATTERNS ==========

    #[test]
    fn test_date_range_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("DateRange", "startDate", "<=", "endDate");

        assert!(result.valid);
        let sql = result
            .sql_constraint
            .expect("sql_constraint must be Some when result.valid is true");
        assert!(sql.contains("CHECK"));
    }

    #[test]
    fn test_age_constraint() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression("User", "age >= 18 && age <= 120");

        assert!(result.valid);
    }

    #[test]
    fn test_email_field_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression(
            "User",
            "email matches \"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\"",
        );

        assert!(result.valid);
    }

    #[test]
    fn test_complex_user_validation() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_elo_expression(
            "User",
            "email matches pattern && age >= 18 && verified == true",
        );

        assert!(result.valid);
    }

    #[test]
    fn test_suggestion_on_typo() {
        let context = create_test_context();
        let validator = CompileTimeValidator::new(context);

        let result = validator.validate_cross_field_rule("User", "typ0", "<", "age");

        assert!(!result.valid);
        assert!(result.errors[0].suggestion.is_some());
        assert!(
            result.errors[0]
                .suggestion
                .as_ref()
                .expect("suggestion must be Some for typo error")
                .contains("Available fields")
        );
    }
}

mod composite_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;
    use crate::validation::{composite::validate_single_rule, rules::ValidationRule};

    #[test]
    fn test_validate_all_passes() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let result = validate_all(&rules, "hello123", "password", true);
        result.unwrap_or_else(|e| panic!("expected all validators to pass: {e}"));
    }

    #[test]
    fn test_validate_all_fails_first() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(10),
                max: None,
            },
        ];
        let result = validate_all(&rules, "short", "password", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("All validators must pass")),
            "expected Validation error with 'All validators must pass', got: {result:?}"
        );
    }

    #[test]
    fn test_validate_all_fails_second() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_all(&rules, "Hello123", "username", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("All validators must pass")),
            "expected Validation error with 'All validators must pass', got: {result:?}"
        );
    }

    #[test]
    fn test_validate_all_multiple_failures() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(10),
                max: None,
            },
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_all(&rules, "Hi", "field", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("All validators must pass")),
            "expected Validation error with 'All validators must pass', got: {result:?}"
        );
    }

    #[test]
    fn test_validate_any_passes_first() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(20),
                max: None,
            },
        ];
        let result = validate_any(&rules, "abc", "field", true);
        result.unwrap_or_else(|e| panic!("expected any validator to pass: {e}"));
    }

    #[test]
    fn test_validate_any_passes_second() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(2),
                max: None,
            },
        ];
        let result = validate_any(&rules, "Hi", "field", true);
        result.unwrap_or_else(|e| panic!("expected any validator to pass: {e}"));
    }

    #[test]
    fn test_validate_any_fails_all() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(20),
                max: None,
            },
        ];
        let result = validate_any(&rules, "Hi", "field", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("At least one validator must pass")),
            "expected Validation error with 'At least one validator must pass', got: {result:?}"
        );
    }

    #[test]
    fn test_validate_any_multiple_passes() {
        let rules = vec![
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            ValidationRule::Length {
                min: Some(2),
                max: None,
            },
            ValidationRule::Enum {
                values: vec!["hello".to_string(), "world".to_string()],
            },
        ];
        let result = validate_any(&rules, "hello", "field", true);
        result.unwrap_or_else(|e| panic!("expected any validator to pass: {e}"));
    }

    #[test]
    fn test_validate_not_passes_when_rule_fails() {
        let rule = ValidationRule::Pattern {
            pattern: "^[0-9]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc", "field", true);
        result.unwrap_or_else(|e| panic!("expected not-validator to pass when rule fails: {e}"));
    }

    #[test]
    fn test_validate_not_fails_when_rule_passes() {
        let rule = ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc", "field", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must fail but passed")),
            "expected Validation error with 'must fail but passed', got: {result:?}"
        );
    }

    #[test]
    fn test_validate_optional_skips_absent() {
        let rule = ValidationRule::Length {
            min: Some(100),
            max: None,
        };
        let result = validate_optional(&rule, "", "field", false);
        result.unwrap_or_else(|e| panic!("expected optional to skip absent field: {e}"));
    }

    #[test]
    fn test_validate_optional_applies_present() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: None,
        };
        let result = validate_optional(&rule, "hello", "field", true);
        result.unwrap_or_else(|e| panic!("expected optional to pass for valid present field: {e}"));
    }

    #[test]
    fn test_validate_optional_fails_present() {
        let rule = ValidationRule::Length {
            min: Some(10),
            max: None,
        };
        let result = validate_optional(&rule, "hi", "field", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for present field failing rule, got: {result:?}"
        );
    }

    #[test]
    fn test_composite_operator_display() {
        assert_eq!(CompositeOperator::All.to_string(), "All validators must pass");
        assert_eq!(CompositeOperator::Any.to_string(), "At least one validator must pass");
        assert_eq!(CompositeOperator::Not.to_string(), "Validator must fail");
        assert_eq!(CompositeOperator::Optional.to_string(), "Optional validation");
    }

    #[test]
    fn test_nested_all_and_pattern() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: Some(20),
            },
            ValidationRule::Pattern {
                pattern: "^[A-Za-z0-9]+$".to_string(),
                message: Some("Username must be alphanumeric".to_string()),
            },
        ];
        let result = validate_all(&rules, "User1234", "username", true);
        result.unwrap_or_else(|e| panic!("expected nested all+pattern to pass: {e}"));
    }

    #[test]
    fn test_nested_all_fails_on_length() {
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: Some(20),
            },
            ValidationRule::Pattern {
                pattern: "^[A-Za-z0-9]+$".to_string(),
                message: Some("Username must be alphanumeric".to_string()),
            },
        ];
        let result = validate_all(&rules, "Hi", "username", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("All validators must pass")),
            "expected Validation error for length failure, got: {result:?}"
        );
    }

    #[test]
    fn test_strong_password_pattern_all() {
        // Strong password: at least 1 uppercase, 1 lowercase, 1 digit
        let rules = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(8),
                max: None,
            },
            ValidationRule::Pattern {
                pattern: "^(?=.*[A-Z])".to_string(), // Lookahead for uppercase
                message: Some("Must contain at least one uppercase letter".to_string()),
            },
        ];
        let result = validate_all(&rules, "Password123", "password", true);
        result.unwrap_or_else(|e| panic!("expected strong password to pass: {e}"));
    }

    #[test]
    fn test_enum_or_pattern_any() {
        let rules = vec![
            ValidationRule::Enum {
                values: vec!["admin".to_string(), "user".to_string()],
            },
            ValidationRule::Pattern {
                pattern: "^guest_[0-9]+$".to_string(),
                message: None,
            },
        ];
        let result = validate_any(&rules, "guest_123", "role", true);
        result.unwrap_or_else(|e| panic!("expected enum-or-pattern any to pass: {e}"));
    }

    #[test]
    fn test_not_numeric_for_string_field() {
        let rule = ValidationRule::Pattern {
            pattern: "^[0-9]+$".to_string(),
            message: None,
        };
        let result = validate_not(&rule, "abc123", "code", true);
        // Should pass because the regex doesn't match the whole string
        result.unwrap_or_else(|e| panic!("expected not-numeric to pass for mixed string: {e}"));
    }

    #[test]
    fn test_composite_error_display() {
        let error = CompositeError {
            operator: CompositeOperator::All,
            errors:   vec!["error1".to_string(), "error2".to_string()],
            field:    "field".to_string(),
        };
        let display_str = error.to_string();
        assert!(display_str.contains("All validators must pass"));
        assert!(display_str.contains("error1"));
        assert!(display_str.contains("error2"));
    }

    #[test]
    fn test_multiple_validators_with_required() {
        let rules = vec![ValidationRule::Required];
        let result = validate_all(&rules, "test", "field", true);
        result.unwrap_or_else(|e| panic!("expected required validator to pass: {e}"));
    }

    #[test]
    fn test_empty_rules_all() {
        let rules: Vec<ValidationRule> = vec![];
        let result = validate_all(&rules, "test", "field", true);
        result.unwrap_or_else(|e| panic!("expected empty all-rules to pass vacuously: {e}"));
    }

    #[test]
    fn test_empty_rules_any() {
        let rules: Vec<ValidationRule> = vec![];
        let result = validate_any(&rules, "test", "field", true);
        // Any with no rules vacuously fails (nothing passed)
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("At least one validator must pass")),
            "expected Validation error for empty any-rules, got: {result:?}"
        );
    }

    #[test]
    fn test_length_min_max() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: Some(10),
        };
        let result = validate_single_rule(&rule, "hello", "password", true);
        result.unwrap_or_else(|e| panic!("expected length check to pass for 'hello': {e}"));

        let result = validate_single_rule(&rule, "hi", "password", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("at least")),
            "expected Validation error for too-short value, got: {result:?}"
        );

        let result = validate_single_rule(&rule, "this_is_too_long", "password", true);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("at most")),
            "expected Validation error for too-long value, got: {result:?}"
        );
    }
}

mod cross_field_tests {
    use serde_json::json;

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_operator_parsing() {
        assert_eq!(ComparisonOperator::from_str("<"), Some(ComparisonOperator::LessThan));
        assert_eq!(ComparisonOperator::from_str("lt"), Some(ComparisonOperator::LessThan));
        assert_eq!(ComparisonOperator::from_str("<="), Some(ComparisonOperator::LessEqual));
        assert_eq!(ComparisonOperator::from_str("lte"), Some(ComparisonOperator::LessEqual));
        assert_eq!(ComparisonOperator::from_str(">"), Some(ComparisonOperator::GreaterThan));
        assert_eq!(ComparisonOperator::from_str("gt"), Some(ComparisonOperator::GreaterThan));
        assert_eq!(ComparisonOperator::from_str(">="), Some(ComparisonOperator::GreaterEqual));
        assert_eq!(ComparisonOperator::from_str("gte"), Some(ComparisonOperator::GreaterEqual));
        assert_eq!(ComparisonOperator::from_str("=="), Some(ComparisonOperator::Equal));
        assert_eq!(ComparisonOperator::from_str("eq"), Some(ComparisonOperator::Equal));
        assert_eq!(ComparisonOperator::from_str("!="), Some(ComparisonOperator::NotEqual));
        assert_eq!(ComparisonOperator::from_str("neq"), Some(ComparisonOperator::NotEqual));
        assert_eq!(ComparisonOperator::from_str("invalid"), None);
    }

    #[test]
    fn test_numeric_less_than() {
        let input = json!({
            "start": 10,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected 10 < 20 to pass: {e}"));
    }

    #[test]
    fn test_numeric_less_than_fails() {
        let input = json!({
            "start": 30,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must be") && message.contains("less than")),
            "expected Validation error for 30 < 20, got: {result:?}"
        );
    }

    #[test]
    fn test_numeric_equal() {
        let input = json!({
            "a": 42,
            "b": 42
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::Equal, "b", None);
        result.unwrap_or_else(|e| panic!("expected 42 == 42 to pass: {e}"));
    }

    #[test]
    fn test_numeric_not_equal() {
        let input = json!({
            "a": 10,
            "b": 20
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::NotEqual, "b", None);
        result.unwrap_or_else(|e| panic!("expected 10 != 20 to pass: {e}"));
    }

    #[test]
    fn test_numeric_greater_than_or_equal() {
        let input = json!({
            "min": 10,
            "max": 10
        });
        let result = validate_cross_field_comparison(
            &input,
            "max",
            ComparisonOperator::GreaterEqual,
            "min",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected 10 >= 10 to pass: {e}"));
    }

    #[test]
    fn test_string_comparison() {
        let input = json!({
            "start_name": "alice",
            "end_name": "zoe"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_name",
            ComparisonOperator::LessThan,
            "end_name",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected 'alice' < 'zoe' to pass: {e}"));
    }

    #[test]
    fn test_string_comparison_fails() {
        let input = json!({
            "start_name": "zoe",
            "end_name": "alice"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_name",
            ComparisonOperator::LessThan,
            "end_name",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must be") && message.contains("less than")),
            "expected Validation error for 'zoe' < 'alice', got: {result:?}"
        );
    }

    #[test]
    fn test_date_string_comparison() {
        let input = json!({
            "start_date": "2024-01-01",
            "end_date": "2024-12-31"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start_date",
            ComparisonOperator::LessThan,
            "end_date",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected date string comparison to pass: {e}"));
    }

    #[test]
    fn test_float_comparison() {
        let input = json!({
            "price": 19.99,
            "budget": 25.50
        });
        let result = validate_cross_field_comparison(
            &input,
            "price",
            ComparisonOperator::LessThan,
            "budget",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected 19.99 < 25.50 to pass: {e}"));
    }

    #[test]
    fn test_missing_left_field() {
        let input = json!({
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("not found")),
            "expected Validation error for missing left field, got: {result:?}"
        );
    }

    #[test]
    fn test_missing_right_field() {
        let input = json!({
            "start": 10
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("not found")),
            "expected Validation error for missing right field, got: {result:?}"
        );
    }

    #[test]
    fn test_null_fields_skipped() {
        let input = json!({
            "start": null,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected null field to be skipped: {e}"));
    }

    #[test]
    fn test_both_null_fields_skipped() {
        let input = json!({
            "start": null,
            "end": null
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        result.unwrap_or_else(|e| panic!("expected both null fields to be skipped: {e}"));
    }

    #[test]
    fn test_type_mismatch_error() {
        let input = json!({
            "start": 10,
            "end": "twenty"
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Cannot compare")),
            "expected Validation error for type mismatch, got: {result:?}"
        );
    }

    #[test]
    fn test_error_includes_context_path() {
        let input = json!({
            "start": 30,
            "end": 20
        });
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            Some("dateRange"),
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref path, .. }) if *path == Some("dateRange".to_string())),
            "expected Validation error with path 'dateRange', got: {result:?}"
        );
    }

    #[test]
    fn test_error_message_includes_values() {
        let input = json!({
            "price": 100,
            "max_price": 50
        });
        let result = validate_cross_field_comparison(
            &input,
            "price",
            ComparisonOperator::LessThan,
            "max_price",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("price") && message.contains("max_price") && message.contains("100") && message.contains("50")),
            "expected Validation error with field names and values, got: {result:?}"
        );
    }

    #[test]
    fn test_all_operators() {
        let test_cases = vec![
            (10, 20, ComparisonOperator::LessThan, true),
            (10, 10, ComparisonOperator::LessEqual, true),
            (20, 10, ComparisonOperator::GreaterThan, true),
            (10, 10, ComparisonOperator::GreaterEqual, true),
            (42, 42, ComparisonOperator::Equal, true),
            (10, 20, ComparisonOperator::NotEqual, true),
            (20, 10, ComparisonOperator::LessThan, false),
            (10, 20, ComparisonOperator::GreaterThan, false),
        ];

        for (left, right, op, should_pass) in test_cases {
            let input = json!({ "a": left, "b": right });
            let result = validate_cross_field_comparison(&input, "a", op, "b", None);
            assert_eq!(
                result.is_ok(),
                should_pass,
                "Failed for {} {} {}",
                left,
                op.symbol(),
                right
            );
        }
    }

    #[test]
    fn test_non_object_input() {
        let input = json!([1, 2, 3]);
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("not an object")),
            "expected Validation error for non-object input, got: {result:?}"
        );
    }

    #[test]
    fn test_empty_object() {
        let input = json!({});
        let result = validate_cross_field_comparison(
            &input,
            "start",
            ComparisonOperator::LessThan,
            "end",
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("not found")),
            "expected Validation error for empty object, got: {result:?}"
        );
    }

    #[test]
    fn test_zero_comparison() {
        let input = json!({
            "a": 0,
            "b": 0
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::Equal, "b", None);
        result.unwrap_or_else(|e| panic!("expected 0 == 0 to pass: {e}"));
    }

    #[test]
    fn test_negative_number_comparison() {
        let input = json!({
            "a": -10,
            "b": 5
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        result.unwrap_or_else(|e| panic!("expected -10 < 5 to pass: {e}"));
    }

    #[test]
    fn test_empty_string_comparison() {
        let input = json!({
            "a": "",
            "b": "text"
        });
        let result =
            validate_cross_field_comparison(&input, "a", ComparisonOperator::LessThan, "b", None);
        result.unwrap_or_else(|e| panic!("expected '' < 'text' to pass: {e}"));
    }
}

mod custom_scalar_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::{Value, json};

    use super::super::CustomScalar;
    use crate::error::{FraiseQLError, Result};

    /// Minimal email scalar for testing the trait.
    #[derive(Debug)]
    struct EmailScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for EmailScalar {
        fn name(&self) -> &str {
            "Email"
        }

        fn serialize(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_value(&self, value: &Value) -> Result<Value> {
            let s = value
                .as_str()
                .ok_or_else(|| FraiseQLError::validation("Email must be a string"))?;
            if !s.contains('@') {
                return Err(FraiseQLError::validation(format!("invalid email: {s}")));
            }
            Ok(Value::String(s.to_string()))
        }

        fn parse_literal(&self, ast: &Value) -> Result<Value> {
            self.parse_value(ast)
        }
    }

    #[test]
    fn test_name() {
        let scalar = EmailScalar;
        assert_eq!(scalar.name(), "Email");
    }

    #[test]
    fn test_serialize_returns_value_unchanged() {
        let scalar = EmailScalar;
        let v = json!("user@example.com");
        assert_eq!(scalar.serialize(&v).unwrap(), v);
    }

    #[test]
    fn test_parse_value_valid_email() {
        let scalar = EmailScalar;
        let v = json!("user@example.com");
        assert_eq!(scalar.parse_value(&v).unwrap(), v);
    }

    #[test]
    fn test_parse_value_invalid_email_no_at() {
        let scalar = EmailScalar;
        let v = json!("notanemail");
        assert!(
            matches!(scalar.parse_value(&v), Err(crate::error::FraiseQLError::Validation { .. })),
            "email without '@' should fail with Validation error, got: {:?}",
            scalar.parse_value(&v)
        );
    }

    #[test]
    fn test_parse_value_non_string_input() {
        let scalar = EmailScalar;
        let v = json!(42);
        let err = scalar.parse_value(&v).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("string") || msg.contains("Email"), "unexpected: {msg}");
    }

    #[test]
    fn test_parse_literal_delegates_to_parse_value() {
        let scalar = EmailScalar;
        let v = json!("lit@example.com");
        assert_eq!(scalar.parse_literal(&v).unwrap(), v);
    }

    #[test]
    fn test_custom_scalar_result_type_alias_is_result_value() {
        let _result: super::CustomScalarResult = Ok(json!("ok"));
    }
}

mod custom_scalar_registry_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[derive(Debug)]
    struct TestScalar;

    impl CustomScalar for TestScalar {
        #[allow(clippy::unnecessary_literal_bound)] // Reason: trait requires &str return type
        fn name(&self) -> &str {
            "Test"
        }

        fn serialize(&self, value: &serde_json::Value) -> crate::error::Result<serde_json::Value> {
            Ok(value.clone())
        }

        fn parse_value(
            &self,
            value: &serde_json::Value,
        ) -> crate::error::Result<serde_json::Value> {
            Ok(value.clone())
        }

        fn parse_literal(
            &self,
            ast: &serde_json::Value,
        ) -> crate::error::Result<serde_json::Value> {
            Ok(ast.clone())
        }
    }

    #[test]
    fn test_register_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry
            .register(scalar)
            .unwrap_or_else(|e| panic!("first registration should succeed: {e}"));
        assert!(registry.has_scalar("Test").unwrap());
    }

    #[test]
    fn test_prevent_duplicate_registration() {
        let registry = CustomScalarRegistry::new();
        let scalar1: Arc<dyn CustomScalar> = Arc::new(TestScalar);
        let scalar2: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry
            .register(scalar1)
            .unwrap_or_else(|e| panic!("first registration should succeed: {e}"));
        assert!(
            matches!(
                registry.register(scalar2),
                Err(crate::error::FraiseQLError::Validation { .. })
            ),
            "duplicate registration should return Validation error"
        );
    }

    #[test]
    fn test_get_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar.clone()).unwrap();
        assert!(registry.get_scalar("Test").unwrap().is_some());
        assert!(registry.get_scalar("NotFound").unwrap().is_none());
    }

    #[test]
    fn test_unregister_scalar() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar).unwrap();
        assert!(registry.has_scalar("Test").unwrap());

        registry.unregister("Test").unwrap();
        assert!(!registry.has_scalar("Test").unwrap());
    }

    #[test]
    fn test_clear_scalars() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        registry.register(scalar).unwrap();
        registry.clear().unwrap();

        assert!(registry.get_scalar_names().unwrap().is_empty());
    }

    #[test]
    fn test_has_scalar_returns_false_for_missing() {
        let registry = CustomScalarRegistry::new();
        assert!(
            !registry.has_scalar("NonExistent").unwrap(),
            "has_scalar should return false for unregistered name"
        );
    }

    #[test]
    fn test_get_scalar_names_returns_registered_names() {
        let registry = CustomScalarRegistry::new();
        let scalar: Arc<dyn CustomScalar> = Arc::new(TestScalar);

        assert!(
            registry.get_scalar_names().unwrap().is_empty(),
            "fresh registry should have no names"
        );

        registry.register(scalar).unwrap();
        let names = registry.get_scalar_names().unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "Test");
    }
}

mod date_validators_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use chrono::Datelike;

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;
    use crate::validation::date_validators::{
        compare_dates, days_between, get_days_in_month, is_leap_year, parse_date,
    };

    // ── Helpers for time-independent tests ──────────────────────────────────

    /// Returns "YYYY-MM-DD" for `years` years before today.
    fn years_ago(years: u32) -> String {
        let today = chrono::Utc::now().date_naive();
        let y = today.year() - i32::try_from(years).unwrap_or(0);
        format!("{y}-{:02}-{:02}", today.month(), today.day())
    }

    /// Returns "YYYY-MM-DD" for today.
    fn today_str() -> String {
        chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string()
    }

    // ── parse_date ───────────────────────────────────────────────────────────

    #[test]
    fn test_parse_date_valid() {
        let result = parse_date("2026-02-08");
        let parsed = result.unwrap_or_else(|e| panic!("valid date should parse: {e}"));
        assert_eq!(parsed, (2026, 2, 8));
    }

    #[test]
    fn test_parse_date_invalid_format() {
        assert!(
            matches!(parse_date("2026/02/08"), Err(FraiseQLError::Validation { .. })),
            "slash-separated date should fail parsing"
        );
        assert!(
            matches!(parse_date("02-08-2026"), Err(FraiseQLError::Validation { .. })),
            "MM-DD-YYYY format should fail parsing"
        );
    }

    #[test]
    fn test_parse_date_invalid_month() {
        assert!(
            matches!(parse_date("2026-13-01"), Err(FraiseQLError::Validation { .. })),
            "month 13 should fail validation"
        );
        assert!(
            matches!(parse_date("2026-00-01"), Err(FraiseQLError::Validation { .. })),
            "month 0 should fail validation"
        );
    }

    #[test]
    fn test_parse_date_invalid_day() {
        assert!(
            matches!(parse_date("2026-02-30"), Err(FraiseQLError::Validation { .. })),
            "Feb 30 should fail validation"
        );
        assert!(
            matches!(parse_date("2026-04-31"), Err(FraiseQLError::Validation { .. })),
            "Apr 31 should fail validation"
        );
    }

    // ── leap year / days in month ────────────────────────────────────────────

    #[test]
    fn test_leap_year_detection() {
        assert!(is_leap_year(2024));
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2025));
    }

    #[test]
    fn test_days_in_month() {
        assert_eq!(get_days_in_month(1, 2026), 31);
        assert_eq!(get_days_in_month(2, 2024), 29); // Leap year
        assert_eq!(get_days_in_month(2, 2026), 28); // Non-leap year
        assert_eq!(get_days_in_month(4, 2026), 30);
    }

    #[test]
    fn test_february_leap_year_edge_case() {
        parse_date("2024-02-29")
            .unwrap_or_else(|e| panic!("Feb 29 on leap year should parse: {e}"));
        assert!(
            matches!(parse_date("2024-02-30"), Err(FraiseQLError::Validation { .. })),
            "Feb 30 on leap year should fail"
        );
    }

    #[test]
    fn test_february_non_leap_year_edge_case() {
        parse_date("2025-02-28")
            .unwrap_or_else(|e| panic!("Feb 28 on non-leap year should parse: {e}"));
        assert!(
            matches!(parse_date("2025-02-29"), Err(FraiseQLError::Validation { .. })),
            "Feb 29 on non-leap year should fail"
        );
    }

    #[test]
    fn test_year_2000_leap_year() {
        assert!(is_leap_year(2000));
        parse_date("2000-02-29").unwrap_or_else(|e| panic!("Feb 29 in 2000 should parse: {e}"));
    }

    #[test]
    fn test_year_1900_not_leap_year() {
        assert!(!is_leap_year(1900));
        assert!(
            matches!(parse_date("1900-02-29"), Err(FraiseQLError::Validation { .. })),
            "Feb 29 in 1900 (not leap) should fail"
        );
    }

    // ── compare_dates / days_between ────────────────────────────────────────

    #[test]
    fn test_compare_dates() {
        assert!(compare_dates((2026, 2, 8), (2026, 2, 7)) > 0);
        assert!(compare_dates((2026, 2, 7), (2026, 2, 8)) < 0);
        assert_eq!(compare_dates((2026, 2, 8), (2026, 2, 8)), 0);
        assert!(compare_dates((2026, 3, 1), (2026, 2, 28)) > 0);
        assert!(compare_dates((2027, 1, 1), (2026, 12, 31)) > 0);
    }

    #[test]
    fn test_days_between_same_date() {
        assert_eq!(days_between((2026, 2, 8), (2026, 2, 8)), 0);
    }

    #[test]
    fn test_days_between_year_difference() {
        let diff = days_between((2027, 2, 8), (2026, 2, 8));
        assert!(diff > 0);
    }

    // ── validate_min_date / validate_max_date / validate_date_range ─────────

    #[test]
    fn test_min_date_passes() {
        validate_min_date("2026-02-08", "2026-02-01")
            .unwrap_or_else(|e| panic!("date after min should pass: {e}"));
        validate_min_date("2026-02-08", "2026-02-08")
            .unwrap_or_else(|e| panic!("date equal to min should pass: {e}"));
    }

    #[test]
    fn test_min_date_fails() {
        let result = validate_min_date("2026-02-08", "2026-02-09");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date before min should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_max_date_passes() {
        validate_max_date("2026-02-08", "2026-02-15")
            .unwrap_or_else(|e| panic!("date before max should pass: {e}"));
        validate_max_date("2026-02-08", "2026-02-08")
            .unwrap_or_else(|e| panic!("date equal to max should pass: {e}"));
    }

    #[test]
    fn test_max_date_fails() {
        let result = validate_max_date("2026-02-08", "2026-02-07");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date after max should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_date_range_passes() {
        validate_date_range("2026-02-08", "2026-01-01", "2026-12-31")
            .unwrap_or_else(|e| panic!("date within range should pass: {e}"));
    }

    #[test]
    fn test_date_range_fails_below_min() {
        let result = validate_date_range("2025-12-31", "2026-01-01", "2026-12-31");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date below range should fail, got: {result:?}"
        );
    }

    #[test]
    fn test_date_range_fails_above_max() {
        let result = validate_date_range("2027-01-01", "2026-01-01", "2026-12-31");
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "date above range should fail, got: {result:?}"
        );
    }

    // ── validate_min_age / validate_max_age (time-independent) ──────────────

    #[test]
    fn test_min_age_passes_clearly_old_enough() {
        // Born 50 years ago: definitely passes min_age = 18
        validate_min_age(&years_ago(50), 18)
            .unwrap_or_else(|e| panic!("50yo should pass min_age=18: {e}"));
    }

    #[test]
    fn test_min_age_fails_too_young() {
        // Born 5 years ago: cannot pass min_age = 18
        let result = validate_min_age(&years_ago(5), 18);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "5yo should fail min_age=18, got: {result:?}"
        );
    }

    #[test]
    fn test_min_age_birthday_today_exactly_18() {
        // Born exactly 18 years ago today → passes min_age = 18
        validate_min_age(&years_ago(18), 18)
            .unwrap_or_else(|e| panic!("exactly 18yo should pass min_age=18: {e}"));
    }

    #[test]
    fn test_max_age_passes_clearly_young_enough() {
        // Born 5 years ago: definitely passes max_age = 18
        validate_max_age(&years_ago(5), 18)
            .unwrap_or_else(|e| panic!("5yo should pass max_age=18: {e}"));
    }

    #[test]
    fn test_max_age_fails_too_old() {
        // Born 100 years ago: cannot pass max_age = 90
        let result = validate_max_age(&years_ago(100), 90);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "100yo should fail max_age=90, got: {result:?}"
        );
    }

    // ── validate_max_days_in_future / validate_max_days_in_past ─────────────

    #[test]
    fn test_max_days_in_future_today_passes() {
        // Today is 0 days in the future — always passes
        validate_max_days_in_future(&today_str(), 0)
            .unwrap_or_else(|e| panic!("today should pass max_days_in_future=0: {e}"));
    }

    #[test]
    fn test_max_days_in_future_past_date_passes() {
        // A date in 2000 is never in the future
        validate_max_days_in_future("2000-01-01", 0)
            .unwrap_or_else(|e| panic!("past date should pass max_days_in_future: {e}"));
    }

    #[test]
    fn test_max_days_in_future_far_future_fails() {
        // Year 9999 is always more than 30 days in the future
        let result = validate_max_days_in_future("9999-12-31", 30);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "year 9999 should fail max_days_in_future=30, got: {result:?}"
        );
    }

    #[test]
    fn test_max_days_in_past_today_passes() {
        // Today is 0 days in the past — always passes
        validate_max_days_in_past(&today_str(), 0)
            .unwrap_or_else(|e| panic!("today should pass max_days_in_past=0: {e}"));
    }

    #[test]
    fn test_max_days_in_past_far_past_fails() {
        // A date 50 years ago is more than 30 days in the past
        let result = validate_max_days_in_past(&years_ago(50), 30);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "50 years ago should fail max_days_in_past=30, got: {result:?}"
        );
    }
}

mod elo_expressions_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // Helper to create test context
    fn create_test_user() -> Value {
        json!({
            "email": "user@example.com",
            "age": 25,
            "verified": true,
            "birthDate": "2000-01-15",
            "role": "user"
        })
    }

    // ========== COMPARISON OPERATORS ==========

    #[test]
    fn test_simple_greater_than() {
        let eval = EloExpressionEvaluator::new("age > 18".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_simple_greater_than_fails() {
        let eval = EloExpressionEvaluator::new("age > 30".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_greater_or_equal() {
        let eval = EloExpressionEvaluator::new("age >= 25".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_less_than() {
        let eval = EloExpressionEvaluator::new("age < 30".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_less_or_equal() {
        let eval = EloExpressionEvaluator::new("age <= 25".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_equality() {
        let eval = EloExpressionEvaluator::new("role == \"user\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_inequality() {
        let eval = EloExpressionEvaluator::new("role != \"admin\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    // ========== LOGICAL OPERATORS ==========

    #[test]
    fn test_and_both_true() {
        let eval = EloExpressionEvaluator::new("age > 18 && verified == true".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_and_first_false() {
        let eval = EloExpressionEvaluator::new("age < 18 && verified == true".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_and_second_false() {
        let eval = EloExpressionEvaluator::new("age > 18 && verified == false".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_or_both_true() {
        let eval = EloExpressionEvaluator::new("age > 18 || role == \"admin\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_or_first_true() {
        let eval = EloExpressionEvaluator::new("age > 18 || role == \"guest\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_or_second_true() {
        let eval = EloExpressionEvaluator::new("age > 30 || role == \"user\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_or_both_false() {
        let eval = EloExpressionEvaluator::new("age > 30 || role == \"admin\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_negation() {
        let eval = EloExpressionEvaluator::new("!(role == \"admin\")".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_negation_of_true() {
        let eval = EloExpressionEvaluator::new("!(verified == true)".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    // ========== FUNCTION CALLS ==========

    #[test]
    fn test_matches_function() {
        let eval = EloExpressionEvaluator::new(
            "matches(email, \"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\")".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_matches_function_fails() {
        let eval = EloExpressionEvaluator::new("matches(email, \"^[0-9]+$\")".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_contains_function() {
        let eval = EloExpressionEvaluator::new("contains(email, \"example.com\")".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_contains_function_fails() {
        let eval = EloExpressionEvaluator::new("contains(email, \"gmail.com\")".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid);
    }

    // ========== COMPLEX EXPRESSIONS ==========

    #[test]
    fn test_complex_and_or() {
        let eval = EloExpressionEvaluator::new(
            "age > 18 && (role == \"user\" || role == \"admin\")".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_complex_with_matches() {
        let eval = EloExpressionEvaluator::new(
            "age >= 18 && matches(email, \"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\")"
                .to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_complex_with_negation() {
        let eval = EloExpressionEvaluator::new(
            "!(role == \"banned\") && age > 18 && verified == true".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    // ========== FIELD ACCESS ==========

    #[test]
    fn test_field_access_string() {
        let eval = EloExpressionEvaluator::new("email == \"user@example.com\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_field_access_number() {
        let eval = EloExpressionEvaluator::new("age == 25".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_field_access_boolean() {
        let eval = EloExpressionEvaluator::new("verified == true".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    // ========== ERROR CASES ==========

    #[test]
    fn test_unknown_function_error() {
        let eval = EloExpressionEvaluator::new("unknown_func(email)".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "unknown function should return Validation error, got: {result:?}"
        );
    }

    #[test]
    fn test_invalid_regex_error() {
        let eval = EloExpressionEvaluator::new("matches(email, \"[\")".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "invalid regex in matches() should return Validation error, got: {result:?}"
        );
    }

    #[test]
    fn test_wrong_argument_count_error() {
        let eval = EloExpressionEvaluator::new("matches(email)".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "wrong argument count for matches() should return Validation error, got: {result:?}"
        );
    }

    // ========== EDGE CASES ==========

    #[test]
    fn test_whitespace_handling() {
        let eval = EloExpressionEvaluator::new("  age   >   18  ".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_multiple_operators_precedence() {
        let eval =
            EloExpressionEvaluator::new("age > 20 && age < 30 && role == \"user\"".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_string_literal_quotes() {
        let eval = EloExpressionEvaluator::new("role == 'user'".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_number_literals() {
        let eval = EloExpressionEvaluator::new("age > 20".to_string());
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    // ========== REAL-WORLD PATTERNS ==========

    #[test]
    fn test_email_validation_pattern() {
        let eval = EloExpressionEvaluator::new(
            "matches(email, \"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\\\.[a-zA-Z]{2,}$\")".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_user_creation_rules() {
        let eval = EloExpressionEvaluator::new(
            "age >= 18 && verified == true && role != \"banned\"".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(result.valid);
    }

    #[test]
    fn test_admin_access_rules() {
        let eval = EloExpressionEvaluator::new(
            "(role == \"admin\" || role == \"moderator\") && verified == true".to_string(),
        );
        let user = create_test_user();
        let result = eval.evaluate(&user).unwrap();
        assert!(!result.valid); // User role is "user", not admin
    }
}

mod elo_rust_integration_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_config_default() {
        let config = RustValidatorRegistryConfig::default();
        assert!(config.enabled);
        assert!(config.cache_validators);
        assert_eq!(config.max_cache_size, 1000);
    }

    #[test]
    fn test_validator_creation() {
        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };
        assert_eq!(validator.name, "test");
    }

    #[test]
    fn test_registry_register_get() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        let retrieved = registry.get("test");
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_registry_exists() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "x > 0".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert!(registry.exists("test"));
        assert!(!registry.exists("nonexistent"));
    }

    #[test]
    fn test_registry_count() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        for i in 0..5 {
            let validator = EloRustValidator {
                name:           format!("v{}", i),
                elo_expression: "true".to_string(),
                generated_code: None,
            };
            registry.register(validator);
        }

        assert_eq!(registry.count(), 5);
    }

    #[test]
    fn test_registry_remove() {
        let config = RustValidatorRegistryConfig::default();
        let registry = RustValidatorRegistry::new(config);

        let validator = EloRustValidator {
            name:           "test".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        };

        registry.register(validator);
        assert_eq!(registry.count(), 1);

        registry.remove("test");
        assert_eq!(registry.count(), 0);
    }

    #[test]
    fn test_registry_is_enabled() {
        let config = RustValidatorRegistryConfig {
            enabled: true,
            ..Default::default()
        };
        let registry = RustValidatorRegistry::new(config);
        assert!(registry.is_enabled());

        let config2 = RustValidatorRegistryConfig {
            enabled: false,
            ..Default::default()
        };
        let registry2 = RustValidatorRegistry::new(config2);
        assert!(!registry2.is_enabled());
    }

    #[test]
    fn test_registry_is_empty() {
        let registry = RustValidatorRegistry::new(RustValidatorRegistryConfig::default());
        assert!(registry.is_empty());

        registry.register(EloRustValidator {
            name:           "v".to_string(),
            elo_expression: "true".to_string(),
            generated_code: None,
        });
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_registry_list_all() {
        let registry = RustValidatorRegistry::new(RustValidatorRegistryConfig::default());
        assert!(registry.list_all().is_empty());

        for i in 0..3 {
            registry.register(EloRustValidator {
                name:           format!("v{i}"),
                elo_expression: "true".to_string(),
                generated_code: None,
            });
        }

        let all = registry.list_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_registry_clear() {
        let registry = RustValidatorRegistry::new(RustValidatorRegistryConfig::default());

        for i in 0..3 {
            registry.register(EloRustValidator {
                name:           format!("v{i}"),
                elo_expression: "true".to_string(),
                generated_code: None,
            });
        }

        assert_eq!(registry.count(), 3);
        registry.clear();
        assert_eq!(registry.count(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_config() {
        let config = RustValidatorRegistryConfig {
            enabled:          true,
            cache_validators: false,
            max_cache_size:   500,
        };
        let registry = RustValidatorRegistry::new(config);
        let cfg = registry.config();
        assert!(cfg.enabled);
        assert!(!cfg.cache_validators);
        assert_eq!(cfg.max_cache_size, 500);
    }
}

mod error_responses_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_create_empty_response() {
        let response = GraphQLValidationResponse::new();
        assert!(!response.has_errors());
        assert_eq!(response.error_count, 0);
    }

    #[test]
    fn test_add_single_error() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("email", "pattern", "Invalid email format");
        response.add_field_error(field_error, None);

        assert!(response.has_errors());
        assert_eq!(response.error_count, 1);
        assert_eq!(response.errors[0].extensions.as_ref().unwrap().rule_type, "pattern");
    }

    #[test]
    fn test_add_multiple_errors() {
        let mut response = GraphQLValidationResponse::new();
        let errors = vec![
            ValidationFieldError::new("email", "pattern", "Invalid email"),
            ValidationFieldError::new("phone", "pattern", "Invalid phone"),
        ];
        response.add_errors(errors);

        assert_eq!(response.error_count, 2);
    }

    #[test]
    fn test_path_parsing() {
        let path = GraphQLValidationResponse::parse_path("user.email");
        assert_eq!(path, vec!["user".to_string(), "email".to_string()]);

        let path = GraphQLValidationResponse::parse_path("address.zipcode");
        assert_eq!(path, vec!["address".to_string(), "zipcode".to_string()]);
    }

    #[test]
    fn test_json_serialization() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("field1", "rule1", "Error message");
        response.add_field_error(field_error, Some(serde_json::json!({"detail": "extra"})));

        let json = response.to_graphql_errors();
        assert!(json["error_count"].is_number());
        assert!(json["errors"].is_array());
    }

    #[test]
    fn test_from_fraiseql_error() {
        let error = FraiseQLError::Validation {
            message: "Validation failed".to_string(),
            path:    Some("user.email".to_string()),
        };

        let response = GraphQLValidationResponse::from_error(&error);
        assert!(response.is_some());
        let response = response.unwrap();
        assert_eq!(response.error_count, 1);
    }

    #[test]
    fn test_context_inclusion() {
        let mut response = GraphQLValidationResponse::new();
        let field_error = ValidationFieldError::new("password", "length", "Too short");
        let context = serde_json::json!({"minimum_length": 12, "provided_length": 8});
        response.add_field_error(field_error, Some(context));

        assert!(response.errors[0].extensions.as_ref().unwrap().context.is_some());
    }
}

mod id_policy_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // ==================== UUID Format Tests ====================

    #[test]
    fn test_validate_valid_uuid() {
        // Standard UUID format
        let result = validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::UUID);
        result.unwrap_or_else(|e| panic!("valid UUID should pass: {e}"));
    }

    #[test]
    fn test_validate_valid_uuid_uppercase() {
        // UUIDs are case-insensitive
        let result = validate_id("550E8400-E29B-41D4-A716-446655440000", IDPolicy::UUID);
        result.unwrap_or_else(|e| panic!("uppercase UUID should pass: {e}"));
    }

    #[test]
    fn test_validate_valid_uuid_mixed_case() {
        let result = validate_id("550e8400-E29b-41d4-A716-446655440000", IDPolicy::UUID);
        result.unwrap_or_else(|e| panic!("mixed-case UUID should pass: {e}"));
    }

    #[test]
    fn test_validate_nil_uuid() {
        // Nil UUID (all zeros) is valid
        let result = validate_id("00000000-0000-0000-0000-000000000000", IDPolicy::UUID);
        result.unwrap_or_else(|e| panic!("nil UUID should pass: {e}"));
    }

    #[test]
    fn test_validate_max_uuid() {
        // Max UUID (all Fs) is valid
        let result = validate_id("ffffffff-ffff-ffff-ffff-ffffffffffff", IDPolicy::UUID);
        result.unwrap_or_else(|e| panic!("max UUID should pass: {e}"));
    }

    #[test]
    fn test_validate_uuid_wrong_length() {
        let result = validate_id("550e8400-e29b-41d4-a716", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "short UUID string should fail with Validation error, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert_eq!(err.policy, IDPolicy::UUID);
        assert!(err.message.contains("36 characters"));
    }

    #[test]
    fn test_validate_uuid_extra_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-446655440000x", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "extra chars should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_uuid_missing_hyphens() {
        // 36 chars without hyphens - all hex digits, same length as UUID but no separators
        let result = validate_id("550e8400e29b41d4a716446655440000", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "UUID without hyphens should fail, got: {result:?}"
        );
        let err = result.unwrap_err();
        // Fails length check since 32 chars != 36
        assert!(err.message.contains("36 characters"));
    }

    #[test]
    fn test_validate_uuid_wrong_segment_lengths() {
        // First segment too short (7 chars instead of 8)
        // Need 36 chars total, so pad the last segment: 550e840-e29b-41d4-a716-4466554400001
        let result = validate_id("550e840-e29b-41d4-a716-4466554400001", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "UUID with wrong segment lengths should fail, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("segment"));
    }

    #[test]
    fn test_validate_uuid_non_hex_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-44665544000g", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "UUID with non-hex chars should fail, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("non-hexadecimal"));
    }

    #[test]
    fn test_validate_uuid_special_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-4466554400@0", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "special chars should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_uuid_empty_string() {
        let result = validate_id("", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "empty string should fail UUID validation, got: {result:?}"
        );
    }

    // ==================== OPAQUE Policy Tests ====================

    #[test]
    fn test_opaque_accepts_any_string() {
        validate_id("not-a-uuid", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
        validate_id("anything", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
        validate_id("12345", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
        validate_id("special@chars!#$%", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
    }

    #[test]
    fn test_opaque_accepts_empty_string() {
        validate_id("", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept empty string: {e}"));
    }

    #[test]
    fn test_opaque_accepts_uuid() {
        validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept UUID string: {e}"));
    }

    // ==================== Multiple IDs Tests ====================

    #[test]
    fn test_validate_multiple_valid_uuids() {
        let ids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ];
        validate_ids(&ids, IDPolicy::UUID)
            .unwrap_or_else(|e| panic!("all valid UUIDs should pass: {e}"));
    }

    #[test]
    fn test_validate_multiple_fails_on_first_invalid() {
        let ids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "invalid-id",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ];
        let result = validate_ids(&ids, IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "batch with invalid ID should fail, got: {result:?}"
        );
        assert_eq!(result.unwrap_err().value, "invalid-id");
    }

    #[test]
    fn test_validate_multiple_opaque_all_pass() {
        let ids = vec!["anything", "goes", "here", "12345"];
        validate_ids(&ids, IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept all strings: {e}"));
    }

    // ==================== Policy Behavior Tests ====================

    #[test]
    fn test_policy_enforces_uuid() {
        assert!(IDPolicy::UUID.enforces_uuid());
        assert!(!IDPolicy::OPAQUE.enforces_uuid());
    }

    #[test]
    fn test_policy_as_str() {
        assert_eq!(IDPolicy::UUID.as_str(), "uuid");
        assert_eq!(IDPolicy::OPAQUE.as_str(), "opaque");
    }

    #[test]
    fn test_policy_default() {
        assert_eq!(IDPolicy::default(), IDPolicy::UUID);
    }

    #[test]
    fn test_policy_display() {
        assert_eq!(format!("{}", IDPolicy::UUID), "uuid");
        assert_eq!(format!("{}", IDPolicy::OPAQUE), "opaque");
    }

    // ==================== Security Scenarios ====================

    #[test]
    fn test_security_prevent_sql_injection_via_uuid() {
        // UUID validation prevents malicious IDs with SQL injection
        let result = validate_id("'; DROP TABLE users; --", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "SQL injection string should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_security_prevent_path_traversal_via_uuid() {
        let result = validate_id("../../etc/passwd", IDPolicy::UUID);
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "path traversal string should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_security_opaque_policy_accepts_any_format() {
        // OPAQUE policy explicitly accepts any string
        // Input validation and authorization must be done elsewhere
        validate_id("'; DROP TABLE users; --", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept SQL injection string: {e}"));
        validate_id("../../etc/passwd", IDPolicy::OPAQUE)
            .unwrap_or_else(|e| panic!("opaque should accept path traversal string: {e}"));
    }

    #[test]
    fn test_validation_error_contains_policy_info() {
        let err = validate_id("invalid", IDPolicy::UUID).unwrap_err();
        assert_eq!(err.policy, IDPolicy::UUID);
        assert_eq!(err.value, "invalid");
        assert!(!err.message.is_empty());
    }

    // ==================== UUID Validator Tests ====================

    #[test]
    fn test_uuid_validator_valid() {
        let validator = UuidIdValidator;
        let result = validator.validate("550e8400-e29b-41d4-a716-446655440000");
        result.unwrap_or_else(|e| panic!("valid UUID should pass UuidIdValidator: {e}"));
    }

    #[test]
    fn test_uuid_validator_invalid() {
        let validator = UuidIdValidator;
        let result = validator.validate("not-a-uuid");
        assert!(
            matches!(
                result,
                Err(IDValidationError {
                    policy: IDPolicy::UUID,
                    ..
                })
            ),
            "invalid string should fail UuidIdValidator, got: {result:?}"
        );
        assert_eq!(result.unwrap_err().value, "not-a-uuid");
    }

    #[test]
    fn test_uuid_validator_format_name() {
        let validator = UuidIdValidator;
        assert_eq!(validator.format_name(), "UUID");
    }

    #[test]
    fn test_uuid_validator_nil_uuid() {
        let validator = UuidIdValidator;
        validator
            .validate("00000000-0000-0000-0000-000000000000")
            .unwrap_or_else(|e| panic!("nil UUID should pass UuidIdValidator: {e}"));
    }

    #[test]
    fn test_uuid_validator_uppercase() {
        let validator = UuidIdValidator;
        validator
            .validate("550E8400-E29B-41D4-A716-446655440000")
            .unwrap_or_else(|e| panic!("uppercase UUID should pass UuidIdValidator: {e}"));
    }

    // ==================== Numeric Validator Tests ====================

    #[test]
    fn test_numeric_validator_valid_positive() {
        let validator = NumericIdValidator;
        validator
            .validate("12345")
            .unwrap_or_else(|e| panic!("positive int should pass: {e}"));
        validator.validate("0").unwrap_or_else(|e| panic!("zero should pass: {e}"));
        validator
            .validate("9223372036854775807")
            .unwrap_or_else(|e| panic!("i64::MAX should pass: {e}"));
    }

    #[test]
    fn test_numeric_validator_valid_negative() {
        let validator = NumericIdValidator;
        validator
            .validate("-1")
            .unwrap_or_else(|e| panic!("negative int should pass: {e}"));
        validator
            .validate("-12345")
            .unwrap_or_else(|e| panic!("negative int should pass: {e}"));
        validator
            .validate("-9223372036854775808")
            .unwrap_or_else(|e| panic!("i64::MIN should pass: {e}"));
    }

    #[test]
    fn test_numeric_validator_invalid_float() {
        let validator = NumericIdValidator;
        let result = validator.validate("123.45");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "float string should fail NumericIdValidator, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert_eq!(err.value, "123.45");
    }

    #[test]
    fn test_numeric_validator_invalid_non_numeric() {
        let validator = NumericIdValidator;
        let result = validator.validate("abc123");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "non-numeric string should fail NumericIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_numeric_validator_overflow() {
        let validator = NumericIdValidator;
        // Too large for i64
        let result = validator.validate("9223372036854775808");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "i64 overflow should fail NumericIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_numeric_validator_empty_string() {
        let validator = NumericIdValidator;
        let result = validator.validate("");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "empty string should fail NumericIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_numeric_validator_format_name() {
        let validator = NumericIdValidator;
        assert_eq!(validator.format_name(), "integer");
    }

    // ==================== ULID Validator Tests ====================

    #[test]
    fn test_ulid_validator_valid() {
        let validator = UlidIdValidator;
        // Valid ULID: 01ARZ3NDEKTSV4RRFFQ69G5FAV
        validator
            .validate("01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .unwrap_or_else(|e| panic!("valid ULID should pass: {e}"));
    }

    #[test]
    fn test_ulid_validator_valid_all_digits() {
        let validator = UlidIdValidator;
        // Valid ULID with all digits: 01234567890123456789012345
        validator
            .validate("01234567890123456789012345")
            .unwrap_or_else(|e| panic!("all-digit ULID should pass: {e}"));
    }

    #[test]
    fn test_ulid_validator_valid_all_uppercase() {
        let validator = UlidIdValidator;
        // Valid ULID with all uppercase (no I, L, O, U)
        validator
            .validate("ABCDEFGHJKMNPQRSTVWXYZ0123")
            .unwrap_or_else(|e| panic!("all-uppercase ULID should pass: {e}"));
    }

    #[test]
    fn test_ulid_validator_invalid_length_short() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5F");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "short ULID should fail UlidIdValidator, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("26 characters"));
    }

    #[test]
    fn test_ulid_validator_invalid_length_long() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAVA");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "long ULID should fail UlidIdValidator, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("26 characters"));
    }

    #[test]
    fn test_ulid_validator_invalid_lowercase() {
        let validator = UlidIdValidator;
        let result = validator.validate("01arz3ndektsv4rrffq69g5fav");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "lowercase should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_invalid_char_i() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAI");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "char 'I' should fail UlidIdValidator, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("Crockford base32"));
    }

    #[test]
    fn test_ulid_validator_invalid_char_l() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAL");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "char 'L' should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_invalid_char_o() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAO");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "char 'O' should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_invalid_char_u() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAU");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "char 'U' should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_invalid_special_chars() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FA-");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "special char should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_empty_string() {
        let validator = UlidIdValidator;
        let result = validator.validate("");
        assert!(
            matches!(result, Err(IDValidationError { .. })),
            "empty string should fail UlidIdValidator, got: {result:?}"
        );
    }

    #[test]
    fn test_ulid_validator_format_name() {
        let validator = UlidIdValidator;
        assert_eq!(validator.format_name(), "ULID");
    }

    // ==================== Opaque Validator Tests ====================

    #[test]
    fn test_opaque_validator_any_string() {
        let validator = OpaqueIdValidator;
        validator
            .validate("anything")
            .unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
        validator
            .validate("12345")
            .unwrap_or_else(|e| panic!("opaque should accept digits: {e}"));
        validator
            .validate("special@chars!#$%")
            .unwrap_or_else(|e| panic!("opaque should accept special chars: {e}"));
        validator
            .validate("")
            .unwrap_or_else(|e| panic!("opaque should accept empty string: {e}"));
    }

    #[test]
    fn test_opaque_validator_malicious_strings() {
        let validator = OpaqueIdValidator;
        // Opaque validator accepts anything - security is delegated to application layer
        validator
            .validate("'; DROP TABLE users; --")
            .unwrap_or_else(|e| panic!("opaque should accept SQL injection: {e}"));
        validator
            .validate("../../etc/passwd")
            .unwrap_or_else(|e| panic!("opaque should accept path traversal: {e}"));
        validator
            .validate("<script>alert('xss')</script>")
            .unwrap_or_else(|e| panic!("opaque should accept XSS: {e}"));
    }

    #[test]
    fn test_opaque_validator_uuid() {
        let validator = OpaqueIdValidator;
        validator
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("opaque should accept UUID: {e}"));
    }

    #[test]
    fn test_opaque_validator_format_name() {
        let validator = OpaqueIdValidator;
        assert_eq!(validator.format_name(), "opaque");
    }

    // ==================== Cross-Validator Tests ====================

    #[test]
    fn test_validators_trait_object() {
        let validators: Vec<Box<dyn IdValidator>> = vec![
            Box::new(UuidIdValidator),
            Box::new(NumericIdValidator),
            Box::new(UlidIdValidator),
            Box::new(OpaqueIdValidator),
        ];

        for validator in validators {
            // All validators should have format names
            let name = validator.format_name();
            assert!(!name.is_empty());
        }
    }

    #[test]
    fn test_validator_selection_by_id_format() {
        // Demonstrate using correct validator for different ID formats
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let numeric = "12345";
        let ulid = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

        let uuid_validator = UuidIdValidator;
        let numeric_validator = NumericIdValidator;
        let ulid_validator = UlidIdValidator;

        uuid_validator
            .validate(uuid)
            .unwrap_or_else(|e| panic!("UUID validator should accept UUID: {e}"));
        numeric_validator
            .validate(numeric)
            .unwrap_or_else(|e| panic!("numeric validator should accept number: {e}"));
        ulid_validator
            .validate(ulid)
            .unwrap_or_else(|e| panic!("ULID validator should accept ULID: {e}"));

        // Wrong validators should fail
        assert!(
            matches!(uuid_validator.validate(numeric), Err(IDValidationError { .. })),
            "UUID validator should reject numeric ID"
        );
        assert!(
            matches!(numeric_validator.validate(uuid), Err(IDValidationError { .. })),
            "numeric validator should reject UUID"
        );
        assert!(
            matches!(ulid_validator.validate(numeric), Err(IDValidationError { .. })),
            "ULID validator should reject numeric ID"
        );
    }

    // ==================== ID Validation Profile Tests ====================

    #[test]
    fn test_id_validation_profile_uuid() {
        let profile = IDValidationProfile::uuid();
        assert_eq!(profile.name, "uuid");
        profile
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("UUID profile should accept valid UUID: {e}"));
        assert!(
            matches!(profile.validate("not-a-uuid"), Err(IDValidationError { .. })),
            "UUID profile should reject invalid string"
        );
    }

    #[test]
    fn test_id_validation_profile_numeric() {
        let profile = IDValidationProfile::numeric();
        assert_eq!(profile.name, "numeric");
        profile
            .validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile should accept number: {e}"));
        assert!(
            matches!(profile.validate("not-a-number"), Err(IDValidationError { .. })),
            "numeric profile should reject non-number"
        );
    }

    #[test]
    fn test_id_validation_profile_ulid() {
        let profile = IDValidationProfile::ulid();
        assert_eq!(profile.name, "ulid");
        profile
            .validate("01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .unwrap_or_else(|e| panic!("ULID profile should accept valid ULID: {e}"));
        assert!(
            matches!(profile.validate("not-a-ulid"), Err(IDValidationError { .. })),
            "ULID profile should reject invalid string"
        );
    }

    #[test]
    fn test_id_validation_profile_opaque() {
        let profile = IDValidationProfile::opaque();
        assert_eq!(profile.name, "opaque");
        profile
            .validate("anything")
            .unwrap_or_else(|e| panic!("opaque profile should accept any string: {e}"));
        profile
            .validate("12345")
            .unwrap_or_else(|e| panic!("opaque profile should accept digits: {e}"));
        profile
            .validate("special@chars!#$%")
            .unwrap_or_else(|e| panic!("opaque profile should accept special chars: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name() {
        // Test exact matches
        assert!(IDValidationProfile::by_name("uuid").is_some(), "uuid profile should exist");
        assert!(
            IDValidationProfile::by_name("numeric").is_some(),
            "numeric profile should exist"
        );
        assert!(IDValidationProfile::by_name("ulid").is_some(), "ulid profile should exist");
        assert!(IDValidationProfile::by_name("opaque").is_some(), "opaque profile should exist");

        // Test case insensitivity
        assert!(
            IDValidationProfile::by_name("UUID").is_some(),
            "UUID (uppercase) should resolve"
        );
        assert!(
            IDValidationProfile::by_name("NUMERIC").is_some(),
            "NUMERIC (uppercase) should resolve"
        );
        assert!(
            IDValidationProfile::by_name("ULID").is_some(),
            "ULID (uppercase) should resolve"
        );

        // Test aliases
        assert!(
            IDValidationProfile::by_name("integer").is_some(),
            "integer alias should resolve"
        );
        assert!(IDValidationProfile::by_name("string").is_some(), "string alias should resolve");

        // Test invalid
        assert!(
            IDValidationProfile::by_name("invalid").is_none(),
            "unknown name should return None"
        );
    }

    #[test]
    fn test_id_validation_profile_by_name_uuid_validation() {
        let profile = IDValidationProfile::by_name("uuid").unwrap();
        assert_eq!(profile.name, "uuid");
        profile
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("UUID profile by name should accept valid UUID: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name_numeric_validation() {
        let profile = IDValidationProfile::by_name("numeric").unwrap();
        assert_eq!(profile.name, "numeric");
        profile
            .validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile by name should accept number: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name_integer_alias() {
        let profile_numeric = IDValidationProfile::by_name("numeric").unwrap();
        let profile_integer = IDValidationProfile::by_name("integer").unwrap();

        // Both should validate the same way
        profile_numeric
            .validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile should accept number: {e}"));
        profile_integer
            .validate("12345")
            .unwrap_or_else(|e| panic!("integer alias should accept number: {e}"));
        assert!(
            matches!(profile_numeric.validate("not-a-number"), Err(IDValidationError { .. })),
            "numeric profile should reject non-number"
        );
        assert!(
            matches!(profile_integer.validate("not-a-number"), Err(IDValidationError { .. })),
            "integer alias should reject non-number"
        );
    }

    #[test]
    fn test_id_validation_profile_by_name_string_alias() {
        let profile_opaque = IDValidationProfile::by_name("opaque").unwrap();
        let profile_string = IDValidationProfile::by_name("string").unwrap();

        // Both should validate the same way
        profile_opaque
            .validate("anything")
            .unwrap_or_else(|e| panic!("opaque profile should accept any string: {e}"));
        profile_string
            .validate("anything")
            .unwrap_or_else(|e| panic!("string alias should accept any string: {e}"));
    }

    #[test]
    fn test_validation_profile_type_as_validator() {
        let uuid_type = ValidationProfileType::Uuid(UuidIdValidator);
        uuid_type
            .as_validator()
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("UUID profile type should accept valid UUID: {e}"));

        let numeric_type = ValidationProfileType::Numeric(NumericIdValidator);
        numeric_type
            .as_validator()
            .validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile type should accept number: {e}"));

        let ulid_type = ValidationProfileType::Ulid(UlidIdValidator);
        ulid_type
            .as_validator()
            .validate("01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .unwrap_or_else(|e| panic!("ULID profile type should accept valid ULID: {e}"));

        let opaque_type = ValidationProfileType::Opaque(OpaqueIdValidator);
        opaque_type
            .as_validator()
            .validate("any_value")
            .unwrap_or_else(|e| panic!("opaque profile type should accept any string: {e}"));
    }

    #[test]
    fn test_id_validation_profile_clone() {
        let profile1 = IDValidationProfile::uuid();
        let profile2 = profile1.clone();

        assert_eq!(profile1.name, profile2.name);
        profile1
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("original profile should accept valid UUID: {e}"));
        profile2
            .validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("cloned profile should accept valid UUID: {e}"));
    }

    #[test]
    fn test_all_profiles_available() {
        let profiles = [
            IDValidationProfile::uuid(),
            IDValidationProfile::numeric(),
            IDValidationProfile::ulid(),
            IDValidationProfile::opaque(),
        ];

        assert_eq!(profiles.len(), 4);
        assert_eq!(profiles[0].name, "uuid");
        assert_eq!(profiles[1].name, "numeric");
        assert_eq!(profiles[2].name, "ulid");
        assert_eq!(profiles[3].name, "opaque");
    }
}

mod inheritance_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_override_mode() {
        let parent = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Override);
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], ValidationRule::Pattern { .. }));
    }

    #[test]
    fn test_merge_mode() {
        let parent = vec![
            ValidationRule::Required,
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
        ];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_child_first_mode() {
        let parent = vec![ValidationRule::Required];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::ChildFirst);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], ValidationRule::Pattern { .. }));
        assert!(matches!(result[1], ValidationRule::Required));
    }

    #[test]
    fn test_parent_first_mode() {
        let parent = vec![ValidationRule::Required];
        let child = vec![ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: None,
        }];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::ParentFirst);
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], ValidationRule::Required));
        assert!(matches!(result[1], ValidationRule::Pattern { .. }));
    }

    #[test]
    fn test_registry_register_type() {
        let mut registry = ValidationRuleRegistry::new();
        let rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", rules);

        assert!(registry.rules_by_type.contains_key("UserInput"));
    }

    #[test]
    fn test_registry_set_parent() {
        let mut registry = ValidationRuleRegistry::new();
        registry.set_parent("AdminUserInput", "UserInput");

        assert_eq!(registry.get_parent("AdminUserInput"), Some("UserInput"));
    }

    #[test]
    fn test_registry_has_parent() {
        let mut registry = ValidationRuleRegistry::new();
        registry.set_parent("ChildType", "ParentType");

        assert!(registry.has_parent("ChildType"));
        assert!(!registry.has_parent("ParentType"));
    }

    #[test]
    fn test_registry_get_rules_with_merge() {
        let mut registry = ValidationRuleRegistry::new();

        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        assert_eq!(inherited.len(), 2);
    }

    #[test]
    fn test_registry_get_rules_with_override() {
        let mut registry = ValidationRuleRegistry::new();

        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Override);
        assert_eq!(inherited.len(), 1);
        assert!(matches!(inherited[0].rule, ValidationRule::Length { .. }));
    }

    #[test]
    fn test_validate_inheritance_success() {
        let mut registry = ValidationRuleRegistry::new();
        let parent_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", parent_rules);

        let result = validate_inheritance("AdminUserInput", "UserInput", &registry);
        result.unwrap_or_else(|e| panic!("inheritance from registered parent should succeed: {e}"));
    }

    #[test]
    fn test_validate_inheritance_parent_not_found() {
        let registry = ValidationRuleRegistry::new();
        let result = validate_inheritance("AdminUserInput", "NonExistent", &registry);
        assert!(result.is_err(), "inheritance from unknown parent should fail, got: {result:?}");
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_validate_inheritance_circular() {
        let mut registry = ValidationRuleRegistry::new();

        let user_rules = vec![RuleMetadata::new(ValidationRule::Required, "UserInput")];
        registry.register_type("UserInput", user_rules);

        let admin_rules = vec![RuleMetadata::new(
            ValidationRule::Required,
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", admin_rules);

        registry.set_parent("UserInput", "AdminUserInput");
        registry.set_parent("AdminUserInput", "UserInput");

        let result = validate_inheritance("UserInput", "AdminUserInput", &registry);
        assert!(result.is_err(), "circular inheritance should fail, got: {result:?}");
        assert!(result.unwrap_err().contains("Circular"));
    }

    #[test]
    fn test_multi_level_inheritance() {
        let mut registry = ValidationRuleRegistry::new();

        // GrandParent
        let grandparent_rules = vec![RuleMetadata::new(ValidationRule::Required, "BaseInput")];
        registry.register_type("BaseInput", grandparent_rules);

        // Parent
        let parent_rules = vec![RuleMetadata::new(
            ValidationRule::Length {
                min: Some(5),
                max: None,
            },
            "UserInput",
        )];
        registry.register_type("UserInput", parent_rules);
        registry.set_parent("UserInput", "BaseInput");

        // Child
        let child_rules = vec![RuleMetadata::new(
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", child_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        // Should have: grandparent rule + parent rule + child rule
        assert_eq!(inherited.len(), 3);
    }

    #[test]
    fn test_rule_metadata_non_overrideable() {
        let rule = RuleMetadata::new(ValidationRule::Required, "UserInput").non_overrideable();
        assert!(!rule.overrideable);
        assert!(!rule.inherited);
    }

    #[test]
    fn test_rule_metadata_as_inherited() {
        let mut rule = RuleMetadata::new(ValidationRule::Required, "UserInput");
        rule = rule.as_inherited();
        assert!(rule.inherited);
        assert!(rule.overrideable);
    }

    #[test]
    fn test_inheritance_mode_description() {
        assert!(!InheritanceMode::Override.description().is_empty());
        assert!(!InheritanceMode::Merge.description().is_empty());
        assert!(!InheritanceMode::ChildFirst.description().is_empty());
        assert!(!InheritanceMode::ParentFirst.description().is_empty());
    }

    #[test]
    fn test_complex_inheritance_scenario() {
        let mut registry = ValidationRuleRegistry::new();

        // Base: email + minLength 5
        let base_rules = vec![
            RuleMetadata::new(ValidationRule::Required, "BaseInput"),
            RuleMetadata::new(
                ValidationRule::Length {
                    min: Some(5),
                    max: None,
                },
                "BaseInput",
            ),
        ];
        registry.register_type("BaseInput", base_rules);

        // User extends Base: adds pattern
        let user_rules = vec![RuleMetadata::new(
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
            "UserInput",
        )];
        registry.register_type("UserInput", user_rules);
        registry.set_parent("UserInput", "BaseInput");

        // Admin extends User: adds enum constraint
        let admin_rules = vec![RuleMetadata::new(
            ValidationRule::Enum {
                values: vec!["admin".to_string(), "moderator".to_string()],
            },
            "AdminUserInput",
        )];
        registry.register_type("AdminUserInput", admin_rules);
        registry.set_parent("AdminUserInput", "UserInput");

        let inherited = registry.get_rules("AdminUserInput", InheritanceMode::Merge);
        // Should have all rules: 2 from base + 1 from user + 1 from admin = 4
        assert_eq!(inherited.len(), 4);
    }

    #[test]
    fn test_empty_child_rules() {
        let parent = vec![ValidationRule::Required];
        let child: Vec<ValidationRule> = vec![];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_parent_rules() {
        let parent: Vec<ValidationRule> = vec![];
        let child = vec![ValidationRule::Required];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_empty_both_rules() {
        let parent: Vec<ValidationRule> = vec![];
        let child: Vec<ValidationRule> = vec![];

        let result = inherit_validation_rules(&parent, &child, InheritanceMode::Merge);
        assert!(result.is_empty());
    }
}

mod input_object_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_any_of_passes() {
        let input = json!({
            "email": "user@example.com",
            "phone": null,
            "address": null
        });
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| panic!("any_of should pass when email is present: {e}"));
    }

    #[test]
    fn test_any_of_fails() {
        let input = json!({
            "email": null,
            "phone": null,
            "address": null
        });
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("At least one of")),
            "expected Validation error about missing fields, got: {result:?}"
        );
    }

    #[test]
    fn test_one_of_passes() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("one_of should pass when exactly one field is present: {e}")
        });
    }

    #[test]
    fn test_one_of_fails_both_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Exactly one of")),
            "expected Validation error about exactly one field, got: {result:?}"
        );
    }

    #[test]
    fn test_one_of_fails_neither_present() {
        let input = json!({
            "entityId": null,
            "entityPayload": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Exactly one of")),
            "expected Validation error about exactly one field, got: {result:?}"
        );
    }

    #[test]
    fn test_conditional_required_passes() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("conditional_required should pass when condition is met: {e}")
        });
    }

    #[test]
    fn test_conditional_required_fails() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must also be provided")),
            "expected Validation error about missing conditional fields, got: {result:?}"
        );
    }

    #[test]
    fn test_conditional_required_skips_when_condition_false() {
        let input = json!({
            "isPremium": null,
            "paymentMethod": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isPremium".to_string(),
            then_fields: vec!["paymentMethod".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("conditional_required should skip when condition field is null: {e}")
        });
    }

    #[test]
    fn test_required_if_absent_passes() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("required_if_absent should pass when all then_fields are provided: {e}")
        });
    }

    #[test]
    fn test_required_if_absent_fails() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": null,
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must be provided")),
            "expected Validation error about missing required fields, got: {result:?}"
        );
    }

    #[test]
    fn test_required_if_absent_skips_when_field_present() {
        let input = json!({
            "addressId": "addr_123",
            "street": null,
            "city": null,
            "zip": null
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("required_if_absent should skip when absent_field is present: {e}")
        });
    }

    #[test]
    fn test_multiple_rules_all_pass() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null,
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| panic!("multiple rules should all pass: {e}"));
    }

    #[test]
    fn test_multiple_rules_one_fails() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null,
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error when one rule fails, got: {result:?}"
        );
    }

    #[test]
    fn test_multiple_rules_both_fail() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" },
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. })
                if message.contains("Exactly one") || message.contains("must also be provided")),
            "expected aggregated Validation error with both failures, got: {result:?}"
        );
    }

    #[test]
    fn test_error_aggregation() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" },
            "isPremium": true,
            "paymentMethod": null
        });
        let rules = vec![
            InputObjectRule::OneOf {
                fields: vec!["entityId".to_string(), "entityPayload".to_string()],
            },
            InputObjectRule::ConditionalRequired {
                if_field:    "isPremium".to_string(),
                then_fields: vec!["paymentMethod".to_string()],
            },
        ];

        let result = validate_input_object(&input, &rules, Some("createInput"));
        match result {
            Err(FraiseQLError::Validation {
                ref message,
                ref path,
            }) => {
                assert_eq!(*path, Some("createInput".to_string()));
                assert!(message.contains("failed"), "expected 'failed' in message, got: {message}");
            },
            other => panic!("expected Validation error with custom path, got: {other:?}"),
        }
    }

    #[test]
    fn test_conditional_required_multiple_fields() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": "50.00"
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isInternational".to_string(),
            then_fields: vec!["customsCode".to_string(), "importDuties".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("conditional_required with multiple fields should pass: {e}")
        });
    }

    #[test]
    fn test_conditional_required_multiple_fields_one_missing() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": null
        });
        let rules = vec![InputObjectRule::ConditionalRequired {
            if_field:    "isInternational".to_string(),
            then_fields: vec!["customsCode".to_string(), "importDuties".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must also be provided")),
            "expected Validation error about missing conditional field, got: {result:?}"
        );
    }

    #[test]
    fn test_validation_result_aggregation() {
        let mut result = InputObjectValidationResult::new();
        assert!(!result.has_errors());
        assert_eq!(result.error_count, 0);

        result.add_error("Error 1".to_string());
        assert!(result.has_errors());
        assert_eq!(result.error_count, 1);

        result.add_errors(vec!["Error 2".to_string(), "Error 3".to_string()]);
        assert_eq!(result.error_count, 3);
    }

    #[test]
    fn test_validation_result_into_result_success() {
        let result = InputObjectValidationResult::new();
        result
            .into_result()
            .unwrap_or_else(|e| panic!("empty result should be Ok: {e}"));
    }

    #[test]
    fn test_validation_result_into_result_failure() {
        let mut result = InputObjectValidationResult::new();
        result.add_error("Test error".to_string());
        let outcome = result.into_result();
        assert!(
            matches!(outcome, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Test error")),
            "expected Validation error containing 'Test error', got: {outcome:?}"
        );
    }

    #[test]
    fn test_non_object_input() {
        let input = json!([1, 2, 3]);
        let rules = vec![InputObjectRule::AnyOf {
            fields: vec!["field".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("must be an object")),
            "expected Validation error about non-object input, got: {result:?}"
        );
    }

    #[test]
    fn test_empty_rules() {
        let input = json!({"field": "value"});
        let rules: Vec<InputObjectRule> = vec![];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| panic!("empty rules should always pass: {e}"));
    }

    #[test]
    fn test_custom_validator_not_implemented() {
        let input = json!({"field": "value"});
        let rules = vec![InputObjectRule::Custom {
            name: "myValidator".to_string(),
        }];
        let result = validate_input_object(&input, &rules, None);
        match result {
            Err(FraiseQLError::Validation { ref message, .. }) => {
                assert!(
                    message.contains("myValidator"),
                    "expected 'myValidator' in message, got: {message}"
                );
                assert!(
                    message.contains("InputValidatorRegistry"),
                    "expected 'InputValidatorRegistry' in message, got: {message}"
                );
            },
            other => {
                panic!("expected Validation error about unregistered validator, got: {other:?}")
            },
        }
    }

    #[test]
    fn test_complex_create_or_reference_pattern() {
        // Either provide entityId OR provide (name + description), but not both
        let input = json!({
            "entityId": "123",
            "name": null,
            "description": null
        });
        let rules = vec![InputObjectRule::OneOf {
            fields: vec!["entityId".to_string(), "name".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("create_or_reference pattern should pass with entityId: {e}")
        });
    }

    #[test]
    fn test_complex_address_pattern() {
        // Either provide addressId OR provide all of (street, city, zip)
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let rules = vec![InputObjectRule::RequiredIfAbsent {
            absent_field: "addressId".to_string(),
            then_fields:  vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        }];
        let result = validate_input_object(&input, &rules, None);
        result.unwrap_or_else(|e| {
            panic!("address pattern should pass with all fields provided: {e}")
        });
    }
}

mod input_processor_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_process_valid_uuid_id() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "userId": "550e8400-e29b-41d4-a716-446655440000"
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("valid UUID should pass: {e}"));
    }

    #[test]
    fn test_process_invalid_uuid_id() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "userId": "invalid-id"
        });

        let result = process_variables(&variables, &config);
        let err = result.expect_err("invalid UUID should fail validation");
        assert!(
            err.field_path.contains("userId"),
            "expected field_path to contain 'userId', got: {}",
            err.field_path
        );
    }

    #[test]
    fn test_process_multiple_ids() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "userId": "550e8400-e29b-41d4-a716-446655440000",
            "postId": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "name": "John"
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("multiple valid UUIDs should pass: {e}"));
    }

    #[test]
    fn test_process_nested_ids() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "input": {
                "userId": "550e8400-e29b-41d4-a716-446655440000",
                "profile": {
                    "authorId": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
                }
            }
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("nested valid UUIDs should pass: {e}"));
    }

    #[test]
    fn test_process_nested_invalid_id() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "input": {
                "userId": "550e8400-e29b-41d4-a716-446655440000",
                "profile": {
                    "authorId": "invalid"
                }
            }
        });

        let result = process_variables(&variables, &config);
        let err = result.expect_err("nested invalid UUID should fail");
        assert!(
            err.field_path.contains("authorId"),
            "expected field_path to contain 'authorId', got: {}",
            err.field_path
        );
    }

    #[test]
    fn test_process_array_of_ids() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "userIds": [
                "550e8400-e29b-41d4-a716-446655440000",
                "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
            ]
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("array of valid UUIDs should pass: {e}"));
    }

    #[test]
    fn test_process_array_with_invalid_id() {
        let mut config = InputProcessingConfig::strict_uuid();
        // Add "userIds" as a recognized ID field
        config.add_id_field("userIds".to_string());
        let variables = json!({
            "userIds": [
                "550e8400-e29b-41d4-a716-446655440000",
                "invalid-id"
            ]
        });

        let result = process_variables(&variables, &config);
        let err = result.expect_err("array with invalid UUID should fail");
        assert!(
            err.field_path.contains("userIds"),
            "expected field_path to contain 'userIds', got: {}",
            err.field_path
        );
    }

    #[test]
    fn test_opaque_policy_accepts_any_id() {
        let config = InputProcessingConfig::opaque();
        let variables = json!({
            "userId": "any-string-here"
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("opaque policy should accept any ID: {e}"));
    }

    #[test]
    fn test_disabled_validation_skips_checks() {
        let mut config = InputProcessingConfig::strict_uuid();
        config.validate_ids = false;

        let variables = json!({
            "userId": "invalid-id"
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("disabled validation should skip checks: {e}"));
    }

    #[test]
    fn test_custom_id_field_names() {
        let mut config = InputProcessingConfig::strict_uuid();
        config.add_id_field("customId".to_string());

        let variables = json!({
            "customId": "550e8400-e29b-41d4-a716-446655440000"
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| panic!("custom ID field with valid UUID should pass: {e}"));
    }

    #[test]
    fn test_process_null_variables() {
        let config = InputProcessingConfig::strict_uuid();
        let result = process_variables(&Value::Null, &config);
        let value = result.unwrap_or_else(|e| panic!("null variables should pass: {e}"));
        assert!(value.is_null(), "expected null output, got: {value:?}");
    }

    #[test]
    fn test_non_id_fields_pass_through() {
        let config = InputProcessingConfig::strict_uuid();
        let variables = json!({
            "name": "not-a-uuid",
            "email": "invalid-format@email",
            "age": 25
        });

        let result = process_variables(&variables, &config);
        result.unwrap_or_else(|e| {
            panic!("non-ID fields should pass through without validation: {e}")
        });
    }
}

mod mutual_exclusivity_tests {
    use serde_json::json;

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_one_of_validator_exactly_one_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": null
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        result.unwrap_or_else(|e| panic!("expected exactly-one to pass with one present: {e}"));
    }

    #[test]
    fn test_one_of_validator_both_present() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Exactly one of")),
            "expected Validation error for both fields present, got: {result:?}"
        );
    }

    #[test]
    fn test_one_of_validator_neither_present() {
        let input = json!({
            "entityId": null,
            "entityPayload": null
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Exactly one of")),
            "expected Validation error for neither field present, got: {result:?}"
        );
    }

    #[test]
    fn test_one_of_validator_missing_field() {
        let input = json!({
            "entityId": "123"
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected exactly-one to pass with one field missing from object: {e}")
        });
    }

    #[test]
    fn test_any_of_validator_one_present() {
        let input = json!({
            "email": "user@example.com",
            "phone": null,
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        result.unwrap_or_else(|e| panic!("expected any-of to pass with one present: {e}"));
    }

    #[test]
    fn test_any_of_validator_multiple_present() {
        let input = json!({
            "email": "user@example.com",
            "phone": "+1234567890",
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        result.unwrap_or_else(|e| panic!("expected any-of to pass with multiple present: {e}"));
    }

    #[test]
    fn test_any_of_validator_none_present() {
        let input = json!({
            "email": null,
            "phone": null,
            "address": null
        });
        let result = AnyOfValidator::validate(
            &input,
            &[
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("At least one of")),
            "expected Validation error for no fields present, got: {result:?}"
        );
    }

    #[test]
    fn test_conditional_required_validator_condition_met_requirement_met() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": "credit_card"
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected conditional-required to pass when requirement met: {e}")
        });
    }

    #[test]
    fn test_conditional_required_validator_condition_met_requirement_missing() {
        let input = json!({
            "isPremium": true,
            "paymentMethod": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Since") && message.contains("must also be provided")),
            "expected Validation error for missing conditional requirement, got: {result:?}"
        );
    }

    #[test]
    fn test_conditional_required_validator_condition_not_met() {
        let input = json!({
            "isPremium": null,
            "paymentMethod": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isPremium",
            &["paymentMethod".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected conditional-required to pass when condition not met: {e}")
        });
    }

    #[test]
    fn test_conditional_required_validator_multiple_requirements() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": "50.00"
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isInternational",
            &["customsCode".to_string(), "importDuties".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected conditional-required to pass with all requirements met: {e}")
        });
    }

    #[test]
    fn test_conditional_required_validator_one_requirement_missing() {
        let input = json!({
            "isInternational": true,
            "customsCode": "ABC123",
            "importDuties": null
        });
        let result = ConditionalRequiredValidator::validate(
            &input,
            "isInternational",
            &["customsCode".to_string(), "importDuties".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Since") && message.contains("must also be provided")),
            "expected Validation error for one missing requirement, got: {result:?}"
        );
    }

    #[test]
    fn test_required_if_absent_validator_field_absent_requirements_met() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": "Springfield",
            "zip": "12345"
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected required-if-absent to pass when requirements met: {e}")
        });
    }

    #[test]
    fn test_required_if_absent_validator_field_absent_requirements_missing() {
        let input = json!({
            "addressId": null,
            "street": "123 Main St",
            "city": null,
            "zip": "12345"
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Since") && message.contains("must be provided")),
            "expected Validation error for missing requirements when field absent, got: {result:?}"
        );
    }

    #[test]
    fn test_required_if_absent_validator_field_present() {
        let input = json!({
            "addressId": "addr_123",
            "street": null,
            "city": null,
            "zip": null
        });
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string(), "zip".to_string()],
            None,
        );
        result.unwrap_or_else(|e| {
            panic!("expected required-if-absent to pass when field present: {e}")
        });
    }

    #[test]
    fn test_required_if_absent_validator_all_missing_from_object() {
        let input = json!({});
        let result = RequiredIfAbsentValidator::validate(
            &input,
            "addressId",
            &["street".to_string(), "city".to_string()],
            None,
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref message, .. }) if message.contains("Since") && message.contains("must be provided")),
            "expected Validation error for all fields missing from empty object, got: {result:?}"
        );
    }

    #[test]
    fn test_error_messages_include_context() {
        let input = json!({
            "entityId": "123",
            "entityPayload": { "name": "test" }
        });
        let result = OneOfValidator::validate(
            &input,
            &["entityId".to_string(), "entityPayload".to_string()],
            Some("createInput"),
        );
        assert!(
            matches!(result, Err(FraiseQLError::Validation { ref path, .. }) if *path == Some("createInput".to_string())),
            "expected Validation error with path 'createInput', got: {result:?}"
        );
    }
}

mod rate_limiting_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;
    use crate::validation::rate_limiting::DimensionRateLimiter;

    #[test]
    fn test_dimension_rate_limiter_allows_within_limit() {
        let limiter = DimensionRateLimiter::new(3, 60);
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 1: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 2: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 3: {e}"));
    }

    #[test]
    fn test_dimension_rate_limiter_rejects_over_limit() {
        let limiter = DimensionRateLimiter::new(2, 60);
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 1: {e}"));
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok on request 2: {e}"));
        assert!(
            matches!(limiter.check("key"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited error on request 3, got: {:?}",
            limiter.check("key")
        );
    }

    #[test]
    fn test_dimension_rate_limiter_per_key() {
        let limiter = DimensionRateLimiter::new(2, 60);
        limiter
            .check("key1")
            .unwrap_or_else(|e| panic!("expected Ok for key1 request 1: {e}"));
        limiter
            .check("key1")
            .unwrap_or_else(|e| panic!("expected Ok for key1 request 2: {e}"));
        limiter
            .check("key2")
            .unwrap_or_else(|e| panic!("expected Ok for key2 request 1 (independent key): {e}"));
    }

    #[test]
    fn test_dimension_rate_limiter_clear() {
        let limiter = DimensionRateLimiter::new(1, 60);
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok before limit: {e}"));
        assert!(
            matches!(limiter.check("key"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited error at limit, got: {:?}",
            limiter.check("key")
        );
        limiter.clear();
        limiter.check("key").unwrap_or_else(|e| panic!("expected Ok after clear: {e}"));
    }

    #[test]
    fn test_config_defaults() {
        let config = ValidationRateLimitingConfig::default();
        assert!(config.enabled);
        assert!(config.validation_errors_max_requests > 0);
        assert!(config.depth_errors_max_requests > 0);
        assert!(config.complexity_errors_max_requests > 0);
        assert!(config.malformed_errors_max_requests > 0);
        assert!(config.async_validation_errors_max_requests > 0);
    }

    #[test]
    fn test_validation_limiter_independent_dimensions() {
        let config = ValidationRateLimitingConfig::default();
        let limiter = ValidationRateLimiter::new(&config);
        let key = "test-key";

        // Fill up validation errors
        for _ in 0..100 {
            let _ = limiter.check_validation_errors(key);
        }

        // Validation errors should be limited
        assert!(
            matches!(limiter.check_validation_errors(key), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited after exhausting validation_errors quota"
        );

        // But other dimensions should still work
        limiter
            .check_depth_errors(key)
            .unwrap_or_else(|e| panic!("depth_errors should still allow: {e}"));
        limiter
            .check_complexity_errors(key)
            .unwrap_or_else(|e| panic!("complexity_errors should still allow: {e}"));
        limiter
            .check_malformed_errors(key)
            .unwrap_or_else(|e| panic!("malformed_errors should still allow: {e}"));
        limiter
            .check_async_validation_errors(key)
            .unwrap_or_else(|e| panic!("async_validation_errors should still allow: {e}"));
    }

    #[test]
    fn test_validation_limiter_clone_shares_state() {
        let config = ValidationRateLimitingConfig::default();
        let limiter1 = ValidationRateLimiter::new(&config);
        let limiter2 = limiter1.clone();

        let key = "shared-key";

        for _ in 0..100 {
            let _ = limiter1.check_validation_errors(key);
        }

        // limiter2 should see the same limit
        assert!(
            matches!(limiter2.check_validation_errors(key), Err(FraiseQLError::RateLimited { .. })),
            "cloned limiter should share rate limit state"
        );
    }

    #[test]
    fn test_window_rollover_does_not_leak_across_windows() {
        use std::time::Duration;

        use crate::utils::clock::ManualClock;

        let clock = ManualClock::new();
        let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());
        let config = ValidationRateLimitingConfig {
            enabled: true,
            validation_errors_max_requests: 2,
            validation_errors_window_secs: 60,
            ..ValidationRateLimitingConfig::default()
        };
        let limiter = ValidationRateLimiter::new_with_clock(&config, clock_arc);

        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok on 1st request: {e}")); // 1st
        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok on 2nd request: {e}")); // 2nd
        assert!(
            matches!(limiter.check_validation_errors("u1"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited on 3rd request (over limit)"
        ); // over limit

        clock.advance(Duration::from_secs(61)); // cross the window boundary

        limiter
            .check_validation_errors("u1")
            .unwrap_or_else(|e| panic!("expected Ok after window rollover: {e}")); // new window, limit reset
    }

    /// Sentinel: advancing by exactly `window_secs` must reset the window.
    ///
    /// Kills the `>= → >` mutation on the window-expiry check:
    /// `now >= record.window_start + self.dimension.window_secs`
    #[test]
    fn test_window_exact_boundary_triggers_rollover() {
        use std::time::Duration;

        use crate::utils::clock::ManualClock;

        let clock = ManualClock::new();
        let clock_arc: Arc<dyn Clock> = Arc::new(clock.clone());
        let window_secs = 60u64;
        let max = 2u32;
        let limiter = DimensionRateLimiter::new_with_clock(max, window_secs, clock_arc);

        // Fill to limit
        for _ in 0..max {
            limiter.check("u").unwrap_or_else(|e| panic!("expected Ok filling window: {e}"));
        }
        assert!(
            matches!(limiter.check("u"), Err(FraiseQLError::RateLimited { .. })),
            "expected RateLimited when over limit"
        );

        // Advance by EXACTLY window_secs — the `>=` boundary must trigger a reset
        clock.advance(Duration::from_secs(window_secs));

        limiter
            .check("u")
            .unwrap_or_else(|e| panic!("expected Ok at exact window boundary (>= not >): {e}"));
    }

    /// Sentinel: `max_requests = 0` must disable the limiter (every request allowed).
    ///
    /// Kills the `== 0 → != 0` and `== 0 → > 0` mutations on `is_rate_limited()`.
    #[test]
    fn test_max_requests_zero_disables_limiter() {
        let limiter = DimensionRateLimiter::new(0, 60);

        for i in 0..10u32 {
            limiter
                .check("key")
                .unwrap_or_else(|e| panic!("expected Ok with max_requests=0 on request {i}: {e}"));
        }
    }

    /// Sentinel: `window_secs = 0` must not panic.
    ///
    /// With a zero-length window `now >= window_start + 0` is always true, so
    /// every call resets the counter and the limiter never triggers.
    #[test]
    fn test_window_secs_zero_does_not_panic() {
        use crate::utils::clock::ManualClock;

        let clock_arc: Arc<dyn Clock> = Arc::new(ManualClock::new());
        // max_requests > 0 so the limiter is "active", but window_secs = 0
        let limiter = DimensionRateLimiter::new_with_clock(5, 0, clock_arc);

        // Every request resets the window because now >= window_start + 0 is always true
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (1st): {e}"));
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (2nd): {e}"));
        limiter
            .check("key")
            .unwrap_or_else(|e| panic!("expected Ok with window_secs=0 (3rd): {e}"));
    }
}

mod rich_scalars_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    // Email tests
    #[test]
    fn test_email_valid() {
        assert!(EmailValidator::validate("user@example.com"));
        assert!(EmailValidator::validate("john.doe@company.co.uk"));
    }

    #[test]
    fn test_email_invalid() {
        assert!(!EmailValidator::validate("invalid.email"));
        assert!(!EmailValidator::validate("user@"));
        assert!(!EmailValidator::validate("@example.com"));
        // Single-label domain (no TLD dot) must be rejected — regression for patterns::EMAIL `+` vs
        // `*`
        assert!(!EmailValidator::validate("user@localhost"));
        assert!(!EmailValidator::validate("user@example"));
    }

    #[test]
    fn test_email_empty() {
        assert!(!EmailValidator::validate(""));
    }

    // Phone tests
    #[test]
    fn test_phone_valid_plus_format() {
        assert!(PhoneNumberValidator::validate("+1234567890"));
        assert!(PhoneNumberValidator::validate("+33612345678"));
    }

    #[test]
    fn test_phone_valid_no_plus() {
        assert!(PhoneNumberValidator::validate("1234567890"));
    }

    #[test]
    fn test_phone_invalid() {
        assert!(!PhoneNumberValidator::validate("+0123456789")); // Can't start with 0
        assert!(!PhoneNumberValidator::validate(""));
    }

    // VIN tests
    #[test]
    fn test_vin_valid() {
        assert!(VinValidator::validate("3G1FB1E30D1109186"));
        assert!(VinValidator::validate("JH2RC5004LM200591"));
    }

    #[test]
    fn test_vin_valid_lowercase() {
        assert!(VinValidator::validate("3g1fb1e30d1109186"));
    }

    #[test]
    fn test_vin_invalid_length() {
        assert!(!VinValidator::validate("3G1FB1E30D110918"));
        assert!(!VinValidator::validate("3G1FB1E30D11091861"));
    }

    #[test]
    fn test_vin_invalid_chars() {
        assert!(!VinValidator::validate("3G1FB1E30D110918I")); // Contains I
        assert!(!VinValidator::validate("3G1FB1E30D110918O")); // Contains O
        assert!(!VinValidator::validate("3G1FB1E30D110918Q")); // Contains Q
    }

    #[test]
    fn test_vin_empty_rejected_by_length_guard() {
        assert!(!VinValidator::validate(""), "empty string rejected before regex");
    }

    #[test]
    fn test_vin_16_chars_rejected_by_length_guard() {
        // One char short — should be rejected by the length guard, not the regex.
        assert!(!VinValidator::validate("3G1FB1E30D110918"), "16-char VIN rejected");
    }

    #[test]
    fn test_vin_18_chars_rejected_by_length_guard() {
        // One char too long.
        assert!(!VinValidator::validate("3G1FB1E30D11091862"), "18-char VIN rejected");
    }

    #[test]
    fn test_vin_very_long_string_rejected_by_length_guard() {
        // 100-char input — the guard must reject this before allocating uppercase.
        let long_input = "A".repeat(100);
        assert!(!VinValidator::validate(&long_input), "100-char string rejected");
    }

    // Country code tests
    #[test]
    fn test_country_code_valid() {
        let validator = CountryCodeValidator::new();
        assert!(validator.validate("US"));
        assert!(validator.validate("GB"));
        assert!(validator.validate("DE"));
        assert!(validator.validate("FR"));
    }

    #[test]
    fn test_country_code_lowercase() {
        let validator = CountryCodeValidator::new();
        assert!(validator.validate("us"));
        assert!(validator.validate("gb"));
    }

    #[test]
    fn test_country_code_invalid() {
        let validator = CountryCodeValidator::new();
        assert!(!validator.validate("XX"));
        assert!(!validator.validate("USA"));
        assert!(!validator.validate("U"));
    }
}

mod rules_tests {
    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_required_rule() {
        let rule = ValidationRule::Required;
        assert!(rule.is_required());
    }

    #[test]
    fn test_pattern_rule() {
        let rule = ValidationRule::Pattern {
            pattern: "^[a-z]+$".to_string(),
            message: Some("Only lowercase letters allowed".to_string()),
        };
        assert!(!rule.is_required());
        let desc = rule.description();
        assert_eq!(desc, "Only lowercase letters allowed");
    }

    #[test]
    fn test_length_rule() {
        let rule = ValidationRule::Length {
            min: Some(5),
            max: Some(10),
        };
        let desc = rule.description();
        assert!(desc.contains('5'));
        assert!(desc.contains("10"));
    }

    #[test]
    fn test_rule_serialization() {
        let rule = ValidationRule::Enum {
            values: vec!["active".to_string(), "inactive".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::Enum { .. }));
    }

    #[test]
    fn test_composite_all_rule() {
        let rule = ValidationRule::All(vec![
            ValidationRule::Required,
            ValidationRule::Pattern {
                pattern: "^[a-z]+$".to_string(),
                message: None,
            },
        ]);
        let desc = rule.description();
        assert!(desc.contains("All rules"));
    }

    #[test]
    fn test_one_of_rule() {
        let rule = ValidationRule::OneOf {
            fields: vec!["entityId".to_string(), "entityPayload".to_string()],
        };
        assert!(!rule.is_required());
        let desc = rule.description();
        assert!(desc.contains("Exactly one"));
        assert!(desc.contains("entityId"));
        assert!(desc.contains("entityPayload"));
    }

    #[test]
    fn test_any_of_rule() {
        let rule = ValidationRule::AnyOf {
            fields: vec![
                "email".to_string(),
                "phone".to_string(),
                "address".to_string(),
            ],
        };
        let desc = rule.description();
        assert!(desc.contains("At least one"));
        assert!(desc.contains("email"));
        assert!(desc.contains("phone"));
        assert!(desc.contains("address"));
    }

    #[test]
    fn test_conditional_required_rule() {
        let rule = ValidationRule::ConditionalRequired {
            if_field_present: "entityId".to_string(),
            then_required:    vec!["createdAt".to_string(), "updatedAt".to_string()],
        };
        let desc = rule.description();
        assert!(desc.contains("If"));
        assert!(desc.contains("entityId"));
        assert!(desc.contains("createdAt"));
        assert!(desc.contains("updatedAt"));
    }

    #[test]
    fn test_required_if_absent_rule() {
        let rule = ValidationRule::RequiredIfAbsent {
            absent_field:  "addressId".to_string(),
            then_required: vec!["street".to_string(), "city".to_string(), "zip".to_string()],
        };
        let desc = rule.description();
        assert!(desc.contains("If"));
        assert!(desc.contains("addressId"));
        assert!(desc.contains("absent"));
        assert!(desc.contains("street"));
    }

    #[test]
    fn test_one_of_serialization() {
        let rule = ValidationRule::OneOf {
            fields: vec!["id".to_string(), "payload".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::OneOf { .. }));
    }

    #[test]
    fn test_conditional_required_serialization() {
        let rule = ValidationRule::ConditionalRequired {
            if_field_present: "isPremium".to_string(),
            then_required:    vec!["paymentMethod".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::ConditionalRequired { .. }));
    }

    #[test]
    fn test_required_if_absent_serialization() {
        let rule = ValidationRule::RequiredIfAbsent {
            absent_field:  "presetId".to_string(),
            then_required: vec!["settings".to_string()],
        };
        let json = serde_json::to_string(&rule).expect("serialization failed");
        let deserialized: ValidationRule =
            serde_json::from_str(&json).expect("deserialization failed");
        assert!(matches!(deserialized, ValidationRule::RequiredIfAbsent { .. }));
    }
}

mod scalar_validator_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::{Value, json};

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;
    use crate::error::{FraiseQLError, Result};

    /// Passthrough scalar that always succeeds (for happy-path tests).
    #[derive(Debug)]
    struct PassthroughScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for PassthroughScalar {
        fn name(&self) -> &str {
            "Passthrough"
        }

        fn serialize(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_value(&self, value: &Value) -> Result<Value> {
            Ok(value.clone())
        }

        fn parse_literal(&self, ast: &Value) -> Result<Value> {
            Ok(ast.clone())
        }
    }

    /// Scalar that always fails with a descriptive error.
    #[derive(Debug)]
    struct FailScalar;

    #[allow(clippy::unnecessary_literal_bound)] // Reason: test impl of trait returning a literal
    impl CustomScalar for FailScalar {
        fn name(&self) -> &str {
            "AlwaysFail"
        }

        fn serialize(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("serialize always fails"))
        }

        fn parse_value(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("parse_value always fails"))
        }

        fn parse_literal(&self, _: &Value) -> Result<Value> {
            Err(FraiseQLError::validation("parse_literal always fails"))
        }
    }

    // ── ValidationContext tests ────────────────────────────────────────────────

    #[test]
    fn test_validation_context_as_str_serialize() {
        assert_eq!(ValidationContext::Serialize.as_str(), "serialize");
    }

    #[test]
    fn test_validation_context_as_str_parse_value() {
        assert_eq!(ValidationContext::ParseValue.as_str(), "parseValue");
    }

    #[test]
    fn test_validation_context_as_str_parse_literal() {
        assert_eq!(ValidationContext::ParseLiteral.as_str(), "parseLiteral");
    }

    #[test]
    fn test_validation_context_eq() {
        assert_eq!(ValidationContext::Serialize, ValidationContext::Serialize);
        assert_ne!(ValidationContext::Serialize, ValidationContext::ParseValue);
    }

    // ── ScalarValidationError tests ────────────────────────────────────────────

    #[test]
    fn test_scalar_validation_error_new() {
        let err = ScalarValidationError::new("Email", "parseValue", "not an email");
        assert_eq!(err.scalar_name, "Email");
        assert_eq!(err.context, "parseValue");
        assert_eq!(err.message, "not an email");
    }

    #[test]
    fn test_scalar_validation_error_display() {
        let err = ScalarValidationError::new("Email", "parseValue", "bad input");
        let s = format!("{err}");
        assert!(s.contains("Email"), "missing scalar name: {s}");
        assert!(s.contains("parseValue"), "missing context: {s}");
        assert!(s.contains("bad input"), "missing message: {s}");
    }

    #[test]
    fn test_scalar_validation_error_into_fraiseql_error() {
        let err = ScalarValidationError::new("T", "serialize", "oops");
        let fraiseql_err = err.into_fraiseql_error();
        let msg = format!("{fraiseql_err}");
        assert!(msg.contains("oops"), "error message lost: {msg}");
    }

    // ── validate_custom_scalar tests ──────────────────────────────────────────

    #[test]
    fn test_validate_serialize_success() {
        let scalar = PassthroughScalar;
        let v = json!("hello");
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::Serialize);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_parse_value_success() {
        let scalar = PassthroughScalar;
        let v = json!(42);
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::ParseValue);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_parse_literal_success() {
        let scalar = PassthroughScalar;
        let v = json!(true);
        let result = validate_custom_scalar(&scalar, &v, ValidationContext::ParseLiteral);
        assert_eq!(result.unwrap(), v);
    }

    #[test]
    fn test_validate_serialize_failure_wraps_error() {
        let scalar = FailScalar;
        let err =
            validate_custom_scalar(&scalar, &json!("x"), ValidationContext::Serialize).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("serialize"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn test_validate_parse_value_failure_wraps_error() {
        let scalar = FailScalar;
        let err = validate_custom_scalar(&scalar, &json!("x"), ValidationContext::ParseValue)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("parseValue"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn test_validate_parse_literal_failure_wraps_error() {
        let scalar = FailScalar;
        let err = validate_custom_scalar(&scalar, &json!("x"), ValidationContext::ParseLiteral)
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("AlwaysFail") || msg.contains("parseLiteral"),
            "unexpected error message: {msg}"
        );
    }

    // ── validate_custom_scalar_parse_value convenience fn ─────────────────────

    #[test]
    fn test_convenience_fn_success() {
        let scalar = PassthroughScalar;
        let v = json!("text");
        assert_eq!(validate_custom_scalar_parse_value(&scalar, &v).unwrap(), v);
    }

    #[test]
    fn test_convenience_fn_failure() {
        let scalar = FailScalar;
        let result = validate_custom_scalar_parse_value(&scalar, &json!("x"));
        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error, got: {result:?}"
        );
    }
}

mod validators_tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::super::*;
    #[allow(unused_imports)]
    // Reason: nested test mod re-imports may not all be used by every test
    use super::*;

    #[test]
    fn test_pattern_validator() {
        let validator = PatternValidator::new_default_message("^[a-z]+$").unwrap();
        assert!(validator.validate_pattern("hello"));
        assert!(!validator.validate_pattern("Hello"));
        assert!(!validator.validate_pattern("hello123"));
    }

    #[test]
    fn test_pattern_validator_validation() {
        let validator = PatternValidator::new_default_message("^[a-z]+$").unwrap();
        validator
            .validate("hello", "name")
            .unwrap_or_else(|e| panic!("lowercase-only string should pass pattern: {e}"));
        assert!(
            matches!(validator.validate("Hello", "name"), Err(FraiseQLError::Validation { .. })),
            "mixed-case string should fail pattern with Validation error"
        );
    }

    #[test]
    fn test_length_validator() {
        let validator = LengthValidator::new(Some(3), Some(10));
        assert!(validator.validate_length("hello"));
        assert!(!validator.validate_length("ab"));
        assert!(!validator.validate_length("this is too long"));
    }

    #[test]
    fn test_length_validator_error_message() {
        let validator = LengthValidator::new(Some(5), Some(10));
        let msg = validator.error_message();
        assert!(msg.contains('5'));
        assert!(msg.contains("10"));
    }

    #[test]
    fn test_range_validator() {
        let validator = RangeValidator::new(Some(0), Some(100));
        assert!(validator.validate_range(50));
        assert!(!validator.validate_range(-1));
        assert!(!validator.validate_range(101));
    }

    #[test]
    fn test_enum_validator() {
        let validator = EnumValidator::new(vec![
            "active".to_string(),
            "inactive".to_string(),
            "pending".to_string(),
        ]);
        assert!(validator.validate_enum("active"));
        assert!(!validator.validate_enum("unknown"));
    }

    #[test]
    fn test_required_validator() {
        let validator = RequiredValidator;
        validator
            .validate("hello", "name")
            .unwrap_or_else(|e| panic!("non-empty string should pass required validator: {e}"));
        assert!(
            matches!(validator.validate("", "name"), Err(FraiseQLError::Validation { .. })),
            "empty string should fail required validator with Validation error"
        );
    }

    #[test]
    fn test_create_validator_from_rule() {
        let rule = ValidationRule::Pattern {
            pattern: "^test".to_string(),
            message: None,
        };
        let validator = create_validator_from_rule(&rule);
        assert!(validator.is_some());
    }
}

mod js_codegen_tests {
    use crate::validation::js_codegen::JsCodegen;

    #[test]
    fn new_returns_default_instance() {
        let _cg = JsCodegen::new();
    }

    #[test]
    fn emit_validator_produces_function() {
        let cg = JsCodegen::new();
        let js = cg.emit_validator("User", "age >= 18");
        assert!(
            js.contains("export function validate_User(data)"),
            "should produce named export function"
        );
        assert!(js.contains("errors"), "should include errors array");
    }

    #[test]
    fn emit_validator_translates_comparison() {
        let cg = JsCodegen::new();
        let js = cg.emit_validator("Item", "price > 0");
        assert!(
            js.contains("data.price > 0"),
            "field comparison should reference data.field: {js}"
        );
    }

    #[test]
    fn emit_validator_translates_equality_to_strict() {
        let cg = JsCodegen::new();
        let js = cg.emit_validator("Flag", "status == 1");
        assert!(js.contains("==="), "== should become === in JavaScript: {js}");
    }

    #[test]
    fn emit_validator_translates_not_equal_to_strict() {
        let cg = JsCodegen::new();
        let js = cg.emit_validator("Flag", "status != 0");
        assert!(js.contains("!=="), "!= should become !== in JavaScript: {js}");
    }

    #[test]
    fn emit_module_includes_header_and_multiple_validators() {
        let cg = JsCodegen::new();
        let module = cg.emit_module(&[("User", "age >= 18"), ("Post", "length(title) > 0")]);
        assert!(module.contains("Generated by FraiseQL"), "module should include header comment");
        assert!(module.contains("validate_User"), "module should include User validator");
        assert!(module.contains("validate_Post"), "module should include Post validator");
    }

    #[test]
    fn emit_module_empty_list() {
        let cg = JsCodegen::new();
        let module = cg.emit_module(&[]);
        assert!(
            module.contains("Generated by FraiseQL"),
            "empty module should still include header"
        );
        assert!(!module.contains("export function"), "empty module should have no functions");
    }

    #[test]
    fn elo_to_js_logical_and() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("a > 1 && b < 10");
        assert!(js.contains("&&"), "should preserve AND operator: {js}");
    }

    #[test]
    fn elo_to_js_logical_or() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("x == 1 || y == 2");
        assert!(js.contains("||"), "should preserve OR operator: {js}");
    }

    #[test]
    fn elo_to_js_negation() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("!active");
        assert!(js.contains("!data.active"), "should negate field ref: {js}");
    }

    #[test]
    fn elo_to_js_length_function() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("length(name) > 0");
        assert!(
            js.contains("(data.name || '').length"),
            "length() should map to JS string length: {js}"
        );
    }

    #[test]
    fn elo_to_js_contains_function() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("contains(tags, \"rust\")");
        assert!(js.contains(".includes("), "contains() should map to JS includes: {js}");
    }

    #[test]
    fn elo_to_js_matches_function() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("matches(email, \"^[^@]+@\")");
        assert!(js.contains("new RegExp("), "matches() should produce RegExp in JS: {js}");
    }

    #[test]
    fn elo_to_js_numeric_literal() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("count > 42");
        assert!(js.contains("42"), "numeric literal should pass through: {js}");
    }

    #[test]
    fn elo_to_js_boolean_literal() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("active == true");
        assert!(js.contains("true"), "boolean literal should pass through: {js}");
    }

    #[test]
    fn elo_to_js_parenthesized_expression() {
        let cg = JsCodegen::new();
        let js = cg.elo_to_js("(a > 1)");
        assert!(js.starts_with('(') && js.ends_with(')'), "should preserve parentheses: {js}");
    }
}
