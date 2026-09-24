//! A GraphQL selection into a nested type, gated as a read of that type — reproductions.
//!
//! `{ users { orders { margin } } }` is served from the `users` view's materialised `data`:
//! the `Order` documents the view embeds reach the response as the view composed them. What
//! a read of `Order` would get — `Order`'s field-level RBAC and `Order`'s RLS policy — has
//! to reach them too. These tests state that it must.
//!
//! **Each reproduction is `#[ignore]`d and fails when run** (`-- --ignored`): they are the
//! reproduction for an unfixed defect, kept on a private branch until the fix lands, at
//! which point the `#[ignore]` comes off and they are its proof. Each has a **control**
//! beside it that selects the same thing at the root, where the gate does apply, and
//! passes — so a reproduction fails for the reason it names, not because the rig cannot
//! gate at all.
//!
//! RLS is asserted on whether `Order`'s policy is **asked**, not on the rows served: this
//! double returns the same rows whatever it is asked, so a fix that joins the nested type's
//! view would be invisible in its rows. Whether PostgreSQL serves another principal's
//! orders is `graphql_nested_type_gates_e2e_pg`'s question.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::{
    backend::{JsonbValue, WhereClause, WhereOperator},
    error::{FraiseQLError, Result},
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::{RLSPolicy, RlsWhereClause, SecurityContext, rls_policy::RlsTarget},
};

// ---------------------------------------------------------------------------
// Rig
// ---------------------------------------------------------------------------

fn scoped(name: &str, scope: &str, on_deny: FieldDenyPolicy) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::Int);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = on_deny;
    field
}

fn list_query(name: &str, return_type: &str, view: &str) -> QueryDefinition {
    let mut query = QueryDefinition::new(name, return_type);
    query.returns_list = true;
    query.sql_source = Some(view.to_string());
    query
}

/// `User.orders` is a list of `Order` documents embedded in `v_user`'s `data`. `Order`
/// gates `margin` (Mask) and `cost_price` (Reject); `User` gates nothing. Each order embeds
/// its `items`, whose `note` is masked — a gate two levels below the root. The `analyst`
/// role holds `read:margin`.
fn schema() -> CompiledSchema {
    let mut schema = CompiledSchema::default();
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("name", FieldType::String),
            FieldDefinition::new(
                "orders",
                FieldType::List(Box::new(FieldType::Object("Order".to_string()))),
            ),
        ],
        ..TypeDefinition::new("User", "v_user")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            FieldDefinition::new("owner", FieldType::String),
            scoped("margin", "read:margin", FieldDenyPolicy::Mask),
            scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
            FieldDefinition::new(
                "items",
                FieldType::List(Box::new(FieldType::Object("Item".to_string()))),
            ),
        ],
        ..TypeDefinition::new("Order", "v_order")
    });
    schema.types.push(TypeDefinition {
        fields: vec![
            FieldDefinition::new("id", FieldType::Int),
            scoped("note", "read:note", FieldDenyPolicy::Mask),
        ],
        ..TypeDefinition::new("Item", "v_item")
    });
    schema.queries.push(list_query("users", "User", "v_user"));
    schema.queries.push(list_query("orders", "Order", "v_order"));
    // Field-level RBAC is inert without a security section; with the default one, a
    // principal holding no role holds no scope.
    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("analyst", vec!["read:margin".to_string()]));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          crate::types::UserId::new("u-alice"),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-nested-gates".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

fn alices_order() -> Value {
    json!({
        "id": 10, "owner": "u-alice", "margin": 7, "cost_price": 90,
        "items": [{"id": 100, "note": "fragile"}]
    })
}

fn mallorys_order() -> Value {
    json!({"id": 11, "owner": "u-mallory", "margin": 8, "cost_price": 91})
}

/// What `v_user` materialises: alice's user with both orders embedded.
fn user_rows() -> Vec<Value> {
    vec![json!({"id": 1, "name": "alice", "orders": [alices_order(), mallorys_order()]})]
}

fn order_rows() -> Vec<Value> {
    vec![alices_order(), mallorys_order()]
}

async fn execute(rows: Vec<Value>, config: RuntimeConfig, query: &str) -> Result<Value> {
    execute_as(rows, config, query, Some(&principal())).await
}

