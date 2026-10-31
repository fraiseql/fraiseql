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
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::{Executor, relay::encode_node_id},
    schema::{
        CompiledSchema, FieldDefinition, FieldDenyPolicy, FieldType, InterfaceDefinition,
        RoleDefinition, SecurityConfig, SessionVariableMapping, SessionVariableSource, SourceKind,
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
DROP TABLE IF EXISTS v_selection_conformance_ledger CASCADE;
CREATE TABLE v_selection_conformance_ledger (pk_ledger bigint, data jsonb);
INSERT INTO v_selection_conformance_ledger (pk_ledger, data) VALUES
  (1, '{"total":1.50,"entry":{"amount":100.00,"memo":"m","splits":[0.10,2.500]},
     "lines":[{"amount":-0.001,"qty":2},{"amount":0,"qty":1}],"rates":[0.10,[2.50],null]}'::jsonb),
  (2, '{"total":12345678901234567890.12,"entry":null,"lines":[],"rates":[]}'::jsonb);
CREATE OR REPLACE FUNCTION fn_selection_conformance_ledger() RETURNS SETOF v_selection_conformance_ledger LANGUAGE sql STABLE AS 'SELECT * FROM v_selection_conformance_ledger';
"#;

/// Every key stored `snake_case` under a `camelCase` field, at the root, in a nested object
/// two levels deep and in a list of objects holding an object, plus keys no query selects.
const PARITY_VIEW: &str = "v_selection_conformance_parity";
const PARITY_FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_selection_conformance_parity;
CREATE TABLE v_selection_conformance_parity (data jsonb);
INSERT INTO v_selection_conformance_parity (data) VALUES
  ('{"id":"c1","display_name":"Ada","unselected":"ROOT",
     "home_address":{"postal_code":"75001","street_line":"1 rue","unselected":"D1",
       "home_region":{"region_name":"IDF","unselected":"D2"}},
     "past_orders":[{"order_total":12,"unselected":"L1",
       "ship_to":{"postal_code":"69001","street_line":"2 rue","unselected":"L2"}}]}'::jsonb),
  ('{"id":"c2","display_name":"Bob","home_address":null,"past_orders":[]}'::jsonb);
"#;

/// Lists of objects: an empty list, a `null` list, a missing one, a `null` element, elements
/// holding keys nobody selects and objects of their own, in an order the rows must keep.
const ROSTER_VIEW: &str = "v_selection_conformance_roster";
const ROSTER_FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_selection_conformance_roster;
CREATE TABLE v_selection_conformance_roster (pk_roster bigint, data jsonb);
INSERT INTO v_selection_conformance_roster (pk_roster, data) VALUES
  (1, '{"name":"r1","members":[{"name":"c","role":"lead","unselected":"E1"},
       {"name":"a","role":null}],"reserves":[{"name":"q","role":"cook"},null],
     "crew":[{"name":"z","role":"cook","home":{"city":"Lyon","zip":"1"},
         "tasks":[{"what":"fry"},{"what":"boil"}]},
       {"name":"y","role":"mate","home":null,"tasks":[]},{"name":"x","role":"cook"}]}'::jsonb),
  (2, '{"name":"r2","members":[],"crew":[{"name":"w","role":"mate","home":{"city":"Nice"}}]}'::jsonb),
  (3, '{"name":"r3","members":null,"crew":[]}'::jsonb),
  (4, '{"name":"r4","crew":[]}'::jsonb);
"#;

