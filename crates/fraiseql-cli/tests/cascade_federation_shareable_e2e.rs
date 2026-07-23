#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! Regression: the synthesized graphql-cascade envelope value types must be `@shareable`
//! so a multi-subgraph federation with `cascade = true` mutations composes (#698).
//!
//! A `cascade = true` mutation makes the cli synthesize a fixed set of envelope value types
//! (`UpdatedEntity`, `DeletedEntity`, `CascadeMetadata`, `QueryInvalidation`, `CascadeUpdates`)
//! — structurally identical in **every** cascade-enabled subgraph and carrying no identity.
//! Before this fix they were emitted as plain, non-`@shareable` types, so composing two such
//! subgraphs into a supergraph fails Federation-v2 validation with one `INVALID_FIELD_SHARING`
//! per field (21 in total): the same value type is resolved from multiple subgraphs and is
//! non-shareable in all of them. This is the exact class already handled for the authored
//! `MutationError` via `federation.shareable_types`; the synthesizer is the still-unhandled
//! instance of it.
//!
//! The fix marks the five envelope types `@shareable` at the single synthesis site. This test
//! drives the real compile path (`SchemaConverter::convert`) on a federation-enabled subgraph
//! with a cascade mutation and asserts, hermetically, both halves:
//!   1. the compiled `federation.shareable_types` carries the five envelope types, and
//!   2. the rendered `_service { sdl }` marks each of them `type <X> @shareable`.
//!
//! Two subgraphs that both render the identical envelope types `@shareable` compose without
//! `INVALID_FIELD_SHARING` — the real-composer assertion lives in the federation-compose CI
//! leg; this is its hermetic companion (same split as `federation_compose.rs`).

use std::path::PathBuf;

use fraiseql_cli::schema::{IntermediateSchema, converter::SchemaConverter};

/// The five synthesized graphql-cascade envelope value types that are identical across every
/// cascade subgraph and therefore must be `@shareable`. (The per-mutation `<Name>Payload`
/// types are uniquely named per entity and never collide; the shared enums and the
/// `CascadeNode` interface compose by identity, so neither needs `@shareable`.)
const ENVELOPE_TYPES: &[&str] = &[
    "UpdatedEntity",
    "DeletedEntity",
    "CascadeMetadata",
    "QueryInvalidation",
    "CascadeUpdates",
];

/// An SDK-shaped federation subgraph with exactly one `cascade = true` mutation — the shape
/// `@fraiseql.type(crud=True, cascade=True)` + a federation block produces.
fn cascade_subgraph_json(service_name: &str, entity: &str, sql_source: &str) -> String {
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
  "queries": [],
  "mutations": [
    {{
      "name": "create{entity}",
      "return_type": "{entity}",
      "cascade": true,
      "sql_source": "fn_create_{sql_source}",
      "operation": "CREATE"
    }}
  ],
  "subscriptions": [],
  "federation": {{
    "enabled": true,
    "service_name": "{service_name}",
    "apollo_version": 2,
    "entities": [
      {{"name": "{entity}", "key_fields": ["id"]}}
    ]
  }}
}}
"#
    )
}

fn compile(json: &str) -> fraiseql_core::schema::CompiledSchema {
    let intermediate: IntermediateSchema =
        serde_json::from_str(json).expect("parse SDK schema.json");
    SchemaConverter::convert(intermediate).expect("cascade + federation subgraph must compile")
}

fn service_sdl(schema: &fraiseql_core::schema::CompiledSchema) -> String {
    let raw = schema.raw_schema();
    let meta = schema
        .federation_metadata()
        .expect("federation_metadata must be Some when enabled");
    fraiseql_core::federation::generate_service_sdl(&raw, &meta)
}

#[test]
fn cascade_envelope_types_are_added_to_shareable_types() {
    let compiled = compile(&cascade_subgraph_json("orders", "Order", "v_order"));
    let fed = compiled.federation.as_ref().expect("federation block present");

    for name in ENVELOPE_TYPES {
        assert!(
            fed.shareable_types.iter().any(|t| t == name),
            "synthesized cascade envelope type {name:?} must be in federation.shareable_types \
             (else composition fails INVALID_FIELD_SHARING); got {:?}",
            fed.shareable_types
        );
    }
}

#[test]
fn cascade_envelope_types_render_shareable_in_service_sdl() {
    let compiled = compile(&cascade_subgraph_json("orders", "Order", "v_order"));
    let sdl = service_sdl(&compiled);

    for name in ENVELOPE_TYPES {
        let marker = format!("type {name} @shareable");
        assert!(
            sdl.contains(&marker),
            "cascade envelope type {name:?} must render `{marker}` in the _service SDL, got:\n{sdl}"
        );
    }
}

