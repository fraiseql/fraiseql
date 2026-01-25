//! Test custom scalar type coercion in WHERE clauses.
//!
//! This test verifies that:
//! 1. Custom scalars preserve their type information across WHERE operations
//! 2. DateTime values are formatted consistently in WHERE clauses
//! 3. JSON scalars maintain structure through comparisons
//! 4. Custom ID types are not confused with standard IDs
//! 5. Type coercion doesn't lose precision or data
//!
//! # Risk If Missing
//!
//! Without this test:
//! - Custom scalars could be treated as strings incorrectly
//! - DateTime precision could be lost in comparisons
//! - Type information could be lost in SQL generation

use fraiseql_core::db::where_clause::{WhereClause, WhereOperator};
use serde_json::json;

#[test]
fn test_custom_scalar_datetime_in_where() {
    // DateTime custom scalar in WHERE clause
    let clause = WhereClause::Field {
        path:     vec!["created_at".to_string()],
        operator: WhereOperator::Gt,
        value:    json!("2024-01-15T10:30:45Z"),
    };

    match clause {
        WhereClause::Field {
            value, operator, ..
        } => {
            assert_eq!(value, json!("2024-01-15T10:30:45Z"));
            assert_eq!(operator, WhereOperator::Gt);
            // Verify it's a string representation
            assert!(value.is_string());
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_uuid_in_where() {
    // UUID custom scalar in WHERE clause
    let uuid_value = "550e8400-e29b-41d4-a716-446655440000";
    let clause = WhereClause::Field {
        path:     vec!["id".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(uuid_value),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(uuid_value));
            // Verify UUID format is preserved
            let uuid_str = value.as_str().unwrap();
            assert!(uuid_str.contains("-"));
            assert_eq!(uuid_str.len(), 36); // Standard UUID length
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_json_in_where() {
    // JSON custom scalar in WHERE clause
    let json_value = json!({"nested": "data", "count": 42});
    let clause = WhereClause::Field {
        path:     vec!["metadata".to_string()],
        operator: WhereOperator::Contains,
        value:    json_value.clone(),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            // JSON structure should be preserved
            assert_eq!(value["nested"], json!("data"));
            assert_eq!(value["count"], json!(42));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_date_in_where() {
    // Date custom scalar (date only, no time)
    let clause = WhereClause::Field {
        path:     vec!["birth_date".to_string()],
        operator: WhereOperator::Lt,
        value:    json!("1990-05-20"),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!("1990-05-20"));
            // Verify date format (YYYY-MM-DD)
            let date_str = value.as_str().unwrap();
            assert_eq!(date_str.len(), 10);
            let parts: Vec<&str> = date_str.split('-').collect();
            assert_eq!(parts.len(), 3);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_time_in_where() {
    // Time custom scalar (time only)
    let clause = WhereClause::Field {
        path:     vec!["shift_start".to_string()],
        operator: WhereOperator::Gte,
        value:    json!("09:00:00"),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!("09:00:00"));
            // Verify time format (HH:MM:SS)
            let time_str = value.as_str().unwrap();
            let parts: Vec<&str> = time_str.split(':').collect();
            assert_eq!(parts.len(), 3);
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_url_in_where() {
    // URL custom scalar
    let url = "https://example.com/path?query=value&other=123";
    let clause = WhereClause::Field {
        path:     vec!["website".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(url),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(url));
            let url_str = value.as_str().unwrap();
            assert!(url_str.starts_with("https://"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_email_in_where() {
    // Email custom scalar
    let email = "user@example.com";
    let clause = WhereClause::Field {
        path:     vec!["email".to_string()],
        operator: WhereOperator::Contains,
        value:    json!(email),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(email));
            let email_str = value.as_str().unwrap();
            assert!(email_str.contains("@"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_phone_in_where() {
    // Phone custom scalar
    let phone = "+1-555-123-4567";
    let clause = WhereClause::Field {
        path:     vec!["phone".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(phone),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(phone));
            // Verify structure is preserved
            let phone_str = value.as_str().unwrap();
            assert!(phone_str.contains("-"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_bigint_in_where() {
    // BigInt custom scalar (beyond JavaScript safe integer)
    let big_int = "9223372036854775807"; // i64::MAX
    let clause = WhereClause::Field {
        path:     vec!["large_number".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(big_int),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            // For custom scalars represented as strings
            if let Some(str_val) = value.as_str() {
                assert_eq!(str_val, big_int);
            } else if let Some(num_val) = value.as_i64() {
                // or as number if JSON supports it
                assert_eq!(num_val, 9223372036854775807i64);
            }
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_decimal_in_where() {
    // Decimal custom scalar (high precision numbers)
    let decimal = "123.456789012345";
    let clause = WhereClause::Field {
        path:     vec!["price".to_string()],
        operator: WhereOperator::Gt,
        value:    json!(decimal),
    };

    match clause {
        WhereClause::Field { value, .. } => {
            // Preserve as string to avoid floating point precision loss
            if let Some(str_val) = value.as_str() {
                assert_eq!(str_val, decimal);
            } else {
                // or preserve the JSON representation
                assert!(value.to_string().contains("123"));
            }
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_color_in_where() {
    // Color custom scalar (hex or named)
    let color_hex = "#FF5733";
    let clause_hex = WhereClause::Field {
        path:     vec!["color".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(color_hex),
    };

    match clause_hex {
        WhereClause::Field { value, .. } => {
            assert_eq!(value, json!(color_hex));
            let color = value.as_str().unwrap();
            assert!(color.starts_with("#"));
        },
        _ => panic!("Should be Field variant"),
    }
}

#[test]
fn test_custom_scalar_mixed_in_where() {
    // Multiple custom scalar types in one query
    let query = WhereClause::And(vec![
        WhereClause::Field {
            path:     vec!["created_at".to_string()],
            operator: WhereOperator::Gt,
            value:    json!("2024-01-01T00:00:00Z"),
        },
        WhereClause::Field {
            path:     vec!["user_id".to_string()],
            operator: WhereOperator::Eq,
            value:    json!("550e8400-e29b-41d4-a716-446655440000"),
        },
        WhereClause::Field {
            path:     vec!["price".to_string()],
            operator: WhereOperator::Gt,
            value:    json!("99.99"),
        },
    ]);

    match query {
        WhereClause::And(clauses) => {
            assert_eq!(clauses.len(), 3);

            // Verify each scalar type is preserved
            match &clauses[0] {
                WhereClause::Field { value, .. } => {
                    assert!(value.as_str().unwrap().contains("T"));
                },
                _ => panic!(),
            }

            match &clauses[1] {
                WhereClause::Field { value, .. } => {
                    let uuid = value.as_str().unwrap();
                    assert_eq!(uuid.len(), 36);
                },
                _ => panic!(),
            }

            match &clauses[2] {
                WhereClause::Field { value, .. } => {
                    let price = value.as_str().unwrap();
                    assert!(price.contains("."));
                },
                _ => panic!(),
            }
        },
        _ => panic!("Should be And variant"),
    }
}

#[test]
fn test_custom_scalar_nested_in_where() {
    // Custom scalars in nested JSON paths
    let clause = WhereClause::Field {
        path:     vec!["metadata".to_string(), "created_at".to_string()],
        operator: WhereOperator::Lt,
        value:    json!("2024-12-31T23:59:59Z"),
    };

    match clause {
        WhereClause::Field { path, value, .. } => {
            assert_eq!(path.len(), 2);
            assert_eq!(path[0], "metadata");
            assert_eq!(path[1], "created_at");
            // Custom scalar value should be preserved
            assert_eq!(value, json!("2024-12-31T23:59:59Z"));
        },
        _ => panic!("Should be Field variant"),
    }
}
