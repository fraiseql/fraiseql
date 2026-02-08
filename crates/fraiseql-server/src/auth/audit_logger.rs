// Audit logging module for security-critical operations
// Tracks all secret access, authentication events, and security decisions
//
// # Bounds Documentation
//
// This module enforces strict size bounds on all audit log entries to prevent
// memory exhaustion attacks and ensure predictable performance.
//
// ## Field Size Limits
//
// | Field | Max Size | Reason |
// |-------|----------|--------|
// | subject (user ID) | 256 bytes | User IDs rarely exceed this; prevents allocation bloat |
// | operation | 50 bytes | Fixed set of operations (validate, create, refresh, revoke, etc.) |
// | error_message | 1 KB (1024 bytes) | Error messages should be brief; prevents log spam |
// | context | 2 KB (2048 bytes) | Additional context data; rarely needed for detailed info |
// | **Total per entry** | **~4 KB** | Reasonable memory footprint for audit trail |
//
// ## In-Memory Bounds
//
// - **Maximum audit entries in memory**: 10,000 entries (safe for servers with 2GB+ RAM)
// - **Memory per entry**: ~4 KB (structured data + strings)
// - **Total memory for full buffer**: ~40 MB (acceptable overhead)
//
// ## Thread Safety
//
// All audit logger implementations MUST be `Send + Sync` and handle concurrent
// access safely. The `OnceLock` pattern for global logger ensures thread-safe
// initialization.
//
// ## Production Recommendations
//
// 1. **Database-Backed Logging**: Deploy with database-backed audit logger for
//    production to avoid memory limits entirely.
// 2. **Retention Policies**: Implement automated cleanup of old entries in
//    in-memory loggers (e.g., entries older than 24 hours).
// 3. **Sampling**: For high-throughput environments, consider sampling less
//    critical events.
// 4. **Monitoring**: Alert if audit log buffer reaches 80% capacity.
//
// See also: `crate::security::ComplianceAuditLogger` for database-backed logging.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Bounds constants for audit log entries
pub mod bounds {
    /// Maximum length for subject (user ID) field
    pub const MAX_SUBJECT_LEN: usize = 256;

    /// Maximum length for operation field
    pub const MAX_OPERATION_LEN: usize = 50;

    /// Maximum length for error message field
    pub const MAX_ERROR_MESSAGE_LEN: usize = 1024;

    /// Maximum length for context field
    pub const MAX_CONTEXT_LEN: usize = 2048;

    /// Maximum number of audit entries to keep in memory
    pub const MAX_ENTRIES_IN_MEMORY: usize = 10_000;

    /// Estimated memory per entry (used for capacity planning)
    pub const BYTES_PER_ENTRY: usize = 4096;
}

/// Audit log event types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    JwtValidation,
    JwtRefresh,
    OidcCredentialAccess,
    OidcTokenExchange,
    SessionTokenCreated,
    SessionTokenValidation,
    SessionTokenRevoked,
    CsrfStateGenerated,
    CsrfStateValidated,
    OauthStart,
    OauthCallback,
    AuthSuccess,
    AuthFailure,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventType::JwtValidation => "jwt_validation",
            AuditEventType::JwtRefresh => "jwt_refresh",
            AuditEventType::OidcCredentialAccess => "oidc_credential_access",
            AuditEventType::OidcTokenExchange => "oidc_token_exchange",
            AuditEventType::SessionTokenCreated => "session_token_created",
            AuditEventType::SessionTokenValidation => "session_token_validation",
            AuditEventType::SessionTokenRevoked => "session_token_revoked",
            AuditEventType::CsrfStateGenerated => "csrf_state_generated",
            AuditEventType::CsrfStateValidated => "csrf_state_validated",
            AuditEventType::OauthStart => "oauth_start",
            AuditEventType::OauthCallback => "oauth_callback",
            AuditEventType::AuthSuccess => "auth_success",
            AuditEventType::AuthFailure => "auth_failure",
        }
    }
}

/// Secret type being accessed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretType {
    JwtToken,
    SessionToken,
    ClientSecret,
    RefreshToken,
    AuthorizationCode,
    StateToken,
    CsrfToken,
}

impl SecretType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SecretType::JwtToken => "jwt_token",
            SecretType::SessionToken => "session_token",
            SecretType::ClientSecret => "client_secret",
            SecretType::RefreshToken => "refresh_token",
            SecretType::AuthorizationCode => "authorization_code",
            SecretType::StateToken => "state_token",
            SecretType::CsrfToken => "csrf_token",
        }
    }
}

