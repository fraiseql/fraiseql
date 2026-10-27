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
    schema::{
        CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition, SessionVariableMapping,
        SessionVariableSource,
    },
    security::SecurityContext,
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
     "email":"alice@example.com","secret":"SECRET-VALUE",
     "profile":{"tier":"gold","zone":"eu","address":{"city":"Paris","zip":"75001"}}}'::jsonb),
  (2, '{"id":"bbbb0000-0000-0000-0000-000000000002","name":"Bob","pk_user":2,
     "email":"bob@example.com","secret":"SECRET-VALUE",
     "profile":{"tier":"silver","zone":"us","address":{"city":"Austin","zip":"73301"}}}'::jsonb);
"#;

/// Numbers whose text `serde_json` writes back otherwise: a scale, a width past `u64`, more
/// digits than an `f64` holds, an exponent, `-0`. `jsonb` keeps a number's scale and digits
/// and drops its exponent.
const MEASURE_VIEW: &str = "v_selection_conformance_measure";
const MEASURE_FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_selection_conformance_measure;
CREATE TABLE v_selection_conformance_measure (data jsonb);
INSERT INTO v_selection_conformance_measure (data) VALUES
  ('{"price":1.50,"score":0.1,"total":100.00,"count":7}'::jsonb),
  ('{"price":12345678901234567890.12,"score":1e3,"total":123456789012345678.99,
     "count":18446744073709551616}'::jsonb),
  ('{"price":100000000000000000000,"score":-0,"total":2.5E-3,"count":-9223372036854775809}'::jsonb),
  ('{"price":19.99,"score":1.5e300,"total":-0.0,"count":1234567890123456789}'::jsonb),
  ('{"price":0.000000000000000000001,"score":0.3333333333333333333333,"total":1.0,
     "count":0}'::jsonb);
DROP TABLE IF EXISTS v_selection_conformance_ledger;
CREATE TABLE v_selection_conformance_ledger (pk_ledger bigint, data jsonb);
INSERT INTO v_selection_conformance_ledger (pk_ledger, data) VALUES
  (1, '{"total":1.50,"entry":{"amount":100.00,"memo":"m"},
     "lines":[{"amount":-0.001,"qty":2},{"amount":0,"qty":1}],"rates":[0.10,[2.50],null]}'::jsonb),
  (2, '{"total":12345678901234567890.12,"entry":null,"lines":[],"rates":[]}'::jsonb);
"#;

/// Decimals at the root, in a nested object, in a list of objects and in a list of lists.
const LEDGER_VIEW: &str = "v_selection_conformance_ledger";

/// A document of 120 keys, past the 50 pairs one `jsonb_build_object` call could carry (#1544).
const WIDE_VIEW: &str = "v_selection_conformance_wide";
const WIDE_FIELDS: usize = 120;

/// The `i`-th wide field: letters only (`kaa`, `kab`, …), so its stored key is its own name.
fn wide_field(i: usize) -> String {
    let letter = |n: usize| char::from(b'a' + u8::try_from(n % 26).unwrap());
    format!("k{}{}", letter(i / 26), letter(i))
}

