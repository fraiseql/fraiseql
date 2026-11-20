//! A multi-root document (GraphQL § 6.4.4): every root is planned before any is read, a
//! root refused while planned refuses the document, and a root that fails while read is a
//! field error at its path while the others are served.

#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use crate::{
    error::FraiseQLError,
    runtime::{
        Executor, RuntimeConfig,
        executor::test_support::{MockAdapter, mock_user_results, test_schema},
    },
    schema::{CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, TypeDefinition},
    security::SecurityContext,
};

/// The text a failed read's database error carries: a relation name a client must not see
/// when the embedder renders the error.
const DRIVER_TEXT: &str = "relation \"core.v_account_internal\" does not exist";

/// `users: [User!]!` on `v_user`, and `account: User` (nullable, one object) on
/// `v_account`. `User.salary` refuses a caller without `read:salary`, `User.email` is
/// masked for a caller without `read:email`.
fn schema() -> CompiledSchema {
    let mut schema = test_schema();
    let mut account = schema.queries[0].clone();
    account.name = "account".to_string();
    account.returns_list = false;
    account.nullable = true;
    account.sql_source = Some("v_account".to_string());
    schema.queries.push(account);
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::String),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::nullable("salary", FieldType::Float)
                .with_requires_scope("read:salary")
                .with_on_deny(FieldDenyPolicy::Reject),
            FieldDefinition::nullable("email", FieldType::String)
                .with_requires_scope("read:email")
                .with_on_deny(FieldDenyPolicy::Mask),
        ],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.build_indexes();
    schema
}

fn account_row() -> Vec<crate::backend::types::JsonbValue> {
    vec![crate::backend::types::JsonbValue::new(serde_json::json!({
        "id": "9", "name": "Carol", "salary": 1.5, "email": "carol@example.test"
    }))]
}

fn adapter() -> MockAdapter {
    MockAdapter::new(mock_user_results()).with_view("v_account", account_row())
}

fn executor_with(adapter: MockAdapter, config: RuntimeConfig) -> (Executor, Arc<MockAdapter>) {
    let adapter = Arc::new(adapter);
    (Executor::with_config(schema(), Arc::clone(&adapter), config), adapter)
}

/// A principal holding no scope.
fn unscoped() -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        authenticated_at: chrono::Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// A nullable root whose read fails is `null`, with one error at its path, and the root
/// beside it is served.
#[tokio::test]
async fn a_nullable_root_that_fails_is_null_beside_the_roots_served() {
    let (executor, _) = executor_with(
        adapter().with_failing_view("v_account", DRIVER_TEXT),
        RuntimeConfig::default(),
    );
    let response = executor.execute("{ users { id } account { id } }", None).await.unwrap();
    assert_eq!(
        response["data"],
        serde_json::json!({ "users": [{ "id": "1" }, { "id": "2" }], "account": null }),
        "{response}"
    );
    let errors = response["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{response}");
    assert_eq!(errors[0]["path"], serde_json::json!(["account"]), "{response}");
}

/// A root published non-null that fails nulls `data` (§ 6.4.4), and its error is reported
/// once: not again as a `null` in a non-null position.
#[tokio::test]
async fn a_non_null_root_that_fails_nulls_data_with_its_one_error() {
    let (executor, _) =
        executor_with(adapter().with_failing_view("v_user", DRIVER_TEXT), RuntimeConfig::default());
    let response = executor.execute("{ account { id } list: users { id } }", None).await.unwrap();
    assert_eq!(response["data"], serde_json::Value::Null, "{response}");
    let errors = response["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{response}");
    assert_eq!(errors[0]["path"], serde_json::json!(["list"]), "under its alias: {response}");
}

/// The failed root's error is the embedder's rendering of it, the treatment a failed
/// request gets: the server's renderer withholds the database's text.
#[tokio::test]
async fn a_failed_roots_error_is_rendered_by_the_embedders_renderer() {
    let config = RuntimeConfig {
        root_error_renderer: Some(Arc::new(|error: &FraiseQLError| {
            serde_json::json!({
                "message": "An internal error occurred",
                "extensions": { "code": error.error_code() },
            })
        })),
        ..RuntimeConfig::default()
    };
    let (executor, _) =
        executor_with(adapter().with_failing_view("v_account", DRIVER_TEXT), config);
    let response = executor.execute("{ users { id } account { id } }", None).await.unwrap();
    assert!(!response.to_string().contains("v_account_internal"), "{response}");
    assert_eq!(response["errors"][0]["message"], "An internal error occurred", "{response}");
    assert_eq!(response["errors"][0]["path"], serde_json::json!(["account"]), "{response}");
}

/// A root no query answers refuses the document, before any root is read.
#[tokio::test]
async fn a_root_refused_while_planned_refuses_the_document_before_any_read() {
    let (executor, adapter) = executor_with(adapter(), RuntimeConfig::default());
    let refused = executor.execute("{ users { id } nope { id } }", None).await;
    assert!(matches!(refused, Err(FraiseQLError::Validation { .. })), "{refused:?}");
    assert!(adapter.served_name_buffers.lock().unwrap().is_empty(), "a root was read");
}

/// A field the caller may not read refuses the document whichever root selects it, as it
/// refuses that root sent alone, and nothing is read.
#[tokio::test]
async fn a_refused_field_on_any_root_refuses_the_document_as_it_does_alone() {
    let alone = executor_with(adapter(), RuntimeConfig::default())
        .0
        .execute_with_security("{ account { id salary } }", None, &unscoped())
        .await
        .unwrap_err();
    assert!(matches!(alone, FraiseQLError::Authorization { .. }), "{alone:?}");
    for document in [
        "{ users { id } account { id salary } }",
        "{ account { id salary } users { id } }",
        "{ users { id } a: account { id } b: account { id salary } }",
    ] {
        let (executor, adapter) = executor_with(adapter(), RuntimeConfig::default());
        let refused =
            executor.execute_with_security(document, None, &unscoped()).await.unwrap_err();
        assert_eq!(refused.to_string(), alone.to_string(), "{document}");
        assert!(
            adapter.served_name_buffers.lock().unwrap().is_empty(),
            "{document}: a root was read"
        );
    }
}

/// A masked field is masked on whichever root selects it, as on that root alone.
#[tokio::test]
async fn a_masked_field_on_any_root_is_masked_as_it_is_alone() {
    let (executor, _) = executor_with(adapter(), RuntimeConfig::default());
    let alone = executor
        .execute_with_security("{ account { id email } }", None, &unscoped())
        .await;
    let alone = alone.unwrap();
    assert_eq!(alone["data"]["account"]["email"], serde_json::Value::Null, "{alone}");
    let both = executor
        .execute_with_security("{ users { id } account { id email } }", None, &unscoped())
        .await
        .unwrap();
    assert_eq!(both["data"]["account"], alone["data"]["account"], "{both}");
}

/// A document with one root keeps refusing as a whole when its root fails.
#[tokio::test]
async fn a_single_root_that_fails_still_fails_the_request() {
    let (executor, _) = executor_with(
        adapter().with_failing_view("v_account", DRIVER_TEXT),
        RuntimeConfig::default(),
    );
    let failed = executor.execute("{ account { id } }", None).await;
    assert!(matches!(failed, Err(FraiseQLError::Database { .. })), "{failed:?}");
}
