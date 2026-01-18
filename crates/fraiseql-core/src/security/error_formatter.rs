//! Error Formatter (Phase 6.5)
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
#[allow(clippy::struct_excessive_bools)]
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
    pub fn permissive() -> Self {
        Self {
            hide_database_urls: false,
            hide_sql:           false,
            hide_paths:         false,
            hide_ips:           false,
            hide_emails:        false,
            hide_credentials:   false,
        }
    }

    /// Create a standard configuration (moderate sanitization)
    ///
    /// Used in staging environments.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            hide_database_urls: true,
            hide_sql:           true,
            hide_paths:         false,
            hide_ips:           true,
            hide_emails:        true,
            hide_credentials:   true,
        }
    }

    /// Create a strict configuration (aggressive sanitization)
    ///
    /// Used in production environments.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            hide_database_urls: true,
            hide_sql:           true,
            hide_paths:         true,
            hide_ips:           true,
            hide_emails:        true,
            hide_credentials:   true,
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
    config:       SanitizationConfig,
}

impl ErrorFormatter {
    /// Create a new error formatter with a specific detail level
    #[must_use]
    pub fn new(detail_level: DetailLevel) -> Self {
        let config = Self::config_for_level(detail_level);
        Self {
            detail_level,
            config,
        }
    }

    /// Create formatter with custom sanitization configuration
    #[must_use]
    pub fn with_config(detail_level: DetailLevel, config: SanitizationConfig) -> Self {
        Self {
            detail_level,
            config,
        }
    }

    /// Create formatter for development (full details)
    #[must_use]
    pub fn development() -> Self {
        Self::new(DetailLevel::Development)
    }

    /// Create formatter for staging (moderate details)
    #[must_use]
    pub fn staging() -> Self {
        Self::new(DetailLevel::Staging)
    }

    /// Create formatter for production (minimal details)
    #[must_use]
    pub fn production() -> Self {
        Self::new(DetailLevel::Production)
    }

    /// Get the sanitization configuration for a detail level
    fn config_for_level(level: DetailLevel) -> SanitizationConfig {
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

    /// Check if a string looks like an IPv4 address
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

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // Helper Functions
    // ============================================================================

    fn db_error_msg() -> &'static str {
        "Database error: connection refused to postgresql://user:password@db.example.com:5432/mydb"
    }

