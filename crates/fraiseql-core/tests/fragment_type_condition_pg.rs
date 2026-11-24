//! A fragment's type condition, executed against PostgreSQL: a fragment on a type the
//! schema does not define (§ 5.5.1.2), or on one that can never apply where it stands
//! (§ 5.5.2.3), is refused before the database is read, on `node(id:)` and on every other
//! root; a field selected through a fragment on a Node type exists on it (§ 5.3.1).
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `ftc_*` fixtures → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    error::FraiseQLError,
    runtime::{Executor, relay::encode_node_id},
    schema::{CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition},
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

const USERS: &str = "v_ftc_user";
const LEDGERS: &str = "v_ftc_ledger";
const TAGS: &str = "v_ftc_tag";
const ALICE: &str = "aaaa0000-0000-0000-0000-000000000001";

fn schema() -> CompiledSchema {
    let user = TestTypeBuilder::new("User", USERS)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .build();
    let ledger = TestTypeBuilder::new("Ledger", LEDGERS)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("total", FieldType::Int)
        .build();
    let tag = TestTypeBuilder::new("Tag", TAGS)
        .with_simple_field("label", FieldType::String)
        .build();
    let list = |name: &str, ty: &str, view: &str| {
        TestQueryBuilder::new(name, ty).returns_list(true).with_sql_source(view).build()
    };
    let mut schema = TestSchemaBuilder::new()
        .with_type(user)
        .with_type(ledger)
        .with_type(tag)
        .with_query(list("users", "User", USERS))
        .with_query(list("tags", "Tag", TAGS))
        .build();
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.build_indexes();
    schema
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {USERS}, {LEDGERS}, {TAGS}"),
        format!("CREATE TABLE {USERS} (pk_user bigint, data jsonb)"),
        format!(r#"INSERT INTO {USERS} VALUES (1, '{{"id": "{ALICE}", "name": "Alice"}}')"#),
        format!("CREATE TABLE {LEDGERS} (pk_ledger bigint, data jsonb)"),
        format!("CREATE TABLE {TAGS} (data jsonb)"),
        format!(r#"INSERT INTO {TAGS} VALUES ('{{"label": "red"}}')"#),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new_with_relay(schema(), Arc::new(adapter)))
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            vec![],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       fraiseql_time::Timestamp::now()
            + fraiseql_time::jiff::SignedDuration::from_hours(1),
        authenticated_at: fraiseql_time::Timestamp::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// `document`'s refusal through each entry a document reaches the executor by, as the
/// validation message, or what it was answered with instead.
async fn refusals(executor: &Executor, document: &str) -> Vec<Result<String, String>> {
    let refusal = |r: fraiseql_core::error::Result<Value>| match r {
        Err(FraiseQLError::Validation { message, .. }) => Ok(message),
        other => Err(format!("{other:?}")),
    };
    let body = executor
        .execute_operation_body(document, None, None)
        .await
        .and_then(fraiseql_core::runtime::ResponseBody::into_value);
    vec![
        refusal(executor.execute(document, None).await),
        refusal(body),
        refusal(executor.execute_with_security(document, None, &principal()).await),
    ]
}

fn node() -> String {
    format!(r#"node(id: "{}")"#, encode_node_id("User", ALICE))
}

#[tokio::test]
async fn a_fragment_on_an_unknown_type_or_one_that_cannot_apply_is_refused_at_every_root() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let node = node();
    // (document, what the refusal says)
    let cases = [
        (format!("{{ {node} {{ ... on Nosuch {{ nope }} }} }}"), "Unknown type `Nosuch`"),
        (format!("{{ {node} {{ ... on Tag {{ label }} }} }}"), "can never apply here"),
        (
            format!("fragment T on Tag {{ label }} query {{ {node} {{ ...T }} }}"),
            "can never apply here",
        ),
        ("{ users { id ... on Nosuch { name } } }".to_string(), "Unknown type `Nosuch`"),
        (
            "fragment F on Nosuch { name } query { users { id ...F } }".to_string(),
            "Unknown type `Nosuch`",
        ),
        ("{ users { id ... on Ledger { total } } }".to_string(), "can never apply here"),
        ("{ tags { ... on Node { id } } }".to_string(), "can never apply here"),
        (
            format!("{{ users {{ id }} {node} {{ ... on Nosuch {{ nope }} }} }}"),
            "Unknown type `Nosuch`",
        ),
    ];
    let mut failures = Vec::new();
    for (document, says) in &cases {
        for answer in refusals(&executor, document).await {
            match answer {
                Ok(message) if message.contains(says) => {},
                other => failures.push(format!("{document}: {other:?}")),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} refusal(s) missing:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// A field a fragment on `User` selects under `node` exists on `User`: refused, when it does
/// not, with the refusal a root returning `User` gives it.
#[tokio::test]
async fn a_field_a_node_fragment_selects_exists_on_its_type() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let expected = refusals(&executor, "{ users { nope } }").await;
    assert!(expected.iter().all(Result::is_ok), "{expected:?}");
    let node = node();
    for document in [
        format!("{{ {node} {{ ... on User {{ nope }} }} }}"),
        format!("fragment U on User {{ id nope }} query {{ {node} {{ ...U }} }}"),
        format!("{{ users {{ id }} {node} {{ ... on User {{ nope }} }} }}"),
    ] {
        assert_eq!(refusals(&executor, &document).await, expected, "{document}");
    }
    // On another Node type than the id names: held to that type's fields all the same.
    let document = format!("{{ {node} {{ id ... on Ledger {{ nope }} }} }}");
    for answer in refusals(&executor, &document).await {
        assert_eq!(
            answer,
            Ok("Cannot query field 'nope' on type 'Ledger'.".to_string()),
            "{document}"
        );
    }
}

/// The controls: a fragment that applies where it stands is served.
#[tokio::test]
async fn a_fragment_that_can_apply_is_served() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let node = node();
    let alice = serde_json::json!({"name": "Alice"});
    for (document, root, expected) in [
        (
            format!("{{ {node} {{ ... on User {{ name }} ... on Ledger {{ total }} }} }}"),
            "node",
            alice.clone(),
        ),
        (
            format!(
                "fragment L on Ledger {{ total }} query {{ {node} {{ ...L ... on Node {{ ... on User {{ name }} }} }} }}"
            ),
            "node",
            alice.clone(),
        ),
        (
            "{ users { ... on Node { ... on User { name } } } }".to_string(),
            "users",
            serde_json::json!([alice]),
        ),
        (
            "fragment F on User { name } query { users { ...F } }".to_string(),
            "users",
            serde_json::json!([alice]),
        ),
    ] {
        let answer = executor.execute(&document, None).await.unwrap();
        assert_eq!(answer, serde_json::json!({"data": {root: expected}}), "{document}");
    }
    // Beside another root, `node` is planned by the multi-root pipeline.
    let document = format!(
        "fragment L on Ledger {{ total }} query {{ users {{ name }} {node} {{ ...L ... on User {{ name }} }} }}"
    );
    let answer = executor.execute(&document, None).await.unwrap();
    assert_eq!(
        answer,
        serde_json::json!({"data": {"users": [alice], "node": alice}}),
        "{document}"
    );
}

/// The executor's matcher, which a transport planning against the document calls directly
/// (`@stream` over SSE), refuses the same fragments.
#[tokio::test]
async fn the_matcher_refuses_them_too() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for (document, says) in [
        ("{ users { id ... on Nosuch { name } } }", "Unknown type `Nosuch`"),
        (
            "fragment F on Nosuch { name } query { users { id ...F } }",
            "Unknown type `Nosuch`",
        ),
        ("{ users { id ... on Ledger { total } } }", "can never apply here"),
    ] {
        match executor.matcher().match_query(document, None) {
            Err(FraiseQLError::Validation { message, .. }) if message.contains(says) => {},
            other => panic!("{document}: {:?}", other.map(|m| m.selections)),
        }
    }
}
