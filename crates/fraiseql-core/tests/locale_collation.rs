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

use std::{collections::BTreeMap, sync::Arc};

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::{Executor, with_request_locale},
    schema::{CompiledSchema, FieldType, LocaleConfig, LocaleSource},
};
use fraiseql_test_utils::schema_builder::{TestQueryBuilder, TestSchemaBuilder, TestTypeBuilder};
use serde_json::Value;

const VIEW: &str = "v_locale_word";
const WORDS: [&str; 7] = ["cote", "côte", "coté", "côté", "apfel", "Äpfel", "Zebra"];

fn schema() -> CompiledSchema {
    let word = TestTypeBuilder::new("Word", VIEW)
        .relay_node()
        .with_simple_field("id", FieldType::Id)
        .with_simple_field("word", FieldType::String)
        .with_simple_field("rank", FieldType::Int)
        .build();
    let mut words = TestQueryBuilder::new("words", "Word")
        .returns_list(true)
        .with_sql_source(VIEW)
        .build();
    words.auto_params.has_order_by = true;
    let mut schema = TestSchemaBuilder::new().with_type(word).with_query(words).build();
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
            format!("({i}, jsonb_build_object('id', '{i}', 'word', '{w}', 'rank', {}))", 10 - i)
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
