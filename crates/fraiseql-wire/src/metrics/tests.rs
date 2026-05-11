use super::*;

#[test]
fn test_labels_exported() {
    // Verify public API
    let _entity = labels::ENTITY;
    let _error = labels::ERROR_CATEGORY;
    let _type_name = labels::TYPE_NAME;
}

#[test]
fn test_counters_exported() {
    // Verify counters are callable (won't panic)
    counters::query_submitted("test", true, false, true);
    counters::query_success("test");
}

#[test]
fn test_histograms_exported() {
    // Verify histograms are callable (won't panic)
    histograms::query_startup_duration("test", 100);
    histograms::chunk_processing_duration("test", 50);
}

#[test]
fn test_gauges_exported() {
    // Verify gauges are callable (won't panic)
    gauges::current_chunk_size("test", 256);
    gauges::stream_buffered_items("test", 50);
}
