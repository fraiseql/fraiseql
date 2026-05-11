use super::*;

#[test]
fn test_current_chunk_size() {
    current_chunk_size("test_entity", 256);
    current_chunk_size("test_entity", 512);
    current_chunk_size("test_entity", 128);
}

#[test]
fn test_stream_buffered_items() {
    stream_buffered_items("test_entity", 0);
    stream_buffered_items("test_entity", 50);
    stream_buffered_items("test_entity", 256);
}
