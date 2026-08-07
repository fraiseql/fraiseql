//! Selection-set conformance against **real PostgreSQL**.
//!
//! The unit-level suite (`graphql_selection_conformance.rs`) drives a mock
//! adapter, so for `node(id:)` it can only assert the *projection SQL* the
//! runtime built — the mock ignores the hint and hands back the fixture row
//! whole. That is precisely the gap #827 lived in: the projection and the bytes
//! the client receives are two different claims, and only a real database
//! connects them.
//!
//! This suite asserts the bytes. Each fixture row carries a `secret` field that
//! no query selects, so a projection that degraded to "select the whole `data`
//! column" is visible in the response rather than inferred from a SQL string.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-core --test selection_conformance_postgres
//! ```
//!
//! Runs in the Dagger `integration: postgres` leg via its `--test '*'` sweep.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::{Executor, relay::encode_node_id},
    schema::{CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

/// Uniquely named so it cannot collide with the fixtures other suites share in
/// the same database.
const VIEW: &str = "v_selection_conformance_user";

const ALICE_UUID: &str = "aaaa0000-0000-0000-0000-000000000001";

/// `pk_user` is both a real column (the keyset cursor the relay SQL orders and
/// filters on) and a `data` key (the runtime reads the emitted cursor out of the
/// JSONB blob).
const FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_selection_conformance_user;
CREATE TABLE v_selection_conformance_user (pk_user bigint, data jsonb);
INSERT INTO v_selection_conformance_user (pk_user, data) VALUES
  (1, '{"id":"aaaa0000-0000-0000-0000-000000000001","name":"Alice","pk_user":1,
     "email":"alice@example.com","secret":"SECRET-VALUE"}'::jsonb),
  (2, '{"id":"bbbb0000-0000-0000-0000-000000000002","name":"Bob","pk_user":2,
     "email":"bob@example.com","secret":"SECRET-VALUE"}'::jsonb);
"#;

fn schema() -> CompiledSchema {
    let user_type = TestTypeBuilder::new("User", VIEW)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("email", FieldType::String)
        .with_simple_field("secret", FieldType::String)
        .build();

    let users_query = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();

    // The same rows behind a Relay connection, so the relay runner's arguments
    // can be asserted in rows against a real database (#904).
    let mut connection_query = TestQueryBuilder::new("usersConnection", "User")
        .returns_list(true)
        .with_sql_source(VIEW)
        .relay_cursor_column("pk_user")
        .build();
    connection_query.auto_params.has_where = true;
    connection_query.auto_params.has_order_by = true;

    let mut schema = TestSchemaBuilder::new()
        .with_type(user_type)
        .with_query(users_query)
        .with_query(connection_query)
        .build();
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema
}

async fn executor() -> Option<Executor<PostgresAdapter>> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n") {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter
            .execute_raw_query(stmt)
            .await
            .expect("provision the conformance fixture");
    }
    Some(Executor::new_with_relay(schema(), Arc::new(adapter)))
}

/// Response keys of `data.node`, in response order.
fn node_keys(response: &Value) -> Vec<String> {
    response["data"]["node"]
        .as_object()
        .unwrap_or_else(|| panic!("expected an object at data.node, got: {response}"))
        .keys()
        .cloned()
        .collect()
}

/// Response keys of the first `users` row, in response order.
fn user_keys(response: &Value) -> Vec<String> {
    response["data"]["users"][0]
        .as_object()
        .unwrap_or_else(|| panic!("expected an object at data.users[0], got: {response}"))
        .keys()
        .cloned()
        .collect()
}

/// `name` of every node in a relay connection, in page order.
fn edge_names(response: &Value) -> Vec<String> {
    response["data"]["usersConnection"]["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("expected edges at data.usersConnection, got: {response}"))
        .iter()
        .map(|e| {
            e["node"]["name"]
                .as_str()
                .unwrap_or_else(|| panic!("edge node has no name: {response}"))
                .to_string()
        })
        .collect()
}

/// One case: a document, and exactly the response keys it must produce.
struct Case {
    name:   &'static str,
    query:  &'static str,
    expect: &'static [&'static str],
}

