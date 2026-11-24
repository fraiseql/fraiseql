//! The arguments of an aggregate or window root are read as a list query's are.
//!
//! An explicit `null` is the argument's absence. A value of the published shape is read.
//! An argument whose published type is a list takes one object as a one-item list (GraphQL
//! input coercion). Anything else is refused, naming the argument, before the database is
//! read. A value of another shape was read as the argument's absence, and the response
//! answered as if filtered, grouped or ordered when it was not.
//!
//! Every case is asked on each entry that reaches the parsers: written inline, as a
//! variable named like the argument, as a declared `$variable`, and through the embedder
//! entries, which skip the document entirely.

#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

use std::sync::Arc;

use serde_json::{Value, json};

use super::aggregate_rls_tests::{schema_with_fact_table, tenant_security_context};
use crate::{
    error::{FraiseQLError, Result},
    runtime::{Executor, executor::test_support::CapturingMockAdapter},
};

/// The aggregate root's other arguments, so the one under test is the only variable.
const AGG: &[(&str, &str)] = &[
    ("groupBy", "{tenant_id: true}"),
    ("aggregates", "[{count: true}]"),
];
/// The window root's.
const WIN: &[(&str, &str)] = &[
    ("select", r#"[{type: "filter", name: "tenant_id", alias: "tenant_id"}]"#),
    ("windows", r#"[{function: {type: "row_number"}, alias: "rank"}]"#),
];

#[derive(Clone, Copy, Debug)]
enum Root {
    Aggregate,
    Window,
}

impl Root {
    const fn name(self) -> &'static str {
        match self {
            Self::Aggregate => "sales_aggregate",
            Self::Window => "sales_window",
        }
    }

    const fn base(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Aggregate => AGG,
            Self::Window => WIN,
        }
    }

    /// The root's document, with `argument` written as `literal` (replacing a base argument
    /// of the same name), or left out when `literal` is `None`.
    fn document(self, argument: &str, literal: Option<&str>) -> String {
        let mut args: Vec<String> = self
            .base()
            .iter()
            .filter(|(name, _)| *name != argument)
            .map(|(name, value)| format!("{name}: {value}"))
            .collect();
        if let Some(literal) = literal {
            args.push(format!("{argument}: {literal}"));
        }
        let selection = match self {
            Self::Aggregate => " { count }",
            Self::Window => "",
        };
        format!("{{ {}({}){selection} }}", self.name(), args.join(", "))
    }

    /// The embedder request: the base arguments as JSON, with `argument` set to `value`.
    fn request(self, argument: &str, value: Option<&Value>) -> Value {
        let mut request = match self {
            Self::Aggregate => {
                json!({"groupBy": {"tenant_id": true}, "aggregates": [{"count": true}]})
            },
            Self::Window => json!({
                "select": [{"type": "filter", "name": "tenant_id", "alias": "tenant_id"}],
                "windows": [{"function": {"type": "row_number"}, "alias": "rank"}]
            }),
        };
        let map = request.as_object_mut().unwrap();
        map.remove(argument);
        if let Some(value) = value {
            map.insert(argument.to_string(), value.clone());
        }
        request
    }
}

/// The outcome of one request and the statement it sent, if any.
type Outcome = (Result<Value>, Option<String>);

/// The fixture's fact table with its roots published, as every loaded schema has them: a
/// schema without them is one no server serves.
fn published() -> crate::schema::CompiledSchema {
    let mut schema = schema_with_fact_table();
    schema.build_indexes();
    schema
}

async fn by_document(document: &str, variables: &Value) -> Outcome {
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let res = Executor::new(published(), adapter.clone())
        .execute_with_security(document, Some(variables), &tenant_security_context("tenant-abc"))
        .await;
    (res, adapter.captured_aggregate_sql())
}

async fn by_embedder(root: Root, request: &Value) -> Outcome {
    let schema = published();
    let metadata = schema.get_fact_table("tf_sales").unwrap().clone();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let executor = Executor::new(schema, adapter.clone());
    let res = match root {
        Root::Aggregate => executor.execute_aggregate_query(request, root.name(), &metadata).await,
        Root::Window => executor.execute_window_query(request, root.name(), &metadata).await,
    };
    (res, adapter.captured_aggregate_sql())
}

/// `argument` set to `value` (written inline as `literal`), on every entry: inline, as a
/// variable named like the argument, and through the embedder entry.
async fn every_entry(
    root: Root,
    argument: &str,
    literal: &str,
    value: &Value,
) -> Vec<(String, Outcome)> {
    vec![
        (
            format!("{} inline {argument}: {literal}", root.name()),
            by_document(&root.document(argument, Some(literal)), &json!({})).await,
        ),
        (
            format!("{} variable {argument}: {value}", root.name()),
            by_document(&root.document(argument, None), &json!({ argument: value })).await,
        ),
        (
            format!("{} embedder {argument}: {value}", root.name()),
            by_embedder(root, &root.request(argument, Some(value))).await,
        ),
    ]
}

