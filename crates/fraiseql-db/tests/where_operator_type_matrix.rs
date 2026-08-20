//! Operator × declared-type matrix, executed against a real PostgreSQL.
//!
//! Every WHERE operator is exercised against every scalar type FraiseQL can
//! declare, and each case asserts **both** that the generated SQL executes and
//! that the rows it returns are the right ones. Asserting only "the query did
//! not error" is how `= ANY ARRAY[…]` (#835) and `in: [19.9]` returning zero
//! rows (#800) survived; asserting only the SQL string is how
//! `(data->>'created_at')::numeric` (#798) survived three years of snapshots.
//!
//! The three defects this suite pins:
//!
//! * **#798** — `gt`/`gte`/`lt`/`lte` cast unconditionally to `::numeric`, so every date,
//!   timestamp, UUID and string range filter aborted the statement.
//! * **#800** — `in`/`nin` applied no cast at all, so `in: [19.9]` missed a stored `19.90` that
//!   `eq: 19.9` matched, and the complementary `nin` returned the row the client excluded.
//! * **#828** — the REST bracket operators `ne` and `is_null` parsed nowhere.
//!
//! # Running
//!
//! ```bash
//! DATABASE_URL=postgres://…  cargo test -p fraiseql-db --test where_operator_type_matrix
//! ```
//!
//! Runs in the Dagger `integration: postgres` leg via its `--test '*'` sweep.

#![cfg(feature = "postgres")]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(clippy::print_stderr)] // Reason: skip diagnostic when no backing Postgres

use std::sync::Arc;

use fraiseql_db::{
    ScalarFieldType, WhereClause,
    postgres::PostgresAdapter,
    traits::DatabaseAdapter,
    where_clause::{FieldTypeMap, SharedFieldTypes, WhereFieldSchema},
};
use serde_json::{Value, json};

/// Table the matrix runs against. Uniquely named so it cannot collide with the
/// fixtures other suites share in the same database.
const VIEW: &str = "v_operator_type_matrix";

/// Declared types for every field in the fixture, exactly as the compiled
/// schema would supply them.
fn declared_types() -> SharedFieldTypes {
    Arc::new(FieldTypeMap::from_pairs([
        ("int_val", ScalarFieldType::Integer),
        ("num_val", ScalarFieldType::Numeric),
        ("bool_val", ScalarFieldType::Boolean),
        ("text_val", ScalarFieldType::Text),
        ("uuid_val", ScalarFieldType::Text),
        ("date_val", ScalarFieldType::Date),
        ("ts_val", ScalarFieldType::DateTime),
        ("time_val", ScalarFieldType::Time),
        ("absent", ScalarFieldType::Text),
    ]))
}

/// Three rows chosen so that text ordering and typed ordering **disagree** on
/// `ts_val`: row 1 is `10:00+02:00` (= `08:00Z`), which sorts after row 2's
/// `09:00Z` as text and before it as an instant.
const FIXTURE: &str = r#"
DROP TABLE IF EXISTS v_operator_type_matrix;
CREATE TABLE v_operator_type_matrix (data jsonb);
INSERT INTO v_operator_type_matrix (data) VALUES
  ('{"id":1,"int_val":5,"num_val":19.90,"bool_val":true,"text_val":"alpha",
     "uuid_val":"11111111-1111-1111-1111-111111111111","date_val":"2024-01-01",
     "ts_val":"2024-01-01T10:00:00+02:00","time_val":"08:30:00"}'::jsonb),
  ('{"id":2,"int_val":10,"num_val":24.50,"bool_val":false,"text_val":"mike",
     "uuid_val":"22222222-2222-2222-2222-222222222222","date_val":"2024-06-15",
     "ts_val":"2024-01-01T09:00:00Z","time_val":"12:00:00"}'::jsonb),
  ('{"id":3,"int_val":15,"num_val":100.00,"bool_val":true,"text_val":"zulu",
     "uuid_val":"33333333-3333-3333-3333-333333333333","date_val":"2024-12-31",
     "ts_val":"2024-12-31T23:59:59Z","time_val":"23:59:59"}'::jsonb);
"#;

