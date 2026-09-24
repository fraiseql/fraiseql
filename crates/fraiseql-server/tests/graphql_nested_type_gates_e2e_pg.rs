//! A GraphQL selection into a nested type, against PostgreSQL.
//!
//! `{ users { orders { … } } }` is served from the `users` view's `data`, which embeds the
//! `Order` documents the view's SQL joined in. This suite asks, in the rows PostgreSQL
//! serves, whether those documents are gated as a read of `Order` would be:
//!
//! * **(a)** `Order`'s field-level RBAC — `margin` (Mask), `cost_price` (Reject);
//! * **(b)** `Order`'s RLS policy — two realistic policies over two realistic view compositions: an
//!   **owner** policy (a principal reads the orders it owns), and a **tenant** policy over a view
//!   that joins orders to their user by foreign key alone, and over one that also joins on the
//!   tenant — evaluated over the embedded documents when the policy declares its keys, and over
//!   `Order`'s own view, joined, when it does not.
//!
//! `a_nested_margin_is_masked`, `a_nested_cost_price_is_refused`,
//! `owner_policy_scopes_nested_orders` and
//! `tenant_policy_scopes_nested_orders_over_a_foreign_key_view` began as the reproductions of
//! the defect, `#[ignore]`d until the fix. Beside each is a control — the same rows read at
//! the root — so the rig is shown to gate. `a_tenant_joined_view_implies_the_tenant_policy`
//! passed before the fix as well: a view whose join carries the tenant cannot embed another
//! tenant's order. It implies nothing about an owner policy, which no view can see.
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
    runtime::{Executor, QueryMatch, RuntimeConfig},
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        Relationship, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::{
        CompiledRLSPolicy, DefaultRLSPolicy, RLSPolicy, RlsWhereClause, SecurityContext,
        rls_policy::{RLSRule, RlsTarget},
    },
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
        // A to-one: each member embeds its team. Member 2 is tenant A's, and its team is
        // tenant B's. The embedded team also carries `audit`, which `Team` does not
        // declare: a key the view stored and no selection names.
        format!(
            "CREATE TABLE {SCHEMA}.tb_team (id bigint PRIMARY KEY, tenant_id text NOT NULL, name \
             text NOT NULL, budget bigint NOT NULL)"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_member (id bigint PRIMARY KEY, tenant_id text NOT NULL, \
             fk_team bigint NOT NULL REFERENCES {SCHEMA}.tb_team(id))"
        ),
        format!("INSERT INTO {SCHEMA}.tb_team VALUES (1, 'A', 'red', 500), (2, 'B', 'blue', 700)"),
        format!("INSERT INTO {SCHEMA}.tb_member VALUES (1, 'A', 1), (2, 'A', 2)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_team AS SELECT id, jsonb_build_object('id', id, 'tenant_id', \
             tenant_id, 'name', name, 'budget', budget) AS data FROM {SCHEMA}.tb_team"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_member AS SELECT m.id, jsonb_build_object('id', m.id, \
             'tenant_id', m.tenant_id, 'fk_team', m.fk_team, 'team', (SELECT \
             jsonb_build_object('id', t.id, 'tenant_id', t.tenant_id, 'name', t.name, 'budget', \
             t.budget, 'audit', 'internal') FROM \
             {SCHEMA}.tb_team t WHERE t.id = m.fk_team)) AS data FROM {SCHEMA}.tb_member m"
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

    let mut team = TypeDefinition::new("Team", format!("{SCHEMA}.v_team"));
    team.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("name", FieldType::String),
        scoped("budget", "read:budget", FieldDenyPolicy::Mask),
    ];
    schema.types.push(team);
    let mut member = TypeDefinition::new("Member", format!("{SCHEMA}.v_member"));
    member.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("team", FieldType::Object("Team".to_string())),
    ];
    schema.types.push(member);

    for (name, return_type, view) in [
        ("users", "User", user_view),
        ("orders", "Order", "v_order"),
        ("teams", "Team", "v_team"),
        ("members", "Member", "v_member"),
    ] {
        schema.queries.push(
            QueryDefinition::new(name, return_type)
                .returning_list()
                .with_sql_source(format!("{SCHEMA}.{view}")),
        );
    }

    // A principal with no role holds no scope; `analyst` holds `read:margin`.
    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("analyst", vec!["read:margin".to_string()]));
    security.add_role(RoleDefinition::new("costing", vec!["read:cost".to_string()]));
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

