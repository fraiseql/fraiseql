//! A mutation argument binds to whatever type its function parameter declares (#1547).
//!
//! The call carries no cast, so PostgreSQL types each `$n` from the function's signature and
//! expects that type's wire form. The arguments were sent as raw UTF-8 in the *binary*
//! format for every type but six, which a `float8` (8 bytes) or a `date` (4 bytes) cannot
//! read: `insufficient data left in message`. Each function here stores what it received;
//! the test reads it back, through a single call and through a batch (`execute_writes`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates its own `p1547` schema → run `--test-threads=1`.

#![cfg(all(feature = "postgres", feature = "test-postgres"))]
#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::fmt::Write as _;

use fraiseql_db::{PostgresAdapter, Writer as _};
use serde_json::{Value, json};

/// `(parameter type, argument as the mutation runner passes it, the value stored, as text)`.
///
/// The arguments are JSON, as a GraphQL request carries them: a list argument is a JSON array,
/// which is not PostgreSQL's array syntax, and the edge values of each type's input function
/// (`NaN`, `infinity`, a zone offset, a zone-less timestamp, more digits than a `float8`) are
/// read by the server exactly as written.
const CASES: [(&str, &str, &str); 31] = [
    ("float8", "9.5", "9.5"),
    ("float4", "0.25", "0.25"),
    (
        "numeric",
        "\"12345678901234567890.123456789\"",
        "12345678901234567890.123456789",
    ),
    (
        "numeric",
        "\"0.1000000000000000055511151231257827\"",
        "0.1000000000000000055511151231257827",
    ),
    ("int2", "7", "7"),
    ("date", "\"2026-10-18\"", "2026-10-18"),
    ("timestamp", "\"2026-10-18T12:34:56.789\"", "2026-10-18 12:34:56.789"),
    ("timestamptz", "\"2026-10-18T12:34:56+00:00\"", "2026-10-18 12:34:56+00"),
    // An offset is honoured, and a zone-less value is read in the session's zone (UTC here).
    ("timestamptz", "\"2026-10-18T12:34:56+02:00\"", "2026-10-18 10:34:56+00"),
    ("timestamptz", "\"2026-10-18T12:34:56\"", "2026-10-18 12:34:56+00"),
    ("interval", "\"1 day 02:03:04\"", "1 day 02:03:04"),
    // The special values each input function accepts.
    ("float8", "\"NaN\"", "NaN"),
    ("float8", "\"-Infinity\"", "-Infinity"),
    ("numeric", "\"NaN\"", "NaN"),
    ("numeric", "\"Infinity\"", "Infinity"),
    ("timestamptz", "\"infinity\"", "infinity"),
    ("date", "\"-infinity\"", "-infinity"),
    // An enum and domains are parsed by their own (or their base type's) input function.
    ("p1547.mood", "\"happy\"", "happy"),
    ("p1547.posint", "7", "7"),
    ("p1547.ratio", "0.25", "0.25"),
    // Arrays: a string already in PostgreSQL's array syntax, and the JSON array a GraphQL
    // list argument is, whatever its items hold.
    ("text[]", "\"{a,b}\"", "{a,b}"),
    (
        "text[]",
        r#"["a,b", "c\"d", "{x}", "back\\slash", "NULL", null, ""]"#,
        r#"{"a,b","c\"d","{x}","back\\slash","NULL",NULL,""}"#,
    ),
    ("text[]", "[]", "{}"),
    ("float8[]", "[1.5, 2.5]", "{1.5,2.5}"),
    ("int4[]", "[1, 2, 3]", "{1,2,3}"),
    ("int4[]", "[[1, 2], [3, 4]]", "{{1,2},{3,4}}"),
    ("bool[]", "[true, false]", "{t,f}"),
    ("date[]", r#"["2026-10-18", null]"#, "{2026-10-18,NULL}"),
    ("p1547.mood[]", r#"["happy", "sad"]"#, "{happy,sad}"),
    ("jsonb[]", r#"[{"a": 1}, [2]]"#, r#"{"{\"a\": 1}",[2]}"#),
    ("p1547.labels", r#"["a", "b"]"#, "{a,b}"),
];

/// The function `p1547.fn_<n>` of each case stores its one argument as text and returns it.
async fn install() -> PostgresAdapter {
    let url = fraiseql_test_support::database_url();
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    let mut ddl = String::from(
        "DROP SCHEMA IF EXISTS p1547 CASCADE; CREATE SCHEMA p1547;
         SET TIME ZONE 'UTC';
         CREATE TYPE p1547.mood AS ENUM ('happy', 'sad');
         CREATE DOMAIN p1547.posint AS int4 CHECK (VALUE > 0);
         CREATE DOMAIN p1547.ratio AS float8;
         CREATE DOMAIN p1547.labels AS text[];
         CREATE TABLE p1547.received (kind text, value text);",
    );
    for (n, (kind, ..)) in CASES.iter().enumerate() {
        writeln!(
            ddl,
            "CREATE FUNCTION p1547.fn_{n}(p {kind}) RETURNS TABLE(value text) LANGUAGE sql AS $$
               INSERT INTO p1547.received VALUES ('{kind}', p::text) RETURNING value $$;"
        )
        .unwrap();
    }
    client.batch_execute(&ddl).await.unwrap();
    PostgresAdapter::with_pool_size(&format!("{url}?options=-c%20TimeZone%3DUTC"), 1)
        .await
        .unwrap()
}

fn argument(raw: &str) -> [Value; 1] {
    [serde_json::from_str(raw).unwrap()]
}

#[tokio::test]
async fn each_parameter_type_binds_through_a_single_call() {
    let adapter = install().await;
    let mut wrong = Vec::new();
    for (n, (kind, raw, stored)) in CASES.iter().enumerate() {
        let function = format!("p1547.fn_{n}");
        let args = argument(raw);
        let request = fraiseql_db::WriteRequest::new(&function, &args);

        match adapter.execute_write(&request, &|_| Ok(())).await {
            Ok(rows) if rows[0]["value"] == json!(stored) => {},
            Ok(rows) => wrong.push(format!("{kind} {raw}: stored {}", rows[0]["value"])),
            Err(e) => wrong.push(format!("{kind} {raw}: {e}")),
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases:\n{}",
        wrong.len(),
        CASES.len(),
        wrong.join("\n")
    );
}

#[tokio::test]
async fn each_parameter_type_binds_through_a_batch() {
    let adapter = install().await;
    let functions: Vec<String> = (0..CASES.len()).map(|n| format!("p1547.fn_{n}")).collect();
    let args: Vec<[Value; 1]> = CASES.iter().map(|(_, raw, _)| argument(raw)).collect();
    let requests: Vec<_> = functions
        .iter()
        .zip(&args)
        .map(|(function, args)| fraiseql_db::WriteRequest::new(function, args))
        .collect();

    let answers = adapter.execute_writes(&requests, &|_, _| Ok(())).await.unwrap();

    let stored: Vec<&Value> = answers.iter().map(|rows| &rows[0]["value"]).collect();
    let expected: Vec<Value> = CASES.iter().map(|(.., stored)| json!(stored)).collect();
    assert_eq!(stored, expected.iter().collect::<Vec<_>>());
}

/// The six types the binding encoded in binary before keep binding, beside text.
#[tokio::test]
async fn the_types_that_bound_before_still_bind() {
    let url = fraiseql_test_support::database_url();
    let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls).await.unwrap();
    tokio::spawn(connection);
    client
        .batch_execute(
            "CREATE SCHEMA IF NOT EXISTS p1547b;
             CREATE OR REPLACE FUNCTION p1547b.fn_all(p_j jsonb, p_js json, p_u uuid, p_i int4,
                 p_l int8, p_b bool, p_t text)
               RETURNS TABLE(value text) LANGUAGE sql AS $$
               SELECT concat_ws('|', p_j::text, p_js::text, p_u::text, p_i::text, p_l::text,
                                p_b::text, p_t) $$;",
        )
        .await
        .unwrap();
    let adapter = PostgresAdapter::with_pool_size(&url, 1).await.unwrap();
    let args = [
        json!({"a": 1}),
        json!([1, 2]),
        json!("00000000-0000-0000-0000-000000001547"),
        json!(42),
        json!(9_007_199_254_740_993_i64),
        json!(true),
        json!("plain"),
    ];
    let request = fraiseql_db::WriteRequest::new("p1547b.fn_all", &args);

    let rows = adapter.execute_write(&request, &|_| Ok(())).await.unwrap();

    assert_eq!(
        rows[0]["value"],
        json!(
            "{\"a\": 1}|[1,2]|00000000-0000-0000-0000-000000001547|42|9007199254740993|true|plain"
        )
    );
}
