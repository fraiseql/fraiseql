//! Async audit logging for security events.

use super::errors::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use uuid::Uuid;

/// Audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    // Authentication
    /// Successful user login
    LoginSuccess,
    /// Failed login attempt
    LoginFailure,
    /// User logout
    Logout,
    /// Authentication token refreshed
    TokenRefresh,
    /// Authentication token revoked
    TokenRevoke,

    // Authorization
    /// Permission check passed
    PermissionGranted,
    /// Permission check failed
    PermissionDenied,
    /// Role assigned to user
    RoleAssigned,
    /// Role removed from user
    RoleRevoked,

    // Data access
    /// Data read operation
    DataRead,
    /// Data write operation
    DataWrite,
    /// Data delete operation
    DataDelete,

    // Security
    /// Rate limit threshold exceeded
    RateLimitExceeded,
    /// Invalid or expired token used
    InvalidToken,
    /// Suspicious activity detected
    SuspiciousActivity,
    /// Security policy violation
    SecurityViolation,
}

impl AuditEventType {
    /// Get the default severity level for this event type
    #[must_use]
    pub const fn severity(&self) -> AuditSeverity {
        match self {
            Self::LoginFailure
            | Self::PermissionDenied
            | Self::InvalidToken
            | Self::SecurityViolation => AuditSeverity::High,

            Self::RateLimitExceeded | Self::SuspiciousActivity => AuditSeverity::Medium,

            _ => AuditSeverity::Low,
        }
    }

    /// Get the category name for this event type
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self {
            Self::LoginSuccess
            | Self::LoginFailure
            | Self::Logout
            | Self::TokenRefresh
            | Self::TokenRevoke => "authentication",

            Self::PermissionGranted
            | Self::PermissionDenied
            | Self::RoleAssigned
            | Self::RoleRevoked => "authorization",

            Self::DataRead | Self::DataWrite | Self::DataDelete => "data_access",

            Self::RateLimitExceeded
            | Self::InvalidToken
            | Self::SuspiciousActivity
            | Self::SecurityViolation => "security",
        }
    }
}

/// Audit event severity levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuditSeverity {
    /// Low severity - informational events
    Low,
    /// Medium severity - potential issues
    Medium,
    /// High severity - security concerns
    High,
    /// Critical severity - immediate action required
    Critical,
}

/// Audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    /// Unique event identifier
    pub id: Uuid,
    /// Type of audit event
    pub event_type: AuditEventType,
    /// User who triggered the event
    pub user_id: Option<Uuid>,
    /// Tenant context
    pub tenant_id: Option<Uuid>,
    /// Resource being accessed
    pub resource: Option<String>,
    /// Action performed
    pub action: Option<String>,
    /// Event status: "success" or "failure"
    pub status: String,
    /// Client IP address
    pub ip_address: Option<String>,
    /// Client user agent
    pub user_agent: Option<String>,
    /// Additional event metadata
    pub metadata: Option<serde_json::Value>,
    /// Event timestamp
    pub timestamp: DateTime<Utc>,
    /// Event severity level
    pub severity: AuditSeverity,
}

impl AuditEvent {
    /// Create a new audit event with default values
    #[must_use]
    pub fn new(event_type: AuditEventType) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type: event_type.clone(),
            user_id: None,
            tenant_id: None,
            resource: None,
            action: None,
            status: "success".to_string(),
            ip_address: None,
            user_agent: None,
            metadata: None,
            timestamp: Utc::now(),
            severity: event_type.severity(),
        }
    }

    /// Set the user ID for this audit event
    #[must_use]
    pub const fn with_user(mut self, user_id: Uuid) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Set the tenant ID for this audit event
    #[must_use]
    pub const fn with_tenant(mut self, tenant_id: Uuid) -> Self {
        self.tenant_id = Some(tenant_id);
        self
    }

    /// Set the resource and action for this audit event
    #[must_use]
    pub fn with_resource(mut self, resource: String, action: String) -> Self {
        self.resource = Some(resource);
        self.action = Some(action);
        self
    }

    /// Set the status for this audit event
    #[must_use]
    pub fn with_status(mut self, status: String) -> Self {
        self.status = status;
        self
    }

    /// Set additional metadata for this audit event
    #[must_use]
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Set the IP address for this audit event
    #[must_use]
    pub fn with_ip(mut self, ip_address: String) -> Self {
        self.ip_address = Some(ip_address);
        self
    }

    /// Set the user agent for this audit event
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }

    /// Set the severity level for this audit event
    #[must_use]
    pub const fn with_severity(mut self, severity: AuditSeverity) -> Self {
        self.severity = severity;
        self
    }
}

/// Async audit logger with buffered writes
pub struct AuditLogger {
    tx: mpsc::UnboundedSender<AuditEvent>,
}

