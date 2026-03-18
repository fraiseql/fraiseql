//! Syslog audit exporter (RFC 5424).
//!
//! Streams [`AuditEntry`] records as RFC 5424 structured data messages over
//! TCP or UDP. Feature-gated behind `audit-syslog`.
//!
//! # Message format
//!
//! ```text
//! <PRI>1 TIMESTAMP HOSTNAME fraiseql - AUDIT [audit@49152 user="123"
//!   operation="query" level="INFO"] { users { id name } }
//! ```

use std::io::Write;
use std::net::{TcpStream, UdpSocket};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;

use super::audit::{AuditEntry, AuditError, AuditExporter, AuditLevel, SyslogExportConfig};

/// IANA Private Enterprise Number placeholder for structured data.
const SD_ID: &str = "audit@49152";

/// Maximum UDP message size (RFC 5426 recommends 2048 for syslog over UDP).
const MAX_UDP_MSG_LEN: usize = 2048;

/// Syslog transport.
enum Transport {
    Udp(UdpSocket),
    Tcp(TcpStream),
}

impl Transport {
    fn send(&mut self, msg: &[u8]) -> std::io::Result<()> {
        match self {
            Transport::Udp(sock) => {
                let end = msg.len().min(MAX_UDP_MSG_LEN);
                sock.send(&msg[..end])?;
                Ok(())
            },
            Transport::Tcp(stream) => {
                stream.write_all(msg)?;
                stream.write_all(b"\n")?;
                Ok(())
            },
        }
    }
}

/// RFC 5424 syslog audit exporter for GraphQL operation audit entries.
pub struct SyslogAuditExporter {
    transport: Mutex<Transport>,
    hostname:  String,
}

impl SyslogAuditExporter {
    /// Create a new syslog audit exporter from configuration.
    ///
    /// # Errors
    ///
    /// Returns [`AuditError::Export`] if the transport socket cannot be created.
    pub fn new(config: &SyslogExportConfig) -> Result<Self, AuditError> {
        let addr = format!("{}:{}", config.address, config.port);
        let transport = if config.protocol.as_str() == "tcp" {
            let stream = TcpStream::connect(&addr)
                .map_err(|e| AuditError::Export(format!("syslog TCP connect to {addr}: {e}")))?;
            Transport::Tcp(stream)
        } else {
            let sock = UdpSocket::bind("0.0.0.0:0")
                .map_err(|e| AuditError::Export(format!("syslog UDP bind: {e}")))?;
            sock.connect(&addr)
                .map_err(|e| AuditError::Export(format!("syslog UDP connect to {addr}: {e}")))?;
            Transport::Udp(sock)
        };

        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_else(|| "fraiseql".to_string());

        Ok(Self {
            transport: Mutex::new(transport),
            hostname,
        })
    }

    /// Map audit level to RFC 5424 severity (0-7).
    const fn severity(level: AuditLevel) -> u8 {
        match level {
            AuditLevel::INFO => 6,  // Informational
            AuditLevel::WARN => 4,  // Warning
            AuditLevel::ERROR => 3, // Error
        }
    }

    /// Format an audit entry as an RFC 5424 message.
    fn format_message(&self, entry: &AuditEntry) -> String {
        let severity = Self::severity(entry.level);
        // Facility 10 (authpriv) * 8 + severity
        let priority = 80 + u16::from(severity);
        let timestamp = Utc::now().to_rfc3339();

        let error_part = entry
            .error
            .as_deref()
            .map_or(String::new(), |e| format!(" error=\"{}\"", escape_sd_value(e)));

        let sd = format!(
            "[{SD_ID} user=\"{}\" tenant=\"{}\" operation=\"{}\" level=\"{}\"{error_part}]",
            entry.user_id,
            entry.tenant_id,
            escape_sd_value(&entry.operation),
            entry.level.as_str(),
        );

        // Truncate query for the message body to avoid oversized syslog messages.
        let query_preview = if entry.query.len() > 200 {
            format!("{}...", &entry.query[..200])
        } else {
            entry.query.clone()
        };

        format!(
            "<{priority}>1 {timestamp} {} fraiseql - AUDIT {sd} {query_preview}",
            self.hostname,
        )
    }
}

#[async_trait]
impl AuditExporter for SyslogAuditExporter {
    async fn export(&self, entry: &AuditEntry) -> Result<(), AuditError> {
        let msg = self.format_message(entry);
        self.transport
            .lock()
            .send(msg.as_bytes())
            .map_err(|e| AuditError::Export(format!("syslog send: {e}")))?;
        Ok(())
    }

