//! Sensitive field masking for compliance profiles
//!
//! This module handles masking sensitive data in GraphQL responses for REGULATED profiles.
//! Field sensitivity is determined by field name patterns and explicit marking.
//!
//! ## Field Sensitivity Levels
//!
//! - **Public**: No masking (e.g., id, name, title)
//! - **Sensitive**: Partial masking - show first char + *** (e.g., email, phone)
//! - **PII**: Heavy masking - type + **** (e.g., `ssn`, `credit_card`)
//! - **Secret**: Always masked - **** (e.g., `password`, `api_key`)
//!
//! ## Pattern Matching
//!
//! Fields are classified based on name patterns:
//! - `password*`, `secret*`, `token*`, `key*` → Secret
//! - `ssn`, `credit_card`, `cvv`, `pin` → PII
//! - `email`, `phone`, `mobile`, `telephone` → Sensitive
//! - Everything else → Public
//!
//! ## Usage
//!
//! ```
//! use fraiseql_core::security::{FieldMasker, FieldSensitivity};
//!
//! let sensitivity = FieldMasker::detect_sensitivity("email");
//! assert_eq!(sensitivity, FieldSensitivity::Sensitive);
//!
//! let masked = FieldMasker::mask_value("user@example.com", sensitivity);
//! assert_eq!(masked, "u***");
//! ```

use std::fmt;

use crate::security::SecurityProfile;

/// Field sensitivity classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FieldSensitivity {
    /// Public field - no masking
    Public,
    /// Sensitive field - partial masking
    Sensitive,
    /// Personally Identifiable Information - heavy masking
    PII,
    /// Secret field - always masked
    Secret,
}

impl fmt::Display for FieldSensitivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Sensitive => write!(f, "sensitive"),
            Self::PII => write!(f, "pii"),
            Self::Secret => write!(f, "secret"),
        }
    }
}

/// Field masking rules and patterns
#[derive(Debug)]
pub struct FieldMasker;

impl FieldMasker {
    /// Detect field sensitivity based on name patterns
    #[must_use]
    pub fn detect_sensitivity(field_name: &str) -> FieldSensitivity {
        let lower = field_name.to_lowercase();

        // Secret fields - cryptographic material and credentials
        if lower.starts_with("password")
            || lower.starts_with("secret")
            || lower.starts_with("token")
            || lower.contains("token")  // Also catch refresh_token, bearer_token, etc.
            || lower.starts_with("key")
            || lower.starts_with("api_key")
            || lower.starts_with("api_secret")
            || lower.starts_with("auth")
            || lower.starts_with("oauth")
            || lower == "hash"
            || lower == "signature"
            || lower.contains("webhook_secret")
            || lower.contains("private_key")
            || lower.contains("certificate")
            || lower.contains("tls_secret")
            || lower.contains("encryption_key")
            || lower.contains("database_url")
            || lower.contains("connection_string")
            || lower.contains("access_token")
            // JWT and OAuth credentials
            || lower == "jwt"
            || lower.starts_with("jwt_")
            || lower.ends_with("_jwt")
            || lower == "nonce"
            || lower.starts_with("nonce_")
            || lower.ends_with("_nonce")
            || lower == "bearer"
            || lower.starts_with("bearer_")
            || lower == "client_secret"
            || lower.contains("client_secret")
        {
            return FieldSensitivity::Secret;
        }

        // PII fields - personally identifiable and financial information
        if lower == "ssn"
            || lower == "social_security_number"
            || lower.contains("credit_card")
            || lower.contains("card_number")
            || lower == "cvv"
            || lower == "cvc"
            || lower.contains("bank_account")
            || lower == "pin"
            || lower.contains("driver_license")
            || lower.contains("driver's_license")
            || lower.contains("passport")
            || lower == "date_of_birth"
            || lower == "dob"
            || lower.contains("maiden_name")
            || lower.contains("mother's_name")
            || lower.contains("routing_number")
            || lower.contains("swift_code")
            || lower == "iban"
            || lower.contains("health_record")
            || lower.contains("medical_record")
            || lower.contains("state_id")
            || lower.contains("drivers_license_number")
        {
            return FieldSensitivity::PII;
        }

        // Sensitive fields - personal contact and identification info
        if lower == "email"
            || lower.starts_with("email_")
            || lower.ends_with("_email")
            || lower == "phone"
            || lower == "phone_number"
            || lower.starts_with("phone_")
            || lower == "mobile"
            || lower.starts_with("mobile_")
            || lower == "telephone"
            || lower == "fax"
            || lower.contains("ip_address")
            || lower.contains("ipaddress")
            || lower == "mac_address"
            || lower == "macaddress"
            || lower == "username"
            || lower.starts_with("username_")
            || lower.contains("login_name")
            || lower.contains("im_handle")
            || lower.contains("slack_id")
            || lower.contains("twitter_handle")
            || lower.contains("billing_address")
            || lower.contains("shipping_address")
            || lower.contains("home_address")
            || lower.contains("work_address")
            || lower.contains("zip_code")
            || lower.contains("postal_code")
            || lower.contains("ssn_last_four")
        {
            return FieldSensitivity::Sensitive;
        }

        // Default: public
        FieldSensitivity::Public
    }

    /// Mask a string value based on sensitivity level
    #[must_use]
    pub fn mask_value(value: &str, sensitivity: FieldSensitivity) -> String {
        match sensitivity {
            FieldSensitivity::Public => value.to_string(),
            FieldSensitivity::Sensitive => Self::mask_sensitive(value),
            FieldSensitivity::PII => Self::mask_pii(value),
            FieldSensitivity::Secret => Self::mask_secret(value),
        }
    }

    /// Mask sensitive value - show first char + ***
    fn mask_sensitive(value: &str) -> String {
        if value.is_empty() {
            "***".to_string()
        } else {
            let first_char = value.chars().next().unwrap_or('*');
            format!("{first_char}***")
        }
    }

    /// Mask PII - show only type + ****
    fn mask_pii(_value: &str) -> String {
        "[PII]".to_string()
    }

    /// Mask secret - always ****
    fn mask_secret(_value: &str) -> String {
        "****".to_string()
    }

    /// Determine if value should be masked for this profile
    #[must_use]
    pub fn should_mask(sensitivity: FieldSensitivity, profile: &SecurityProfile) -> bool {
        match profile {
            SecurityProfile::Standard => false,
            SecurityProfile::Regulated => sensitivity != FieldSensitivity::Public,
        }
    }
}
