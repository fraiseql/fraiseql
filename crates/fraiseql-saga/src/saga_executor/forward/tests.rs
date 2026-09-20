use fraiseql_error::FraiseQLError;
use serde_json::json;

use super::*;

#[test]
fn ok_outcome_maps_to_completed_success() {
    let outcome = Ok(json!({"__typename": "Order", "id": "o1"}));
    let (result, state) = step_result_from(1, &outcome, 7);

    assert!(result.success);
    assert_eq!(result.step_number, 1);
    assert_eq!(result.data, Some(json!({"__typename": "Order", "id": "o1"})));
    assert!(result.error.is_none());
    assert_eq!(result.duration_ms, 7);
    assert_eq!(state, StepState::Completed);
}

#[test]
fn err_outcome_maps_to_failed_without_fabricating_data() {
    let outcome: Result<Value> = Err(FraiseQLError::Validation {
        message: "row not found".to_string(),
        path:    None,
    });
    let (result, state) = step_result_from(2, &outcome, 3);

    assert!(!result.success, "a mutation error must never report success");
    assert!(result.data.is_none(), "a failed step must not fabricate result data");
    assert!(result.error.as_deref().unwrap_or_default().contains("row not found"));
    assert_eq!(state, StepState::Failed);
}

#[test]
fn all_steps_succeeded_is_completed() {
    let results = vec![
        step_result_from(1, &Ok(json!({})), 1).0,
        step_result_from(2, &Ok(json!({})), 1).0,
    ];
    assert_eq!(saga_state_for(&results), SagaState::Completed);
}

#[test]
fn any_failed_step_is_failed() {
    let err: Result<Value> = Err(FraiseQLError::Validation {
        message: "boom".to_string(),
        path:    None,
    });
    let results = vec![
        step_result_from(1, &Ok(json!({})), 1).0,
        step_result_from(2, &err, 1).0,
    ];
    assert_eq!(saga_state_for(&results), SagaState::Failed);
}

#[test]
fn empty_result_set_is_vacuously_completed() {
    assert_eq!(saga_state_for(&[]), SagaState::Completed);
}
