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
//! The depth section asks the same at every depth, over a chain of `Folder` to-ones: no
//! stored document leaves unprojected, however deep the selection that reaches it.
//!
//! The mutation section asks it of a mutation's result selection: `touchUser` returns the
//! same `User` document `users` reads, and `touchOrder` an `Order`. The cascade section asks
//! it of a cascade payload's `entity`, and of each entity its `cascade.updated` reports; the
//! error section of an error payload, projected from the write's `error_detail`. Their
//! reproductions began `#[ignore]`d, each beside a control; a refusal there is decided before
//! the write, which `tb_write` shows never ran.
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
        ArgumentDefinition, Cardinality, CompiledSchema, FieldDefinition, FieldDenyPolicy,
        FieldType, MutationDefinition, MutationOperation, QueryDefinition, Relationship,
        RoleDefinition, SecurityConfig, TypeDefinition, UnionDefinition,
    },
    security::{
        Authorizer, AuthzDecision, AuthzRequest, CompiledRLSPolicy, ConstrainedPaths,
        DefaultRLSPolicy, RLSPolicy, RlsWhereClause, SecurityContext,
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
    // `fn_touch_<table>(id)`: rewrite the row unchanged, log the write in `tb_write`, and
    // return the row as `view` serves it.
    let touch = |table: &str, entity_type: &str, view: &str| {
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_touch_{table}(p_id bigint) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN UPDATE \
             {SCHEMA}.tb_{table} SET id = id WHERE id = p_id; INSERT INTO {SCHEMA}.tb_write \
             VALUES ('{table}', p_id); v.succeeded := true; \
             v.state_changed := true; v.message := 'touched'; v.entity_type := '{entity_type}'; \
             v.entity := (SELECT data FROM {SCHEMA}.{view} WHERE id = p_id); RETURN v; END $$"
        )
    };
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
        // Two to-ones deep across two types: each badge embeds its holder, which embeds its
        // team. Badge 2's holder is tenant A's and its team tenant B's; badge 3's holder is
        // tenant B's, and its team tenant A's `red`.
        format!(
            "CREATE TABLE {SCHEMA}.tb_holder (id bigint PRIMARY KEY, tenant_id text NOT NULL, \
             fk_team bigint NOT NULL REFERENCES {SCHEMA}.tb_team(id))"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_badge (id bigint PRIMARY KEY, tenant_id text NOT NULL, \
             fk_holder bigint NOT NULL REFERENCES {SCHEMA}.tb_holder(id))"
        ),
        format!("INSERT INTO {SCHEMA}.tb_holder VALUES (1, 'A', 1), (2, 'A', 2), (3, 'B', 1)"),
        format!("INSERT INTO {SCHEMA}.tb_badge VALUES (1, 'A', 1), (2, 'A', 2), (3, 'A', 3)"),
        format!(
            "CREATE FUNCTION {SCHEMA}.holder_doc(hid bigint) RETURNS jsonb LANGUAGE sql STABLE \
             AS $$ SELECT jsonb_build_object('id', h.id, 'tenant_id', h.tenant_id, 'fk_team', \
             h.fk_team, 'team', (SELECT jsonb_build_object('id', t.id, 'tenant_id', t.tenant_id, \
             'name', t.name, 'budget', t.budget) FROM {SCHEMA}.tb_team t WHERE t.id = \
             h.fk_team)) FROM {SCHEMA}.tb_holder h WHERE h.id = hid $$"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_holder AS SELECT id, {SCHEMA}.holder_doc(id) AS data FROM \
             {SCHEMA}.tb_holder"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_badge AS SELECT b.id, jsonb_build_object('id', b.id, \
             'tenant_id', b.tenant_id, 'fk_holder', b.fk_holder, 'holder', \
             {SCHEMA}.holder_doc(b.fk_holder)) AS data FROM {SCHEMA}.tb_badge b"
        ),
        // A chain of to-ones as deep as a selection can reach: folder `k`'s parent is
        // folder `k - 1`, and each document embeds every ancestor. Folder 2 is mallory's,
        // so it sits six levels under folder 8.
        format!(
            "CREATE TABLE {SCHEMA}.tb_folder (id bigint PRIMARY KEY, fk_parent bigint \
             REFERENCES {SCHEMA}.tb_folder(id), tenant_id text NOT NULL, owner text NOT NULL, \
             margin bigint NOT NULL, cost_price bigint NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_folder SELECT k, NULLIF(k - 1, 0), 'A', CASE k WHEN 2 THEN \
             'u-mallory' ELSE 'u-alice' END, k, 100 + k FROM generate_series(1, 60) AS k"
        ),
        format!(
            "CREATE FUNCTION {SCHEMA}.folder_doc(fid bigint, lvl int) RETURNS jsonb LANGUAGE \
             plpgsql STABLE AS $$ BEGIN RETURN (SELECT jsonb_build_object('id', f.id, \
             'fk_parent', f.fk_parent, 'tenant_id', f.tenant_id, 'owner', f.owner, 'margin', \
             f.margin, 'cost_price', f.cost_price, 'audit', 'internal', 'parent', CASE WHEN lvl \
             > 0 AND f.fk_parent IS NOT NULL THEN {SCHEMA}.folder_doc(f.fk_parent, lvl - 1) END) \
             FROM {SCHEMA}.tb_folder f WHERE f.id = fid); END $$"
        ),
        format!(
            "CREATE VIEW {SCHEMA}.v_folder AS SELECT id, {SCHEMA}.folder_doc(id, 59) AS data FROM \
             {SCHEMA}.tb_folder"
        ),
        // The `app.mutation_response` contract, provisioned idempotently as the other
        // mutation suites do, and two writes that return what a read of their row serves:
        // `touchUser` the `v_user_fk` document, with every order it embeds.
        "CREATE SCHEMA IF NOT EXISTS app".to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
         'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
         'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, state_changed \
         BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, http_status \
         SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
         updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
         EXCEPTION WHEN duplicate_object THEN NULL; END $$;"
            .to_string(),
        format!("CREATE TABLE {SCHEMA}.tb_write (tbl text NOT NULL, id bigint NOT NULL)"),
        touch("user", "User", "v_user_fk"),
        touch("order", "Order", "v_order"),
        // `touchUserCascade`: the `touchUser` write, whose cascade reports the user and each
        // of its orders as updated — every entity as its own view serves it.
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_cascade_user(p_id bigint) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN UPDATE \
             {SCHEMA}.tb_user SET id = id WHERE id = p_id; INSERT INTO {SCHEMA}.tb_write VALUES \
             ('user', p_id); v.succeeded := true; v.state_changed := true; v.message := \
             'touched'; v.entity_type := 'User'; v.entity := (SELECT data FROM \
             {SCHEMA}.v_user_fk WHERE id = p_id); v.cascade := jsonb_build_object('updated', \
             (SELECT jsonb_build_object('__typename', 'User', 'id', u.id, 'operation', \
             'UPDATED', 'entity', u.data) FROM {SCHEMA}.v_user_fk u WHERE u.id = p_id) || \
             COALESCE((SELECT jsonb_agg(jsonb_build_object('__typename', 'Order', 'id', o.id, \
             'operation', 'UPDATED', 'entity', o.data) ORDER BY o.id) FROM {SCHEMA}.v_order o \
             WHERE (o.data->>'fk_user')::bigint = p_id), '[]'::jsonb), 'deleted', \
             '[]'::jsonb); RETURN v; END $$"
        ),
        // `failOrder`: a refused write, whose error detail carries the order's scoped fields.
        format!(
            "CREATE FUNCTION {SCHEMA}.fn_fail_order(p_id bigint) RETURNS app.mutation_response \
             LANGUAGE plpgsql AS $$ DECLARE v app.mutation_response; BEGIN v.succeeded := \
             false; v.state_changed := false; v.error_class := 'conflict'; v.message := \
             'order is locked'; v.entity_type := 'OrderConflict'; v.error_detail := (SELECT \
             jsonb_build_object('order_id', id, 'margin', margin, 'cost_price', cost_price) \
             FROM {SCHEMA}.tb_order WHERE id = p_id); RETURN v; END $$"
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
    let mut holder = TypeDefinition::new("Holder", format!("{SCHEMA}.v_holder"));
    holder.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("team", FieldType::Object("Team".to_string())),
    ];
    schema.types.push(holder);
    let mut badge = TypeDefinition::new("Badge", format!("{SCHEMA}.v_badge"));
    badge.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("holder", FieldType::Object("Holder".to_string())),
    ];
    schema.types.push(badge);
    let mut folder = TypeDefinition::new("Folder", format!("{SCHEMA}.v_folder"));
    folder.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("tenant_id", FieldType::String),
        FieldDefinition::new("owner", FieldType::String),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
        FieldDefinition::new("parent", FieldType::Object("Folder".to_string())),
    ];
    schema.types.push(folder);

    for (name, return_type, view) in [
        ("users", "User", user_view),
        ("orders", "Order", "v_order"),
        ("teams", "Team", "v_team"),
        ("members", "Member", "v_member"),
        ("holders", "Holder", "v_holder"),
        ("badges", "Badge", "v_badge"),
        ("folders", "Folder", "v_folder"),
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

/// A policy that declares its keys for every type but `.0`: a type it is opaque for is read
/// through its declared relationship, the others over the embedded document — so both kinds
/// of nested row gate meet on one path.
struct OpaqueFor<P>(&'static [&'static str], P);