struct Rig {
    adapter: PostgresAdapter,
    types:   SharedFieldTypes,
}

impl Rig {
    /// Run one `where:` argument through the live path — schema-typed parse,
    /// SQL generation, execution — and return the matching `id`s in order.
    async fn ids_matching(&self, where_json: &Value) -> Result<Vec<i64>, String> {
        let clause = WhereClause::from_graphql_json(
            where_json,
            &WhereFieldSchema::casts_only(Arc::clone(&self.types)),
        )
        .map_err(|e| format!("parse failed: {e}"))?;
        let rows = self
            .adapter
            .execute_where_query(VIEW, Some(&clause), None, None, None)
            .await
            .map_err(|e| format!("execute failed: {e}"))?;
        let mut ids: Vec<i64> = rows
            .iter()
            .map(|r| r.as_value()["id"].as_i64().expect("fixture has id"))
            .collect();
        ids.sort_unstable();
        Ok(ids)
    }
}

async fn rig() -> Option<Rig> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.expect("connect to the bound PostgreSQL");
    for stmt in FIXTURE.split(";\n") {
        if stmt.trim().is_empty() {
            continue;
        }
        adapter.execute_raw_query(stmt).await.expect("provision the matrix fixture");
    }
    Some(Rig {
        adapter,
        types: declared_types(),
    })
}

/// One row of the matrix: a `where:` argument and the exact `id`s it must
/// return. `expect` is the *row set*, not merely "no error" — a case whose SQL
/// executes but silently under-matches is the #800 defect.
struct Case {
    name:   &'static str,
    filter: fn() -> Value,
    expect: &'static [i64],
}