/// Decimals at the root, in a nested object, in a list of objects and in a list of lists.
const LEDGER_VIEW: &str = "v_selection_conformance_ledger";
/// A set-returning function over [`LEDGER_VIEW`]'s rows.
const LEDGER_FUNCTION: &str = "fn_selection_conformance_ledger";

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
        // An abstract position: its `__typename` is the concrete type of each value.
        .with_nullable_field("partner", FieldType::Interface("Node".to_string()))
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
        .with_simple_field(
            "splits",
            FieldType::List {
                item:           Box::new(FieldType::Decimal),
                nullable_items: true,
            },
        )
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
    let region_type = TestTypeBuilder::new("ParityRegion", "v_selection_conformance_region")
        .with_simple_field("regionName", FieldType::String)
        .build();
    let parity_address_type = TestTypeBuilder::new("ParityAddress", "v_selection_conformance_pa")
        .with_simple_field("postalCode", FieldType::String)
        .with_simple_field("streetLine", FieldType::String)
        .with_nullable_field("homeRegion", FieldType::Object("ParityRegion".to_string()))
        .build();
    let order_type = TestTypeBuilder::new("ParityOrder", "v_selection_conformance_order")
        .with_simple_field("orderTotal", FieldType::Int)
        .with_nullable_field("shipTo", FieldType::Object("ParityAddress".to_string()))
        .build();
    let customer_type = TestTypeBuilder::new("ParityCustomer", PARITY_VIEW)
        .with_simple_field("id", FieldType::String)
        .with_simple_field("displayName", FieldType::String)
        .with_nullable_field("homeAddress", FieldType::Object("ParityAddress".to_string()))
        .with_simple_field(
            "pastOrders",
            FieldType::List {
                item:           Box::new(FieldType::Object("ParityOrder".to_string())),
                nullable_items: false,
            },
        )
        .build();
    let home_type = TestTypeBuilder::new("RosterHome", "v_selection_conformance_home")
        .with_simple_field("city", FieldType::String)
        .build();
    let task_type = TestTypeBuilder::new("RosterTask", "v_selection_conformance_task")
        .with_simple_field("what", FieldType::String)
        .build();
    let member_type = TestTypeBuilder::new("Member", "v_selection_conformance_member")
        .with_simple_field("name", FieldType::String)
        .with_nullable_field("role", FieldType::String)
        .with_nullable_field("home", FieldType::Object("RosterHome".to_string()))
        .with_nullable_field(
            "tasks",
            FieldType::List {
                item:           Box::new(FieldType::Object("RosterTask".to_string())),
                nullable_items: false,
            },
        )
        .build();
    let roster_type = TestTypeBuilder::new("Roster", ROSTER_VIEW)
        .with_simple_field("name", FieldType::String)
        .with_nullable_field(
            "members",
            FieldType::List {
                item:           Box::new(FieldType::Object("Member".to_string())),
                nullable_items: true,
            },
        )
        .with_simple_field(
            "crew",
            FieldType::List {
                item:           Box::new(FieldType::Object("Member".to_string())),
                nullable_items: false,
            },
        )
        .with_nullable_field(
            "reserves",
            FieldType::List {
                item:           Box::new(FieldType::Object("Member".to_string())),
                nullable_items: true,
            },
        )
        .build();
    let rosters_query = TestQueryBuilder::new("rosters", "Roster")
        .returns_list(true)
        .with_sql_source(ROSTER_VIEW)
        .build();
    let customers_query = TestQueryBuilder::new("customers", "ParityCustomer")
        .returns_list(true)
        .with_sql_source(PARITY_VIEW)
        .build();
    // The same view read with its entry's amount as a `Float`.
    let entry_estimate_type =
        TestTypeBuilder::new("EntryEstimate", "v_selection_conformance_entry")
            .with_simple_field("amount", FieldType::Float)
            .build();
    let ledger_estimate_type = TestTypeBuilder::new("LedgerEstimate", LEDGER_VIEW)
        .with_nullable_field("entry", FieldType::Object("EntryEstimate".to_string()))
        .build();
    let ledger_estimates_query = TestQueryBuilder::new("ledgerEstimates", "LedgerEstimate")
        .returns_list(true)
        .with_sql_source(LEDGER_VIEW)
        .build();
    let ledgers_query = TestQueryBuilder::new("ledgers", "Ledger")
        .returns_list(true)
        .with_sql_source(LEDGER_VIEW)
        .build();
    // The same rows read from a set-returning function.
    let mut ledger_rows_query = TestQueryBuilder::new("ledgerRows", "Ledger")
        .returns_list(true)
        .with_sql_source(LEDGER_FUNCTION)
        .build();
    ledger_rows_query.sql_source_kind = SourceKind::Function;
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
    // One user, picked by a filter: a single root.
    let mut user_query = TestQueryBuilder::new("user", "User")
        .returns_list(false)
        .with_sql_source(VIEW)
        .build();
    user_query.auto_params.has_where = true;

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
        .with_query(ledger_rows_query)
        .with_type(entry_estimate_type)
        .with_type(region_type)
        .with_type(parity_address_type)
        .with_type(order_type)
        .with_type(customer_type)
        .with_query(customers_query)
        .with_type(home_type)
        .with_type(task_type)
        .with_type(member_type)
        .with_type(roster_type)
        .with_query(rosters_query)
        .with_type(ledger_estimate_type)
        .with_query(ledger_estimates_query)
        .with_query(users_query)
        .with_query(user_query)
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
    for stmt in FIXTURE
        .split(";\n")
        .chain(MEASURE_FIXTURE.split(";\n"))
        .chain(PARITY_FIXTURE.split(";\n"))
        .chain(ROSTER_FIXTURE.split(";\n"))
    {
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

/// `__typename` is a field like any other in the response: answered where it was selected,
/// under its alias, at the root as below it (§ 6.3).
#[tokio::test]
async fn typename_is_answered_where_it_was_selected_under_its_alias() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for (query, keys) in [
        ("{ users { name __typename } }", vec!["name", "__typename"]),
        ("{ users { kind: __typename name } }", vec!["kind", "name"]),
        ("{ users { name kind: __typename } }", vec!["name", "kind"]),
    ] {
        let response = exec.execute(query, None).await.unwrap();
        let user = &response["data"]["users"][0];
        assert_eq!(keys_of(user), keys, "{query}: {response}");
        let typename = user[keys.iter().find(|k| **k != "name").unwrap()].as_str();
        assert_eq!(typename, Some("User"), "{query}: {response}");
    }
}

