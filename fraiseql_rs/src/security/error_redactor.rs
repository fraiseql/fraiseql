//! Error detail redaction for compliance profiles
//!
//! This module handles redacting sensitive error information for REGULATED profiles
//! while keeping detailed errors for STANDARD profiles.
//!
//! ## Redaction Strategy
//!
//! ### STANDARD Profile
//! - Returns full error messages with all details
//! - Helpful for debugging
//! - Safe for internal/trusted clients
//!
//! ### REGULATED Profile
//! - Removes internal implementation details
//! - Hides database-specific error messages
//! - Removes stack traces and backtraces
//! - Maps errors to generic messages
//! - Prevents information disclosure attacks
//!
//! ## Usage
//!
//! ```ignore
//! use fraiseql_rs::security::{SecurityProfile, error_redactor::ErrorRedactor};
//!
//! let error = GraphQLError::new("Database connection failed: user not found");
//! let profile = SecurityProfile::regulated();
//!
//! let redacted = ErrorRedactor::redact(&error, &profile);
//! // Output: "Query execution failed" (details removed)
//! ```

use crate::security::SecurityProfile;
use std::fmt;

/// GraphQL error representation
#[derive(Debug, Clone)]
pub struct GraphQLError {
    /// Error message
    pub message: String,
    /// Error code/category
    pub code: Option<String>,
    /// Additional extensions
    pub extensions: Option<serde_json::Value>,
}

impl GraphQLError {
    /// Create a new GraphQL error
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: None,
            extensions: None,
        }
    }

    /// Set error code
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// Set extensions
    pub fn with_extensions(mut self, extensions: serde_json::Value) -> Self {
        self.extensions = Some(extensions);
        self
    }
}

impl fmt::Display for GraphQLError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// Error redaction levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionLevel {
    /// No redaction (STANDARD profile)
    None,
    /// Partial redaction (hide implementation details)
    Partial,
    /// Full redaction (only error category visible)
    Full,
}

/// Error redactor for compliance profiles
#[derive(Debug)]
pub struct ErrorRedactor;

impl ErrorRedactor {
    /// Redact error details based on security profile
    ///
    /// # Arguments
    /// - `error`: The GraphQL error to redact
    /// - `profile`: The security profile (determines redaction level)
    ///
    /// # Returns
    /// Redacted error (unchanged for STANDARD, redacted for REGULATED)
    pub fn redact(error: &GraphQLError, profile: &SecurityProfile) -> GraphQLError {
        match profile {
            SecurityProfile::Standard => {
                // Standard: return full error
                error.clone()
            }
            SecurityProfile::Regulated => {
                // Regulated: redact sensitive details
                Self::redact_for_compliance(error)
            }
        }
    }

    /// Determine redaction level based on error category
    fn get_redaction_level(error_msg: &str) -> RedactionLevel {
        if error_msg.contains("database")
            || error_msg.contains("connection")
            || error_msg.contains("sql")
        {
            RedactionLevel::Full
        } else if error_msg.contains("internal")
            || error_msg.contains("stack")
            || error_msg.contains("backtrace")
        {
            RedactionLevel::Full
        } else if error_msg.contains("syntax") || error_msg.contains("parse") {
            RedactionLevel::Partial
        } else if error_msg.contains("permission") || error_msg.contains("unauthorized") {
            RedactionLevel::Partial
        } else {
            RedactionLevel::None
        }
    }

    /// Redact error for REGULATED profile
    fn redact_for_compliance(error: &GraphQLError) -> GraphQLError {
        let redaction_level = Self::get_redaction_level(&error.message);

        let redacted_message = match redaction_level {
            RedactionLevel::None => error.message.clone(),
            RedactionLevel::Partial => Self::partially_redact_message(&error.message),
            RedactionLevel::Full => Self::fully_redact_message(&error.message),
        };

        let mut redacted = GraphQLError::new(redacted_message);
        redacted.code = error.code.clone();

        // Remove sensitive extensions
        if let Some(extensions) = &error.extensions {
            let mut safe_ext = extensions.clone();
            // Remove traces, backtraces, stack info
            if let Some(obj) = safe_ext.as_object_mut() {
                obj.remove("trace");
                obj.remove("backtrace");
                obj.remove("stack");
                obj.remove("stackTrace");
                obj.remove("internal");
                obj.remove("debug");
            }
            if !safe_ext.is_null() {
                redacted.extensions = Some(safe_ext);
            }
        }

        redacted
    }

