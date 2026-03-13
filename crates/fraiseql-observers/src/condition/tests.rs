#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::event::{EventKind, FieldChanges};

#[test]
fn test_parse_simple_comparison() {
    let parser = ConditionParser::new();
    let ast = parser.parse("total == 100").unwrap();

    match ast {
        ConditionAst::Comparison { field, op, value } => {
            assert_eq!(field, "total");
            assert_eq!(op, "==");
            assert_eq!(value, "100");
        },
        _ => panic!("Expected comparison"),
    }
}

#[test]
fn test_parse_has_field() {
    let parser = ConditionParser::new();
    let ast = parser.parse("has_field('status')").unwrap();

    match ast {
        ConditionAst::HasField { field } => {
            assert_eq!(field, "status");
        },
        _ => panic!("Expected has_field"),
    }
}

#[test]
fn test_parse_field_changed_to() {
    let parser = ConditionParser::new();
    let ast = parser.parse("field_changed_to('status', 'shipped')").unwrap();

    match ast {
        ConditionAst::FieldChangedTo { field, value } => {
            assert_eq!(field, "status");
            assert_eq!(value, "shipped");
        },
        _ => panic!("Expected field_changed_to"),
    }
}

#[test]
fn test_parse_and_operator() {
    let parser = ConditionParser::new();
    let ast = parser.parse("total > 100 && field_changed_to('status', 'shipped')").unwrap();

    match ast {
        ConditionAst::And { left, right } => {
            assert!(matches!(*left, ConditionAst::Comparison { .. }));
            assert!(matches!(*right, ConditionAst::FieldChangedTo { .. }));
        },
        _ => panic!("Expected AND"),
    }
}

#[test]
fn test_parse_or_operator() {
    let parser = ConditionParser::new();
    let ast = parser.parse("status == 'pending' || status == 'processing'").unwrap();

    match ast {
        ConditionAst::Or { .. } => {},
        _ => panic!("Expected OR"),
    }
}

#[test]
fn test_parse_not_operator() {
    let parser = ConditionParser::new();
    let ast = parser.parse("!has_field('deleted_at')").unwrap();

    match ast {
        ConditionAst::Not { .. } => {},
        _ => panic!("Expected NOT"),
    }
}

#[test]
fn test_parse_parentheses() {
    let parser = ConditionParser::new();
    let ast = parser.parse("(total > 100) && (status == 'shipped')").unwrap();

    match ast {
        ConditionAst::And { .. } => {},
        _ => panic!("Expected AND"),
    }
}

#[test]
fn test_evaluate_simple_comparison() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 150, "status": "pending"}),
    );

    let result = parser.parse_and_evaluate("total > 100", &event).unwrap();
    assert!(result);

    let result = parser.parse_and_evaluate("total < 100", &event).unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_equality() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "pending"}),
    );

    let result = parser.parse_and_evaluate("status == 'pending'", &event).unwrap();
    assert!(result);

    let result = parser.parse_and_evaluate("status == 'shipped'", &event).unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_has_field() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 100}),
    );

    let result = parser.parse_and_evaluate("has_field('total')", &event).unwrap();
    assert!(result);

    let result = parser.parse_and_evaluate("has_field('nonexistent')", &event).unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_and_operator() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 150, "status": "shipped"}),
    );

    let result =
        parser.parse_and_evaluate("total > 100 && status == 'shipped'", &event).unwrap();
    assert!(result);

    let result =
        parser.parse_and_evaluate("total > 100 && status == 'pending'", &event).unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_or_operator() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "shipped"}),
    );

    let result = parser
        .parse_and_evaluate("status == 'pending' || status == 'shipped'", &event)
        .unwrap();
    assert!(result);

    let result = parser
        .parse_and_evaluate("status == 'pending' || status == 'processing'", &event)
        .unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_not_operator() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 100}),
    );

    let result = parser.parse_and_evaluate("!has_field('deleted_at')", &event).unwrap();
    assert!(result);

    let result = parser.parse_and_evaluate("!has_field('total')", &event).unwrap();
    assert!(!result);
}

#[test]
fn test_evaluate_complex_condition() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 150, "status": "shipped", "priority": "high"}),
    );

    let result = parser
        .parse_and_evaluate(
            "(total > 100 && status == 'shipped') || priority == 'high'",
            &event,
        )
        .unwrap();
    assert!(result);
}

// =========================================================================
// Additional tests for condition coverage
// =========================================================================

#[test]
fn test_equality_match_field_equals_value() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "pending"}),
    );
    let result = parser.parse_and_evaluate("status == 'pending'", &event).unwrap();
    assert!(result, "Field equality match should succeed");
}

#[test]
fn test_inequality_match_field_differs() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "shipped"}),
    );
    let result = parser.parse_and_evaluate("status != 'pending'", &event).unwrap();
    assert!(result, "Inequality match should succeed when field differs");
}

