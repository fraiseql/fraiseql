#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_representations() {
        let input = json!([
            {"__typename": "User", "id": "123"},
            {"__typename": "User", "id": "456"},
        ]);

        let metadata = FederationMetadata::default();
        let reps = parse_representations(&input, &metadata).unwrap();

        assert_eq!(reps.len(), 2);
        assert_eq!(reps[0].typename, "User");
        assert_eq!(reps[1].typename, "User");
    }

    #[test]
    fn test_parse_representations_invalid() {
        let input = json!("not an array");

        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);

        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for non-array input, got: {result:?}"
        );
    }

    #[test]
    fn test_parse_representations_missing_typename() {
        let input = json!([
            {"id": "123"},
        ]);

        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);

        assert!(
            matches!(result, Err(FraiseQLError::Validation { .. })),
            "expected Validation error for missing __typename, got: {result:?}"
        );
    }

    // ── Batch-size guard tests ─────────────────────────────────────────────────

    #[test]
    fn test_parse_representations_at_max_accepted() {
        // MAX_ENTITIES_BATCH_SIZE items must be accepted.
        let items: Vec<_> = (0..MAX_ENTITIES_BATCH_SIZE)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        assert!(result.is_ok(), "exactly MAX_ENTITIES_BATCH_SIZE reps must be accepted");
        assert_eq!(result.unwrap().len(), MAX_ENTITIES_BATCH_SIZE);
    }

    #[test]
    fn test_parse_representations_exceeding_max_rejected() {
        // MAX_ENTITIES_BATCH_SIZE + 1 items must be rejected before any parsing.
        let items: Vec<_> = (0..=MAX_ENTITIES_BATCH_SIZE)
            .map(|i| json!({"__typename": "User", "id": i.to_string()}))
            .collect();
        let input = Value::Array(items);
        let metadata = FederationMetadata::default();
        let result = parse_representations(&input, &metadata);
        assert!(result.is_err(), "batch exceeding max must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("1000") || msg.contains("1001"),
            "error must mention the count: {msg}"
        );
    }