    /// Partially redact message (keep category, remove details)
    fn partially_redact_message(msg: &str) -> String {
        if msg.contains("syntax") || msg.contains("parse") {
            "Invalid query syntax".to_string()
        } else if msg.contains("permission") || msg.contains("unauthorized") {
            "Access denied".to_string()
        } else if msg.contains("validation") {
            "Input validation failed".to_string()
        } else if msg.contains("not found") {
            "Resource not found".to_string()
        } else {
            "Query processing failed".to_string()
        }
    }

    /// Fully redact message (hide all details)
    fn fully_redact_message(msg: &str) -> String {
        if msg.contains("database") || msg.contains("connection") || msg.contains("sql") {
            "Query execution failed".to_string()
        } else if msg.contains("internal") || msg.contains("server") {
            "Internal server error".to_string()
        } else {
            "An error occurred".to_string()
        }
    }

    /// Check if error is sensitive (should be redacted)
    pub fn is_sensitive_error(error: &GraphQLError) -> bool {
        let msg = &error.message;
        msg.contains("database")
            || msg.contains("connection")
            || msg.contains("sql")
            || msg.contains("internal")
            || msg.contains("stack")
            || msg.contains("backtrace")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Test Suite 1: Error Creation
    // ========================================================================

    #[test]
    fn test_create_simple_error() {
        let error = GraphQLError::new("Test error message");
        assert_eq!(error.message, "Test error message");
        assert!(error.code.is_none());
        assert!(error.extensions.is_none());
    }

    #[test]
    fn test_create_error_with_code() {
        let error = GraphQLError::new("Test error").with_code("ERR_001");
        assert_eq!(error.code, Some("ERR_001".to_string()));
    }

    #[test]
    fn test_create_error_with_extensions() {
        let ext = serde_json::json!({ "detail": "extra info" });
        let error = GraphQLError::new("Test").with_extensions(ext.clone());
        assert_eq!(error.extensions, Some(ext));
    }

    #[test]
    fn test_error_display() {
        let error = GraphQLError::new("Test message");
        assert_eq!(error.to_string(), "Test message");
    }

    // ========================================================================
    // Test Suite 2: Standard Profile (No Redaction)
    // ========================================================================

    #[test]
    fn test_standard_profile_keeps_full_error() {
        let error = GraphQLError::new("Database connection failed: timeout");
        let profile = SecurityProfile::standard();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, error.message);
    }

