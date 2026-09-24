//! A GraphQL selection into a nested type, gated as a read of that type.
//!
//! `{ users { orders { margin } } }` is served from the `users` view's materialised `data`:
//! the `Order` documents the view embeds reach the response as the view composed them. What
//! a read of `Order` would get — `Order`'s field-level RBAC and `Order`'s RLS policy — has
//! to reach them too. These tests state that it does.
//!
//! The first six began as the reproductions of the defect, `#[ignore]`d until the fix
//! (`runners/query_nested.rs`); each keeps the **control** beside it that selects the same
//! thing at the root, so a failure is about nesting, not about a rig that cannot gate.
//!
//! This double returns the rows it is given whatever it is asked, so RLS is asserted on
//! what the engine *asks* — the policy's targets, and the composed statement the adapter
//! receives — rather than on the rows served. Whether PostgreSQL then serves another
//! principal's orders is `graphql_nested_type_gates_e2e_pg`'s question.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::{
    backend::{
        ComposedLevel, EmbedShape, EmbedSource, JsonbValue, LevelKeys, WhereClause, WhereOperator,
    },
    error::{FraiseQLError, Result},
    runtime::{Executor, RuntimeConfig, executor::test_support::CapturingMockAdapter},
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, QueryDefinition, Relationship, RoleDefinition, SecurityConfig,
        TypeDefinition,
    },
    security::{
        ConstrainedPaths, RLSPolicy, RlsWhereClause, SecurityContext, rls_policy::RlsTarget,
    },
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
/// role holds `read:margin`, `costing` holds `read:cost`.
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
    security.add_role(RoleDefinition::new("costing", vec!["read:cost".to_string()]));
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
    run(schema(), rows, config, query, caller).await.0
}

/// Execute over `schema`, and hand back the adapter to ask what reached it.
async fn run(
    schema: CompiledSchema,
    rows: Vec<Value>,
    config: RuntimeConfig,
    query: &str,
    caller: Option<&SecurityContext>,
) -> (Result<Value>, Arc<CapturingMockAdapter>) {
    let adapter =
        Arc::new(CapturingMockAdapter::new(rows.into_iter().map(JsonbValue::new).collect()));
    let executor = Executor::read_only_with_config(schema, adapter.clone(), config);
    let result = match caller {
        Some(caller) => executor.execute_with_security(query, None, caller).await,
        None => executor.execute(query, None).await,
    };
    (result, adapter)
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
    /// The only key its predicate reads — `Order` declares it, so a nested `Order` is
    /// gated over the documents `v_user` embeds.
    fn constrained_paths(&self, _target: &RlsTarget<'_>) -> ConstrainedPaths {
        ConstrainedPaths::Declared(vec!["owner".to_string()])
    }

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
async fn a_nested_order_read_asks_the_order_policy() {
    let asked = types_asked(user_rows(), "{ users { id orders { id } } }").await;
    assert!(
        asked.iter().any(|t| t == "Order"),
        "Order's policy decides which orders a principal may read, and it was never asked: \
         {asked:?}"
    );
}

// ---------------------------------------------------------------------------
// (b) What a nested level is read with
// ---------------------------------------------------------------------------

fn owner_is(user: &str) -> WhereClause {
    WhereClause::Field {
        path:     vec!["owner".to_string()],
        operator: WhereOperator::Eq,
        value:    json!(user),
    }
}

/// An owner policy that answers per type, and declares its paths or not.
struct OwnerPolicy {
    /// Types it scopes; any other it leaves unfiltered.
    scopes:   &'static [&'static str],
    /// What `constrained_paths` answers.
    declares: Option<Vec<String>>,
    /// What its predicate reads — `owner`, unless it is lying about it.
    reads:    &'static str,
}

impl OwnerPolicy {
    fn declared(scopes: &'static [&'static str]) -> Arc<Self> {
        Arc::new(Self {
            scopes,
            declares: Some(vec!["owner".to_string()]),
            reads: "owner",
        })
    }

    fn opaque(scopes: &'static [&'static str]) -> Arc<Self> {
        Arc::new(Self {
            scopes,
            declares: None,
            reads: "owner",
        })
    }
}

impl RLSPolicy for OwnerPolicy {
    fn constrained_paths(&self, _target: &RlsTarget<'_>) -> ConstrainedPaths {
        self.declares
            .clone()
            .map_or(ConstrainedPaths::Opaque, ConstrainedPaths::Declared)
    }

    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        let scoped = target.type_name.is_some_and(|t| self.scopes.contains(&t));
        Ok(scoped.then(|| {
            RlsWhereClause::new(WhereClause::Field {
                path:     vec![self.reads.to_string()],
                operator: WhereOperator::Eq,
                value:    json!(context.user_id.to_string()),
            })
        }))
    }
}

