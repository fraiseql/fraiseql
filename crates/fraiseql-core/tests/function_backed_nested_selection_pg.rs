//! A function-backed root query field, read through a nested selection (#1693).
//!
//! The function returns the field's data as the guest built it. Root fields and the items of
//! nested lists are projected to the selection; these cases ask the same of nested **single
//! objects**: only the selected keys, `null` for a selected key the guest left out, and
//! `requires_scope` applied as on any other field.
//!
//! `Lookup.category` is a `Category` carrying `salary` (Mask behind `read:salary`) and `cost`
//! (Reject behind `read:cost`). The guest returns both, and the reader holds neither scope.
//!
//! **Execution engine:** none (the function path issues no SQL) · **Infrastructure:**
//! `DATABASE_URL`, only to construct the adapter · **Parallelism:** safe.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::{FraiseQLError, Result},
    prelude::UserId,
    runtime::{Executor, QueryFunctionRequest, QueryFunctionResolver, RuntimeConfig},
    schema::{
        ArgumentDefinition, CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType,
        QueryDefinition, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

/// Answers every call with one fixed document.
struct Fixed(Value);

impl QueryFunctionResolver for Fixed {
    fn resolve<'a>(
        &'a self,
        _request: QueryFunctionRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + Send + 'a>> {
        let value = self.0.clone();
        Box::pin(async move { Ok(value) })
    }
}

/// What the guest returns: every nested object carries keys the reader did not select, and
/// `category` carries both scope-gated fields and no `meta`.
fn guest_document() -> Value {
    json!({
        "id": "k-1",
        "status": "found",
        "address": {"lines": ["1 Main St"], "postcode": "75015", "city": "Paris"},
        "problems": [{"identifier": "closed", "message": "Closed",
                      "owner": {"code": "O", "salary": 2, "cost": 3}}],
        "category": {
            "id": "c-1", "code": "A", "country": "FR", "salary": 5000, "cost": 90,
            "parent": {"code": "P", "country": "FR", "salary": 9000},
            "items": [{"name": "i1", "salary": 1, "extra": "x",
                       "owner": {"code": "Q", "salary": 4, "cost": 5}}]
        }
    })
}

fn scoped(name: &str, scope: &str, on_deny: FieldDenyPolicy) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::Int);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = on_deny;
    field
}

fn object(name: &str, fields: Vec<FieldDefinition>) -> TypeDefinition {
    let mut type_def = TypeDefinition::new(name, "");
    type_def.fields = fields;
    type_def
}

const fn nullable(mut field: FieldDefinition) -> FieldDefinition {
    field.nullable = true;
    field
}

fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::new();
    schema.types.push(object(
        "Lookup",
        vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("status", FieldType::String),
            nullable(FieldDefinition::new("address", FieldType::Object("Address".to_string()))),
            FieldDefinition::new(
                "problems",
                FieldType::list(FieldType::Object("Problem".to_string())),
            ),
            nullable(FieldDefinition::new("category", FieldType::Object("Category".to_string()))),
        ],
    ));
    schema.types.push(object(
        "Address",
        vec![
            FieldDefinition::new("lines", FieldType::list(FieldType::String)),
            nullable(FieldDefinition::new("postcode", FieldType::String)),
            nullable(FieldDefinition::new("city", FieldType::String)),
        ],
    ));
    schema.types.push(object(
        "Problem",
        vec![
            FieldDefinition::new("identifier", FieldType::String),
            FieldDefinition::new("message", FieldType::String),
            nullable(FieldDefinition::new("owner", FieldType::Object("Category".to_string()))),
        ],
    ));
    schema.types.push(object(
        "Item",
        vec![
            FieldDefinition::new("name", FieldType::String),
            scoped("salary", "read:salary", FieldDenyPolicy::Mask),
            nullable(FieldDefinition::new("owner", FieldType::Object("Category".to_string()))),
        ],
    ));
    schema.types.push(object(
        "Category",
        vec![
            FieldDefinition::new("id", FieldType::Id),
            FieldDefinition::new("code", FieldType::String),
            FieldDefinition::new("country", FieldType::String),
            nullable(FieldDefinition::new("meta", FieldType::Object("Address".to_string()))),
            scoped("salary", "read:salary", FieldDenyPolicy::Mask),
            scoped("cost", "read:cost", FieldDenyPolicy::Reject),
            nullable(FieldDefinition::new("parent", FieldType::Object("Category".to_string()))),
            FieldDefinition::new("items", FieldType::list(FieldType::Object("Item".to_string()))),
        ],
    ));

    let mut lookup = QueryDefinition::new("lookup", "Lookup");
    lookup.function = Some("lookup".to_string());
    lookup.nullable = true;
    lookup.arguments = vec![ArgumentDefinition::new("key", FieldType::String)];
    schema.queries.push(lookup);

    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("hr", vec!["read:salary".to_string()]));
    security.add_role(RoleDefinition::new("costing", vec!["read:cost".to_string()]));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

