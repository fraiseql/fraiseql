//! Syslog audit logging backend (RFC 5424).
//!
//! Sends audit entries as RFC 5424 structured data messages over TCP or UDP.
//! Feature-gated behind `audit-syslog`.
//!
//! # Message format
//!
//! ```text
//! <PRI>1 TIMESTAMP HOSTNAME fraiseql - AUDIT [audit@49152 event="jwt_validation"
//!   secret="jwt_token" subject="user42" operation="validate" success="true"] message
//! ```
//!
//! # Thread safety
//!
//! `SyslogAuditLogger` is `Send + Sync`. The transport socket is guarded by a
//! [`parking_lot::Mutex`].

use std::io::Write;
use std::net::{TcpStream, UdpSocket};

use chrono::Utc;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::audit_logger::{AuditEntry, AuditEventType, AuditLogger};

/// IANA Private Enterprise Number placeholder for structured data.
/// Using the "example" PEN 49152 (reserved for documentation/testing).
const SD_ID: &str = "audit@49152";

/// Maximum UDP message size (RFC 5426 recommends 2048 for syslog over UDP).
const MAX_UDP_MSG_LEN: usize = 2048;

// ============================================================================
// Configuration
// ============================================================================

/// Transport protocol for syslog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SyslogTransport {
    /// UDP (fire-and-forget, no delivery guarantee).
    Udp,
    /// TCP (reliable delivery, connection-oriented).
    Tcp,
}

/// Syslog facility codes (RFC 5424 §6.2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum SyslogFacility {
    /// Security/authorization messages (facility 4).
    Auth,
    /// Security/authorization messages (facility 10, private).
    AuthPriv,
    /// Local use 0 (facility 16).
    Local0,
    /// Local use 6 (facility 22).
    Local6,
}

impl SyslogFacility {
    /// Return the numeric facility code.
    const fn code(self) -> u8 {
        match self {
            Self::Auth => 4,
            Self::AuthPriv => 10,
            Self::Local0 => 16,
            Self::Local6 => 22,
        }
    }
}

/// Configuration for the syslog audit backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyslogAuditConfig {
    /// Syslog server hostname or IP.
    pub host: String,
    /// Syslog server port (default: 514).
    pub port: u16,
    /// Transport protocol (TCP or UDP).
    pub transport: SyslogTransport,
    /// Syslog facility.
    pub facility: SyslogFacility,
    /// Hostname to include in syslog messages. Defaults to system hostname.
    pub hostname: Option<String>,
}

impl Default for SyslogAuditConfig {
    fn default() -> Self {
        Self {
            host:      "127.0.0.1".to_string(),
            port:      514,
            transport: SyslogTransport::Udp,
            facility:  SyslogFacility::Auth,
            hostname:  None,
        }
    }
}

// ============================================================================
// Transport abstraction
// ============================================================================

enum Transport {
    Udp(UdpSocket),
    Tcp(TcpStream),
}

impl Transport {
    fn send(&mut self, msg: &[u8]) -> std::io::Result<()> {
        match self {
            Transport::Udp(sock) => {
                // Truncate if exceeding UDP limit.
                let end = msg.len().min(MAX_UDP_MSG_LEN);
                sock.send(&msg[..end])?;
                Ok(())
            },
            Transport::Tcp(stream) => {
                stream.write_all(msg)?;
                // RFC 5425: TCP syslog uses newline framing.
                stream.write_all(b"\n")?;
                Ok(())
            },
        }
    }
}

// ============================================================================
// SyslogAuditLogger
// ============================================================================

/// RFC 5424 syslog audit logger.
pub struct SyslogAuditLogger {
    transport: Mutex<Transport>,
    facility:  SyslogFacility,
    hostname:  String,
}

impl SyslogAuditLogger {
    /// Create a new syslog audit logger.
    ///
    /// # Errors
    ///
    /// Returns `std::io::Error` if the transport socket cannot be created.
    pub fn new(config: SyslogAuditConfig) -> std::io::Result<Self> {
        let addr = format!("{}:{}", config.host, config.port);
        let transport = match config.transport {
            SyslogTransport::Udp => {
                let sock = UdpSocket::bind("0.0.0.0:0")?;
                sock.connect(&addr)?;
                Transport::Udp(sock)
            },
            SyslogTransport::Tcp => {
                let stream = TcpStream::connect(&addr)?;
                Transport::Tcp(stream)
            },
        };

        let hostname = config.hostname.unwrap_or_else(|| {
            hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "fraiseql".to_string())
        });

