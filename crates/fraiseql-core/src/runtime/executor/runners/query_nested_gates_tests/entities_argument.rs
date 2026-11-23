//! `_entities` reads the list its `representations` argument is bound to: a variable of any
//! name, or a literal, in the operation the request selects. Authorization and resolution
//! both read that list, and no other.

use std::sync::Mutex;

use super::{
    federation::{entity_rows, federated},
    *,
};

/// Records the `target_type` of every authorization request, and denies `Order`.
#[derive(Default)]
struct RecordingDenyOrders(Mutex<Vec<String>>);

impl crate::security::Authorizer for RecordingDenyOrders {
    fn authorize(
        &self,
        req: &crate::security::AuthzRequest<'_>,
    ) -> Result<crate::security::AuthzDecision> {
        if let Some(target) = req.target_type {
            self.0.lock().unwrap().push(target.to_string());
        }
        Ok(if req.target_type == Some("Order") {
            crate::security::AuthzDecision::Deny {
                reason: "no orders".to_string(),
            }
        } else {
            crate::security::AuthzDecision::Allow
        })
    }
}

/// What resolving `query` (operation `operation`, with `variables`) answers, which entity
/// types the authorizer was asked about, and the entity read that reached the adapter.
async fn resolve(
    query: &str,
    variables: Option<Value>,
    operation: Option<&str>,
) -> (Result<Value>, Vec<String>, Option<String>) {
    let schema = federated(schema());
    let authorizer = Arc::new(RecordingDenyOrders::default());
    let config = RuntimeConfig::from_compiled_schema(&schema)
        .unwrap()
        .with_authorizer(authorizer.clone());
    let adapter = Arc::new(
        CapturingMockAdapter::new(Vec::new()).with_aggregate_rows(entity_rows(user_rows())),
    );
    let executor = Executor::with_config(schema, adapter.clone(), config);
    let result = executor
        .execute_operation_with_security(query, variables.as_ref(), &principal(), operation)
        .await;
    let asked = authorizer.0.lock().unwrap().clone();
    (result, asked, adapter.captured_aggregate_sql())
}

fn alice(result: Result<Value>) -> Value {
    let out = result.unwrap_or_else(|e| panic!("the entity must resolve: {e:?}"));
    assert_eq!(out["data"]["_entities"][0]["name"], json!("alice"), "{out}");
    out
}

const BY_R: &str =
    "query($r: [_Any!]!) { _entities(representations: $r) { ... on User { id name } } }";

#[tokio::test]
async fn a_representations_variable_of_any_name_resolves() {
    let variables = json!({"r": [{"__typename": "User", "id": 1}]});
    let (result, asked, sql) = resolve(BY_R, Some(variables), None).await;
    alice(result);
    assert_eq!(asked, vec!["User".to_string()]);
    assert!(sql.is_some_and(|sql| sql.contains("v_user")), "the read is of `User`");
}

#[tokio::test]
async fn an_inline_representations_list_resolves_without_variables() {
    let query = r#"{ _entities(representations: [{ __typename: "User", id: 1 }]) { ... on User { id name } } }"#;
    let (result, asked, _) = resolve(query, None, None).await;
    alice(result);
    assert_eq!(asked, vec!["User".to_string()]);
}

#[tokio::test]
async fn a_variable_default_supplies_the_representations() {
    let query = r#"query($r: [_Any!]! = [{ __typename: "User", id: 1 }]) { _entities(representations: $r) { ... on User { id name } } }"#;
    let (result, asked, _) = resolve(query, None, None).await;
    alice(result);
    assert_eq!(asked, vec!["User".to_string()]);
}

/// A variable named `representations` that the argument is not bound to is only a variable:
/// it decides neither what is authorized nor what is read.
#[tokio::test]
async fn a_decoy_representations_variable_is_neither_authorized_nor_read() {
    let variables = json!({
        "r": [{"__typename": "User", "id": 1}],
        "representations": [{"__typename": "Order", "id": 10}],
    });
    let (result, asked, sql) = resolve(BY_R, Some(variables), None).await;
    alice(result);
    assert_eq!(asked, vec!["User".to_string()], "only the bound list is put to the authorizer");
    assert!(
        sql.is_some_and(|sql| sql.contains("v_user") && !sql.contains("v_order")),
        "only the bound list is read"
    );
}

/// The selected operation's `_entities` field is the one read, not the document's first.
#[tokio::test]
async fn the_selected_operations_entities_field_is_read() {
    let query = "query Orders($o: [_Any!]!) { _entities(representations: $o) { ... on Order { id } } } \
                 query Users($u: [_Any!]!) { _entities(representations: $u) { ... on User { id name } } }";
    let variables = json!({
        "o": [{"__typename": "Order", "id": 10}],
        "u": [{"__typename": "User", "id": 1}],
    });
    let (result, asked, _) = resolve(query, Some(variables), Some("Users")).await;
    alice(result);
    assert_eq!(asked, vec!["User".to_string()]);
}

#[tokio::test]
async fn an_entities_field_without_representations_is_refused_naming_the_argument() {
    let query = "{ _entities { ... on User { id } } }";
    let (result, _, sql) = resolve(query, None, None).await;
    let err = result.expect_err("no representations, no entities");
    assert!(err.to_string().contains("`representations` argument"), "{err}");
    assert!(sql.is_none(), "nothing is read");
}
