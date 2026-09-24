//! A GraphQL selection into a nested type, against PostgreSQL — reproductions.
//!
//! `{ users { orders { … } } }` is served from the `users` view's `data`, which embeds the
//! `Order` documents the view's SQL joined in. This suite asks, in the rows PostgreSQL
//! serves, whether those documents are gated as a read of `Order` would be:
//!
//! * **(a)** `Order`'s field-level RBAC — `margin` (Mask), `cost_price` (Reject);
//! * **(b)** `Order`'s RLS policy — two realistic policies over two realistic view compositions: an
//!   **owner** policy (a principal reads the orders it owns), and a **tenant** policy over a view
//!   that joins orders to their user by foreign key alone, and over one that also joins on the
//!   tenant.
//!
//! Every reproduction is `#[ignore]`d and fails when run (`-- --ignored`). Beside each is a
//! control — the same rows read at the root, where the gate applies — that passes, so the
//! rig is shown to gate. One case is expected to pass as it stands, and is not ignored: a
//! view whose join carries the tenant cannot embed another tenant's order, so it implies a
//! tenant policy. It implies nothing about an owner policy, which no view can see.
//!
//! Self-skips when no `DATABASE_URL` is set.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p_nested_gates` schema → run
//! `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::{FraiseQLError, Result},
    prelude::{DatabaseAdapter as _, UserId},
    runtime::{Executor, RuntimeConfig},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::{CompiledRLSPolicy, DefaultRLSPolicy, SecurityContext, rls_policy::RLSRule},
    types::TenantId,
};
use fraiseql_test_support::try_database_url;
use serde_json::Value;

const SCHEMA: &str = "p_nested_gates";

/// One user, alice's, in tenant `A`, and three orders pointing at it:
///
/// | order | tenant | owner |
/// |---|---|---|
/// | 10 | A | u-alice |
/// | 11 | A | u-mallory |
/// | 12 | B | u-alice |
///
/// Order 11 is the owner policy's discriminating row: same tenant, someone else's.
/// Order 12 is the tenant policy's: its foreign key names a tenant-`A` user, and it is a
/// tenant-`B` row — which a schema whose foreign keys do not carry the tenant permits.
///
/// Two user views: `v_user_fk` embeds a user's orders by `fk_user` alone; `v_user_tenant`
/// also requires the order's tenant to be the user's.
async fn seed(adapter: &PostgresAdapter) {
    let embed = |join: &str| {
        format!(
            "CREATE VIEW {SCHEMA}.{join} AS SELECT u.id, jsonb_build_object('id', u.id, 'name', \
             u.name, 'tenant_id', u.tenant_id, 'owner', u.owner, 'orders', COALESCE((SELECT \
             jsonb_agg(jsonb_build_object('id', o.id, 'tenant_id', o.tenant_id, 'owner', \
             o.owner, 'margin', o.margin, 'cost_price', o.cost_price) ORDER BY o.id) FROM \
             {SCHEMA}.tb_order o WHERE o.fk_user = u.id{}), '[]'::jsonb)) AS data FROM \
             {SCHEMA}.tb_user u",
            if join == "v_user_tenant" {
                " AND o.tenant_id = u.tenant_id"
            } else {
                ""
            }
        )
    };
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL, tenant_id \
             text NOT NULL, owner text NOT NULL)"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL \
             REFERENCES {SCHEMA}.tb_user(id), tenant_id text NOT NULL, owner text NOT NULL, \
             margin bigint NOT NULL, cost_price bigint NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice', 'A', 'u-alice')"),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, 'A', 'u-alice', 7, 90), (11, 1, 'A', \
             'u-mallory', 8, 91), (12, 1, 'B', 'u-alice', 9, 92)"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_order AS SELECT id, jsonb_build_object('id', id, 'fk_user', \
             fk_user, 'tenant_id', tenant_id, 'owner', owner, 'margin', margin, 'cost_price', \
             cost_price) AS data FROM {SCHEMA}.tb_order"
        ),
        embed("v_user_fk"),
        embed("v_user_tenant"),
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

/// `User` over `user_view`, with `orders` a list of the `Order` documents it embeds.
fn schema(user_view: &str) -> CompiledSchema {
    let mut schema = CompiledSchema::new();

    let mut user = TypeDefinition::new("User", format!("{SCHEMA}.{user_view}"));
    user.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("name", FieldType::String),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("owner", FieldType::String),
        FieldDefinition::new(
            "orders",
            FieldType::List(Box::new(FieldType::Object("Order".to_string()))),
        ),
    ];
    schema.types.push(user);

    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("owner", FieldType::String),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
    ];
    schema.types.push(order);

    for (name, return_type, view) in [("users", "User", user_view), ("orders", "Order", "v_order")]
    {
        schema.queries.push(
            QueryDefinition::new(name, return_type)
                .returning_list()
                .with_sql_source(format!("{SCHEMA}.{view}")),
        );
    }

    // A principal with no role holds no scope; `analyst` holds `read:margin`.
    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("analyst", vec!["read:margin".to_string()]));
    schema.security = Some(security);
    schema.build_indexes();
    schema
}