fn reader() -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from("u-reader"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-function-nested".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

async fn executor() -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    let config =
        RuntimeConfig::default().with_query_function_resolver(Arc::new(Fixed(guest_document())));
    Some(Executor::with_config(schema(), adapter, config))
}

macro_rules! executor_or_skip {
    () => {
        match executor().await {
            Some(executor) => executor,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn lookup(executor: &Executor, selection: &str) -> Result<Value> {
    let query = format!(r#"{{ lookup(key: "x") {selection} }}"#);
    executor.execute_with_security(&query, None, &reader()).await
}

/// A nested object serves only the keys selected on it.
#[tokio::test]
async fn an_unselected_nested_object_field_is_absent() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ status address { city } category { code } }")
        .await
        .unwrap();
    assert_eq!(
        out["data"]["lookup"],
        json!({"status": "found", "address": {"city": "Paris"}, "category": {"code": "A"}}),
        "{out}"
    );
}

/// The guest's `salary` and `cost` never reach a reader who did not select them: a
/// scope-gated key must not ride along on an unprojected object.
#[tokio::test]
async fn an_unselected_scope_gated_nested_field_is_absent() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { code } }").await.unwrap();
    let category = &out["data"]["lookup"]["category"];
    assert!(category.get("salary").is_none(), "salary leaked: {out}");
    assert!(category.get("cost").is_none(), "cost leaked: {out}");
}

/// The same, one object deeper.
#[tokio::test]
async fn an_unselected_scope_gated_field_two_objects_deep_is_absent() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { parent { code } } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["category"], json!({"parent": {"code": "P"}}), "{out}");
}

/// A selected `salary`, without `read:salary`, is `null`.
#[tokio::test]
async fn a_selected_masked_nested_field_is_null() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { code salary } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["category"], json!({"code": "A", "salary": null}), "{out}");
}

/// A selected `cost`, without `read:cost`, refuses the read.
#[tokio::test]
async fn a_selected_rejected_nested_field_is_refused() {
    let executor = executor_or_skip!();
    let error = lookup(&executor, "{ category { cost } }").await.unwrap_err();
    assert!(matches!(error, FraiseQLError::Authorization { .. }), "{error:?}");
}

/// A selected nullable key the guest left out is `null`, not missing.
#[tokio::test]
async fn a_selected_nested_field_the_guest_omitted_is_null() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { code meta { city } } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["category"], json!({"code": "A", "meta": null}), "{out}");
}

/// An alias names the key it serves, at nested depth too.
#[tokio::test]
async fn an_aliased_nested_object_selection_is_served_as_named() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ c: category { k: code } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["c"], json!({"k": "A"}), "{out}");
}

/// Nested lists were already projected; they stay so.
#[tokio::test]
async fn a_nested_list_is_projected() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ problems { identifier } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["problems"], json!([{"identifier": "closed"}]), "{out}");
}

/// An object inside a list inside an object: each level serves only its selection, and the
/// scope-gated keys at the list element and below it are absent.
#[tokio::test]
async fn an_object_in_a_list_in_an_object_is_projected_at_every_level() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { items { name owner { code } } } }")
        .await
        .unwrap();
    assert_eq!(
        out["data"]["lookup"]["category"],
        json!({"items": [{"name": "i1", "owner": {"code": "Q"}}]}),
        "{out}"
    );
}

/// An object inside a list at the root: the same.
#[tokio::test]
async fn an_object_in_a_list_is_projected() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ problems { owner { code } } }").await.unwrap();
    assert_eq!(out["data"]["lookup"]["problems"], json!([{"owner": {"code": "O"}}]), "{out}");
}

/// A selected `salary` three levels down, without `read:salary`, is `null`.
#[tokio::test]
async fn a_selected_masked_field_under_a_list_is_null() {
    let executor = executor_or_skip!();
    let out = lookup(&executor, "{ category { items { owner { salary } } } }").await.unwrap();
    assert_eq!(
        out["data"]["lookup"]["category"],
        json!({"items": [{"owner": {"salary": null}}]}),
        "{out}"
    );
}

/// A selected `cost` three levels down, without `read:cost`, refuses the read.
#[tokio::test]
async fn a_selected_rejected_field_under_a_list_is_refused() {
    let executor = executor_or_skip!();
    let error = lookup(&executor, "{ category { items { owner { cost } } } }")
        .await
        .unwrap_err();
    assert!(matches!(error, FraiseQLError::Authorization { .. }), "{error:?}");
}