impl<P: RLSPolicy> RLSPolicy for OpaqueFor<P> {
    fn evaluate(
        &self,
        context: &SecurityContext,
        target: &RlsTarget<'_>,
    ) -> Result<Option<RlsWhereClause>> {
        self.1.evaluate(context, target)
    }

    fn constrained_paths(&self, target: &RlsTarget<'_>) -> ConstrainedPaths {
        if target.type_name.is_some_and(|t| self.0.contains(&t)) {
            ConstrainedPaths::Opaque
        } else {
            self.1.constrained_paths(target)
        }
    }
}

fn owner_policy() -> DefaultRLSPolicy {
    DefaultRLSPolicy::new()
        .with_single_tenant()
        .with_owner_field("owner".to_string())
}

#[derive(Clone, Copy)]
enum Policy {
    Owner,
    Tenant,
    None,
    OpaqueOwner,
    OpaqueTenant,
    /// The tenant policy, opaque for the named types only.
    OpaqueTenantFor(&'static [&'static str]),
}

async fn rig(user_view: &str, policy: Policy) -> Option<Executor> {
    rig_over(schema(user_view), policy).await
}

async fn rig_over(schema: CompiledSchema, policy: Policy) -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let config = policy_config(&schema, policy);
    Some(Executor::with_config(schema, adapter, config))
}

fn policy_config(schema: &CompiledSchema, policy: Policy) -> RuntimeConfig {
    let config = RuntimeConfig::from_compiled_schema(schema).expect("runtime config");
    match policy {
        Policy::Owner => config.with_rls_policy(Arc::new(owner_policy())),
        Policy::Tenant => config.with_rls_policy(Arc::new(tenant_policy())),
        Policy::OpaqueOwner => config.with_rls_policy(Arc::new(Opaque(owner_policy()))),
        Policy::OpaqueTenant => config.with_rls_policy(Arc::new(Opaque(tenant_policy()))),
        Policy::OpaqueTenantFor(types) => {
            config.with_rls_policy(Arc::new(OpaqueFor(types, tenant_policy())))
        },
        Policy::None => config,
    }
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

// ---------------------------------------------------------------------------
// Relay connections and `node(id:)`
// ---------------------------------------------------------------------------
//
// Both Relay runners compose the root's RLS and `inject_params` into their WHERE, and
// neither ran field-level RBAC: a connection served each row's stored `data` as its
// `node`, whatever the selection; `node(id:)` projected the selection, unclassified, and
// a nested object whole. So `Order`'s scopes did not reach an `Order` read through either,
// at the root or nested, and `Order`'s policy did not reach the orders a `User` embeds.
// Each reproduction sits beside a control showing the same runner applies the root's row
// gate. Now `node(id:)` is read as the GraphQL root is — classified at every level, and
// composed when a nested level is row-gated — and so is a connection: it projects and
// classifies each `node`, and a nested level whose type scopes its rows is read as a
// level of a composed statement whose root is the keyset page itself.

/// `schema`, with a Relay connection over each list: `ordersPage`, `usersPage`,
/// `foldersPage` and `membersPage`.
fn relay_schema(user_view: &str) -> CompiledSchema {
    let mut schema = schema(user_view);
    for (name, return_type, view) in [
        ("ordersPage", "Order", "v_order"),
        ("usersPage", "User", user_view),
        ("foldersPage", "Folder", "v_folder"),
        ("membersPage", "Member", "v_member"),
    ] {
        let mut query = QueryDefinition::new(name, return_type)
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.{view}"));
        query.relay = true;
        query.relay_cursor_column = Some("id".to_string());
        schema.queries.push(query);
    }
    schema.build_indexes();
    schema
}

async fn relay_rig(user_view: &str, policy: Policy) -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let schema = relay_schema(user_view);
    let config = policy_config(&schema, policy);
    Some(Executor::with_config_and_relay(schema, adapter, config))
}

macro_rules! relay_rig_or_skip {
    ($view:expr, $policy:expr) => {
        match relay_rig($view, $policy).await {
            Some(executor) => executor,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

/// Every `node` of the connection at `data.<field>`.
fn relay_nodes(response: &Value, field: &str) -> Vec<Value> {
    response["data"][field]["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("no edges: {response}"))
        .iter()
        .map(|edge| edge["node"].clone())
        .collect()
}

/// Control: a connection over `Order` is read under `Order`'s owner policy.
#[tokio::test]
async fn control_a_relay_connection_applies_its_types_policy() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, "{ ordersPage(first: 10) { edges { node { id } } } }")
        .await
        .unwrap();
    let mut ids: Vec<i64> = relay_nodes(&out, "ordersPage")
        .iter()
        .map(|o| o["id"].as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, [10, 12], "{out}");
}

/// **Reproduction.** `Order.margin` through a connection over `Order`.
#[tokio::test]
async fn a_relay_node_margin_is_masked() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ ordersPage(first: 10) { edges { node { id margin } } } }")
        .await
        .unwrap();
    let nodes = relay_nodes(&out, "ordersPage");
    assert_eq!(nodes.len(), 3, "{out}");
    assert!(
        nodes.iter().all(|o| o["margin"].is_null()),
        "Order.margin served in full: {out}"
    );
}

/// **Reproduction.** `Order.cost_price` (Reject) through a connection over `Order`.
#[tokio::test]
async fn a_relay_node_cost_price_is_refused() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::None);
    let result =
        graphql(&executor, "{ ordersPage(first: 10) { edges { node { id cost_price } } } }").await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through a connection: {result:?}"
    );
}

/// **Reproduction.** A `node` carries what was selected — not the stored document, whose
/// other keys include every gated field the caller never asked for.
#[tokio::test]
async fn a_relay_node_serves_only_its_selection() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, "{ ordersPage(first: 10) { edges { node { id } } } }")
        .await
        .unwrap();
    let nodes = relay_nodes(&out, "ordersPage");
    assert_eq!(nodes.len(), 3, "{out}");
    for node in &nodes {
        let keys: Vec<&String> = node.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["id"], "margin and cost_price served unselected: {out}");
    }
}

/// A connection's nested orders follow `Order`'s owner policy: alice's orders 10 and 12,
/// not mallory's 11. `v_user_fk` embeds all three and the view knows nothing of the
/// principal. Was refused (403) while a keyset page could not carry a composed level.
#[tokio::test]
async fn relay_nested_orders_follow_the_owner_policy() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::Owner);
    let out =
        graphql(&executor, "{ usersPage(first: 10) { edges { node { id orders { id } } } } }")
            .await
            .unwrap();
    let nodes = relay_nodes(&out, "usersPage");
    let ids: Vec<i64> = nodes[0]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("no orders: {out}"))
        .iter()
        .map(|o| o["id"].as_i64().unwrap())
        .collect();
    assert_eq!(ids, [10, 12], "{out}");
}

/// …and so does a to-one: under a tenant policy, the team tenant A's member 2 embeds is
/// tenant B's, and it is `null`.
#[tokio::test]
async fn a_relay_nested_to_one_follows_its_tenant_policy() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::Tenant);
    let out = graphql(
        &executor,
        "{ membersPage(first: 10) { edges { node { id team { id name } } } } }",
    )
    .await
    .unwrap();
    assert_eq!(
        Value::Array(relay_nodes(&out, "membersPage")),
        serde_json::json!([{"id": 1, "team": {"id": 1, "name": "red"}}, {"id": 2, "team": null}]),
        "{out}"
    );
}

