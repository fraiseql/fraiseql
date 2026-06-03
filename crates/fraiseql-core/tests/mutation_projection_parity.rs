#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #410 — a mutation's success-response projection must produce the SAME
//! shape as a query selecting the same fields over the same entity.
//!
//! ## The bug
//!
//! `execute_mutation_impl`'s success arm projects the returned entity with flat
//! `FieldMapping::simple` ("source == output"), so nested typed-object fields are
//! not recursively projected, casing-normalised, or `__typename`-injected. The
//! error arm already does this correctly via `build_field_mappings_from_type`.
//!
//! ## The oracle
//!
//! Queries project in SQL (the view's `jsonb_build_object`); mutations project in
//! Rust. The contract is that a client selecting `{ thing { id billingAddress {
//! postalCode } } }` sees the identical JSON whether the entity arrives via a
//! query or as a mutation's success payload. So we run the SAME selection through
//! both paths against a real PostgreSQL container and assert the shapes are equal
//! — no hard-coded casing guesses.
//!
//! The mutation function returns the RAW entity (not the view row) so the Rust
//! projection is actually exercised: if the function pre-projected via the view,
//! Rust would have nothing to do and the bug would be invisible.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::json;

const SCHEMA: &str = "issue_410";

/// Provision the `app.mutation_response` contract + an isolated schema holding a
/// `tb_thing` table, a camelCase-projecting view, and an update function that
/// returns the raw entity.
async fn provision(adapter: &PostgresAdapter) {
    adapter.execute_raw_query("CREATE SCHEMA IF NOT EXISTS app").await.unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_error_class AS ENUM ('validation','conflict',\
             'not_found','unauthorized','forbidden','internal','transaction_failed','timeout',\
             'rate_limited','service_unavailable'); EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();
    adapter
        .execute_raw_query(
            "DO $$ BEGIN CREATE TYPE app.mutation_response AS (succeeded BOOLEAN, \
             state_changed BOOLEAN, error_class app.mutation_error_class, status_detail TEXT, \
             http_status SMALLINT, message TEXT, entity_id UUID, entity_type TEXT, entity JSONB, \
             updated_fields TEXT[], cascade JSONB, error_detail JSONB, metadata JSONB); \
             EXCEPTION WHEN duplicate_object THEN NULL; END $$;",
        )
        .await
        .unwrap();

    adapter
        .execute_raw_query(&format!("DROP SCHEMA IF EXISTS {SCHEMA} CASCADE"))
        .await
        .unwrap();
    adapter.execute_raw_query(&format!("CREATE SCHEMA {SCHEMA}")).await.unwrap();

    adapter
        .execute_raw_query(&format!(
            "CREATE TABLE {SCHEMA}.tb_thing (\
             id UUID PRIMARY KEY DEFAULT gen_random_uuid(), \
             name TEXT, postal_code TEXT, city TEXT)"
        ))
        .await
        .unwrap();

    // View `data` uses snake_case keys — FraiseQL's contract: the projection
    // generator reads `data->'billing_address'->>'postal_code'` (snake source) and
    // outputs the camelCase surface key. The view emits snake; queries surface camel.
    adapter
        .execute_raw_query(&format!(
            "CREATE VIEW {SCHEMA}.v_thing AS SELECT id, \
             jsonb_build_object(\
               'id', id, \
               'name', name, \
               'billing_address', jsonb_build_object('postal_code', postal_code, 'city', city)\
             ) AS data FROM {SCHEMA}.tb_thing"
        ))
        .await
        .unwrap();

    // Update function returns the RAW entity (snake_case keys, NOT the view row).
    adapter
        .execute_raw_query(&format!(
            "CREATE FUNCTION {SCHEMA}.fn_update_thing(p_id uuid, p_name text) \
             RETURNS app.mutation_response LANGUAGE plpgsql AS $$ \
             DECLARE v_row {SCHEMA}.tb_thing%ROWTYPE; v_response app.mutation_response; BEGIN \
             UPDATE {SCHEMA}.tb_thing SET name = p_name WHERE id = p_id RETURNING * INTO v_row; \
             v_response.succeeded := true; \
             v_response.state_changed := true; \
             v_response.message := 'thing updated'; \
             v_response.entity_type := 'Thing'; \
             v_response.entity_id := v_row.id; \
             v_response.entity := jsonb_build_object(\
               'id', v_row.id, \
               'name', v_row.name, \
               'billing_address', jsonb_build_object('postal_code', v_row.postal_code, 'city', v_row.city)\
             ); \
             v_response.updated_fields := ARRAY['name']; \
             RETURN v_response; END; $$"
        ))
        .await
        .unwrap();
}