    #[test]
    fn test_standard_profile_keeps_code() {
        let error = GraphQLError::new("Error").with_code("ERR_DB");
        let profile = SecurityProfile::standard();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.code, Some("ERR_DB".to_string()));
    }

    #[test]
    fn test_standard_profile_keeps_extensions() {
        let ext = serde_json::json!({ "trace": "...", "debug": "..." });
        let error = GraphQLError::new("Error").with_extensions(ext.clone());
        let profile = SecurityProfile::standard();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.extensions, Some(ext));
    }

    // ========================================================================
    // Test Suite 3: Regulated Profile - Full Redaction
    // ========================================================================

    #[test]
    fn test_regulated_redacts_database_error() {
        let error = GraphQLError::new("Database connection failed: authentication error");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Query execution failed");
        assert!(redacted.message != error.message);
    }

    #[test]
    fn test_regulated_redacts_connection_error() {
        let error = GraphQLError::new("Failed to connect to database server");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Query execution failed");
    }

    #[test]
    fn test_regulated_redacts_sql_error() {
        let error = GraphQLError::new("SQL syntax error: invalid column reference");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Query execution failed");
    }

    #[test]
    fn test_regulated_redacts_internal_error() {
        let error = GraphQLError::new("Internal server error in process_query");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Internal server error");
    }

    // ========================================================================
    // Test Suite 4: Regulated Profile - Partial Redaction
    // ========================================================================

    #[test]
    fn test_regulated_partially_redacts_syntax_error() {
        let error = GraphQLError::new("Syntax error in query: unexpected token");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Invalid query syntax");
        assert!(redacted.message != error.message);
    }

    #[test]
    fn test_regulated_partially_redacts_parse_error() {
        let error = GraphQLError::new("Parse error: malformed graphql");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Invalid query syntax");
    }

    #[test]
    fn test_regulated_partially_redacts_permission_error() {
        let error = GraphQLError::new("Permission denied: user lacks admin role");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Access denied");
    }

    #[test]
    fn test_regulated_partially_redacts_unauthorized_error() {
        let error = GraphQLError::new("Unauthorized access attempt for sensitive field");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Access denied");
    }

    #[test]
    fn test_regulated_partially_redacts_validation_error() {
        let error = GraphQLError::new("Validation error: input exceeds max length");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Input validation failed");
    }

    // ========================================================================
    // Test Suite 5: Extension Cleaning
    // ========================================================================

    #[test]
    fn test_regulated_removes_trace_extension() {
        let ext = serde_json::json!({
            "trace": "full stack trace here",
            "safe_field": "this should remain"
        });
        let error = GraphQLError::new("Error").with_extensions(ext);
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);

        if let Some(exts) = &redacted.extensions {
            assert!(!exts.as_object().unwrap().contains_key("trace"));
            assert!(exts.as_object().unwrap().contains_key("safe_field"));
        }
    }

    #[test]
    fn test_regulated_removes_backtrace_extension() {
        let ext = serde_json::json!({
            "backtrace": "stack backtrace"
        });
        let error = GraphQLError::new("Error").with_extensions(ext);
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);

        if let Some(exts) = &redacted.extensions {
            assert!(!exts.as_object().unwrap().contains_key("backtrace"));
        }
    }

    #[test]
    fn test_regulated_removes_internal_extension() {
        let ext = serde_json::json!({
            "internal": "debug info",
            "debug": "debug details"
        });
        let error = GraphQLError::new("Error").with_extensions(ext);
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);

        if let Some(exts) = &redacted.extensions {
            assert!(!exts.as_object().unwrap().contains_key("internal"));
            assert!(!exts.as_object().unwrap().contains_key("debug"));
        }
    }

    // ========================================================================
    // Test Suite 6: Code Preservation
    // ========================================================================

    #[test]
    fn test_regulated_preserves_error_code() {
        let error = GraphQLError::new("Database error").with_code("ERR_DB_001");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.code, error.code);
    }

    #[test]
    fn test_regulated_preserves_none_code() {
        let error = GraphQLError::new("Some error");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert!(redacted.code.is_none());
    }

    // ========================================================================
    // Test Suite 7: Sensitive Error Detection
    // ========================================================================

    #[test]
    fn test_detect_database_error_as_sensitive() {
        let error = GraphQLError::new("Database error: connection failed");
        assert!(ErrorRedactor::is_sensitive_error(&error));
    }

    #[test]
    fn test_detect_sql_error_as_sensitive() {
        let error = GraphQLError::new("SQL syntax error");
        assert!(ErrorRedactor::is_sensitive_error(&error));
    }

    #[test]
    fn test_detect_internal_error_as_sensitive() {
        let error = GraphQLError::new("Internal error in module X");
        assert!(ErrorRedactor::is_sensitive_error(&error));
    }

    #[test]
    fn test_detect_stack_error_as_sensitive() {
        let error = GraphQLError::new("stack trace information");
        assert!(ErrorRedactor::is_sensitive_error(&error));
    }

    #[test]
    fn test_detect_generic_error_not_sensitive() {
        let error = GraphQLError::new("Invalid input provided");
        assert!(!ErrorRedactor::is_sensitive_error(&error));
    }

    // ========================================================================
    // Test Suite 8: Edge Cases
    // ========================================================================

    #[test]
    fn test_redact_empty_error_message() {
        let error = GraphQLError::new("");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert_eq!(redacted.message, "Query processing failed");
    }

    #[test]
    fn test_redact_very_long_message() {
        let long_msg = "x".repeat(10000);
        let error = GraphQLError::new(long_msg);
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert!(redacted.message.len() < 100); // Should be much shorter
    }

    #[test]
    fn test_redact_preserves_error_category() {
        let error = GraphQLError::new("Permission denied: details here");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        assert!(redacted.message.contains("denied"));
    }

    #[test]
    fn test_clone_error_after_redaction() {
        let error = GraphQLError::new("Database error");
        let profile = SecurityProfile::regulated();
        let redacted = ErrorRedactor::redact(&error, &profile);
        let cloned = redacted.clone();
        assert_eq!(cloned.message, redacted.message);
    }
}