/// The page, its cursors, `pageInfo` and `totalCount` are the relay adapter's, whichever
/// read serves them: selecting a row-gated level (`parent`, under `Folder`'s owner policy)
/// reads the keyset page as a composed root, selecting `id` alone reads it flat, and the
/// two agree forward and backward, from the start and past a cursor. Folder 2 is mallory's
/// and is on neither page; folder 3's parent is, and is `null`.
#[tokio::test]
async fn a_composed_relay_page_is_the_page_the_flat_read_returns() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::Owner);
    let page = |args: &str, node: &str| {
        format!(
            "{{ foldersPage({args}) {{ totalCount pageInfo {{ hasNextPage hasPreviousPage \
             startCursor endCursor }} edges {{ cursor node {{ {node} }} }} }} }}"
        )
    };
    let cursors = |out: &Value| -> Vec<String> {
        out["data"]["foldersPage"]["edges"]
            .as_array()
            .unwrap_or_else(|| panic!("no edges: {out}"))
            .iter()
            .map(|e| e["cursor"].as_str().unwrap().to_string())
            .collect()
    };
    let first = graphql(&executor, &page("first: 5", "id")).await.unwrap();
    let third = cursors(&first)[2].clone();
    let last = graphql(&executor, &page("last: 5", "id")).await.unwrap();
    let third_last = cursors(&last)[2].clone();

    for args in [
        "first: 5".to_string(),
        format!(r#"first: 5, after: "{third}""#),
        "last: 5".to_string(),
        format!(r#"last: 5, before: "{third_last}""#),
    ] {
        let flat = graphql(&executor, &page(&args, "id")).await.unwrap();
        let composed = graphql(&executor, &page(&args, "id parent { id }")).await.unwrap();
        let (f, c) = (&flat["data"]["foldersPage"], &composed["data"]["foldersPage"]);
        assert_eq!(f["totalCount"], c["totalCount"], "{args}: {composed}");
        assert_eq!(f["pageInfo"], c["pageInfo"], "{args}: {composed}");
        assert_eq!(cursors(&flat), cursors(&composed), "{args}: {composed}");
        let ids = |v: &Value| -> Vec<Value> {
            v["edges"].as_array().unwrap().iter().map(|e| e["node"]["id"].clone()).collect()
        };
        assert_eq!(ids(f), ids(c), "{args}: {composed}");
        assert!(!ids(c).contains(&serde_json::json!(2)), "mallory's folder: {composed}");
    }

    let from_start = graphql(&executor, &page("first: 5", "id parent { id }")).await.unwrap();
    let three = Value::Array(relay_nodes(&from_start, "foldersPage"))
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == 3)
        .cloned()
        .unwrap_or_else(|| panic!("no folder 3: {from_start}"));
    assert!(three["parent"].is_null(), "folder 3's parent is mallory's: {from_start}");
}

/// A connection that stops above the row-gated level is served: the refusal is the
/// nested level's, not the connection's.
#[tokio::test]
async fn a_relay_connection_that_stops_above_a_row_gated_level_is_served() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, "{ usersPage(first: 10) { edges { node { id name } } } }")
        .await
        .unwrap();
    let nodes = relay_nodes(&out, "usersPage");
    assert_eq!(nodes.len(), 1, "{out}");
    assert_eq!(nodes[0]["name"], "alice", "{out}");
}

/// **Reproduction.** `Order.margin` nested under a connection over `User`.
#[tokio::test]
async fn a_relay_nested_margin_is_masked() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(
        &executor,
        "{ usersPage(first: 10) { edges { node { id orders { id margin } } } } }",
    )
    .await
    .unwrap();
    let orders: Vec<Value> = relay_nodes(&out, "usersPage")
        .iter()
        .flat_map(|u| u["orders"].as_array().cloned().unwrap_or_default())
        .collect();
    assert_eq!(orders.len(), 3, "{out}");
    assert!(orders.iter().all(|o| o["margin"].is_null()), "{out}");
}

fn node_query(type_name: &str, id: &str, selection: &str) -> String {
    let node_id = fraiseql_core::runtime::relay::encode_node_id(type_name, id);
    format!(r#"{{ node(id: "{node_id}") {{ ... on {type_name} {{ {selection} }} }} }}"#)
}

/// Control: `node(id:)` resolves an `Order` under `Order`'s owner policy — mallory's
/// order 11 is not found for alice, her order 10 is.
#[tokio::test]
async fn control_a_node_lookup_applies_its_types_policy() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let theirs = graphql(&executor, &node_query("Order", "11", "id")).await.unwrap();
    assert!(theirs["data"]["node"].is_null(), "{theirs}");
    let hers = graphql(&executor, &node_query("Order", "10", "id")).await.unwrap();
    assert_eq!(hers["data"]["node"]["id"], 10, "{hers}");
}

/// **Reproduction.** `Order.margin` through `node(id:)`.
#[tokio::test]
async fn a_node_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &node_query("Order", "10", "id margin")).await.unwrap();
    assert_eq!(out["data"]["node"]["id"], 10, "{out}");
    assert!(out["data"]["node"]["margin"].is_null(), "Order.margin served in full: {out}");
}

/// **Reproduction.** `Order.cost_price` (Reject) through `node(id:)`.
#[tokio::test]
async fn a_node_cost_price_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let result = graphql(&executor, &node_query("Order", "10", "id cost_price")).await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through node(id:): {result:?}"
    );
}

/// **Reproduction.** A `User` resolved by `node(id:)` embeds mallory's order 11.
#[tokio::test]
async fn node_nested_orders_follow_the_owner_policy() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, &node_query("User", "1", "id orders { id }")).await.unwrap();
    let mut ids: Vec<i64> = out["data"]["node"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("no orders: {out}"))
        .iter()
        .map(|o| o["id"].as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, [10, 12], "{out}");
}

/// **Reproduction.** `Order.margin` nested in a `User` resolved by `node(id:)`.
#[tokio::test]
async fn a_node_nested_margin_is_masked() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &node_query("User", "1", "id orders { id margin }"))
        .await
        .unwrap();
    let orders = out["data"]["node"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("no orders: {out}"));
    assert_eq!(orders.len(), 3, "{out}");
    assert!(orders.iter().all(|o| o["margin"].is_null()), "{out}");
}

/// #422: `node(id:)` is put to the authorizer at the operation gate as `node`, before the
/// id says what it reads; once it does, the read is asked again as a read of that type. A
/// rule on `target_type` holds an `Order` read through `node` as through `orders`, and a
/// `User` node's nested orders as a nested read.
#[tokio::test]
async fn a_node_lookup_is_put_to_the_authorizer_as_its_type() {
    struct DenyOrders;
    impl Authorizer for DenyOrders {
        fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
            Ok(if req.target_type == Some("Order") {
                AuthzDecision::Deny {
                    reason: "no orders".to_string(),
                }
            } else {
                AuthzDecision::Allow
            })
        }
    }
    let Some(url) = try_database_url() else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let schema = relay_schema("v_user_fk");
    let config = policy_config(&schema, Policy::None).with_authorizer(Arc::new(DenyOrders));
    let executor = Executor::with_config_and_relay(schema, adapter, config);

    let order = graphql(&executor, &node_query("Order", "10", "id")).await;
    assert!(matches!(order, Err(FraiseQLError::Authorization { .. })), "{order:?}");
    let user = graphql(&executor, &node_query("User", "1", "id")).await.unwrap();
    assert_eq!(user["data"]["node"]["id"], 1, "{user}");
    let nested = graphql(&executor, &node_query("User", "1", "id orders { id }")).await;
    assert!(matches!(nested, Err(FraiseQLError::Authorization { .. })), "{nested:?}");
}

/// A nested object is projected through its sub-selection, not served whole: the orders a
/// `User` node embeds carry `id` and nothing else they store.
#[tokio::test]
async fn a_node_nested_object_serves_only_its_selection() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &node_query("User", "1", "id orders { id }")).await.unwrap();
    let orders = out["data"]["node"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("no orders: {out}"));
    assert_eq!(orders.len(), 3, "{out}");
    for order in orders {
        let keys: Vec<&String> = order.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["id"], "owner, tenant_id, margin, cost_price served unselected: {out}");
    }
}

/// …and so is a nested to-one: the team a `Member` node embeds carries what was selected,
/// not its stored `budget` (Mask) or `audit` (declared nowhere).
#[tokio::test]
async fn a_node_nested_to_one_serves_only_its_selection() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &node_query("Member", "1", "id team { id }")).await.unwrap();
    let team = out["data"]["node"]["team"]
        .as_object()
        .unwrap_or_else(|| panic!("no team: {out}"));
    let keys: Vec<&String> = team.keys().collect();
    assert_eq!(keys, ["id"], "{out}");
}

// ---------------------------------------------------------------------------
// At every depth
// ---------------------------------------------------------------------------
//
// Both projectors used to recurse into a nested object only so deep — the SQL one to
// `MAX_PROJECTION_DEPTH`, the Rust one to `MAX_ENTITY_PROJECTION_DEPTH`, both 4 — and pass
// what lay below through as it was stored. Every *selected* field was still classified,
// masked and row-gated at any depth; what passed through was never selected. The
// invariant now: **no stored document leaves unprojected, at any depth.** The projectors
// follow the selection, which `max_query_depth` bounds (`DEFAULT_MAX_QUERY_DEPTH` when
// undeclared); a cap that survives refuses when it is hit.

/// `parent { … }`, `depth` deep, around `leaf`.
fn ancestors(depth: usize, leaf: &str) -> String {
    format!("{}{leaf}{}", "parent { ".repeat(depth), " }".repeat(depth))
}

/// Folder `id`'s ancestor `depth` levels up, out of a list of folders.
fn ancestor_of(folders: &Value, id: i64, depth: usize) -> Value {
    let folder = folders
        .as_array()
        .and_then(|all| all.iter().find(|f| f["id"] == id))
        .unwrap_or_else(|| panic!("no folder {id}: {folders}"));
    (0..depth).fold(folder.clone(), |level, _| level["parent"].clone())
}

fn ancestor_of_8(folders: &Value, depth: usize) -> Value {
    ancestor_of(folders, 8, depth)
}