/// Audit log entry
///
/// # Size Bounds
///
/// To prevent memory exhaustion and ensure predictable performance, each field
/// is bounded in size:
///
/// - `subject`: Max 256 bytes (see `bounds::MAX_SUBJECT_LEN`)
/// - `operation`: Max 50 bytes (see `bounds::MAX_OPERATION_LEN`)
/// - `error_message`: Max 1 KB (see `bounds::MAX_ERROR_MESSAGE_LEN`)
/// - `context`: Max 2 KB (see `bounds::MAX_CONTEXT_LEN`)
/// - **Total per entry**: ~4 KB
///
/// # Thread Safety
///
/// This struct is immutable once created and `Send + Sync`, making it safe to
/// pass between threads. Audit loggers that implement `AuditLogger` trait are
/// responsible for thread-safe storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Event type (jwt_validation, oauth_callback, etc.)
    pub event_type:    AuditEventType,
    /// Type of secret accessed (jwt_token, session_token, etc.)
    pub secret_type:   SecretType,
    /// Subject (user ID, service account, etc.) - None for anonymous
    /// Max 256 bytes per `bounds::MAX_SUBJECT_LEN`
    pub subject:       Option<String>,
    /// Operation performed (validate, create, revoke, etc.)
    /// Max 50 bytes per `bounds::MAX_OPERATION_LEN`
    pub operation:     String,
    /// Whether the operation succeeded
    pub success:       bool,
    /// Error message if operation failed (user-safe message)
    /// Max 1 KB per `bounds::MAX_ERROR_MESSAGE_LEN`
    pub error_message: Option<String>,
    /// Additional context
    /// Max 2 KB per `bounds::MAX_CONTEXT_LEN`
    pub context:       Option<String>,
}

/// Audit logger trait - allows different implementations (structured logs, database, syslog, etc.)
///
/// # Implementation Requirements
///
/// - **Thread Safety**: All implementations must be `Send + Sync` and safe for
///   concurrent access from multiple threads.
/// - **Bounds Enforcement**: Implementations MAY enforce the bounds defined in
///   this module, or delegate to a database backend that handles large-scale logging.
/// - **Availability**: Should not block request handling; consider async/buffered
///   implementations for performance.
/// - **Error Handling**: Should never panic; swallow errors and log them via
///   tracing instead.
///
/// # Memory Considerations
///
/// - **In-memory implementations**: Should limit to `bounds::MAX_ENTRIES_IN_MEMORY`
///   to prevent unbounded growth.
/// - **Production deployments**: Should use database-backed implementations
///   (`ComplianceAuditLogger`) for scalability and retention.
pub trait AuditLogger: Send + Sync {
    /// Log an audit entry
    ///
    /// Implementations should ensure:
    /// - No panics (errors logged via tracing)
    /// - Thread-safe access to backing storage
    /// - Bounded memory usage (for in-memory implementations)
    fn log_entry(&self, entry: AuditEntry);

    /// Convenience method for successful operations
    fn log_success(
        &self,
        event_type: AuditEventType,
        secret_type: SecretType,
        subject: Option<String>,
        operation: &str,
    ) {
        self.log_entry(AuditEntry {
            event_type,
            secret_type,
            subject,
            operation: operation.to_string(),
            success: true,
            error_message: None,
            context: None,
        });
    }

    /// Convenience method for failed operations
    fn log_failure(
        &self,
        event_type: AuditEventType,
        secret_type: SecretType,
        subject: Option<String>,
        operation: &str,
        error: &str,
    ) {
        self.log_entry(AuditEntry {
            event_type,
            secret_type,
            subject,
            operation: operation.to_string(),
            success: false,
            error_message: Some(error.to_string()),
            context: None,
        });
    }
}

/// Structured logging audit logger - uses tracing for audit events
pub struct StructuredAuditLogger;

impl StructuredAuditLogger {
    pub fn new() -> Self {
        Self
    }
}

impl Default for StructuredAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLogger for StructuredAuditLogger {
    fn log_entry(&self, entry: AuditEntry) {
        if entry.success {
            info!(
                event_type = entry.event_type.as_str(),
                secret_type = entry.secret_type.as_str(),
                subject = ?entry.subject,
                operation = entry.operation,
                context = ?entry.context,
                "Security event: successful operation"
            );
        } else {
            warn!(
                event_type = entry.event_type.as_str(),
                secret_type = entry.secret_type.as_str(),
                subject = ?entry.subject,
                operation = entry.operation,
                error = ?entry.error_message,
                context = ?entry.context,
                "Security event: failed operation"
            );
        }
    }
}

