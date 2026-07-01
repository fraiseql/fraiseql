use fraiseql_error::FraiseQLError;
use serde_json::json;
use uuid::Uuid;

use super::*;
use crate::saga_store::MutationType;

/// Build a store `SagaStep` in a given `order`/`state` with an optional
/// compensation mutation. Only the fields the pure helpers read matter here.
fn step(order: usize, state: StepState, compensation_mutation: Option<&str>) -> SagaStep {
    SagaStep {
        id: Uuid::new_v4(),
        saga_id: Uuid::new_v4(),
        order,
        subgraph: "orders".to_string(),
        mutation_type: MutationType::Create,
        typename: "Order".to_string(),
        variables: json!({"id": "o1"}),
        state,
        result: None,
        started_at: None,
        completed_at: None,
        compensation_mutation: compensation_mutation.map(ToString::to_string),
        compensation_variables: None,
    }
}

#[test]
fn no_compensation_mutation_is_not_compensatable() {
    assert!(!step_is_compensatable(&step(0, StepState::Completed, None)));
}

#[test]
fn empty_compensation_mutation_is_not_compensatable() {
    assert!(!step_is_compensatable(&step(0, StepState::Completed, Some(""))));
}

#[test]
fn registered_compensation_mutation_is_compensatable() {
    assert!(step_is_compensatable(&step(0, StepState::Completed, Some("deleteOrder"))));
}

#[test]
fn ok_outcome_maps_to_success_with_data() {
    let outcome = Ok(json!({"__typename": "Order", "id": "o1", "deleted": true}));
    let result = compensation_result_from(2, &outcome, 5);

    assert!(result.success);
    assert_eq!(result.step_number, 2);
    assert_eq!(result.data, Some(json!({"__typename": "Order", "id": "o1", "deleted": true})));
    assert!(result.error.is_none());
    assert_eq!(result.duration_ms, 5);
}

#[test]
fn err_outcome_maps_to_failure_without_fabricating_data() {
    let outcome: Result<Value> = Err(FraiseQLError::Validation {
        message: "compensation target not found".to_string(),
        path:    None,
    });
    let result = compensation_result_from(3, &outcome, 9);

    assert!(!result.success, "a failed compensation must never report success");
    assert!(result.data.is_none(), "a failed compensation must not fabricate rollback data");
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("compensation target not found")
    );
    assert_eq!(result.duration_ms, 9);
}

#[test]
fn compensation_order_keeps_only_completed_in_reverse() {
    let steps = vec![
        step(0, StepState::Completed, Some("deleteA")),
        step(1, StepState::Failed, Some("deleteB")),
        step(2, StepState::Completed, Some("deleteC")),
        step(3, StepState::Pending, None),
    ];

    let ordered = compensation_order(&steps);

    // Only the two Completed steps, most-recent first (order 2 before order 0).
    let orders: Vec<usize> = ordered.iter().map(|s| s.order).collect();
    assert_eq!(orders, vec![2, 0], "compensate completed steps in reverse execution order");
}

#[test]
fn compensation_order_is_empty_when_no_step_completed() {
    let steps = vec![
        step(0, StepState::Failed, Some("deleteA")),
        step(1, StepState::Pending, None),
    ];
    assert!(
        compensation_order(&steps).is_empty(),
        "nothing completed → nothing to compensate"
    );
}
