//! Each level a REST relay route embeds into its page is read under the caller's scope as
//! the page's rows are, by the target's own rules: its row policy, its `inject_params`, the
//! session settings the request runs under, its field-level RBAC (a masked field `null`, a
//! rejected one refusing), its role gate and the authorizer's answer about a nested read of
//! its type. Each case compares the page with the embed against the page without it, so
//! what the embed's scope removes is only ever embedded rows, never the page's.
//!
//! **Why through the executor rather than the router.** The scope needs a principal, and the
//! REST router's principal comes from the authentication layer. This module exercises the
//! engine entry the router calls for a relay route with an embed,
//! `Executor::execute_relay_composed`, with a principal built directly.
//!
//! **Parallelism:** creates and drops its own `p_relay_embed_scope` schema.

use std::{collections::HashMap, sync::Arc};

#[allow(clippy::disallowed_types)]
// Reason: the test builds the policy it passes in, which is the assertion, not a production
// construction
use fraiseql_core::security::DefaultRLSPolicy;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    prelude::{DatabaseAdapter as _, UserId},
    runtime::{CountSelection, EmbedSelection, Executor, QueryMatch, RuntimeConfig},
    schema::{
        Cardinality, CompiledSchema, CursorType, FieldDefinition, FieldDenyPolicy, FieldType,
        InjectedParamSource, QueryDefinition, Relationship, RoleDefinition, SecurityConfig,
        SessionVariableMapping, SessionVariableSource, TypeDefinition,
    },
    security::{Authorizer, AuthzDecision, AuthzRequest, SecurityContext},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p_relay_embed_scope";

/// Three users, two owned by alice and one by mallory, and three orders: one of alice's
/// user's orders is owned by **mallory**. Every row carries an `owner`.
///
/// | order | user | owner |
/// |---|---|---|
/// | 10 | 1 | u-alice |
/// | 11 | 1 | u-mallory |
/// | 20 | 2 | u-alice |
///
/// Order 11 is the discriminating row: its parent is a row alice may read, and it is not.
/// User 3 is the page's: a page read without its own scope would serve it to alice.
/// `v_order_session` is `v_order` as the request's `app.owner` setting admits it.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text, owner text)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint, owner text, \
             total bigint, margin bigint, cost_price bigint)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice', 'u-alice'), (2, 'bob', 'u-alice'), \
             (3, 'carol', 'u-mallory')"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, 'u-alice', 100, 7, 90), \
             (11, 1, 'u-mallory', 101, 8, 91), (20, 2, 'u-alice', 200, 9, 190)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT id, jsonb_build_object('id', id, 'name', name, \
             'owner', owner) AS data FROM {SCHEMA}.tb_user"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'fk_user', \
             fk_user, 'owner', owner, 'total', total, 'margin', margin, 'cost_price', cost_price) \
             AS data FROM {SCHEMA}.tb_order"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order_session AS SELECT * FROM {SCHEMA}.v_order WHERE \
             data->>'owner' = current_setting('app.owner', true)"
        ),
    ];
    for stmt in stmts {
        let _: Vec<HashMap<String, Value>> =
            adapter.execute_raw_query(&stmt).await.expect("fixture setup");
    }
}

fn scoped(name: &str, scope: &str, on_deny: FieldDenyPolicy) -> FieldDefinition {
    let mut field = FieldDefinition::new(name, FieldType::Int);
    field.requires_scope = Some(scope.to_string());
    field.on_deny = on_deny;
    field
}

