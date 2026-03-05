//! Async validation framework for validators requiring runtime operations.
//!
//! This module provides traits and helpers for validators that need to perform
//! asynchronous operations like network requests or database lookups.
//!
//! The built-in implementations (`EmailFormatValidator`, `PhoneE164Validator`) perform
//! local regex validation only — no network I/O. They implement `AsyncValidator` so they
//! compose with the same dispatch infrastructure as future network-backed validators.

use std::{sync::LazyLock, time::Duration};

use regex::Regex;

use crate::error::{FraiseQLError, Result};

/// Async validator result type.
pub type AsyncValidatorResult = Result<()>;

/// Email format regex (RFC 5321 practical subset).
///
/// Validates `local-part@domain` where the domain contains at least one dot-separated
/// label. Identical to the pattern used in `rich_scalars`.
static EMAIL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+$",
    )
    .expect("email format regex is valid")
});

/// E.164 phone number regex.
///
/// Accepts `+` followed by a non-zero leading digit and 6–14 more digits
/// (7–15 total digits after the `+`), covering all valid ITU-T E.164 numbers.
static PHONE_E164_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\+[1-9]\d{6,14}$").expect("E.164 phone regex is valid")
});

/// Provider types for async validators.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AsyncValidatorProvider {
    /// Email format validation (RFC 5321 regex)
    EmailFormatCheck,
    /// Phone number E.164 format validation
    PhoneE164Check,
    /// IBAN/VIN checksum validation
    ChecksumValidation,
    /// Custom provider
    Custom(String),
}

impl AsyncValidatorProvider {
    /// Get provider name for logging/debugging
    pub fn name(&self) -> String {
        match self {
            Self::EmailFormatCheck => "email_format_check".to_string(),
            Self::PhoneE164Check => "phone_e164_check".to_string(),
            Self::ChecksumValidation => "checksum_validation".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }
}

impl std::fmt::Display for AsyncValidatorProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Configuration for an async validator.
#[derive(Debug, Clone)]
pub struct AsyncValidatorConfig {
    /// The provider to use
    pub provider:       AsyncValidatorProvider,
    /// Timeout duration for the validation operation
    pub timeout:        Duration,
    /// Cache TTL in seconds (0 = no caching)
    pub cache_ttl_secs: u64,
    /// Field pattern this validator applies to (e.g., "*.email")
    pub field_pattern:  String,
}

impl AsyncValidatorConfig {
    /// Create a new async validator configuration.
    pub fn new(provider: AsyncValidatorProvider, timeout_ms: u64) -> Self {
        Self {
            provider,
            timeout: Duration::from_millis(timeout_ms),
            cache_ttl_secs: 0,
            field_pattern: String::new(),
        }
    }

    /// Set cache TTL for this validator.
    pub fn with_cache_ttl(mut self, secs: u64) -> Self {
        self.cache_ttl_secs = secs;
        self
    }

    /// Set field pattern for this validator.
    pub fn with_field_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.field_pattern = pattern.into();
        self
    }
}

/// Trait for async validators.
///
/// Implementers should handle timeout and error cases gracefully.
#[async_trait::async_trait]
pub trait AsyncValidator: Send + Sync {
    /// Validate a value asynchronously.
    ///
    /// # Arguments
    /// * `value` - The value to validate
    /// * `field` - The field name (for error reporting)
    ///
    /// # Returns
    /// `Ok(())` if valid, `Err(FraiseQLError)` if invalid
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult;

    /// Get the provider this validator uses
    fn provider(&self) -> AsyncValidatorProvider;

    /// Get the timeout for this validator
    fn timeout(&self) -> Duration;
}

/// Email format validator.
///
/// Validates that a string is a well-formed email address using the RFC 5321
/// practical regex (`local-part@domain.tld`). No network I/O is performed.
///
/// # Example
///
/// ```
/// use fraiseql_core::validation::async_validators::{AsyncValidator, EmailFormatValidator};
///
/// # #[tokio::main]
/// # async fn main() {
/// let v = EmailFormatValidator::new();
/// assert!(v.validate_async("alice@example.com", "email").await.is_ok());
/// assert!(v.validate_async("not-an-email", "email").await.is_err());
/// # }
/// ```
pub struct EmailFormatValidator {
    config: AsyncValidatorConfig,
}

impl EmailFormatValidator {
    /// Create a new email format validator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: AsyncValidatorConfig::new(AsyncValidatorProvider::EmailFormatCheck, 0),
        }
    }
}

impl Default for EmailFormatValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AsyncValidator for EmailFormatValidator {
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult {
        if EMAIL_REGEX.is_match(value) {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!("Invalid email format for field '{field}'"),
                path:    Some(field.to_string()),
            })
        }
    }

    fn provider(&self) -> AsyncValidatorProvider {
        self.config.provider.clone()
    }

    fn timeout(&self) -> Duration {
        self.config.timeout
    }
}