#[test]
fn test_missing_field_returns_error() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 100}),
    );
    // Referencing a non-existent field in a comparison returns an error
    let result = parser.parse_and_evaluate("nonexistent == 'foo'", &event);
    assert!(result.is_err(), "Missing field should return an error");
}

#[test]
fn test_invalid_filter_string_is_error() {
    let parser = ConditionParser::new();
    let result = parser.parse("@@@invalid&&&");
    assert!(result.is_err(), "Unparseable filter string should return error");
}

#[test]
fn test_case_sensitivity_equality_is_case_sensitive() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "Pending"}),
    );
    // Exact match is case-sensitive
    let result = parser.parse_and_evaluate("status == 'Pending'", &event).unwrap();
    assert!(result, "Case-sensitive match on 'Pending' should succeed");

    let result = parser.parse_and_evaluate("status == 'pending'", &event).unwrap();
    assert!(!result, "Case-sensitive match on 'pending' should fail for 'Pending'");
}

#[test]
fn test_multiple_and_conditions_all_must_match() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "shipped", "priority": "high"}),
    );
    // Both conditions true
    let result =
        parser.parse_and_evaluate("status == 'shipped' && priority == 'high'", &event).unwrap();
    assert!(result, "Both conditions true should pass AND");

    // One condition false
    let result =
        parser.parse_and_evaluate("status == 'shipped' && priority == 'low'", &event).unwrap();
    assert!(!result, "One false condition should fail AND");
}

#[test]
fn test_numeric_string_comparison() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"count": 5}),
    );
    // Numeric comparison: count == 5
    let result = parser.parse_and_evaluate("count == 5", &event).unwrap();
    assert!(result, "Numeric equality should match");
}

#[test]
fn test_empty_string_field_value() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"note": ""}),
    );
    // Empty string equality
    let result = parser.parse_and_evaluate("note == ''", &event).unwrap();
    assert!(result, "Empty string equality should match");
}

#[test]
fn test_has_field_for_existing_field() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"email": "user@example.com"}),
    );
    let result = parser.parse_and_evaluate("has_field('email')", &event).unwrap();
    assert!(result, "has_field should return true for existing field");
}

#[test]
fn test_has_field_for_missing_field() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"total": 50}),
    );
    let result = parser.parse_and_evaluate("has_field('email')", &event).unwrap();
    assert!(!result, "has_field should return false for missing field");
}

#[test]
fn test_inequality_false_when_field_matches() {
    let parser = ConditionParser::new();
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "pending"}),
    );
    // field != 'pending' when status IS 'pending' should be false
    let result = parser.parse_and_evaluate("status != 'pending'", &event).unwrap();
    assert!(!result, "Inequality should fail when field matches value");
}

// ── FieldChangedTo / FieldChangedFrom / FieldChanged evaluation ──────────
// These tests cover evaluate() with the `changes` field populated on the event.
// Parsing is covered above; this section verifies the runtime evaluation path.

fn make_update_event_with_status_change(old: &str, new: &str) -> EntityEvent {
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        "status".to_string(),
        FieldChanges { old: json!(old), new: json!(new) },
    );
    EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": new}),
    )
    .with_changes(changes)
}

#[test]
fn test_evaluate_field_changed_to_matches() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    let result = parser.parse_and_evaluate("field_changed_to('status', 'shipped')", &event);
    assert!(result.unwrap(), "field_changed_to should return true when new == expected");
}

#[test]
fn test_evaluate_field_changed_to_no_match() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    let result = parser.parse_and_evaluate("field_changed_to('status', 'pending')", &event);
    assert!(!result.unwrap(), "field_changed_to should return false when new != expected");
}

#[test]
fn test_evaluate_field_changed_to_no_changes() {
    let parser = ConditionParser::new();
    // Event with NO changes (INSERT event, no changes map)
    let event = EntityEvent::new(
        EventKind::Created,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "shipped"}),
    );
    let result = parser.parse_and_evaluate("field_changed_to('status', 'shipped')", &event);
    assert!(!result.unwrap(), "field_changed_to should return false when no changes present");
}

#[test]
fn test_evaluate_field_changed_from_matches() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    let result = parser.parse_and_evaluate("field_changed_from('status', 'pending')", &event);
    assert!(result.unwrap(), "field_changed_from should return true when old == expected");
}

#[test]
fn test_evaluate_field_changed_from_no_match() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    let result = parser.parse_and_evaluate("field_changed_from('status', 'shipped')", &event);
    assert!(!result.unwrap(), "field_changed_from should return false when old != expected");
}

#[test]
fn test_evaluate_field_changed_any_value() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    // field_changed('status') — true when status field is in changes map
    let result = parser.parse_and_evaluate("field_changed('status')", &event);
    assert!(result.unwrap(), "field_changed should return true when field is in changes");
}

#[test]
fn test_evaluate_field_changed_not_in_changes() {
    let parser = ConditionParser::new();
    let event = make_update_event_with_status_change("pending", "shipped");

    // 'total' was NOT changed — should return false
    let result = parser.parse_and_evaluate("field_changed('total')", &event);
    assert!(!result.unwrap(), "field_changed should return false when field not in changes");
}