/// Tenant isolation, and nothing else, on every type — as a schema author declares it.
fn tenant_policy() -> CompiledRLSPolicy {
    CompiledRLSPolicy::new(
        HashMap::new(),
        Some(RLSRule {
            name:              "tenant".to_string(),
            expression:        "user.tenant_id == object.tenant_id".to_string(),
            cacheable:         false,
            cache_ttl_seconds: None,
        }),
    )
}

fn alice() -> SecurityContext {
    SecurityContext {
        user_id:          UserId::from("u-alice"),
        roles:            vec![],
        tenant_id:        Some(TenantId::from("A")),
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-nested-gates".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

enum Policy {
    Owner,
    Tenant,
    None,
}

async fn rig(user_view: &str, policy: Policy) -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let schema = schema(user_view);
    let config = RuntimeConfig::from_compiled_schema(&schema).expect("runtime config");
    let config = match policy {
        Policy::Owner => config.with_rls_policy(Arc::new(
            DefaultRLSPolicy::new()
                .with_single_tenant()
                .with_owner_field("owner".to_string()),
        )),
        Policy::Tenant => config.with_rls_policy(Arc::new(tenant_policy())),
        Policy::None => config,
    };
    Some(Executor::with_config(schema, adapter, config))
}

async fn graphql(executor: &Executor, query: &str) -> Result<Value> {
    executor.execute_with_security(query, None, &alice()).await
}

/// The margins served under every user's orders, in order-id order.
fn nested_values(response: &Value, key: &str) -> Vec<Value> {
    let users = response["data"]["users"].as_array().unwrap_or_else(|| panic!("{response}"));
    users
        .iter()
        .flat_map(|u| u["orders"].as_array().unwrap().iter())
        .map(|o| o.get(key).cloned().unwrap_or_else(|| panic!("no `{key}` in {o}")))
        .collect()
}

/// The ids of the orders served — at the root (`orders`) or under every user (`users`).
fn order_ids(response: &Value) -> Vec<i64> {
    let data = response.get("data").unwrap_or_else(|| panic!("no data: {response}"));
    let orders: Vec<&Value> = if let Some(users) = data.get("users") {
        users
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|u| u["orders"].as_array().unwrap().iter())
            .collect()
    } else {
        data["orders"].as_array().unwrap().iter().collect()
    };
    let mut ids: Vec<i64> = orders.iter().map(|o| o["id"].as_i64().unwrap()).collect();
    ids.sort_unstable();
    ids
}

macro_rules! rig_or_skip {
    ($view:expr, $policy:expr) => {
        match rig($view, $policy).await {
            Some(executor) => executor,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

// ---------------------------------------------------------------------------
// (a) Order's requires_scope, on the SQL projection path
// ---------------------------------------------------------------------------

/// Control: `Order.margin` at the root is `null` for a principal without `read:margin`.
#[tokio::test]
async fn control_a_root_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ orders { id margin } }").await.unwrap();
    let orders = out["data"]["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 3, "{out}");
    assert!(orders.iter().all(|o| o["margin"].is_null()), "{out}");
}

/// **Reproduction (a), Mask**, as PostgreSQL serves it.
#[tokio::test]
async fn a_nested_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ users { id orders { id margin } } }").await.unwrap();
    let orders = out["data"]["users"][0]["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 3, "{out}");
    assert!(
        orders.iter().all(|o| o["margin"].is_null()),
        "Order.margin served in full through users {{ orders }}: {out}"
    );
}

/// Control: `Order.cost_price` (Reject) at the root refuses.
#[tokio::test]
async fn control_a_root_cost_price_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let result = graphql(&executor, "{ orders { id cost_price } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// **Reproduction (a), Reject**, as PostgreSQL serves it.
#[tokio::test]
async fn a_nested_cost_price_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let result = graphql(&executor, "{ users { id orders { id cost_price } } }").await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through users {{ orders }}: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (a) By field, on the SQL projection path, on both entry points
// ---------------------------------------------------------------------------

/// The typed SQL projection reads `margin` and writes it under `m`, so an alias is the
/// field it names: masked at the root as it is unaliased.
#[tokio::test]
async fn an_aliased_root_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ orders { id m: margin } }").await.unwrap();
    let orders = out["data"]["orders"].as_array().unwrap();
    assert_eq!(orders.len(), 3, "{out}");
    assert!(orders.iter().all(|o| o.get("m") == Some(&Value::Null)), "{out}");
}