/// `users`, a relay query, and `orders`, the list query `User.orders` is read through, read
/// from `orders_view`.
fn schema(orders_view: &str, edit_orders: impl FnOnce(&mut QueryDefinition)) -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", format!("{SCHEMA}.v_user"));
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("name", FieldType::String),
    ];
    user.relationships = vec![Relationship {
        name:           "orders".to_string(),
        target_type:    "Order".to_string(),
        cardinality:    Cardinality::OneToMany,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
        native_join:    None,
    }];
    schema.types.push(user);

    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.{orders_view}"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fk_user", FieldType::Int),
        FieldDefinition::new("owner", FieldType::String),
        FieldDefinition::new("total", FieldType::Int),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
    ];
    schema.types.push(order);

    let mut users = QueryDefinition::new("users", "User")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.v_user"));
    users.relay = true;
    users.relay_cursor_column = Some("id".to_string());
    users.relay_cursor_type = CursorType::Int64;
    users.auto_params.has_where = true;
    schema.queries.push(users);
    let mut orders = QueryDefinition::new("orders", "Order")
        .returning_list()
        .with_sql_source(format!("{SCHEMA}.{orders_view}"));
    orders.auto_params.has_where = true;
    orders.auto_params.has_limit = true;
    edit_orders(&mut orders);
    schema.queries.push(orders);

    // Field-level RBAC is inert without a security section. A scope is granted by a role.
    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("margin_reader", vec!["read:margin".to_string()]));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

fn principal(user: &str, roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from(user),
        roles:            roles.iter().map(ToString::to_string).collect(),
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-relay-embed-scope".to_string(),
        ip_address:       None,
        authenticated_at: fraiseql_core::fraiseql_time::Timestamp::now(),
        expires_at:       fraiseql_core::fraiseql_time::Timestamp::now()
            + fraiseql_core::fraiseql_time::jiff::SignedDuration::from_hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

struct Rig {
    executor: Executor,
    schema:   CompiledSchema,
}

/// The policy that admits the rows whose `owner` is the principal.
#[allow(clippy::disallowed_types)] // Reason: the test builds the policy it passes in, which is the assertion, not a production construction
fn owner_policy() -> Arc<DefaultRLSPolicy> {
    Arc::new(
        DefaultRLSPolicy::new()
            .with_single_tenant()
            .with_owner_field("owner".to_string()),
    )
}

async fn rig_with(
    schema: CompiledSchema,
    edit: impl FnOnce(RuntimeConfig) -> RuntimeConfig,
) -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let config = edit(RuntimeConfig::from_compiled_schema(&schema).expect("runtime config"));
    let executor = Executor::with_config_and_relay(schema.clone(), adapter, config);
    Some(Rig { executor, schema })
}

/// The owner policy over the plain views.
async fn rig() -> Option<Rig> {
    rig_with(schema("v_order", |_| {}), |c| c.with_rls_policy(owner_policy())).await
}

impl Rig {
    /// The relay page of `users` over `fields` with `embeds` and `counts`, as `principal`:
    /// each node.
    async fn page(
        &self,
        fields: &[&str],
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        principal: &SecurityContext,
    ) -> Result<Vec<Value>, FraiseQLError> {
        let definition = self.schema.queries.iter().find(|q| q.name == "users").unwrap().clone();
        let query_match = QueryMatch::from_operation(
            definition,
            fields.iter().map(ToString::to_string).collect(),
            std::iter::once(("first".to_string(), json!(10))).collect(),
            self.schema.find_type("User"),
        )
        .unwrap();
        let body = if embeds.is_empty() && counts.is_empty() {
            self.executor.execute_relay_direct(&query_match, Some(principal), None).await?
        } else {
            self.executor
                .execute_relay_composed(&query_match, embeds, counts, Some(principal), None)
                .await?
        };
        Ok(body["data"]["users"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| edge["node"].clone())
            .collect())
    }

    /// The page with `embeds`, after asserting it holds exactly the rows of the page without.
    async fn embedded(
        &self,
        fields: &[&str],
        embeds: &[EmbedSelection],
        principal: &SecurityContext,
    ) -> Vec<Value> {
        let flat = self.page(fields, &[], &[], principal).await.expect("the flat page");
        let embedded = self.page(fields, embeds, &[], principal).await.expect("the page");
        assert_eq!(ids(&flat), ids(&embedded), "the embed changed which rows the page holds");
        embedded
    }
}

