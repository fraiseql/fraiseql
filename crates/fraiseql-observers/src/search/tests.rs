#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports

#[cfg(test)]
mod search_tests {
    use uuid::Uuid;

    use crate::event::EntityEvent;
    use crate::search::*;

    #[test]
    fn test_indexed_event_creation() {
        let event = EntityEvent::new(
            crate::event::EventKind::Created,
            "Order".to_string(),
            Uuid::new_v4(),
            serde_json::json!({"total": 100}),
        );

        let indexed = IndexedEvent::from_event(
            &event,
            "tenant-1".to_string(),
            vec!["email".to_string()],
            1,
            0,
        );

        assert_eq!(indexed.entity_type, "Order");
        assert_eq!(indexed.success_count, 1);
        assert_eq!(indexed.failure_count, 0);
        assert_eq!(indexed.tenant_id, "tenant-1");
        assert!(!indexed.search_text.is_empty());
    }

    #[test]
    fn test_indexed_event_index_name() {
        let event = EntityEvent::new(
            crate::event::EventKind::Updated,
            "User".to_string(),
            Uuid::new_v4(),
            serde_json::json!({}),
        );

        let indexed = IndexedEvent::from_event(&event, "tenant-1".to_string(), vec![], 0, 0);

        let index_name = indexed.index_name();
        assert!(index_name.starts_with("events-"));
        assert!(index_name.len() >= 15); // events-YYYY-MM-DD
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_search_stats_new() {
        let stats = SearchStats::new();
        assert_eq!(stats.total_indexed, 0);
        assert_eq!(stats.successful_indexes, 0);
        assert_eq!(stats.failed_indexes, 0);
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_search_stats_record_success() {
        let mut stats = SearchStats::new();
        stats.record(true, 15.0);
        stats.record(true, 20.0);

        assert_eq!(stats.total_indexed, 2);
        assert_eq!(stats.successful_indexes, 2);
        assert_eq!(stats.failed_indexes, 0);
        assert!((stats.avg_index_latency_ms - 17.5).abs() < f64::EPSILON);
        assert!((stats.success_rate() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_search_stats_record_failure() {
        let mut stats = SearchStats::new();
        stats.record(true, 10.0);
        stats.record(false, 0.0);

        assert_eq!(stats.total_indexed, 2);
        assert_eq!(stats.successful_indexes, 1);
        assert_eq!(stats.failed_indexes, 1);
        assert!((stats.success_rate() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    #[allow(clippy::float_cmp)] // Reason: exact comparison is intentional in tests
    fn test_search_stats_reset() {
        let mut stats = SearchStats::new();
        stats.record(true, 10.0);
        stats.record(false, 0.0);

        stats.reset();

        assert_eq!(stats.total_indexed, 0);
        assert_eq!(stats.successful_indexes, 0);
        assert_eq!(stats.failed_indexes, 0);
        assert_eq!(stats.success_rate(), 0.0);
    }
}
