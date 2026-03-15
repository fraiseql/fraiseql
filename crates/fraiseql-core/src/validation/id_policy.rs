//! ID Policy validation for GraphQL ID scalar type
//!
//! This module provides validation for ID fields based on the configured ID policy.
//!
//! **Design Pattern**: `FraiseQL` supports two ID policies:
//! 1. **UUID**: IDs must be valid UUIDs (`FraiseQL`'s opinionated default)
//! 2. **OPAQUE**: IDs accept any string (GraphQL spec-compliant)
//!
//! This module enforces UUID format validation when `IDPolicy::UUID` is configured.
//!
//! # Example
//!
//! ```
//! use fraiseql_core::validation::{IDPolicy, validate_id};
//!
//! // UUID policy: strict UUID validation
//! let policy = IDPolicy::UUID;
//! assert!(
//!     validate_id("550e8400-e29b-41d4-a716-446655440000", policy).is_ok(),
//!     "valid UUID should pass UUID policy"
//! );
//! assert!(
//!     validate_id("not-a-uuid", policy).is_err(),
//!     "non-UUID string should fail UUID policy"
//! );
//!
//! // OPAQUE policy: any string accepted
//! let policy = IDPolicy::OPAQUE;
//! assert!(
//!     validate_id("not-a-uuid", policy).is_ok(),
//!     "OPAQUE policy should accept any string"
//! );
//! assert!(
//!     validate_id("any-arbitrary-string", policy).is_ok(),
//!     "OPAQUE policy should accept arbitrary strings"
//! );
//! ```

use serde::{Deserialize, Serialize};

/// ID Policy determines how GraphQL ID scalar type behaves
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IDPolicy {
    /// IDs must be valid UUIDs (`FraiseQL`'s opinionated default)
    #[serde(rename = "uuid")]
    #[default]
    UUID,

    /// IDs accept any string (GraphQL specification compliant)
    #[serde(rename = "opaque")]
    OPAQUE,
}

impl IDPolicy {
    /// Check if this policy enforces UUID format for IDs
    #[must_use]
    pub fn enforces_uuid(self) -> bool {
        self == Self::UUID
    }

    /// Get the policy name as a string
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UUID => "uuid",
            Self::OPAQUE => "opaque",
        }
    }
}

impl std::fmt::Display for IDPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Error type for ID validation failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IDValidationError {
    /// The invalid ID value
    pub value:   String,
    /// The policy that was violated
    pub policy:  IDPolicy,
    /// Error message
    pub message: String,
}

impl std::fmt::Display for IDValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for IDValidationError {}

/// Validate an ID string against the configured ID policy
///
/// # Arguments
///
/// * `id` - The ID value to validate
/// * `policy` - The ID policy to enforce
///
/// # Returns
///
/// `Ok(())` if the ID is valid for the policy, `Err(IDValidationError)` otherwise
///
/// # Errors
///
/// Returns `IDValidationError` if the ID does not conform to the specified policy.
/// For `IDPolicy::UUID`, the ID must be a valid UUID. For `IDPolicy::OPAQUE`, any string is valid.
///
/// # Examples
///
/// ```
/// use fraiseql_core::validation::{IDPolicy, validate_id};
///
/// // UUID policy enforces UUID format
/// assert!(
///     validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::UUID).is_ok(),
///     "valid UUID should pass UUID policy"
/// );
/// assert!(
///     validate_id("not-uuid", IDPolicy::UUID).is_err(),
///     "non-UUID string should fail UUID policy"
/// );
///
/// // OPAQUE policy accepts any string
/// assert!(
///     validate_id("anything", IDPolicy::OPAQUE).is_ok(),
///     "OPAQUE policy should accept any string"
/// );
/// assert!(
///     validate_id("", IDPolicy::OPAQUE).is_ok(),
///     "OPAQUE policy should accept empty string"
/// );
/// ```
pub fn validate_id(id: &str, policy: IDPolicy) -> Result<(), IDValidationError> {
    match policy {
        IDPolicy::UUID => validate_uuid_format(id),
        IDPolicy::OPAQUE => Ok(()), // Opaque IDs accept any string
    }
}