#[test]
fn two_cascade_subgraphs_both_mark_the_envelope_types_shareable() {
    // The composition scenario: two independent cascade-enabled subgraphs each synthesize the
    // identical envelope types. Federation v2 composes them only if *both* mark those shared
    // value types `@shareable`; before #698 neither did → INVALID_FIELD_SHARING.
    let orders = service_sdl(&compile(&cascade_subgraph_json("orders", "Order", "v_order")));
    let users = service_sdl(&compile(&cascade_subgraph_json("users", "User", "v_user")));

    for name in ENVELOPE_TYPES {
        let marker = format!("type {name} @shareable");
        assert!(orders.contains(&marker), "`orders` subgraph missing `{marker}`");
        assert!(users.contains(&marker), "`users` subgraph missing `{marker}`");
    }
}

// ---------------------------------------------------------------------------
// Real-composer companion (#698): committed cascade subgraph SDL fixtures.
//
// The two-subgraph assertion above only checks that each rendered `_service` SDL
// *contains* the `@shareable` markers — it never actually composes them. The real
// `composeServices` proof lives in the federation-compose CI leg
// (`make federation-compose-check` → `tools/federation/run-compose-check.sh`), which
// composes committed `.graphql` fixtures with Apollo Federation v2.
//
// Those fixtures live beside the core golden pair (`catalog`/`reviews`) in
// `crates/fraiseql-core/tests/fixtures/federation_compose/`, but the core builders
// there construct `CompiledSchema` directly and so **bypass the CLI cascade
// synthesis** where the #698 fix lives. A cascade fixture must therefore be rendered
// from the real CLI path (`SchemaConverter::convert` → `federation_metadata()` →
// `generate_service_sdl`) and lock-stepped from *this* crate, so the committed SDL the
// composer validates is exactly what the fix produces. Revert the `shareable_types`
// addition in `converter/cascade_types.rs` and this test goes RED (live SDL drops the
// `@shareable` markers, the committed fixtures still carry them); re-blessing then
// makes the composer leg go RED with `INVALID_FIELD_SHARING`.

/// A cascade subgraph rich enough to *compose*: unlike [`cascade_subgraph_json`] it
/// exposes a root query, so the rendered `_service` SDL carries a non-empty
/// `type Query` (Federation rejects a query-less subgraph with `NO_QUERIES`, which
/// would mask the sharing check this fixture exists to exercise).
fn cascade_compose_subgraph_json(service_name: &str, entity: &str, sql_source: &str) -> String {
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
      "cascade": true,
      "sql_source": "fn_create_{sql_source}",
      "operation": "CREATE"
    }}
  ],
  "subscriptions": [],
  "federation": {{
    "enabled": true,
    "service_name": "{service_name}",
    "apollo_version": 2,
    "entities": [
      {{"name": "{entity}", "key_fields": ["id"]}}
    ]
  }}
}}
"#
    )
}

/// Path to a committed subgraph SDL fixture in the core crate's
/// `federation_compose` fixture dir (shared with the `catalog`/`reviews` golden pair).
fn compose_fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fraiseql-core/tests/fixtures/federation_compose")
        .join(name)
}

/// The committed cascade subgraph SDL fixtures are what the real-`composeServices`
/// CI leg composes, so they must match what the CLI renders *today* — otherwise the
/// leg would validate stale SDL while the live output drifted. This renders both
/// cascade subgraphs through the real CLI path and diffs them against the committed
/// `.graphql` files, failing on any drift. Re-bless after an intentional rendering
/// change with (same env var the core `federation_compose` fixtures use):
///
/// ```sh
/// BLESS_FEDERATION_SDL=1 \
///   cargo test -p fraiseql-cli --test cascade_federation_shareable_e2e --features federation
/// ```
#[test]
fn committed_cascade_sdl_fixtures_are_current() {
    let cases = [
        (
            "cascade_orders.graphql",
            cascade_compose_subgraph_json("orders", "Order", "v_order"),
        ),
        (
            "cascade_users.graphql",
            cascade_compose_subgraph_json("users", "User", "v_user"),
        ),
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
            "missing cascade federation_compose SDL fixture; re-bless with BLESS_FEDERATION_SDL=1",
        );
        assert_eq!(
            committed,
            rendered,
            "{} is stale; re-bless with BLESS_FEDERATION_SDL=1",
            path.display()
        );
    }
}
