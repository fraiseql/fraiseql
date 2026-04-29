//! Rich scalar type validators for specialized data formats.
//!
//! All validators in this module delegate to `fraiseql-validators` for
//! spec-correct, RFC/ISO-compliant implementations. Validation correctness
//! and coverage are the responsibility of that crate's test suite.

use fraiseql_validators::{
    contact::{Email, PhoneE164},
    geographic::CountryCode,
    identifiers::Vin,
};

/// Email address validator.
pub struct EmailValidator;

impl EmailValidator {
    /// Validate an email address format.
    ///
    /// Delegates to `fraiseql-validators` for RFC 5321-compliant validation.
    /// Note: This validates format only, not domain existence (use async validator for that).
    pub fn validate(value: &str) -> bool {
        Email::try_from(value).is_ok()
    }

    /// Return the standard validation error message for an invalid email.
    pub const fn error_message() -> &'static str {
        "Invalid email format"
    }
}

/// International phone number validator (E.164 format).
pub struct PhoneNumberValidator;

impl PhoneNumberValidator {
    /// Validate an E.164 international phone number.
    ///
    /// Delegates to `fraiseql-validators` for ITU-T E.164-compliant validation:
    /// a `+` followed by a non-zero country code digit and 6–14 more digits.
    pub fn validate(value: &str) -> bool {
        PhoneE164::try_from(value).is_ok()
    }

    /// Return the standard validation error message for an invalid phone number.
    pub const fn error_message() -> &'static str {
        "Invalid phone number format"
    }
}

/// VIN (Vehicle Identification Number) validator.
pub struct VinValidator;

impl VinValidator {
    /// Validate a VIN.
    ///
    /// Delegates to `fraiseql-validators` for ISO 3779-compliant validation:
    /// 17 alphanumeric characters (no I, O, Q) with a valid check digit.
    /// Case insensitive — uppercased before validation.
    pub fn validate(value: &str) -> bool {
        if value.len() != 17 {
            return false;
        }
        let upper = value.to_uppercase();
        Vin::try_from(upper.as_str()).is_ok()
    }

    /// Return the standard validation error message for an invalid VIN.
    pub const fn error_message() -> &'static str {
        "Invalid VIN format (must be 17 alphanumeric characters, excluding I, O, Q)"
    }
}

/// Country code validator (ISO 3166-1 alpha-2).
pub struct CountryCodeValidator;

impl CountryCodeValidator {
    /// Create a new country code validator.
    pub const fn new() -> Self {
        Self
    }

    /// Validate a country code against the ISO 3166-1 alpha-2 standard.
    ///
    /// Delegates to `fraiseql-validators` for the authoritative code list.
    /// Case insensitive — "us" and "US" are both accepted.
    pub fn validate(&self, value: &str) -> bool {
        CountryCode::try_from(value).is_ok()
    }

    /// Return the standard validation error message for an invalid country code.
    pub const fn error_message() -> &'static str {
        "Invalid country code (must be ISO 3166-1 alpha-2)"
    }
}

impl Default for CountryCodeValidator {
    fn default() -> Self {
        Self::new()
    }
}