        Ok(Self {
            transport: Mutex::new(transport),
            facility:  config.facility,
            hostname,
        })
    }

    /// Map an `AuditEventType` to an RFC 5424 severity (0-7).
    fn severity(entry: &AuditEntry) -> u8 {
        if entry.success {
            // Informational (6)
            6
        } else {
            match entry.event_type {
                AuditEventType::AuthFailure => 4, // Warning
                _ => 5,                           // Notice
            }
        }
    }

    /// Build an RFC 5424 message.
    fn format_message(&self, entry: &AuditEntry) -> String {
        let severity = Self::severity(entry);
        let priority = u16::from(self.facility.code()) * 8 + u16::from(severity);
        let timestamp = Utc::now().to_rfc3339();

        let subject = entry.subject.as_deref().unwrap_or("-");
        let error_msg = entry.error_message.as_deref().unwrap_or("");

        // RFC 5424 structured data.
        let sd = format!(
            "[{SD_ID} event=\"{}\" secret=\"{}\" subject=\"{}\" operation=\"{}\" success=\"{}\"]",
            entry.event_type.as_str(),
            entry.secret_type.as_str(),
            escape_sd_value(subject),
            escape_sd_value(&entry.operation),
            entry.success,
        );

        // MSG part: human-readable summary.
        let msg = if entry.success {
            format!(
                "{} {} by {}",
                entry.event_type.as_str(),
                entry.operation,
                subject,
            )
        } else {
            format!(
                "{} {} by {} FAILED: {}",
                entry.event_type.as_str(),
                entry.operation,
                subject,
                error_msg,
            )
        };

        // <PRI>VERSION TIMESTAMP HOSTNAME APP-NAME PROCID MSGID SD MSG
        format!(
            "<{priority}>1 {timestamp} {} fraiseql - AUDIT {sd} {msg}",
            self.hostname,
        )
    }
}

impl AuditLogger for SyslogAuditLogger {
    fn log_entry(&self, entry: AuditEntry) {
        let msg = self.format_message(&entry);
        let mut transport = self.transport.lock();
        if let Err(e) = transport.send(msg.as_bytes()) {
            error!(error = %e, "Failed to send audit entry via syslog");
        }
    }
}