/// `execute`, as `caller` — or anonymously, through the unauthenticated entry point.
async fn execute_as(
    rows: Vec<Value>,
    config: RuntimeConfig,
    query: &str,
    caller: Option<&SecurityContext>,
) -> Result<Value> {
    let adapter =
        Arc::new(CapturingMockAdapter::new(rows.into_iter().map(JsonbValue::new).collect()));
    let executor = Executor::read_only_with_config(schema(), adapter, config);
    match caller {
        Some(caller) => executor.execute_with_security(query, None, caller).await,
        None => executor.execute(query, None).await,
    }
}

fn analyst() -> SecurityContext {
    SecurityContext {
        roles: vec!["analyst".to_string()],
        ..principal()
    }
}

fn config() -> RuntimeConfig {
    RuntimeConfig::from_compiled_schema(&schema()).unwrap()
}

/// Every order the response carries, wherever it sits.
fn served_orders(response: &Value, path: &[&str]) -> Vec<Value> {
    let mut at = response.get("data").unwrap_or_else(|| panic!("no data: {response}"));
    for key in path {
        at = &at[*key];
    }
    match at {
        Value::Array(users_or_orders) if path.last() == Some(&"users") => users_or_orders
            .iter()
            .flat_map(|u| u["orders"].as_array().cloned().unwrap_or_default())
            .collect(),
        Value::Array(orders) => orders.clone(),
        other => panic!("not a list at {path:?}: {other}"),
    }
}

// ---------------------------------------------------------------------------
// (a) The nested type's `requires_scope`
// ---------------------------------------------------------------------------

/// Control: `Order.margin` selected at the root is masked for a principal without
/// `read:margin`.
#[tokio::test]
async fn control_a_masked_field_of_a_root_order_is_null() {
    let out = execute(order_rows(), config(), "{ orders { id margin } }").await.unwrap();
    let orders = served_orders(&out, &["orders"]);
    assert_eq!(orders.len(), 2, "{out}");
    assert!(orders.iter().all(|o| o["margin"].is_null()), "{out}");
}

/// **Reproduction (a), Mask.** The same field, selected through `users { orders { … } }`,
/// must be masked the same way.
#[tokio::test]
async fn a_masked_field_of_a_nested_order_is_null() {
    let out = execute(user_rows(), config(), "{ users { id orders { id margin } } }")
        .await
        .unwrap();
    let orders = served_orders(&out, &["users"]);
    assert_eq!(orders.len(), 2, "{out}");
    assert!(
        orders.iter().all(|o| o["margin"].is_null()),
        "Order.margin requires read:margin wherever Order is served: {out}"
    );
}

