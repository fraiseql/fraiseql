#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
use super::*;

#[test]
fn test_graphql_query_ticket_roundtrip() {
    let ticket = FlightTicket::GraphQLQuery {
        query:     "{ users { id } }".to_string(),
        variables: None,
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    assert_eq!(ticket, decoded);
}

#[test]
fn test_graphql_query_with_variables() {
    let ticket = FlightTicket::GraphQLQuery {
        query:     "query($id: ID!) { user(id: $id) { name } }".to_string(),
        variables: Some(serde_json::json!({"id": "123"})),
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    match decoded {
        FlightTicket::GraphQLQuery { query, variables } => {
            assert_eq!(query, "query($id: ID!) { user(id: $id) { name } }");
            assert_eq!(variables, Some(serde_json::json!({"id": "123"})));
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_observer_events_ticket_roundtrip() {
    let ticket = FlightTicket::ObserverEvents {
        entity_type: "Order".to_string(),
        start_date:  Some("2026-01-01".to_string()),
        end_date:    Some("2026-01-31".to_string()),
        limit:       Some(10_000),
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    match decoded {
        FlightTicket::ObserverEvents {
            entity_type, limit, ..
        } => {
            assert_eq!(entity_type, "Order");
            assert_eq!(limit, Some(10_000));
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_optimized_view_ticket() {
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   Some("created_at > '2026-01-01'".to_string()),
        order_by: Some("created_at DESC".to_string()),
        limit:    Some(100_000),
        offset:   Some(0),
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    match decoded {
        FlightTicket::OptimizedView {
            view,
            filter,
            order_by,
            limit,
            offset,
        } => {
            assert_eq!(view, "va_orders");
            assert_eq!(filter, Some("created_at > '2026-01-01'".to_string()));
            assert_eq!(order_by, Some("created_at DESC".to_string()));
            assert_eq!(limit, Some(100_000));
            assert_eq!(offset, Some(0));
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_optimized_view_minimal() {
    let ticket = FlightTicket::OptimizedView {
        view:     "va_users".to_string(),
        filter:   None,
        order_by: None,
        limit:    None,
        offset:   None,
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    assert_eq!(ticket, decoded);
}

#[test]
fn test_bulk_export_ticket() {
    let ticket = FlightTicket::BulkExport {
        table:  "users".to_string(),
        filter: Some("active = true".to_string()),
        limit:  Some(1_000_000),
        format: Some("parquet".to_string()),
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    assert_eq!(ticket, decoded);
}

#[test]
fn test_invalid_ticket_returns_error() {
    let invalid_json = b"not valid json";
    let result = FlightTicket::decode(invalid_json);

    assert!(
        matches!(result, Err(ArrowFlightError::InvalidTicket(_))),
        "expected InvalidTicket error for invalid JSON, got: {result:?}"
    );
}

#[test]
fn test_batched_queries_ticket() {
    let ticket = FlightTicket::BatchedQueries {
        queries: vec![
            "SELECT * FROM ta_users LIMIT 100".to_string(),
            "SELECT * FROM ta_orders LIMIT 50".to_string(),
        ],
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    match decoded {
        FlightTicket::BatchedQueries { queries } => {
            assert_eq!(queries.len(), 2);
            assert_eq!(queries[0], "SELECT * FROM ta_users LIMIT 100");
            assert_eq!(queries[1], "SELECT * FROM ta_orders LIMIT 50");
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_batched_queries_single() {
    let ticket = FlightTicket::BatchedQueries {
        queries: vec!["SELECT COUNT(*) FROM ta_users".to_string()],
    };

    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();

    assert_eq!(ticket, decoded);
}

// --- Additional ticket tests ---

#[test]
fn test_graphql_query_empty_string_roundtrips() {
    let ticket = FlightTicket::GraphQLQuery {
        query:     String::new(),
        variables: None,
    };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    match decoded {
        FlightTicket::GraphQLQuery { query, variables } => {
            assert!(query.is_empty());
            assert!(variables.is_none());
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_observer_events_all_none_optional_fields() {
    let ticket = FlightTicket::ObserverEvents {
        entity_type: "User".to_string(),
        start_date:  None,
        end_date:    None,
        limit:       None,
    };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    match decoded {
        FlightTicket::ObserverEvents {
            entity_type,
            start_date,
            end_date,
            limit,
        } => {
            assert_eq!(entity_type, "User");
            assert!(start_date.is_none());
            assert!(end_date.is_none());
            assert!(limit.is_none());
        },
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_bulk_export_all_none_optional_fields() {
    let ticket = FlightTicket::BulkExport {
        table:  "orders".to_string(),
        filter: None,
        limit:  None,
        format: None,
    };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    assert_eq!(ticket, decoded);
}

#[test]
fn test_batched_queries_empty_list_roundtrips() {
    let ticket = FlightTicket::BatchedQueries { queries: vec![] };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    match decoded {
        FlightTicket::BatchedQueries { queries } => assert!(queries.is_empty()),
        _ => panic!("Wrong ticket type"),
    }
}

#[test]
fn test_invalid_json_with_valid_utf8_returns_error() {
    // Valid UTF-8, but not valid JSON
    let bad_bytes = b"{ not valid JSON at all }";
    let result = FlightTicket::decode(bad_bytes);
    assert!(
        matches!(result, Err(ArrowFlightError::InvalidTicket(_))),
        "expected InvalidTicket error for invalid JSON with valid UTF-8, got: {result:?}"
    );
}

#[test]
fn test_valid_json_but_wrong_type_tag_returns_error() {
    // JSON with an unknown "type" tag
    let bytes = br#"{"type": "UnknownVariant", "data": 42}"#;
    let result = FlightTicket::decode(bytes);
    assert!(
        matches!(result, Err(ArrowFlightError::InvalidTicket(_))),
        "expected InvalidTicket error for unknown type tag, got: {result:?}"
    );
}

#[test]
fn test_encode_produces_valid_utf8_json() {
    let ticket = FlightTicket::GraphQLQuery {
        query:     "{ users { id } }".to_string(),
        variables: None,
    };
    let bytes = ticket.encode().unwrap();
    let s = String::from_utf8(bytes).expect("encoded bytes should be valid UTF-8");
    // JSON must contain the type tag
    assert!(s.contains("GraphQLQuery"));
}

#[test]
fn test_optimized_view_offset_zero_roundtrips() {
    let ticket = FlightTicket::OptimizedView {
        view:     "va_orders".to_string(),
        filter:   None,
        order_by: None,
        limit:    Some(1000),
        offset:   Some(0),
    };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    assert_eq!(ticket, decoded);
}

#[test]
fn test_graphql_query_with_complex_variables_roundtrips() {
    let ticket = FlightTicket::GraphQLQuery {
        query:     "query Q($filter: FilterInput!) { items(filter: $filter) { id } }".to_string(),
        variables: Some(serde_json::json!({
            "filter": {
                "status": "active",
                "ids": [1, 2, 3],
                "nested": {"level": 2}
            }
        })),
    };
    let bytes = ticket.encode().unwrap();
    let decoded = FlightTicket::decode(&bytes).unwrap();
    assert_eq!(ticket, decoded);
}

#[test]
fn test_ticket_exactly_at_size_limit_is_accepted() {
    // A payload of MAX_FLIGHT_TICKET_BYTES is accepted (boundary value).
    let bytes = vec![b'{'; MAX_FLIGHT_TICKET_BYTES];
    // This will fail JSON parsing but NOT the size check.
    let result = FlightTicket::decode(&bytes);
    // The error must be an InvalidTicket (parse error), not a size error.
    match result {
        Err(ArrowFlightError::InvalidTicket(ref msg)) => {
            assert!(!msg.contains("too large"), "Should fail JSON parsing, not size limit: {msg}");
        },
        other => panic!("expected InvalidTicket parse error, got: {other:?}"),
    }
}

#[test]
fn test_ticket_exceeding_size_limit_is_rejected() {
    let oversized = vec![b'x'; MAX_FLIGHT_TICKET_BYTES + 1];
    let result = FlightTicket::decode(&oversized);
    match result {
        Err(ArrowFlightError::InvalidTicket(ref msg)) => {
            assert!(msg.contains("too large"), "Expected size-limit error, got: {msg}");
        },
        other => panic!("expected InvalidTicket size-limit error, got: {other:?}"),
    }
}
