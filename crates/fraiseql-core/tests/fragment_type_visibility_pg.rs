//! A fragment's type condition resolves against the types the caller may see, the same as
//! a variable's type (§ 5.8.2), executed against PostgreSQL.
//!
//! `Ledger` is a Node type only an `auditor` may see. To a caller without the role, a
//! fragment on it answers exactly as a fragment on a type the schema does not define, at
//! every position a fragment is judged at: the root, below a field, through a named spread,
//! in a Relay connection's `edges { node }` and `nodes`, and under `node(id:)`. An
//! `auditor` is answered as the schema is written.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `ftv_*` fixtures → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    chrono::Utc,
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::{Executor, relay::encode_node_id},
    schema::{CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition},
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const USERS: &str = "v_ftv_user";
const LEDGERS: &str = "v_ftv_ledger";
const ALICE: &str = "aaaa0000-0000-0000-0000-000000000001";

fn schema() -> CompiledSchema {
    let user = TestTypeBuilder::new("User", USERS)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_nullable_field("team", FieldType::Object("Team".to_string()))
        .build();
    let team = TestTypeBuilder::new("Team", "")
        .with_simple_field("name", FieldType::String)
        .build();
    let ledger = TestTypeBuilder::new("Ledger", LEDGERS)
        .relay_node()
        .with_implements(&["Node"])
        .requires_role("auditor")
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("total", FieldType::Int)
        .build();
    let mut schema = TestSchemaBuilder::new()
        .with_type(user)
        .with_type(team)
        .with_type(ledger)
        .with_query(
            TestQueryBuilder::new("users", "User")
                .returns_list(true)
                .with_sql_source(USERS)
                .build(),
        )
        .with_query(
            TestQueryBuilder::new("userConnection", "User")
                .returns_list(true)
                .with_sql_source(USERS)
                .relay_cursor_column("pk_user")
                .build(),
        )
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
        format!("DROP TABLE IF EXISTS {USERS}, {LEDGERS}"),
        format!("CREATE TABLE {USERS} (pk_user bigint, data jsonb)"),
        format!(r#"INSERT INTO {USERS} VALUES (1, '{{"id": "{ALICE}", "name": "Alice"}}')"#),
        format!("CREATE TABLE {LEDGERS} (pk_ledger bigint, data jsonb)"),
    ] {
        let _: Vec<HashMap<String, Value>> = adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(Executor::new_with_relay(schema(), Arc::new(adapter)))
}

fn principal(roles: &[&str]) -> SecurityContext {
    SecurityContext {
        user_id:          "user-1".into(),
        roles:            roles.iter().map(ToString::to_string).collect(),
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        expires_at:       Utc::now() + fraiseql_core::chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// Each position a fragment is judged at, as a document with a fragment on `on`.
fn positions(on: &str) -> Vec<(&'static str, String)> {
    let node = format!(r#"node(id: "{}")"#, encode_node_id("User", ALICE));
    vec![
        ("root", format!("{{ users {{ id ... on {on} {{ total }} }} }}")),
        ("below a field", format!("{{ users {{ team {{ ... on {on} {{ total }} }} }} }}")),
        (
            "named spread",
            format!("fragment F on {on} {{ total }} query {{ users {{ ...F }} }}"),
        ),
        (
            "connection edges",
            format!("{{ userConnection {{ edges {{ node {{ ... on {on} {{ total }} }} }} }} }}"),
        ),
        (
            "connection nodes",
            format!("{{ userConnection {{ nodes {{ ... on {on} {{ total }} }} }} }}"),
        ),
        ("node", format!("{{ {node} {{ id ... on {on} {{ total }} }} }}")),
        (
            "node, named spread",
            format!("fragment F on {on} {{ total }} query {{ {node} {{ id ...F }} }}"),
        ),
    ]
}

/// `document`'s answer through each entry an anonymous caller or one without the role
/// reaches the executor by, each as its full `Debug` form: the error's kind, message and
/// path, or the response.
async fn as_outsider(executor: &Executor, document: &str) -> Vec<String> {
    let body = executor
        .execute_operation_body(document, None, None)
        .await
        .and_then(fraiseql_core::runtime::ResponseBody::into_value);
    vec![
        format!("{:?}", executor.execute(document, None).await),
        format!("{body:?}"),
        format!("{:?}", executor.execute_with_security(document, None, &principal(&[])).await),
    ]
}

/// To a caller who may not see `Ledger`, a fragment on it answers as one on a type the
/// schema does not define, at every position: kind, message and path alike.
#[tokio::test]
async fn a_fragment_on_a_type_the_caller_may_not_see_answers_as_one_on_no_type() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let mut failures = Vec::new();
    for ((position, hidden), (_, missing)) in
        positions("Ledger").into_iter().zip(positions("Nosuch"))
    {
        let expected = as_outsider(&executor, &missing).await;
        assert!(
            expected.iter().all(|answer| answer.contains("Unknown type `Nosuch`")),
            "{position}: {expected:?}"
        );
        let actual: Vec<String> = as_outsider(&executor, &hidden)
            .await
            .into_iter()
            .map(|answer| answer.replace("Ledger", "Nosuch"))
            .collect();
        if actual != expected {
            failures
                .push(format!("{position}:\n    got:      {actual:?}\n    expected: {expected:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} position(s) differ:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// An `auditor` sees `Ledger`: a fragment on it is judged as the schema is written, refused
/// where it can never apply and read under `node`, where it can.
#[tokio::test]
async fn a_caller_who_may_see_the_type_is_answered_as_the_schema_is_written() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let auditor = principal(&["auditor"]);
    for (position, document) in positions("Ledger") {
        let answer = executor.execute_with_security(&document, None, &auditor).await;
        if position.starts_with("node") {
            assert_eq!(answer.unwrap(), json!({"data": {"node": {"id": ALICE}}}), "{position}");
            continue;
        }
        let refusal = answer.unwrap_err().to_string();
        assert!(refusal.contains("can never apply here"), "{position}: {refusal}");
    }
}

/// The verdict on a document is held per view: one caller's answer is never another's,
/// whichever asks first.
#[tokio::test]
async fn each_view_is_answered_for_itself_whichever_asks_first() {
    let auditor = principal(&["auditor"]);
    let outsider = principal(&[]);
    for auditor_first in [true, false] {
        let Some(executor) = executor().await else {
            eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
            return;
        };
        for (position, document) in positions("Ledger") {
            let order: [&SecurityContext; 2] = if auditor_first {
                [&auditor, &outsider]
            } else {
                [&outsider, &auditor]
            };
            for caller in order {
                let answer = executor
                    .execute_with_security(&document, None, caller)
                    .await
                    .map_err(|e| e.to_string());
                let unknown = answer.as_ref().is_err_and(|e| e.contains("Unknown type `Ledger`"));
                assert_eq!(
                    unknown,
                    caller.roles.is_empty(),
                    "{position}, auditor first: {auditor_first}, roles {:?}: {answer:?}",
                    caller.roles
                );
            }
        }
    }
}