/// `node(id:)` — the entry point that expanded no spreads and evaluated no
/// directives (#827).
const NODE_MATRIX: &[Case] = &[
    Case {
        name:   "node with a named spread",
        query:  "fragment F on User { name email } query { node(id: \"@ID@\") { id ...F } }",
        expect: &["id", "name", "email"],
    },
    Case {
        name:   "node with a spread-only selection",
        query:  "fragment F on User { name } query { node(id: \"@ID@\") { ...F } }",
        expect: &["name"],
    },
    Case {
        name:   "node with a skipped spread",
        query:  "fragment F on User { email } query { node(id: \"@ID@\") { id ...F @skip(if: \
                 true) } }",
        expect: &["id"],
    },
    Case {
        name:   "node with an included spread",
        query:  "fragment F on User { email } query { node(id: \"@ID@\") { id ...F @include(if: \
                 true) } }",
        expect: &["id", "email"],
    },
    Case {
        name:   "node with a skipped field",
        query:  "{ node(id: \"@ID@\") { id name @skip(if: true) } }",
        expect: &["id"],
    },
    Case {
        name:   "node with an inline fragment",
        query:  "{ node(id: \"@ID@\") { id ... on User { email } } }",
        expect: &["id", "email"],
    },
    Case {
        name:   "node with a skipped inline fragment",
        query:  "{ node(id: \"@ID@\") { id ... on User @skip(if: true) { email } } }",
        expect: &["id"],
    },
    Case {
        name:   "node with a nested spread",
        query:  "fragment Inner on User { email } fragment Outer on User { name ...Inner } query \
                 { node(id: \"@ID@\") { id ...Outer } }",
        expect: &["id", "name", "email"],
    },
    Case {
        name:   "node with everything skipped",
        query:  "{ node(id: \"@ID@\") { id @skip(if: true) } }",
        expect: &[],
    },
];

