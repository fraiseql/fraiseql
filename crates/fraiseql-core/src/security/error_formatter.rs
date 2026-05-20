//! Error Formatter
//!
//! This module provides error sanitization and formatting for different environments.
//! It controls what error details are exposed to clients based on deployment context.
//!
//! # Architecture
//!
//! The Error Formatter acts as the fifth and final layer in the security middleware:
//! ```text
//! GraphQL Error
//!     ↓
//! ErrorFormatter::format_error()
//!     ├─ Check 1: Determine detail level based on environment
//!     ├─ Check 2: Sanitize error message
//!     ├─ Check 3: Remove sensitive information
//!     └─ Check 4: Return formatted error
//!     ↓
//! Safe Error Message (suitable for client)
//! ```
//!
//! # Examples
//!
//! ```no_run
//! use fraiseql_core::security::{ErrorFormatter, DetailLevel};
//!
//! // Create formatter for production (minimal details)
//! let formatter = ErrorFormatter::new(DetailLevel::Production);
//!
//! // Format an error
//! let error_msg = "Database error: connection refused to postgresql://user:pass@db.local";
//! let formatted = formatter.format_error(error_msg);
//! println!("{}", formatted); // Shows only: "Internal server error"
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::security::errors::SecurityError;

/// Detail level for error responses
///
/// Controls how much information is exposed to clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DetailLevel {
    /// Development: Full error details, stack traces, database info
    Development,

    /// Staging: Limited error details, no sensitive information
    Staging,

    /// Production: Minimal error details, generic messages
    Production,
}

impl fmt::Display for DetailLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Development => write!(f, "Development"),
            Self::Staging => write!(f, "Staging"),
            Self::Production => write!(f, "Production"),
        }
    }
}

/// Sanitization configuration
///
/// Configures which sensitive patterns to hide in error messages.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // Reason: each bool controls an independent sanitization rule; bitflags would reduce readability
pub struct SanitizationConfig {
    /// Hide database connection strings
    pub hide_database_urls: bool,

    /// Hide SQL statements
    pub hide_sql: bool,

    /// Hide file system paths
    pub hide_paths: bool,

    /// Hide IP addresses
    pub hide_ips: bool,

    /// Hide email addresses
    pub hide_emails: bool,

    /// Hide API keys and credentials
    pub hide_credentials: bool,
}

impl SanitizationConfig {
    /// Create a permissive configuration (minimal sanitization)
    ///
    /// Used in development environments.
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            hide_database_urls: false,
            hide_sql: false,
            hide_paths: false,
            hide_ips: false,
            hide_emails: false,
            hide_credentials: false,
        }
    }

    /// Create a standard configuration (moderate sanitization)
    ///
    /// Used in staging environments.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            hide_database_urls: true,
            hide_sql: true,
            hide_paths: false,
            hide_ips: true,
            hide_emails: true,
            hide_credentials: true,
        }
    }

    /// Create a strict configuration (aggressive sanitization)
    ///
    /// Used in production environments.
    #[must_use]
    pub const fn strict() -> Self {
        Self {
            hide_database_urls: true,
            hide_sql: true,
            hide_paths: true,
            hide_ips: true,
            hide_emails: true,
            hide_credentials: true,
        }
    }
}

/// Error Formatter
///
/// Sanitizes and formats errors based on environment detail level.
/// Acts as the fifth layer in the security middleware pipeline.
#[derive(Debug, Clone)]
pub struct ErrorFormatter {
    detail_level: DetailLevel,
    config: SanitizationConfig,
}

impl ErrorFormatter {
    /// Create a new error formatter with a specific detail level
    #[must_use]
    pub const fn new(detail_level: DetailLevel) -> Self {
        let config = Self::config_for_level(detail_level);
        Self {
            detail_level,
            config,
        }
    }

    /// Create formatter with custom sanitization configuration
    #[must_use]
    pub const fn with_config(detail_level: DetailLevel, config: SanitizationConfig) -> Self {
        Self {
            detail_level,
            config,
        }
    }

