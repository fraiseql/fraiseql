#![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

use super::*;

#[test]
fn test_query_request_serialization() {
    let msg = ExchangeMessage::Request {
        correlation_id: "req-1".to_string(),
        request_type:   RequestType::Query {
            query:     "{ orders { id total } }".to_string(),
            variables: None,
        },
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Request {
            correlation_id,
            request_type: RequestType::Query { query, variables },
        } => {
            assert_eq!(correlation_id, "req-1");
            assert_eq!(query, "{ orders { id total } }");
            assert!(variables.is_none());
        },
        _ => panic!("Expected Query request"),
    }
}

#[test]
fn test_response_serialization() {
    let msg = ExchangeMessage::Response {
        correlation_id: "req-1".to_string(),
        result:         Ok(vec![1, 2, 3, 4]),
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Response {
            correlation_id,
            result,
        } => {
            assert_eq!(correlation_id, "req-1");
            assert_eq!(result, Ok(vec![1, 2, 3, 4]));
        },
        _ => panic!("Expected Response"),
    }
}

#[test]
fn test_error_response_serialization() {
    let msg = ExchangeMessage::Response {
        correlation_id: "req-1".to_string(),
        result:         Err("Database error".to_string()),
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Response {
            correlation_id,
            result,
        } => {
            assert_eq!(correlation_id, "req-1");
            assert_eq!(result, Err("Database error".to_string()));
        },
        _ => panic!("Expected Response"),
    }
}

#[test]
fn test_complete_serialization() {
    let msg = ExchangeMessage::Complete {
        correlation_id: "stream-complete".to_string(),
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Complete { correlation_id } => {
            assert_eq!(correlation_id, "stream-complete");
        },
        _ => panic!("Expected Complete"),
    }
}

#[test]
fn test_upload_request_serialization() {
    let batch_data = vec![1, 2, 3, 4, 5];
    let msg = ExchangeMessage::Request {
        correlation_id: "upload-1".to_string(),
        request_type:   RequestType::Upload {
            table: "orders".to_string(),
            batch: batch_data.clone(),
        },
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Request {
            correlation_id,
            request_type: RequestType::Upload { table, batch },
        } => {
            assert_eq!(correlation_id, "upload-1");
            assert_eq!(table, "orders");
            assert_eq!(batch, batch_data);
        },
        _ => panic!("Expected Upload request"),
    }
}

#[test]
fn test_query_with_variables_serialization() {
    let variables = serde_json::json!({
        "customerId": 123,
        "status": "pending"
    });

    let msg = ExchangeMessage::Request {
        correlation_id: "query-with-vars".to_string(),
        request_type: RequestType::Query {
            query: "query($customerId: ID!, $status: String) { orders(customerId: $customerId, status: $status) { id } }"
                .to_string(),
            variables: Some(variables.clone()),
        },
    };

    let bytes = msg.to_json_bytes().expect("Failed to serialize");
    let deserialized = ExchangeMessage::from_json_bytes(&bytes).expect("Failed to deserialize");

    match deserialized {
        ExchangeMessage::Request {
            correlation_id,
            request_type:
                RequestType::Query {
                    query,
                    variables: Some(vars),
                },
        } => {
            assert_eq!(correlation_id, "query-with-vars");
            assert!(query.contains("customerId"));
            assert_eq!(vars, variables);
        },
        _ => panic!("Expected Query request with variables"),
    }
}

#[test]
fn test_exchange_message_at_size_limit_is_rejected_as_json_not_size() {
    // Exactly at MAX_EXCHANGE_MESSAGE_BYTES: size check passes, JSON parse fails.
    let bytes = vec![b'{'; MAX_EXCHANGE_MESSAGE_BYTES];
    let result = ExchangeMessage::from_json_bytes(&bytes);
    assert!(result.is_err(), "expected Err, got: {result:?}");
    let msg = result.unwrap_err();
    assert!(!msg.contains("too large"), "Should fail JSON parsing, not size limit: {msg}");
}

#[test]
fn test_exchange_message_exceeding_size_limit_is_rejected() {
    let oversized = vec![b'x'; MAX_EXCHANGE_MESSAGE_BYTES + 1];
    let result = ExchangeMessage::from_json_bytes(&oversized);
    assert!(result.is_err(), "expected Err, got: {result:?}");
    let msg = result.unwrap_err();
    assert!(msg.contains("too large"), "Expected size-limit error, got: {msg}");
}