#[tokio::test]
async fn an_aliased_root_cost_price_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let result = graphql(&executor, "{ orders { id c: cost_price } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

#[tokio::test]
async fn an_aliased_nested_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ users { id orders { id m: margin } } }").await.unwrap();
    assert_eq!(nested_values(&out, "m"), [Value::Null, Value::Null, Value::Null], "{out}");
}

/// The level is classified against `Order`, not refused for being nested: a principal
/// holding `read:margin` is served it.
#[tokio::test]
async fn a_nested_margin_is_served_to_a_principal_holding_its_scope() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let analyst = SecurityContext {
        roles: vec!["analyst".to_string()],
        ..alice()
    };
    let out = executor
        .execute_with_security("{ users { id orders { id margin } } }", None, &analyst)
        .await
        .unwrap();
    assert_eq!(nested_values(&out, "margin"), [7, 8, 9], "{out}");
}

#[tokio::test]
async fn an_anonymous_nested_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = executor.execute("{ users { id orders { id margin } } }", None).await.unwrap();
    assert_eq!(nested_values(&out, "margin"), [Value::Null, Value::Null, Value::Null], "{out}");
}

#[tokio::test]
async fn an_anonymous_nested_cost_price_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let result = executor.execute("{ users { id orders { id cost_price } } }", None).await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

// ---------------------------------------------------------------------------
// (b) Order's RLS policy — owner-scoped
// ---------------------------------------------------------------------------

/// Control: alice reads her own orders at the root — 10 and 12, not mallory's 11.
#[tokio::test]
async fn control_b_owner_policy_scopes_root_orders() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, "{ orders { id } }").await.unwrap();
    assert_eq!(order_ids(&out), [10, 12], "{out}");
}

/// **Reproduction (b), owner.** Alice may read her user; she may not read order 11. No
/// view can imply an owner policy — the view is composed for no principal — so this is
/// reachable over the tenant-joined view as much as the foreign-key one.
#[tokio::test]
#[ignore = "reproduction: a nested selection does not apply its own type's RLS policy"]
async fn owner_policy_scopes_nested_orders() {
    let mut served = Vec::new();
    for view in ["v_user_fk", "v_user_tenant"] {
        let executor = rig_or_skip!(view, Policy::Owner);
        let out = graphql(&executor, "{ users { id orders { id } } }").await.unwrap();
        served.push((view, order_ids(&out)));
    }
    assert!(
        served.iter().all(|(_, ids)| !ids.contains(&11)),
        "mallory's order 11 served to alice through users {{ orders }}: {served:?}"
    );
}

// ---------------------------------------------------------------------------
// (b) Order's RLS policy — tenant-scoped
// ---------------------------------------------------------------------------

/// Control: tenant `A` reads orders 10 and 11 at the root, not tenant `B`'s 12.
#[tokio::test]
async fn control_b_tenant_policy_scopes_root_orders() {
    let executor = rig_or_skip!("v_user_fk", Policy::Tenant);
    let out = graphql(&executor, "{ orders { id } }").await.unwrap();
    assert_eq!(order_ids(&out), [10, 11], "{out}");
}

/// **Reproduction (b), tenant, over a view that joins by foreign key alone.** Tenant `B`'s
/// order 12 points at a tenant-`A` user, so the view embeds it under a row tenant `A`
/// may read.
#[tokio::test]
#[ignore = "reproduction: a nested selection does not apply its own type's RLS policy"]
async fn tenant_policy_scopes_nested_orders_over_a_foreign_key_view() {
    let executor = rig_or_skip!("v_user_fk", Policy::Tenant);
    let out = graphql(&executor, "{ users { id orders { id } } }").await.unwrap();
    assert!(
        !order_ids(&out).contains(&12),
        "tenant B's order 12 served to tenant A through users {{ orders }}: {out}"
    );
}

/// **Not a reproduction: the view implies it.** A view whose join also requires the order's
/// tenant to be the user's cannot embed order 12 under a tenant-`A` user, so the parent's
/// tenant predicate covers the nested level. Passes today; it is what the tenant half of
/// (b) depends on.
#[tokio::test]
async fn a_tenant_joined_view_implies_the_tenant_policy() {
    let executor = rig_or_skip!("v_user_tenant", Policy::Tenant);
    let out = graphql(&executor, "{ users { id orders { id } } }").await.unwrap();
    assert_eq!(order_ids(&out), [10, 11], "{out}");
}
