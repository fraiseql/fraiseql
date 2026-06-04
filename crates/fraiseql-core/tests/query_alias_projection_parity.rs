#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(missing_docs)]

//! Issue #418 — an aliased query field must project the *source* JSONB column,
//! not a column derived from the alias.
//!
//! ## The bug
//!
//! The query SQL projector (`projection_generator::render_field`) derived the
//! JSONB key from the *response key* (`to_snake_case(alias)`), so a selection
//! like `label: name` generated `data->>'label'` (NULL) instead of
//! `data->>'name'`. The mutation projector was already correct after #410.
//!
//! ## The oracle
//!
//! Queries project in SQL (the view's `jsonb_build_object`); mutations project in
//! Rust (`runtime::project_entity`). The contract is that a client selecting the
//! same *aliased* fields sees the identical JSON whether the entity arrives via a
//! query or as a mutation's success payload. So we run the SAME aliased selection
//! through both paths against a real PostgreSQL database and assert the shapes are
//! equal — and concretely that each alias holds its source column's value.
//!
//! The mutation function returns the RAW entity (`snake_case` keys, not the view
//! row) so the Rust projection is actually exercised.

mod common;

use std::sync::Arc;

use fraiseql_core::{
    db::{DatabaseAdapter, postgres::PostgresAdapter},
    runtime::Executor,
    schema::CompiledSchema,
};
use serde_json::json;

const SCHEMA: &str = "issue_418";

/// Provision the `app.mutation_response` contract + an isolated schema holding a
/// `tb_thing` table, a `snake_case`-projecting view, and an update function that
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
    // generator reads the snake_case source column and outputs the requested
    // (possibly aliased) surface key.
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
async fn aliased_query_shape_matches_aliased_mutation_shape() {
    let container = common::testcontainer::get_test_container().await;
    let adapter = Arc::new(PostgresAdapter::new(&container.connection_string()).await.unwrap());
    provision(&adapter).await;
    let id = seed_thing(&adapter).await;

    let executor = Executor::new(schema(), Arc::clone(&adapter));

    // Every field is aliased so the response key (output) differs from the source
    // field name (`label`≠`name`, `code`≠`postalCode`). Pre-#418 the query path
    // read `data->>'label'` / `…->>'code'` (NULL); the mutation path was correct.
    let selection = "{ ident: id label: name billing: billingAddress { code: postalCode } }";

    // Drive the mutation first (sets name), then read it back via the query, so
    // both paths reflect the same persisted state and are directly comparable.
    let mut_vars = json!({ "id": id, "name": "Acme Updated" });
    let mut_doc = format!("mutation {{ updateThing {selection} }}");
    let mut_res = executor.execute(&mut_doc, Some(&mut_vars)).await.unwrap();
    let mut_thing = mut_res.get("data").and_then(|d| d.get("updateThing")).cloned().unwrap();

    let query_doc = format!("{{ thing(id: \"{id}\") {selection} }}");
    let query_res = executor.execute(&query_doc, None).await.unwrap();
    let query_thing = query_res.get("data").and_then(|d| d.get("thing")).cloned().unwrap();

    // Parity: identical JSON from the SQL-projected query and the Rust-projected
    // mutation success payload, under aliasing.
    assert_eq!(
        query_thing,
        mut_thing,
        "\n#418 aliased query/mutation shape mismatch.\n\
         --- QUERY (SQL projection) ---\n{}\n\
         --- MUTATION (Rust projection) ---\n{}\n",
        serde_json::to_string_pretty(&query_thing).unwrap(),
        serde_json::to_string_pretty(&mut_thing).unwrap(),
    );

    // Concrete #418 contract on the QUERY path: each alias holds its *source*
    // column's value, and the un-aliased source keys do not leak into the output.
    assert_eq!(query_thing["ident"], json!(id), "alias `ident` reads `id`");
    assert_eq!(query_thing["label"], json!("Acme Updated"), "alias `label` reads source `name`");
    assert_eq!(
        query_thing["billing"]["code"],
        json!("75001"),
        "nested alias `code` reads source `postalCode`"
    );
    assert!(query_thing.get("name").is_none(), "un-aliased source key `name` must not leak");
    assert!(
        query_thing["billing"].get("postalCode").is_none(),
        "un-aliased nested source key `postalCode` must not leak"
    );
}