    async fn flush(&self) -> Result<(), AuditError> {
        // Syslog is not buffered; nothing to flush.
        Ok(())
    }
}

/// Escape characters that are special in RFC 5424 SD-VALUE: `"`, `\`, `]`.
fn escape_sd_value(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(']', "\\]")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::io::Read;
    use std::net::UdpSocket;

    use chrono::Utc;

    use super::*;

    fn test_entry() -> AuditEntry {
        AuditEntry {
            id:             Some(1),
            timestamp:      Utc::now(),
            level:          AuditLevel::INFO,
            user_id:        123,
            tenant_id:      456,
            operation:      "query".to_string(),
            query:          "{ users { id name } }".to_string(),
            variables:      serde_json::json!({}),
            ip_address:     "192.168.1.1".to_string(),
            user_agent:     "Mozilla/5.0".to_string(),
            error:          None,
            duration_ms:    Some(42),
            previous_hash:  None,
            integrity_hash: None,
        }
    }

    #[test]
    fn test_format_message_rfc5424() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let msg = exporter.format_message(&test_entry());

        // Priority = authpriv(10) * 8 + severity(6) = 86
        assert!(msg.starts_with("<86>1 "), "should start with priority 86, got: {msg}");
        assert!(msg.contains("test-host"));
        assert!(msg.contains("fraiseql"));
        assert!(msg.contains("AUDIT"));
        assert!(msg.contains(r#"user="123""#));
        assert!(msg.contains(r#"tenant="456""#));
        assert!(msg.contains(r#"operation="query""#));
        assert!(msg.contains(r#"level="INFO""#));
        assert!(msg.contains("{ users { id name } }"));
    }

    #[test]
    fn test_format_message_with_error() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let mut entry = test_entry();
        entry.level = AuditLevel::ERROR;
        entry.error = Some("timeout".to_string());

        let msg = exporter.format_message(&entry);

        // Priority = authpriv(10) * 8 + severity(3) = 83
        assert!(msg.starts_with("<83>1 "), "error priority should be 83");
        assert!(msg.contains(r#"error="timeout""#));
        assert!(msg.contains(r#"level="ERROR""#));
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::INFO), 6);
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::WARN), 4);
        assert_eq!(SyslogAuditExporter::severity(AuditLevel::ERROR), 3);
    }

    #[test]
    fn test_syslog_udp_export() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let recv_addr = receiver.local_addr().unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();

        let config = SyslogExportConfig {
            address:  "127.0.0.1".to_string(),
            port:     recv_addr.port(),
            protocol: "udp".to_string(),
        };

        let exporter = SyslogAuditExporter::new(&config).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(exporter.export(&test_entry())).unwrap();

        let mut buf = [0u8; 4096];
        let n = receiver.recv(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(received.contains("AUDIT"));
        assert!(received.contains(r#"user="123""#));
    }

    #[test]
    fn test_syslog_tcp_export() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let listen_addr = listener.local_addr().unwrap();

        let config = SyslogExportConfig {
            address:  "127.0.0.1".to_string(),
            port:     listen_addr.port(),
            protocol: "tcp".to_string(),
        };

        let exporter = SyslogAuditExporter::new(&config).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(exporter.export(&test_entry())).unwrap();

        let (mut conn, _) = listener.accept().unwrap();
        conn.set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();

        let mut buf = [0u8; 4096];
        let n = conn.read(&mut buf).unwrap();
        let received = std::str::from_utf8(&buf[..n]).unwrap();

        assert!(received.contains("AUDIT"));
        assert!(received.ends_with('\n'), "TCP syslog should end with newline");
    }

    #[test]
    fn test_escape_sd_value() {
        assert_eq!(escape_sd_value(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape_sd_value(r"a\b"), r"a\\b");
        assert_eq!(escape_sd_value("a]b"), r"a\]b");
    }

    #[test]
    fn test_long_query_truncated() {
        let exporter = SyslogAuditExporter {
            transport: Mutex::new(Transport::Udp(UdpSocket::bind("0.0.0.0:0").unwrap())),
            hostname:  "test-host".to_string(),
        };

        let mut entry = test_entry();
        entry.query = "x".repeat(500);

        let msg = exporter.format_message(&entry);
        // Query should be truncated to 200 chars + "..."
        assert!(msg.len() < 600, "message should be bounded");
        assert!(msg.contains("..."));
    }
}
