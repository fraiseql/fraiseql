//! GraphQL § 5.8.2 resolves a variable's declared type against the schema the caller may
//! see: a type the caller is not shown is unknown to it here too, down to the
//! message, and a document validated for one caller is validated again for the next.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use serde_json::{Value, json};

use crate::{
    error::FraiseQLError,
    runtime::{
        Executor,
        executor::test_support::{CapturingMockAdapter, mock_user_results},
    },
    schema::CompiledSchema,
    security::SecurityContext,
};

/// `orders(filter: OrderFilter)` is open; `sales(filter: SaleFilter)` needs `manager`, and
/// is the only declaration that reaches `SaleFilter`, so a caller without the role is not
/// shown it. `audit` needs `admin`, so a manager sees a view of its own, not the whole
/// schema.
fn schema() -> CompiledSchema {
    schema_with(&[
        json!({"name": "OrderFilter",
               "fields": [{"name": "id", "field_type": "ID", "nullable": true}]}),
        json!({"name": "SaleFilter",
               "fields": [{"name": "id", "field_type": "ID", "nullable": true}]}),
    ])
}

fn schema_with(input_types: &[Value]) -> CompiledSchema {
    let open_filter: Vec<Value> = input_types
        .iter()
        .filter(|t| t["name"] == "OrderFilter")
        .map(|_| json!({"name": "filter", "arg_type": {"Input": "OrderFilter"}, "nullable": true}))
        .collect();
    let doc = json!({
        "types": [
            {"name": "Order", "sql_source": "v_order",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]},
            {"name": "Sale", "sql_source": "v_sale",
             "fields": [{"name": "id", "field_type": "ID", "nullable": false}]}
        ],
        "queries": [
            {"name": "orders", "return_type": "Order", "returns_list": true, "nullable": false,
             "sql_source": "v_order", "arguments": open_filter},
            {"name": "sales", "return_type": "Sale", "returns_list": true, "nullable": false,
             "sql_source": "v_sale", "requires_role": "manager",
             "arguments": [{"name": "filter", "arg_type": {"Input": "SaleFilter"},
                            "nullable": true}]},
            {"name": "audit", "return_type": "Order", "returns_list": true, "nullable": false,
             "sql_source": "v_order", "requires_role": "admin", "arguments": []}
        ],
        "input_types": input_types
    });
    CompiledSchema::from_json(&doc.to_string(), false).expect("fixture schema loads")
}

fn executor_for(schema: CompiledSchema) -> Executor {
    Executor::new(schema, Arc::new(CapturingMockAdapter::new(mock_user_results())))
}

fn caller(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            roles.iter().map(|r| (*r).to_string()).collect(),
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::default(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn run(executor: &Executor, doc: &str, roles: &[&str]) -> Result<Value, FraiseQLError> {
    executor.execute_with_security(doc, None, &caller(roles)).await
}

async fn refusal(executor: &Executor, doc: &str, roles: &[&str]) -> String {
    match run(executor, doc, roles).await {
        Ok(served) => panic!("`{doc}` was served to {roles:?}: {served}"),
        Err(e) => format!("{e:?}"),
    }
}

/// A document whose one variable is declared with `type_name`.
fn declaring(type_name: &str) -> String {
    format!("query Q($f: {type_name}) {{ orders {{ id }} }}")
}

const SALES: &str = "query Q($f: SaleFilter) { sales(filter: $f) { id } }";

#[tokio::test]
async fn a_type_the_caller_is_not_shown_is_refused_as_an_unknown_type_is() {
    let executor = executor_for(schema());

    let hidden = refusal(&executor, &declaring("SaleFilter"), &["viewer"]).await;
    let unknown = refusal(&executor, &declaring("NoSuchFilter"), &["viewer"]).await;

    assert_eq!(
        hidden.replace("SaleFilter", "<T>"),
        unknown.replace("NoSuchFilter", "<T>"),
        "a hidden type must answer exactly like an unknown one",
    );
    assert!(unknown.contains("declares unknown type"), "not the § 5.8.2 refusal: {unknown}");
}

#[tokio::test]
async fn a_misspelt_type_suggests_no_type_the_caller_is_not_shown() {
    let executor = executor_for(schema());

    // The suggestions are on: a caller who is shown `SaleFilter` is offered it.
    let shown = refusal(&executor, &declaring("SaleFiltr"), &["manager"]).await;
    assert!(shown.contains("Did you mean 'SaleFilter'"), "suggestions are off: {shown}");

    let not_shown = refusal(&executor, &declaring("SaleFiltr"), &["viewer"]).await;
    assert!(!not_shown.contains("SaleFilter"), "a hidden type was suggested: {not_shown}");
}

#[tokio::test]
async fn a_document_served_to_one_caller_is_validated_again_for_the_next() {
    let executor = executor_for(schema());

    run(&executor, SALES, &["manager"])
        .await
        .unwrap_or_else(|e| panic!("the manager is refused: {e}"));
    let after = refusal(&executor, SALES, &["viewer"]).await;

    assert!(after.contains("declares unknown type 'SaleFilter'"), "{after}");
}

#[tokio::test]
async fn a_caller_shown_the_type_is_served_after_one_who_is_not() {
    let executor = executor_for(schema());

    refusal(&executor, SALES, &["viewer"]).await;
    run(&executor, SALES, &["manager"])
        .await
        .unwrap_or_else(|e| panic!("the manager is refused after a viewer: {e}"));
}

#[test]
fn explain_refuses_a_type_the_schema_does_not_declare() {
    let err = executor_for(schema()).plan_query(&declaring("NoSuchFilter"), None).unwrap_err();

    assert!(err.to_string().contains("declares unknown type 'NoSuchFilter'"), "{err}");
}

#[tokio::test]
async fn an_unknown_type_is_refused_when_every_input_type_is_hidden() {
    let only_hidden = schema_with(&[json!({"name": "SaleFilter",
        "fields": [{"name": "id", "field_type": "ID", "nullable": true}]})]);
    let executor = executor_for(only_hidden);

    let unknown = refusal(&executor, &declaring("NoSuchFilter"), &["viewer"]).await;
    let hidden = refusal(&executor, &declaring("SaleFilter"), &["viewer"]).await;

    assert!(unknown.contains("declares unknown type 'NoSuchFilter'"), "{unknown}");
    assert_eq!(hidden.replace("SaleFilter", "<T>"), unknown.replace("NoSuchFilter", "<T>"));
}
