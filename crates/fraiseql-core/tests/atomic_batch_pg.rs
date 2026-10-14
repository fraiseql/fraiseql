#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! A batch write commits all of its items or none of them (#1574).
//!
//! `Executor::execute_mutation_batch` (REST bulk insert) and
//! `Executor::execute_bulk_by_ids` (REST collection `PATCH`/`DELETE`) run every item in
//! one transaction. An item the database refuses, the function refuses, the commit gate
//! refuses or the authorizer refuses leaves nothing written, and the error names it.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL`.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    cache::{CacheConfig, CachedDatabaseAdapter, QueryResultCache},
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    error::FraiseQLError,
    runtime::{Executor, RuntimeConfig},
    schema::CompiledSchema,
    security::{Authorizer, AuthzDecision, AuthzRequest},
};
use serde_json::{Value, json};
use tokio::sync::OnceCell;

const SCHEMA: &str = "batch_1574";

static PROVISIONED: OnceCell<()> = OnceCell::const_new();

/// The `app.mutation_response` contract, the change-log outbox, and `tb_thing` with a
/// unique `name`. `fn_create_thing` refuses a name containing `refused`, and stamps a type
/// the mutation cannot return for a name containing `bogus` (the commit gate's refusal).
/// `fn_update_thing` refuses a row whose name contains `locked`.
async fn provision(adapter: &PostgresAdapter) {
    PROVISIONED
        .get_or_init(|| async {
            let statements = [
                "CREATE SCHEMA IF NOT EXISTS app".to_string(),
                "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation',\
                 'conflict','not_found','unauthorized','forbidden','internal',\
                 'transaction_failed','timeout','rate_limited','service_unavailable'); \
                 EXCEPTION WHEN duplicate_object THEN NULL; END $$"
                    .to_string(),
                "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
                 state_changed BOOLEAN, error_class app.mutation_error_class, \
                 status_detail TEXT, http_status SMALLINT, message TEXT, entity_id UUID, \
                 entity_type TEXT, entity JSONB, updated_fields TEXT[], cascade JSONB, \
                 error_detail JSONB, metadata JSONB); \
                 EXCEPTION WHEN duplicate_object THEN NULL; END $$"
                    .to_string(),
                format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"),
                format!("CREATE SCHEMA {SCHEMA}"),
                format!(
                    "CREATE TABLE {SCHEMA}.tb_thing (id UUID PRIMARY KEY DEFAULT \
                     gen_random_uuid(), name TEXT NOT NULL UNIQUE, note TEXT)"
                ),
                format!(
                    "CREATE VIEW {SCHEMA}.v_thing AS SELECT id, jsonb_build_object('id', id, \
                     'name', name, 'note', note) AS data FROM {SCHEMA}.tb_thing"
                ),
                format!(
                    "CREATE FUNCTION {SCHEMA}.fn_create_thing(p_name text) \
                     RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
                     DECLARE v_row {SCHEMA}.tb_thing%ROWTYPE; r app.mutation_response; BEGIN \
                     IF p_name LIKE '%refused%' THEN \
                       r.succeeded := false; r.state_changed := false; \
                       r.error_class := 'validation'; r.message := 'the function refused it'; \
                       RETURN r; \
                     END IF; \
                     INSERT INTO {SCHEMA}.tb_thing (name) VALUES (p_name) RETURNING * INTO v_row; \
                     r.succeeded := true; r.state_changed := true; \
                     r.entity_type := CASE WHEN p_name LIKE '%bogus%' THEN 'NotAThing' \
                       ELSE 'Thing' END; \
                     r.entity_id := v_row.id; \
                     r.entity := jsonb_build_object('id', v_row.id, 'name', v_row.name, \
                       'note', v_row.note); \
                     RETURN r; END; $$"
                ),
                format!(
                    "CREATE FUNCTION {SCHEMA}.fn_update_thing(p_id uuid, p_note text) \
                     RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
                     DECLARE v_row {SCHEMA}.tb_thing%ROWTYPE; r app.mutation_response; BEGIN \
                     SELECT * INTO v_row FROM {SCHEMA}.tb_thing WHERE id = p_id; \
                     IF v_row.name LIKE '%locked%' THEN \
                       r.succeeded := false; r.state_changed := false; \
                       r.error_class := 'conflict'; r.message := 'the row is locked'; \
                       RETURN r; \
                     END IF; \
                     UPDATE {SCHEMA}.tb_thing SET note = p_note WHERE id = p_id \
                       RETURNING * INTO v_row; \
                     r.succeeded := true; r.state_changed := true; r.entity_type := 'Thing'; \
                     r.entity_id := v_row.id; \
                     r.entity := jsonb_build_object('id', v_row.id, 'name', v_row.name, \
                       'note', v_row.note); \
                     r.updated_fields := ARRAY['note']; \
                     RETURN r; END; $$"
                ),
            ];
            for statement in statements
                .into_iter()
                .chain(fraiseql_test_support::changelog::entity_change_log_provision_statements())
            {
                adapter.execute_raw_query(&statement).await.unwrap();
            }
        })
        .await;
}

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [{
            "name": "Thing",
            "sql_source": format!("{SCHEMA}.v_thing"),
            "fields": [
                { "name": "id", "field_type": "ID" },
                { "name": "name", "field_type": "String" },
                { "name": "note", "field_type": "String", "nullable": true }
            ]
        }],
        "queries": [{
            "name": "things",
            "return_type": "Thing",
            "returns_list": true,
            "sql_source": format!("{SCHEMA}.v_thing")
        }],
        "mutations": [
            {
                "name": "createThing",
                "return_type": "Thing",
                "sql_source": format!("{SCHEMA}.fn_create_thing"),
                "operation": { "Insert": { "table": "tb_thing" } },
                "arguments": [{ "name": "name", "arg_type": "String", "nullable": false }]
            },
            {
                "name": "updateThing",
                "return_type": "Thing",
                "sql_source": format!("{SCHEMA}.fn_update_thing"),
                "operation": { "Update": { "table": "tb_thing" } },
                "arguments": [
                    { "name": "id", "arg_type": "ID", "nullable": false },
                    { "name": "note", "arg_type": "String", "nullable": true }
                ]
            }
        ]
    }))
    .expect("schema")
}