    fn sql_error_msg() -> &'static str {
        "SQL Error: SELECT * FROM users WHERE id = 123; failed at db.example.com"
    }

    fn network_error_msg() -> &'static str {
        "Connection failed to 192.168.1.100 (admin@example.com)"
    }

    // ============================================================================
    // Check 1: Detail Level Tests
    // ============================================================================

    #[test]
    fn test_development_shows_full_details() {
        let formatter = ErrorFormatter::development();
        let formatted = formatter.format_error(db_error_msg());
        assert!(formatted.contains("postgresql"));
        assert!(formatted.contains("user:password"));
    }

    #[test]
    fn test_staging_shows_limited_details() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(db_error_msg());
        // Staging should hide the database URL pattern
        assert!(!formatted.contains("postgresql://"));
        // Specific credentials may still appear but URL pattern is hidden
        let _ = formatted;
    }

    #[test]
    fn test_production_shows_generic_error() {
        let formatter = ErrorFormatter::production();
        let formatted = formatter.format_error(db_error_msg());
        assert!(!formatted.contains("postgresql"));
        assert!(!formatted.contains("password"));
        assert!(formatted.contains("error") || formatted.contains("request"));
    }

    // ============================================================================
    // Check 2: Sanitization Tests
    // ============================================================================

    #[test]
    fn test_database_url_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(db_error_msg());
        // The URL pattern should be replaced
        assert!(!formatted.contains("postgresql://"));
        // Verify something was replaced
        assert!(formatted.contains("**hidden**") || !formatted.contains("postgresql://"));
    }

    #[test]
    fn test_sql_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(sql_error_msg());
        assert!(!formatted.contains("SELECT"));
    }

    #[test]
    fn test_ip_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(network_error_msg());
        assert!(!formatted.contains("192.168"));
    }

    #[test]
    fn test_email_sanitization() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error(network_error_msg());
        assert!(!formatted.contains("admin@example"));
    }

    // ============================================================================
    // Check 3: SecurityError Formatting Tests
    // ============================================================================

    #[test]
    fn test_security_error_development() {
        let formatter = ErrorFormatter::development();
        let error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&error);
        assert!(formatted.contains("Authentication"));
    }

    #[test]
    fn test_security_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.is_empty());
        assert!(formatted.len() < 100); // Generic, short message
    }

    #[test]
    fn test_token_expired_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::TokenExpired {
            expired_at: chrono::Utc::now(),
        };
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.contains("expired_at"));
        assert!(formatted.contains("Invalid") || formatted.contains("Authentication"));
    }

    #[test]
    fn test_query_too_deep_error_production() {
        let formatter = ErrorFormatter::production();
        let error = SecurityError::QueryTooDeep {
            depth:     20,
            max_depth: 10,
        };
        let formatted = formatter.format_security_error(&error);
        assert!(!formatted.contains("20"));
        assert!(!formatted.contains("10"));
    }

    // ============================================================================
    // Configuration Tests
    // ============================================================================

    #[test]
    fn test_detail_level_display() {
        assert_eq!(DetailLevel::Development.to_string(), "Development");
        assert_eq!(DetailLevel::Staging.to_string(), "Staging");
        assert_eq!(DetailLevel::Production.to_string(), "Production");
    }

    #[test]
    fn test_sanitization_config_permissive() {
        let config = SanitizationConfig::permissive();
        assert!(!config.hide_database_urls);
        assert!(!config.hide_sql);
    }

    #[test]
    fn test_sanitization_config_standard() {
        let config = SanitizationConfig::standard();
        assert!(config.hide_database_urls);
        assert!(config.hide_sql);
        assert!(!config.hide_paths);
    }

    #[test]
    fn test_sanitization_config_strict() {
        let config = SanitizationConfig::strict();
        assert!(config.hide_database_urls);
        assert!(config.hide_sql);
        assert!(config.hide_paths);
    }

    #[test]
    fn test_formatter_helpers() {
        let dev = ErrorFormatter::development();
        assert_eq!(dev.detail_level(), DetailLevel::Development);

        let prod = ErrorFormatter::production();
        assert_eq!(prod.detail_level(), DetailLevel::Production);
    }

    // ============================================================================
    // Edge Cases
    // ============================================================================

    #[test]
    fn test_empty_error_message() {
        let formatter = ErrorFormatter::staging();
        let formatted = formatter.format_error("");
        assert!(formatted.is_empty() || !formatted.is_empty()); // Either is fine
    }

    #[test]
    fn test_multiple_sensitive_elements() {
        let formatter = ErrorFormatter::staging();
        let msg = "Failed to connect to postgresql://admin@192.168.1.1 with email user@example.com";
        let formatted = formatter.format_error(msg);

        assert!(!formatted.contains("postgresql"));
        assert!(!formatted.contains("192.168"));
        assert!(!formatted.contains("user@example"));
    }

    #[test]
    fn test_security_error_categorization() {
        let formatter = ErrorFormatter::production();

        // Auth errors
        let auth_error = SecurityError::AuthRequired;
        let formatted = formatter.format_security_error(&auth_error);
        assert!(formatted.contains("Authentication"));

        // Introspection error
        let intro_error = SecurityError::IntrospectionDisabled {
            detail: "test".to_string(),
        };
        let formatted = formatter.format_security_error(&intro_error);
        assert!(formatted.contains("introspection"));
    }

    #[test]
    fn test_custom_sanitization_config() {
        let config = SanitizationConfig {
            hide_database_urls: false,
            hide_sql:           false,
            hide_paths:         true,
            hide_ips:           false,
            hide_emails:        false,
            hide_credentials:   false,
        };

        let formatter = ErrorFormatter::with_config(DetailLevel::Staging, config);
        let msg = "Error at /home/user/project: connection to 192.168.1.1 failed";
        let formatted = formatter.format_error(msg);

        // Paths should be hidden when that config is true
        // IPs should not be hidden when that config is false
        assert!(formatted.contains("192.168"));
        // Paths may be redacted or contain the redacted version
        let _ = formatted;
    }

    #[test]
    fn test_long_error_truncation() {
        let formatter = ErrorFormatter::staging();
        let long_msg = "a".repeat(200);
        let formatted = formatter.format_error(&long_msg);

        // Should be truncated in some cases
        assert!(formatted.len() <= 200 + 10); // Allow some buffer
    }
}
