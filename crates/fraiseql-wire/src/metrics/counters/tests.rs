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