async fn adapter() -> Arc<PostgresAdapter> {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    adapter
}

/// A name no other test uses: every test reads only its own rows.
fn tag() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn creates(names: &[String]) -> Vec<Value> {
    names.iter().map(|name| json!({ "name": name })).collect()
}

/// The rows whose name starts with `tag`.
async fn rows_tagged(adapter: &PostgresAdapter, tag: &str) -> Vec<(String, Option<String>)> {
    adapter
        .execute_raw_query(&format!(
            "SELECT name, note FROM {SCHEMA}.tb_thing WHERE name LIKE '{tag}%' ORDER BY name"
        ))
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap().to_string(),
                row["note"].as_str().map(str::to_string),
            )
        })
        .collect()
}

/// How many transactions wrote the rows tagged `tag`, and the outbox rows describing them.
async fn writers_of(adapter: &PostgresAdapter, tag: &str) -> (i64, i64) {
    let one = |sql: String| async move {
        adapter.execute_raw_query(&sql).await.unwrap()[0]["n"].as_i64().unwrap()
    };
    (
        one(format!(
            "SELECT count(DISTINCT xmin::text) AS n FROM {SCHEMA}.tb_thing WHERE name LIKE '{tag}%'"
        ))
        .await,
        one(format!(
            "SELECT count(DISTINCT xmin::text) AS n FROM core.tb_entity_change_log \
             WHERE object_data->>'name' LIKE '{tag}%'"
        ))
        .await,
    )
}

async fn outbox_rows(adapter: &PostgresAdapter, tag: &str) -> i64 {
    adapter
        .execute_raw_query(&format!(
            "SELECT count(*) AS n FROM core.tb_entity_change_log \
             WHERE object_data->>'name' LIKE '{tag}%'"
        ))
        .await
        .unwrap()[0]["n"]
        .as_i64()
        .unwrap()
}

fn message(error: &FraiseQLError) -> String {
    error.to_string()
}

/// Insert `names` directly, and return their ids.
async fn seed(adapter: &PostgresAdapter, names: &[String]) -> Vec<Value> {
    let mut ids = Vec::with_capacity(names.len());
    for name in names {
        let rows = adapter
            .execute_raw_query(&format!(
                "INSERT INTO {SCHEMA}.tb_thing (name) VALUES ('{name}') RETURNING id::text AS id"
            ))
            .await
            .unwrap();
        ids.push(rows[0]["id"].clone());
    }
    ids
}

