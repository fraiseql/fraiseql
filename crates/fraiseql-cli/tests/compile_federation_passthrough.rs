#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end coverage for the `federation` block surviving `fraiseql compile`.
//!
//! Regression for the bug where `fraiseql compile` silently dropped the input
//! schema's federation configuration: the SDK emits the block under the top-level
//! `"federation"` key, but `IntermediateSchema` only bound `"federation_config"`,
//! so the compiled subgraph lost all federation metadata (`_service` / SDL gone).
//!
//! These tests exercise both authoring workflows:
//! - **Legacy JSON** (`compile schema.json`): the SDK's `"federation"` key must bind into
//!   `IntermediateSchema.federation_config` and carry through to `CompiledSchema.federation`.
//! - **TOML** (`compile fraiseql.toml`): the `[federation]` section must carry through the merger
//!   into the compiled schema.
//!
//! The headline assertion is a server-readable round-trip: the compiled schema's
//! `federation_metadata()` must produce an Apollo Federation v2 `_service` SDL.

use std::io::Write;

use fraiseql_cli::{
    commands::compile::{CompileOptions, compile_to_schema},
    schema::{IntermediateSchema, converter::SchemaConverter, merger::SchemaMerger},
};
use tempfile::{NamedTempFile, TempDir};

/// A realistic SDK-shaped `schema.json` carrying a federation block exactly as the
/// Python SDK's `export_schema(..., federation=Federation(...))` emits it:
/// top-level `"federation"` key, `version`, and entities shaped as
/// `{ "name": ..., "key_fields": [...] }` (NOT `{type_name, key_fields: "id"}`).
const SDK_SCHEMA_WITH_FEDERATION: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "total", "type": "Float", "nullable": false}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_orders",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": [],
  "federation": {
    "enabled": true,
    "service_name": "orders",
    "version": "v2",
    "entities": [
      {"name": "Order", "key_fields": ["id"]}
    ]
  }
}
"#;

/// The same schema with no federation block at all.
const SDK_SCHEMA_WITHOUT_FEDERATION: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false}
      ],
      "sql_source": "v_orders",
      "is_input": false
    }
  ],
  "queries": [
    {
      "name": "list_orders",
      "return_type": "Order",
      "returns_list": true,
      "sql_source": "v_orders",
      "nullable": false,
      "arguments": []
    }
  ],
  "mutations": [],
  "subscriptions": []
}
"#;

// ---- Legacy JSON workflow: deserialize + convert (CWD-free, parallel-safe) ----

#[test]
fn legacy_json_federation_key_binds_into_intermediate() {
    // The crux of the bug: the SDK's top-level `"federation"` key must bind into
    // `IntermediateSchema.federation_config` (it deserialized only from
    // `"federation_config"` before, so the block silently vanished here).
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    assert!(
        intermediate.federation_config.is_some(),
        "the SDK's `federation` key must bind into IntermediateSchema.federation_config"
    );
}

#[test]
fn legacy_json_federation_carries_into_compiled_schema() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled, "federation must be enabled");
    assert_eq!(fed.service_name.as_deref(), Some("orders"));
    assert_eq!(fed.entities.len(), 1);
    assert_eq!(fed.entities[0].name, "Order");
    assert_eq!(fed.entities[0].key_fields, vec!["id".to_string()]);

    // The compiled JSON must expose the top-level `federation` key (the consumer's
    // `jq 'has("federation")'` check that returned `false` before the fix).
    let json = serde_json::to_value(&compiled).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_some(),
        "serialized compiled schema must have a top-level `federation` key"
    );
}

#[test]
fn legacy_json_federation_produces_apollo_v2_service_sdl() {
    // Server-readable round-trip: compiled federation → `_service { sdl }` emits a
    // proper Apollo Federation v2 SDL with `@key` directives on the entity.
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITH_FEDERATION).expect("parse SDK schema.json");
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");

    let metadata = compiled
        .federation_metadata()
        .expect("federation_metadata must be Some when federation is enabled");
    let sdl = fraiseql_core::federation::generate_service_sdl(&compiled.raw_schema(), &metadata);

    assert!(
        sdl.contains("https://specs.apollo.dev/federation/v2.0"),
        "SDL must @link the Apollo Federation v2 spec, got:\n{sdl}"
    );
    assert!(
        sdl.contains("@key(fields: \"id\")"),
        "SDL must carry the Order entity's @key directive, got:\n{sdl}"
    );
    // The subgraph SDL must expose its root operations, or a gateway composes a
    // subgraph with no root fields and fails with NO_QUERIES.
    assert!(sdl.contains("type Query {"), "SDL must declare a root Query type, got:\n{sdl}");
    assert!(
        sdl.contains("[Order"),
        "SDL must expose the list-orders root query returning Order, got:\n{sdl}"
    );
}