#[test]
fn test_evaluate_field_changed_to_combined_with_and() {
    // Compound condition: field changed to 'shipped' AND total > 100
    let parser = ConditionParser::new();
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        "status".to_string(),
        FieldChanges { old: json!("pending"), new: json!("shipped") },
    );
    let event = EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "shipped", "total": 150}),
    )
    .with_changes(changes);

    let result = parser
        .parse_and_evaluate("field_changed_to('status', 'shipped') && total > 100", &event);
    assert!(result.unwrap(), "combined condition with field_changed_to should evaluate correctly");
}

#[test]
fn test_evaluate_field_changed_to_combined_false_when_not_changed() {
    let parser = ConditionParser::new();
    // 'status' was NOT changed — only 'total' changed
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        "total".to_string(),
        FieldChanges { old: json!(50), new: json!(150) },
    );
    let event = EntityEvent::new(
        EventKind::Updated,
        "Order".to_string(),
        Uuid::new_v4(),
        json!({"status": "pending", "total": 150}),
    )
    .with_changes(changes);

    let result = parser
        .parse_and_evaluate("field_changed_to('status', 'shipped') && total > 100", &event);
    assert!(!result.unwrap(), "condition should be false when field_changed_to mismatch");
}

// ── Depth limit tests (14-1) ─────────────────────────────────────────────

#[test]
fn test_depth_64_is_accepted() {
    // 64 nested parentheses wrapping a simple comparison — should succeed.
    let parser = ConditionParser::new();
    let open: String = "(".repeat(64);
    let close: String = ")".repeat(64);
    let condition = format!("{open}total == 100{close}");
    assert!(
        parser.parse(&condition).is_ok(),
        "64 levels of nesting must be accepted (== MAX_CONDITION_DEPTH)"
    );
}

#[test]
fn test_depth_65_returns_max_depth_error() {
    // 65 nested parentheses — one beyond the limit — must fail.
    let parser = ConditionParser::new();
    let open: String = "(".repeat(65);
    let close: String = ")".repeat(65);
    let condition = format!("{open}total == 100{close}");
    let result = parser.parse(&condition);
    assert!(result.is_err(), "65 levels of nesting must be rejected");
    assert!(
        result.unwrap_err().to_string().contains("nesting depth"),
        "error message must mention nesting depth"
    );
}

#[test]
fn test_deeply_nested_not_operators_limited() {
    // 65 consecutive `!` operators — must fail with depth exceeded.
    let parser = ConditionParser::new();
    let nots: String = "!".repeat(65);
    let condition = format!("{nots}has_field('x')");
    let result = parser.parse(&condition);
    assert!(result.is_err(), "65 nested NOT operators must be rejected");
}

// ── Size-cap tests (S20-H3 / S20-H4) ─────────────────────────────────────

#[test]
fn condition_input_at_limit_is_accepted() {
    // A short well-formed condition must still parse (size cap not triggered).
    let parser = ConditionParser::new();
    assert!(parser.parse("total == 100").is_ok());
}

#[test]
fn condition_input_exceeding_size_limit_is_rejected() {
    // Build a condition string longer than MAX_CONDITION_INPUT_BYTES (4096).
    let parser = ConditionParser::new();
    // Pad with spaces so the string is > 4096 bytes but would otherwise tokenize.
    let long_condition = format!("total == 1{}", " ".repeat(4100));
    let result = parser.parse(&long_condition);
    assert!(result.is_err(), "oversized condition must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("too long") || msg.contains("4096"),
        "error must mention size limit: {msg}"
    );
}

#[test]
fn condition_function_exceeding_arg_limit_is_rejected() {
    // Build in_set('a', 'b', ...) with 33 args — exceeds MAX_CONDITION_FUNCTION_ARGS=32.
    let parser = ConditionParser::new();
    let args: Vec<String> = (0_u8..33).map(|i| format!("'{i}'")).collect();
    let condition = format!("in_set({})", args.join(","));
    let result = parser.parse(&condition);
    assert!(result.is_err(), "too many function arguments must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("Too many") || msg.contains("32"),
        "error must mention argument limit: {msg}"
    );
}

#[test]
fn condition_function_at_arg_limit_is_accepted() {
    // Exactly MAX_CONDITION_FUNCTION_ARGS=32 args must succeed.
    let parser = ConditionParser::new();
    let args: Vec<String> = (0_u8..32).map(|i| format!("'arg{i}'")).collect();
    let condition = format!("in_set({})", args.join(","));
    // We don't assert Ok because in_set may not be a recognised function;
    // the key property is that it must NOT fail with the arg-limit error.
    if let Err(e) = parser.parse(&condition) {
        assert!(
            !e.to_string().contains("Too many"),
            "32 args must not trigger arg limit: {e}"
        );
    }
}
