//! Counter metrics for fraiseql-wire
//!
//! Counters track counts of events that only increase over time:
//! - Queries submitted, completed, failed
//! - Errors by category
//! - Rows processed, filtered, deserialized
//! - Authentication attempts and successes

use crate::metrics::labels;
use metrics::counter;

/// Record a query submission
pub fn query_submitted(
    entity: &str,
    has_where_sql: bool,
    has_where_rust: bool,
    has_order_by: bool,
) {
    counter!(
        "fraiseql_queries_total",
        labels::ENTITY => entity.to_string(),
        "has_where_sql" => has_where_sql.to_string(),
        "has_where_rust" => has_where_rust.to_string(),
        "has_order_by" => has_order_by.to_string(),
    )
    .increment(1);
}

/// Record a successful query completion
pub fn query_success(entity: &str) {
    counter!(
        "fraiseql_query_success_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record a failed query
pub fn query_error(entity: &str, error_category: &str) {
    counter!(
        "fraiseql_query_error_total",
        labels::ENTITY => entity.to_string(),
        labels::ERROR_CATEGORY => error_category.to_string(),
    )
    .increment(1);
}

/// Record a cancelled query
pub fn query_cancelled(entity: &str) {
    counter!(
        "fraiseql_query_cancelled_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record query completion with status (success, error, cancelled)
pub fn query_completed(status: &str, entity: &str) {
    counter!(
        "fraiseql_query_completed_total",
        labels::ENTITY => entity.to_string(),
        labels::STATUS => status.to_string(),
    )
    .increment(1);
}

/// Record rows processed from the database
pub fn rows_processed(entity: &str, count: u64, status: &str) {
    counter!(
        "fraiseql_rows_processed_total",
        labels::ENTITY => entity.to_string(),
        labels::STATUS => status.to_string(),
    )
    .increment(count);
}

/// Record rows filtered by Rust predicates
pub fn rows_filtered(entity: &str, count: u64) {
    counter!(
        "fraiseql_rows_filtered_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(count);
}

/// Record successful deserialization
pub fn deserialization_success(entity: &str, type_name: &str) {
    counter!(
        "fraiseql_rows_deserialized_total",
        labels::ENTITY => entity.to_string(),
        labels::TYPE_NAME => type_name.to_string(),
    )
    .increment(1);
}

/// Record deserialization failure
pub fn deserialization_failure(entity: &str, type_name: &str, reason: &str) {
    counter!(
        "fraiseql_rows_deserialization_failed_total",
        labels::ENTITY => entity.to_string(),
        labels::TYPE_NAME => type_name.to_string(),
        labels::REASON => reason.to_string(),
    )
    .increment(1);
}

/// Record a generic error
pub fn error_occurred(category: &str, phase: &str) {
    counter!(
        "fraiseql_errors_total",
        labels::ERROR_CATEGORY => category.to_string(),
        labels::PHASE => phase.to_string(),
    )
    .increment(1);
}

/// Record a protocol error
pub fn protocol_error(message_type: &str) {
    counter!(
        "fraiseql_protocol_errors_total",
        "message_type" => message_type.to_string(),
    )
    .increment(1);
}

/// Record a JSON parsing error by entity
pub fn json_parse_error(entity: &str) {
    counter!(
        "fraiseql_json_parse_errors_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record connection creation
pub fn connection_created(transport: &str) {
    counter!(
        "fraiseql_connections_created_total",
        labels::TRANSPORT => transport.to_string(),
    )
    .increment(1);
}

/// Record connection failure
pub fn connection_failed(phase: &str, error_category: &str) {
    counter!(
        "fraiseql_connections_failed_total",
        labels::PHASE => phase.to_string(),
        labels::ERROR_CATEGORY => error_category.to_string(),
    )
    .increment(1);
}

/// Record authentication attempt
pub fn auth_attempted(mechanism: &str) {
    counter!(
        "fraiseql_authentications_total",
        labels::MECHANISM => mechanism.to_string(),
    )
    .increment(1);
}

/// Record successful authentication
pub fn auth_successful(mechanism: &str) {
    counter!(
        "fraiseql_authentications_successful_total",
        labels::MECHANISM => mechanism.to_string(),
    )
    .increment(1);
}

/// Record failed authentication
pub fn auth_failed(mechanism: &str, reason: &str) {
    counter!(
        "fraiseql_authentications_failed_total",
        labels::MECHANISM => mechanism.to_string(),
        labels::REASON => reason.to_string(),
    )
    .increment(1);
}

/// Record memory limit exceeded event
pub fn memory_limit_exceeded(entity: &str) {
    counter!(
        "fraiseql_memory_limit_exceeded_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record an adaptive chunk size adjustment
///
/// Called when adaptive chunking decides to increase or decrease chunk_size
/// based on channel occupancy observations.
///
/// # Labels
/// - `entity`: The query entity (project, etc.)
/// - `direction`: Either "increase" or "decrease"
/// - `old_size`: The previous chunk size (e.g., "256")
/// - `new_size`: The new chunk size after adjustment (e.g., "384")
///
/// # Example
/// ```ignore
/// adaptive_chunk_adjusted("projects", 256, 384);
/// ```
pub fn adaptive_chunk_adjusted(entity: &str, old_size: usize, new_size: usize) {
    let direction = if new_size > old_size {
        "increase"
    } else {
        "decrease"
    };

    counter!(
        "fraiseql_adaptive_chunk_adjusted_total",
        labels::ENTITY => entity.to_string(),
        "direction" => direction.to_string(),
        "old_size" => old_size.to_string(),
        "new_size" => new_size.to_string(),
    )
    .increment(1);
}

/// Record a stream pause event
///
/// Called when a stream is paused by the user.
///
/// # Labels
/// - `entity`: The query entity (project, etc.)
///
/// # Example
/// ```ignore
/// stream_paused("projects");
/// ```
pub fn stream_paused(entity: &str) {
    counter!(
        "fraiseql_stream_paused_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record a stream resume event
///
/// Called when a paused stream is resumed by the user.
///
/// # Labels
/// - `entity`: The query entity (project, etc.)
///
/// # Example
/// ```ignore
/// stream_resumed("projects");
/// ```
pub fn stream_resumed(entity: &str) {
    counter!(
        "fraiseql_stream_resumed_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

/// Record a stream pause timeout expiry (auto-resume)
pub fn stream_pause_timeout_expired(entity: &str) {
    counter!(
        "fraiseql_stream_pause_timeout_expired_total",
        labels::ENTITY => entity.to_string(),
    )
    .increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_submitted() {
        // Should not panic when called
        query_submitted("test_entity", true, false, true);
    }

    #[test]
    fn test_query_success() {
        query_success("test_entity");
    }

    #[test]
    fn test_query_error() {
        query_error("test_entity", "connection");
    }

    #[test]
    fn test_rows_processed() {
        rows_processed("test_entity", 100, "ok");
        rows_processed("test_entity", 5, "error");
    }

    #[test]
    fn test_error_occurred() {
        error_occurred("protocol", labels::PHASE_QUERY);
    }

    #[test]
    fn test_auth_attempted() {
        auth_attempted(labels::MECHANISM_SCRAM);
    }

    #[test]
    fn test_memory_limit_exceeded() {
        memory_limit_exceeded("test_entity");
    }

    #[test]
    fn test_adaptive_chunk_adjusted_increase() {
        adaptive_chunk_adjusted("test_entity", 256, 384);
    }

    #[test]
    fn test_adaptive_chunk_adjusted_decrease() {
        adaptive_chunk_adjusted("test_entity", 256, 170);
    }

    #[test]
    fn test_stream_paused() {
        stream_paused("test_entity");
    }

    #[test]
    fn test_stream_resumed() {
        stream_resumed("test_entity");
    }
}