/// `schema`, with `User.orders` and `Member.team` declared as the relationships they are.
fn joinable(mut schema: CompiledSchema) -> CompiledSchema {
    let declare = |schema: &mut CompiledSchema, on: &str, rel: Relationship| {
        schema.types.iter_mut().find(|t| t.name == on).unwrap().relationships.push(rel);
    };
    declare(
        &mut schema,
        "User",
        Relationship {
            name:           "orders".to_string(),
            target_type:    "Order".to_string(),
            cardinality:    Cardinality::OneToMany,
            foreign_key:    "fk_user".to_string(),
            referenced_key: "id".to_string(),
        },
    );
    declare(
        &mut schema,
        "Member",
        Relationship {
            name:           "team".to_string(),
            target_type:    "Team".to_string(),
            cardinality:    Cardinality::ManyToOne,
            foreign_key:    "fk_team".to_string(),
            referenced_key: "id".to_string(),
        },
    );
    schema.build_indexes();
    schema
}

/// A policy that does not declare the keys it reads — every out-of-tree policy written
/// before `constrained_paths` existed.
struct Opaque<P>(P);

impl<P: RLSPolicy> RLSPolicy for Opaque<P> {
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        self.0.evaluate(context, target)
    }
}

fn owner_policy() -> DefaultRLSPolicy {
    DefaultRLSPolicy::new()
        .with_single_tenant()
        .with_owner_field("owner".to_string())
}

enum Policy {
    Owner,
    Tenant,
    None,
    OpaqueOwner,
    OpaqueTenant,
}

async fn rig(user_view: &str, policy: Policy) -> Option<Executor> {
    rig_over(schema(user_view), policy).await
}

async fn rig_over(schema: CompiledSchema, policy: Policy) -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let config = RuntimeConfig::from_compiled_schema(&schema).expect("runtime config");
    let config = match policy {
        Policy::Owner => config.with_rls_policy(Arc::new(owner_policy())),
        Policy::Tenant => config.with_rls_policy(Arc::new(tenant_policy())),
        Policy::OpaqueOwner => config.with_rls_policy(Arc::new(Opaque(owner_policy()))),
        Policy::OpaqueTenant => config.with_rls_policy(Arc::new(Opaque(tenant_policy()))),
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
    (over $schema:expr, $policy:expr) => {
        match rig_over($schema, $policy).await {
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

// ---------------------------------------------------------------------------
// (b) Joined, refused, and to-one
// ---------------------------------------------------------------------------

/// A policy that does not declare its keys is read over `Order`'s own view, joined by
/// `User.orders` — over either user view, since neither is then consulted for orders.
#[tokio::test]
async fn an_opaque_owner_policy_joins_the_order_view_through_the_relationship() {
    for view in ["v_user_fk", "v_user_tenant"] {
        let executor = rig_or_skip!(over joinable(schema(view)), Policy::OpaqueOwner);
        let out = graphql(&executor, "{ users { id orders { id } } }").await.unwrap();
        assert_eq!(order_ids(&out), [10, 12], "{view}: {out}");
    }
}

/// Opaque, and no relationship to join through: refused, not served unfiltered.
#[tokio::test]
async fn an_opaque_policy_with_no_relationship_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::OpaqueOwner);
    let result = graphql(&executor, "{ users { id orders { id } } }").await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// Aliases and a masked field, through the composed read: the gated rows, each projected
/// as the client named it, `margin` null.
#[tokio::test]
async fn a_gated_nested_level_is_projected_as_selected() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, "{ users { uid: id os: orders { oid: id m: margin } } }")
        .await
        .unwrap();
    assert_eq!(
        out["data"]["users"],
        serde_json::json!([{"uid": 1, "os": [{"oid": 10, "m": null}, {"oid": 12, "m": null}]}]),
        "{out}"
    );
}

/// A to-one: tenant A's member 2 belongs to tenant B's team, which tenant A may not read
/// — `null`, whether the team is read from the member's document or joined.
#[tokio::test]
async fn a_nested_to_one_across_tenants_is_null() {
    let materialised = rig_or_skip!("v_user_fk", Policy::Tenant);
    let joined = rig_or_skip!(over joinable(schema("v_user_fk")), Policy::OpaqueTenant);
    for (how, executor) in [("materialised", materialised), ("joined", joined)] {
        let out = graphql(&executor, "{ members { id team { id name } } }").await.unwrap();
        assert_eq!(
            out["data"]["members"],
            serde_json::json!([{"id": 1, "team": {"id": 1, "name": "red"}}, {"id": 2, "team": null}]),
            "{how}: {out}"
        );
    }
}