fn schema() -> CompiledSchema {
    let user_type = TestTypeBuilder::new("User", VIEW)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("email", FieldType::String)
        .with_simple_field("secret", FieldType::String)
        // A nested single object, so `__typename` inside one has somewhere to land (#912).
        .with_simple_field("profile", FieldType::Object("Profile".to_string()))
        .build();

    let profile_type = TestTypeBuilder::new("Profile", "v_selection_conformance_profile")
        .with_simple_field("tier", FieldType::String)
        .with_simple_field("zone", FieldType::String)
        .with_simple_field("address", FieldType::Object("Address".to_string()))
        .build();
    let address_type = TestTypeBuilder::new("Address", "v_selection_conformance_address")
        .with_simple_field("city", FieldType::String)
        .with_simple_field("zip", FieldType::String)
        .build();
    let mut wide_type = TestTypeBuilder::new("Wide", WIDE_VIEW);
    for i in 0..WIDE_FIELDS {
        wide_type = wide_type.with_simple_field(&wide_field(i), FieldType::Int);
    }
    let measure_type = TestTypeBuilder::new("Measure", MEASURE_VIEW)
        .with_simple_field("price", FieldType::Decimal)
        .with_simple_field("score", FieldType::Float)
        .with_simple_field("total", FieldType::Decimal)
        .with_simple_field("count", FieldType::Int)
        .build();
    let entry_type = TestTypeBuilder::new("Entry", "v_selection_conformance_entry")
        .with_simple_field("amount", FieldType::Decimal)
        .with_simple_field("memo", FieldType::String)
        .with_simple_field("qty", FieldType::Int)
        .build();
    let ledger_type = TestTypeBuilder::new("Ledger", LEDGER_VIEW)
        .with_simple_field("total", FieldType::Decimal)
        .with_nullable_field("entry", FieldType::Object("Entry".to_string()))
        .with_simple_field(
            "lines",
            FieldType::List {
                item:           Box::new(FieldType::Object("Entry".to_string())),
                nullable_items: false,
            },
        )
        .with_simple_field(
            "rates",
            FieldType::List {
                item:           Box::new(FieldType::List {
                    item:           Box::new(FieldType::Decimal),
                    nullable_items: true,
                }),
                nullable_items: true,
            },
        )
        .build();
    let ledgers_query = TestQueryBuilder::new("ledgers", "Ledger")
        .returns_list(true)
        .with_sql_source(LEDGER_VIEW)
        .build();
    let measures_query = TestQueryBuilder::new("measures", "Measure")
        .returns_list(true)
        .with_sql_source(MEASURE_VIEW)
        .build();
    let wides_query = TestQueryBuilder::new("wides", "Wide")
        .returns_list(true)
        .with_sql_source(WIDE_VIEW)
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
        .with_type(profile_type)
        .with_type(address_type)
        .with_type(wide_type.build())
        .with_type(measure_type)
        .with_type(entry_type)
        .with_type(ledger_type)
        .with_query(ledgers_query)
        .with_query(users_query)
        .with_query(measures_query)
        .with_query(wides_query)
        .with_query(connection_query)
        .build();
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema
}

async fn executor() -> Option<Executor> {
    executor_for(schema()).await
}

/// [`executor`] over `schema`: the same fixture, provisioned the same way.
async fn executor_for(schema: CompiledSchema) -> Option<Executor> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n").chain(MEASURE_FIXTURE.split(";\n")) {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter
            .execute_raw_query(stmt)
            .await
            .expect("provision the conformance fixture");
    }
    let wide_document = (0..WIDE_FIELDS)
        .map(|i| format!("'{}', {i}", wide_field(i)))
        .collect::<Vec<_>>();
    let wide_document = wide_document
        .chunks(50)
        .map(|chunk| format!("jsonb_build_object({})", chunk.join(", ")))
        .collect::<Vec<_>>()
        .join(" || ");
    for stmt in [
        format!("DROP TABLE IF EXISTS {WIDE_VIEW}"),
        format!("CREATE TABLE {WIDE_VIEW} (data jsonb)"),
        format!("INSERT INTO {WIDE_VIEW} (data) SELECT {wide_document}"),
    ] {
        adapter.execute_raw_query(&stmt).await.expect("provision the wide fixture");
    }
    Some(Executor::new_with_relay(schema, Arc::new(adapter)))
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

