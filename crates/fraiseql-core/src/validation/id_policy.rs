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
//! assert!(validate_id("550e8400-e29b-41d4-a716-446655440000", policy).is_ok());
//! assert!(validate_id("not-a-uuid", policy).is_err());
//!
//! // OPAQUE policy: any string accepted
//! let policy = IDPolicy::OPAQUE;
//! assert!(validate_id("not-a-uuid", policy).is_ok());
//! assert!(validate_id("any-arbitrary-string", policy).is_ok());
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
/// assert!(validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::UUID).is_ok());
/// assert!(validate_id("not-uuid", IDPolicy::UUID).is_err());
///
/// // OPAQUE policy accepts any string
/// assert!(validate_id("anything", IDPolicy::OPAQUE).is_ok());
/// assert!(validate_id("", IDPolicy::OPAQUE).is_ok());
/// ```
pub fn validate_id(id: &str, policy: IDPolicy) -> Result<(), IDValidationError> {
    match policy {
        IDPolicy::UUID => validate_uuid_format(id),
        IDPolicy::OPAQUE => Ok(()), // Opaque IDs accept any string
    }
}

/// Validate that an ID is a valid UUID string
///
/// **Security Note**: This validation happens at the Rust layer for defense-in-depth.
/// Python layer validation via `IDPolicy` is the primary enforcement mechanism.
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
/// ```ignore
/// use fraiseql_core::validation::{IDPolicy, validate_ids};
///
/// let ids = vec![
///     "550e8400-e29b-41d4-a716-446655440000",
///     "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
/// ];
/// assert!(validate_ids(&ids, IDPolicy::UUID).is_ok());
/// ```
///
/// # Errors
///
/// Returns `IDValidationError` if any ID fails validation.
#[allow(dead_code)]
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
/// ```ignore
/// use fraiseql_core::validation::IdValidator;
///
/// struct CustomIdValidator;
///
/// impl IdValidator for CustomIdValidator {
///     fn validate(&self, value: &str) -> Result<(), IDValidationError> {
///         if value.starts_with("CUSTOM-") {
///             Ok(())
///         } else {
///             Err(IDValidationError {
///                 value: value.to_string(),
///                 policy: IDPolicy::OPAQUE,
///                 message: "Custom IDs must start with 'CUSTOM-'".to_string(),
///             })
///         }
///     }
///
///     fn format_name(&self) -> &'static str {
///         "CUSTOM"
///     }
/// }
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
            value: value.to_string(),
            policy: IDPolicy::OPAQUE,
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
                value: value.to_string(),
                policy: IDPolicy::OPAQUE,
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
                value: value.to_string(),
                policy: IDPolicy::OPAQUE,
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
            name: "uuid".to_string(),
            validator: ValidationProfileType::Uuid(UuidIdValidator),
        }
    }

    /// Create a numeric (integer) validation profile
    #[must_use]
    pub fn numeric() -> Self {
        Self {
            name: "numeric".to_string(),
            validator: ValidationProfileType::Numeric(NumericIdValidator),
        }
    }

    /// Create a ULID validation profile
    #[must_use]
    pub fn ulid() -> Self {
        Self {
            name: "ulid".to_string(),
            validator: ValidationProfileType::Ulid(UlidIdValidator),
        }
    }

    /// Create an opaque (any string) validation profile
    #[must_use]
    pub fn opaque() -> Self {
        Self {
            name: "opaque".to_string(),
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
    use super::*;

    // ==================== UUID Format Tests ====================

    #[test]
    fn test_validate_valid_uuid() {
        // Standard UUID format
        let result = validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::UUID);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_valid_uuid_uppercase() {
        // UUIDs are case-insensitive
        let result = validate_id("550E8400-E29B-41D4-A716-446655440000", IDPolicy::UUID);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_valid_uuid_mixed_case() {
        let result = validate_id("550e8400-E29b-41d4-A716-446655440000", IDPolicy::UUID);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_nil_uuid() {
        // Nil UUID (all zeros) is valid
        let result = validate_id("00000000-0000-0000-0000-000000000000", IDPolicy::UUID);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_max_uuid() {
        // Max UUID (all Fs) is valid
        let result = validate_id("ffffffff-ffff-ffff-ffff-ffffffffffff", IDPolicy::UUID);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_uuid_wrong_length() {
        let result = validate_id("550e8400-e29b-41d4-a716", IDPolicy::UUID);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.policy, IDPolicy::UUID);
        assert!(err.message.contains("36 characters"));
    }

    #[test]
    fn test_validate_uuid_extra_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-446655440000x", IDPolicy::UUID);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_uuid_missing_hyphens() {
        // 36 chars without hyphens - all hex digits, same length as UUID but no separators
        let result = validate_id("550e8400e29b41d4a716446655440000", IDPolicy::UUID);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // Fails length check since 32 chars != 36
        assert!(err.message.contains("36 characters"));
    }

    #[test]
    fn test_validate_uuid_wrong_segment_lengths() {
        // First segment too short (7 chars instead of 8)
        // Need 36 chars total, so pad the last segment: 550e840-e29b-41d4-a716-4466554400001
        let result = validate_id("550e840-e29b-41d4-a716-4466554400001", IDPolicy::UUID);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("segment"));
    }

    #[test]
    fn test_validate_uuid_non_hex_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-44665544000g", IDPolicy::UUID);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("non-hexadecimal"));
    }

    #[test]
    fn test_validate_uuid_special_chars() {
        let result = validate_id("550e8400-e29b-41d4-a716-4466554400@0", IDPolicy::UUID);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_uuid_empty_string() {
        let result = validate_id("", IDPolicy::UUID);
        assert!(result.is_err());
    }

    // ==================== OPAQUE Policy Tests ====================

    #[test]
    fn test_opaque_accepts_any_string() {
        assert!(validate_id("not-a-uuid", IDPolicy::OPAQUE).is_ok());
        assert!(validate_id("anything", IDPolicy::OPAQUE).is_ok());
        assert!(validate_id("12345", IDPolicy::OPAQUE).is_ok());
        assert!(validate_id("special@chars!#$%", IDPolicy::OPAQUE).is_ok());
    }

    #[test]
    fn test_opaque_accepts_empty_string() {
        assert!(validate_id("", IDPolicy::OPAQUE).is_ok());
    }

    #[test]
    fn test_opaque_accepts_uuid() {
        assert!(validate_id("550e8400-e29b-41d4-a716-446655440000", IDPolicy::OPAQUE).is_ok());
    }

    // ==================== Multiple IDs Tests ====================

    #[test]
    fn test_validate_multiple_valid_uuids() {
        let ids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ];
        assert!(validate_ids(&ids, IDPolicy::UUID).is_ok());
    }

    #[test]
    fn test_validate_multiple_fails_on_first_invalid() {
        let ids = vec![
            "550e8400-e29b-41d4-a716-446655440000",
            "invalid-id",
            "6ba7b811-9dad-11d1-80b4-00c04fd430c8",
        ];
        let result = validate_ids(&ids, IDPolicy::UUID);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().value, "invalid-id");
    }

    #[test]
    fn test_validate_multiple_opaque_all_pass() {
        let ids = vec!["anything", "goes", "here", "12345"];
        assert!(validate_ids(&ids, IDPolicy::OPAQUE).is_ok());
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
        assert!(result.is_err());
    }

    #[test]
    fn test_security_prevent_path_traversal_via_uuid() {
        let result = validate_id("../../etc/passwd", IDPolicy::UUID);
        assert!(result.is_err());
    }

    #[test]
    fn test_security_opaque_policy_accepts_any_format() {
        // OPAQUE policy explicitly accepts any string
        // Input validation and authorization must be done elsewhere
        assert!(validate_id("'; DROP TABLE users; --", IDPolicy::OPAQUE).is_ok());
        assert!(validate_id("../../etc/passwd", IDPolicy::OPAQUE).is_ok());
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
        assert!(result.is_ok());
    }

    #[test]
    fn test_uuid_validator_invalid() {
        let validator = UuidIdValidator;
        let result = validator.validate("not-a-uuid");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.value, "not-a-uuid");
    }

    #[test]
    fn test_uuid_validator_format_name() {
        let validator = UuidIdValidator;
        assert_eq!(validator.format_name(), "UUID");
    }

    #[test]
    fn test_uuid_validator_nil_uuid() {
        let validator = UuidIdValidator;
        assert!(validator.validate("00000000-0000-0000-0000-000000000000").is_ok());
    }

    #[test]
    fn test_uuid_validator_uppercase() {
        let validator = UuidIdValidator;
        assert!(validator.validate("550E8400-E29B-41D4-A716-446655440000").is_ok());
    }

    // ==================== Numeric Validator Tests ====================

    #[test]
    fn test_numeric_validator_valid_positive() {
        let validator = NumericIdValidator;
        assert!(validator.validate("12345").is_ok());
        assert!(validator.validate("0").is_ok());
        assert!(validator.validate("9223372036854775807").is_ok()); // i64::MAX
    }

    #[test]
    fn test_numeric_validator_valid_negative() {
        let validator = NumericIdValidator;
        assert!(validator.validate("-1").is_ok());
        assert!(validator.validate("-12345").is_ok());
        assert!(validator.validate("-9223372036854775808").is_ok()); // i64::MIN
    }

    #[test]
    fn test_numeric_validator_invalid_float() {
        let validator = NumericIdValidator;
        let result = validator.validate("123.45");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.value, "123.45");
    }

    #[test]
    fn test_numeric_validator_invalid_non_numeric() {
        let validator = NumericIdValidator;
        let result = validator.validate("abc123");
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_validator_overflow() {
        let validator = NumericIdValidator;
        // Too large for i64
        let result = validator.validate("9223372036854775808");
        assert!(result.is_err());
    }

    #[test]
    fn test_numeric_validator_empty_string() {
        let validator = NumericIdValidator;
        let result = validator.validate("");
        assert!(result.is_err());
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
        assert!(validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
    }

    #[test]
    fn test_ulid_validator_valid_all_digits() {
        let validator = UlidIdValidator;
        // Valid ULID with all digits: 01234567890123456789012345
        assert!(validator.validate("01234567890123456789012345").is_ok());
    }

    #[test]
    fn test_ulid_validator_valid_all_uppercase() {
        let validator = UlidIdValidator;
        // Valid ULID with all uppercase (no I, L, O, U)
        assert!(validator.validate("ABCDEFGHJKMNPQRSTVWXYZ0123").is_ok());
    }

    #[test]
    fn test_ulid_validator_invalid_length_short() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5F");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("26 characters"));
    }

    #[test]
    fn test_ulid_validator_invalid_length_long() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAVA");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("26 characters"));
    }

    #[test]
    fn test_ulid_validator_invalid_lowercase() {
        let validator = UlidIdValidator;
        let result = validator.validate("01arz3ndektsv4rrffq69g5fav");
        assert!(result.is_err());
    }

    #[test]
    fn test_ulid_validator_invalid_char_i() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAI");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Crockford base32"));
    }

    #[test]
    fn test_ulid_validator_invalid_char_l() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAL");
        assert!(result.is_err());
    }

    #[test]
    fn test_ulid_validator_invalid_char_o() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAO");
        assert!(result.is_err());
    }

    #[test]
    fn test_ulid_validator_invalid_char_u() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FAU");
        assert!(result.is_err());
    }

    #[test]
    fn test_ulid_validator_invalid_special_chars() {
        let validator = UlidIdValidator;
        let result = validator.validate("01ARZ3NDEKTSV4RRFFQ69G5FA-");
        assert!(result.is_err());
    }

    #[test]
    fn test_ulid_validator_empty_string() {
        let validator = UlidIdValidator;
        let result = validator.validate("");
        assert!(result.is_err());
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
        assert!(validator.validate("anything").is_ok());
        assert!(validator.validate("12345").is_ok());
        assert!(validator.validate("special@chars!#$%").is_ok());
        assert!(validator.validate("").is_ok());
    }

    #[test]
    fn test_opaque_validator_malicious_strings() {
        let validator = OpaqueIdValidator;
        // Opaque validator accepts anything - security is delegated to application layer
        assert!(validator.validate("'; DROP TABLE users; --").is_ok());
        assert!(validator.validate("../../etc/passwd").is_ok());
        assert!(validator.validate("<script>alert('xss')</script>").is_ok());
    }

    #[test]
    fn test_opaque_validator_uuid() {
        let validator = OpaqueIdValidator;
        assert!(validator.validate("550e8400-e29b-41d4-a716-446655440000").is_ok());
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

        assert!(uuid_validator.validate(uuid).is_ok());
        assert!(numeric_validator.validate(numeric).is_ok());
        assert!(ulid_validator.validate(ulid).is_ok());

        // Wrong validators should fail
        assert!(uuid_validator.validate(numeric).is_err());
        assert!(numeric_validator.validate(uuid).is_err());
        assert!(ulid_validator.validate(numeric).is_err());
    }

    // ==================== ID Validation Profile Tests ====================

    #[test]
    fn test_id_validation_profile_uuid() {
        let profile = IDValidationProfile::uuid();
        assert_eq!(profile.name, "uuid");
        assert!(profile.validate("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(profile.validate("not-a-uuid").is_err());
    }

    #[test]
    fn test_id_validation_profile_numeric() {
        let profile = IDValidationProfile::numeric();
        assert_eq!(profile.name, "numeric");
        assert!(profile.validate("12345").is_ok());
        assert!(profile.validate("not-a-number").is_err());
    }

    #[test]
    fn test_id_validation_profile_ulid() {
        let profile = IDValidationProfile::ulid();
        assert_eq!(profile.name, "ulid");
        assert!(profile.validate("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
        assert!(profile.validate("not-a-ulid").is_err());
    }

    #[test]
    fn test_id_validation_profile_opaque() {
        let profile = IDValidationProfile::opaque();
        assert_eq!(profile.name, "opaque");
        assert!(profile.validate("anything").is_ok());
        assert!(profile.validate("12345").is_ok());
        assert!(profile.validate("special@chars!#$%").is_ok());
    }

    #[test]
    fn test_id_validation_profile_by_name() {
        // Test exact matches
        assert!(IDValidationProfile::by_name("uuid").is_some());
        assert!(IDValidationProfile::by_name("numeric").is_some());
        assert!(IDValidationProfile::by_name("ulid").is_some());
        assert!(IDValidationProfile::by_name("opaque").is_some());

        // Test case insensitivity
        assert!(IDValidationProfile::by_name("UUID").is_some());
        assert!(IDValidationProfile::by_name("NUMERIC").is_some());
        assert!(IDValidationProfile::by_name("ULID").is_some());

        // Test aliases
        assert!(IDValidationProfile::by_name("integer").is_some());
        assert!(IDValidationProfile::by_name("string").is_some());

        // Test invalid
        assert!(IDValidationProfile::by_name("invalid").is_none());
    }

    #[test]
    fn test_id_validation_profile_by_name_uuid_validation() {
        let profile = IDValidationProfile::by_name("uuid").unwrap();
        assert_eq!(profile.name, "uuid");
        assert!(profile.validate("550e8400-e29b-41d4-a716-446655440000").is_ok());
    }

    #[test]
    fn test_id_validation_profile_by_name_numeric_validation() {
        let profile = IDValidationProfile::by_name("numeric").unwrap();
        assert_eq!(profile.name, "numeric");
        assert!(profile.validate("12345").is_ok());
    }

    #[test]
    fn test_id_validation_profile_by_name_integer_alias() {
        let profile_numeric = IDValidationProfile::by_name("numeric").unwrap();
        let profile_integer = IDValidationProfile::by_name("integer").unwrap();

        // Both should validate the same way
        assert!(profile_numeric.validate("12345").is_ok());
        assert!(profile_integer.validate("12345").is_ok());
        assert!(profile_numeric.validate("not-a-number").is_err());
        assert!(profile_integer.validate("not-a-number").is_err());
    }

    #[test]
    fn test_id_validation_profile_by_name_string_alias() {
        let profile_opaque = IDValidationProfile::by_name("opaque").unwrap();
        let profile_string = IDValidationProfile::by_name("string").unwrap();

        // Both should validate the same way
        assert!(profile_opaque.validate("anything").is_ok());
        assert!(profile_string.validate("anything").is_ok());
    }

    #[test]
    fn test_validation_profile_type_as_validator() {
        let uuid_type = ValidationProfileType::Uuid(UuidIdValidator);
        assert!(uuid_type.as_validator().validate("550e8400-e29b-41d4-a716-446655440000").is_ok());

        let numeric_type = ValidationProfileType::Numeric(NumericIdValidator);
        assert!(numeric_type.as_validator().validate("12345").is_ok());

        let ulid_type = ValidationProfileType::Ulid(UlidIdValidator);
        assert!(ulid_type.as_validator().validate("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());

        let opaque_type = ValidationProfileType::Opaque(OpaqueIdValidator);
        assert!(opaque_type.as_validator().validate("any_value").is_ok());
    }

    #[test]
    fn test_id_validation_profile_clone() {
        let profile1 = IDValidationProfile::uuid();
        let profile2 = profile1.clone();

        assert_eq!(profile1.name, profile2.name);
        assert!(profile1.validate("550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(profile2.validate("550e8400-e29b-41d4-a716-446655440000").is_ok());
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