// ---------------------------------------------------------------------------
// REST: a plain `?select=` of a declared nested-object field
// ---------------------------------------------------------------------------
//
// `members?select=id,team`, with `team` a field of `Member` rather than an embedded
// relationship, reads the whole object — every field `Team` declares — as `Team`: its
// field RBAC and its row security, as the GraphQL selection of the same fields. Read
// through `execute_query_direct`, the engine entry the REST GET resolver calls.
//
// The first three began as reproductions: a to-one came back as the stored sub-object,
// ungated, and a list as one `{}` per *stored* element.

/// `<query>?select=<fields>`, as alice.
fn rest_match(executor: &Executor, query: &str, fields: &[&str]) -> QueryMatch {
    let schema = executor.schema();
    let definition = schema.queries.iter().find(|q| q.name == query).unwrap().clone();
    let return_type = definition.return_type.clone();
    QueryMatch::from_operation(
        definition,
        fields.iter().map(ToString::to_string).collect(),
        HashMap::new(),
        schema.find_type(&return_type),
    )
    .unwrap()
}

async fn rest_select(executor: &Executor, query: &str, fields: &[&str]) -> Result<Value> {
    let query_match = rest_match(executor, query, fields);
    executor.execute_query_direct(&query_match, None, Some(&alice()), None).await
}

/// A GraphQL counterpart: `Team.budget` under `members { team }`.
#[tokio::test]
async fn a_nested_to_one_masked_field_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ members { id team { id budget } } }").await.unwrap();
    assert_eq!(
        out["data"]["members"],
        serde_json::json!([{"id": 1, "team": {"id": 1, "budget": null}}, {"id": 2, "team": {"id": 2, "budget": null}}]),
        "{out}"
    );
}

/// Tenant A's member 2 belongs to tenant B's team: `null`.
#[tokio::test]
async fn a_rest_selection_of_a_to_one_object_applies_its_tenant_policy() {
    let executor = rig_or_skip!("v_user_fk", Policy::Tenant);
    let out = rest_select(&executor, "members", &["id", "team"]).await.unwrap();
    assert_eq!(
        out["data"]["members"],
        serde_json::json!([
            {"id": 1, "team": {"id": 1, "tenant_id": "A", "name": "red", "budget": null}},
            {"id": 2, "team": null}
        ]),
        "{out}"
    );
}

/// `Team.budget` requires `read:budget`, and the object is `Team`'s declared fields — not
/// the stored sub-object, whose undeclared `audit` no selection names.
#[tokio::test]
async fn a_rest_selection_of_a_to_one_object_masks_its_masked_field() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = rest_select(&executor, "members", &["id", "team"]).await.unwrap();
    assert_eq!(
        out["data"]["members"],
        serde_json::json!([
            {"id": 1, "team": {"id": 1, "tenant_id": "A", "name": "red", "budget": null}},
            {"id": 2, "team": {"id": 2, "tenant_id": "B", "name": "blue", "budget": null}}
        ]),
        "{out}"
    );
}

/// A list: alice's orders only, each read as an `Order`.
#[tokio::test]
async fn a_rest_selection_of_a_nested_list_applies_its_owner_policy() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let out = rest_select(&executor, "users", &["id", "orders"]).await;
    // `cost_price` is Reject and is one of Order's declared fields: the whole object
    // includes it, as a REST read of `orders` with no `?select=` does.
    assert!(matches!(out, Err(FraiseQLError::Authorization { .. })), "{out:?}");

    let cleared = SecurityContext {
        roles: vec!["costing".to_string()],
        ..alice()
    };
    let query_match = rest_match(&executor, "users", &["id", "orders"]);
    let out = executor
        .execute_query_direct(&query_match, None, Some(&cleared), None)
        .await
        .unwrap();
    let orders = out["data"]["users"][0]["orders"].as_array().unwrap();
    let ids: Vec<i64> = orders.iter().map(|o| o["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, [10, 12], "{out}");
    assert!(
        orders.iter().all(|o| o["margin"].is_null() && o["cost_price"].is_i64()),
        "{out}"
    );
}

/// Streamed, a gated nested level is refused rather than streamed ungated.
#[tokio::test]
async fn a_streamed_rest_selection_of_a_gated_nested_object_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::Tenant);
    let query_match = rest_match(&executor, "members", &["id", "team"]);
    let result = executor.stream_query_direct(query_match, None, Some(alice())).await;
    assert!(matches!(result, Err(FraiseQLError::Unsupported { .. })), "{:?}", result.err());

    // Ungated, the same selection streams.
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let query_match = rest_match(&executor, "members", &["id", "team"]);
    assert!(executor.stream_query_direct(query_match, None, Some(alice())).await.is_ok());
}
