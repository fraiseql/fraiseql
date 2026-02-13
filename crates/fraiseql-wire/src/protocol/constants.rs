//! Postgres protocol constants

/// Protocol version 3.0
pub const PROTOCOL_VERSION: i32 = 0x0003_0000;

/// Message type tags
pub mod tags {
    /// Authentication request
    pub const AUTHENTICATION: u8 = b'R';

    /// Backend key data
    pub const BACKEND_KEY_DATA: u8 = b'K';

    /// Command complete
    pub const COMMAND_COMPLETE: u8 = b'C';

    /// Data row
    pub const DATA_ROW: u8 = b'D';

    /// Error response
    pub const ERROR_RESPONSE: u8 = b'E';

    /// Notice response
    pub const NOTICE_RESPONSE: u8 = b'N';

    /// Parameter status
    pub const PARAMETER_STATUS: u8 = b'S';

    /// Ready for query
    pub const READY_FOR_QUERY: u8 = b'Z';

    /// Row description
    pub const ROW_DESCRIPTION: u8 = b'T';
}

/// Authentication types
pub mod auth {
    /// Authentication successful
    pub const OK: i32 = 0;

    /// Cleartext password required
    pub const CLEARTEXT_PASSWORD: i32 = 3;

    /// MD5 password required
    pub const MD5_PASSWORD: i32 = 5;

    /// SASL mechanisms available (Postgres 10+)
    pub const SASL: i32 = 10;

    /// SASL server challenge
    pub const SASL_CONTINUE: i32 = 11;

    /// SASL server final message
    pub const SASL_FINAL: i32 = 12;
}

/// Transaction status
pub mod tx_status {
    /// Idle (not in transaction)
    pub const IDLE: u8 = b'I';

    /// In transaction block
    pub const IN_TRANSACTION: u8 = b'T';

    /// Failed transaction (queries will be rejected until END)
    pub const FAILED: u8 = b'E';
}