/// A type name is written into the SQL as a quoted literal: whatever a compiled schema
/// names its type, the database reads it as data and answers it as written.
#[tokio::test]
async fn a_typename_is_quoted_as_data_whatever_it_holds() {
    let name = "Quote'd'); DROP TABLE x; --";
    let quoted = TestTypeBuilder::new(name, VIEW)
        .with_simple_field("name", FieldType::String)
        .build();
    let quoted_query = TestQueryBuilder::new("quoted", name)
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    let schema = TestSchemaBuilder::new().with_type(quoted).with_query(quoted_query).build();
    let Some(exec) = executor_for(schema).await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let body = exec
        .execute_operation_body("{ quoted { __typename name } }", None, None)
        .await
        .unwrap();
    assert!(body.is_raw(), "answered from the projection");
    let response: Value = serde_json::from_slice(&body.into_bytes().unwrap()).unwrap();
    assert_eq!(response["data"]["quoted"][0]["__typename"], json!(name), "{response}");
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
    // `__typename` of a statically concrete position is the literal the projection writes,
    // where it was selected and under its alias, at every depth.
    "{ users { __typename name } }",
    "{ users { name __typename } }",
    "{ users { kind: __typename name } }",
    "{ users { profile { __typename tier } } }",
    "{ users { name profile { tier address { city kind: __typename } } __typename } }",
    r#"{ user(where: { name: { eq: "Alice" } }) { name __typename } }"#,
    // Selected twice, one response key: merged before the read (§ 5.3.2).
    "{ users { __typename name __typename } }",
    // A list of objects is built from its elements' selected sub-fields, in element order:
    // an empty list, a `null` list and a missing one stay what they are.
    "{ rosters { name members { name role } } }",
    "{ rosters { members { who: name } crew { role name } } }",
    "{ rosters { crew { name home { city } } name } }",
    "{ rosters { crew { __typename name } } }",
    "{ rosters { crew { name tasks { what } } } }",
    "{ customers { id pastOrders { orderTotal shipTo { postalCode } } } }",
    "{ customers { pastOrders { orderTotal } displayName } }",
    "{ ledgers { lines { amount qty } } }",
];

