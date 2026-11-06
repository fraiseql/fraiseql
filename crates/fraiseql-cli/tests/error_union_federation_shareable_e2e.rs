#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! Regression: the `MutationError` type `auto_error_union` synthesizes must be `@shareable`
//! so two federated subgraphs that both use the flag compose (#1944).
//!
//! `[fraiseql.mutations] auto_error_union = true` makes the cli synthesize one shared
//! `MutationError` value type, structurally identical in every subgraph that synthesizes it
//! and carrying no identity. Emitted without `@shareable`, a supergraph composing two such
//! subgraphs fails Federation-v2 validation with one `INVALID_FIELD_SHARING` per field
//! (`status`, `message`, `httpStatus`, `errorClass`, `errors`). The cascade envelope types
//! had the same defect (#698); an authored `MutationError` never did, since it is listed in
//! `federation.shareable_types`.
//!
//! This test drives the real compile path (`SchemaConverter::convert_with_options` with
//! `auto_error_union`) on federation-enabled subgraphs and asserts, hermetically, that the
//! compiled `federation.shareable_types` carries `MutationError` and that the rendered
//! `_service { sdl }` marks it `type MutationError @shareable`. The real-composer half
//! composes the committed `error_union_*.graphql` fixtures in the federation-compose CI leg
//! (`tools/federation/run-compose-check.sh`); `committed_error_union_sdl_fixtures_are_current`
//! keeps those fixtures in step with what this crate renders.

use std::path::PathBuf;

use fraiseql_cli::schema::{ConvertOptions, IntermediateSchema, converter::SchemaConverter};
use fraiseql_core::schema::CompiledSchema;

const ERROR_TYPE: &str = "MutationError";

/// An SDK-shaped federation subgraph with one entity, its list query and one
/// object-returning mutation, which `auto_error_union` wraps in `<Mutation>Result`.
/// The root query keeps the rendered SDL composable (Federation rejects a query-less
/// subgraph with `NO_QUERIES`, which would mask the sharing check).
fn subgraph_json(service_name: &str, entity: &str, sql_source: &str) -> String {
    let list_field = format!("{}s", entity.to_lowercase()); // Order → orders
    format!(
        r#"
{{
  "version": "2.0.0",
  "types": [
    {{
      "name": "{entity}",
      "fields": [
        {{"name": "id", "type": "ID", "nullable": false}},
        {{"name": "total", "type": "Float", "nullable": false}}
      ],
      "sql_source": "{sql_source}"
    }}
  ],
  "queries": [
    {{
      "name": "{list_field}",
      "return_type": "{entity}",
      "returns_list": true,
      "sql_source": "{sql_source}"
    }}
  ],
  "mutations": [
    {{
      "name": "create{entity}",
      "return_type": "{entity}",
      "sql_source": "fn_create_{sql_source}",
      "operation": "CREATE"
    }}
  ],
  "subscriptions": [],
  "federation": {{
    "enabled": true,
    "service_name": "{service_name}",
    "version": "v2",
    "entities": [
      {{"name": "{entity}", "key_fields": ["id"]}}
    ]
  }}
}}
"#
    )
}

fn compile(json: &str) -> CompiledSchema {
    let intermediate: IntermediateSchema =
        serde_json::from_str(json).expect("parse SDK schema.json");
    SchemaConverter::convert_with_options(
        intermediate,
        &ConvertOptions {
            auto_error_union: true,
        },
    )
    .expect("auto_error_union + federation subgraph must compile")
}

fn service_sdl(schema: &CompiledSchema) -> String {
    schema.federation_service_sdl().expect("federation is enabled")
}