const MATRIX: &[Case] = &[
    // ── Integer ───────────────────────────────────────────────────────────
    Case {
        name:   "int eq",
        filter: || json!({"intVal": {"eq": 10}}),
        expect: &[2],
    },
    Case {
        name:   "int neq",
        filter: || json!({"intVal": {"neq": 10}}),
        expect: &[1, 3],
    },
    Case {
        name:   "int gt",
        filter: || json!({"intVal": {"gt": 5}}),
        expect: &[2, 3],
    },
    Case {
        name:   "int gte",
        filter: || json!({"intVal": {"gte": 10}}),
        expect: &[2, 3],
    },
    Case {
        name:   "int lt",
        filter: || json!({"intVal": {"lt": 10}}),
        expect: &[1],
    },
    Case {
        name:   "int lte",
        filter: || json!({"intVal": {"lte": 10}}),
        expect: &[1, 2],
    },
    Case {
        name:   "int in",
        filter: || json!({"intVal": {"in": [5, 15]}}),
        expect: &[1, 3],
    },
    Case {
        name:   "int nin",
        filter: || json!({"intVal": {"nin": [5]}}),
        expect: &[2, 3],
    },
    Case {
        name:   "int in empty",
        filter: || json!({"intVal": {"in": []}}),
        expect: &[],
    },
    Case {
        name:   "int nin empty",
        filter: || json!({"intVal": {"nin": []}}),
        expect: &[1, 2, 3],
    },
    // ── Numeric — #800: 19.90 is stored, 19.9 is asked for ────────────────
    Case {
        name:   "num eq trailing zero",
        filter: || json!({"numVal": {"eq": 19.9}}),
        expect: &[1],
    },
    Case {
        name:   "num in trailing zero",
        filter: || json!({"numVal": {"in": [19.9]}}),
        expect: &[1],
    },
    Case {
        name:   "num nin trailing zero",
        filter: || json!({"numVal": {"nin": [19.9]}}),
        expect: &[2, 3],
    },
    Case {
        name:   "num gte",
        filter: || json!({"numVal": {"gte": 24.5}}),
        expect: &[2, 3],
    },
    Case {
        name:   "num lt",
        filter: || json!({"numVal": {"lt": 24.5}}),
        expect: &[1],
    },
    // ── Boolean ───────────────────────────────────────────────────────────
    Case {
        name:   "bool eq true",
        filter: || json!({"boolVal": {"eq": true}}),
        expect: &[1, 3],
    },
    Case {
        name:   "bool eq false",
        filter: || json!({"boolVal": {"eq": false}}),
        expect: &[2],
    },
    Case {
        name:   "bool neq",
        filter: || json!({"boolVal": {"neq": true}}),
        expect: &[2],
    },
    Case {
        name:   "bool in",
        filter: || json!({"boolVal": {"in": [true]}}),
        expect: &[1, 3],
    },
    // ── Text — #798: a string range was a hard cast error ─────────────────
    Case {
        name:   "text eq",
        filter: || json!({"textVal": {"eq": "mike"}}),
        expect: &[2],
    },
    Case {
        name:   "text gte",
        filter: || json!({"textVal": {"gte": "m"}}),
        expect: &[2, 3],
    },
    Case {
        name:   "text lt",
        filter: || json!({"textVal": {"lt": "m"}}),
        expect: &[1],
    },
    Case {
        name:   "text in",
        filter: || json!({"textVal": {"in": ["alpha", "zulu"]}}),
        expect: &[1, 3],
    },
    Case {
        name:   "text contains",
        filter: || json!({"textVal": {"contains": "ik"}}),
        expect: &[2],
    },
    Case {
        name:   "text startswith",
        filter: || json!({"textVal": {"startswith": "al"}}),
        expect: &[1],
    },
    Case {
        name:   "text endswith",
        filter: || json!({"textVal": {"endswith": "lu"}}),
        expect: &[3],
    },
    Case {
        name:   "text icontains",
        filter: || json!({"textVal": {"icontains": "IK"}}),
        expect: &[2],
    },
    // ── UUID (declared Text) — #798: a UUID range was a hard cast error ───
    Case {
        name:   "uuid gt",
        filter: || json!({"uuidVal": {"gt": "22222222-2222-2222-2222-222222222222"}}),
        expect: &[3],
    },
    Case {
        name:   "uuid lte",
        filter: || json!({"uuidVal": {"lte": "22222222-2222-2222-2222-222222222222"}}),
        expect: &[1, 2],
    },
    // ── Date — #798: the single most common GraphQL filter ────────────────
    Case {
        name:   "date gte",
        filter: || json!({"dateVal": {"gte": "2024-06-15"}}),
        expect: &[2, 3],
    },
    Case {
        name:   "date lt",
        filter: || json!({"dateVal": {"lt": "2024-06-15"}}),
        expect: &[1],
    },
    Case {
        name:   "date eq",
        filter: || json!({"dateVal": {"eq": "2024-01-01"}}),
        expect: &[1],
    },
    // ── DateTime — the case where text ordering is WRONG ──────────────────
    // Row 1 is 10:00+02:00 = 08:00Z, which is *before* row 2's 09:00Z but
    // sorts *after* it as text. Only a typed comparison answers this correctly.
    Case {
        name:   "datetime lt across offsets",
        filter: || json!({"tsVal": {"lt": "2024-01-01T09:00:00Z"}}),
        expect: &[1],
    },
    Case {
        name:   "datetime gte across offsets",
        filter: || json!({"tsVal": {"gte": "2024-01-01T09:00:00Z"}}),
        expect: &[2, 3],
    },
    Case {
        name:   "datetime eq across offsets",
        filter: || json!({"tsVal": {"eq": "2024-01-01T08:00:00Z"}}),
        expect: &[1],
    },
    // ── Time ──────────────────────────────────────────────────────────────
    Case {
        name:   "time gte",
        filter: || json!({"timeVal": {"gte": "12:00:00"}}),
        expect: &[2, 3],
    },
    Case {
        name:   "time lt",
        filter: || json!({"timeVal": {"lt": "12:00:00"}}),
        expect: &[1],
    },
    // ── NULL checks — #828 exposes `is_null` through REST ─────────────────
    Case {
        name:   "isnull absent",
        filter: || json!({"absent": {"is_null": true}}),
        expect: &[1, 2, 3],
    },
    Case {
        name:   "isnull present",
        filter: || json!({"textVal": {"is_null": true}}),
        expect: &[],
    },
    Case {
        name:   "is_not_null present",
        filter: || json!({"textVal": {"is_not_null": true}}),
        expect: &[1, 2, 3],
    },
    Case {
        name:   "ne alias",
        filter: || json!({"intVal": {"ne": 10}}),
        expect: &[1, 3],
    },
    // ── Logical composition ───────────────────────────────────────────────
    Case {
        name:   "and across types",
        filter: || json!({"boolVal": {"eq": true}, "dateVal": {"gte": "2024-06-15"}}),
        expect: &[3],
    },
    Case {
        name:   "or across types",
        filter: || json!({"_or": [{"intVal": {"eq": 5}}, {"dateVal": {"gte": "2024-12-01"}}]}),
        expect: &[1, 3],
    },
    Case {
        name:   "not",
        filter: || json!({"_not": {"boolVal": {"eq": true}}}),
        expect: &[2],
    },
];