fn orders(fields: &[&str]) -> EmbedSelection {
    EmbedSelection {
        relationship: "orders".to_string(),
        output_key: "orders".to_string(),
        fields: fields.iter().map(ToString::to_string).collect(),
        limit: Some(100),
        ..EmbedSelection::default()
    }
}

fn ids(rows: &[Value]) -> Vec<i64> {
    rows.iter().map(|r| r["id"].as_i64().unwrap()).collect()
}

/// The ids of `rows`' embedded orders under user `id`, sorted.
fn order_ids(rows: &[Value], id: i64) -> Vec<i64> {
    let row = rows
        .iter()
        .find(|r| r["id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("no user {id} in {rows:?}"));
    let mut ids: Vec<i64> = row["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("user {id} carries its orders: {row}"))
        .iter()
        .map(|o| o["id"].as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    ids
}

// ---------------------------------------------------------------------------

/// The target's row policy scopes the embedded rows: alice may read both users, and not
/// order 11 under user 1.
#[tokio::test]
async fn the_targets_policy_scopes_the_embedded_rows() {
    let Some(rig) = rig().await else {
        return;
    };
    let rows = rig.embedded(&["id"], &[orders(&["id"])], &principal("u-alice", &[])).await;
    assert_eq!(ids(&rows), [1, 2]);
    assert_eq!(order_ids(&rows, 1), [10], "order 11 is mallory's: {rows:?}");
    assert_eq!(order_ids(&rows, 2), [20]);
}

/// An embedded count counts the rows the target's policy admits.
#[tokio::test]
async fn an_embedded_count_counts_what_the_targets_policy_admits() {
    let Some(rig) = rig().await else {
        return;
    };
    let count = CountSelection {
        relationship: "orders".to_string(),
        output_key:   "orders_count".to_string(),
        filter:       None,
    };
    let rows = rig.page(&["id"], &[], &[count], &principal("u-alice", &[])).await.unwrap();
    assert_eq!(ids(&rows), [1, 2]);
    assert_eq!(rows[0]["orders_count"], json!(1), "order 11 is not counted: {rows:?}");
    assert_eq!(rows[1]["orders_count"], json!(1), "{rows:?}");
}

/// The target query's `inject_params` scope the embedded rows, with no row policy at all:
/// the page is every user, each user's orders only the principal's.
#[tokio::test]
async fn the_targets_inject_params_scope_the_embedded_rows() {
    let schema = schema("v_order", |orders| {
        orders
            .inject_params
            .insert("owner".to_string(), InjectedParamSource::Jwt("sub".to_string()));
    });
    let Some(rig) = rig_with(schema, |c| c).await else {
        return;
    };
    let alice = rig.embedded(&["id"], &[orders(&["id"])], &principal("u-alice", &[])).await;
    assert_eq!(order_ids(&alice, 1), [10], "{alice:?}");
    let mallory = rig.embedded(&["id"], &[orders(&["id"])], &principal("u-mallory", &[])).await;
    assert_eq!(ids(&mallory), [1, 2, 3], "the page is not scoped: {mallory:?}");
    assert_eq!(order_ids(&mallory, 1), [11], "{mallory:?}");
    assert!(order_ids(&mallory, 2).is_empty(), "{mallory:?}");
}

/// The embedded rows are read under the request's session settings: the orders view admits
/// the rows of the `app.owner` the principal's `sub` sets, and each principal sees its own.
#[tokio::test]
async fn the_embedded_rows_are_read_under_the_requests_session_settings() {
    let mut schema = schema("v_order_session", |_| {});
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.owner".to_string(),
        source: SessionVariableSource::Jwt {
            claim: "sub".to_string(),
        },
    });
    let Some(rig) = rig_with(schema, |c| c).await else {
        return;
    };
    let alice = rig.embedded(&["id"], &[orders(&["id"])], &principal("u-alice", &[])).await;
    assert_eq!(order_ids(&alice, 1), [10], "{alice:?}");
    assert_eq!(order_ids(&alice, 2), [20], "{alice:?}");
    let mallory = rig.embedded(&["id"], &[orders(&["id"])], &principal("u-mallory", &[])).await;
    assert_eq!(order_ids(&mallory, 1), [11], "{mallory:?}");
    assert!(order_ids(&mallory, 2).is_empty(), "{mallory:?}");
}

/// A masked field of the target is `null` in every embedded row unless the caller holds
/// the scope; the page's own fields are served either way.
#[tokio::test]
async fn a_masked_field_of_the_target_is_null_unless_the_scope_is_held() {
    let Some(rig) = rig().await else {
        return;
    };
    let rows = rig
        .embedded(&["id", "name"], &[orders(&["id", "margin"])], &principal("u-alice", &[]))
        .await;
    assert_eq!(rows[0]["name"], json!("alice"));
    assert!(rows[0]["orders"][0]["margin"].is_null(), "{rows:?}");
    let reader = principal("u-alice", &["margin_reader"]);
    let rows = rig.embedded(&["id"], &[orders(&["id", "margin"])], &reader).await;
    assert_eq!(rows[0]["orders"][0]["margin"], json!(7), "{rows:?}");
}

/// A rejected field of the target refuses the request.
#[tokio::test]
async fn a_rejected_field_of_the_target_refuses_the_request() {
    let Some(rig) = rig().await else {
        return;
    };
    let alice = principal("u-alice", &[]);
    rig.page(&["id"], &[], &[], &alice).await.expect("the page alone is served");
    let refused = rig.page(&["id"], &[orders(&["id", "cost_price"])], &[], &alice).await;
    assert!(matches!(refused, Err(FraiseQLError::Authorization { .. })), "{refused:?}");
}

/// The target query's role gate applies to its embedded level.
#[tokio::test]
async fn the_targets_role_gate_applies_to_its_level() {
    let schema = schema("v_order", |orders| orders.requires_role = Some("auditor".to_string()));
    let Some(rig) = rig_with(schema, |c| c.with_rls_policy(owner_policy())).await else {
        return;
    };
    let alice = principal("u-alice", &[]);
    rig.page(&["id"], &[], &[], &alice).await.expect("the page alone is served");
    let refused = rig.page(&["id"], &[orders(&["id"])], &[], &alice).await;
    assert!(refused.is_err(), "{refused:?}");
    let auditor = principal("u-alice", &["auditor"]);
    let rows = rig.embedded(&["id"], &[orders(&["id"])], &auditor).await;
    assert_eq!(order_ids(&rows, 1), [10]);
}

/// Denies a read of `Order` asked as a nested level; allows everything else.
struct DenyNestedOrders;

impl Authorizer for DenyNestedOrders {
    fn authorize(&self, req: &AuthzRequest<'_>) -> fraiseql_core::error::Result<AuthzDecision> {
        Ok(if req.target_type == Some("Order") && req.nesting.is_some() {
            AuthzDecision::Deny {
                reason: "no embedded orders".to_string(),
            }
        } else {
            AuthzDecision::Allow
        })
    }
}

/// The authorizer is asked about the embed as a nested read of its type.
#[tokio::test]
async fn the_authorizer_is_asked_about_the_embed_as_a_nested_read() {
    let Some(rig) = rig_with(schema("v_order", |_| {}), |c| {
        c.with_rls_policy(owner_policy()).with_authorizer(Arc::new(DenyNestedOrders))
    })
    .await
    else {
        return;
    };
    let alice = principal("u-alice", &[]);
    rig.page(&["id"], &[], &[], &alice).await.expect("the page alone is served");
    let refused = rig.page(&["id"], &[orders(&["id"])], &[], &alice).await;
    assert!(matches!(refused, Err(FraiseQLError::Authorization { .. })), "{refused:?}");
}
