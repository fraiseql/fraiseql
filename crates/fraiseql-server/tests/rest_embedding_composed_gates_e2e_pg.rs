//! Each level of a composed `?select=` read is gated as a read of its **own** target.
//!
//! An embed used to be a sub-read of the target's list query, so the target's RLS
//! predicate, its field-level RBAC and its role gate all applied to it because it *was*
//! a read. Composed into the parent's statement it is not a read any more, and a gate
//! attached to a read stops seeing it unless the composition carries it there. This suite
//! serves the composed statement against PostgreSQL and asserts, in the rows that come
//! back, that it does:
//!
//! * the target's RLS predicate scopes the embedded rows — a parent the principal may read does not
//!   bring in related rows the principal may not;
//! * the target's to-one row is `null` when its policy withholds it;
//! * a `Mask`ed field of the target is `null`, and served in full to a principal holding the scope;
//!   a `Reject`ed one refuses the request;
//! * an embedded count counts only the rows the policy admits.
//!
//! The statement's *shape* — which predicate lands in which `LATERAL`, which keys a level
//! returns — is unit-tested beside the renderer and the plan (`fraiseql-db`'s
//! `composed_tests`, `fraiseql-core`'s `query_composed_tests`). What those cannot show is
//! that PostgreSQL binds each level's unqualified predicate to that level's own view,
//! which is what these rows show.
//!
//! **Why through the executor rather than the router.** RLS needs a principal, and the
//! REST router's principal comes from the authentication layer. This suite exercises the
//! engine entry the router calls — `Executor::execute_query_composed` — with a principal
//! built directly, which is the seam under test.
//!
//! Self-skips when no `DATABASE_URL` is set (no `#[ignore]`), so it is inert in the
//! database-free `test` leg and runs in the Dagger `integration: server` suite.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `p_composed_gates` schema → run
//! `--test-threads=1`.
#![cfg(feature = "rest")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::postgres::PostgresAdapter,
    error::FraiseQLError,
    prelude::{DatabaseAdapter as _, UserId},
    runtime::{CountSelection, EmbedSelection, Executor, QueryMatch, RuntimeConfig},
    schema::{
        Cardinality, CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        Relationship, RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::{DefaultRLSPolicy, SecurityContext},
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "p_composed_gates";

/// Two users, both owned by alice's principal, and three orders — one of alice's user's
/// orders owned by **mallory**. Every row carries an `owner`, and the policy admits the
/// rows whose `owner` is the principal.
///
/// | order | user | owner |
/// |---|---|---|
/// | 10 | 1 (alice) | u-alice |
/// | 11 | 1 (alice) | u-mallory |
/// | 20 | 2 (bob)   | u-alice |
///
/// Order 11 is the discriminating row: its parent is a row alice may read, and it is not.
async fn seed(adapter: &PostgresAdapter) {
    let stmts = vec![
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL, owner text \
             NOT NULL)"
        ),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL, \
             owner text NOT NULL, total bigint NOT NULL, margin bigint NOT NULL, cost_price \
             bigint NOT NULL)"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice', 'u-alice'), (2, 'bob', 'u-alice')"
        ),
        format!(
            "INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, 'u-alice', 100, 7, 90), (11, 1, \
             'u-mallory', 101, 8, 91), (20, 2, 'u-alice', 200, 9, 190)"
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

fn schema() -> CompiledSchema {
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
    }];
    schema.types.push(user);

    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        FieldDefinition::new("fk_user", FieldType::Int),
        FieldDefinition::new("total", FieldType::Int),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
    ];
    order.relationships = vec![Relationship {
        name:           "user".to_string(),
        target_type:    "User".to_string(),
        cardinality:    Cardinality::ManyToOne,
        foreign_key:    "fk_user".to_string(),
        referenced_key: "id".to_string(),
    }];
    schema.types.push(order);

    for (name, return_type, view) in [("users", "User", "v_user"), ("orders", "Order", "v_order")] {
        let mut query = QueryDefinition::new(name, return_type)
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.{view}"));
        query.auto_params.has_where = true;
        query.auto_params.has_limit = true;
        schema.queries.push(query);
    }

    // Field-level RBAC is inert without a security section. A scope is granted by a role,
    // not carried on the principal.
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
        request_id:       "req-composed-gates".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
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

async fn rig() -> Option<Rig> {
    let url = try_database_url()?;
    let adapter = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&adapter).await;

    let schema = schema();
    let policy = DefaultRLSPolicy::new()
        .with_single_tenant()
        .with_owner_field("owner".to_string());
    let config = RuntimeConfig::from_compiled_schema(&schema)
        .expect("the schema must yield a runtime config")
        .with_rls_policy(Arc::new(policy));
    let executor = Executor::with_config(schema.clone(), adapter, config);
    Some(Rig { executor, schema })
}