#[test]
fn legacy_json_absent_federation_stays_absent() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_WITHOUT_FEDERATION).expect("parse SDK schema.json");
    assert!(intermediate.federation_config.is_none());

    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    assert!(compiled.federation.is_none(), "no federation block ⇒ no compiled federation");
    assert!(compiled.federation_metadata().is_none());

    let json = serde_json::to_value(&compiled).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_none(),
        "absent federation must not synthesize a `federation` key in the compiled JSON"
    );
}

#[test]
fn legacy_json_malformed_federation_fails_loudly() {
    // A non-empty federation block that cannot be carried must fail the compile,
    // not silently vanish: `entities` typed as a string is structurally invalid.
    let malformed = r#"
    {
      "version": "2.0.0",
      "types": [
        {"name": "Order", "fields": [{"name": "id", "type": "ID", "nullable": false}],
         "sql_source": "v_orders", "is_input": false}
      ],
      "queries": [],
      "mutations": [],
      "subscriptions": [],
      "federation": {"enabled": true, "entities": "not-an-array"}
    }
    "#;
    let intermediate: IntermediateSchema =
        serde_json::from_str(malformed).expect("federation_config is untyped JSON, so this parses");
    let err = SchemaConverter::convert(intermediate)
        .expect_err("a structurally invalid federation block must fail conversion, not vanish");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("federation"),
        "the error must point at the federation block, got: {msg}"
    );
}

// ---- TOML workflow: the merger must carry `[federation]` through ----

#[test]
fn toml_federation_carries_through_merger_and_converter() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "orders"

        [types.Order]
        sql_source = "v_orders"
        fields.id = { type = "ID" }

        [federation]
        enabled = true
        apollo_version = 2

        [[federation.entities]]
        name = "Order"
        key_fields = ["id"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    assert!(
        intermediate.federation_config.is_some(),
        "the merger must carry `[federation]` into IntermediateSchema.federation_config"
    );

    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled);
    assert_eq!(fed.entities.len(), 1);
    assert_eq!(fed.entities[0].name, "Order");
    assert_eq!(fed.entities[0].key_fields, vec!["id".to_string()]);
}

