#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable

//! #1512 part 2: text ordering follows the request locale's collation.
//!
//! The words are chosen so the orders disagree: Canadian French orders accents from the end of
//! the word (`côte` before `coté`), Swedish puts `Ä` after `Z`, and both differ from the
//! database's own default. PostgreSQL is the oracle for each expected order (the same words
//! sorted with `COLLATE "<tag>-x-icu"`); the test asserts the response rows come back in it,
//! and first that the oracle orders really differ from the default.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** creates and drops its own `v_locale_word` table.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use fraiseql_core::{
    compiler::fact_table::{DimensionColumn, FactTableMetadata, MeasureColumn, SqlType},
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldType, LocaleConfig, LocaleSource},
    security::SecurityContext,
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

const VIEW: &str = "v_locale_word";
const WORDS: [&str; 7] = ["cote", "côte", "coté", "côté", "apfel", "Äpfel", "Zebra"];

fn schema() -> CompiledSchema {
    let word = TestTypeBuilder::new("Word", VIEW)
        .relay_node()
        .with_implements(&["Node"])
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("word", FieldType::String)
        .with_simple_field("rank", FieldType::Int)
        .build();
    let mut words = TestQueryBuilder::new("words", "Word")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    words.auto_params.has_order_by = true;
    let mut connection = TestQueryBuilder::new("wordsConnection", "Word")
        .returns_list(true)
        .with_sql_source(VIEW)
        .relay_cursor_column("pk_word")
        .build();
    connection.auto_params.has_order_by = true;
    let mut schema = TestSchemaBuilder::new()
        .with_type(word)
        .with_query(words)
        .with_query(connection)
        .build();
    schema.interfaces.push(
        fraiseql_core::schema::InterfaceDefinition::new("Node")
            .with_field(fraiseql_core::schema::FieldDefinition::new("id", FieldType::Id)),
    );
    schema.locale = Some(
        LocaleConfig::new(
            "en-US",
            ["en-US", "fr-CA", "fr-FR", "sv-SE", "de-DE"].map(String::from).to_vec(),
            BTreeMap::new(),
            vec![LocaleSource::Header {
                header: "accept-language".to_string(),
            }],
        )
        .unwrap(),
    );
    schema
}

async fn adapter() -> Option<PostgresAdapter> {
    let pg = fraiseql_test_support::postgres().await?;
    let adapter = PostgresAdapter::new(pg.url()).await.unwrap();
    let values: Vec<String> = WORDS
        .iter()
        .enumerate()
        .map(|(i, w)| {
            format!(
                "({i}, jsonb_build_object('id', '{i}', 'word', '{w}', 'rank', {}, 'pk_word', {i}))",
                10 - i
            )
        })
        .collect();
    for ddl in [
        format!("DROP TABLE IF EXISTS {VIEW}"),
        format!("CREATE TABLE {VIEW} (pk_word bigint, data jsonb)"),
        format!("INSERT INTO {VIEW} (pk_word, data) VALUES {}", values.join(", ")),
    ] {
        adapter.execute_raw_query(&ddl).await.unwrap();
    }
    Some(adapter)
}

/// The words in the order PostgreSQL sorts them with `collation` (`None`: the default).
async fn oracle(adapter: &PostgresAdapter, collation: Option<&str>) -> Vec<String> {
    let collate = collation.map(|c| format!(" COLLATE \"{c}\"")).unwrap_or_default();
    let rows = adapter
        .execute_raw_query(&format!(
            "SELECT data->>'word' AS w FROM {VIEW} ORDER BY data->>'word'{collate}"
        ))
        .await
        .unwrap();
    rows.iter().map(|r| r["w"].as_str().unwrap().to_string()).collect()
}

fn words_of(response: &Value) -> Vec<String> {
    response["data"]["words"]
        .as_array()
        .unwrap_or_else(|| panic!("no words in {response}"))
        .iter()
        .map(|w| w["word"].as_str().unwrap().to_string())
        .collect()
}