/// #939: an undeclared field is a validation error, and execution never starts.
///
/// The symptom is only visible against a real database: the unknown name was
/// lowered into the projection as `data->>'phantom_field'`, which PostgreSQL
/// evaluates to NULL and the runtime serialises as a legitimate-looking `null`
/// under HTTP 200 with no `errors` array. A mock adapter that hands back the
/// fixture row cannot produce that null, so it cannot show the defect.
#[tokio::test]
async fn an_undeclared_field_never_reaches_the_database() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let err = exec
        .execute("{ users { phantom_field } }", None)
        .await
        .expect_err("a selection on an undeclared field is an invalid document (§ 5.3.1)");
    let msg = err.to_string();
    assert!(
        msg.contains("phantom_field") && msg.contains("User"),
        "the error names the field and the type it is not on, got: {msg}"
    );

    // The same document one level down, and through a spread.
    for query in [
        "{ users { id profile { nope } } }",
        "fragment F on User { emial } query { users { id ...F } }",
    ] {
        let err = exec.execute(query, None).await.expect_err(&format!("invalid: {query}"));
        assert!(
            matches!(err, fraiseql_core::error::FraiseQLError::Validation { .. }),
            "expected a validation error for `{query}`, got: {err:?}"
        );
    }

    // Control: the declared shape still executes against the same fixture, so
    // the assertions above are not passing because the query is broken for some
    // unrelated reason.
    let ok = exec
        .execute("{ users { id name email } }", None)
        .await
        .expect("the declared shape must still run");
    assert_eq!(ok["data"]["users"][0]["name"], json!("Alice"), "{ok}");
}

/// #912: a nested `__typename` survives the real SQL projection.
///
/// This is the case only a real database can make: `__typename` is a meta-field,
/// not a JSONB key, so it is stripped from the projection at every depth — a
/// projection that emitted it would produce `data->>'__typename'`, a literal
/// NULL. The mock-adapter suite cannot see that, because the mock hands back the
/// fixture row whole and never applies the hint at all.
#[tokio::test]
async fn nested_typename_survives_the_sql_projection() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let response = exec
        .execute("{ users { id profile { __typename tier } } }", None)
        .await
        .expect("nested-typename query must run");

    let profile = response["data"]["users"][0]["profile"]
        .as_object()
        .unwrap_or_else(|| panic!("expected a profile object: {response}"));

    assert_eq!(
        profile.get("__typename").and_then(Value::as_str),
        Some("Profile"),
        "`__typename` is `String!` — a requested nested one is neither null nor absent: {response}"
    );
    assert_eq!(
        profile.get("tier").and_then(Value::as_str),
        Some("gold"),
        "the sibling field must still be projected: {response}"
    );
    let keys: Vec<&str> = profile.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        vec!["__typename", "tier"],
        "the response's fields follow the query's order: {response}"
    );

    // Control: unrequested stays absent, never present-and-null.
    let plain = exec
        .execute("{ users { profile { tier } } }", None)
        .await
        .expect("control query must run");
    assert!(
        !plain["data"]["users"][0]["profile"]
            .as_object()
            .unwrap_or_else(|| panic!("{plain}"))
            .contains_key("__typename"),
        "an unrequested `__typename` must be absent, not null: {plain}"
    );
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

/// The keys of an object, in response order.
fn keys_of(value: &Value) -> Vec<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("expected an object, got: {value}"))
        .keys()
        .cloned()
        .collect()
}

/// GraphQL § 6.3: a nested object's fields come back in the order the selection asks for them,
/// at every depth (#1563). jsonb stores keys shortest first, so a projection built as jsonb
/// answered `tier, zone, address` and `zip, city`.
#[tokio::test]
async fn nested_objects_follow_the_selection_order() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: no PostgreSQL bound");
        return;
    };
    let response = executor
        .execute("{ users { profile { zone address { city zip } tier } } }", None)
        .await
        .unwrap();
    let profile = &response["data"]["users"][0]["profile"];
    assert_eq!(keys_of(profile), ["zone", "address", "tier"], "{response}");
    assert_eq!(keys_of(&profile["address"]), ["city", "zip"], "{response}");
}

/// 120 fields selected in reverse: all of them come back, in that order (#1544, #1563).
#[tokio::test]
async fn a_wide_selection_comes_back_whole_and_in_order() {
    let Some(executor) = executor().await else {
        eprintln!("skipping: no PostgreSQL bound");
        return;
    };
    let selected: Vec<String> = (0..WIDE_FIELDS).rev().map(wide_field).collect();
    let response = executor
        .execute(&format!("{{ wides {{ {} }} }}", selected.join(" ")), None)
        .await
        .unwrap();
    assert!(response.get("errors").is_none(), "{response}");
    assert_eq!(keys_of(&response["data"]["wides"][0]), selected, "{response}");
}

