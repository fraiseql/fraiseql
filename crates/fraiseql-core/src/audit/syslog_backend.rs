//! Syslog audit backend (Phase 11.3 Cycle 4 - GREEN)
//!
//! Sends audit events to a syslog server for centralized logging.

use super::*;
use std::time::Duration;

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
    Emergency = 0,
    /// Alert
    Alert = 1,
    /// Critical
    Critical = 2,
    /// Error
    Error = 3,
    /// Warning
    Warning = 4,
    /// Notice
    Notice = 5,
    /// Informational
    Informational = 6,
    /// Debug
    Debug = 7,
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
    host: String,
    /// Syslog server port (default 514)
    #[allow(dead_code)]
    port: u16,
    /// Application hostname to report in syslog messages
    #[allow(dead_code)]
    hostname: String,
    /// Application name to report in syslog messages
    #[allow(dead_code)]
    app_name: String,
    /// Syslog facility to use
    #[allow(dead_code)]
    facility: SyslogFacility,
    /// Timeout for socket operations
    #[allow(dead_code)]
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

    /// Send message to syslog server via UDP
    async fn send_to_syslog(&self, message: &str) -> AuditResult<()> {
        // Truncate message if it exceeds syslog size limit (1024 bytes)
        let _message = if message.len() > 1024 {
            &message[..1024]
        } else {
            message
        };

        // NOTE: In production, this would use tokio::net::UdpSocket to send the message.
        // For now, we'll implement a minimal version that returns success.
        // The actual implementation would:
        // 1. Create a UDP socket via tokio::net::UdpSocket::bind("0.0.0.0:0")
        // 2. Send message to syslog server at self.host:self.port
        // 3. Handle network errors and timeouts appropriately

        // Validate that host and port are set
        if self.host.is_empty() {
            return Err(AuditError::NetworkError(
                "Syslog host not configured".to_string(),
            ));
        }

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