/// E.164 phone number validator.
///
/// Validates that a string is a valid E.164 international phone number:
/// a `+` followed by a non-zero country code digit and 6–14 more digits
/// (7–15 digits total after the `+`). No network I/O is performed.
///
/// # Example
///
/// ```
/// use fraiseql_core::validation::async_validators::{AsyncValidator, PhoneE164Validator};
///
/// # #[tokio::main]
/// # async fn main() {
/// let v = PhoneE164Validator::new();
/// assert!(v.validate_async("+14155552671", "phone").await.is_ok());
/// assert!(v.validate_async("0044207946000", "phone").await.is_err()); // missing +
/// assert!(v.validate_async("+123", "phone").await.is_err());           // too short
/// # }
/// ```
pub struct PhoneE164Validator {
    config: AsyncValidatorConfig,
}

impl PhoneE164Validator {
    /// Create a new E.164 phone number validator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: AsyncValidatorConfig::new(AsyncValidatorProvider::PhoneE164Check, 0),
        }
    }
}

impl Default for PhoneE164Validator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AsyncValidator for PhoneE164Validator {
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult {
        if PHONE_E164_REGEX.is_match(value) {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Invalid E.164 phone number for field '{field}': \
                     expected '+' followed by 7–15 digits (e.g. +14155552671)"
                ),
                path:    Some(field.to_string()),
            })
        }
    }

    fn provider(&self) -> AsyncValidatorProvider {
        self.config.provider.clone()
    }

    fn timeout(&self) -> Duration {
        self.config.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── EmailFormatValidator ──────────────────────────────────────────────────

    #[tokio::test]
    async fn test_email_valid_simple() {
        let v = EmailFormatValidator::new();
        assert!(v.validate_async("user@example.com", "email").await.is_ok());
    }

    #[tokio::test]
    async fn test_email_valid_subdomain() {
        let v = EmailFormatValidator::new();
        assert!(v.validate_async("user@mail.example.co.uk", "email").await.is_ok());
    }

    #[tokio::test]
    async fn test_email_valid_plus_addressing() {
        let v = EmailFormatValidator::new();
        assert!(v.validate_async("user+tag@example.com", "email").await.is_ok());
    }

    #[tokio::test]
    async fn test_email_valid_corporate_domain() {
        let v = EmailFormatValidator::new();
        // Must accept any valid domain, not a hardcoded allowlist
        assert!(v.validate_async("alice@my-company.io", "email").await.is_ok());
        assert!(v.validate_async("bob@university.edu", "email").await.is_ok());
    }

    #[tokio::test]
    async fn test_email_invalid_no_at() {
        let v = EmailFormatValidator::new();
        assert!(v.validate_async("notanemail", "email").await.is_err());
    }

    #[tokio::test]
    async fn test_email_invalid_no_tld() {
        let v = EmailFormatValidator::new();
        // Single label after @ has no dot — rejected
        assert!(v.validate_async("user@localhost", "email").await.is_err());
    }

    #[tokio::test]
    async fn test_email_invalid_empty() {
        let v = EmailFormatValidator::new();
        assert!(v.validate_async("", "email").await.is_err());
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
        assert!(v.validate_async("+14155552671", "phone").await.is_ok());
    }

    #[tokio::test]
    async fn test_phone_valid_uk() {
        let v = PhoneE164Validator::new();
        assert!(v.validate_async("+447911123456", "phone").await.is_ok());
    }

    #[tokio::test]
    async fn test_phone_valid_any_country_code() {
        let v = PhoneE164Validator::new();
        // Must accept all country codes, not a hardcoded subset
        assert!(v.validate_async("+819012345678", "phone").await.is_ok()); // Japan
        assert!(v.validate_async("+5511987654321", "phone").await.is_ok()); // Brazil
        assert!(v.validate_async("+27821234567", "phone").await.is_ok()); // South Africa
    }

    #[tokio::test]
    async fn test_phone_invalid_missing_plus() {
        let v = PhoneE164Validator::new();
        assert!(v.validate_async("14155552671", "phone").await.is_err());
    }

    #[tokio::test]
    async fn test_phone_invalid_too_short() {
        let v = PhoneE164Validator::new();
        // 5 digits after + — below E.164 minimum of 7
        assert!(v.validate_async("+12345", "phone").await.is_err());
    }

    #[tokio::test]
    async fn test_phone_invalid_too_long() {
        let v = PhoneE164Validator::new();
        // 16 digits after + — above E.164 maximum of 15
        assert!(v.validate_async("+1234567890123456", "phone").await.is_err());
    }

    #[tokio::test]
    async fn test_phone_invalid_leading_zero_country_code() {
        let v = PhoneE164Validator::new();
        assert!(v.validate_async("+0441234567890", "phone").await.is_err());
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
        assert_eq!(
            AsyncValidatorProvider::EmailFormatCheck.to_string(),
            "email_format_check"
        );
        assert_eq!(
            AsyncValidatorProvider::PhoneE164Check.to_string(),
            "phone_e164_check"
        );
    }

    #[test]
    fn test_email_validator_timeout_zero() {
        let v = EmailFormatValidator::new();
        assert_eq!(v.timeout(), Duration::from_millis(0));
    }

    #[test]
    fn test_phone_validator_timeout_zero() {
        let v = PhoneE164Validator::new();
        assert_eq!(v.timeout(), Duration::from_millis(0));
    }
}