/// Validate that an ID is a valid UUID string
///
/// **Security Note**: This is a defense-in-depth check at the Rust runtime layer.
/// The primary enforcement point is the CLI compiler (`fraiseql-cli compile`), which
/// validates ID policy rules when producing `schema.compiled.json`.
///
/// UUID format validation requires:
/// - 36 characters total
/// - 8-4-4-4-12 hexadecimal digits separated by hyphens
/// - Case-insensitive
///
/// # Arguments
///
/// * `id` - The ID string to validate
///
/// # Returns
///
/// `Ok(())` if valid UUID format, `Err(IDValidationError)` otherwise
fn validate_uuid_format(id: &str) -> Result<(), IDValidationError> {
    // UUID must be 36 characters: 8-4-4-4-12
    if id.len() != 36 {
        return Err(IDValidationError {
            value:   id.to_string(),
            policy:  IDPolicy::UUID,
            message: format!(
                "ID must be a valid UUID (36 characters), got {} characters",
                id.len()
            ),
        });
    }

    // Check overall structure: 8-4-4-4-12
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 5 {
        return Err(IDValidationError {
            value:   id.to_string(),
            policy:  IDPolicy::UUID,
            message: "ID must be a valid UUID with format XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
                .to_string(),
        });
    }

    // Validate segment lengths
    let expected_lengths = [8, 4, 4, 4, 12];
    for (i, (part, &expected_len)) in parts.iter().zip(&expected_lengths).enumerate() {
        if part.len() != expected_len {
            return Err(IDValidationError {
                value:   id.to_string(),
                policy:  IDPolicy::UUID,
                message: format!(
                    "UUID segment {} has invalid length: expected {}, got {}",
                    i,
                    expected_len,
                    part.len()
                ),
            });
        }
    }

    // Validate all characters are hexadecimal
    for (i, part) in parts.iter().enumerate() {
        if !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(IDValidationError {
                value:   id.to_string(),
                policy:  IDPolicy::UUID,
                message: format!("UUID segment {i} contains non-hexadecimal characters: '{part}'"),
            });
        }
    }

    Ok(())
}

/// Validate multiple IDs against a policy
///
/// # Arguments
///
/// * `ids` - Slice of ID strings to validate
/// * `policy` - The ID policy to enforce
///
/// # Returns
///
/// `Ok(())` if all IDs are valid, `Err(IDValidationError)` for the first invalid ID
///
/// # Examples
///
/// ```
/// use fraiseql_core::validation::{validate_id, IDPolicy};
///
/// // Validate each ID individually
/// assert!(
///     validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::UUID).is_ok(),
///     "first UUID should pass validation"
/// );
/// assert!(
///     validate_id("6ba7b810-9dad-11d1-80b4-00c04fd430c8", IDPolicy::UUID).is_ok(),
///     "second UUID should pass validation"
/// );
/// ```
///
/// # Errors
///
/// Returns `IDValidationError` if any ID fails validation.
#[allow(dead_code)] // Reason: public API intended for external consumers; no in-crate callers yet
pub fn validate_ids(ids: &[&str], policy: IDPolicy) -> Result<(), IDValidationError> {
    for id in ids {
        validate_id(id, policy)?;
    }
    Ok(())
}

// =============================================================================
// Pluggable ID Validator Trait System
// =============================================================================

/// Trait for pluggable ID validation strategies
///
/// This trait enables users to implement custom ID validation logic
/// beyond the built-in UUID and OPAQUE policies.
///
/// # Examples
///
/// ```no_run
/// # // Requires: IdValidator and IDValidationError are not re-exported from the public API
/// use fraiseql_core::validation::{IDPolicy, IDValidationError};
///
/// struct CustomIdValidator;
///
/// // impl IdValidator for CustomIdValidator { ... }
/// ```
pub trait IdValidator: Send + Sync {
    /// Validate an ID value
    fn validate(&self, value: &str) -> Result<(), IDValidationError>;

    /// Human-readable name of the format (for error messages)
    fn format_name(&self) -> &'static str;
}

/// UUID format validator
#[derive(Debug, Clone, Copy)]
pub struct UuidIdValidator;

impl IdValidator for UuidIdValidator {
    fn validate(&self, value: &str) -> Result<(), IDValidationError> {
        validate_uuid_format(value)
    }

    fn format_name(&self) -> &'static str {
        "UUID"
    }
}

