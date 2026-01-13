//! Label constants for consistent metric labeling
//!
//! These constants ensure consistent label names and values across all metrics.

/// Label: entity name (table/view being queried)
pub const ENTITY: &str = "entity";

/// Label: error category (connection, protocol, json_decode, etc.)
pub const ERROR_CATEGORY: &str = "error_category";

/// Label: type name for deserialization metrics
pub const TYPE_NAME: &str = "type_name";

/// Label: transport type (tcp, unix)
pub const TRANSPORT: &str = "transport";

/// Label: authentication mechanism (cleartext, scram)
pub const MECHANISM: &str = "mechanism";

/// Label: result status (ok, error, filtered, etc.)
pub const STATUS: &str = "status";

/// Label: reason for failure or state
pub const REASON: &str = "reason";

/// Label: phase of execution (auth, startup, query, streaming)
pub const PHASE: &str = "phase";

/// Status value: ok
pub const STATUS_OK: &str = "ok";
/// Status value: error
pub const STATUS_ERROR: &str = "error";
/// Status value: filtered
pub const STATUS_FILTERED: &str = "filtered";
/// Status value: cancelled
pub const STATUS_CANCELLED: &str = "cancelled";

/// Transport value: TCP socket
pub const TRANSPORT_TCP: &str = "tcp";
/// Transport value: Unix domain socket
pub const TRANSPORT_UNIX: &str = "unix";

/// Mechanism value: cleartext password
pub const MECHANISM_CLEARTEXT: &str = "cleartext";
/// Mechanism value: SCRAM-SHA-256
pub const MECHANISM_SCRAM: &str = "scram";

/// Phase value: authentication
pub const PHASE_AUTH: &str = "auth";
/// Phase value: startup
pub const PHASE_STARTUP: &str = "startup";
/// Phase value: query execution
pub const PHASE_QUERY: &str = "query";
/// Phase value: streaming results
pub const PHASE_STREAMING: &str = "streaming";
/// Phase value: transport connection
pub const PHASE_TRANSPORT: &str = "transport";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_constants() {
        // Verify constants are not empty and reasonable
        assert!(!ENTITY.is_empty());
        assert!(!ERROR_CATEGORY.is_empty());
        assert!(!TYPE_NAME.is_empty());
        assert_eq!(ENTITY, "entity");
        assert_eq!(ERROR_CATEGORY, "error_category");
    }

    #[test]
    fn test_status_values() {
        assert_eq!(STATUS_OK, "ok");
        assert_eq!(STATUS_ERROR, "error");
        assert_eq!(STATUS_CANCELLED, "cancelled");
    }

    #[test]
    fn test_mechanism_values() {
        assert_eq!(MECHANISM_CLEARTEXT, "cleartext");
        assert_eq!(MECHANISM_SCRAM, "scram");
    }
}