#[tokio::test]
async fn operator_type_matrix_executes_and_returns_the_right_rows() {
    let Some(rig) = rig().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let mut failures = Vec::new();
    for case in MATRIX {
        let filter = (case.filter)();
        match rig.ids_matching(&filter).await {
            Ok(ids) if ids == case.expect => {},
            Ok(ids) => failures.push(format!(
                "{}: filter {filter} returned {ids:?}, expected {:?}",
                case.name, case.expect
            )),
            Err(e) => failures.push(format!("{}: filter {filter} — {e}", case.name)),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} matrix cases failed:\n  {}",
        failures.len(),
        MATRIX.len(),
        failures.join("\n  ")
    );
}

/// `eq: X` and `in: [X]` must select the same rows for every type.
///
/// This is #800 stated as an invariant rather than as a list of cases: the two
/// operators were rendered by different code paths, and the one without a cast
/// silently under-matched. A single shared cast decision makes them agree by
/// construction; this asserts it against the database rather than the source.
#[tokio::test]
async fn eq_and_single_element_in_agree_for_every_type() {
    let Some(rig) = rig().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let probes: &[(&str, Value)] = &[
        ("intVal", json!(10)),
        ("numVal", json!(19.9)),
        ("boolVal", json!(true)),
        ("textVal", json!("mike")),
        ("uuidVal", json!("22222222-2222-2222-2222-222222222222")),
        ("dateVal", json!("2024-06-15")),
        ("tsVal", json!("2024-01-01T08:00:00Z")),
        ("timeVal", json!("12:00:00")),
    ];

    for (field, value) in probes {
        let eq = rig.ids_matching(&json!({ *field: { "eq": value } })).await;
        let inn = rig.ids_matching(&json!({ *field: { "in": [value] } })).await;
        assert_eq!(eq, inn, "eq vs in disagree on {field} = {value}");
        assert!(
            matches!(&eq, Ok(ids) if !ids.is_empty()),
            "{field} = {value} matched nothing, so the comparison proves nothing: {eq:?}"
        );
    }
}

/// `nin: [X]` must return exactly the complement of `in: [X]`.
///
/// An under-matching `IN` becomes an *over*-matching `NOT IN` — the shape that
/// made #800 return rows the client had explicitly excluded.
#[tokio::test]
async fn nin_is_the_complement_of_in_for_every_type() {
    let Some(rig) = rig().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let probes: &[(&str, Value)] = &[
        ("intVal", json!(10)),
        ("numVal", json!(19.9)),
        ("textVal", json!("mike")),
        ("dateVal", json!("2024-06-15")),
        ("tsVal", json!("2024-01-01T08:00:00Z")),
    ];

    for (field, value) in probes {
        let inside = rig.ids_matching(&json!({ *field: { "in": [value] } })).await.unwrap();
        let outside = rig.ids_matching(&json!({ *field: { "nin": [value] } })).await.unwrap();
        let mut union: Vec<i64> = inside.iter().chain(outside.iter()).copied().collect();
        union.sort_unstable();
        assert_eq!(
            union,
            vec![1, 2, 3],
            "in/nin on {field} = {value} are not complementary: in={inside:?} nin={outside:?}"
        );
    }
}