/// Control: `Order.cost_price` (Reject) selected at the root refuses the request.
#[tokio::test]
async fn control_a_rejected_field_of_a_root_order_refuses() {
    let result = execute(order_rows(), config(), "{ orders { id cost_price } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// **Reproduction (a), Reject.** Selected through `users { orders { … } }`, it must refuse
/// the request the same way.
#[tokio::test]
async fn a_rejected_field_of_a_nested_order_refuses() {
    let result = execute(user_rows(), config(), "{ users { id orders { id cost_price } } }").await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price is Reject wherever Order is served: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (a) By field, at every level, on every entry point
// ---------------------------------------------------------------------------

/// A principal holding `read:margin` is served the nested value: the level is classified
/// against its own type, not denied for being nested.
#[tokio::test]
async fn a_nested_field_the_caller_may_read_is_served() {
    let out = execute_as(
        user_rows(),
        config(),
        "{ users { id orders { id margin } } }",
        Some(&analyst()),
    )
    .await
    .unwrap();
    let margins: Vec<Value> =
        served_orders(&out, &["users"]).iter().map(|o| o["margin"].clone()).collect();
    assert_eq!(margins, [json!(7), json!(8)], "{out}");
}

/// An alias is an output key, not another field: `c: cost_price` is `cost_price`.
#[tokio::test]
async fn an_aliased_rejected_field_of_a_root_order_refuses() {
    let result = execute(order_rows(), config(), "{ orders { id c: cost_price } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

#[tokio::test]
async fn an_aliased_rejected_field_of_a_nested_order_refuses() {
    let result =
        execute(user_rows(), config(), "{ users { id orders { id c: cost_price } } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// Masked under the key the response carries it: the alias.
#[tokio::test]
async fn an_aliased_masked_field_of_a_nested_order_is_null() {
    let out = execute(user_rows(), config(), "{ users { id orders { id m: margin } } }")
        .await
        .unwrap();
    let orders = served_orders(&out, &["users"]);
    assert_eq!(orders.len(), 2, "{out}");
    assert!(orders.iter().all(|o| o.get("m") == Some(&Value::Null)), "{out}");
}

/// A field selected through an inline fragment is selected.
#[tokio::test]
async fn a_rejected_field_in_a_nested_inline_fragment_refuses() {
    let result = execute(
        user_rows(),
        config(),
        "{ users { id orders { id ... on Order { cost_price } } } }",
    )
    .await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// Two levels below the root: `Item.note`, under `users { orders { items } }`.
#[tokio::test]
async fn a_masked_field_two_levels_down_is_null() {
    let out = execute(user_rows(), config(), "{ users { orders { id items { id note } } } }")
        .await
        .unwrap();
    let items: Vec<Value> = served_orders(&out, &["users"])
        .iter()
        .flat_map(|o| o["items"].as_array().cloned().unwrap_or_default())
        .collect();
    assert_eq!(items.len(), 1, "{out}");
    assert_eq!(items[0]["id"], json!(100), "{out}");
    assert!(items[0]["note"].is_null(), "Item.note requires read:note: {out}");
}

/// The unauthenticated entry point classifies nested levels too.
#[tokio::test]
async fn an_anonymous_nested_masked_field_is_null() {
    let out = execute_as(user_rows(), config(), "{ users { id orders { id margin } } }", None)
        .await
        .unwrap();
    let orders = served_orders(&out, &["users"]);
    assert_eq!(orders.len(), 2, "{out}");
    assert!(orders.iter().all(|o| o["margin"].is_null()), "{out}");
}

#[tokio::test]
async fn an_anonymous_nested_rejected_field_refuses() {
    let result =
        execute_as(user_rows(), config(), "{ users { id orders { id cost_price } } }", None).await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

// ---------------------------------------------------------------------------
// (b) The nested type's RLS policy
// ---------------------------------------------------------------------------

/// An owner policy for every type that records which types it was asked about.
#[derive(Default)]
struct RecordingPolicy {
    asked: Mutex<Vec<String>>,
}

impl RLSPolicy for RecordingPolicy {
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        self.asked
            .lock()
            .unwrap()
            .push(target.type_name.unwrap_or("<none>").to_string());
        Ok(Some(RlsWhereClause::new(WhereClause::Field {
            path:     vec!["owner".to_string()],
            operator: WhereOperator::Eq,
            value:    json!(context.user_id.to_string()),
        })))
    }
}

async fn types_asked(rows: Vec<Value>, query: &str) -> Vec<String> {
    let policy = Arc::new(RecordingPolicy::default());
    let config = config().with_rls_policy(policy.clone());
    execute(rows, config, query).await.unwrap();
    let asked = policy.asked.lock().unwrap().clone();
    asked
}

/// Control: a root read of `Order` asks `Order`'s policy.
#[tokio::test]
async fn control_b_a_root_order_read_asks_the_order_policy() {
    let asked = types_asked(order_rows(), "{ orders { id } }").await;
    assert!(asked.iter().any(|t| t == "Order"), "{asked:?}");
}

/// **Reproduction (b).** Serving `Order` documents through `users { orders { … } }` must
/// ask `Order`'s policy too — whether the fix evaluates it over the embedded documents or
/// joins `Order`'s view, it cannot answer for `Order` without asking.
#[tokio::test]
#[ignore = "reproduction: a nested selection does not ask its own type's RLS policy"]
async fn a_nested_order_read_asks_the_order_policy() {
    let asked = types_asked(user_rows(), "{ users { id orders { id } } }").await;
    assert!(
        asked.iter().any(|t| t == "Order"),
        "Order's policy decides which orders a principal may read, and it was never asked: \
         {asked:?}"
    );
}