/// Reads that must stay on the `Value` path: each needs a step the bytes cannot take.
const RAW_INELIGIBLE: &[(&str, &str)] = &[
    (
        "`__typename` below an interface position",
        "{ users { name partner { __typename id } } }",
    ),
    ("two root fields", "{ users { name } wides { kaa } }"),
    ("a list of scalars", "{ ledgers { entry { splits } } }"),
    ("a list of lists", "{ ledgers { rates } }"),
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

/// [`schema`] with `User.secret` and `Profile.zone` masked for a caller without the
/// `auditor` role, which grants their scopes, and `nullable` or not.
fn masked_schema(nullable: bool) -> CompiledSchema {
    let mut schema = schema();
    for (type_name, field, scope) in [
        ("User", "secret", "read:secret"),
        ("Profile", "zone", "read:zone"),
    ] {
        let field = schema
            .types
            .iter_mut()
            .find(|t| t.name == type_name)
            .and_then(|t| t.fields.iter_mut().find(|f| f.name == field))
            .unwrap();
        field.requires_scope = Some(scope.to_string());
        field.on_deny = FieldDenyPolicy::Mask;
        field.nullable = nullable;
    }
    let mut security = SecurityConfig::new();
    security.add_role(RoleDefinition::new(
        "auditor",
        vec!["read:secret".to_string(), "read:zone".to_string()],
    ));
    schema.security = Some(security);
    schema
}

/// Reads that select a masked field, at the root and below it, under an alias or not.
/// Masked fields are nullable here, as a field that can read as `null` is.
const MASKED_READS: &[&str] = &[
    "{ users { name secret } }",
    "{ users { hidden: secret name profile { tier zone } } }",
    "{ users { profile { address { city } region: zone } secret } }",
    r#"{ user(where: { name: { eq: "Alice" } }) { secret name } }"#,
];

/// A caller masked alike reads the masked fields as `null` raw, with the bytes the `Value`
/// path serializes; a caller allowed to read them reads them raw. On one executor, in
/// turn, so a projection kept for one caller is in place when the other reads: it never
/// serves the stored value to the masked caller, nor `null` to the allowed one.
#[tokio::test]
async fn a_masked_read_is_answered_raw_with_the_value_paths_bytes() {
    let Some(exec) = executor_for(masked_schema(true)).await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let masked = principal();
    let allowed = SecurityContext {
        roles: vec!["auditor".to_string()],
        ..principal()
    };

    let mut failures = Vec::new();
    for query in MASKED_READS {
        for (who, caller) in [
            ("allowed", &allowed),
            ("masked", &masked),
            ("allowed", &allowed),
        ] {
            let value = exec
                .execute_operation_with_security(query, None, caller, None)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            let expected = serde_json::to_vec(&value).unwrap();
            assert!(holds_rows(&expected), "{query}: the fixture returned no rows");
            let body = exec
                .execute_operation_with_security_body(query, None, caller, None)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            if !body.is_raw() {
                failures.push(format!("{who} {query}: answered as a `Value`, not raw"));
                continue;
            }
            let bytes = body.into_bytes().unwrap();
            let text = String::from_utf8_lossy(&bytes);
            let stored = ["SECRET-VALUE", "\"eu\""].iter().any(|v| text.contains(v));
            if bytes != expected {
                failures.push(format!(
                    "{who} {query}:\n    raw   {text}\n    value {}",
                    String::from_utf8_lossy(&expected)
                ));
            } else if stored != (who == "allowed") {
                failures.push(format!("{who} {query}: {text}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} read(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// Whether `query`, for `caller`, is answered with the `Value` path's bytes or its error,
/// and whether raw: `Err` with what differs.
async fn as_the_value_path(
    exec: &Executor,
    query: &str,
    caller: &SecurityContext,
) -> Result<bool, String> {
    let value = exec.execute_operation_with_security(query, None, caller, None).await;
    let body = exec.execute_operation_with_security_body(query, None, caller, None).await;
    match (value, body) {
        (Ok(value), Ok(body)) => {
            let expected = serde_json::to_vec(&value).unwrap();
            let raw = body.is_raw();
            let bytes = body.into_bytes().unwrap();
            if bytes == expected {
                Ok(raw)
            } else {
                Err(format!(
                    "{query}:\n    body  {}\n    value {}",
                    String::from_utf8_lossy(&bytes),
                    String::from_utf8_lossy(&expected)
                ))
            }
        },
        (Err(value), Err(body)) if value.to_string() == body.to_string() => Ok(false),
        (value, body) => Err(format!("{query}: value {value:?}, body {body:?}")),
    }
}

/// A `null` under a key some non-null field is selected under, or where no key names it,
/// is completed in Rust (§ 6.4.4); under a nullable field's key alone it is served raw.
/// Each is answered with the `Value` path's bytes, or its error.
#[tokio::test]
async fn a_null_at_a_non_null_position_is_completed_as_the_value_path_completes_it() {
    let mut strict = masked_schema(false);
    let ledger = TestTypeBuilder::new("StrictLedger", LEDGER_VIEW)
        .with_simple_field(
            "rates",
            FieldType::List {
                item:           Box::new(FieldType::List {
                    item:           Box::new(FieldType::Decimal),
                    nullable_items: true,
                }),
                nullable_items: false,
            },
        )
        .build();
    strict.types.push(ledger);
    strict.queries.push(
        TestQueryBuilder::new("strictLedgers", "StrictLedger")
            .returns_list(true)
            .with_sql_source(LEDGER_VIEW)
            .build(),
    );
    strict.build_indexes();
    let nullable = masked_schema(true);
    let caller = principal();

    // (schema, query, answered raw)
    let cases: [(&CompiledSchema, &str, bool); 6] = [
        // A masked nullable field: its `null` needs no completion.
        (&nullable, "{ users { id name secret } }", true),
        // `zone` is non-null at the root (an alias of `name`) and masked, so `null`, below:
        // the key is non-null somewhere, so the read is completed in Rust.
        (&nullable, "{ users { zone: name profile { zone } } }", false),
        // The `null` under the non-null use of the key: completed, up to the nearest
        // nullable parent.
        (&strict, "{ users { name profile { secret: tier } secret } }", false),
        // A masked non-null field.
        (&strict, "{ users { name secret } }", false),
        (&strict, "{ users { name profile { tier zone } } }", false),
        // A `null` element of a `[T]!` whose elements are non-null: a list is never raw.
        (&strict, "{ strictLedgers { rates } }", false),
    ];
    let mut failures = Vec::new();
    for (schema, query, raw) in cases {
        let Some(exec) = executor_for(schema.clone()).await else {
            eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
            return;
        };
        match as_the_value_path(&exec, query, &caller).await {
            Ok(answered_raw) if answered_raw == raw => {},
            Ok(answered_raw) => {
                failures.push(format!("{query}: raw {answered_raw}, expected {raw}"));
            },
            Err(differs) => failures.push(differs),
        }
    }
    assert!(
        failures.is_empty(),
        "{} read(s) failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// A list of objects answers the elements it stored, in their order, each holding what its
/// selection names: `[]` for no elements, `null` for a `null` or missing list, a `null`
/// element kept, and a list inside an element read from that element's own.
#[tokio::test]
async fn a_list_of_objects_answers_its_stored_elements_in_order() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let response = exec
        .execute(
            "{ rosters { members { name } crew { who: name tasks { what } } reserves { name } } }",
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        column(&response, "rosters", "members"),
        vec![
            json!([{"name": "c"}, {"name": "a"}]),
            json!([]),
            Value::Null,
            Value::Null
        ],
        "{response}"
    );
    assert_eq!(
        column(&response, "rosters", "crew")[0],
        json!([
            {"who": "z", "tasks": [{"what": "fry"}, {"what": "boil"}]},
            {"who": "y", "tasks": []},
            {"who": "x", "tasks": null},
        ]),
        "{response}"
    );
    assert_eq!(
        column(&response, "rosters", "reserves"),
        vec![
            json!([{"name": "q"}, null]),
            Value::Null,
            Value::Null,
            Value::Null
        ],
        "{response}"
    );
}

/// A `null` element sits under no key, so a list holding one may hold it at a non-null
/// element type, and the read is completed in Rust (§ 6.4.4), with the `Value` path's bytes.
#[tokio::test]
async fn a_list_holding_a_null_element_is_completed_as_the_value_path_completes_it() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for query in [
        "{ rosters { name reserves { name role } } }",
        "{ rosters { reserves { who: name } crew { role name } } }",
    ] {
        assert_eq!(as_the_value_path(&exec, query, &principal()).await, Ok(false), "{query}");
    }
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

/// A read from a set-returning function keeps every `Decimal`'s digits, at the root, in a
/// nested object and in a list of objects, which it reads as the stored array.
#[tokio::test]
async fn a_decimal_read_from_a_function_keeps_its_digits() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let response = exec
        .execute("{ ledgerRows { total entry { amount } lines { amount } } }", None)
        .await
        .unwrap();
    assert_eq!(
        column(&response, "ledgerRows", "total"),
        vec![json!("1.50"), json!("12345678901234567890.12")],
        "{response}"
    );
    let first = &response["data"]["ledgerRows"][0];
    assert_eq!(first["entry"]["amount"], "100.00", "{response}");
    let nested = exec.execute("{ ledgers { entry { splits } } }", None).await.unwrap();
    assert_eq!(
        nested["data"]["ledgers"][0]["entry"]["splits"],
        json!(["0.10", "2.500"]),
        "a list inside a projected object is read as its stored array: {nested}"
    );
    assert_eq!(first["lines"], json!([{"amount": "-0.001"}, {"amount": "0"}]), "{response}");
}

/// A read answered from the cache hands back the digits the first read did, and a read
/// of the same rows as a `Float` never answers it.
#[tokio::test]
async fn a_cached_decimal_keeps_its_digits() {
    if executor().await.is_none() {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    }
    let pg = fraiseql_test_support::postgres().await.unwrap();
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "decimal-v1".to_string()));
    let exec = Executor::new_with_relay(schema(), Arc::clone(&cached));
    let hits = || cached.cache().metrics().unwrap().hits;

    let estimate = exec.execute("{ ledgerEstimates { entry { amount } } }", None).await.unwrap();
    assert_eq!(estimate["data"]["ledgerEstimates"][0]["entry"]["amount"], json!(100.0));
    let entries = json!([{ "amount": "100.00" }, null]);
    for (read, hit) in [("first", 0), ("cached", 1)] {
        let response = exec.execute("{ ledgers { entry { amount } } }", None).await.unwrap();
        assert_eq!(json!(column(&response, "ledgers", "entry")), entries, "{read}: {response}");
        assert_eq!(hits(), hit, "{read}: answered from the cache {} times", hits());
    }
}

/// A direct read takes the stored row whole, with no projection, so a read of the rows as a
/// `Float` and a later one as a `Decimal` run the same SQL; only the shape they are decoded
/// with tells their cache entries apart.
#[tokio::test]
async fn a_cached_direct_read_keeps_the_shape_it_was_read_with() {
    if executor().await.is_none() {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    }
    let pg = fraiseql_test_support::postgres().await.unwrap();
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let cache = QueryResultCache::new(CacheConfig::enabled());
    let cached = Arc::new(CachedDatabaseAdapter::new(adapter, cache, "decimal-v1".to_string()));
    let exec = Executor::new_with_relay(schema(), Arc::clone(&cached));
    let matcher = fraiseql_core::runtime::QueryMatcher::new(exec.schema().clone());
    let direct = |document: &'static str| {
        let (exec, matcher) = (&exec, &matcher);
        async move {
            let query_match = matcher.match_query(document, None).unwrap();
            exec.execute_query_direct(&query_match, None, None, None).await.unwrap()
        }
    };

    let estimate = direct("{ ledgerEstimates { entry { amount } } }").await;
    assert_eq!(estimate["data"]["ledgerEstimates"][0]["entry"]["amount"], json!(100.0));
    for read in ["first", "cached"] {
        let response = direct("{ ledgers { entry { amount } } }").await;
        assert_eq!(
            json!(column(&response, "ledgers", "entry")),
            json!([{ "amount": "100.00" }, null]),
            "{read}: {response}"
        );
    }
    assert_eq!(cached.cache().metrics().unwrap().hits, 1, "the second Decimal read is a hit");
}

/// A direct read (REST) answers what the GraphQL read of the same selection answers, key
/// order included, at every depth: a `camelCase` field stored `snake_case` at the root, in
/// a nested object two levels down and in a list of objects holding one, `__typename`
/// below the root, and a leaf selection of an object field.
#[tokio::test]
async fn a_direct_read_answers_what_the_graphql_read_answers() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let matcher = fraiseql_core::runtime::QueryMatcher::new(exec.schema().clone());
    let mut differ = Vec::new();
    for document in [
        "{ customers { id displayName } }",
        "{ customers { displayName homeAddress { postalCode } } }",
        "{ customers { homeAddress { streetLine homeRegion { regionName } postalCode } } }",
        "{ customers { pastOrders { orderTotal shipTo { postalCode } } } }",
        "{ customers { homeAddress { __typename postalCode homeRegion { __typename regionName } } } }",
        "{ customers { pastOrders { __typename shipTo { __typename streetLine } } } }",
    ] {
        let graphql = exec.execute(document, None).await.unwrap();
        let query_match = matcher.match_query(document, None).unwrap();
        let direct = exec.execute_query_direct(&query_match, None, None, None).await;
        match direct {
            Ok(direct) if direct["data"] == graphql["data"] => {},
            Ok(direct) => differ.push(format!(
                "{document}\n    graphql: {}\n    direct:  {}",
                graphql["data"], direct["data"]
            )),
            Err(e) => differ.push(format!("{document}\n    graphql: {graphql}\n    direct: {e}")),
        }
    }

    // A REST leaf selection of an object field reads it whole, as its type, which GraphQL
    // spells as every field in declaration order.
    let mut leaf = matcher
        .match_query("{ customers { id homeAddress { postalCode } } }", None)
        .unwrap();
    let mut roots = leaf.selections.to_vec();
    roots[0].nested_fields[1].nested_fields.clear();
    leaf.selections = roots.into();
    let whole =
        "{ customers { id homeAddress { postalCode streetLine homeRegion { regionName } } } }";
    let graphql = exec.execute(whole, None).await.unwrap();
    match exec.execute_query_direct(&leaf, None, None, None).await {
        Ok(direct) if direct["data"] == graphql["data"] => {},
        Ok(direct) => differ.push(format!(
            "leaf homeAddress\n    graphql: {}\n    direct:  {}",
            graphql["data"], direct["data"]
        )),
        Err(e) => differ.push(format!("leaf homeAddress\n    direct: {e}")),
    }
    assert!(differ.is_empty(), "the two reads differ:\n  {}", differ.join("\n  "));
}