/// A list query ordered by a text field, in two locales whose orders differ from each other
/// and from the default.
#[tokio::test]
async fn a_text_sort_follows_the_request_locale() {
    let Some(adapter) = adapter().await else {
        return;
    };
    let default = oracle(&adapter, None).await;
    let executor = Executor::new(schema(), Arc::new(adapter));
    for (locale, collation) in [("fr-CA", "fr-CA-x-icu"), ("sv-SE", "sv-SE-x-icu")] {
        let adapter = PostgresAdapter::new(&fraiseql_test_support::database_url()).await.unwrap();
        let expected = oracle(&adapter, Some(collation)).await;
        assert_ne!(expected, default, "{locale}: the data must discriminate");
        let response = with_request_locale(
            locale,
            executor.execute("{ words(orderBy: {word: ASC}) { word } }", None),
        )
        .await
        .unwrap();
        assert_eq!(words_of(&response), expected, "{locale}: {response}");
    }
}

/// A collation applies to text only: a numeric key sorted in a locale sorts as a number (a
/// `COLLATE` on it would be a SQL error).
#[tokio::test]
async fn a_numeric_sort_takes_no_collation() {
    let Some(adapter) = adapter().await else {
        return;
    };
    let executor = Executor::new(schema(), Arc::new(adapter));
    let response = with_request_locale(
        "sv-SE",
        executor.execute("{ words(orderBy: {rank: ASC}) { word } }", None),
    )
    .await
    .unwrap_or_else(|e| panic!("a numeric sort in a locale must run: {e}"));
    let expected: Vec<String> = WORDS.iter().rev().map(ToString::to_string).collect();
    assert_eq!(words_of(&response), expected, "{response}");
}

fn principal() -> SecurityContext {
    SecurityContext {
        user_id:          fraiseql_core::prelude::UserId::new("collation"),
        tenant_id:        None,
        roles:            vec![],
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-collation".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
        email:            None,
        display_name:     None,
    }
}

/// The fr-CA oracle order, and an executor over the words (with the relay runner).
async fn fr_ca() -> Option<(Vec<String>, Executor)> {
    let adapter = adapter().await?;
    let expected = oracle(&adapter, Some("fr-CA-x-icu")).await;
    assert_ne!(expected, oracle(&adapter, None).await, "the data must discriminate");
    Some((expected, Executor::new_with_relay(schema(), Arc::new(adapter))))
}

/// The authenticated list path (a separate runner arm from the anonymous one).
#[tokio::test]
async fn an_authenticated_text_sort_follows_the_request_locale() {
    let Some((expected, executor)) = fr_ca().await else {
        return;
    };
    let ctx = principal();
    let response = with_request_locale(
        "fr-CA",
        executor.execute_with_security("{ words(orderBy: {word: ASC}) { word } }", None, &ctx),
    )
    .await
    .unwrap();
    assert_eq!(words_of(&response), expected, "{response}");
}