    /// Create formatter for development (full details)
    #[must_use]
    pub const fn development() -> Self {
        Self::new(DetailLevel::Development)
    }

    /// Create formatter for staging (moderate details)
    #[must_use]
    pub const fn staging() -> Self {
        Self::new(DetailLevel::Staging)
    }

    /// Create formatter for production (minimal details)
    #[must_use]
    pub const fn production() -> Self {
        Self::new(DetailLevel::Production)
    }

    /// Get the sanitization configuration for a detail level
    const fn config_for_level(level: DetailLevel) -> SanitizationConfig {
        match level {
            DetailLevel::Development => SanitizationConfig::permissive(),
            DetailLevel::Staging => SanitizationConfig::standard(),
            DetailLevel::Production => SanitizationConfig::strict(),
        }
    }

    /// Format an error message for client consumption
    ///
    /// Performs 4-step sanitization:
    /// 1. Determine detail level
    /// 2. Sanitize message content
    /// 3. Remove sensitive information
    /// 4. Return formatted error
    #[must_use]
    pub fn format_error(&self, error_msg: &str) -> String {
        match self.detail_level {
            DetailLevel::Development => {
                // Development: return full error
                error_msg.to_string()
            },
            DetailLevel::Staging => {
                // Staging: sanitize but keep error type
                self.sanitize_error(error_msg)
            },
            DetailLevel::Production => {
                // Production: return generic error
                if Self::is_security_related(error_msg) {
                    "Security validation failed".to_string()
                } else {
                    "An error occurred while processing your request".to_string()
                }
            },
        }
    }

    /// Format a `SecurityError` for client consumption
    #[must_use]
    pub fn format_security_error(&self, error: &SecurityError) -> String {
        let error_msg = error.to_string();

        match self.detail_level {
            DetailLevel::Development => {
                // Development: full error message
                error_msg
            },
            DetailLevel::Staging => {
                // Staging: keep the error type but sanitize details
                self.extract_error_type_and_sanitize(&error_msg)
            },
            DetailLevel::Production => {
                // Production: generic message with error category
                match error {
                    SecurityError::AuthRequired => "Authentication required".to_string(),
                    SecurityError::InvalidToken
                    | SecurityError::TokenExpired { .. }
                    | SecurityError::TokenMissingClaim { .. }
                    | SecurityError::InvalidTokenAlgorithm { .. } => {
                        "Invalid authentication".to_string()
                    },
                    SecurityError::TlsRequired { .. }
                    | SecurityError::TlsVersionTooOld { .. }
                    | SecurityError::MtlsRequired { .. }
                    | SecurityError::InvalidClientCert { .. } => {
                        "Connection security validation failed".to_string()
                    },
                    SecurityError::QueryTooDeep { .. }
                    | SecurityError::QueryTooComplex { .. }
                    | SecurityError::QueryTooLarge { .. } => "Query validation failed".to_string(),
                    SecurityError::IntrospectionDisabled { .. } => {
                        "Schema introspection is not available".to_string()
                    },
                    _ => "An error occurred while processing your request".to_string(),
                }
            },
        }
    }

    /// Sanitize an error message by removing sensitive information
    fn sanitize_error(&self, error_msg: &str) -> String {
        let mut result = error_msg.to_string();

        // Sanitize database URLs (postgresql://user:pass@host)
        if self.config.hide_database_urls {
            result = Self::hide_pattern(&result, "postgresql://", "**hidden**");
            result = Self::hide_pattern(&result, "mysql://", "**hidden**");
            result = Self::hide_pattern(&result, "mongodb://", "**hidden**");
        }

        // Sanitize SQL statements
        if self.config.hide_sql {
            result = Self::hide_pattern(&result, "SELECT ", "[SQL hidden]");
            result = Self::hide_pattern(&result, "INSERT ", "[SQL hidden]");
            result = Self::hide_pattern(&result, "UPDATE ", "[SQL hidden]");
            result = Self::hide_pattern(&result, "DELETE ", "[SQL hidden]");
        }

        // Sanitize file paths
        if self.config.hide_paths {
            result = Self::redact_paths(&result);
        }

        // Sanitize IP addresses
        if self.config.hide_ips {
            result = Self::redact_ips(&result);
        }

        // Sanitize email addresses
        if self.config.hide_emails {
            result = Self::redact_emails(&result);
        }

        // Sanitize credentials
        if self.config.hide_credentials {
            result = Self::hide_pattern(&result, "@", "[credentials redacted]");
        }

        result
    }

