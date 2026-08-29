//! Argument **values** must have their argument's type — GraphQL § 5.6.1
//! (*Values of Correct Type*), § 5.8.5 (*All Variable Usages Are Allowed*) and
//! § 6.1.2 (*`CoerceVariableValues`*) — against **real PostgreSQL** (#1197).
//!
//! The rule these pin is not "the server should be stricter". It is that a
//! mistyped `limit` used to **widen** the result set: `as_u64()` on a
//! `String`/`Bool`/`Float`/negative/overflowing value returns `None`, the
//! `LIMIT` clause is then simply not emitted, and a request that explicitly
//! asked for 2 rows received the whole table under `exit 0` with no `errors`
//! array. Dropping the clause is the one outcome that must not survive.
//!
//! Every case here is therefore written so the **correct answer differs from
//! the wrong one**: the fixture holds three rows and every pagination case asks
//! for two. A fixture whose scoped and unscoped answers coincide would pass
//! against both implementations and pin nothing.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-core --test argument_value_types_postgres
//! ```
//!
//! Runs in the Dagger `integration: postgres` leg via its `--test '*'` sweep.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_core::{
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    error::FraiseQLError,
    runtime::Executor,
    schema::{ArgumentDefinition, AutoParams, CompiledSchema, FieldType},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

/// Uniquely named so it cannot collide with fixtures other suites share in the
/// same database.
const VIEW: &str = "v_argument_value_types_thing";

/// Three rows, so `limit: 2` has an answer (2) that differs from the answer a
/// dropped `LIMIT` produces (3).
const FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_argument_value_types_thing;
CREATE TABLE v_argument_value_types_thing (pk_thing bigint, data jsonb);
INSERT INTO v_argument_value_types_thing (pk_thing, data) VALUES
  (1, '{"id":"11110000-0000-0000-0000-000000000001","name":"first"}'::jsonb),
  (2, '{"id":"22220000-0000-0000-0000-000000000002","name":"second"}'::jsonb),
  (3, '{"id":"33330000-0000-0000-0000-000000000003","name":"third"}'::jsonb);
"#;

const ROW_COUNT: usize = 3;

fn schema() -> CompiledSchema {
    let thing_type = TestTypeBuilder::new("Thing", VIEW)
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .build();

    let mut things = TestQueryBuilder::new("things", "Thing")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    things.auto_params = AutoParams::all();

    // A single-row query with a declared, typed argument, so a *literal* of the
    // wrong type has somewhere to be caught other than the pagination path.
    let mut thing = TestQueryBuilder::new("thing", "Thing").with_sql_source(VIEW).build();
    thing.arguments = vec![ArgumentDefinition::new("id", FieldType::Uuid)];

    TestSchemaBuilder::new()
        .with_type(thing_type)
        .with_query(things)
        .with_query(thing)
        .build()
}

async fn executor() -> Option<Executor<PostgresAdapter>> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n") {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter.execute_raw_query(stmt).await.expect("provision the #1197 fixture");
    }
    Some(Executor::new(schema(), Arc::new(adapter)))
}

/// What one document actually did: the rows it returned, or the error variant
/// it raised. Both outcomes are observable, which is the point — the defect is
/// that the *rows* branch was reached with the whole table in it.
enum Outcome {
    Rows(usize),
    Validation(String),
    Other(String),
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rows(n) => write!(f, "{n} rows, no error"),
            Self::Validation(m) => write!(f, "Validation({m})"),
            Self::Other(m) => write!(f, "{m}"),
        }
    }
}

async fn run(exec: &Executor<PostgresAdapter>, doc: &str, vars: Option<&Value>) -> Outcome {
    match exec.execute(doc, vars).await {
        Ok(response) => {
            let rows = response["data"]["things"]
                .as_array()
                .map_or_else(|| usize::from(!response["data"]["thing"].is_null()), Vec::len);
            Outcome::Rows(rows)
        },
        Err(FraiseQLError::Validation { message, .. }) => Outcome::Validation(message),
        Err(other) => Outcome::Other(format!("{other:?}")),
    }
}

/// One document whose `limit`/`offset` value is not an `Int`, and which must
/// therefore be **refused** rather than answered with the unpaginated table.
struct Reject {
    name: &'static str,
    doc:  &'static str,
    vars: Option<&'static str>,
}

