//! Async validation framework for validators requiring runtime operations.
//!
//! This module provides traits and helpers for validators that need to perform
//! asynchronous operations like network requests or database lookups.

use std::time::Duration;

use crate::error::{FraiseQLError, Result};

/// Async validator result type.
pub type AsyncValidatorResult = Result<()>;

/// Provider types for async validators.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum AsyncValidatorProvider {
    /// Email domain MX record verification
    EmailDomainCheck,
    /// Phone number validation via provider (e.g., Twilio)
    PhoneNumberValidation,
    /// IBAN/VIN checksum validation
    ChecksumValidation,
    /// Custom provider
    Custom(String),
}

impl AsyncValidatorProvider {
    /// Get provider name for logging/debugging
    pub fn name(&self) -> String {
        match self {
            Self::EmailDomainCheck => "email_domain_check".to_string(),
            Self::PhoneNumberValidation => "phone_validation".to_string(),
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
    /// Ok(()) if valid, Err(FraiseQLError) if invalid
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult;

    /// Get the provider this validator uses
    fn provider(&self) -> AsyncValidatorProvider;

    /// Get the timeout for this validator
    fn timeout(&self) -> Duration;
}

/// Mock email domain validator for testing.
///
/// In production, this would perform actual MX record lookups.
pub struct MockEmailDomainValidator {
    config:        AsyncValidatorConfig,
    /// List of valid domains (for testing)
    valid_domains: Vec<String>,
}

impl MockEmailDomainValidator {
    /// Create a new mock email domain validator.
    pub fn new(timeout_ms: u64) -> Self {
        let config =
            AsyncValidatorConfig::new(AsyncValidatorProvider::EmailDomainCheck, timeout_ms);
        Self {
            config,
            valid_domains: vec![
                "gmail.com".to_string(),
                "yahoo.com".to_string(),
                "outlook.com".to_string(),
                "example.com".to_string(),
            ],
        }
    }

    /// Add a valid domain for testing.
    pub fn add_valid_domain(&mut self, domain: impl Into<String>) {
        self.valid_domains.push(domain.into());
    }
}

#[async_trait::async_trait]
impl AsyncValidator for MockEmailDomainValidator {
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult {
        // Extract domain from email
        if let Some(at_index) = value.find('@') {
            let domain = &value[at_index + 1..];

            // Simulate async operation with small delay
            tokio::time::sleep(Duration::from_millis(10)).await;

            if self.valid_domains.iter().any(|d| d.eq_ignore_ascii_case(domain)) {
                Ok(())
            } else {
                Err(FraiseQLError::Validation {
                    message: format!(
                        "Email domain validation failed: {} (domain {} not found)",
                        field, domain
                    ),
                    path:    Some(field.to_string()),
                })
            }
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Email domain validation failed: {} (invalid email format)",
                    field
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

/// Mock phone number validator for testing.
///
/// In production, this would integrate with Twilio or similar service.
pub struct MockPhoneNumberValidator {
    config:          AsyncValidatorConfig,
    /// List of valid country codes (for testing)
    valid_countries: Vec<String>,
}

impl MockPhoneNumberValidator {
    /// Create a new mock phone number validator.
    pub fn new(timeout_ms: u64) -> Self {
        let config =
            AsyncValidatorConfig::new(AsyncValidatorProvider::PhoneNumberValidation, timeout_ms);
        Self {
            config,
            valid_countries: vec![
                "1".to_string(),  // US
                "44".to_string(), // UK
                "33".to_string(), // France
                "49".to_string(), // Germany
            ],
        }
    }

    /// Add a valid country code for testing.
    pub fn add_valid_country(&mut self, code: impl Into<String>) {
        self.valid_countries.push(code.into());
    }
}

#[async_trait::async_trait]
impl AsyncValidator for MockPhoneNumberValidator {
    async fn validate_async(&self, value: &str, field: &str) -> AsyncValidatorResult {
        let phone_clean = value.trim_start_matches('+');

        // Simulate async operation
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Check if starts with valid country code
        let is_valid = self.valid_countries.iter().any(|cc| phone_clean.starts_with(cc));

        if is_valid && phone_clean.len() >= 10 {
            Ok(())
        } else {
            Err(FraiseQLError::Validation {
                message: format!(
                    "Phone number validation failed: {} (invalid phone number)",
                    field
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

    #[tokio::test]
    async fn test_mock_email_domain_valid() {
        let validator = MockEmailDomainValidator::new(5000);
        let result = validator.validate_async("user@gmail.com", "email").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_email_domain_invalid() {
        let validator = MockEmailDomainValidator::new(5000);
        let result = validator.validate_async("user@invalid-domain.com", "email").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_email_domain_no_at() {
        let validator = MockEmailDomainValidator::new(5000);
        let result = validator.validate_async("invalid-email", "email").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_phone_valid() {
        let validator = MockPhoneNumberValidator::new(5000);
        let result = validator.validate_async("+14155552671", "phone").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_phone_invalid_country() {
        let validator = MockPhoneNumberValidator::new(5000);
        let result = validator.validate_async("+999999999999", "phone").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_phone_too_short() {
        let validator = MockPhoneNumberValidator::new(5000);
        let result = validator.validate_async("+123", "phone").await;
        assert!(result.is_err());
    }

    #[test]
    fn test_async_validator_config() {
        let config = AsyncValidatorConfig::new(AsyncValidatorProvider::EmailDomainCheck, 5000)
            .with_cache_ttl(3600)
            .with_field_pattern("*.email");

        assert_eq!(config.provider, AsyncValidatorProvider::EmailDomainCheck);
        assert_eq!(config.timeout, Duration::from_millis(5000));
        assert_eq!(config.cache_ttl_secs, 3600);
        assert_eq!(config.field_pattern, "*.email");
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(AsyncValidatorProvider::EmailDomainCheck.to_string(), "email_domain_check");
        assert_eq!(AsyncValidatorProvider::PhoneNumberValidation.to_string(), "phone_validation");
    }

    #[tokio::test]
    async fn test_timeout_duration() {
        let validator = MockEmailDomainValidator::new(2000);
        assert_eq!(validator.timeout(), Duration::from_millis(2000));
    }
}
