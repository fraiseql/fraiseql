//! Golden two-subgraph federation compose test (hermetic layer).
//!
//! Every federation bug we have shipped — the `_service` SDL chain (`NO_QUERIES`,
//! `Unknown type`), dropped `@shareable`/`@external`/`extends` directives, and the
//! `snake_case` change-log surface — shared one root cause: a single subgraph always
//! composed clean, so nothing exercised the path where two subgraphs share a type.
//!
//! This test renders two subgraphs through the *real* chain the server uses —
//! [`CompiledSchema::federation_service_sdl`] — and asserts the SDL invariants each of those bugs
//! individually broke. It is hermetic (no Docker, no composer binary); the
//! companion real-`composeServices` check lives in the federation-compose CI leg.
//!
//! These `catalog`/`reviews` subgraphs are built from `CompiledSchema` builders, which
//! **bypass the cli's cascade synthesis** — so the #698 cascade-envelope `@shareable`
//! fix (which lives in `fraiseql-cli`) cannot be exercised here. Its committed compose
//! fixtures (`cascade_orders.graphql` / `cascade_users.graphql`, beside these) are
//! rendered from the cli converter and lock-stepped by
//! `committed_cascade_sdl_fixtures_are_current` in
//! `crates/fraiseql-cli/tests/cascade_federation_shareable_e2e.rs`.
//!
//! The two subgraphs deliberately share everything a real supergraph shares:
//! - `Money` — a keyless `@shareable` value type defined in *both* subgraphs.
//! - `Product` — an entity *owned* by `catalog` and *extended* by `reviews` (`extend type Product
//!   @key` with an `@external` key field).
//! - a camelCase `catalog` subgraph that also exposes the #149 change-log, so the SDL pins the
//!   naming-convention fix too.

#![cfg(feature = "federation")]
#![allow(clippy::expect_used, clippy::unwrap_used)] // Reason: test code, panics are acceptable.

use std::path::PathBuf;

use fraiseql_core::schema::{
    ChangelogConfig, CompiledSchema, FederationConfig, FederationEntity, FieldDefinition,
    FieldType, NamingConvention, QueryDefinition, TypeDefinition, inject_changelog,
};

/// Render a subgraph's Apollo-Federation `_service { sdl }` exactly as the server
/// does.
fn service_sdl(schema: &CompiledSchema) -> String {
    schema
        .federation_service_sdl()
        .expect("subgraph has an enabled federation block")
}

/// Shared `@shareable` value type — `Money { amount, currency }`. Defined
/// identically in both subgraphs; composition only succeeds if both mark it
/// `@shareable` (otherwise `INVALID_FIELD_SHARING`).
fn money_type() -> TypeDefinition {
    TypeDefinition::new("Money", "public.v_money")
        .with_field(FieldDefinition::new("amount", FieldType::Int))
        .with_field(FieldDefinition::new("currency", FieldType::String))
}

/// `catalog` subgraph — owns `Product @key(id)`, defines the shared `@shareable`
/// `Money`, exposes a root query + the camelCase #149 change-log, and a root query a
/// role gates, returning a type nothing else reaches: both carry the role's `@tag`.
fn subgraph_catalog() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    s.naming_convention = NamingConvention::CamelCase;

    s.types.push(money_type());
    s.types.push(
        // `createdAt: DateTime` forces a non-built-in scalar declaration — the
        // exact gap that made gateways report `Unknown type DateTime` (#495).
        TypeDefinition::new("Product", "public.v_product")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("name", FieldType::String))
            .with_field(FieldDefinition::new("price", FieldType::Object("Money".into())))
            .with_field(FieldDefinition::new("createdAt", FieldType::DateTime)),
    );

    s.queries.push(
        QueryDefinition::new("products", "Product")
            .returning_list()
            .with_sql_source("public.v_product"),
    );

    s.types.push(
        TypeDefinition::new("ProductCost", "public.v_product_cost")
            .with_field(FieldDefinition::new("productId", FieldType::Id))
            .with_field(FieldDefinition::new("unitCost", FieldType::Object("Money".into()))),
    );
    s.queries.push(QueryDefinition {
        requires_role: Some("catalog-admin".to_string()),
        ..QueryDefinition::new("productCosts", "ProductCost")
            .returning_list()
            .with_sql_source("public.v_product_cost")
    });

    s.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        service_name: Some("catalog".to_string()),
        entities: vec![FederationEntity {
            name: "Product".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        shareable_types: vec!["Money".to_string()],
        ..Default::default()
    });

    // The single supergraph owner of the change-log (#497), in camelCase (#498).
    s.changelog = Some(ChangelogConfig {
        expose: true,
        ..Default::default()
    });
    inject_changelog(&mut s);

    s.build_indexes();
    s
}

