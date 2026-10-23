//! A nested selection read through an embedded `FraiseWireAdapter`, against PostgreSQL (#1680).
//!
//! `FraiseWireAdapter` cannot apply the SQL projection hint: it reads each row's whole stored
//! document. An embedder can still serve it through `Executor::read_only`. These cases ask
//! whether what the executor returns over that adapter is the selection, and the selection
//! gated by `requires_scope`, as it is over `PostgresAdapter`, which each case also reads as
//! the control.
//!
//! `User` embeds its `Order` documents. Each stored order carries `margin` (Mask behind
//! `read:margin`) and `cost_price` (Reject behind `read:cost`); the reader holds neither scope.
//!
//! **Execution engine:** `PostgreSQL` through `fraiseql-wire` · **Infrastructure:**
//! `DATABASE_URL` · **Parallelism:** creates and drops its own `wire_nested` schema; run
//! `--test-threads=1`.
#![cfg(feature = "wire-backend")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use fraiseql_core::{
    db::{FraiseWireAdapter, postgres::PostgresAdapter},
    error::{FraiseQLError, Result},
    prelude::{DatabaseAdapter as _, UserId},
    runtime::Executor,
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, QueryDefinition,
        RoleDefinition, SecurityConfig, TypeDefinition,
    },
    security::SecurityContext,
};
use fraiseql_test_support::try_database_url;
use serde_json::{Value, json};

const SCHEMA: &str = "wire_nested";

async fn seed(adapter: &PostgresAdapter) {
    for stmt in [
        format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {SCHEMA}"),
        format!("CREATE TABLE {SCHEMA}.tb_user (id bigint PRIMARY KEY, name text NOT NULL)"),
        format!(
            "CREATE TABLE {SCHEMA}.tb_order (id bigint PRIMARY KEY, fk_user bigint NOT NULL \
             REFERENCES {SCHEMA}.tb_user(id), margin bigint NOT NULL, cost_price bigint NOT NULL)"
        ),
        format!("INSERT INTO {SCHEMA}.tb_user VALUES (1, 'alice')"),
        format!("INSERT INTO {SCHEMA}.tb_order VALUES (10, 1, 7, 90), (11, 1, 8, 91)"),
        format!(
            "CREATE VIEW {SCHEMA}.v_user AS SELECT u.id, jsonb_build_object('id', u.id, 'name', \
             u.name, 'profile', jsonb_build_object('bio', 'hi', 'salary', 5000, 'manager', \
             jsonb_build_object('name', 'bob', 'salary', 9000)), 'orders', COALESCE((SELECT jsonb_agg(jsonb_build_object('id', o.id, \
             'margin', o.margin, 'cost_price', o.cost_price) ORDER BY o.id) FROM \
             {SCHEMA}.tb_order o WHERE o.fk_user = u.id), '[]'::jsonb)) AS data FROM \
             {SCHEMA}.tb_user u"
        ),
    ] {
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
        FieldDefinition::new("orders", FieldType::list(FieldType::Object("Order".to_string()))),
        FieldDefinition::new("profile", FieldType::Object("Profile".to_string())),
    ];
    schema.types.push(user);

    let mut profile = TypeDefinition::new("Profile", format!("{SCHEMA}.v_profile"));
    profile.fields = vec![
        FieldDefinition::new("bio", FieldType::String),
        scoped("salary", "read:salary", FieldDenyPolicy::Mask),
        FieldDefinition::new("manager", FieldType::Object("Manager".to_string())),
    ];
    schema.types.push(profile);

    let mut manager = TypeDefinition::new("Manager", format!("{SCHEMA}.v_manager"));
    manager.fields = vec![
        FieldDefinition::new("name", FieldType::String),
        scoped("salary", "read:salary", FieldDenyPolicy::Mask),
    ];
    schema.types.push(manager);

    let mut order = TypeDefinition::new("Order", format!("{SCHEMA}.v_order"));
    order.fields = vec![
        FieldDefinition::new("id", FieldType::Int),
        scoped("margin", "read:margin", FieldDenyPolicy::Mask),
        scoped("cost_price", "read:cost", FieldDenyPolicy::Reject),
    ];
    schema.types.push(order);

    schema.queries.push(
        QueryDefinition::new("users", "User")
            .returning_list()
            .with_sql_source(format!("{SCHEMA}.v_user")),
    );

    let mut security = SecurityConfig::default();
    security.add_role(RoleDefinition::new("analyst", vec!["read:margin".to_string()]));
    security.add_role(RoleDefinition::new("costing", vec!["read:cost".to_string()]));
    security.add_role(RoleDefinition::new("hr", vec!["read:salary".to_string()]));
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
        request_id:       "req-wire-nested".to_string(),
        ip_address:       None,
        authenticated_at: Utc::now(),
        expires_at:       Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The control (`PostgresAdapter`) and the subject (`FraiseWireAdapter`), over one seeded
/// database.
async fn rigs() -> Option<[(&'static str, Executor); 2]> {
    let url = try_database_url()?;
    let pg = Arc::new(PostgresAdapter::new(&url).await.expect("connect"));
    seed(&pg).await;
    let wire = Arc::new(FraiseWireAdapter::new(&url));
    Some([
        ("postgres", Executor::read_only(schema(), pg)),
        ("wire", Executor::read_only(schema(), wire)),
    ])
}

macro_rules! rigs_or_skip {
    () => {
        match rigs().await {
            Some(rigs) => rigs,
            None => {
                eprintln!("skipping: DATABASE_URL not set");
                return;
            },
        }
    };
}

async fn graphql(executor: &Executor, query: &str) -> Result<Value> {
    executor.execute_with_security(query, None, &reader()).await
}

/// A nested selection that names only `id` serves only `id`: no stored key the client did
/// not select — `margin` and `cost_price` among them — leaves.
#[tokio::test]
async fn an_unselected_nested_field_is_absent() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id orders { id } } }").await.unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"id": 1, "orders": [{"id": 10}, {"id": 11}]}]),
            "{name}: {out}"
        );
    }
}