fn keys(object: &Value) -> Vec<&String> {
    object
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {object}"))
        .keys()
        .collect()
}

/// Ten levels, and the deepest selection every entry can carry: `graphql-parser` refuses
/// a document nested past 50 brackets, and a node or Relay selection spends a few of them
/// above `parent`. Depth 50 itself is exercised on the projectors directly, which parse
/// nothing (`project_entity_projects_an_object_at_every_depth`,
/// `test_typed_projection_recurses_at_every_depth`).
const DEPTHS: [usize; 2] = [10, 44];

/// `schema`, declaring a `max_query_depth` the deepest selection fits under.
fn deep(mut schema: CompiledSchema) -> CompiledSchema {
    schema.validation_config = Some(fraiseql_core::schema::ValidationConfig {
        max_query_depth: Some(64),
        ..Default::default()
    });
    schema.build_indexes();
    schema
}

/// Control: four levels down, an ancestor carries its selection and nothing else.
#[tokio::test]
async fn control_an_ancestor_four_deep_serves_only_its_selection() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &format!("{{ folders {{ id {} }} }}", ancestors(4, "id")))
        .await
        .unwrap();
    let fourth = ancestor_of_8(&out["data"]["folders"], 4);
    assert_eq!(keys(&fourth), ["id"], "{out}");
}

/// Five levels down — was the reproduction: the ancestor was its stored document, `margin`
/// unmasked, `cost_price` unrefused, `audit`, and every ancestor beneath it.
#[tokio::test]
async fn an_ancestor_five_deep_serves_only_its_selection() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let out = graphql(&executor, &format!("{{ folders {{ id {} }} }}", ancestors(5, "id")))
        .await
        .unwrap();
    let fifth = ancestor_of_8(&out["data"]["folders"], 5);
    assert_eq!(keys(&fifth), ["id"], "{out}");
}

/// …through `node(id:)` — was the reproduction.
#[tokio::test]
async fn a_node_ancestor_five_deep_serves_only_its_selection() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let query = node_query("Folder", "8", &format!("id {}", ancestors(5, "id")));
    let out = graphql(&executor, &query).await.unwrap();
    let fifth = (0..5).fold(out["data"]["node"].clone(), |level, _| level["parent"].clone());
    assert_eq!(keys(&fifth), ["id"], "{out}");
}

/// …through a Relay connection — was the reproduction.
#[tokio::test]
async fn a_relay_ancestor_five_deep_serves_only_its_selection() {
    let executor = relay_rig_or_skip!("v_user_fk", Policy::None);
    let query = format!(
        "{{ foldersPage(first: 10) {{ edges {{ node {{ id {} }} }} }} }}",
        ancestors(5, "id")
    );
    let out = graphql(&executor, &query).await.unwrap();
    let nodes = Value::Array(relay_nodes(&out, "foldersPage"));
    let fifth = ancestor_of_8(&nodes, 5);
    assert_eq!(keys(&fifth), ["id"], "{out}");
}

/// Folder 60's ancestor `depth` levels up, selecting `leaf`, through each GraphQL entry
/// that projects a stored document: the root, `node(id:)` and a Relay connection.
async fn every_entry(depth: usize, leaf: &str) -> Option<Vec<(&'static str, Result<Value>)>> {
    let root = rig_over(deep(schema("v_user_fk")), Policy::None).await?;
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    let relay_schema = deep(relay_schema("v_user_fk"));
    let config = policy_config(&relay_schema, Policy::None);
    let relay = Executor::with_config_and_relay(relay_schema, adapter, config);

    let chain = ancestors(depth, leaf);
    let pick = |folders: &Value| ancestor_of(folders, 60, depth);
    let mut out = Vec::new();
    let r = graphql(&root, &format!("{{ folders {{ id {chain} }} }}")).await;
    out.push(("root", r.map(|v| pick(&v["data"]["folders"]))));
    let r = graphql(&root, &node_query("Folder", "60", &format!("id {chain}"))).await;
    out.push((
        "node",
        r.map(|v| (0..depth).fold(v["data"]["node"].clone(), |l, _| l["parent"].clone())),
    ));
    let r = graphql(
        &relay,
        &format!("{{ foldersPage(first: 100) {{ edges {{ node {{ id {chain} }} }} }} }}"),
    )
    .await;
    out.push(("relay", r.map(|v| pick(&Value::Array(relay_nodes(&v, "foldersPage"))))));
    Some(out)
}

/// **The invariant, selecting `id`.** Ten and forty-four levels up, through every entry, the
/// ancestor is `{"id": …}` and nothing stored reaches the response: no `margin`, no
/// `cost_price`, no `audit`.
#[tokio::test]
async fn no_stored_document_leaves_unprojected_at_any_depth() {
    for depth in DEPTHS {
        let Some(entries) = every_entry(depth, "id").await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        for (entry, result) in entries {
            let ancestor = result.unwrap_or_else(|e| panic!("{entry} at {depth}: {e:?}"));
            assert_eq!(ancestor, serde_json::json!({"id": 60 - depth}), "{entry} at {depth}");
        }
    }
}

/// **The invariant, Mask.** `margin` selected ten and forty-four levels up is `null`.
#[tokio::test]
async fn a_masked_field_is_null_at_any_depth() {
    for depth in DEPTHS {
        let Some(entries) = every_entry(depth, "id margin").await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        for (entry, result) in entries {
            let ancestor = result.unwrap_or_else(|e| panic!("{entry} at {depth}: {e:?}"));
            assert_eq!(
                ancestor,
                serde_json::json!({"id": 60 - depth, "margin": null}),
                "{entry} at {depth}"
            );
        }
    }
}

/// **The invariant, Reject.** `cost_price` selected ten and forty-four levels up refuses.
#[tokio::test]
async fn a_rejected_field_refuses_at_any_depth() {
    for depth in DEPTHS {
        let Some(entries) = every_entry(depth, "id cost_price").await else {
            eprintln!("skipping: DATABASE_URL not set");
            return;
        };
        for (entry, result) in entries {
            assert!(
                matches!(result, Err(FraiseQLError::Authorization { .. })),
                "{entry} at {depth}: {result:?}"
            );
        }
    }
}

/// With no `max_query_depth` declared, the default bounds the selection: nine ancestors
/// (depth 11, the default) are served, ten are refused by the depth gate.
#[tokio::test]
async fn an_undeclared_depth_is_bounded_by_the_default() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let within = format!("{{ folders {{ id {} }} }}", ancestors(9, "id"));
    let out = graphql(&executor, &within).await.unwrap();
    assert_eq!(ancestor_of(&out["data"]["folders"], 60, 9), serde_json::json!({"id": 51}));

    let past = format!("{{ folders {{ id {} }} }}", ancestors(10, "id"));
    let err = graphql(&executor, &past).await.expect_err("depth 12 past the default 11");
    assert!(err.to_string().contains("(max: 11)"), "{err}");
}

/// Control: a row-gated level is composed at any depth — six levels up from folder 8 is
/// mallory's folder 2, and it is `null`.
#[tokio::test]
async fn control_an_owner_policy_reaches_an_ancestor_six_deep() {
    let executor = rig_or_skip!("v_user_fk", Policy::Owner);
    let out = graphql(&executor, &format!("{{ folders {{ id {} }} }}", ancestors(6, "id")))
        .await
        .unwrap();
    assert!(ancestor_of_8(&out["data"]["folders"], 5).is_object(), "{out}");
    assert!(ancestor_of_8(&out["data"]["folders"], 6).is_null(), "{out}");
}

/// A REST leaf `?select=parent` expands `Folder` as a whole object, which reaches itself:
/// past the expansion's depth it **refuses** — it neither serves the stored ancestor nor
/// silently stops.
#[tokio::test]
async fn a_rest_leaf_object_deeper_than_its_expansion_is_refused() {
    let executor = rig_or_skip!("v_user_fk", Policy::None);
    let cleared = SecurityContext {
        roles: vec!["costing".to_string()],
        ..alice()
    };
    let query_match = rest_match(&executor, "folders", &["id", "parent"]);
    let result = executor.execute_query_direct(&query_match, None, Some(&cleared), None).await;
    assert!(matches!(result, Err(FraiseQLError::Validation { .. })), "{result:?}");

    // A to-one that does not reach itself expands whole, as before.
    let query_match = rest_match(&executor, "members", &["id", "team"]);
    let out = executor
        .execute_query_direct(&query_match, None, Some(&cleared), None)
        .await
        .unwrap();
    assert_eq!(out["data"]["members"][0]["team"]["name"], "red", "{out}");
}

// ---------------------------------------------------------------------------
// (m) A mutation's result selection
// ---------------------------------------------------------------------------
//
// A mutation's payload was projected by `project_entity` and put to the #423 field
// authorizer, and nothing else: neither the root level nor any nested one met
// `requires_scope`, the nested type's RLS, its read's role, or the #422 authorizer. Each
// reproduction below is the read-path test of the same gate, asked of `touchUser` /
// `touchOrder`; the read-path test is its control. The payload is now classified through the
// read path's classifier before the write (`runners/mutation/payload_gates`).