fn with_policy(policy: Arc<dyn RLSPolicy>) -> RuntimeConfig {
    config().with_rls_policy(policy)
}

/// `schema()`, with `User.orders` declared as the relationship it is.
fn joinable_schema() -> CompiledSchema {
    let mut schema = schema();
    let user = schema.types.iter_mut().find(|t| t.name == "User").unwrap();
    user.relationships.push(Relationship {
        name:           "orders".to_string(),
        target_type:    "Order".to_string(),
        cardinality:    Cardinality::OneToMany,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
    });
    schema.build_indexes();
    schema
}

fn only_embed(read: &ComposedLevel) -> &crate::backend::ComposedEmbed {
    assert_eq!(read.embeds.len(), 1, "{read:#?}");
    &read.embeds[0]
}

/// Declared paths the embedded documents carry: `Order`'s predicate filters the orders
/// `v_user` embedded, in the statement — and the root no longer returns what `v_user`
/// stored there, which that predicate never saw.
#[tokio::test]
async fn a_nested_level_carries_its_types_predicate_over_the_embedded_documents() {
    let (result, adapter) = run(
        schema(),
        vec![],
        with_policy(OwnerPolicy::declared(&["User", "Order"])),
        "{ users { id orders { id margin } } }",
        Some(&principal()),
    )
    .await;
    result.unwrap();

    let read = adapter.captured_composed().expect("a composed read");
    assert_eq!(read.view, "v_user");
    assert_eq!(read.where_clause, Some(owner_is("u-alice")), "the root's own predicate");
    assert_eq!(read.keys, LevelKeys::Without(vec!["orders".to_string()]));
    let embed = only_embed(&read);
    assert_eq!(embed.output_key, "orders");
    assert_eq!(embed.shape, EmbedShape::Many);
    assert_eq!(
        embed.source,
        EmbedSource::Materialised {
            keys: vec!["orders".to_string()],
        }
    );
    assert_eq!(embed.level.where_clause, Some(owner_is("u-alice")), "Order's predicate");
    assert_eq!(
        embed.level.keys,
        LevelKeys::Only {
            kept:   vec!["id".to_string()],
            masked: vec!["margin".to_string()],
        },
        "a masked value never leaves the database"
    );
}

