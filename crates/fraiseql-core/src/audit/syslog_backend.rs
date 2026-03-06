//! Syslog audit backend
//!
//! Sends audit events to a syslog server for centralized logging.

use std::time::Duration;

use super::*;

/// Syslog facility (for priority calculation)
/// Facility * 8 + severity = priority
#[derive(Debug, Clone, Copy)]
pub enum SyslogFacility {
    /// Local use 0
    Local0 = 16,
    /// Local use 1
    Local1 = 17,
    /// Local use 2
    Local2 = 18,
    /// Local use 3
    Local3 = 19,
    /// Local use 4
    Local4 = 20,
    /// Local use 5
    Local5 = 21,
    /// Local use 6
    Local6 = 22,
    /// Local use 7
    Local7 = 23,
}

/// Syslog severity levels
#[derive(Debug, Clone, Copy)]
pub enum SyslogSeverity {
    /// Emergency
    Emergency     = 0,
    /// Alert
    Alert         = 1,
    /// Critical
    Critical      = 2,
    /// Error
    Error         = 3,
    /// Warning
    Warning       = 4,
    /// Notice
    Notice        = 5,
    /// Informational
    Informational = 6,
    /// Debug
    Debug         = 7,
}

impl SyslogSeverity {
    /// Map audit event status to syslog severity
    fn from_audit_status(status: &str) -> Self {
        match status {
            "success" => SyslogSeverity::Informational,
            "failure" => SyslogSeverity::Warning,
            "denied" => SyslogSeverity::Notice,
            _ => SyslogSeverity::Debug,
        }
    }
}

/// Syslog audit backend for centralized audit logging.
///
/// Sends audit events to a remote syslog server via UDP using RFC 3164 format.
/// Syslog backend does not store events locally - it acts as a forwarding sink.
#[derive(Clone)]
pub struct SyslogAuditBackend {
    /// Syslog server host
    host:     String,
    /// Syslog server port (default 514)
    port: u16,
    /// Application hostname to report in syslog messages
    hostname: String,
    /// Application name to report in syslog messages
    app_name: String,
    /// Syslog facility to use
    facility: SyslogFacility,
    /// Timeout for socket operations
    timeout: Duration,
}

impl SyslogAuditBackend {
    /// Create a new Syslog audit backend.
    ///
    /// # Arguments
    ///
    /// * `host` - Syslog server hostname or IP
    /// * `port` - Syslog server port (typically 514)
    ///
    /// # Returns
    ///
    /// New backend instance (connection is lazy - happens on first log)
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            hostname: "fraiseql".to_string(),
            app_name: "fraiseql-audit".to_string(),
            facility: SyslogFacility::Local0,
            timeout: Duration::from_secs(5),
        }
    }

    /// Set the syslog facility to use
    pub fn with_facility(mut self, facility: SyslogFacility) -> Self {
        self.facility = facility;
        self
    }

    /// Set the application name
    pub fn with_app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = app_name.into();
        self
    }

    /// Set the socket timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Format an event as an RFC 3164 syslog message
    fn format_rfc3164(&self, event: &AuditEvent, severity: SyslogSeverity) -> String {
        // Calculate priority: (facility * 8) + severity
        let priority = (self.facility as u8) * 8 + (severity as u8);

        // Get current timestamp in RFC 3164 format (Dec  6 10:10:00)
        let now = chrono::Local::now();
        let timestamp = now.format("%b %e %H:%M:%S").to_string();

        // Convert event to JSON for the message body
        let event_json = serde_json::to_string(event)
            .unwrap_or_else(|_| "failed to serialize event".to_string());

        // RFC 3164 format: <PRI>TIMESTAMP HOSTNAME TAG[PID]: MESSAGE
        format!(
            "<{}>{} {} {}[{}]: {}",
            priority,
            timestamp,
            self.hostname,
            self.app_name,
            std::process::id(),
            event_json
        )
    }

    /// Send message to syslog server via UDP (RFC 3164, §4).
    async fn send_to_syslog(&self, message: &str) -> AuditResult<()> {
        if self.host.is_empty() {
            return Err(AuditError::NetworkError("Syslog host not configured".to_string()));
        }

        // Truncate to 1024 bytes as required by RFC 3164.
        let payload = if message.len() > 1024 { &message[..1024] } else { message };

        let addr = format!("{}:{}", self.host, self.port);
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| AuditError::NetworkError(format!("Failed to bind UDP socket: {e}")))?;
        tokio::time::timeout(self.timeout, socket.send_to(payload.as_bytes(), &addr))
            .await
            .map_err(|_| AuditError::NetworkError(format!("Timed out sending syslog to {addr}")))?
            .map_err(|e| AuditError::NetworkError(format!("Failed to send syslog packet to {addr}: {e}")))?;
        Ok(())
    }
}

#[async_trait::async_trait]
impl AuditBackend for SyslogAuditBackend {
    /// Log an audit event to syslog.
    async fn log_event(&self, event: AuditEvent) -> AuditResult<()> {
        // Validate event before logging
        event.validate()?;

        // Determine severity based on event status
        let severity = SyslogSeverity::from_audit_status(&event.status);

        // Format as syslog message
        let message = self.format_rfc3164(&event, severity);

        // Send to syslog server (blocking operation in async context)
        // For production, consider using tokio::net::UdpSocket or similar
        self.send_to_syslog(&message).await
    }

    /// Query audit events from syslog backend.
    ///
    /// Syslog backend does not support querying - events are sent to syslog
    /// server for centralized storage and querying there.
    /// This returns an empty vector to indicate no local storage.
    async fn query_events(&self, _filters: AuditQueryFilters) -> AuditResult<Vec<AuditEvent>> {
        // Syslog backend doesn't store events locally
        // Return empty vector - queries must be performed on syslog server
        Ok(vec![])
    }
}

// Re-export for convenience
pub use super::AuditBackend;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event() -> AuditEvent {
        AuditEvent::new_user_action("user-1", "alice", "127.0.0.1", "users", "query", "success")
    }

    #[test]
    fn test_syslog_format_rfc3164() {
        let backend = SyslogAuditBackend::new("localhost", 514);
        let event = test_event();
        let msg = backend.format_rfc3164(&event, SyslogSeverity::Informational);

        // RFC 3164: <PRI>TIMESTAMP HOSTNAME TAG[PID]: MESSAGE
        assert!(msg.starts_with('<'), "must start with priority: {msg}");
        assert!(msg.contains("fraiseql-audit"), "must contain app name");
        assert!(msg.contains("fraiseql"), "must contain hostname");
        // Priority for Local0 (16*8=128) + Informational (6) = 134
        assert!(msg.starts_with("<134>"), "priority mismatch: {msg}");
    }
}