/// Numeric ID validator (integers)
#[derive(Debug, Clone, Copy)]
pub struct NumericIdValidator;

impl IdValidator for NumericIdValidator {
    fn validate(&self, value: &str) -> Result<(), IDValidationError> {
        value.parse::<i64>().map_err(|_| IDValidationError {
            value:   value.to_string(),
            policy:  IDPolicy::OPAQUE,
            message: format!(
                "ID must be a valid {} (parseable as 64-bit integer)",
                self.format_name()
            ),
        })?;
        Ok(())
    }

    fn format_name(&self) -> &'static str {
        "integer"
    }
}

/// ULID format validator (Universally Unique Lexicographically Sortable Identifier)
///
/// ULIDs are 26 uppercase alphanumeric characters, providing sortable unique IDs.
/// Example: `01ARZ3NDEKTSV4RRFFQ69G5FAV`
#[derive(Debug, Clone, Copy)]
pub struct UlidIdValidator;

impl IdValidator for UlidIdValidator {
    fn validate(&self, value: &str) -> Result<(), IDValidationError> {
        if value.len() != 26 {
            return Err(IDValidationError {
                value:   value.to_string(),
                policy:  IDPolicy::OPAQUE,
                message: format!(
                    "ID must be a valid {} ({} characters), got {}",
                    self.format_name(),
                    26,
                    value.len()
                ),
            });
        }

        // ULIDs use Crockford base32 encoding (0-9, A-Z except I, L, O, U)
        if !value.chars().all(|c| {
            c.is_ascii_digit()
                || (c.is_ascii_uppercase() && c != 'I' && c != 'L' && c != 'O' && c != 'U')
        }) {
            return Err(IDValidationError {
                value:   value.to_string(),
                policy:  IDPolicy::OPAQUE,
                message: format!(
                    "ID must be a valid {} (Crockford base32: 0-9, A-Z except I, L, O, U)",
                    self.format_name()
                ),
            });
        }

        Ok(())
    }

    fn format_name(&self) -> &'static str {
        "ULID"
    }
}

/// Opaque ID validator (accepts any string)
#[derive(Debug, Clone, Copy)]
pub struct OpaqueIdValidator;

impl IdValidator for OpaqueIdValidator {
    fn validate(&self, _value: &str) -> Result<(), IDValidationError> {
        Ok(()) // Accept any string
    }

    fn format_name(&self) -> &'static str {
        "opaque"
    }
}

/// ID validation profile for different use cases
///
/// Profiles provide preset ID validation configurations for common scenarios.
/// Each profile includes a name and a validator instance.
///
/// # Built-in Profiles
///
/// - **UUID**: Strict UUID format validation (FraiseQL default)
/// - **Numeric**: Integer-based IDs (suitable for sequential IDs)
/// - **ULID**: Sortable unique identifiers (recommended for distributed systems)
/// - **Opaque**: Any string accepted (GraphQL spec compliant)
#[derive(Debug, Clone)]
pub struct IDValidationProfile {
    /// Profile name (e.g., "uuid", "ulid", "numeric")
    pub name: String,

    /// Validator instance for this profile
    pub validator: ValidationProfileType,
}

/// Type of validation profile
#[derive(Debug, Clone)]
pub enum ValidationProfileType {
    /// UUID format validation
    Uuid(UuidIdValidator),

    /// Numeric (integer) validation
    Numeric(NumericIdValidator),

    /// ULID format validation
    Ulid(UlidIdValidator),

    /// Opaque (any string) validation
    Opaque(OpaqueIdValidator),
}

impl ValidationProfileType {
    /// Get the validator as a trait object
    pub fn as_validator(&self) -> &dyn IdValidator {
        match self {
            Self::Uuid(v) => v,
            Self::Numeric(v) => v,
            Self::Ulid(v) => v,
            Self::Opaque(v) => v,
        }
    }
}

impl IDValidationProfile {
    /// Create a UUID validation profile (FraiseQL default)
    #[must_use]
    pub fn uuid() -> Self {
        Self {
            name:      "uuid".to_string(),
            validator: ValidationProfileType::Uuid(UuidIdValidator),
        }
    }

    /// Create a numeric (integer) validation profile
    #[must_use]
    pub fn numeric() -> Self {
        Self {
            name:      "numeric".to_string(),
            validator: ValidationProfileType::Numeric(NumericIdValidator),
        }
    }

