//! An inline fragment with no type condition, inside a field, executed against PostgreSQL.
//!
//! `... { name }` and `... @include(if: $on) { name }` select on the enclosing type
//! (GraphQL § 2.8.2): their fields are the field's own, under the fragment's directives.
//! Each form is served as the same document with the fragment written out, through every
//! entry a document reaches the executor by.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `uif_*` fixtures → run `--test-threads=1`.
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    chrono::Utc,
    compiler::fact_table::{
        DimensionColumn, FactRowScope, FactTableMetadata, FilterColumn, MeasureColumn, SqlType,
    },
    db::{postgres::PostgresAdapter, traits::DatabaseAdapter},
    runtime::{Executor, relay::encode_node_id},
    schema::{CompiledSchema, FieldDefinition, FieldType, InterfaceDefinition},
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::{Value, json};

const VIEW: &str = "v_uif_user";
const FACT: &str = "tf_uif_sale";
const ALICE: &str = "aaaa0000-0000-0000-0000-000000000001";

fn schema() -> CompiledSchema {
    let user = TestTypeBuilder::new("User", VIEW)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Uuid)
        .with_simple_field("name", FieldType::String)
        .with_simple_field("email", FieldType::String)
        .build();
    let users = TestQueryBuilder::new("users", "User")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    let mut schema = TestSchemaBuilder::new().with_type(user).with_query(users).build();
    schema.interfaces.push(
        InterfaceDefinition::new("Node").with_field(FieldDefinition::new("id", FieldType::Id)),
    );
    schema.add_fact_table(
        FACT.to_string(),
        FactTableMetadata {
            table_name:               FACT.to_string(),
            type_name:                None,
            row_scope:                FactRowScope::Server,
            inject_params:            indexmap::IndexMap::default(),
            measures:                 vec![MeasureColumn {
                name:       "amount".to_string(),
                sql_type:   SqlType::BigInt,
                nullable:   false,
                additivity: fraiseql_core::compiler::fact_table::Additivity::Additive,
                column:     None,
            }],
            dimensions:               DimensionColumn {
                name:  "data".to_string(),
                paths: vec![],
            },
            denormalized_filters:     vec![FilterColumn {
                name:      "region".to_string(),
                sql_type:  SqlType::Text,
                indexed:   true,
                hierarchy: None,
            }],
            calendar_dimensions:      vec![],
            grain:                    None,
            native_dimension_mapping: HashMap::new(),
        },
    );
    schema.build_indexes();
    schema
}