/// An opaque policy is read over `Order`'s own view, joined by the relationship.
#[tokio::test]
async fn an_opaque_policy_joins_the_nested_types_view_through_its_relationship() {
    let (result, adapter) = run(
        joinable_schema(),
        vec![],
        with_policy(OwnerPolicy::opaque(&["User", "Order"])),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    result.unwrap();

    let read = adapter.captured_composed().expect("a composed read");
    let embed = only_embed(&read);
    assert!(
        matches!(&embed.source, EmbedSource::Correlated { target_key, parent_key, .. }
            if target_key == &["fk_user".to_string()] && parent_key == &["id".to_string()]),
        "{:?}",
        embed.source
    );
    assert_eq!(embed.level.view, "v_order");
    assert_eq!(embed.level.where_clause, Some(owner_is("u-alice")));
}

/// A joined level is joined whatever the principal's predicate: which rows it holds does
/// not depend on who asks.
#[tokio::test]
async fn a_joined_level_is_joined_when_the_policy_leaves_it_unfiltered() {
    let (result, adapter) = run(
        joinable_schema(),
        vec![],
        with_policy(OwnerPolicy::opaque(&["User"])),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    result.unwrap();

    let read = adapter.captured_composed().expect("a composed read");
    let embed = only_embed(&read);
    assert!(matches!(embed.source, EmbedSource::Correlated { .. }), "{:?}", embed.source);
    assert_eq!(embed.level.where_clause, None);
}

/// Opaque, and nothing to join through: refused before anything is read.
#[tokio::test]
async fn an_opaque_policy_with_no_relationship_refuses_before_the_read() {
    let (result, adapter) = run(
        schema(),
        user_rows(),
        with_policy(OwnerPolicy::opaque(&["User", "Order"])),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(error, FraiseQLError::Authorization { .. }), "{error:?}");
    assert!(error.to_string().contains("constrained_paths"), "says what to declare: {error}");
    assert!(adapter.captured_composed().is_none() && adapter.captured_where().is_none());
}

/// …but when the policy filters nothing for the nested type, there is nothing to apply.
#[tokio::test]
async fn an_opaque_policy_that_leaves_the_nested_type_unfiltered_serves_it_flat() {
    let (result, adapter) = run(
        schema(),
        user_rows(),
        with_policy(OwnerPolicy::opaque(&["User"])),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    assert_eq!(served_orders(&result.unwrap(), &["users"]).len(), 2);
    assert!(adapter.captured_composed().is_none(), "no nested predicate, no composed read");
}

/// A policy that declares `owner` and returns a predicate on anything else is refused:
/// the declaration is what made evaluating it over embedded documents sound.
#[tokio::test]
async fn a_predicate_on_a_key_the_policy_did_not_declare_is_refused() {
    let lying = Arc::new(OwnerPolicy {
        scopes:   &["Order"],
        declares: Some(vec!["owner".to_string()]),
        reads:    "author_id",
    });
    let (result, adapter) = run(
        schema(),
        user_rows(),
        with_policy(lying),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    let error = result.unwrap_err();
    assert!(matches!(error, FraiseQLError::Authorization { .. }), "{error:?}");
    assert!(adapter.captured_composed().is_none() && adapter.captured_where().is_none());
}

/// A declared path the nested type does not declare as a field cannot be carried by its
/// embedded documents: joined when it can be, refused when it cannot.
#[tokio::test]
async fn a_declared_path_the_nested_type_lacks_is_joined_or_refused() {
    let tenant_only = || {
        Arc::new(OwnerPolicy {
            scopes:   &["Order"],
            declares: Some(vec!["tenant_id".to_string()]),
            reads:    "tenant_id",
        })
    };
    let query = "{ users { id orders { id } } }";

    let (joined, adapter) =
        run(joinable_schema(), vec![], with_policy(tenant_only()), query, Some(&principal())).await;
    joined.unwrap();
    let read = adapter.captured_composed().expect("a composed read");
    assert!(matches!(only_embed(&read).source, EmbedSource::Correlated { .. }));

    let (refused, _) =
        run(schema(), user_rows(), with_policy(tenant_only()), query, Some(&principal())).await;
    assert!(matches!(refused, Err(FraiseQLError::Authorization { .. })), "{refused:?}");
}

/// A type with no read of its own is part of its parent's row: its policy is not asked
/// and nothing is composed for it.
#[tokio::test]
async fn a_nested_value_object_is_not_asked_its_policy() {
    let policy = Arc::new(RecordingPolicy::default());
    let (result, adapter) = run(
        schema(),
        order_rows(),
        config().with_rls_policy(policy.clone()),
        "{ orders { id items { id } } }",
        Some(&principal()),
    )
    .await;
    result.unwrap();
    assert_eq!(*policy.asked.lock().unwrap(), ["Order"], "Item has no read of its own");
    assert!(adapter.captured_composed().is_none());
}

/// Two levels down: `users { orders { items } }`, with `Item` readable and scoped. Each
/// level carries its own type's predicate, and the level between is read to carry it.
#[tokio::test]
async fn a_gate_two_levels_down_is_carried_by_every_level_above_it() {
    let mut schema = schema();
    let item = schema.types.iter_mut().find(|t| t.name == "Item").unwrap();
    item.fields.push(FieldDefinition::new("owner", FieldType::String));
    schema.queries.push(list_query("items", "Item", "v_item"));
    schema.build_indexes();

    let (result, adapter) = run(
        schema,
        vec![],
        with_policy(OwnerPolicy::declared(&["Item"])),
        "{ users { orders { id items { id } } } }",
        Some(&principal()),
    )
    .await;
    result.unwrap();

    let read = adapter.captured_composed().expect("a composed read");
    let orders = only_embed(&read);
    assert_eq!(orders.level.where_clause, None, "Order is not scoped by this policy");
    assert_eq!(
        orders.level.keys,
        LevelKeys::Only {
            kept:   vec!["id".to_string()],
            masked: vec![],
        },
        "items is embedded, not kept"
    );
    let items = only_embed(&orders.level);
    assert_eq!(items.output_key, "items");
    assert_eq!(items.level.where_clause, Some(owner_is("u-alice")));
}

/// `inject_params` scope a nested type's rows as they scope its root read — with no
/// policy configured at all.
#[tokio::test]
async fn a_nested_types_inject_params_scope_its_embedded_rows() {
    let mut schema = schema();
    let orders = schema.queries.iter_mut().find(|q| q.name == "orders").unwrap();
    orders
        .inject_params
        .insert("owner".to_string(), InjectedParamSource::Jwt("sub".to_string()));
    schema.build_indexes();

    let query = "{ users { id orders { id } } }";
    let (result, adapter) = run(schema.clone(), vec![], config(), query, Some(&principal())).await;
    result.unwrap();
    let read = adapter.captured_composed().expect("a composed read");
    let embed = only_embed(&read);
    assert!(matches!(embed.source, EmbedSource::Materialised { .. }));
    assert_eq!(embed.level.where_clause, Some(owner_is("u-alice")));

    // Anonymous: nothing to resolve the scope from, so the read is refused.
    let (anonymous, adapter) = run(schema, user_rows(), config(), query, None).await;
    assert!(matches!(anonymous, Err(FraiseQLError::Validation { .. })), "{anonymous:?}");
    assert!(adapter.captured_where().is_none() && adapter.captured_composed().is_none());
}

/// A native-column scope is not in an embedded document: with nothing to join, refused.
#[tokio::test]
async fn a_nested_types_native_column_scope_is_not_evaluated_over_embedded_documents() {
    let mut schema = schema();
    let orders = schema.queries.iter_mut().find(|q| q.name == "orders").unwrap();
    orders
        .inject_params
        .insert("owner".to_string(), InjectedParamSource::Jwt("sub".to_string()));
    orders.native_columns.insert("owner".to_string(), "text".to_string());
    schema.build_indexes();

    let (result, _) = run(
        schema,
        user_rows(),
        config(),
        "{ users { id orders { id } } }",
        Some(&principal()),
    )
    .await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// The composed rows' embedded values replace what the view stored, and are projected as
/// the client selected them — aliases included.
#[tokio::test]
async fn a_composed_rows_embedded_levels_are_merged_and_projected() {
    let composed = json!({
        "d": {"id": 1, "name": "alice"},
        "e": {"orders": [{"d": {"id": 10, "margin": null}, "e": {}}]}
    });
    let (result, _) = run(
        schema(),
        vec![composed],
        with_policy(OwnerPolicy::declared(&["Order"])),
        "{ users { id os: orders { oid: id margin } } }",
        Some(&principal()),
    )
    .await;
    assert_eq!(
        result.unwrap(),
        json!({"data": {"users": [{"id": 1, "os": [{"oid": 10, "margin": null}]}]}})
    );
}

/// An adapter that cannot compose is refused from its capability, before it is called.
#[tokio::test]
async fn a_gated_nested_level_over_an_adapter_that_cannot_compose_is_refused() {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]).without_composed_reads());
    let executor = Executor::read_only_with_config(
        schema(),
        adapter.clone(),
        with_policy(OwnerPolicy::declared(&["Order"])),
    );
    let result = executor
        .execute_with_security("{ users { id orders { id } } }", None, &principal())
        .await;
    assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })), "{result:?}");
    assert!(adapter.captured_composed().is_none());
}