/// Reads the database answers whole: every field a stored scalar, or an object whose
/// sub-fields are. Such a read needs no shaping in Rust, so its body is the bytes the
/// database returned.
const RAW_ELIGIBLE: &[&str] = &[
    "{ users { id name email } }",
    "{ users { email id } }",
    "{ users { handle: name id } }",
    "{ users { name profile { tier address { city zip } } } }",
    "query Named { users { name } }",
    "{ wides { kaa kab kbb kep } }",
    "{ measures { price score total count } }",
];

/// Reads that must stay on the `Value` path: each needs a step the bytes cannot take.
const RAW_INELIGIBLE: &[(&str, &str)] = &[
    ("`__typename` at the root", "{ users { __typename name } }"),
    ("`__typename` in a nested object", "{ users { profile { __typename tier } } }"),
    ("two root fields", "{ users { name } wides { kaa } }"),
    (
        "a relay connection",
        "{ usersConnection(first: 1) { edges { node { name } } } }",
    ),
    ("nothing selected", "{ users { id @skip(if: true) } }"),
];

/// The body the `Value` path serializes, for the same document.
async fn value_body(exec: &Executor, query: &str) -> Vec<u8> {
    let value = exec.execute(query, None).await.unwrap_or_else(|e| panic!("{query}: {e}"));
    serde_json::to_vec(&value).unwrap()
}

/// Whether a response's root field holds rows. Two bodies of no rows agree whatever path
/// read them, so a differential over an empty read compares nothing.
fn holds_rows(body: &[u8]) -> bool {
    let value: Value = serde_json::from_slice(body).unwrap();
    value["data"]
        .as_object()
        .and_then(|data| data.values().next())
        .is_some_and(|root| root.as_array().map_or(root.is_object(), |rows| !rows.is_empty()))
}