/// `schema` with the two writes, over `v_user_fk`.
fn mutation_schema() -> CompiledSchema {
    let mut schema = schema("v_user_fk");
    for (name, return_type, table) in [
        ("touchUser", "User", "user"),
        ("touchOrder", "Order", "order"),
    ] {
        let mut mutation = MutationDefinition::new(name, return_type);
        mutation.sql_source = Some(format!("{SCHEMA}.fn_touch_{table}"));
        mutation.operation = MutationOperation::Update {
            table: format!("tb_{table}"),
        };
        mutation.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
        schema.mutations.push(mutation);
    }
    schema.build_indexes();
    schema
}

/// The orders `touchUser` served.
fn touched_orders(response: &Value) -> &Vec<Value> {
    response["data"]["touchUser"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

fn ids(orders: &[Value]) -> Vec<i64> {
    let mut ids: Vec<i64> = orders.iter().map(|o| o["id"].as_i64().unwrap()).collect();
    ids.sort_unstable();
    ids
}

const TOUCH_USER_ORDERS: &str = "mutation { touchUser(id: 1) { id orders { id } } }";

/// Control: with no gate in play, a mutation serves the orders its row embeds — all three.
/// Without it a reproduction below could pass on a rig that serves no order at all.
#[tokio::test]
async fn control_m_a_mutation_serves_its_nested_orders() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let out = graphql(&executor, TOUCH_USER_ORDERS).await.unwrap();
    assert_eq!(ids(touched_orders(&out)), [10, 11, 12], "{out}");
}

/// Control: a principal holding `read:margin` reads it through a mutation, at the root and
/// nested. A fix that refused every scoped field on a write would fail here.
#[tokio::test]
async fn control_m_a_mutation_serves_margin_to_a_principal_holding_its_scope() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let analyst = SecurityContext {
        roles: vec!["analyst".to_string()],
        ..alice()
    };
    let root = executor
        .execute_with_security("mutation { touchOrder(id: 10) { id margin } }", None, &analyst)
        .await
        .unwrap();
    assert_eq!(root["data"]["touchOrder"]["margin"], 7, "{root}");
    let nested = executor
        .execute_with_security(
            "mutation { touchUser(id: 1) { id orders { id margin } } }",
            None,
            &analyst,
        )
        .await
        .unwrap();
    let margins: Vec<&Value> = touched_orders(&nested).iter().map(|o| &o["margin"]).collect();
    assert_eq!(margins, [7, 8, 9], "{nested}");
}

/// **Reproduction (m), root Mask.** Control: `control_a_root_margin_is_masked`.
#[tokio::test]
async fn a_mutation_root_margin_is_masked() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let out = graphql(&executor, "mutation { touchOrder(id: 10) { id margin } }")
        .await
        .unwrap();
    assert!(
        out["data"]["touchOrder"]["margin"].is_null(),
        "Order.margin served in full through touchOrder: {out}"
    );
}

/// **Reproduction (m), root Reject.** Control: `control_a_root_cost_price_is_refused`.
#[tokio::test]
async fn a_mutation_root_cost_price_is_refused() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let result = graphql(&executor, "mutation { touchOrder(id: 10) { id cost_price } }").await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through touchOrder: {result:?}"
    );
}

/// **Reproduction (m), nested Mask.** Control: `a_nested_margin_is_masked`.
#[tokio::test]
async fn a_mutation_nested_margin_is_masked() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let out = graphql(&executor, "mutation { touchUser(id: 1) { id orders { id margin } } }")
        .await
        .unwrap();
    assert!(
        touched_orders(&out).iter().all(|o| o["margin"].is_null()),
        "Order.margin served in full through touchUser {{ orders }}: {out}"
    );
}

/// **Reproduction (m), nested Reject.** Control: `a_nested_cost_price_is_refused`.
#[tokio::test]
async fn a_mutation_nested_cost_price_is_refused() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::None);
    let result =
        graphql(&executor, "mutation { touchUser(id: 1) { id orders { id cost_price } } }").await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through touchUser {{ orders }}: {result:?}"
    );
}

/// **Reproduction (m), owner RLS.** Control: `owner_policy_scopes_nested_orders`.
#[tokio::test]
async fn a_mutation_nested_orders_follow_the_owner_policy() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::Owner);
    let out = graphql(&executor, TOUCH_USER_ORDERS).await.unwrap();
    assert_eq!(
        ids(touched_orders(&out)),
        [10, 12],
        "mallory's order 11 served to alice through touchUser {{ orders }}: {out}"
    );
}

/// **Reproduction (m), tenant RLS.** Control:
/// `tenant_policy_scopes_nested_orders_over_a_foreign_key_view`.
#[tokio::test]
async fn a_mutation_nested_orders_follow_the_tenant_policy() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::Tenant);
    let out = graphql(&executor, TOUCH_USER_ORDERS).await.unwrap();
    assert_eq!(
        ids(touched_orders(&out)),
        [10, 11],
        "tenant B's order 12 served to tenant A through touchUser {{ orders }}: {out}"
    );
}

/// `mutation_schema`, with `Order`'s read gated by a type-level role alice does not hold.
fn clerk_only_orders() -> CompiledSchema {
    let mut schema = mutation_schema();
    schema.types.iter_mut().find(|t| t.name == "Order").unwrap().requires_role =
        Some("clerk".to_string());
    schema.build_indexes();
    schema
}

/// Control: the read path refuses `users { orders }` when `Order`'s read requires a role
/// alice does not hold, and a mutation that does not select `orders` is served.
#[tokio::test]
async fn control_m_a_role_gated_nested_type_refuses_the_read() {
    let executor = rig_or_skip!(over clerk_only_orders(), Policy::None);
    let read = graphql(&executor, "{ users { id orders { id } } }").await;
    assert!(matches!(read, Err(FraiseQLError::Authorization { .. })), "{read:?}");
    let out = graphql(&executor, "mutation { touchUser(id: 1) { id } }").await.unwrap();
    assert_eq!(out["data"]["touchUser"]["id"], 1, "{out}");
}

/// **Reproduction (m), role.** Control: `control_m_a_role_gated_nested_type_refuses_the_read`.
#[tokio::test]
async fn a_mutation_nested_level_follows_its_types_role() {
    let executor = rig_or_skip!(over clerk_only_orders(), Policy::None);
    let result = graphql(&executor, TOUCH_USER_ORDERS).await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order read through touchUser {{ orders }} without its role: {result:?}"
    );
}

/// An authorizer that denies every read of `Order`, and admits everything else.
struct DenyOrderReads;

impl Authorizer for DenyOrderReads {
    fn authorize(&self, req: &AuthzRequest<'_>) -> Result<AuthzDecision> {
        Ok(if req.target_type == Some("Order") && req.nesting.is_some() {
            AuthzDecision::Deny {
                reason: "no orders".to_string(),
            }
        } else {
            AuthzDecision::Allow
        })
    }
}

async fn deny_order_reads_rig() -> Option<Executor> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;
    let schema = mutation_schema();
    let config = policy_config(&schema, Policy::None).with_authorizer(Arc::new(DenyOrderReads));
    Some(Executor::with_config(schema, adapter, config))
}

/// Control: the #422 authorizer denies the read path's nested `Order` level, and admits a
/// mutation that does not select it.
#[tokio::test]
async fn control_m_the_authorizer_denies_a_nested_order_read() {
    let Some(executor) = deny_order_reads_rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let read = graphql(&executor, "{ users { id orders { id } } }").await;
    assert!(matches!(read, Err(FraiseQLError::Authorization { .. })), "{read:?}");
    let out = graphql(&executor, "mutation { touchUser(id: 1) { id } }").await.unwrap();
    assert_eq!(out["data"]["touchUser"]["id"], 1, "{out}");
}