#[tokio::test]
async fn a_bulk_insert_commits_every_item_in_one_transaction() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let names: Vec<String> = (1..=5).map(|i| format!("{tag}-{i}")).collect();

    let result = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    let result = result.unwrap();
    assert_eq!(result.affected_rows, 5);
    let returned: Vec<&str> = result
        .entities
        .as_deref()
        .unwrap()
        .iter()
        .map(|e| e["data"]["createThing"]["name"].as_str().unwrap())
        .collect();
    assert_eq!(returned, names.iter().map(String::as_str).collect::<Vec<_>>(), "in item order");
    assert_eq!(rows_tagged(&adapter, &tag).await.len(), 5);
    assert_eq!(
        writers_of(&adapter, &tag).await,
        (1, 1),
        "every row and every outbox row written by one transaction"
    );
    assert_eq!(outbox_rows(&adapter, &tag).await, 5, "one outbox row per item");
}

#[tokio::test]
async fn a_bulk_insert_whose_third_item_violates_a_constraint_writes_nothing() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let mut names: Vec<String> = (1..=5).map(|i| format!("{tag}-{i}")).collect();
    names[2] = names[0].clone();

    let refused = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    let error = refused.expect_err("a duplicate name refuses the batch");
    assert_eq!(rows_tagged(&adapter, &tag).await, vec![], "items 1 and 2 rolled back");
    assert_eq!(outbox_rows(&adapter, &tag).await, 0, "and their outbox rows");
    assert!(
        matches!(&error, FraiseQLError::Database { sql_state: Some(s), .. } if s == "23505"),
        "the item's own error, still a unique violation: {error:?}"
    );
    assert!(message(&error).contains("item 3 of 5"), "names the item: {error}");
}

#[tokio::test]
async fn a_bulk_insert_whose_item_the_function_refuses_writes_nothing() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let mut names: Vec<String> = (1..=5).map(|i| format!("{tag}-{i}")).collect();
    names[3] = format!("{tag}-refused");

    let refused = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    let error = refused.expect_err("a refused item refuses the batch");
    assert_eq!(rows_tagged(&adapter, &tag).await, vec![], "items 1-3 and 5 rolled back");
    assert!(
        matches!(error, FraiseQLError::Validation { .. }),
        "the function's error class, `validation`: {error:?}"
    );
    assert!(message(&error).contains("item 4 of 5"), "names the item: {error}");
    assert!(message(&error).contains("the function refused it"), "{error}");
}

#[tokio::test]
async fn the_commit_gate_refusing_item_k_rolls_back_the_items_before_it() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let names = vec![
        format!("{tag}-1"),
        format!("{tag}-2"),
        format!("{tag}-bogus"),
    ];

    let refused = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    let error = refused.expect_err("an off-contract stamp is refused before the commit");
    assert_eq!(rows_tagged(&adapter, &tag).await, vec![]);
    assert!(message(&error).contains("item 3 of 3"), "names the item: {error}");
}

/// Refuses a mutation whose input names a thing `forbidden`.
struct NoForbiddenThings;

impl Authorizer for NoForbiddenThings {
    fn authorize(&self, req: &AuthzRequest<'_>) -> fraiseql_core::error::Result<AuthzDecision> {
        let name = req.input.and_then(|i| i.get("name")).and_then(Value::as_str);
        Ok(if name.is_some_and(|n| n.contains("forbidden")) {
            AuthzDecision::Deny {
                reason: "no forbidden things".to_string(),
            }
        } else {
            AuthzDecision::Allow
        })
    }
}

#[tokio::test]
async fn an_item_the_authorizer_refuses_writes_nothing() {
    let adapter = adapter().await;
    let config = RuntimeConfig {
        authorizer: Some(Arc::new(NoForbiddenThings)),
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema(), Arc::clone(&adapter), config);
    let tag = tag();
    let names = vec![
        format!("{tag}-1"),
        format!("{tag}-2"),
        format!("{tag}-forbidden"),
    ];

    let refused = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    let error = refused.expect_err("an unauthorized item refuses the batch");
    assert_eq!(rows_tagged(&adapter, &tag).await, vec![], "nothing ran before the refusal");
    assert!(matches!(error, FraiseQLError::Authorization { .. }), "{error:?}");
    assert!(message(&error).contains("item 3 of 3"), "names the item: {error}");
}

