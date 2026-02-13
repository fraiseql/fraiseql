//! Protocol message types

use bytes::Bytes;

/// Frontend message (client → server)
#[derive(Debug, Clone)]
pub enum FrontendMessage {
    /// Startup message
    Startup {
        /// Protocol version
        version: i32,
        /// Connection parameters
        params: Vec<(String, String)>,
    },

    /// Password message
    Password(String),

    /// Query message
    Query(String),

    /// Terminate message
    Terminate,

    /// SASL initial response message
    SaslInitialResponse {
        /// SASL mechanism name (e.g., "SCRAM-SHA-256")
        mechanism: String,
        /// SASL client first message data
        data: Vec<u8>,
    },

    /// SASL response message
    SaslResponse {
        /// SASL client final message data
        data: Vec<u8>,
    },
}

/// Backend message (server → client)
#[derive(Debug, Clone)]
pub enum BackendMessage {
    /// Authentication request
    Authentication(AuthenticationMessage),

    /// Backend key data (for cancellation)
    BackendKeyData {
        /// Process ID
        process_id: i32,
        /// Secret key
        secret_key: i32,
    },

    /// Command complete
    CommandComplete(String),

    /// Data row
    DataRow(Vec<Option<Bytes>>),

    /// Error response
    ErrorResponse(ErrorFields),

    /// Notice response
    NoticeResponse(ErrorFields),

    /// Parameter status
    ParameterStatus {
        /// Parameter name
        name: String,
        /// Parameter value
        value: String,
    },

    /// Ready for query
    ReadyForQuery {
        /// Transaction status
        status: u8,
    },

    /// Row description
    RowDescription(Vec<FieldDescription>),
}

/// Authentication message types
#[derive(Debug, Clone)]
pub enum AuthenticationMessage {
    /// Authentication OK
    Ok,

    /// Cleartext password required
    CleartextPassword,

    /// MD5 password required
    Md5Password {
        /// Salt for MD5 hash
        salt: [u8; 4],
    },

    /// SASL authentication mechanisms available (Postgres 10+)
    Sasl {
        /// List of SASL mechanism names (e.g., ["SCRAM-SHA-256"])
        mechanisms: Vec<String>,
    },

    /// SASL continuation message (server challenge)
    SaslContinue {
        /// SASL server first/continue message data
        data: Vec<u8>,
    },

    /// SASL final message (server verification)
    SaslFinal {
        /// SASL server final message data
        data: Vec<u8>,
    },
}

/// Field description (column metadata)
#[derive(Debug, Clone)]
pub struct FieldDescription {
    /// Column name
    pub name: String,
    /// Table OID (0 if not a table column)
    pub table_oid: i32,
    /// Column attribute number (0 if not a table column)
    pub column_attr: i16,
    /// Data type OID
    pub type_oid: u32,
    /// Data type size
    pub type_size: i16,
    /// Type modifier
    pub type_modifier: i32,
    /// Format code (0 = text, 1 = binary)
    pub format_code: i16,
}

/// Error/notice fields
#[derive(Debug, Clone, Default)]
pub struct ErrorFields {
    /// Severity (ERROR, WARNING, etc.)
    pub severity: Option<String>,
    /// SQLSTATE code
    pub code: Option<String>,
    /// Human-readable message
    pub message: Option<String>,
    /// Additional detail
    pub detail: Option<String>,
    /// Hint
    pub hint: Option<String>,
    /// Position in query string
    pub position: Option<String>,
}

impl std::fmt::Display for ErrorFields {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref msg) = self.message {
            write!(f, "{}", msg)?;
        }
        if let Some(ref code) = self.code {
            write!(f, " ({})", code)?;
        }
        Ok(())
    }
}