/// The refusal's message and path, or a panic naming what came back instead.
fn refusal(case: &str, outcome: &Outcome) -> (String, Option<String>) {
    let (res, sql) = outcome;
    assert!(sql.is_none(), "{case}: the statement reached the database: {sql:?}");
    match res {
        Err(FraiseQLError::Validation { message, path }) => (message.clone(), path.clone()),
        other => panic!("{case}: not refused: {other:?}"),
    }
}

// ── Shape: a value that is neither `null` nor the published shape ────────────

/// Each argument, the message its refusal carries, and values of another shape, each
/// as a GraphQL literal and as JSON.
fn wrong_shapes() -> Vec<(Root, &'static str, &'static str, Vec<(&'static str, Value)>)> {
    let scalars = || {
        vec![
            (r#""tenant_id""#, json!("tenant_id")),
            ("7", json!(7)),
            ("true", json!(true)),
        ]
    };
    let scalars_and_list = || {
        let mut shapes = scalars();
        shapes.push(("[]", json!([])));
        shapes
    };
    vec![
        (
            Root::Aggregate,
            "where",
            "where clause must be a JSON object",
            scalars_and_list(),
        ),
        (Root::Aggregate, "groupBy", "groupBy must be an object", scalars_and_list()),
        (
            Root::Aggregate,
            "aggregates",
            "aggregates must be an object or array",
            scalars(),
        ),
        (Root::Aggregate, "having", "having must be an object", scalars_and_list()),
        (Root::Aggregate, "orderBy", "orderBy must be an object or array", scalars()),
        (Root::Window, "select", "select must be an object or array", scalars()),
        (Root::Window, "windows", "windows must be an object or array", scalars()),
        (Root::Window, "where", "where clause must be a JSON object", scalars_and_list()),
        (Root::Window, "orderBy", "orderBy must be an object or array", scalars()),
    ]
}

#[tokio::test]
async fn a_root_argument_of_another_shape_is_refused_naming_it_on_every_entry() {
    for (root, argument, expected, shapes) in wrong_shapes() {
        for (literal, value) in shapes {
            for (case, outcome) in every_entry(root, argument, literal, &value).await {
                let (message, path) = refusal(&case, &outcome);
                assert_eq!(message, expected, "{case}");
                assert_eq!(path.as_deref(), Some(argument), "{case}");
            }
        }
    }
}

/// A declared `$variable` reaches the same parser: the document validator does not hold
/// a fact-table root's variables to their declared type.
#[tokio::test]
async fn a_declared_variable_of_another_shape_is_refused() {
    let mut schema = schema_with_fact_table();
    schema.build_indexes();
    let adapter = Arc::new(CapturingMockAdapter::new(vec![]));
    let res = Executor::new(schema, adapter.clone())
        .execute_with_security(
            "query Q($w: SalesAggregateWhere) { sales_aggregate(where: $w, groupBy: \
             {tenant_id: true}, aggregates: [{count: true}]) { count } }",
            Some(&json!({"w": 7})),
            &tenant_security_context("tenant-abc"),
        )
        .await;
    let (message, _) = refusal("declared $w: 7", &(res, adapter.captured_aggregate_sql()));
    assert!(
        message == "where clause must be a JSON object" || message.contains("$w"),
        "refused by the parser, or by the validator naming the variable: {message}"
    );
}

/// The list query's refusal of the same `where` value carries the text the analytics roots
/// are held to above: the same question, the same message.
#[test]
fn the_where_refusal_is_the_list_querys() {
    let list = fraiseql_db::WhereClause::from_graphql_json(
        &json!(7),
        &fraiseql_db::where_clause::WhereFieldSchema::default(),
    )
    .expect_err("a list query refuses it");
    assert!(
        matches!(&list, FraiseQLError::Validation { message, .. } if message == "where clause must be a JSON object"),
        "{list:?}"
    );
}

/// A non-object item of a list argument is refused, naming the argument.
#[tokio::test]
async fn a_list_item_that_is_not_an_object_is_refused() {
    for (root, argument, literal, value) in [
        (Root::Aggregate, "aggregates", r#"["count"]"#, json!(["count"])),
        (Root::Window, "select", "[7]", json!([7])),
        (Root::Window, "windows", "[7]", json!([7])),
        (Root::Window, "orderBy", "[7]", json!([7])),
    ] {
        for (case, outcome) in every_entry(root, argument, literal, &value).await {
            let (message, _) = refusal(&case, &outcome);
            assert!(message.contains(argument) || message.contains("Missing"), "{case}: {message}");
        }
    }
}

/// A window function's own `partitionBy` and `orderBy` follow the rule too.
#[tokio::test]
async fn a_window_functions_partition_and_order_of_another_shape_are_refused() {
    for (key, expected) in [
        ("partitionBy", "partitionBy must be an object or array"),
        ("orderBy", "orderBy must be an object or array"),
    ] {
        for (literal, value) in [(r#""tenant_id""#, json!("tenant_id")), ("7", json!(7))] {
            let windows_literal = format!(
                r#"[{{function: {{type: "row_number"}}, alias: "rank", {key}: {literal}}}]"#
            );
            let windows =
                json!([{"function": {"type": "row_number"}, "alias": "rank", key: value}]);
            for (case, outcome) in
                every_entry(Root::Window, "windows", &windows_literal, &windows).await
            {
                let (message, path) = refusal(&case, &outcome);
                assert_eq!(message, expected, "{case}");
                assert_eq!(path.as_deref(), Some(format!("windows.{key}").as_str()), "{case}");
            }
        }
    }
}

// ── `null` is absent ─────────────────────────────────────────────────────────

/// An explicit `null` sends the statement the argument's absence sends (#1745).
#[tokio::test]
async fn a_null_argument_reads_as_its_absence() {
    for (root, argument) in [
        (Root::Aggregate, "where"),
        (Root::Aggregate, "groupBy"),
        (Root::Aggregate, "having"),
        (Root::Aggregate, "orderBy"),
        (Root::Window, "where"),
        (Root::Window, "orderBy"),
        (Root::Window, "select"),
    ] {
        let (absent, absent_sql) = by_document(&root.document(argument, None), &json!({})).await;
        assert!(absent.is_ok(), "{root:?} {argument} absent: {absent:?}");
        let absent_sql = absent_sql.expect("the absent case reaches the database");
        for (case, (res, sql)) in every_entry(root, argument, "null", &Value::Null).await {
            assert!(res.is_ok(), "{case}: {res:?}");
            assert_eq!(sql.as_deref(), Some(absent_sql.as_str()), "{case}");
        }
    }
    // A window function's own `partitionBy`, `orderBy` and `frame`, each `null`.
    let plain = r#"[{function: {type: "row_number"}, alias: "rank"}]"#;
    let (_, plain_sql) =
        by_document(&Root::Window.document("windows", Some(plain)), &json!({})).await;
    for key in ["partitionBy", "orderBy", "frame"] {
        let literal =
            format!(r#"[{{function: {{type: "row_number"}}, alias: "rank", {key}: null}}]"#);
        let value = json!([{"function": {"type": "row_number"}, "alias": "rank", key: null}]);
        for (case, (res, sql)) in every_entry(Root::Window, "windows", &literal, &value).await {
            assert!(res.is_ok(), "{case}: {res:?}");
            assert_eq!(sql, plain_sql, "{case}");
        }
    }
}

// ── A list type takes one object as a one-item list ──────────────────────────

/// One object sends the statement its one-item list sends, and that statement applies it.
async fn assert_coerced(root: Root, argument: &str, object: (&str, Value), applied: &str) {
    let (list_literal, list_value) = (format!("[{}]", object.0), json!([object.1.clone()]));
    let listed = every_entry(root, argument, &list_literal, &list_value).await;
    let coerced = every_entry(root, argument, object.0, &object.1).await;
    for ((list_case, (list_res, list_sql)), (case, (res, sql))) in listed.into_iter().zip(coerced) {
        assert!(list_res.is_ok(), "{list_case}: {list_res:?}");
        let list_sql = list_sql.unwrap_or_else(|| panic!("{list_case}: no statement"));
        assert!(list_sql.contains(applied), "{list_case}: `{applied}` not in {list_sql}");
        assert!(res.is_ok(), "{case}: {res:?}");
        assert_eq!(sql.as_deref(), Some(list_sql.as_str()), "{case}");
    }
}

#[tokio::test]
async fn a_window_order_by_of_one_object_is_a_one_item_list() {
    assert_coerced(
        Root::Window,
        "orderBy",
        (
            r#"{field: "tenant_id", direction: DESC}"#,
            json!({"field": "tenant_id", "direction": "DESC"}),
        ),
        "DESC",
    )
    .await;
}

#[tokio::test]
async fn a_window_functions_order_and_partition_of_one_object_are_one_item_lists() {
    for (key, object, value, applied) in [
        (
            "orderBy",
            r#"{field: "tenant_id", direction: DESC}"#,
            json!({"field": "tenant_id", "direction": "DESC"}),
            "DESC",
        ),
        (
            "partitionBy",
            r#"{type: "filter", name: "tenant_id"}"#,
            json!({"type": "filter", "name": "tenant_id"}),
            "PARTITION BY",
        ),
    ] {
        let spec = |inner: &str| {
            format!(r#"{{function: {{type: "row_number"}}, alias: "rank", {key}: {inner}}}"#)
        };
        let spec_value =
            |inner: Value| json!({"function": {"type": "row_number"}, "alias": "rank", key: inner});
        // The window spec is itself written as a list, so only its own key is coerced.
        let listed = every_entry(
            Root::Window,
            "windows",
            &format!("[{}]", spec(&format!("[{object}]"))),
            &json!([spec_value(json!([value.clone()]))]),
        )
        .await;
        let coerced = every_entry(
            Root::Window,
            "windows",
            &format!("[{}]", spec(object)),
            &json!([spec_value(value.clone())]),
        )
        .await;
        for ((list_case, (list_res, list_sql)), (case, (res, sql))) in
            listed.into_iter().zip(coerced)
        {
            assert!(list_res.is_ok(), "{list_case}: {list_res:?}");
            let list_sql = list_sql.unwrap();
            assert!(list_sql.contains(applied), "{list_case}: `{applied}` not in {list_sql}");
            assert!(res.is_ok(), "{case}: {res:?}");
            assert_eq!(sql.as_deref(), Some(list_sql.as_str()), "{case}");
        }
    }
}

#[tokio::test]
async fn a_list_argument_of_one_object_is_a_one_item_list() {
    assert_coerced(
        Root::Aggregate,
        "aggregates",
        ("{count: true}", json!({"count": true})),
        "COUNT(",
    )
    .await;
    assert_coerced(
        Root::Window,
        "select",
        (
            r#"{type: "filter", name: "tenant_id", alias: "tenant_id"}"#,
            json!({"type": "filter", "name": "tenant_id", "alias": "tenant_id"}),
        ),
        "tenant_id",
    )
    .await;
    assert_coerced(
        Root::Window,
        "windows",
        (
            r#"{function: {type: "row_number"}, alias: "rank"}"#,
            json!({"function": {"type": "row_number"}, "alias": "rank"}),
        ),
        "ROW_NUMBER()",
    )
    .await;
}

// ── Key level: a key the parser cannot place ─────────────────────────────────

/// A key of the right container that names nothing the root can read is refused, naming
/// the key, rather than skipped. A value at an enum position is the published enum's to
/// refuse, as a list root's is, never quoting it: `enum_tests`.
#[tokio::test]
async fn a_key_the_root_cannot_read_is_refused_naming_it() {
    let mut failures = Vec::new();
    for (root, argument, literal, value, key) in [
        (Root::Aggregate, "having", "{bogus: 5}", json!({"bogus": 5}), "bogus"),
        (Root::Aggregate, "having", "{count_zz: 5}", json!({"count_zz": 5}), "count_zz"),
        (
            Root::Aggregate,
            "where",
            r#"{tenant_id_zz: "a"}"#,
            json!({"tenant_id_zz": "a"}),
            "tenant_id_zz",
        ),
        (
            Root::Window,
            "where",
            r#"{tenant_id_zz: "a"}"#,
            json!({"tenant_id_zz": "a"}),
            "tenant_id_zz",
        ),
        (
            Root::Aggregate,
            "aggregates",
            "[{bogus: true}]",
            json!([{"bogus": true}]),
            "bogus",
        ),
        (
            Root::Window,
            "orderBy",
            r#"[{field: "bogus"}]"#,
            json!([{"field": "bogus"}]),
            "bogus",
        ),
        (
            Root::Window,
            "select",
            r#"[{type: "measure", name: "bogus", alias: "b"}]"#,
            json!([{"type": "measure", "name": "bogus", "alias": "b"}]),
            "bogus",
        ),
        (
            Root::Window,
            "windows",
            r#"[{function: {type: "row_number"}, alias: "rank", partitionBy: [{type: "measure", name: "bogus"}]}]"#,
            json!([{"function": {"type": "row_number"}, "alias": "rank", "partitionBy": [{"type": "measure", "name": "bogus"}]}]),
            "bogus",
        ),
    ] {
        for (case, (res, sql)) in every_entry(root, argument, literal, &value).await {
            match (res, sql) {
                (Err(e), None) if e.to_string().contains(key) => {},
                (Err(e), None) => failures.push(format!("{case}: does not name `{key}`: {e}")),
                (res, sql) => failures.push(format!("{case}: not refused: {res:?} {sql:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[path = "aggregate_argument_enum_tests.rs"]
mod enum_tests;
