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

use std::{
    io::Write,
    net::{TcpStream, UdpSocket},
};

use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;

use super::audit::{AuditEntry, AuditError, AuditExporter, AuditLevel, SyslogExportConfig};

/// IANA Private Enterprise Number placeholder for structured data.
const SD_ID: &str = "audit@49152";

/// Maximum UDP message size (RFC 5426 recommends 2048 for syslog over UDP).
const MAX_UDP_MSG_LEN: usize = 2048;

/// Syslog transport.
pub(crate) enum Transport {
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
    pub(crate) transport: Mutex<Transport>,
    pub(crate) hostname: String,
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
    pub(crate) const fn severity(level: AuditLevel) -> u8 {
        match level {
            AuditLevel::INFO => 6,  // Informational
            AuditLevel::WARN => 4,  // Warning
            AuditLevel::ERROR => 3, // Error
        }
    }

    /// Format an audit entry as an RFC 5424 message.
    pub(crate) fn format_message(&self, entry: &AuditEntry) -> String {
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
pub(crate) fn escape_sd_value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace(']', "\\]")
}