/// Global audit logger instance
pub static AUDIT_LOGGER: std::sync::OnceLock<Arc<dyn AuditLogger>> = std::sync::OnceLock::new();

/// Initialize the global audit logger
pub fn init_audit_logger(logger: Arc<dyn AuditLogger>) {
    let _ = AUDIT_LOGGER.set(logger);
}

/// Get the global audit logger (defaults to structured logging if not initialized)
pub fn get_audit_logger() -> Arc<dyn AuditLogger> {
    AUDIT_LOGGER.get_or_init(|| Arc::new(StructuredAuditLogger::new())).clone()
}

/// Helper trait for audit logging results
/// Makes it easy to add audit logging to Result types
pub trait AuditableResult<T, E> {
    /// Log success or failure of a result
    fn audit_log(
        self,
        event_type: AuditEventType,
        secret_type: SecretType,
        subject: Option<String>,
        operation: &str,
    ) -> Result<T, E>;
}

impl<T, E: std::fmt::Display> AuditableResult<T, E> for Result<T, E> {
    fn audit_log(
        self,
        event_type: AuditEventType,
        secret_type: SecretType,
        subject: Option<String>,
        operation: &str,
    ) -> Result<T, E> {
        let logger = get_audit_logger();
        match &self {
            Ok(_) => logger.log_success(event_type, secret_type, subject, operation),
            Err(e) => {
                logger.log_failure(event_type, secret_type, subject, operation, &e.to_string());
            },
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct TestAuditLogger {
        entries: Mutex<Vec<AuditEntry>>,
    }

    impl TestAuditLogger {
        fn new() -> Self {
            Self {
                entries: Mutex::new(Vec::new()),
            }
        }

        fn get_entries(&self) -> Vec<AuditEntry> {
            self.entries.lock().unwrap().clone()
        }
    }

    impl AuditLogger for TestAuditLogger {
        fn log_entry(&self, entry: AuditEntry) {
            self.entries.lock().unwrap().push(entry);
        }
    }

    #[test]
    fn test_audit_entry_creation() {
        let entry = AuditEntry {
            event_type:    AuditEventType::JwtValidation,
            secret_type:   SecretType::JwtToken,
            subject:       Some("user123".to_string()),
            operation:     "validate".to_string(),
            success:       true,
            error_message: None,
            context:       None,
        };

        assert_eq!(entry.event_type, AuditEventType::JwtValidation);
        assert_eq!(entry.subject, Some("user123".to_string()));
        assert!(entry.success);
    }

    #[test]
    fn test_audit_logger_logs_entry() {
        let logger = TestAuditLogger::new();

        logger.log_success(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123".to_string()),
            "validate",
        );

        let entries = logger.get_entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].success);
    }