/// **Reproduction (m), #422 per level.** Control:
/// `control_m_the_authorizer_denies_a_nested_order_read`.
#[tokio::test]
async fn a_mutation_nested_level_is_put_to_the_authorizer() {
    let Some(executor) = deny_order_reads_rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let result = graphql(&executor, TOUCH_USER_ORDERS).await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "the authorizer was not asked of touchUser {{ orders }}: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (c) A cascade mutation's payload
// ---------------------------------------------------------------------------
//
// A `cascade = true` mutation answers `{ entity, cascade { updated { entity } } }`, and each
// `entity` there went through the same projector and the same #423 pass as the plain
// payload above, and nothing else. Each is now served as the plain payload is.

/// `mutation_schema` with `touchUserCascade`: the `touchUser` write as a cascade mutation.
/// Its payload's `entity` is the `User`, and `cascade.updated` reports the user and each of
/// its orders.
fn cascade_schema() -> CompiledSchema {
    let mut schema = mutation_schema();
    for t in &mut schema.types {
        if t.name == "User" || t.name == "Order" {
            t.implements = vec!["CascadeNode".to_string()];
        }
    }
    let mut mutation = MutationDefinition::new("touchUserCascade", "TouchUserCascadePayload");
    mutation.sql_source = Some(format!("{SCHEMA}.fn_cascade_user"));
    mutation.operation = MutationOperation::Update {
        table: "tb_user".to_string(),
    };
    mutation.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
    mutation.cascade = true;
    schema.mutations.push(mutation);
    schema.build_indexes();
    schema
}

/// The orders a cascade payload's `entity` served.
fn cascade_entity_orders(response: &Value) -> &Vec<Value> {
    response["data"]["touchUserCascade"]["entity"]["orders"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
}

/// Control: with no gate in play, a cascade serves its entity's orders, all three, and all
/// four updated entities.
#[tokio::test]
async fn control_c_a_cascade_serves_its_entity_and_its_updated_entities() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { entity { id orders { id } } cascade { updated { \
         id entity { ... on User { id } ... on Order { id } } } } } }",
    )
    .await
    .unwrap();
    assert_eq!(ids(cascade_entity_orders(&out)), [10, 11, 12], "{out}");
    let updated = out["data"]["touchUserCascade"]["cascade"]["updated"].as_array().unwrap();
    let updated_ids: Vec<i64> =
        updated.iter().map(|u| u["entity"]["id"].as_i64().unwrap()).collect();
    assert_eq!(updated_ids, [1, 10, 11, 12], "{out}");
}

/// **Reproduction (c), nested Mask.** Control: `a_nested_margin_is_masked`.
#[tokio::test]
async fn a_cascade_entity_nested_margin_is_masked() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { entity { id orders { id margin } } } }",
    )
    .await
    .unwrap();
    assert!(
        cascade_entity_orders(&out).iter().all(|o| o["margin"].is_null()),
        "Order.margin served in full through a cascade payload's entity: {out}"
    );
}

/// **Reproduction (c), nested Reject.** Control: `a_nested_cost_price_is_refused`.
#[tokio::test]
async fn a_cascade_entity_nested_cost_price_is_refused() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    let result = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { entity { id orders { id cost_price } } } }",
    )
    .await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through a cascade payload's entity: {result:?}"
    );
}

/// **Reproduction (c), owner RLS.** Control: `owner_policy_scopes_nested_orders`.
#[tokio::test]
async fn a_cascade_entity_nested_orders_follow_the_owner_policy() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::Owner);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { entity { id orders { id } } } }",
    )
    .await
    .unwrap();
    assert_eq!(
        ids(cascade_entity_orders(&out)),
        [10, 12],
        "mallory's order 11 served to alice through a cascade payload's entity: {out}"
    );
}

/// The entities a cascade's `updated` served, in the order the write reported them: the
/// user, then orders 10, 11 and 12.
fn updated_entities(response: &Value) -> Vec<&Value> {
    response["data"]["touchUserCascade"]["cascade"]["updated"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|u| &u["entity"])
        .collect()
}

/// **Reproduction (c), updated Mask.** Control: `control_a_root_margin_is_masked`.
#[tokio::test]
async fn an_updated_orders_margin_is_masked() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { cascade { updated { entity { ... on Order { id \
         margin } } } } } }",
    )
    .await
    .unwrap();
    let orders = &updated_entities(&out)[1..];
    assert!(
        orders.iter().all(|o| o["id"].is_i64() && o["margin"].is_null()),
        "Order.margin served in full through cascade.updated: {out}"
    );
}

/// **Reproduction (c), updated Reject.** Control: `control_a_root_cost_price_is_refused`.
#[tokio::test]
async fn an_updated_orders_cost_price_is_refused() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    let result = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { cascade { updated { entity { ... on Order { id \
         cost_price } } } } } }",
    )
    .await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "Order.cost_price served through cascade.updated: {result:?}"
    );
}

/// **Reproduction (c), updated nested owner RLS.** Control:
/// `owner_policy_scopes_nested_orders`.
#[tokio::test]
async fn an_updated_users_orders_follow_the_owner_policy() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::Owner);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { cascade { updated { entity { ... on User { id \
         orders { id } } } } } } }",
    )
    .await
    .unwrap();
    let user = updated_entities(&out)[0];
    assert_eq!(
        ids(user["orders"].as_array().unwrap_or_else(|| panic!("{out}"))),
        [10, 12],
        "mallory's order 11 served to alice under cascade.updated's User: {out}"
    );
}

// ---------------------------------------------------------------------------
// (e) A mutation's error payload
// ---------------------------------------------------------------------------
//
// An error outcome is projected from the function's `error_detail` as its declared error
// type, through the same projector and #423 pass — and, now, the same read gates.

/// `mutation_schema` with `failOrder`, which returns `OrderConflict`: an error type whose
/// detail carries the order's `margin` and `cost_price`, scoped as `Order` scopes them.
fn error_schema() -> CompiledSchema {
    let mut schema = mutation_schema();
    let mut conflict = TypeDefinition::new("OrderConflict", "");
    conflict.is_error = true;
    conflict.fields = vec![
        FieldDefinition::new("order_id", FieldType::Int),
        FieldDefinition::new("message", FieldType::String),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
    ];
    schema.types.push(conflict);
    schema.unions.push(
        UnionDefinition::new("FailOrderResult")
            .with_members(vec!["Order".to_string(), "OrderConflict".to_string()]),
    );
    let mut mutation = MutationDefinition::new("failOrder", "FailOrderResult");
    mutation.sql_source = Some(format!("{SCHEMA}.fn_fail_order"));
    mutation.operation = MutationOperation::Update {
        table: "tb_order".to_string(),
    };
    mutation.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
    schema.mutations.push(mutation);
    schema.build_indexes();
    schema
}

/// Control: the error payload serves its unscoped detail, and `margin` to a principal that
/// holds `read:margin`.
#[tokio::test]
async fn control_e_an_error_payload_serves_its_detail() {
    let executor = rig_or_skip!(over error_schema(), Policy::None);
    let out = graphql(
        &executor,
        "mutation { failOrder(id: 10) { ... on OrderConflict { order_id message } } }",
    )
    .await
    .unwrap();
    assert_eq!(out["data"]["failOrder"]["order_id"], 10, "{out}");
    let analyst = SecurityContext {
        roles: vec!["analyst".to_string()],
        ..alice()
    };
    let out = executor
        .execute_with_security(
            "mutation { failOrder(id: 10) { ... on OrderConflict { order_id margin } } }",
            None,
            &analyst,
        )
        .await
        .unwrap();
    assert_eq!(out["data"]["failOrder"]["margin"], 7, "{out}");
}

/// **Reproduction (e), Mask.** Control: `control_a_root_margin_is_masked`.
#[tokio::test]
async fn an_error_payloads_margin_is_masked() {
    let executor = rig_or_skip!(over error_schema(), Policy::None);
    let out = graphql(
        &executor,
        "mutation { failOrder(id: 10) { ... on OrderConflict { order_id margin } } }",
    )
    .await
    .unwrap();
    assert_eq!(out["data"]["failOrder"]["order_id"], 10, "{out}");
    assert!(
        out["data"]["failOrder"]["margin"].is_null(),
        "OrderConflict.margin served in full through failOrder: {out}"
    );
}

/// **Reproduction (e), Reject.** Control: `control_a_root_cost_price_is_refused`.
#[tokio::test]
async fn an_error_payloads_cost_price_is_refused() {
    let executor = rig_or_skip!(over error_schema(), Policy::None);
    let result = graphql(
        &executor,
        "mutation { failOrder(id: 10) { ... on OrderConflict { order_id cost_price } } }",
    )
    .await;
    assert!(
        matches!(result, Err(FraiseQLError::Authorization { .. })),
        "OrderConflict.cost_price served through failOrder: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// (w) When a payload refusal is decided
// ---------------------------------------------------------------------------

/// The writes `fn_touch_*` / `fn_cascade_user` ran, as `(table, id)`.
async fn writes() -> Vec<(String, i64)> {
    let url = try_database_url().unwrap();
    let adapter = PostgresAdapter::new(&url).await.expect("connect");
    let rows: Vec<HashMap<String, Value>> = adapter
        .execute_raw_query(&format!("SELECT tbl, id FROM {SCHEMA}.tb_write ORDER BY tbl, id"))
        .await
        .unwrap();
    rows.iter()
        .map(|r| (r["tbl"].as_str().unwrap().to_string(), r["id"].as_i64().unwrap()))
        .collect()
}

/// Control: a write the payload gates admit runs, and is logged — a masked field and a
/// row-filtered nested level do not refuse it.
#[tokio::test]
async fn control_w_an_admitted_payload_runs_the_write() {
    let executor = rig_or_skip!(over mutation_schema(), Policy::Owner);
    graphql(&executor, "mutation { touchUser(id: 1) { id orders { id margin } } }")
        .await
        .unwrap();
    assert_eq!(writes().await, [("user".to_string(), 1)]);
}

/// A `Reject` at any level refuses before the write: the function never runs.
#[tokio::test]
async fn a_rejected_payload_field_never_runs_the_write() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::None);
    for mutation in [
        "mutation { touchOrder(id: 10) { id cost_price } }",
        "mutation { touchUser(id: 1) { id orders { id cost_price } } }",
        "mutation { touchUserCascade(id: 1) { cascade { updated { entity { ... on Order { \
         cost_price } } } } } }",
    ] {
        let result = graphql(&executor, mutation).await;
        assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
    }
    assert_eq!(writes().await, [], "a refused selection ran its write");
}