/// A root selection that omits `name` and `orders` serves neither.
#[tokio::test]
async fn an_unselected_root_field_is_absent() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id } }").await.unwrap();
        assert_eq!(out["data"]["users"], json!([{"id": 1}]), "{name}: {out}");
    }
}

/// A selected `margin`, without `read:margin`, is `null`.
#[tokio::test]
async fn a_selected_nested_masked_field_is_null() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id orders { id margin } } }").await.unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"id": 1, "orders": [{"id": 10, "margin": null}, {"id": 11, "margin": null}]}]),
            "{name}: {out}"
        );
    }
}

/// An aliased selection is served under its aliases, and nothing else.
#[tokio::test]
async fn an_aliased_nested_selection_is_served_as_named() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { uid: id os: orders { oid: id m: margin } } }")
            .await
            .unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"uid": 1, "os": [{"oid": 10, "m": null}, {"oid": 11, "m": null}]}]),
            "{name}: {out}"
        );
    }
}

/// A selected `cost_price`, without `read:cost`, refuses the query.
#[tokio::test]
async fn a_selected_nested_rejected_field_refuses() {
    for (name, executor) in rigs_or_skip!() {
        let result = graphql(&executor, "{ users { id orders { id cost_price } } }").await;
        assert!(matches!(result, Err(FraiseQLError::Authorization { .. })), "{name}: {result:?}");
    }
}

/// A to-one nested object serves only its selection: `salary` is stored, not selected.
#[tokio::test]
async fn an_unselected_field_of_a_nested_object_is_absent() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id profile { bio } } }").await.unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"id": 1, "profile": {"bio": "hi"}}]),
            "{name}: {out}"
        );
    }
}

/// Two levels down, the same.
#[tokio::test]
async fn an_unselected_field_two_levels_down_is_absent() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id profile { manager { name } } } }")
            .await
            .unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"id": 1, "profile": {"manager": {"name": "bob"}}}]),
            "{name}: {out}"
        );
    }
}

/// A selected masked field of a nested object, two levels down, is `null`.
#[tokio::test]
async fn a_selected_masked_field_two_levels_down_is_null() {
    for (name, executor) in rigs_or_skip!() {
        let out = graphql(&executor, "{ users { id profile { manager { name salary } } } }")
            .await
            .unwrap();
        assert_eq!(
            out["data"]["users"],
            json!([{"id": 1, "profile": {"manager": {"name": "bob", "salary": null}}}]),
            "{name}: {out}"
        );
    }
}