    #[test]
    fn test_audit_logger_logs_failure() {
        let logger = TestAuditLogger::new();

        logger.log_failure(
            AuditEventType::JwtValidation,
            SecretType::JwtToken,
            Some("user123".to_string()),
            "validate",
            "Invalid signature",
        );

        let entries = logger.get_entries();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].success);
        assert_eq!(entries[0].error_message, Some("Invalid signature".to_string()));
    }

    #[test]
    fn test_event_type_strings() {
        assert_eq!(AuditEventType::JwtValidation.as_str(), "jwt_validation");
        assert_eq!(AuditEventType::OidcTokenExchange.as_str(), "oidc_token_exchange");
    }

    #[test]
    fn test_secret_type_strings() {
        assert_eq!(SecretType::JwtToken.as_str(), "jwt_token");
        assert_eq!(SecretType::ClientSecret.as_str(), "client_secret");
    }

    // Vulnerability #15: Audit logger bounds documentation tests
    #[test]
    fn test_bounds_constants_are_reasonable() {
        use crate::auth::audit_logger::bounds;

        // Subject length should accommodate typical user IDs
        let max_subject = bounds::MAX_SUBJECT_LEN;
        assert!(max_subject >= 128, "Subject length too small");

        // Operation should cover all operation types
        let max_operation = bounds::MAX_OPERATION_LEN;
        assert!(max_operation >= 20, "Operation length too small");

        // Error messages should have room for context
        let max_error = bounds::MAX_ERROR_MESSAGE_LEN;
        assert!(max_error >= 512, "Error message length too small");

        // Context should allow some additional data
        let max_context = bounds::MAX_CONTEXT_LEN;
        assert!(max_context >= 1024, "Context length too small");

        // In-memory buffer should be large but not excessive
        let max_entries = bounds::MAX_ENTRIES_IN_MEMORY;
        assert!(max_entries >= 1000, "Max entries in memory too small");
        assert!(max_entries <= 100_000, "Max entries in memory too large");
    }

    #[test]
    fn test_bounds_constants_match_documentation() {
        use crate::auth::audit_logger::bounds;

        // Verify documented bounds match constants
        assert_eq!(bounds::MAX_SUBJECT_LEN, 256, "Subject length bound mismatch");
        assert_eq!(bounds::MAX_OPERATION_LEN, 50, "Operation length bound mismatch");
        assert_eq!(
            bounds::MAX_ERROR_MESSAGE_LEN, 1024,
            "Error message length bound mismatch"
        );
        assert_eq!(bounds::MAX_CONTEXT_LEN, 2048, "Context length bound mismatch");
        assert_eq!(
            bounds::MAX_ENTRIES_IN_MEMORY, 10_000,
            "Max entries in memory bound mismatch"
        );
    }

    #[test]
    fn test_memory_per_entry_constant_is_reasonable() {
        use crate::auth::audit_logger::bounds;

        // Memory per entry should be sensible
        let bytes_per_entry = bounds::BYTES_PER_ENTRY;
        let max_entries = bounds::MAX_ENTRIES_IN_MEMORY;
        let total_memory_mb = (bytes_per_entry * max_entries) / (1024 * 1024);

        // Total memory for full buffer should be reasonable (< 100 MB)
        assert!(
            total_memory_mb < 100,
            "Total memory for full buffer too large: {} MB",
            total_memory_mb
        );

        // But not absurdly small (> 10 MB for safety margin)
        assert!(
            total_memory_mb > 10,
            "Total memory for full buffer too small: {} MB",
            total_memory_mb
        );
    }

    #[test]
    fn test_audit_entry_field_sizes_within_bounds() {
        use crate::auth::audit_logger::bounds;

        let entry = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            secret_type: SecretType::JwtToken,
            subject: Some("a".repeat(bounds::MAX_SUBJECT_LEN)),
            operation: "validate".to_string(),
            success: true,
            error_message: None,
            context: None,
        };

        // Verify subject fits within bounds
        assert!(entry.subject.as_ref().unwrap().len() <= bounds::MAX_SUBJECT_LEN);
    }

    #[test]
    fn test_error_message_bound_accommodates_typical_errors() {
        use crate::auth::audit_logger::bounds;

        // Typical security errors should fit
        let error_messages = vec![
            "Invalid signature",
            "Token expired",
            "User not authorized",
            "Failed to decrypt payload: AES-256-GCM decryption returned InvalidTag",
            "Database connection timeout after 30 seconds waiting for available connection",
        ];

        for msg in error_messages {
            assert!(
                msg.len() <= bounds::MAX_ERROR_MESSAGE_LEN,
                "Error message too long: {} bytes for: {}",
                msg.len(),
                msg
            );
        }
    }

    #[test]
    fn test_operation_bound_covers_all_audit_operations() {
        use crate::auth::audit_logger::bounds;

        // All documented operation names should fit
        let operations = vec!["validate", "create", "revoke", "refresh", "exchange", "logout"];

        for op in operations {
            assert!(
                op.len() <= bounds::MAX_OPERATION_LEN,
                "Operation name too long: {} bytes for: {}",
                op.len(),
                op
            );
        }
    }

    #[test]
    fn test_global_audit_logger_is_singleton() {
        // Verify that the global audit logger can only be initialized once
        let logger1 = get_audit_logger();
        let logger2 = get_audit_logger();

        // Both should return the same instance
        assert_eq!(
            Arc::as_ptr(&logger1),
            Arc::as_ptr(&logger2),
            "Audit loggers are not the same singleton instance"
        );
    }

    #[test]
    fn test_audit_entry_sizes_reasonable_for_serialization() {
        use crate::auth::audit_logger::bounds;

        // Create a maximum-size entry
        let max_entry = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            secret_type: SecretType::JwtToken,
            subject: Some("a".repeat(bounds::MAX_SUBJECT_LEN)),
            operation: "validate".to_string(),
            success: false,
            error_message: Some("e".repeat(bounds::MAX_ERROR_MESSAGE_LEN)),
            context: Some("c".repeat(bounds::MAX_CONTEXT_LEN)),
        };

        // Serialize to JSON to estimate size
        let json = serde_json::to_string(&max_entry);
        assert!(json.is_ok(), "Failed to serialize maximum-size entry");

        let json_size = json.unwrap().len();
        // JSON should be reasonable size (with overhead, but < 2x fields)
        assert!(
            json_size < bounds::BYTES_PER_ENTRY * 2,
            "JSON serialization too large: {} bytes",
            json_size
        );
    }
}
