#![allow(missing_docs)]

use fraiseql_error::ObserverError;

#[test]
fn invalid_condition_error_code_and_display() {
    let err = ObserverError::InvalidCondition {
        message: "unknown field".into(),
    };
    assert_eq!(err.error_code(), "observer_invalid_condition");
    assert_eq!(err.to_string(), "Invalid condition: unknown field");
}

#[test]
fn template_error_code_and_display() {
    let err = ObserverError::TemplateError {
        message: "syntax error".into(),
    };
    assert_eq!(err.error_code(), "observer_template_error");
    assert_eq!(err.to_string(), "Template error: syntax error");
}

#[test]
fn action_failed_error_code_and_display() {
    let err = ObserverError::ActionFailed {
        action: "send_email".into(),
        message: "smtp down".into(),
    };
    assert_eq!(err.error_code(), "observer_action_failed");
    assert_eq!(err.to_string(), "Action failed: send_email - smtp down");
}

#[test]
fn invalid_config_error_code_and_display() {
    let err = ObserverError::InvalidConfig {
        message: "missing handler".into(),
    };
    assert_eq!(err.error_code(), "observer_invalid_config");
    assert_eq!(err.to_string(), "Invalid configuration: missing handler");
}

#[test]
fn processing_failed_error_code_and_display() {
    let err = ObserverError::ProcessingFailed {
        message: "deserialization error".into(),
    };
    assert_eq!(err.error_code(), "observer_processing_failed");
    assert_eq!(err.to_string(), "Event processing failed: deserialization error");
}

#[test]
fn max_retries_exceeded_error_code_and_display() {
    let err = ObserverError::MaxRetriesExceeded {
        event_id: "evt_456".into(),
    };
    assert_eq!(err.error_code(), "observer_max_retries");
    assert_eq!(err.to_string(), "Max retries exceeded for event evt_456");
}

#[test]
fn database_error_from_sqlx() {
    let sqlx_err = sqlx::Error::Protocol("test protocol error".into());
    let err: ObserverError = sqlx_err.into();
    assert_eq!(err.error_code(), "observer_database_error");
    assert!(err.to_string().starts_with("Database error:"));
}

#[test]
fn database_error_code_and_display() {
    let err = ObserverError::Database(sqlx::Error::RowNotFound);
    assert_eq!(err.error_code(), "observer_database_error");
    assert_eq!(
        err.to_string(),
        "Database error: no rows returned by a query that expected to return at least one row"
    );
}
