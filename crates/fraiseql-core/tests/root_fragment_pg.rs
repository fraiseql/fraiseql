//! A fragment at the root of an operation, executed against PostgreSQL.
//!
//! GraphQL § 5.5 allows a fragment wherever a selection set is, the operation's own
//! included, and § 6.3.2 (`CollectFields`) expands it before anything executes. Client tools
//! that factor root fields into fragments produce these documents. Each is served as the
//! same document with the fragment written out, through every entry a document reaches the
//! executor by; a fragment typed other than the operation's root type is refused, naming it.
//!
//! Self-skips when no `DATABASE_URL` is set, so it is inert in the database-free `test` leg
//! and runs in the Dagger `integration: postgres` leg (`--test '*'`).
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `rf_*` fixtures → run `--test-threads=1`.
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

const VIEW: &str = "v_rf_user";
const FACT: &str = "tf_rf_sale";
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

/// The pairs `(with a root fragment, written out)`.
fn pairs() -> Vec<(String, String)> {
    let node = format!(r#"node(id: "{}")"#, encode_node_id("User", ALICE));
    let mut pairs: Vec<(String, String)> = vec![
        ("{ ... on Query { users { id name } } }", "{ users { id name } }".into()),
        ("{ ... { users { id name } } }", "{ users { id name } }".into()),
        ("{ ...Q } fragment Q on Query { users { id name } }", "{ users { id name } }".into()),
        // A spread inside a root fragment, and one of two roots.
        (
            "{ ...Q } fragment Q on Query { ...R } fragment R on Query { a: users { id } b: users { email } }",
            "{ a: users { id } b: users { email } }".into(),
        ),
        ("{ ... on Query { users { id } __typename } }", "{ users { id } __typename }".into()),
        ("{ ... on Query { __typename } }", "{ __typename }".into()),
        ("{ ... { rf_sale_aggregate { count amount_sum } } }", "{ rf_sale_aggregate { count amount_sum } }".into()),
    ]
    .into_iter()
    .map(|(fragment, written): (&str, String)| (fragment.to_string(), written))
    .collect();
    pairs.push((
        format!("{{ ... on Query {{ {node} {{ id ... on User {{ name }} }} }} }}"),
        format!("{{ {node} {{ id ... on User {{ name }} }} }}"),
    ));
    pairs.push((
        format!("{{ ...N }} fragment N on Query {{ {node} {{ id }} users {{ id }} }}"),
        format!("{{ {node} {{ id }} users {{ id }} }}"),
    ));
    pairs
}

/// A root fragment, inline or named, typed or not, is served as the document with the
/// fragment written out, through every entry.
#[tokio::test]
async fn a_root_fragment_is_served_as_its_fields_written_out() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for (fragment, written) in pairs() {
        let expected = answers(&executor, &written, None).await;
        assert!(expected[0].get("data").is_some_and(|d| !d.is_null()), "{written}: {expected:?}");
        assert_eq!(answers(&executor, &fragment, None).await, expected, "{fragment}");
    }
}

/// `@skip` and `@include` on a root fragment decide its fields, as on each field written
/// out: per request, from the request's variables.
#[tokio::test]
async fn a_directive_on_a_root_fragment_decides_its_fields() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    let fragment =
        "query($on: Boolean!) { a: users { id } ... @include(if: $on) { b: users { name } } }";
    let written = "query($on: Boolean!) { a: users { id } b: users @include(if: $on) { name } }";
    for on in [true, false] {
        let variables = json!({ "on": on });
        let expected = answers(&executor, written, Some(&variables)).await;
        assert_eq!(answers(&executor, fragment, Some(&variables)).await, expected, "on: {on}");
    }
    let skipped = answers(
        &executor,
        "{ a: users { id } ...Q @skip(if: true) } fragment Q on Query { b: users { name } }",
        None,
    )
    .await;
    assert_eq!(skipped, answers(&executor, "{ a: users { id } }", None).await);
}

/// A fragment at the root typed other than `Query` is refused, naming its type; and the
/// root-level checks hold for the fields a root fragment holds: `node` selects only what
/// `Node` declares outside a fragment on a type.
#[tokio::test]
async fn a_root_fragment_is_held_to_the_roots_rules() {
    let Some(executor) = executor().await else {
        eprintln!("SKIP: no PostgreSQL (set DATABASE_URL)");
        return;
    };
    for document in [
        "{ ... on User { id } }",
        "{ ...U } fragment U on User { id }",
        "{ users { id } ...U } fragment U on User { id }",
        "{ ... on Mutation { users { id } } }",
    ] {
        for answer in answers(&executor, document, None).await {
            let text = answer.to_string();
            assert!(
                text.starts_with("\"Validation error: ")
                    && text.contains("at the root of a query")
                    && !text.contains("not found in schema"),
                "{document}: {text}"
            );
        }
    }
    let node = format!(r#"node(id: "{}")"#, encode_node_id("User", ALICE));
    for document in [
        format!("{{ ... on Query {{ {node} {{ id name }} }} }}"),
        format!("{{ users {{ id }} ... on Query {{ {node} {{ id name }} }} }}"),
        format!("{{ users {{ id }} ...N }} fragment N on Query {{ {node} {{ name }} }}"),
    ] {
        for answer in answers(&executor, &document, None).await {
            let text = answer.to_string();
            assert!(text.contains("`name`") && text.contains("`Node`"), "{document}: {text}");
        }
    }
}
