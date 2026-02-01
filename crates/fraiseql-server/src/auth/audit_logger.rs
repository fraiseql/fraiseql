// Audit logging module for security-critical operations
// Tracks all secret access, authentication events, and security decisions
// Phase 7, Cycle 1: GREEN phase - Implementation

use std::sync::Arc;

use tracing::{info, warn};

/// Audit log event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct AuditEntry {
    /// Event type (jwt_validation, oauth_callback, etc.)
    pub event_type:    AuditEventType,
    /// Type of secret accessed (jwt_token, session_token, etc.)
    pub secret_type:   SecretType,
    /// Subject (user ID, service account, etc.) - None for anonymous
    pub subject:       Option<String>,
    /// Operation performed (validate, create, revoke, etc.)
    pub operation:     String,
    /// Whether the operation succeeded
    pub success:       bool,
    /// Error message if operation failed (user-safe message)
    pub error_message: Option<String>,
    /// Additional context
    pub context:       Option<String>,
}

/// Audit logger trait - allows different implementations (structured logs, database, syslog, etc.)
pub trait AuditLogger: Send + Sync {
    /// Log an audit entry
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
}