impl AuditLogger {
    /// Create audit logger with async worker
    #[must_use]
    pub fn new(pool: Pool) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn async worker to write audit logs
        tokio::spawn(async move {
            Self::audit_worker(pool, rx).await;
        });

        Self { tx }
    }

    /// Log audit event (non-blocking)
    pub fn log(&self, event: AuditEvent) {
        // Fire and forget - if channel is closed, event is lost
        // Production would use reliable queue (Kafka, RabbitMQ)
        let _ = self.tx.send(event);
    }

    /// Async worker to write audit logs to database
    async fn audit_worker(pool: Pool, mut rx: mpsc::UnboundedReceiver<AuditEvent>) {
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 10;

        while let Some(event) = rx.recv().await {
            match Self::write_event(&pool, &event).await {
                Ok(()) => {
                    consecutive_errors = 0; // Reset error counter on success
                }
                Err(e) => {
                    consecutive_errors += 1;
                    eprintln!("Failed to write audit log (attempt {consecutive_errors}): {e}");

                    // If too many consecutive errors, log to stderr and continue
                    // In production, this might trigger alerts or fallback logging
                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        eprintln!("WARNING: {consecutive_errors} consecutive audit log failures. Check database connectivity.");
                        // Could implement circuit breaker pattern here
                    }

                    // For critical events, retry with backoff
                    if Self::should_retry(&event, consecutive_errors) {
                        consecutive_errors =
                            Self::retry_critical_event(&pool, &event, consecutive_errors).await;
                    }
                }
            }
        }
    }

    /// Check if we should retry writing this event
    const fn should_retry(event: &AuditEvent, consecutive_errors: u32) -> bool {
        Self::is_critical_event(event) && consecutive_errors < 3
    }

    /// Retry writing a critical event with exponential backoff
    async fn retry_critical_event(
        pool: &deadpool_postgres::Pool,
        event: &AuditEvent,
        consecutive_errors: u32,
    ) -> u32 {
        // Exponential backoff
        tokio::time::sleep(tokio::time::Duration::from_millis(
            100 * u64::from(consecutive_errors),
        ))
        .await;

        // Retry write
        if (Self::write_event(pool, event).await).is_ok() {
            0 // Reset error counter on success
        } else {
            consecutive_errors // Keep current count on failure
        }
    }

    /// Check if event is critical and should be retried
    const fn is_critical_event(event: &AuditEvent) -> bool {
        matches!(
            event.event_type,
            AuditEventType::LoginFailure
                | AuditEventType::PermissionDenied
                | AuditEventType::InvalidToken
                | AuditEventType::SecurityViolation
                | AuditEventType::SuspiciousActivity
        )
    }

    /// Write single event to database
    async fn write_event(pool: &Pool, event: &AuditEvent) -> Result<()> {
        let client = pool.get().await?;

        let sql = r"
            INSERT INTO audit_logs (
                id, event_type, user_id, tenant_id, resource, action,
                status, ip_address, user_agent, metadata, timestamp, severity
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ";

        let event_type_json = serde_json::to_string(&event.event_type).map_err(|e| {
            super::errors::SecurityError::AuditLogFailure(format!(
                "Failed to serialize event type: {e}"
            ))
        })?;
        let severity_json = serde_json::to_string(&event.severity).map_err(|e| {
            super::errors::SecurityError::AuditLogFailure(format!(
                "Failed to serialize severity: {e}"
            ))
        })?;

        client
            .execute(
                sql,
                &[
                    &event.id.to_string(),
                    &event_type_json,
                    &event.user_id.map(|u| u.to_string()),
                    &event.tenant_id.map(|t| t.to_string()),
                    &event.resource,
                    &event.action,
                    &event.status,
                    &event.ip_address,
                    &event.user_agent,
                    &event
                        .metadata
                        .as_ref()
                        .map(|m| serde_json::to_string(m).unwrap_or_default()),
                    &event.timestamp.to_rfc3339(),
                    &severity_json,
                ],
            )
            .await?;

        Ok(())
    }

    /// Get audit statistics
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database pool connection fails
    /// - SQL query execution fails
    /// - Result row extraction fails
    pub async fn stats(&self, pool: &Pool) -> Result<AuditStats> {
        let client = pool.get().await?;

        let sql = "SELECT COUNT(*) as total_events FROM audit_logs";
        let total_events: i64 = client.query_one(sql, &[]).await?.get(0);

        let sql = "SELECT COUNT(*) as recent_events FROM audit_logs WHERE timestamp > NOW() - INTERVAL '1 hour'";
        let recent_events: i64 = client.query_one(sql, &[]).await?.get(0);

        Ok(AuditStats {
            total_events: total_events as usize,
            recent_events: recent_events as usize,
        })
    }
}

impl Clone for AuditLogger {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}

/// Audit log statistics
#[derive(Debug)]
pub struct AuditStats {
    /// Total number of audit events
    pub total_events: usize,
    /// Number of recent events (last 24 hours)
    pub recent_events: usize,
}