#[test]
fn synthesized_mutation_error_is_added_to_shareable_types() {
    let compiled = compile(&subgraph_json("orders", "Order", "v_order"));
    assert!(
        compiled.types.iter().any(|t| t.name == ERROR_TYPE),
        "auto_error_union must synthesize {ERROR_TYPE}"
    );
    let fed = compiled.federation.as_ref().expect("federation block present");
    assert!(
        fed.shareable_types.iter().any(|t| t == ERROR_TYPE),
        "synthesized {ERROR_TYPE} must be in federation.shareable_types (else composition fails \
         INVALID_FIELD_SHARING); got {:?}",
        fed.shareable_types
    );
}

#[test]
fn synthesized_mutation_error_renders_shareable_in_service_sdl() {
    let sdl = service_sdl(&compile(&subgraph_json("orders", "Order", "v_order")));
    let marker = format!("type {ERROR_TYPE} @shareable");
    assert!(sdl.contains(&marker), "the _service SDL must render `{marker}`, got:\n{sdl}");
}

#[test]
fn result_unions_are_not_marked_shareable() {
    // `<Mutation>Result` unions are named per mutation and never collide; they stay unmarked.
    let compiled = compile(&subgraph_json("orders", "Order", "v_order"));
    let fed = compiled.federation.as_ref().expect("federation block present");
    assert!(
        !fed.shareable_types.iter().any(|t| t == "CreateOrderResult"),
        "result unions must not be listed as shareable: {:?}",
        fed.shareable_types
    );
}

#[test]
fn shareable_types_lists_mutation_error_once() {
    // A subgraph that already lists the name (an operator-declared `shareable_types`) keeps
    // a single entry.
    let json = subgraph_json("orders", "Order", "v_order").replace(
        r#""version": "v2","#,
        r#""version": "v2", "shareable_types": ["MutationError"],"#,
    );
    let compiled = compile(&json);
    let fed = compiled.federation.as_ref().expect("federation block present");
    let count = fed.shareable_types.iter().filter(|t| *t == ERROR_TYPE).count();
    assert_eq!(count, 1, "got {:?}", fed.shareable_types);
}

#[test]
fn without_federation_nothing_is_marked() {
    let json = r#"
{
  "version": "2.0.0",
  "types": [
    {"name": "Order", "fields": [{"name": "id", "type": "ID", "nullable": false}],
     "sql_source": "v_order"}
  ],
  "queries": [],
  "mutations": [
    {"name": "createOrder", "return_type": "Order", "sql_source": "fn_create_order",
     "operation": "CREATE"}
  ],
  "subscriptions": []
}
"#;
    let compiled = compile(json);
    assert!(compiled.types.iter().any(|t| t.name == ERROR_TYPE));
    assert!(compiled.federation.is_none());
}

/// Path to a committed subgraph SDL fixture in the core crate's `federation_compose` dir.
fn compose_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fraiseql-core/tests/fixtures/federation_compose")
        .join(name)
}

/// The committed `error_union_*` SDL fixtures are what the real-composer CI leg composes, so
/// they must match what the cli renders today. Re-bless after an intentional rendering change:
///
/// ```sh
/// BLESS_FEDERATION_SDL=1 \
///   cargo test -p fraiseql-cli --test error_union_federation_shareable_e2e --features federation
/// ```
#[test]
fn committed_error_union_sdl_fixtures_are_current() {
    let cases = [
        ("error_union_orders.graphql", subgraph_json("orders", "Order", "v_order")),
        ("error_union_users.graphql", subgraph_json("users", "User", "v_user")),
    ];
    let bless = std::env::var_os("BLESS_FEDERATION_SDL").is_some();

    for (file, json) in cases {
        let rendered = format!("{}\n", service_sdl(&compile(&json)).trim_end());
        let path = compose_fixture_path(file);

        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &rendered).unwrap();
            continue;
        }

        let committed = std::fs::read_to_string(&path).expect(
            "missing error_union federation_compose SDL fixture; re-bless with \
             BLESS_FEDERATION_SDL=1",
        );
        assert_eq!(
            committed,
            rendered,
            "{} is stale; re-bless with BLESS_FEDERATION_SDL=1",
            path.display()
        );
    }
}