#[test]
fn toml_federation_service_name_and_version_reach_compiled_schema() {
    // The `[federation]` section must be able to name the subgraph and pin the spec
    // version — these used to be rejected by `deny_unknown_fields` and, even once
    // accepted, were dropped on the way to the compiled schema.
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "orders"

        [types.Order]
        sql_source = "v_orders"
        fields.id = { type = "ID" }

        [federation]
        enabled = true
        service_name = "orders"
        version = "v2"

        [[federation.entities]]
        name = "Order"
        key_fields = ["id"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");
    assert_eq!(fed.service_name.as_deref(), Some("orders"));
    assert_eq!(fed.version.as_deref(), Some("v2"));

    // Server-readable round-trip: the named subgraph serves an Apollo Fed v2 SDL.
    let metadata = compiled.federation_metadata().expect("federation metadata");
    let sdl = fraiseql_core::federation::generate_service_sdl(&compiled.raw_schema(), &metadata);
    assert!(sdl.contains("https://specs.apollo.dev/federation/v2.0"), "got:\n{sdl}");
    assert!(sdl.contains("@key(fields: \"id\")"), "got:\n{sdl}");
}

// ---- Real CLI pipeline: `compile_to_schema` (mutates CWD; keep it alone) ----

#[tokio::test]
async fn compile_to_schema_legacy_json_carries_federation_end_to_end() {
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(dir.path().join("schema.json"), SDK_SCHEMA_WITH_FEDERATION)
        .expect("write schema.json");

    let original = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(dir.path()).expect("chdir into temp dir");
    let result = compile_to_schema(CompileOptions::new("schema.json")).await;
    std::env::set_current_dir(original).expect("restore cwd");

    let schema = result.expect("compile must succeed").0.schema;
    let fed = schema.federation.as_ref().expect("compiled schema must carry federation");
    assert!(fed.enabled);
    assert_eq!(fed.service_name.as_deref(), Some("orders"));

    let json = serde_json::to_value(&schema).expect("serialize compiled schema");
    assert!(
        json.get("federation").is_some(),
        "the real compile pipeline must write a top-level `federation` key"
    );
}

/// The TOML surface must be able to declare an EXTENDED entity (#1195).
///
/// The compiled schema and the SDL renderer have always supported
/// `extend type User @key(fields: "id") { id: ID! @external }` — `FederationEntity`
/// carries `extends` / `external_fields` / `shareable_fields`, and `service_sdl`
/// renders them. The TOML struct declared only `name` and `key_fields`, so every
/// other spelling was refused at parse and the capability was unreachable from the
/// workflow every in-repo example uses.
///
/// Asserts the rendered SDL, not the struct: a test that only checks the compiled
/// fields cannot see a renderer that ignores them.
#[test]
fn toml_federation_declares_an_extended_entity_and_renders_it() {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(
        br#"
        [schema]
        name = "orders"

        [types.Order]
        sql_source = "v_orders"
        fields.id = { type = "ID" }

        [types.User]
        sql_source = "v_users"
        fields.id = { type = "ID" }
        fields.email = { type = "String" }

        [federation]
        enabled = true
        service_name = "orders"
        version = "v2"
        shareable_types = ["Money"]

        [[federation.entities]]
        name = "Order"
        key_fields = ["id"]

        [[federation.entities]]
        name = "User"
        key_fields = ["id"]
        extends = true
        external_fields = ["id"]
        shareable_fields = ["email"]
    "#,
    )
    .unwrap();
    f.flush().unwrap();

    let intermediate = SchemaMerger::merge_toml_only(f.path().to_str().unwrap()).unwrap();
    let compiled = SchemaConverter::convert(intermediate).expect("convert to compiled schema");
    let fed = compiled.federation.as_ref().expect("compiled schema must carry federation");

    let user = fed.entities.iter().find(|e| e.name == "User").expect("User entity");
    assert!(user.extends, "`extends = true` must reach the compiled entity");
    assert_eq!(user.external_fields, vec!["id".to_string()]);
    assert_eq!(user.shareable_fields, vec!["email".to_string()]);
    assert_eq!(fed.shareable_types, vec!["Money".to_string()]);

    let order = fed.entities.iter().find(|e| e.name == "Order").expect("Order entity");
    assert!(!order.extends, "an entity that declares nothing stays a plain owned entity");

    let metadata = compiled.federation_metadata().expect("federation metadata");
    let sdl = fraiseql_core::federation::generate_service_sdl(&compiled.raw_schema(), &metadata);
    assert!(
        sdl.contains("extend type User"),
        "an extended entity must render as `extend type`, got:\n{sdl}"
    );
}

/// The compiled `federation` block refuses a key it does not know (#1395): the
/// 2.15.0 Python SDK emitted `apollo_version`, which the core struct dropped in
/// silence, and the dead `federation.types` shape was dropped the same way.
#[test]
fn legacy_json_unknown_federation_key_fails_loudly() {
    for unknown in [r#""apollo_version": 2"#, r#""types": []"#] {
        let doc = SDK_SCHEMA_WITH_FEDERATION
            .replace(r#""version": "v2","#, &format!(r#""version": "v2", {unknown},"#));
        let intermediate: IntermediateSchema = serde_json::from_str(&doc).unwrap();
        let err = SchemaConverter::convert(intermediate)
            .expect_err("an unknown federation key must fail the compile");
        let chain = format!("{err:#}");
        let key = unknown.split('"').nth(1).unwrap();
        assert!(chain.contains(&format!("unknown field `{key}`")), "{key}: {chain}");
    }
}

/// `SchemaConverter::convert` reports a bad key itself, in the same report as every
/// other schema problem (#1395) — not only when `compile` reloads its own output.
#[test]
fn the_converter_reports_an_unknown_key_field_with_the_other_schema_problems() {
    let doc = SDK_SCHEMA_WITH_FEDERATION
        .replace(r#""key_fields": ["id"]"#, r#""key_fields": ["id", "region"]"#)
        .replace(r#""return_type": "Order","#, r#""return_type": "Ordr","#);
    let intermediate: IntermediateSchema = serde_json::from_str(&doc).unwrap();
    let err = format!("{:#}", SchemaConverter::convert(intermediate).unwrap_err());
    assert!(err.contains("2 problems"), "both problems in one report: {err}");
    assert!(err.contains("'Order'") && err.contains("'region'"), "{err}");
    assert!(err.contains("'Ordr'"), "{err}");
}

/// An output field of the federated `Order` declaring `extra` alongside its name.
fn order_with_card_field(extra: &str) -> String {
    SDK_SCHEMA_WITH_FEDERATION.replace(
        r#"{"name": "total", "type": "Float", "nullable": false}"#,
        &format!(
            r#"{{"name": "total", "type": "Float", "nullable": false}},
        {{"name": "card_number", "type": "String", "nullable": false, {extra}}}"#
        ),
    )
}

/// What the compile does with `card_number`: refuses it, or the line the supergraph SDL
/// renders for it.
fn card_number_outcome(doc: &str) -> Result<String, String> {
    let intermediate: IntermediateSchema =
        serde_json::from_str(doc).map_err(|e| format!("refused at parse: {e}"))?;
    let compiled = SchemaConverter::convert(intermediate).map_err(|e| format!("{e:#}"))?;
    let sdl = compiled.federation_service_sdl().expect("federation is enabled");
    Ok(sdl
        .lines()
        .find(|l| l.trim_start().starts_with("card_number"))
        .unwrap_or_else(|| panic!("card_number is not in the SDL:\n{sdl}"))
        .trim()
        .to_string())
}

const INACCESSIBLE: &str = r#""directives": [{"name": "inaccessible"}]"#;

/// An `@inaccessible` output field is hidden from the supergraph, or the compile refuses
/// it. Rendering it without the directive publishes a field the author hid.
#[test]
fn an_inaccessible_output_field_is_never_rendered_visible() {
    match card_number_outcome(&order_with_card_field(INACCESSIBLE)) {
        Ok(line) => assert!(line.contains("@inaccessible"), "rendered `{line}`"),
        Err(refusal) => assert!(refusal.contains("inaccessible"), "refused, but: {refusal}"),
    }
}

/// The hidden field reaches what the introspection filter and `_entities` read, and
/// survives the compiled artifact's own round trip.
#[test]
fn an_inaccessible_field_is_carried_to_the_federation_metadata_through_the_artifact() {
    let intermediate: IntermediateSchema =
        serde_json::from_str(&order_with_card_field(INACCESSIBLE)).unwrap();
    let compiled = SchemaConverter::convert(intermediate).unwrap();
    let reloaded =
        fraiseql_core::schema::CompiledSchema::from_json(&compiled.to_json().unwrap(), false)
            .expect("the compiler's own output loads");

    let metadata = reloaded.federation_metadata().expect("federation metadata");
    let order = metadata.types.iter().find(|t| t.name == "Order").expect("Order is federated");
    assert_eq!(order.inaccessible_fields, ["card_number"]);
    assert!(order.field_is_inaccessible("card_number"));
    assert!(!order.keys.is_empty(), "the entity keeps its @key");
}

/// On a type that is not an entity, the field is hidden too, and the type does not
/// become one.
#[test]
fn an_inaccessible_field_of_a_non_entity_type_is_hidden() {
    let doc = SDK_SCHEMA_WITH_FEDERATION.replace(
        r#"      "sql_source": "v_orders",
      "is_input": false
    }
  ],"#,
        r#"      "sql_source": "v_orders",
      "is_input": false
    },
    {
      "name": "Receipt",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "card_number", "type": "String", "nullable": false,
         "directives": [{"name": "inaccessible"}]}
      ],
      "sql_source": "v_receipts",
      "is_input": false
    }
  ],"#,
    );
    let compiled = SchemaConverter::convert(serde_json::from_str(&doc).unwrap()).unwrap();
    let sdl = compiled.federation_service_sdl().unwrap();
    let receipt_block: String = sdl
        .split("type Receipt")
        .nth(1)
        .unwrap_or_else(|| panic!("Receipt is rendered:\n{sdl}"))
        .lines()
        .take_while(|l| !l.starts_with('}'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(receipt_block.contains("card_number: String! @inaccessible"), "{receipt_block}");
    assert!(!receipt_block.lines().next().unwrap().contains("@key"), "{receipt_block}");
    assert!(sdl.contains("union _Entity = Order\n"), "Receipt is no entity:\n{sdl}");
}

/// Without federation there is no supergraph to hide the field from, so the directive
/// would hide nothing: refused, naming the field.
#[test]
fn an_inaccessible_field_without_federation_is_refused() {
    let doc =
        order_with_card_field(INACCESSIBLE).replace(r#""enabled": true"#, r#""enabled": false"#);
    let error = SchemaConverter::convert(serde_json::from_str(&doc).unwrap()).unwrap_err();
    let chain = format!("{error:#}");
    assert!(chain.contains("Order.card_number") && chain.contains("federation"), "{chain}");
}

/// On an interface field nothing compiled carries `@inaccessible`: refused, not dropped.
#[test]
fn an_inaccessible_interface_field_is_refused() {
    let doc = SDK_SCHEMA_WITH_FEDERATION.replace(
        r#"  "mutations": [],"#,
        r#"  "mutations": [],
  "interfaces": [{"name": "Payable", "fields": [
    {"name": "card_number", "type": "String", "nullable": false,
     "directives": [{"name": "inaccessible"}]}]}],"#,
    );
    let error = SchemaConverter::convert(serde_json::from_str(&doc).unwrap()).unwrap_err();
    let chain = format!("{error:#}");
    assert!(chain.contains("card_number") && chain.contains("@inaccessible"), "{chain}");
}

/// A `@tag` on an output field reaches the SDL, or the compile refuses it.
#[test]
fn a_tagged_output_field_is_never_rendered_untagged() {
    let extra = r#""directives": [{"name": "tag", "arguments": {"name": "pci"}}]"#;
    match card_number_outcome(&order_with_card_field(extra)) {
        Ok(line) => assert!(line.contains("@tag"), "rendered `{line}`"),
        Err(refusal) => assert!(refusal.contains("tag"), "refused, but: {refusal}"),
    }
}

/// The served schema's introspection omits the hidden field: the filter the executor
/// applies is fed from what the compiler now carries.
#[tokio::test]
async fn introspection_omits_an_inaccessible_field() {
    use std::sync::Arc;

    use fraiseql_core::runtime::Executor;
    use fraiseql_test_utils::failing_adapter::FailingAdapter;

    let intermediate: IntermediateSchema =
        serde_json::from_str(&order_with_card_field(INACCESSIBLE)).unwrap();
    let executor = Executor::new(
        SchemaConverter::convert(intermediate).unwrap(),
        Arc::new(FailingAdapter::new()),
    );

    let result = executor
        .execute(r#"{ __type(name: "Order") { fields { name } } }"#, None)
        .await
        .expect("introspection runs");
    let names: Vec<&str> = result["data"]["__type"]["fields"]
        .as_array()
        .unwrap_or_else(|| panic!("{result}"))
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"total"), "{names:?}");
    assert!(!names.contains(&"card_number"), "{names:?}");
}

/// `@tenant_id` on an object type's field is read by tenancy validation before
/// conversion, so it compiles; on an interface field nothing reads it, so it is refused.
#[test]
fn tenant_id_compiles_on_an_object_type_field_and_is_refused_on_an_interface_field() {
    let on_type = order_with_card_field(r#""directives": [{"name": "tenant_id"}]"#);
    assert!(SchemaConverter::convert(serde_json::from_str(&on_type).unwrap()).is_ok());

    let on_interface = SDK_SCHEMA_WITH_FEDERATION.replace(
        r#"  "mutations": [],"#,
        r#"  "mutations": [],
  "interfaces": [{"name": "Owned", "fields": [
    {"name": "tenant_id", "type": "String", "nullable": false,
     "directives": [{"name": "tenant_id"}]}]}],"#,
    );
    let error = SchemaConverter::convert(serde_json::from_str(&on_interface).unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("@tenant_id"), "{error:#}");
}