impl Rig {
    /// `<query>?select=<fields>,<embeds>,<counts>` as `principal`, returning the rows.
    async fn read(
        &self,
        query: &str,
        fields: &[&str],
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        principal: &SecurityContext,
    ) -> Result<Vec<Value>, FraiseQLError> {
        let definition = self.schema.queries.iter().find(|q| q.name == query).unwrap().clone();
        let return_type = definition.return_type.clone();
        let query_match = QueryMatch::from_operation(
            definition,
            fields.iter().map(ToString::to_string).collect(),
            HashMap::new(),
            self.schema.find_type(&return_type),
        )
        .unwrap();
        let body = self
            .executor
            .execute_query_composed(&query_match, embeds, counts, None, Some(principal), None)
            .await?;
        Ok(body["data"][query].as_array().cloned().unwrap_or_default())
    }
}

fn embed(relationship: &str, fields: &[&str]) -> EmbedSelection {
    EmbedSelection {
        relationship: relationship.to_string(),
        output_key: relationship.to_string(),
        fields: fields.iter().map(ToString::to_string).collect(),
        limit: Some(1000),
        ..EmbedSelection::default()
    }
}

/// The row whose `id` is `id`.
fn by_id(rows: &[Value], id: i64) -> &Value {
    rows.iter()
        .find(|r| r.get("id").and_then(Value::as_i64) == Some(id))
        .unwrap_or_else(|| panic!("no row {id} in {rows:?}"))
}

fn ids(rows: &Value) -> Vec<i64> {
    let mut ids: Vec<i64> = rows
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r.get("id").and_then(Value::as_i64))
        .collect();
    ids.sort_unstable();
    ids
}

// ---------------------------------------------------------------------------

/// **The target's policy scopes the embedded rows.** Alice may read both users; she may
/// not read order 11, although its parent is a user she may read. A lateral scoped by
/// the parent's predicate — or by none — serves it.
#[tokio::test]
async fn an_embedded_level_is_scoped_by_its_targets_policy() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let alice = principal("u-alice", &[]);

    let rows = rig
        .read("users", &["id"], &[embed("orders", &["id"])], &[], &alice)
        .await
        .unwrap();

    assert_eq!(rows.len(), 2, "both users are alice's: {rows:?}");
    assert_eq!(ids(&by_id(&rows, 1)["orders"]), [10], "not mallory's order 11: {rows:?}");
    assert_eq!(ids(&by_id(&rows, 2)["orders"]), [20], "{rows:?}");
}

/// **An embedded count counts what the policy admits**, as the rows beside it do.
#[tokio::test]
async fn an_embedded_count_counts_only_what_the_policy_admits() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let alice = principal("u-alice", &[]);
    let count = CountSelection {
        relationship: "orders".to_string(),
        output_key:   "orders_count".to_string(),
        filter:       None,
    };

    let rows = rig.read("users", &["id"], &[], &[count], &alice).await.unwrap();

    assert_eq!(by_id(&rows, 1)["orders_count"], json!(1), "order 11 is not counted: {rows:?}");
    assert_eq!(by_id(&rows, 2)["orders_count"], json!(1), "{rows:?}");
}

/// **A to-one target withheld by its policy is `null`.** Mallory may read order 11; its
/// user is alice's, and mallory may not read it.
#[tokio::test]
async fn a_to_one_target_withheld_by_its_policy_is_null() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let mallory = principal("u-mallory", &[]);

    let rows = rig
        .read("orders", &["id"], &[embed("user", &["name"])], &[], &mallory)
        .await
        .unwrap();

    assert_eq!(rows, [json!({"id": 11, "user": null})], "mallory reads order 11 and no user");
}

/// **A masked field of the target is `null`; a principal whose role grants the scope
/// reads it.**
///
/// The positive twin is the half that makes the first meaningful: without it, a level that
/// returned no `margin` at all would pass.
#[tokio::test]
async fn a_masked_field_of_the_target_is_null_unless_the_scope_is_held() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };
    let embeds = [embed("orders", &["id", "margin"])];

    let rows = rig
        .read("users", &["id"], &embeds, &[], &principal("u-alice", &[]))
        .await
        .unwrap();
    assert_eq!(
        by_id(&rows, 1)["orders"],
        json!([{"id": 10, "margin": null}]),
        "withheld without read:margin"
    );

    let rows = rig
        .read("users", &["id"], &embeds, &[], &principal("u-alice", &["margin_reader"]))
        .await
        .unwrap();
    assert_eq!(by_id(&rows, 1)["orders"], json!([{"id": 10, "margin": 7}]), "served with it");
}

/// **A rejected field of the target refuses the request** — however many rows it would
/// have touched, and before any are read.
#[tokio::test]
async fn a_rejected_field_of_the_target_refuses_the_request() {
    let Some(rig) = rig().await else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let refused = rig
        .read(
            "users",
            &["id"],
            &[embed("orders", &["id", "cost_price"])],
            &[],
            &principal("u-alice", &[]),
        )
        .await;

    match refused {
        Err(FraiseQLError::Authorization { resource, .. }) => {
            assert_eq!(resource.as_deref(), Some("Order.cost_price"));
        },
        other => panic!("expected the target's Reject: {other:?}"),
    }
}
