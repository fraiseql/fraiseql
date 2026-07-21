#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics are acceptable
//! End-to-end proof that an author-declared embedded value object (#687) compiles under a
//! cascade — the SDK-realistic half of Phase 02, closing the #653 bug.
//!
//! The scenario is exactly what `@fraiseql.type(embedded=True)` emits (Phase 02 SDK side):
//! a `Money` value object with **`embedded: true` and no `sql_source`** (the SDK suppresses
//! the synthesized `v_money`), embedded as `Order.total`, under a `cascade = true`
//! `createOrder` mutation. On v2.13.1 this schema **does not compile at all**: `Money`
//! carries a synthesized source, is classified a cascade entity, and hard-fails the
//! `CascadeNode` `id: ID!` contract on a type that has no identity by design.
//!
//! This drives the whole SDK-shaped compile path (`SchemaConverter::convert`, the same entry
//! the CLI's `compile` command uses) and asserts three things:
//!   (i)   it **compiles clean** (so `validate()` accepts it — it will load and boot);
//!   (ii)  `Money` does **not** implement `CascadeNode` (a value object is never a cache node),
//!         while the genuine `Order` entity does;
//!   (iii) the compiled `Money.embedded == true` — the author's declaration round-trips into
//!         the compiled schema the server re-loads.
//!
//! (iii) is the assertion genuinely RED before Phase 01: serde drops the unknown `embedded`
//! field on a pre-01 compiler, so the flag never reaches the compiled schema. (i)/(ii) are
//! green either way here, because a source-suppressed `Money` is already excluded by the
//! empty-source leg of `is_queryable_entity` — which is the whole point of source-suppression.
//! This is distinct from Phase 01 Cycle 2, which tests the `&& !ty.embedded` guard against a
//! *non-empty-source* contradiction the SDK never emits; here the source is genuinely absent.

use fraiseql_cli::schema::{IntermediateSchema, converter::SchemaConverter};

/// An SDK-shaped `schema.json`: an embedded `Money` value object (no source) under a cascade
/// mutation. This is the exact shape `@fraiseql.type(embedded=True)` produces — mirrored by
/// the `python-sdk-02-embedded.json` golden fixture.
const SDK_SCHEMA_EMBEDDED_UNDER_CASCADE: &str = r#"
{
  "version": "2.0.0",
  "types": [
    {
      "name": "Order",
      "fields": [
        {"name": "id", "type": "ID", "nullable": false},
        {"name": "total", "type": "Money", "nullable": false}
      ],
      "sql_source": "v_order"
    },
    {
      "name": "Money",
      "embedded": true,
      "fields": [
        {"name": "amount", "type": "Int", "nullable": false},
        {"name": "currency", "type": "String", "nullable": false}
      ]
    }
  ],
  "queries": [],
  "mutations": [
    {
      "name": "createOrder",
      "return_type": "Order",
      "cascade": true,
      "sql_source": "fn_create_order",
      "operation": "CREATE"
    }
  ],
  "subscriptions": []
}
"#;

fn compile() -> fraiseql_core::schema::CompiledSchema {
    let intermediate: IntermediateSchema =
        serde_json::from_str(SDK_SCHEMA_EMBEDDED_UNDER_CASCADE).expect("parse SDK schema.json");
    // A successful convert() means the full pipeline ran — including `validate()` after
    // `synthesize_cascade_types` — so the compiled schema is load-valid and the server will
    // boot on it. Before #687 (source-suppression) this returned `Err`: the synthesized
    // `v_money` made `Money` a cascade entity that could not satisfy `id: ID!`.
    SchemaConverter::convert(intermediate)
        .expect("#687: an embedded value object under a cascade must compile")
}

fn implements_cascade_node(
    schema: &fraiseql_core::schema::CompiledSchema,
    type_name: &str,
) -> bool {
    schema
        .types
        .iter()
        .find(|t| t.name.as_str() == type_name)
        .unwrap_or_else(|| panic!("{type_name} must be present"))
        .implements
        .iter()
        .any(|i| i == "CascadeNode")
}

#[test]
fn embedded_value_object_under_cascade_compiles() {
    // (i) The headline unblock: the schema compiles at all (the #653 bug this closes).
    let compiled = compile();

    // The cascade surface was still synthesized for the genuine entity mutation.
    assert!(
        compiled.interfaces.iter().any(|i| i.name == "CascadeNode"),
        "CascadeNode interface synthesized"
    );
    let create_order = compiled
        .mutations
        .iter()
        .find(|m| m.name == "createOrder")
        .expect("createOrder present");
    assert_eq!(
        create_order.return_type, "CreateOrderPayload",
        "cascade rewrites the mutation return type to its payload"
    );
}

#[test]
fn embedded_money_is_not_a_cascade_node_but_order_is() {
    let compiled = compile();

    // (ii) The genuine `Order` entity IS a cascade node — enforcement is unchanged for it.
    assert!(
        implements_cascade_node(&compiled, "Order"),
        "the user's Order entity implements CascadeNode"
    );

    // (ii) The declared value object is exempt: it is delivered inside its parent's payload,
    // never as a cache node of its own, so it must not auto-implement CascadeNode.
    let money = compiled
        .types
        .iter()
        .find(|t| t.name.as_str() == "Money")
        .expect("Money must be present");
    assert!(
        !money.implements.iter().any(|i| i == "CascadeNode"),
        "a value object is never a cascade node: {:?}",
        money.implements
    );
    assert!(
        money.find_field("id").is_none(),
        "no synthetic `id` was forced onto the value object"
    );
}

#[test]
fn compiled_money_carries_the_embedded_flag() {
    let compiled = compile();

    // (iii) The RED assertion: the author's `embedded=True` declaration round-trips into the
    // compiled schema the server re-loads. On a pre-Phase-01 compiler this is false — serde
    // silently drops the unknown `embedded` field. Proven RED by mutation: reverting
    // `embedded: intermediate.embedded` to `embedded: false` in `converter/types.rs` turns
    // this assertion RED while leaving the compile/exemption assertions green.
    let money = compiled
        .types
        .iter()
        .find(|t| t.name.as_str() == "Money")
        .expect("Money must be present");
    assert!(money.embedded, "the compiled schema must carry Money.embedded == true");
}