// ---------------------------------------------------------------------------
// REST: a leaf selection of a nested object, alongside an embed
// ---------------------------------------------------------------------------

/// `users?select=id,orders,orders(id)`-shaped: a leaf `orders` field whose type scopes
/// its rows, in a read that also embeds a relationship. The composed plan embeds
/// relationships only, so the leaf object is refused rather than served ungated.
#[tokio::test]
async fn a_composed_rest_read_selecting_a_gated_nested_object_is_refused() {
    let schema = joinable_schema();
    let users = schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
    let query_match = crate::runtime::QueryMatch::from_operation(
        users,
        vec!["id".to_string(), "orders".to_string()],
        std::collections::HashMap::new(),
        schema.find_type("User"),
    )
    .unwrap();
    let embed = crate::runtime::EmbedSelection {
        relationship: "orders".to_string(),
        output_key: "embedded".to_string(),
        fields: vec!["id".to_string()],
        limit: Some(10),
        ..crate::runtime::EmbedSelection::default()
    };
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::read_only_with_config(
        schema,
        adapter.clone(),
        with_policy(OwnerPolicy::declared(&["Order"])),
    );
    let result = executor
        // Holding `read:cost`: the whole `Order` includes `cost_price`, whose Reject would
        // refuse first, and for another reason.
        .execute_query_composed(
            &query_match,
            &[embed],
            &[],
            None,
            Some(&SecurityContext {
                roles: vec!["costing".to_string()],
                ..principal()
            }),
            None,
        )
        .await;
    assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })), "{result:?}");
    assert!(adapter.captured_composed().is_none());
}