const MUST_REJECT: &[Reject] = &[
    Reject {
        name: "limit as a string literal",
        doc:  r#"{ things(limit: "2") { name } }"#,
        vars: None,
    },
    Reject {
        name: "limit as a float literal",
        doc:  "{ things(limit: 2.5) { name } }",
        vars: None,
    },
    Reject {
        name: "limit as a boolean literal",
        doc:  "{ things(limit: true) { name } }",
        vars: None,
    },
    Reject {
        name: "limit as a negative literal",
        doc:  "{ things(limit: -1) { name } }",
        vars: None,
    },
    Reject {
        name: "limit beyond u32",
        doc:  "{ things(limit: 99999999999999) { name } }",
        vars: None,
    },
    Reject {
        name: "offset as a string literal",
        doc:  r#"{ things(limit: 2, offset: "1") { name } }"#,
        vars: None,
    },
    Reject {
        name: "limit through a String-declared variable (§ 5.8.5)",
        doc:  "query Q($n: String!) { things(limit: $n) { name } }",
        vars: Some(r#"{"n":"2"}"#),
    },
    Reject {
        name: "limit through a Boolean-declared variable (§ 5.8.5)",
        doc:  "query Q($n: Boolean!) { things(limit: $n) { name } }",
        vars: Some(r#"{"n":true}"#),
    },
    // Not in #1197's table, and the one the § 5.6.1 + § 5.8.5 halves alone do
    // NOT reach: the variable is *declared* `Int!`, so the usage is allowed and
    // only its supplied **value** is wrong (§ 6.1.2).
    Reject {
        name: "limit through an Int-declared variable carrying a string value (§ 6.1.2)",
        doc:  "query Q($n: Int!) { things(limit: $n) { name } }",
        vars: Some(r#"{"n":"2"}"#),
    },
    Reject {
        name: "offset through an Int-declared variable carrying a string value (§ 6.1.2)",
        doc:  "query Q($o: Int!) { things(limit: 2, offset: $o) { name } }",
        vars: Some(r#"{"o":"1"}"#),
    },
    // Also absent from #1197, and the same widening by a third route: the
    // declaration is non-null, no value arrives, the argument is dropped, and
    // the table comes back.
    Reject {
        name: "a non-null variable that is never supplied (§ 6.1.2)",
        doc:  "query Q($n: Int!) { things(limit: $n) { name } }",
        vars: None,
    },
    Reject {
        name: "a non-null variable supplied as null (§ 6.1.2)",
        doc:  "query Q($n: Int!) { things(limit: $n) { name } }",
        vars: Some(r#"{"n":null}"#),
    },
];

#[tokio::test]
async fn a_mistyped_pagination_argument_is_refused_not_dropped() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for case in MUST_REJECT {
        let vars: Option<Value> = case.vars.map(|v| serde_json::from_str(v).unwrap());
        let outcome = run(&exec, case.doc, vars.as_ref()).await;
        match outcome {
            Outcome::Validation(_) => {},
            other => {
                failures.push(format!("  {}: expected a Validation error, got {other}", case.name));
            },
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} mistyped pagination arguments were not refused:\n{}",
        failures.len(),
        MUST_REJECT.len(),
        failures.join("\n")
    );
}

/// The failure that made #1197 HIGH rather than a spec nicety: the refusal must
/// not be reached by *widening*. Every rejected case above, had it been
/// answered, would have returned the whole table.
#[tokio::test]
async fn a_mistyped_limit_never_returns_more_rows_than_it_asked_for() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for case in MUST_REJECT {
        let vars: Option<Value> = case.vars.map(|v| serde_json::from_str(v).unwrap());
        if let Outcome::Rows(n) = run(&exec, case.doc, vars.as_ref()).await {
            if n > 2 {
                failures.push(format!(
                    "  {}: asked for 2 rows, received {n} of {ROW_COUNT} — the bound was dropped",
                    case.name
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} document(s) were answered with an unbounded result set:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The literals and variables that are correct must keep working, and keep
/// producing the *bounded* answer. Without this, "reject everything" would pass
/// the two tests above.
#[tokio::test]
async fn well_typed_pagination_still_paginates() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let two = json!({"n": 2});
    let cases: Vec<(&str, &str, Option<&Value>, usize)> = vec![
        ("integer literal", "{ things(limit: 2) { name } }", None, 2),
        (
            "Int-declared variable with an integer value",
            "query Q($n: Int!) { things(limit: $n) { name } }",
            Some(&two),
            2,
        ),
        // An explicit `null` is a legal value for a nullable argument and means
        // "no limit" — it must stay an answer, not become an error.
        ("explicit null", "{ things(limit: null) { name } }", None, ROW_COUNT),
        // A *declared but unsupplied* variable drops its argument, deliberately
        // and spec-correctly (§ 6.1.2 leaves the argument absent). This is what
        // lets `limit: $limit` fall back to the query's compiled default.
        (
            "declared, unsupplied variable",
            "query Q($n: Int) { things(limit: $n) { name } }",
            None,
            ROW_COUNT,
        ),
        ("well-typed offset", "{ things(limit: 2, offset: 1) { name } }", None, 2),
        ("limit of zero", "{ things(limit: 0) { name } }", None, 0),
    ];

    let mut failures = Vec::new();
    for (name, doc, vars, expected) in cases {
        let outcome = run(&exec, doc, vars).await;
        match outcome {
            Outcome::Rows(n) if n == expected => {},
            other => failures.push(format!("  {name}: expected {expected} rows, got {other}")),
        }
    }

    assert!(
        failures.is_empty(),
        "{} well-typed document(s) regressed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// § 5.6.1 past the pagination path: an `Int` literal written where the schema
/// declares `UUID` used to reach PostgreSQL, which answered with a `Database`
/// error — a 500-shaped response to a client mistake, and the shape whose
/// message can carry a stored value back to the caller.
#[tokio::test]
async fn a_literal_of_the_wrong_type_is_a_validation_error_not_a_database_error() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let outcome = run(&exec, "{ thing(id: 1) { name } }", None).await;
    match outcome {
        Outcome::Validation(message) => {
            assert!(
                message.contains("id"),
                "the error should name the argument; message was: {message}"
            );
        },
        other => panic!("expected a Validation error naming `id`, got {other}"),
    }
}

/// The same mismatch arriving through a variable declared `Int!` where the
/// argument is `UUID` (§ 5.8.5).
#[tokio::test]
async fn a_variable_declared_at_the_wrong_type_is_refused_before_the_database() {
    let Some(exec) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let vars = json!({"id": 1});
    let outcome = run(&exec, "query Q($id: Int!) { thing(id: $id) { name } }", Some(&vars)).await;
    match outcome {
        Outcome::Validation(message) => {
            assert!(
                message.contains("id"),
                "the error should name the argument or variable; message was: {message}"
            );
        },
        other => panic!("expected a Validation error, got {other}"),
    }
}