/// `reviews` subgraph — owns `Review @key(id)`, *extends* `Product` (owned by
/// `catalog`) via an `@external` key, and also defines the shared `@shareable`
/// `Money`.
fn subgraph_reviews() -> CompiledSchema {
    let mut s = CompiledSchema::new();
    s.naming_convention = NamingConvention::CamelCase;

    s.types.push(money_type());
    s.types.push(
        TypeDefinition::new("Review", "public.v_review")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("body", FieldType::String))
            .with_field(FieldDefinition::new("rating", FieldType::Int)),
    );
    s.types.push(
        // The extension stub: `id` is owned elsewhere (`@external`); this subgraph
        // only contributes `reviews`.
        TypeDefinition::new("Product", "public.v_product_reviews")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new(
                "reviews",
                FieldType::list(FieldType::Object("Review".into())),
            )),
    );

    s.queries.push(
        QueryDefinition::new("reviews", "Review")
            .returning_list()
            .with_sql_source("public.v_review"),
    );

    s.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        service_name: Some("reviews".to_string()),
        entities: vec![
            FederationEntity {
                name: "Review".to_string(),
                key_fields: vec!["id".to_string()],
                ..Default::default()
            },
            FederationEntity {
                name: "Product".to_string(),
                key_fields: vec!["id".to_string()],
                extends: true,
                external_fields: vec!["id".to_string()],
                ..Default::default()
            },
        ],
        shareable_types: vec!["Money".to_string()],
        ..Default::default()
    });

    s.build_indexes();
    s
}

/// `reviews`, but *also* exposing the change-log — the #497 misconfiguration: two
/// subgraphs both owning the (non-`@shareable`) `EntityChangeLog` type and
/// `entityChangeLogs` root query. Composing this against `catalog` MUST fail with
/// `INVALID_FIELD_SHARING`; the real-composer CI leg asserts exactly that.
fn subgraph_reviews_with_changelog() -> CompiledSchema {
    let mut s = subgraph_reviews();
    s.changelog = Some(ChangelogConfig {
        expose: true,
        ..Default::default()
    });
    inject_changelog(&mut s);
    s.build_indexes();
    s
}

#[test]
fn catalog_sdl_holds_every_federation_invariant() {
    let sdl = service_sdl(&subgraph_catalog());

    // Federation v2 envelope.
    assert!(sdl.contains("extend schema @link("), "missing @link federation header");
    assert!(sdl.contains("scalar _Any"), "missing _Any scalar");

    // #495 — non-built-in scalars referenced by fields MUST be declared, or a
    // gateway rejects the subgraph with `Unknown type`.
    assert!(sdl.contains("scalar DateTime"), "DateTime referenced but not declared (#495)");

    // #495 — root operations must render, or composition fails with NO_QUERIES.
    assert!(sdl.contains("type Query {"), "no root Query type rendered (#495)");
    assert!(sdl.contains("products"), "root query field missing from Query (#495)");

    // #496 — owned entity carries @key; the shared value type carries a type-level
    // @shareable so both subgraphs can define it.
    assert!(
        sdl.contains("type Product @key(fields: \"id\")"),
        "entity @key not rendered on Product (#496)"
    );
    assert!(
        sdl.contains("type Money @shareable"),
        "shareable value type missing @shareable (#496)"
    );

    // #496 — a keyless @shareable value type must NOT join the _Entity union (an
    // unkeyed union member is an invalid composition).
    assert!(sdl.contains("union _Entity = Product"), "Product missing from _Entity union");
    assert!(!sdl.contains("Money |"), "keyless Money leaked into _Entity union (#496)");
    assert!(!sdl.contains("| Money"), "keyless Money leaked into _Entity union (#496)");

    // #498 — the injected change-log honours the camelCase naming convention.
    assert!(sdl.contains("entityChangeLogs"), "change-log query not camelCased (#498)");
    assert!(sdl.contains("pkEntityChangeLog"), "change-log field not camelCased (#498)");
    assert!(!sdl.contains("entity_change_logs"), "snake_case change-log query leaked (#498)");
    assert!(
        !sdl.contains("pk_entity_change_log"),
        "snake_case change-log field leaked (#498)"
    );
}

#[test]
fn reviews_sdl_extends_the_borrowed_entity() {
    let sdl = service_sdl(&subgraph_reviews());

    // #496 — a borrowed entity renders `extend type … @key` with an `@external` key.
    assert!(
        sdl.contains("extend type Product @key(fields: \"id\")"),
        "extended entity not rendered (#496)"
    );
    assert!(sdl.contains("@external"), "external key field not marked @external (#496)");

    // Its own entity + the shared value type.
    assert!(sdl.contains("type Review @key(fields: \"id\")"), "owned entity @key missing");
    assert!(sdl.contains("type Money @shareable"), "shareable value type missing @shareable");

    // Both keyed entities are resolvable via _entities.
    assert!(sdl.contains("union _Entity ="), "no _Entity union rendered");
    assert!(sdl.contains("Review"), "Review missing from _Entity union");
    assert!(sdl.contains("Product"), "Product missing from _Entity union");
}

