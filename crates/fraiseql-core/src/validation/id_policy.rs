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
#[non_exhaustive]
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
///
/// # Errors
///
/// Returns [`IDValidationError`] if `id` does not conform to `policy`
/// (e.g., not a valid UUID when `IDPolicy::UUID` is used).
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
#[non_exhaustive]
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
    #[must_use] 
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

    /// Validate an ID using this profile.
    ///
    /// # Errors
    ///
    /// Returns [`IDValidationError`] if the value does not conform to this
    /// profile's validator (e.g., not a valid UUID, ULID, or integer).
    pub fn validate(&self, value: &str) -> Result<(), IDValidationError> {
        self.validator.as_validator().validate(value)
    }
}