#[tokio::test]
async fn an_eligible_read_is_answered_raw_with_the_value_paths_bytes() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for query in RAW_ELIGIBLE {
        let expected = value_body(&exec, query).await;
        assert!(holds_rows(&expected), "{query}: the fixture returned no rows");
        let body = exec
            .execute_operation_body(query, None, None)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        if !body.is_raw() {
            failures.push(format!("{query}: answered as a `Value`, not raw"));
            continue;
        }
        let bytes = body.into_bytes().unwrap();
        if bytes != expected {
            failures.push(format!(
                "{query}:\n    raw   {}\n    value {}",
                String::from_utf8_lossy(&bytes),
                String::from_utf8_lossy(&expected)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} read(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

#[tokio::test]
async fn an_ineligible_read_stays_on_the_value_path() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for (why, query) in RAW_INELIGIBLE {
        let expected = value_body(&exec, query).await;
        let body = exec
            .execute_operation_body(query, None, None)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        if body.is_raw() {
            failures.push(format!("{why}: answered raw"));
        } else if body.into_bytes().unwrap() != expected {
            failures.push(format!("{why}: the `Value` body differs from `execute`'s"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} read(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// An authenticated caller with nothing that gates the fixture.
fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("conformance"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       std::collections::HashMap::new(),
        request_id:       "req-conformance".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The same reads for an authenticated caller, whose session variables ride on the read's
/// connection: answered raw, and the bytes the `Value` path serializes for that caller.
#[tokio::test]
async fn an_authenticated_eligible_read_is_answered_raw_with_the_value_paths_bytes() {
    let mut schema = schema();
    schema.session_variables.variables.push(SessionVariableMapping {
        name:   "app.conformance".to_string(),
        source: SessionVariableSource::Literal {
            value: "on".to_string(),
        },
    });
    let Some(exec) = executor_for(schema).await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let principal = principal();

    let mut failures = Vec::new();
    for query in RAW_ELIGIBLE {
        let value = exec
            .execute_operation_with_security(query, None, &principal, None)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let expected = serde_json::to_vec(&value).unwrap();
        assert!(holds_rows(&expected), "{query}: the fixture returned no rows");
        let body = exec
            .execute_operation_with_security_body(query, None, &principal, None)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        if !body.is_raw() {
            failures.push(format!("{query}: answered as a `Value`, not raw"));
        } else if body.into_bytes().unwrap() != expected {
            failures.push(format!("{query}: the raw body differs from the `Value` body"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} read(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// What PostgreSQL itself prints for `expr` over `view`'s rows, in table order: the digits a
/// `Decimal` was stored with.
async fn stored_digits(view: &str, expr: &str) -> Vec<Value> {
    let pg = fraiseql_test_support::postgres().await.unwrap();
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    adapter
        .execute_raw_query(&format!("SELECT {expr} AS v FROM {view}"))
        .await
        .unwrap()
        .into_iter()
        .map(|mut row| row.remove("v").unwrap())
        .collect()
}

/// A field of every row of a response's root list.
fn column(response: &Value, root: &str, field: &str) -> Vec<Value> {
    response["data"][root]
        .as_array()
        .unwrap_or_else(|| panic!("expected a list at data.{root}: {response}"))
        .iter()
        .map(|row| row[field].clone())
        .collect()
}

/// A `Decimal` is a JSON string of the digits PostgreSQL stored, on the `Value` path
/// and on the raw path alike; a `Float` and an `Int` stay numbers.
#[tokio::test]
async fn a_decimal_is_answered_as_the_digits_it_was_stored_with() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let price = stored_digits(MEASURE_VIEW, "data->>'price'").await;
    let total = stored_digits(MEASURE_VIEW, "data->>'total'").await;
    assert!(price.iter().all(Value::is_string), "{price:?}");

    let tree = exec
        .execute("{ measures { __typename price total score } }", None)
        .await
        .unwrap();
    let raw_body = exec
        .execute_operation_body("{ measures { price total score } }", None, None)
        .await
        .unwrap();
    assert!(raw_body.is_raw(), "the read without `__typename` is answered raw");
    let raw: Value = serde_json::from_slice(&raw_body.into_bytes().unwrap()).unwrap();
    for (path, response) in [("value", &tree), ("raw", &raw)] {
        assert_eq!(column(response, "measures", "price"), price, "{path}: {response}");
        assert_eq!(column(response, "measures", "total"), total, "{path}: {response}");
        assert!(
            column(response, "measures", "score").iter().all(Value::is_number),
            "{path}: a Float stays a number: {response}"
        );
    }
}

/// A `Decimal` keeps its digits in a nested object, in a list of objects and in a list
/// of lists, under an alias, and on the `Value` path a `__typename` forces.
#[tokio::test]
async fn a_decimal_below_the_root_keeps_its_digits() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let expected = json!([
        {"total": "1.50", "entry": {"amount": "100.00"},
         "lines": [{"amount": "-0.001", "qty": 2}, {"amount": "0", "qty": 1}],
         "rates": ["0.10", ["2.50"], null]},
        {"total": "12345678901234567890.12", "entry": null, "lines": [], "rates": []}
    ]);
    let response = exec
        .execute("{ ledgers { total entry { amount } lines { amount qty } rates } }", None)
        .await
        .unwrap();
    assert_eq!(response["data"]["ledgers"], expected, "{response}");

    let typed = exec
        .execute("{ ledgers { __typename total entry { __typename amount } lines { amount qty } rates } }", None)
        .await
        .unwrap();
    assert_eq!(
        column(&typed, "ledgers", "total"),
        column(&response, "ledgers", "total"),
        "{typed}"
    );
    assert_eq!(typed["data"]["ledgers"][0]["entry"]["amount"], "100.00", "{typed}");

    let aliased = exec
        .execute("{ ledgers { t: total entry { a: amount } lines { n: amount } } }", None)
        .await
        .unwrap();
    assert_eq!(
        aliased["data"]["ledgers"][0],
        json!({"t": "1.50", "entry": {"a": "100.00"}, "lines": [{"n": "-0.001"}, {"n": "0"}]}),
        "{aliased}"
    );
}