/// Path to a committed subgraph SDL fixture.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/federation_compose")
        .join(name)
}

/// The committed subgraph SDL fixtures are what the real-`composeServices` CI leg
/// composes, so they must match what FraiseQL renders *today* — otherwise the leg
/// would validate stale SDL while the live output drifted. This test renders both
/// subgraphs and diffs them against the committed `.graphql` files, failing on any
/// drift. Re-bless after an intentional rendering change with:
///
/// ```sh
/// BLESS_FEDERATION_SDL=1 \
///   cargo test -p fraiseql-core --test federation_compose --features federation
/// ```
#[test]
fn committed_sdl_fixtures_are_current() {
    let cases = [
        ("catalog.graphql", subgraph_catalog()),
        ("reviews.graphql", subgraph_reviews()),
        // Negative fixture: a second change-log owner (composes → INVALID_FIELD_SHARING).
        ("reviews_conflict.graphql", subgraph_reviews_with_changelog()),
    ];
    let bless = std::env::var_os("BLESS_FEDERATION_SDL").is_some();

    for (file, schema) in cases {
        let rendered = format!("{}\n", service_sdl(&schema).trim_end());
        let path = fixture_path(file);

        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &rendered).unwrap();
            continue;
        }

        let committed = std::fs::read_to_string(&path)
            .expect("missing federation_compose SDL fixture; re-bless with BLESS_FEDERATION_SDL=1");
        assert_eq!(
            committed,
            rendered,
            "{} is stale; re-bless with BLESS_FEDERATION_SDL=1",
            path.display()
        );
    }
}

/// The shared `Money` value type must be byte-identical across subgraphs, or
/// Apollo composition rejects the supergraph even with `@shareable`. This guards
/// the field set the two builders emit (a drift here would only surface at real
/// `composeServices`, which the CI leg runs).
#[test]
fn shared_value_type_is_identical_across_subgraphs() {
    let catalog = service_sdl(&subgraph_catalog());
    let reviews = service_sdl(&subgraph_reviews());

    for sdl in [&catalog, &reviews] {
        assert!(sdl.contains("type Money @shareable {"), "Money header drifted");
        assert!(sdl.contains("amount: Int"), "Money.amount drifted");
        assert!(sdl.contains("currency: String"), "Money.currency drifted");
    }
}

/// #1616: `_service { sdl }` declares each query as introspection does: a count sibling as
/// `Int!`, a relay query as its connection with the cursor arguments. The SDL is rendered
/// from the introspection model since #1522, so this pins the agreement for every query kind
/// rather than fixing a divergence.
#[test]
fn the_service_sdl_declares_every_query_kind_as_introspection_does() {
    let mut s = CompiledSchema::new();
    s.types.push(
        TypeDefinition::new("Order", "v_order")
            .with_field(FieldDefinition::new("id", FieldType::Id))
            .with_field(FieldDefinition::new("name", FieldType::String)),
    );
    s.queries.push(
        QueryDefinition::new("orders", "Order")
            .returning_list()
            .with_sql_source("v_order"),
    );
    let mut count = QueryDefinition::new("ordersCount", "Order").with_sql_source("v_order");
    count.returns_count = true;
    s.queries.push(count);
    let mut page = QueryDefinition::new("ordersPage", "Order")
        .returning_list()
        .with_sql_source("v_order");
    page.relay = true;
    s.queries.push(page);
    s.federation = Some(FederationConfig {
        enabled: true,
        version: Some("v2".to_string()),
        service_name: Some("orders".to_string()),
        entities: vec![FederationEntity {
            name: "Order".to_string(),
            key_fields: vec!["id".to_string()],
            ..Default::default()
        }],
        ..Default::default()
    });
    s.build_indexes();

    let sdl = service_sdl(&s);
    let declared = |field: &str| {
        sdl.lines()
            .map(str::trim)
            .find(|l| l.starts_with(&format!("{field}(")) || l.starts_with(&format!("{field}:")))
            .map(ToString::to_string)
            .unwrap_or_default()
    };

    assert!(declared("ordersCount").ends_with(": Int!"), "{}", declared("ordersCount"));
    let page = declared("ordersPage");
    assert!(page.ends_with(": OrderConnection!"), "{page}");
    for argument in ["first: Int", "after: String", "last: Int", "before: String"] {
        assert!(page.contains(argument), "{page}");
    }
}