/// A relay connection ordered by a text field.
#[tokio::test]
async fn a_relay_connection_sorts_in_the_request_locale() {
    let Some((expected, executor)) = fr_ca().await else {
        return;
    };
    let response = with_request_locale(
        "fr-CA",
        executor.execute(
            "{ wordsConnection(first: 20, orderBy: {word: ASC}) { edges { node { word } } } }",
            None,
        ),
    )
    .await
    .unwrap();
    let words: Vec<String> = response["data"]["wordsConnection"]["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|e| e["node"]["word"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(words, expected, "{response}");
}

fn fact_table() -> FactTableMetadata {
    FactTableMetadata {
        table_name:               VIEW.to_string(),
        type_name:                None,
        measures:                 vec![MeasureColumn {
            name:       "pk_word".to_string(),
            sql_type:   SqlType::BigInt,
            nullable:   false,
            additivity: fraiseql_core::compiler::fact_table::Additivity::Additive,
        }],
        dimensions:               DimensionColumn {
            name:  "data".to_string(),
            paths: vec![fraiseql_core::compiler::fact_table::DimensionPath {
                name:      "word".to_string(),
                json_path: "data->>'word'".to_string(),
                data_type: "string".to_string(),
            }],
        },
        denormalized_filters:     vec![],
        calendar_dimensions:      vec![],
        native_measures:          HashMap::new(),
        native_dimension_mapping: HashMap::new(),
    }
}

/// An aggregate grouped by a text dimension and ordered by it.
#[tokio::test]
async fn an_aggregate_ordered_by_a_text_dimension_sorts_in_the_request_locale() {
    let Some((expected, executor)) = fr_ca().await else {
        return;
    };
    let query = serde_json::json!({
        "table": VIEW,
        "groupBy": { "word": true },
        "aggregates": [{ "count": {} }],
        "orderBy": { "word": "ASC" }
    });
    let response = with_request_locale(
        "fr-CA",
        executor.execute_aggregate_query(&query, "words_aggregate", &fact_table()),
    )
    .await
    .unwrap();
    let words: Vec<String> = response["data"]["words_aggregate"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|r| r["word"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(words, expected, "{response}");
}

/// A window function ordered by a text dimension, and the final ordering by it.
#[tokio::test]
async fn a_window_ordered_by_a_text_dimension_sorts_in_the_request_locale() {
    let Some((expected, executor)) = fr_ca().await else {
        return;
    };
    let query = serde_json::json!({
        "table": VIEW,
        "select": [{ "type": "dimension", "path": "word", "alias": "word" }],
        "windows": [{
            "function": { "type": "row_number" },
            "alias": "position",
            "orderBy": [{ "field": "word", "direction": "ASC" }]
        }],
        "orderBy": [{ "field": "word", "direction": "ASC" }]
    });
    let response = with_request_locale(
        "fr-CA",
        executor.execute_window_query(&query, "words_window", &fact_table()),
    )
    .await
    .unwrap();
    let rows = response["data"]["words_window"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"));
    let words: Vec<String> = rows.iter().map(|r| r["word"].as_str().unwrap().to_string()).collect();
    assert_eq!(words, expected, "final ORDER BY: {response}");
    let positions: Vec<i64> = rows.iter().map(|r| r["position"].as_i64().unwrap()).collect();
    assert_eq!(positions, (1..=7).collect::<Vec<i64>>(), "OVER (ORDER BY): {response}");
}

/// One page of `wordsConnection` in `locale`, ordered by `word`: its words and `pageInfo`.
async fn word_page(executor: &Executor, locale: &str, window: &str) -> (Vec<String>, Value) {
    let query = format!(
        "{{ wordsConnection({window}, orderBy: {{word: ASC}}) {{ edges {{ node {{ word }} }} \
         pageInfo {{ hasNextPage hasPreviousPage startCursor endCursor }} }} }}"
    );
    let response = with_request_locale(locale, executor.execute(&query, None))
        .await
        .unwrap_or_else(|e| panic!("{window}: {e}"));
    let connection = &response["data"]["wordsConnection"];
    let words = connection["edges"]
        .as_array()
        .unwrap_or_else(|| panic!("{response}"))
        .iter()
        .map(|e| e["node"]["word"].as_str().unwrap().to_string())
        .collect();
    (words, connection["pageInfo"].clone())
}

/// #1521: a connection ordered by a text field, walked one row per page with `after` and then
/// with `before`, returns every word once in the request locale's order. The keyset compares
/// each cursor's word under the same collation the `ORDER BY` sorts by.
#[tokio::test]
async fn a_relay_walk_under_a_text_order_follows_the_request_locale() {
    let Some((expected, executor)) = fr_ca().await else {
        return;
    };
    let mut forward = Vec::new();
    let mut window = "first: 1".to_string();
    for _ in 0..=WORDS.len() {
        let (words, info) = word_page(&executor, "fr-CA", &window).await;
        forward.extend(words);
        if info["hasNextPage"] != Value::Bool(true) {
            break;
        }
        window = format!("first: 1, after: {}", info["endCursor"]);
    }
    assert_eq!(forward, expected, "forward");

    let mut backward = Vec::new();
    let mut window = "last: 1".to_string();
    for _ in 0..=WORDS.len() {
        let (words, info) = word_page(&executor, "fr-CA", &window).await;
        backward.splice(0..0, words);
        if info["hasPreviousPage"] != Value::Bool(true) {
            break;
        }
        window = format!("last: 1, before: {}", info["startCursor"]);
    }
    assert_eq!(backward, expected, "backward");
}

/// A cursor resumes only the ordering it was issued under: in another locale the same
/// `orderBy` sorts by another collation, and its word would be compared out of place.
#[tokio::test]
async fn a_cursor_issued_in_one_locale_is_refused_in_another() {
    let Some((_, executor)) = fr_ca().await else {
        return;
    };
    let (_, info) = word_page(&executor, "fr-CA", "first: 1").await;
    let query = format!(
        "{{ wordsConnection(first: 1, after: {}, orderBy: {{word: ASC}}) {{ edges {{ node {{ word \
         }} }} }} }}",
        info["endCursor"]
    );
    let err = with_request_locale("sv-SE", executor.execute(&query, None))
        .await
        .expect_err("a cursor from another locale's ordering is refused");
    assert!(err.to_string().contains("another `orderBy` or locale"), "{err}");
}