    /// Check if an error is security-related
    fn is_security_related(error_msg: &str) -> bool {
        let lower = error_msg.to_lowercase();
        lower.contains("auth")
            || lower.contains("permission")
            || lower.contains("forbidden")
            || lower.contains("security")
            || lower.contains("tls")
            || lower.contains("https")
    }

    /// Extract error type and sanitize details
    fn extract_error_type_and_sanitize(&self, error_msg: &str) -> String {
        let sanitized = self.sanitize_error(error_msg);

        // Keep the first 100 characters if error is short, or first meaningful part
        if sanitized.len() > 100 {
            format!("{}...", &sanitized[..100])
        } else {
            sanitized
        }
    }

    /// Hide a pattern in a string by replacing it
    fn hide_pattern(text: &str, pattern: &str, replacement: &str) -> String {
        if text.contains(pattern) {
            text.replace(pattern, replacement)
        } else {
            text.to_string()
        }
    }

    /// Redact file paths from error messages
    fn redact_paths(text: &str) -> String {
        // Simple pattern: /path/to/file or C:\path\to\file
        let mut result = text.to_string();

        // Match paths with / (Unix-style)
        if result.contains('/') && result.contains(".rs") {
            result = result.replace('/', "*");
        }

        // Match paths with \ (Windows-style)
        if result.contains('\\') {
            result = result.replace('\\', "*");
        }

        result
    }

    /// Redact IP addresses from error messages
    fn redact_ips(text: &str) -> String {
        // Simple pattern detection for IPv4 addresses (x.x.x.x)
        let mut result = String::new();
        let mut current_word = String::new();

        for c in text.chars() {
            if c.is_numeric() || c == '.' {
                current_word.push(c);
            } else {
                // Check if accumulated word looks like an IP
                if Self::looks_like_ip(&current_word) {
                    result.push_str("[IP]");
                } else {
                    result.push_str(&current_word);
                }
                current_word.clear();
                result.push(c);
            }
        }

        // Handle last word
        if Self::looks_like_ip(&current_word) {
            result.push_str("[IP]");
        } else {
            result.push_str(&current_word);
        }

        result
    }

    /// Redact email addresses from error messages
    fn redact_emails(text: &str) -> String {
        // Simple pattern: anything@domain.com
        let mut result = String::new();
        let mut in_email = false;
        let mut email = String::new();

        for c in text.chars() {
            if c == '@' {
                in_email = true;
                email.clear();
                email.push(c);
            } else if in_email {
                email.push(c);
                if c == ' ' || c == '\n' {
                    result.push_str("[email]");
                    result.push(c);
                    in_email = false;
                    email.clear();
                }
            } else {
                result.push(c);
            }
        }

        // Handle email at end of string
        if in_email && email.contains('@') {
            result.push_str("[email]");
        } else {
            result.push_str(&email);
        }

        result
    }

    /// Check if a string looks like an `IPv4` address
    fn looks_like_ip(s: &str) -> bool {
        if !s.contains('.') {
            return false;
        }

        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return false;
        }

        parts.iter().all(|p| {
            !p.is_empty()
                && p.chars().all(|c| c.is_ascii_digit())
                && p.parse::<u32>().unwrap_or(256) <= 255
        })
    }

    /// Get the current detail level
    #[must_use]
    pub const fn detail_level(&self) -> DetailLevel {
        self.detail_level
    }

    /// Get the sanitization configuration
    #[must_use]
    pub const fn config(&self) -> &SanitizationConfig {
        &self.config
    }
}