    /// Create a ULID validation profile
    #[must_use]
    pub fn ulid() -> Self {
        Self {
            name:      "ulid".to_string(),
            validator: ValidationProfileType::Ulid(UlidIdValidator),
        }
    }

    /// Create an opaque (any string) validation profile
    #[must_use]
    pub fn opaque() -> Self {
        Self {
            name:      "opaque".to_string(),
            validator: ValidationProfileType::Opaque(OpaqueIdValidator),
        }
    }

    /// Get profile by name
    ///
    /// Returns a profile matching the given name, or None if not found.
    ///
    /// # Built-in Profile Names
    ///
    /// - "uuid" - UUID validation
    /// - "numeric" - Integer validation
    /// - "ulid" - ULID validation
    /// - "opaque" - Any string validation
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "uuid" => Some(Self::uuid()),
            "numeric" | "integer" => Some(Self::numeric()),
            "ulid" => Some(Self::ulid()),
            "opaque" | "string" => Some(Self::opaque()),
            _ => None,
        }
    }

    /// Validate an ID using this profile
    pub fn validate(&self, value: &str) -> Result<(), IDValidationError> {
        self.validator.as_validator().validate(value)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
            "extra chars should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_uuid_missing_hyphens() {
        // 36 chars without hyphens - all hex digits, same length as UUID but no separators
        let result = validate_id("550e8400e29b41d4a716446655440000", IDPolicy::UUID);
        assert!(
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
            "UUID with wrong segment lengths should fail, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("segment"));
    }

    #[test]
    fn test_validate_uuid_non_hex_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-44665544000g", IDPolicy::UUID);
        assert!(
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
            "UUID with non-hex chars should fail, got: {result:?}"
        );
        let err = result.unwrap_err();
        assert!(err.message.contains("non-hexadecimal"));
    }

    #[test]
    fn test_validate_uuid_special_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-4466554400@0", IDPolicy::UUID);
        assert!(
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
            "special chars should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_validate_uuid_empty_string() {
        let result = validate_id("", IDPolicy::UUID);
        assert!(
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
            "SQL injection string should fail UUID validation, got: {result:?}"
        );
    }

    #[test]
    fn test_security_prevent_path_traversal_via_uuid() {
        let result = validate_id("../../etc/passwd", IDPolicy::UUID);
        assert!(
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
            matches!(result, Err(IDValidationError { policy: IDPolicy::UUID, .. })),
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
        validator.validate("12345").unwrap_or_else(|e| panic!("positive int should pass: {e}"));
        validator.validate("0").unwrap_or_else(|e| panic!("zero should pass: {e}"));
        validator
            .validate("9223372036854775807")
            .unwrap_or_else(|e| panic!("i64::MAX should pass: {e}"));
    }

    #[test]
    fn test_numeric_validator_valid_negative() {
        let validator = NumericIdValidator;
        validator.validate("-1").unwrap_or_else(|e| panic!("negative int should pass: {e}"));
        validator.validate("-12345").unwrap_or_else(|e| panic!("negative int should pass: {e}"));
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
        validator.validate("anything").unwrap_or_else(|e| panic!("opaque should accept any string: {e}"));
        validator.validate("12345").unwrap_or_else(|e| panic!("opaque should accept digits: {e}"));
        validator.validate("special@chars!#$%").unwrap_or_else(|e| panic!("opaque should accept special chars: {e}"));
        validator.validate("").unwrap_or_else(|e| panic!("opaque should accept empty string: {e}"));
    }

    #[test]
    fn test_opaque_validator_malicious_strings() {
        let validator = OpaqueIdValidator;
        // Opaque validator accepts anything - security is delegated to application layer
        validator.validate("'; DROP TABLE users; --").unwrap_or_else(|e| panic!("opaque should accept SQL injection: {e}"));
        validator.validate("../../etc/passwd").unwrap_or_else(|e| panic!("opaque should accept path traversal: {e}"));
        validator.validate("<script>alert('xss')</script>").unwrap_or_else(|e| panic!("opaque should accept XSS: {e}"));
    }

    #[test]
    fn test_opaque_validator_uuid() {
        let validator = OpaqueIdValidator;
        validator.validate("550e8400-e29b-41d4-a716-446655440000").unwrap_or_else(|e| panic!("opaque should accept UUID: {e}"));
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

        uuid_validator.validate(uuid).unwrap_or_else(|e| panic!("UUID validator should accept UUID: {e}"));
        numeric_validator.validate(numeric).unwrap_or_else(|e| panic!("numeric validator should accept number: {e}"));
        ulid_validator.validate(ulid).unwrap_or_else(|e| panic!("ULID validator should accept ULID: {e}"));

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
        profile.validate("550e8400-e29b-41d4-a716-446655440000")
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
        profile.validate("12345")
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
        profile.validate("01ARZ3NDEKTSV4RRFFQ69G5FAV")
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
        profile.validate("anything")
            .unwrap_or_else(|e| panic!("opaque profile should accept any string: {e}"));
        profile.validate("12345")
            .unwrap_or_else(|e| panic!("opaque profile should accept digits: {e}"));
        profile.validate("special@chars!#$%")
            .unwrap_or_else(|e| panic!("opaque profile should accept special chars: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name() {
        // Test exact matches
        assert!(IDValidationProfile::by_name("uuid").is_some(), "uuid profile should exist");
        assert!(IDValidationProfile::by_name("numeric").is_some(), "numeric profile should exist");
        assert!(IDValidationProfile::by_name("ulid").is_some(), "ulid profile should exist");
        assert!(IDValidationProfile::by_name("opaque").is_some(), "opaque profile should exist");

        // Test case insensitivity
        assert!(IDValidationProfile::by_name("UUID").is_some(), "UUID (uppercase) should resolve");
        assert!(IDValidationProfile::by_name("NUMERIC").is_some(), "NUMERIC (uppercase) should resolve");
        assert!(IDValidationProfile::by_name("ULID").is_some(), "ULID (uppercase) should resolve");

        // Test aliases
        assert!(IDValidationProfile::by_name("integer").is_some(), "integer alias should resolve");
        assert!(IDValidationProfile::by_name("string").is_some(), "string alias should resolve");

        // Test invalid
        assert!(IDValidationProfile::by_name("invalid").is_none(), "unknown name should return None");
    }

    #[test]
    fn test_id_validation_profile_by_name_uuid_validation() {
        let profile = IDValidationProfile::by_name("uuid").unwrap();
        assert_eq!(profile.name, "uuid");
        profile.validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("UUID profile by name should accept valid UUID: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name_numeric_validation() {
        let profile = IDValidationProfile::by_name("numeric").unwrap();
        assert_eq!(profile.name, "numeric");
        profile.validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile by name should accept number: {e}"));
    }

    #[test]
    fn test_id_validation_profile_by_name_integer_alias() {
        let profile_numeric = IDValidationProfile::by_name("numeric").unwrap();
        let profile_integer = IDValidationProfile::by_name("integer").unwrap();

        // Both should validate the same way
        profile_numeric.validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile should accept number: {e}"));
        profile_integer.validate("12345")
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
        profile_opaque.validate("anything")
            .unwrap_or_else(|e| panic!("opaque profile should accept any string: {e}"));
        profile_string.validate("anything")
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
        numeric_type.as_validator().validate("12345")
            .unwrap_or_else(|e| panic!("numeric profile type should accept number: {e}"));

        let ulid_type = ValidationProfileType::Ulid(UlidIdValidator);
        ulid_type.as_validator().validate("01ARZ3NDEKTSV4RRFFQ69G5FAV")
            .unwrap_or_else(|e| panic!("ULID profile type should accept valid ULID: {e}"));

        let opaque_type = ValidationProfileType::Opaque(OpaqueIdValidator);
        opaque_type.as_validator().validate("any_value")
            .unwrap_or_else(|e| panic!("opaque profile type should accept any string: {e}"));
    }

    #[test]
    fn test_id_validation_profile_clone() {
        let profile1 = IDValidationProfile::uuid();
        let profile2 = profile1.clone();

        assert_eq!(profile1.name, profile2.name);
        profile1.validate("550e8400-e29b-41d4-a716-446655440000")
            .unwrap_or_else(|e| panic!("original profile should accept valid UUID: {e}"));
        profile2.validate("550e8400-e29b-41d4-a716-446655440000")
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