/// A nested level whose type's policy does not declare its keys cannot be evaluated over the
/// returned document: refused, before the write — with no relationship to read it through,
/// and with one (joining through it would be a read after the write, which the payload
/// does not make).
#[tokio::test]
async fn an_opaque_policy_over_a_payloads_nested_level_refuses_before_the_write() {
    for schema in [mutation_schema(), joinable(mutation_schema())] {
        let executor = rig_or_skip!(over schema, Policy::OpaqueOwner);
        let result = graphql(&executor, TOUCH_USER_ORDERS).await;
        assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
        assert_eq!(writes().await, [], "a refused selection ran its write");
        let out = graphql(&executor, "mutation { touchUser(id: 1) { id } }").await.unwrap();
        assert_eq!(out["data"]["touchUser"]["id"], 1, "{out}");
    }
}

/// The entities a write reports are the write's to report: like the payload's own entity,
/// an updated entity is not row-filtered at its root — mallory's order 11 was updated, and
/// is reported — while the levels nested in it are (`an_updated_users_orders_…`).
#[tokio::test]
async fn an_updated_entity_is_the_writes_to_report() {
    let executor = rig_or_skip!(over cascade_schema(), Policy::Owner);
    let out = graphql(
        &executor,
        "mutation { touchUserCascade(id: 1) { cascade { updated { entity { ... on Order { id \
         } } } } } }",
    )
    .await
    .unwrap();
    let orders: Vec<i64> =
        updated_entities(&out)[1..].iter().map(|o| o["id"].as_i64().unwrap()).collect();
    assert_eq!(orders, [10, 11, 12], "{out}");
}

/// `cascade_schema` with `touchOrderLoose`: `fn_touch_order`, declared as returning
/// `OrderRecord`, a type the schema does not know — so the `Order` the write stamps is a type
/// its payload cannot hold.
fn loose_schema() -> CompiledSchema {
    let mut schema = cascade_schema();
    let mut mutation = MutationDefinition::new("touchOrderLoose", "OrderRecord");
    mutation.sql_source = Some(format!("{SCHEMA}.fn_touch_order"));
    mutation.operation = MutationOperation::Update {
        table: "tb_order".to_string(),
    };
    mutation.arguments = vec![ArgumentDefinition::new("id", FieldType::Int)];
    schema.mutations.push(mutation);
    schema.build_indexes();
    schema
}

/// An entity stamped with a type its mutation cannot hold broke the function's contract
/// (ruling AA 1): `touchOrderLoose` returns `OrderRecord`, and its function stamps `Order`.
/// It is refused with a contract error naming the stamp — whatever the selection, gated or
/// not — rather than classified on arrival and served.
#[tokio::test]
async fn an_entity_stamped_with_a_type_its_mutation_cannot_hold_is_a_contract_error() {
    let executor = rig_or_skip!(over loose_schema(), Policy::None);
    for query in [
        "mutation { touchOrderLoose(id: 10) { id margin } }",
        "mutation { touchOrderLoose(id: 11) { id cost_price } }",
    ] {
        let result = graphql(&executor, query).await;
        let Err(FraiseQLError::Validation { message, .. }) = &result else {
            panic!("a contract error, not {result:?}");
        };
        assert!(message.contains("'Order'"), "names the stamp: {message}");
    }
}

/// The contract error takes the write with it: every write is adjudicated inside its
/// transaction (ruling Z 1), so neither attempt above leaves a row behind.
#[tokio::test]
async fn a_contract_error_takes_the_write_with_it() {
    let executor = rig_or_skip!(over loose_schema(), Policy::None);
    let result = graphql(&executor, "mutation { touchOrderLoose(id: 10) { id margin } }").await;
    assert!(matches!(result, Err(FraiseQLError::Validation { .. })), "{result:?}");
    assert_eq!(writes().await, [], "a write that broke its contract stood");
}

// ---------------------------------------------------------------------------
// Relation filters across a row-secured target type
// ---------------------------------------------------------------------------
//
// A `where` through a to-one relation compiles into the root's SQL, as a path into the
// document the root's view embedded (`data->'team'->>'name'`) — and the nested level's row
// policy removes a related row the caller may not read only afterwards, in memory, on the
// way out. So a filter can ask about a row the response would never show: whether a member
// sits in another tenant's team named `blue`, whether a folder's parent is mallory's. Each
// reproduction accepts either answer that leaks nothing — a refusal, or the answer the
// caller's own rows give — and each has a control: over a readable related row, and with no
// policy at all, the same filters match.

/// `schema(view)` with `where` on every root query.
fn filterable(view: &str) -> CompiledSchema {
    let mut schema = schema(view);
    for query in &mut schema.queries {
        query.auto_params.has_where = true;
    }
    schema
}

/// The ids of the rows served at the root under `key`.
fn root_ids(response: &Value, key: &str) -> Vec<i64> {
    let rows = response["data"][key].as_array().unwrap_or_else(|| panic!("{response}"));
    rows.iter().map(|r| r["id"].as_i64().unwrap()).collect()
}

/// A filter's answer leaks nothing when it is refused or serves no row.
fn assert_learns_nothing(result: &Result<Value>, key: &str, what: &str) {
    if let Ok(out) = result {
        assert!(root_ids(out, key).is_empty(), "{what}: {out}");
    }
}

/// **Reproduction.** Tenant policy, a to-one: member 2 is tenant A's, its team `blue`
/// tenant B's. Filtering members by team name asks about tenant B's team.
#[tokio::test]
async fn a_to_one_relation_filter_cannot_match_a_team_the_tenant_policy_hides() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::Tenant);
    let result =
        graphql(&executor, r#"{ members(where: {team: {name: {eq: "blue"}}}) { id } }"#).await;
    assert_learns_nothing(&result, "members", "matched tenant B's team by its name");
}

/// **Reproduction.** Owner policy, a to-one at depth one and two: folder 2 is mallory's,
/// the parent of alice's folder 3 and the grandparent of her folder 4.
#[tokio::test]
async fn a_to_one_relation_filter_cannot_match_a_folder_the_owner_policy_hides() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::Owner);
    for query in [
        r#"{ folders(where: {parent: {owner: {eq: "u-mallory"}}}) { id } }"#,
        r#"{ folders(where: {parent: {parent: {owner: {eq: "u-mallory"}}}}) { id } }"#,
    ] {
        let result = graphql(&executor, query).await;
        assert_learns_nothing(&result, "folders", &format!("{query} matched mallory's folder 2"));
    }
}

/// Not a reproduction: a list relation cannot be filtered through at all — `where` treats
/// `User.orders` as a scalar and refuses the nested filter — so no filter can probe the
/// orders a policy hides. Pinned because the fix relies on it: were list relations made
/// filterable, the filter would have to carry the target's row predicate per element.
#[tokio::test]
async fn a_filter_through_a_list_relation_is_refused() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::None);
    let result = graphql(&executor, "{ users(where: {orders: {id: {eq: 11}}}) { id } }").await;
    assert!(matches!(result, Err(FraiseQLError::Validation { .. })), "{result:?}");
}

/// Control: a to-one relation filter over a related row the caller may read still matches
/// — tenant A's team `red`, alice's folder 3.
#[tokio::test]
async fn control_a_relation_filter_over_a_readable_row_matches() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::Tenant);
    let out = graphql(&executor, r#"{ members(where: {team: {name: {eq: "red"}}}) { id } }"#)
        .await
        .unwrap();
    assert_eq!(root_ids(&out, "members"), [1], "{out}");

    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::Owner);
    let out = graphql(&executor, "{ folders(where: {parent: {id: {eq: 3}}}) { id } }")
        .await
        .unwrap();
    assert_eq!(root_ids(&out, "folders"), [4], "{out}");
}

/// Control: with no row policy, the reproductions' filters match — the rig can see the rows.
#[tokio::test]
async fn control_without_a_policy_the_relation_filters_match() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::None);
    for (query, key, expected) in [
        (r#"{ members(where: {team: {name: {eq: "blue"}}}) { id } }"#, "members", vec![2]),
        (
            r#"{ folders(where: {parent: {owner: {eq: "u-mallory"}}}) { id } }"#,
            "folders",
            vec![3],
        ),
        (
            r#"{ folders(where: {parent: {parent: {owner: {eq: "u-mallory"}}}}) { id } }"#,
            "folders",
            vec![4],
        ),
    ] {
        let out = graphql(&executor, query).await.unwrap();
        assert_eq!(root_ids(&out, key), expected, "{query}: {out}");
    }
}