#[tokio::test]
async fn node_query_returns_exactly_the_requested_fields() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let node_id = encode_node_id("User", ALICE_UUID);

    let mut failures = Vec::new();
    for case in NODE_MATRIX {
        let query = case.query.replace("@ID@", &node_id);
        match exec.execute(&query, None).await {
            Ok(response) => {
                let keys = node_keys(&response);
                if keys != case.expect {
                    failures.push(format!(
                        "{}: expected {:?}, got {keys:?}\n    response: {response}",
                        case.name, case.expect
                    ));
                }
            },
            Err(e) => failures.push(format!("{}: execution failed: {e}", case.name)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The same matrix through `/graphql`'s regular query path, so the two entry
/// points are pinned to one answer rather than each to its own.
const QUERY_MATRIX: &[Case] = &[
    Case {
        name:   "named spread",
        query:  "fragment F on User { name email } query { users { id ...F } }",
        expect: &["id", "name", "email"],
    },
    Case {
        name:   "skipped spread",
        query:  "fragment F on User { name email } query { users { id ...F @skip(if: true) } }",
        expect: &["id"],
    },
    Case {
        name:   "spread at its document position",
        query:  "fragment F on User { name } query { users { email ...F id } }",
        expect: &["email", "name", "id"],
    },
    Case {
        name:   "nested spread, inner skipped",
        query:  "fragment Inner on User { email } fragment Outer on User { name ...Inner \
                 @skip(if: true) } query { users { id ...Outer } }",
        expect: &["id", "name"],
    },
    Case {
        name:   "everything skipped",
        query:  "{ users { id @skip(if: true) } }",
        expect: &[],
    },
];

#[tokio::test]
async fn regular_query_returns_exactly_the_requested_fields() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for case in QUERY_MATRIX {
        match exec.execute(case.query, None).await {
            Ok(response) => {
                let keys = user_keys(&response);
                if keys != case.expect {
                    failures.push(format!(
                        "{}: expected {:?}, got {keys:?}\n    response: {response}",
                        case.name, case.expect
                    ));
                }
            },
            Err(e) => failures.push(format!("{}: execution failed: {e}", case.name)),
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// The over-disclosure half of #827, stated as its own assertion rather than
/// left implicit in a field-set comparison: whatever the selection resolves to,
/// `secret` is never served unless it was asked for.
#[tokio::test]
async fn an_unselected_field_never_reaches_the_response() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let node_id = encode_node_id("User", ALICE_UUID);

    for query in [
        format!("fragment F on User {{ name }} query {{ node(id: \"{node_id}\") {{ ...F }} }}"),
        format!("{{ node(id: \"{node_id}\") {{ id @skip(if: true) }} }}"),
        format!("{{ node(id: \"{node_id}\") {{ id }} }}"),
        "fragment F on User { name } query { users { ...F } }".to_string(),
    ] {
        let response = exec.execute(&query, None).await.expect("query must run");
        let rendered = response.to_string();
        assert!(
            !rendered.contains("SECRET-VALUE"),
            "unselected field served for `{query}`: {rendered}"
        );
    }

    // …and it *is* served when the client asks for it, so the assertion above is
    // not passing because the column is unreachable.
    let response = exec
        .execute(&format!("{{ node(id: \"{node_id}\") {{ secret }} }}"), None)
        .await
        .expect("query must run");
    assert_eq!(response["data"]["node"]["secret"], json!("SECRET-VALUE"), "{response}");
}

/// A multi-root query fans out into one synthetic sub-query per root. Fragment
/// definitions and directives do not travel in a re-serialized root unless they
/// were resolved first.
#[tokio::test]
async fn multi_root_query_carries_fragments_and_directives() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let response = exec
        .execute(
            "fragment F on User { name } query { a: users { id ...F } b: users { id email \
             @skip(if: true) } }",
            None,
        )
        .await
        .expect("multi-root query with a fragment must run");

    let a_keys: Vec<String> = response["data"]["a"][0]
        .as_object()
        .unwrap_or_else(|| panic!("{response}"))
        .keys()
        .cloned()
        .collect();
    let b_keys: Vec<String> = response["data"]["b"][0]
        .as_object()
        .unwrap_or_else(|| panic!("{response}"))
        .keys()
        .cloned()
        .collect();
    assert_eq!(a_keys, vec!["id", "name"], "{response}");
    assert_eq!(b_keys, vec!["id"], "{response}");
}

/// #904: an inline `where:` on a Relay connection must narrow the page.
///
/// Asserted in **rows**, not in the absence of an error. A dropped filter does
/// not fail the query — it succeeds and serves every row the caller is allowed
/// to see, shaped exactly like the filtered set the client asked for. Only a
/// row count distinguishes the two.
#[tokio::test]
async fn relay_inline_arguments_narrow_the_page() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    // Control: no arguments — the whole connection.
    let all = exec
        .execute("{ usersConnection { edges { node { name } } } }", None)
        .await
        .expect("unfiltered relay query must run");
    assert_eq!(
        edge_names(&all),
        vec!["Alice", "Bob"],
        "the unfiltered connection is the baseline the filtered cases narrow: {all}"
    );

    // Inline `where:` — one row.
    let inline = exec
        .execute(
            r#"{ usersConnection(where: { name: { eq: "Alice" } }) { edges { node { name } } } }"#,
            None,
        )
        .await
        .expect("inline-filtered relay query must run");
    assert_eq!(
        edge_names(&inline),
        vec!["Alice"],
        "an inline `where:` must narrow the page; serving both rows is the whole of #904 — \
         the response is a superset of what was asked for, and nothing says so: {inline}"
    );

    // The variable form of the same filter agrees.
    let via_variable = exec
        .execute(
            "query($w: UserWhereInput) { usersConnection(where: $w) { edges { node { name } } } }",
            Some(&json!({"w": {"name": {"eq": "Alice"}}})),
        )
        .await
        .expect("variable-filtered relay query must run");
    assert_eq!(
        edge_names(&via_variable),
        edge_names(&inline),
        "the inline and variable forms of `where:` must return the same rows: {via_variable}"
    );

    // Inline `first:` bounds the page against the real LIMIT.
    let first_one = exec
        .execute("{ usersConnection(first: 1) { edges { node { name } } } }", None)
        .await
        .expect("inline `first:` relay query must run");
    assert_eq!(
        edge_names(&first_one),
        vec!["Alice"],
        "an inline `first: 1` must bound the page to one edge: {first_one}"
    );
}