async fn executor() -> Option<Executor> {
    let url = fraiseql_test_support::try_database_url()?;
    let adapter = PostgresAdapter::new(&url).await.unwrap();
    for ddl in [
        format!("DROP TABLE IF EXISTS {VIEW}, {FACT}"),
        format!("CREATE TABLE {VIEW} (pk_user bigint, data jsonb)"),
        format!(
            r#"INSERT INTO {VIEW} VALUES
               (1, '{{"id": "{ALICE}", "name": "Alice", "email": "alice@example.com"}}'),
               (2, '{{"id": "bbbb0000-0000-0000-0000-000000000002", "name": "Bob", "email": "bob@example.com"}}')"#
        ),
        format!("CREATE TABLE {FACT} (amount bigint, region text, data jsonb)"),
        format!("INSERT INTO {FACT} VALUES (5, 'eu', '{{}}'), (7, 'us', '{{}}')"),
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
        expires_at:       Utc::now() + fraiseql_core::chrono::Duration::hours(1),
        authenticated_at: Utc::now(),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// `document`'s answer through each entry a document reaches the executor by, as one
/// value each: the error's text when it is refused.
async fn answers(executor: &Executor, document: &str, variables: Option<&Value>) -> Vec<Value> {
    let text = |r: fraiseql_core::error::Result<Value>| r.unwrap_or_else(|e| json!(e.to_string()));
    let body = executor
        .execute_operation_body(document, variables, None)
        .await
        .and_then(fraiseql_core::runtime::ResponseBody::into_value);
    vec![
        text(executor.execute(document, variables).await),
        text(executor.execute_operation(document, variables, None).await),
        text(body),
        text(executor.execute_with_security(document, variables, &principal()).await),
    ]
}

/// The pairs `(with an untyped inline fragment, written out)`, each under `variables`.
fn pairs() -> Vec<(String, String, Value)> {
    let node = format!(r#"node(id: "{}")"#, encode_node_id("User", ALICE));
    let on = |value: bool| json!({ "on": value });
    let mut pairs = vec![
        (
            "{ users { id ... { name } } }".to_string(),
            "{ users { id name } }".to_string(),
            json!({}),
        ),
        (
            "{ users { ... { ... { name } } id } }".into(),
            "{ users { name id } }".into(),
            json!({}),
        ),
        (
            "{ users { ... on User { email } ... { name } } }".into(),
            "{ users { email name } }".into(),
            json!({}),
        ),
        (
            "{ users { id ... @skip(if: true) { name } } }".into(),
            "{ users { id } }".into(),
            json!({}),
        ),
        (
            "{ a: users { id ... { name } } b: users { ... { email } } }".into(),
            "{ a: users { id name } b: users { email } }".into(),
            json!({}),
        ),
        (
            format!("{{ {node} {{ ... {{ id }} }} }}"),
            format!("{{ {node} {{ id }} }}"),
            json!({}),
        ),
        (
            "{ users { ...F } } fragment F on User { id ... { name } }".into(),
            "{ users { id name } }".into(),
            json!({}),
        ),
    ];
    for value in [true, false] {
        pairs.push((
            "query($on: Boolean!) { users { id ... @include(if: $on) { name email } } }".into(),
            "query($on: Boolean!) { users { id name @include(if: $on) email @include(if: $on) } }"
                .into(),
            on(value),
        ));
    }
    pairs
}

/// Each untyped inline fragment is served as its fields written out, through every entry.
#[tokio::test]
async fn an_untyped_inline_fragment_is_served_as_its_fields_written_out() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let mut failures = Vec::new();
    for (fragment, written, variables) in pairs() {
        let expected = answers(&executor, &written, Some(&variables)).await;
        assert!(expected[0].get("data").is_some_and(|d| !d.is_null()), "{written}: {expected:?}");
        assert!(!expected[0].to_string().contains("..."), "{written}: {expected:?}");
        let actual = answers(&executor, &fragment, Some(&variables)).await;
        if actual != expected {
            failures.push(format!(
                "{fragment} {variables}\n    got:      {}\n    expected: {}",
                actual[0], expected[0]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} form(s) differ:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

/// An aggregate's rows are its row type's: an inline fragment on that type, typed or not,
/// is answered as the same fields written out.
#[tokio::test]
async fn an_inline_fragment_on_an_aggregates_row_type_is_answered_as_written() {
    let Some(executor) = executor().await else {
        return;
    };
    let root = "uif_sale_aggregate(groupBy: {region: true}, aggregates: [{amount_sum: true}], \
                orderBy: {region: ASC})";
    let written = answers(&executor, &format!("{{ {root} {{ region amount_sum }} }}"), None).await;
    assert_eq!(
        written[0]["data"]["uif_sale_aggregate"],
        json!([{"region": "eu", "amount_sum": "5"}, {"region": "us", "amount_sum": "7"}]),
        "{written:?}"
    );
    for fragment in [
        format!("{{ {root} {{ ... on UifSaleAggregateRow {{ region amount_sum }} }} }}"),
        format!("{{ {root} {{ region ... {{ amount_sum }} }} }}"),
        format!(
            "{{ {root} {{ region ... on UifSaleAggregateRow @include(if: true) {{ amount_sum }} \
             }} }}"
        ),
    ] {
        assert_eq!(answers(&executor, &fragment, None).await, written, "{fragment}");
    }
}