/// Escape characters that are special in RFC 5424 SD-VALUE: `"`, `\`, `]`.
fn escape_sd_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(']', "\\]")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::audit_logger::{AuditEventType, SecretType};

    fn test_entry() -> AuditEntry {
        AuditEntry {
            event_type:    AuditEventType::JwtValidation,
            secret_type:   SecretType::JwtToken,
            subject:       Some("user42".to_string()),
            operation:     "validate".to_string(),
            success:       true,
            error_message: None,
            context:       None,
            chain_hash:    None,
        }
    }

    fn failure_entry() -> AuditEntry {
        AuditEntry {
            event_type:    AuditEventType::AuthFailure,
            secret_type:   SecretType::ClientSecret,
            subject:       Some("attacker".to_string()),
            operation:     "exchange".to_string(),
            success:       false,
            error_message: Some("Invalid grant".to_string()),
            context:       None,
            chain_hash:    None,
        }
    }

    #[test]
    fn test_format_success_message() {
        let logger = SyslogAuditLogger {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            facility:  SyslogFacility::Auth,
            hostname:  "test-host".to_string(),
        };

        let msg = logger.format_message(&test_entry());

        // Priority = facility(4) * 8 + severity(6) = 38
        assert!(msg.starts_with("<38>1 "), "Priority should be 38, got: {msg}");
        assert!(msg.contains("test-host"));
        assert!(msg.contains("fraiseql"));
        assert!(msg.contains("AUDIT"));
        assert!(msg.contains(r#"event="jwt_validation""#));
        assert!(msg.contains(r#"secret="jwt_token""#));
        assert!(msg.contains(r#"subject="user42""#));
        assert!(msg.contains(r#"operation="validate""#));
        assert!(msg.contains(r#"success="true""#));
    }

    #[test]
    fn test_format_failure_message() {
        let logger = SyslogAuditLogger {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            facility:  SyslogFacility::Auth,
            hostname:  "test-host".to_string(),
        };

        let msg = logger.format_message(&failure_entry());

        // Priority = facility(4) * 8 + severity(4) = 36
        assert!(msg.starts_with("<36>1 "), "Auth failure priority should be 36");
        assert!(msg.contains("FAILED"));
        assert!(msg.contains("Invalid grant"));
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(SyslogAuditLogger::severity(&test_entry()), 6);
        assert_eq!(SyslogAuditLogger::severity(&failure_entry()), 4);

        let notice_fail = AuditEntry {
            event_type: AuditEventType::JwtValidation,
            success:    false,
            ..test_entry()
        };
        assert_eq!(SyslogAuditLogger::severity(&notice_fail), 5);
    }

    #[test]
    fn test_facility_codes() {
        assert_eq!(SyslogFacility::Auth.code(), 4);
        assert_eq!(SyslogFacility::AuthPriv.code(), 10);
        assert_eq!(SyslogFacility::Local0.code(), 16);
        assert_eq!(SyslogFacility::Local6.code(), 22);
    }

    #[test]
    fn test_escape_sd_value() {
        assert_eq!(escape_sd_value(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_sd_value(r"a\b"), r"a\\b");
        assert_eq!(escape_sd_value("a]b"), r"a\]b");
        assert_eq!(escape_sd_value("normal"), "normal");
    }

    #[test]
    fn test_syslog_udp_send() {
        // Bind a local UDP socket to receive messages.
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let recv_addr = receiver.local_addr().unwrap();
        receiver.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();

        let config = SyslogAuditConfig {
            host:      "127.0.0.1".to_string(),
            port:      recv_addr.port(),
            transport: SyslogTransport::Udp,
            facility:  SyslogFacility::Auth,
            hostname:  Some("test-node".to_string()),
        };

        let logger = SyslogAuditLogger::new(config).unwrap();
        logger.log_entry(test_entry());

        let mut buf = [0u8; 4096];
        let n = receiver.recv(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(received.contains("jwt_validation"));
        assert!(received.contains("test-node"));
    }

    #[test]
    fn test_syslog_tcp_send() {
        // Bind a TCP listener.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let listen_addr = listener.local_addr().unwrap();

        let config = SyslogAuditConfig {
            host:      "127.0.0.1".to_string(),
            port:      listen_addr.port(),
            transport: SyslogTransport::Tcp,
            facility:  SyslogFacility::AuthPriv,
            hostname:  Some("tcp-node".to_string()),
        };

        let logger = SyslogAuditLogger::new(config).unwrap();
        logger.log_entry(test_entry());

        let (mut conn, _) = listener.accept().unwrap();
        conn.set_read_timeout(Some(std::time::Duration::from_secs(1))).unwrap();

        let mut buf = [0u8; 4096];
        use std::io::Read;
        let n = conn.read(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        // Priority = AuthPriv(10) * 8 + 6 = 86
        assert!(received.starts_with("<86>1 "));
        assert!(received.contains("tcp-node"));
        assert!(received.ends_with('\n'), "TCP syslog should end with newline");
    }

    #[test]
    fn test_no_subject_uses_dash() {
        let logger = SyslogAuditLogger {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            facility:  SyslogFacility::Auth,
            hostname:  "test-host".to_string(),
        };

        let entry = AuditEntry {
            subject: None,
            ..test_entry()
        };
        let msg = logger.format_message(&entry);
        assert!(msg.contains(r#"subject="-""#));
    }

    #[test]
    fn test_default_config() {
        let config = SyslogAuditConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 514);
        assert_eq!(config.transport, SyslogTransport::Udp);
        assert_eq!(config.facility, SyslogFacility::Auth);
        assert!(config.hostname.is_none());
    }
}