#[tokio::test]
async fn a_collection_patch_whose_fourth_row_the_function_refuses_changes_nothing() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let mut names: Vec<String> = (1..=5).map(|i| format!("{tag}-{i}")).collect();
    names[3] = format!("{tag}-4-locked");
    let ids = seed(&adapter, &names).await;

    let refused = executor
        .execute_bulk_by_ids("updateThing", "id", &ids, Some(&json!({ "note": "patched" })), None)
        .await;

    let error = refused.expect_err("a locked row refuses the batch");
    let notes: Vec<Option<String>> =
        rows_tagged(&adapter, &tag).await.into_iter().map(|(_, note)| note).collect();
    assert_eq!(notes, vec![None; 5], "no row changed");
    assert!(
        matches!(error, FraiseQLError::Conflict { .. }),
        "the function's error class, `conflict`: {error:?}"
    );
    assert!(message(&error).contains("item 4 of 5"), "names the item: {error}");
    assert!(message(&error).contains("the row is locked"), "{error}");
}

#[tokio::test]
async fn a_collection_patch_commits_every_row_in_one_transaction() {
    let adapter = adapter().await;
    let executor = Executor::new(schema(), Arc::clone(&adapter));
    let tag = tag();
    let names: Vec<String> = (1..=5).map(|i| format!("{tag}-{i}")).collect();
    let ids = seed(&adapter, &names).await;

    let result = executor
        .execute_bulk_by_ids("updateThing", "id", &ids, Some(&json!({ "note": "patched" })), None)
        .await
        .unwrap();

    assert_eq!(result.affected_rows, 5);
    let notes: Vec<Option<String>> =
        rows_tagged(&adapter, &tag).await.into_iter().map(|(_, note)| note).collect();
    assert_eq!(notes, vec![Some("patched".to_string()); 5]);
    assert_eq!(writers_of(&adapter, &tag).await, (1, 1), "one transaction");
}

/// The names `things` answers that start with `tag`.
async fn things_tagged(executor: &Executor, tag: &str) -> Vec<String> {
    let response = executor.execute("{ things { name } }", None).await.unwrap();
    response["data"]["things"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .filter(|n| n.starts_with(tag))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn a_batch_that_rolls_back_evicts_nothing_and_one_that_commits_evicts_its_views() {
    let adapter = adapter().await;
    let cached = Arc::new(CachedDatabaseAdapter::new(
        PostgresAdapter::new(
            &common::testcontainer::get_test_container().await.connection_string(),
        )
        .await
        .unwrap(),
        QueryResultCache::new(CacheConfig::enabled()),
        "1.0.0".to_string(),
    ));
    let executor = Executor::new(schema(), Arc::clone(&cached));
    let tag = tag();
    assert_eq!(things_tagged(&executor, &tag).await, Vec::<String>::new());
    // Written behind the cache's back: only a miss can see it.
    seed(&adapter, &[format!("{tag}-0-behind")]).await;
    assert_eq!(
        things_tagged(&executor, &tag).await,
        Vec::<String>::new(),
        "the read is served from the cache"
    );

    let mut names: Vec<String> = (1..=3).map(|i| format!("{tag}-{i}")).collect();
    names[2] = names[0].clone();
    executor
        .execute_mutation_batch("createThing", &creates(&names), None)
        .await
        .expect_err("a duplicate refuses the batch");
    assert_eq!(
        things_tagged(&executor, &tag).await,
        Vec::<String>::new(),
        "a batch that wrote nothing evicts nothing"
    );

    let names: Vec<String> = (1..=2).map(|i| format!("{tag}-{i}")).collect();
    executor
        .execute_mutation_batch("createThing", &creates(&names), None)
        .await
        .unwrap();
    assert_eq!(
        things_tagged(&executor, &tag).await,
        vec![
            format!("{tag}-0-behind"),
            format!("{tag}-1"),
            format!("{tag}-2")
        ],
        "a committed batch evicts its views"
    );
}

#[tokio::test]
async fn a_dry_run_batch_answers_every_item_and_writes_nothing() {
    let adapter = adapter().await;
    let config = RuntimeConfig {
        dry_run_mutations: true,
        ..RuntimeConfig::default()
    };
    let executor = Executor::with_config(schema(), Arc::clone(&adapter), config);
    let tag = tag();
    let names: Vec<String> = (1..=3).map(|i| format!("{tag}-{i}")).collect();

    let result = executor.execute_mutation_batch("createThing", &creates(&names), None).await;

    assert_eq!(result.unwrap().affected_rows, 3);
    assert_eq!(rows_tagged(&adapter, &tag).await, vec![]);
    assert_eq!(outbox_rows(&adapter, &tag).await, 0);
}