/// A LIKE needle containing wildcards matches them literally.
///
/// `escape_like_literal` neutralises `%`, `_` and `\`; PostgreSQL's LIKE
/// defaults to `\` as the escape character, so no `ESCAPE` clause is needed
/// here — SQLite and SQL Server, which have no default, get one from
/// `SqlDialect::like_escape_clause` (#722).
#[tokio::test]
async fn like_metacharacters_in_the_needle_are_matched_literally() {
    let Some(rig) = rig().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    rig.adapter
        .execute_raw_query(
            r#"INSERT INTO v_operator_type_matrix (data) VALUES
                 ('{"id":90,"text_val":"100% pure"}'::jsonb),
                 ('{"id":91,"text_val":"100X pure"}'::jsonb),
                 ('{"id":92,"text_val":"a_b"}'::jsonb),
                 ('{"id":93,"text_val":"axb"}'::jsonb);"#,
        )
        .await
        .unwrap();

    assert_eq!(
        rig.ids_matching(&json!({"textVal": {"contains": "100%"}})).await.unwrap(),
        vec![90],
        "`%` in the needle must match a literal percent, not any-sequence"
    );
    assert_eq!(
        rig.ids_matching(&json!({"textVal": {"contains": "a_b"}})).await.unwrap(),
        vec![92],
        "`_` in the needle must match a literal underscore, not any-character"
    );
}

/// The raw-SQL wire generator's output is accepted by a real PostgreSQL parser.
///
/// `WhereSqlGenerator` assembles SQL as a string rather than as parameters, so
/// nothing downstream re-parses it before the server does. #835 shipped
/// `= ANY ARRAY[…]` — which does not parse — with a unit test asserting that
/// exact string, and a numeric `eq` that produced `text = integer`. A snapshot
/// of invalid SQL is indistinguishable from a snapshot of valid SQL until
/// something tries to run it, so this parses every shape through the server.
#[tokio::test]
async fn wire_generator_output_is_valid_postgresql() {
    let Some(rig) = rig().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };

    let cases: &[(&str, Value)] = &[
        ("in list of strings", json!({"textVal": {"in": ["alpha", "zulu"]}})),
        ("nin list of strings", json!({"textVal": {"nin": ["alpha"]}})),
        ("in list of numbers", json!({"intVal": {"in": [5, 15]}})),
        ("in empty list", json!({"intVal": {"in": []}})),
        ("nin empty list", json!({"intVal": {"nin": []}})),
        ("numeric eq", json!({"numVal": {"eq": 19.9}})),
        ("numeric gt", json!({"intVal": {"gt": 5}})),
        ("boolean eq", json!({"boolVal": {"eq": true}})),
        ("string eq", json!({"textVal": {"eq": "mike"}})),
        ("date gte", json!({"dateVal": {"gte": "2024-06-15"}})),
        ("is null", json!({"absent": {"is_null": true}})),
        ("is not null", json!({"textVal": {"is_not_null": true}})),
        ("and", json!({"intVal": {"gt": 5}, "boolVal": {"eq": true}})),
        ("or", json!({"_or": [{"intVal": {"eq": 5}}, {"intVal": {"eq": 15}}]})),
        ("not", json!({"_not": {"boolVal": {"eq": true}}})),
        ("quote in value", json!({"textVal": {"eq": "o'brien"}})),
    ];

    let mut failures = Vec::new();
    for (name, filter) in cases {
        let clause = WhereClause::from_graphql_json(
            filter,
            &WhereFieldSchema::casts_only(Arc::clone(&rig.types)),
        )
        .expect("parses");
        let predicate = match fraiseql_db::where_sql_generator::WhereSqlGenerator::to_sql(&clause) {
            Ok(sql) => sql,
            Err(e) => {
                failures.push(format!("{name}: generation failed — {e}"));
                continue;
            },
        };
        // Running it is a parse, a plan and a type check in one: it rejects the
        // syntax error and the operator/type mismatch #835 reports.
        let sql = format!("SELECT count(*) FROM {VIEW} WHERE {predicate}");
        if let Err(e) = rig.adapter.execute_raw_query(&sql).await {
            failures.push(format!("{name}: PostgreSQL rejected `{predicate}` — {e}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} wire-generator shapes are not valid PostgreSQL:\n  {}",
        failures.len(),
        cases.len(),
        failures.join("\n  ")
    );
}