/// Insert one row, returning its id as a string.
async fn seed_thing(adapter: &PostgresAdapter) -> String {
    let rows = adapter
        .execute_raw_query(&format!(
            "INSERT INTO {SCHEMA}.tb_thing (name, postal_code, city) \
             VALUES ('Acme', '75001', 'Paris') RETURNING id::text AS id"
        ))
        .await
        .unwrap();
    rows.into_iter()
        .next()
        .and_then(|r| r.get("id").and_then(|v| v.as_str().map(ToString::to_string)))
        .expect("seeded id")
}

fn schema() -> CompiledSchema {
    serde_json::from_value(json!({
        "naming_convention": "camelCase",
        "types": [
            {
                "name": "Thing",
                "sql_source": format!("{SCHEMA}.v_thing"),
                "fields": [
                    { "name": "id", "field_type": "ID" },
                    { "name": "name", "field_type": "String" },
                    { "name": "billingAddress", "field_type": { "Object": "Address" } }
                ]
            },
            {
                "name": "Address",
                "sql_source": format!("{SCHEMA}.v_address"),
                "fields": [
                    { "name": "postalCode", "field_type": "String" },
                    { "name": "city", "field_type": "String" }
                ]
            }
        ],
        "queries": [
            {
                "name": "thing",
                "return_type": "Thing",
                "returns_list": false,
                "nullable": true,
                "sql_source": format!("{SCHEMA}.v_thing"),
                "arguments": [
                    { "name": "id", "arg_type": "ID", "nullable": false }
                ]
            }
        ],
        "mutations": [
            {
                "name": "updateThing",
                "return_type": "Thing",
                "sql_source": format!("{SCHEMA}.fn_update_thing"),
                "operation": { "Update": { "table": "tb_thing" } },
                "arguments": [
                    { "name": "id", "arg_type": "ID", "nullable": false },
                    { "name": "name", "arg_type": "String", "nullable": true }
                ]
            }
        ]
    }))
    .expect("schema")
}

#[tokio::test]
async fn mutation_success_shape_matches_query_shape() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    let executor = Executor::new(schema(), Arc::clone(&adapter));

    // The same selection drives both paths. It selects a nested typed object so the
    // mutation must recursively project (the #410 bug dropped it entirely). The
    // update sets `name`, so `name` is deliberately NOT selected — the comparison is
    // about projected SHAPE, identical between query and mutation.
    let selection = "{ id billingAddress { postalCode city } }";

    // Shape A — query (projected in SQL by the view).
    let query_doc = format!("{{ thing(id: \"{id}\") {selection} }}");
    let query_res = executor.execute(&query_doc, None).await.unwrap();
    let query_thing = query_res.get("data").and_then(|d| d.get("thing")).cloned().unwrap();

    // Shape B — mutation success (projected in Rust from the RAW snake_case entity).
    let mut_vars = json!({ "id": id, "name": "Acme Updated" });
    let mut_doc = format!("mutation {{ updateThing {selection} }}");
    let mut_res = executor.execute(&mut_doc, Some(&mut_vars)).await.unwrap();
    let mut_thing = mut_res.get("data").and_then(|d| d.get("updateThing")).cloned().unwrap();

    assert_eq!(
        query_thing,
        mut_thing,
        "\n#410 query/mutation shape mismatch.\n\
         --- QUERY (shape A) ---\n{}\n\
         --- MUTATION (shape B) ---\n{}\n",
        serde_json::to_string_pretty(&query_thing).unwrap(),
        serde_json::to_string_pretty(&mut_thing).unwrap(),
    );

    // Concrete contract: camelCase output keys read from snake_case storage, nested
    // object recursively projected.
    assert_eq!(query_thing["id"], json!(id));
    assert_eq!(query_thing["billingAddress"]["postalCode"], json!("75001"));
    assert_eq!(query_thing["billingAddress"]["city"], json!("Paris"));
}