/// A hidden related row reads `NULL` to every operator, as an absent one does — not `false`:
/// under `NOT`, member 2's hidden team is not "a team not named `blue`" (which would tell
/// alice that member 2 has a team), and `isnull` sees the `null` the response serves.
#[tokio::test]
async fn a_hidden_related_row_reads_null_to_every_operator() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::Tenant);
    let out =
        graphql(&executor, r#"{ members(where: {_not: {team: {name: {eq: "blue"}}}}) { id } }"#)
            .await
            .unwrap();
    assert_eq!(root_ids(&out, "members"), [1], "{out}");
    let out = graphql(&executor, "{ members(where: {team: {name: {isnull: true}}}) { id } }")
        .await
        .unwrap();
    assert_eq!(root_ids(&out, "members"), [2], "as the response serves `team: null`: {out}");
}

/// A policy that does not declare its keys cannot be read off the embedded document, so a
/// filter through the relation is refused rather than evaluated unguarded (ruling AH 4).
#[tokio::test]
async fn a_relation_filter_whose_policy_cannot_be_read_off_the_document_is_refused() {
    let executor = rig_or_skip!(over filterable("v_user_fk"), Policy::OpaqueTenant);
    let result =
        graphql(&executor, r#"{ members(where: {team: {name: {eq: "red"}}}) { id } }"#).await;
    assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{result:?}");
}

/// **Reproduction (ruling AL).** The same opaque tenant policy, but `Member.team` is a
/// declared relationship: path (a) reads the team level from `v_team` through it, under the
/// policy — so a filter can decide visibility the same way, and should, rather than refuse.
/// Tenant A sees member 1's team `red`; member 2's team is tenant B's and reads as absent to
/// every operator, exactly as `a_hidden_related_row_reads_null_to_every_operator` over a
/// policy that declares its keys.
#[tokio::test]
async fn a_relation_filter_under_an_opaque_policy_reads_through_the_declared_relationship() {
    let executor = rig_or_skip!(over joinable(filterable("v_user_fk")), Policy::OpaqueTenant);
    for (query, expected) in [
        (r#"{ members(where: {team: {name: {eq: "red"}}}) { id } }"#, vec![1]),
        (r#"{ members(where: {team: {name: {eq: "blue"}}}) { id } }"#, vec![]),
        (r#"{ members(where: {_not: {team: {name: {eq: "blue"}}}}) { id } }"#, vec![1]),
        ("{ members(where: {team: {name: {isnull: true}}}) { id } }", vec![2]),
    ] {
        let out = graphql(&executor, query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(root_ids(&out, "members"), expected, "{query}: {out}");
    }
}

// Join guards along a path. Where a type's policy cannot be read off the embedded document,
// a filter through a declared relationship decides the related row's visibility over the
// type's own view (ruling AL); guards stack along the path, a document guard and a join
// guard mixing. `a_relation_filter_under_an_opaque_policy_reads_through_the_declared_relationship`
// covers one level; these cover a chain of them — two types (`Badge.holder.team`) and one
// type through itself (`Folder.parent.parent`) — and the two kinds of guard stacked either
// way round. Each policy shape must give the answers the policy that declares its keys gives.

/// `joinable(filterable(..))`, with `Badge.holder`, `Holder.team` and `Folder.parent`
/// declared too.
fn chained() -> CompiledSchema {
    let mut schema = joinable(filterable("v_user_fk"));
    for (on, name, target, foreign_key) in [
        ("Badge", "holder", "Holder", "fk_holder"),
        ("Holder", "team", "Team", "fk_team"),
        ("Folder", "parent", "Folder", "fk_parent"),
    ] {
        schema
            .types
            .iter_mut()
            .find(|t| t.name == on)
            .unwrap()
            .relationships
            .push(Relationship {
                name:           name.to_string(),
                target_type:    target.to_string(),
                cardinality:    Cardinality::ManyToOne,
                foreign_key:    foreign_key.to_string(),
                referenced_key: "id".to_string(),
            });
    }
    schema.build_indexes();
    schema
}

/// Tenant A: badge 1's holder and team are its own; badge 2's team is tenant B's; badge 3's
/// holder is tenant B's (its team is tenant A's `red`). A hidden level reads `NULL` to every
/// operator below it, whichever guard hides it — the outer one included.
#[tokio::test]
async fn a_relation_filter_two_levels_deep_follows_every_levels_policy_whichever_its_kind() {
    let cases = [
        (r#"{ badges(where: {holder: {team: {name: {eq: "red"}}}}) { id } }"#, vec![1]),
        (r#"{ badges(where: {holder: {team: {name: {eq: "blue"}}}}) { id } }"#, vec![]),
        (
            r#"{ badges(where: {_not: {holder: {team: {name: {eq: "blue"}}}}}) { id } }"#,
            vec![1],
        ),
        ("{ badges(where: {holder: {team: {name: {isnull: true}}}}) { id } }", vec![2, 3]),
        (r#"{ badges(where: {holder: {tenant_id: {eq: "B"}}}) { id } }"#, vec![]),
    ];
    let served = serde_json::json!([
        {"id": 1, "holder": {"id": 1, "team": {"name": "red"}}},
        {"id": 2, "holder": {"id": 2, "team": null}},
        {"id": 3, "holder": null},
    ]);
    for (shape, policy) in [
        ("keys declared", Policy::Tenant),
        ("join under join", Policy::OpaqueTenant),
        ("join under a document guard", Policy::OpaqueTenantFor(&["Team"])),
        ("a document guard under a join", Policy::OpaqueTenantFor(&["Holder"])),
    ] {
        let executor = rig_or_skip!(over chained(), policy);
        for (query, expected) in &cases {
            let out = graphql(&executor, query)
                .await
                .unwrap_or_else(|e| panic!("{shape}: {query}: {e}"));
            assert_eq!(&root_ids(&out, "badges"), expected, "{shape}: {query}: {out}");
        }
        let out = graphql(&executor, "{ badges { id holder { id team { name } } } }")
            .await
            .unwrap_or_else(|e| panic!("{shape}: {e}"));
        assert_eq!(out["data"]["badges"], served, "{shape}: {out}");
    }
}

/// Owner policy through `Folder.parent`: folder 2 is mallory's, the parent of alice's folder
/// 3 and the grandparent of folder 4. A path through it reads `NULL` beyond it, even where
/// the row past it (folder 1) is alice's.
#[tokio::test]
async fn a_relation_filter_through_a_type_itself_follows_the_policy_at_every_depth() {
    let cases = [
        ("{ folders(where: {parent: {parent: {id: {eq: 3}}}}) { id } }", vec![5]),
        ("{ folders(where: {parent: {parent: {id: {eq: 1}}}}) { id } }", vec![]),
        (
            r#"{ folders(where: {parent: {parent: {owner: {eq: "u-mallory"}}}}) { id } }"#,
            vec![],
        ),
        ("{ folders(where: {parent: {parent: {parent: {id: {eq: 1}}}}}) { id } }", vec![]),
        (
            "{ folders(where: {parent: {parent: {parent: {id: {eq: 3}}}}}) { id } }",
            vec![6],
        ),
    ];
    for (shape, policy) in [
        ("keys declared", Policy::Owner),
        ("joined", Policy::OpaqueOwner),
    ] {
        let executor = rig_or_skip!(over chained(), policy);
        for (query, expected) in &cases {
            let out = graphql(&executor, query)
                .await
                .unwrap_or_else(|e| panic!("{shape}: {query}: {e}"));
            assert_eq!(&root_ids(&out, "folders"), expected, "{shape}: {query}: {out}");
        }
    }
}

/// Control: with no policy, every filter above matches the rows the policies hide.
#[tokio::test]
async fn control_without_a_policy_the_chained_relation_filters_match() {
    let executor = rig_or_skip!(over chained(), Policy::None);
    for (query, key, expected) in [
        (
            r#"{ badges(where: {holder: {team: {name: {eq: "red"}}}}) { id } }"#,
            "badges",
            vec![1, 3],
        ),
        (
            r#"{ badges(where: {holder: {team: {name: {eq: "blue"}}}}) { id } }"#,
            "badges",
            vec![2],
        ),
        (
            "{ badges(where: {holder: {team: {name: {isnull: true}}}}) { id } }",
            "badges",
            vec![],
        ),
        (
            r#"{ badges(where: {holder: {tenant_id: {eq: "B"}}}) { id } }"#,
            "badges",
            vec![3],
        ),
        (
            "{ folders(where: {parent: {parent: {id: {eq: 1}}}}) { id } }",
            "folders",
            vec![3],
        ),
        (
            r#"{ folders(where: {parent: {parent: {owner: {eq: "u-mallory"}}}}) { id } }"#,
            "folders",
            vec![4],
        ),
        (
            "{ folders(where: {parent: {parent: {parent: {id: {eq: 1}}}}}) { id } }",
            "folders",
            vec![4],
        ),
    ] {
        let out = graphql(&executor, query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(root_ids(&out, key), expected, "{query}: {out}");
    }
}
