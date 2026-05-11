use super::*;

#[test]
fn test_query_startup_duration() {
    query_startup_duration("test_entity", 100);
    query_startup_duration("test_entity", 250);
    query_startup_duration("test_entity", 50);
}

#[test]
fn test_query_total_duration() {
    query_total_duration("test_entity", 1000);
    query_total_duration("test_entity", 500);
}

#[test]
fn test_query_rows_processed() {
    query_rows_processed("test_entity", 100);
    query_rows_processed("test_entity", 5000);
    query_rows_processed("test_entity", 1);
}

#[test]
fn test_chunk_processing_duration() {
    chunk_processing_duration("test_entity", 10);
    chunk_processing_duration("test_entity", 25);
}

#[test]
fn test_chunk_size() {
    chunk_size("test_entity", 256);
    chunk_size("test_entity", 128);
    chunk_size("test_entity", 42);
}

#[test]
fn test_deserialization_duration() {
    deserialization_duration("test_entity", "User", 5);
    deserialization_duration("test_entity", "Project", 8);
}

#[test]
fn test_auth_duration() {
    auth_duration(crate::metrics::labels::MECHANISM_SCRAM, 150);
    auth_duration(crate::metrics::labels::MECHANISM_CLEARTEXT, 10);
}

#[test]
fn test_channel_occupancy() {
    channel_occupancy("test_entity", 0);
    channel_occupancy("test_entity", 50);
    channel_occupancy("test_entity", 128);
    channel_occupancy("test_entity", 256);
    channel_occupancy("test_entity", 255);
}

#[test]
fn test_stream_pause_duration() {
    stream_pause_duration("test_entity", 0);
    stream_pause_duration("test_entity", 100);
    stream_pause_duration("test_entity", 5000);
}
