#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

use super::*;
use crate::types::KeyDirective;

/// Helper: build a basic `FederationMetadata` with federation enabled.
fn enabled_metadata() -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: Vec::new(),
        remote_subscription_fields: HashMap::new(),
    }
}

/// Helper: build a `FederatedType` with a single `@key(fields: "id")`.
fn user_type_with_key() -> FederatedType {
    FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    }
}

// --- Existing tests (must remain green) ---

#[test]
fn test_generate_service_sdl_empty() {
    let metadata = FederationMetadata::default();
    let base_schema = "type Query { test: String }";

    let sdl = generate_service_sdl(base_schema, &metadata);
    assert_eq!(sdl, base_schema);
}

#[test]
fn test_generate_service_sdl_with_federation() {
    let metadata = enabled_metadata();
    let base_schema = "type Query { test: String }";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(sdl.contains("directive @key"));
    assert!(sdl.contains("scalar _Any"));
    assert!(sdl.contains("union _Entity"));
    assert!(sdl.contains("_service"));
    assert!(sdl.contains("_entities"));
}

#[test]
fn test_validate_sdl() {
    let valid_sdl = r"
        directive @key(fields: String!) on OBJECT
        scalar _Any
        union _Entity
        type _Service { sdl: String! }
        extend type Query {
            _service: _Service!
            _entities(representations: [_Any!]!): [_Entity]!
        }
        ";

    assert!(validate_sdl(valid_sdl));
}

#[test]
fn test_validate_sdl_invalid() {
    let invalid_sdl = "type Query { test: String }";
    assert!(!validate_sdl(invalid_sdl));
}

#[test]
fn test_key_directives_emitted_inline() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user_type_with_key()],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema =
        "type User {\n  id: ID!\n  name: String!\n}\n\ntype Query {\n  user(id: ID!): User\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(sdl.contains("type User @key(fields: \"id\") {"), "SDL: {sdl}");
    assert!(!sdl.contains("# @key"), "must not contain commented @key: {sdl}");
}

#[test]
fn test_inaccessible_directive_declared_in_sdl() {
    let metadata = enabled_metadata();
    let base_schema = "type Query { test: String }";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("directive @inaccessible on"),
        "SDL must declare @inaccessible directive: {sdl}",
    );
}

#[test]
fn test_link_directive_emitted_in_sdl() {
    let metadata = enabled_metadata();
    let base_schema = "type Query { test: String }";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("extend schema @link(url: \"https://specs.apollo.dev/federation/v2.0\","),
        "SDL must emit @link for federation v2 spec import: {sdl}",
    );
}

#[test]
fn test_override_directive_declared_in_sdl() {
    let metadata = enabled_metadata();
    let base_schema = "type Query { test: String }";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("directive @override(from: String!) on FIELD_DEFINITION"),
        "SDL must declare @override directive: {sdl}",
    );
}

// --- field-level directive emission ---

#[test]
fn test_field_external_directive() {
    let mut user = user_type_with_key();
    user.field_directives
        .insert("email".to_string(), FieldFederationDirectives::new().external());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  email: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(sdl.contains("email: String! @external"), "Field must have @external: {sdl}");
}

#[test]
fn test_field_requires_directive() {
    let mut user = user_type_with_key();
    user.field_directives.insert(
        "fullName".to_string(),
        FieldFederationDirectives::new().with_requires(vec![FieldPathSelection {
            path:     vec!["profile".to_string()],
            typename: "User".to_string(),
        }]),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  fullName: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("fullName: String! @requires(fields: \"profile\")"),
        "Field must have @requires: {sdl}"
    );
}

#[test]
fn test_field_requires_nested_path() {
    let mut user = user_type_with_key();
    user.field_directives.insert(
        "displayAge".to_string(),
        FieldFederationDirectives::new().with_requires(vec![FieldPathSelection {
            path:     vec!["profile".to_string(), "age".to_string()],
            typename: "User".to_string(),
        }]),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  displayAge: Int!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("@requires(fields: \"profile { age }\")"),
        "Nested path must serialize correctly: {sdl}"
    );
}

#[test]
fn test_field_provides_directive() {
    let mut user = user_type_with_key();
    user.field_directives.insert(
        "reviews".to_string(),
        FieldFederationDirectives::new().with_provides(vec![FieldPathSelection {
            path:     vec!["body".to_string()],
            typename: "Review".to_string(),
        }]),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  reviews: [Review!]!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("reviews: [Review!]! @provides(fields: \"body\")"),
        "Field must have @provides: {sdl}"
    );
}

#[test]
fn test_field_shareable_directive() {
    let mut user = user_type_with_key();
    user.field_directives
        .insert("name".to_string(), FieldFederationDirectives::new().shareable());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  name: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(sdl.contains("name: String! @shareable"), "Field must have @shareable: {sdl}");
}

#[test]
fn test_field_inaccessible_directive() {
    let mut user = user_type_with_key();
    user.field_directives
        .insert("ssn".to_string(), FieldFederationDirectives::new().inaccessible());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  ssn: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("ssn: String! @inaccessible"),
        "Field must have @inaccessible: {sdl}"
    );
}

#[test]
fn test_field_override_directive() {
    let mut user = user_type_with_key();
    user.field_directives.insert(
        "price".to_string(),
        FieldFederationDirectives::new().with_override_from("old-subgraph".to_string()),
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  price: Float!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("price: Float! @override(from: \"old-subgraph\")"),
        "Field must have @override: {sdl}"
    );
}

#[test]
fn test_multi_directive_field() {
    let mut user = user_type_with_key();
    user.field_directives.insert(
        "email".to_string(),
        FieldFederationDirectives {
            external:      true,
            shareable:     false,
            inaccessible:  false,
            override_from: None,
            requires:      vec![FieldPathSelection {
                path:     vec!["id".to_string()],
                typename: "User".to_string(),
            }],
            provides:      Vec::new(),
        },
    );

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  email: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    // Directive order: @external before @requires
    assert!(
        sdl.contains("email: String! @external @requires(fields: \"id\")"),
        "Multi-directive field must emit all directives in order: {sdl}"
    );
}

#[test]
fn test_extends_type_uses_extend_keyword() {
    let user = FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          true,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("extend type User @key(fields: \"id\") {"),
        "is_extends must produce 'extend type': {sdl}"
    );
    assert!(
        !sdl.contains("type User @extends"),
        "Must NOT use @extends on type header: {sdl}"
    );
}

#[test]
fn test_type_level_shareable() {
    let user = FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      true,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("type User @key(fields: \"id\") @shareable {"),
        "Type-level @shareable must be in type header: {sdl}"
    );
}

#[test]
fn test_link_url_version_v2() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: Vec::new(),
        remote_subscription_fields: HashMap::new(),
    };

    let sdl = generate_service_sdl("type Query { x: Int }", &metadata);
    assert!(sdl.contains("federation/v2.0\""), "v2 must resolve to v2.0: {sdl}");
}

#[test]
fn test_link_url_version_v2_3() {
    let metadata = FederationMetadata {
        enabled: true,
        version: "v2.3".to_string(),
        types: Vec::new(),
        remote_subscription_fields: HashMap::new(),
    };

    let sdl = generate_service_sdl("type Query { x: Int }", &metadata);
    assert!(sdl.contains("federation/v2.3\""), "v2.3 must stay as v2.3: {sdl}");
}

#[test]
fn test_link_import_includes_extends() {
    let metadata = enabled_metadata();
    let sdl = generate_service_sdl("type Query { x: Int }", &metadata);

    assert!(sdl.contains("\"@extends\""), "@extends must be in @link import list: {sdl}");
}

#[test]
fn test_cross_type_field_collision() {
    // Two types both have a field named "name" — directives must only
    // apply to the correct type's field.
    let mut user = user_type_with_key();
    user.field_directives
        .insert("name".to_string(), FieldFederationDirectives::new().shareable());

    let product = FederatedType {
        name:                "Product".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["sku".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user, product],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "\
type User {\n  id: ID!\n  name: String!\n}\n\n\
type Product {\n  sku: ID!\n  name: String!\n}";

    let sdl = generate_service_sdl(base_schema, &metadata);

    // User.name must have @shareable
    assert!(
        sdl.contains("type User @key(fields: \"id\") {\n  id: ID!\n  name: String! @shareable"),
        "User.name must have @shareable: {sdl}"
    );
    // Product.name must NOT have @shareable (no directives set on it)
    let product_section = sdl.split("type Product").nth(1).expect("Product type must exist in SDL");
    assert!(
        product_section.contains("name: String!\n"),
        "Product.name must NOT have directives: {sdl}"
    );
}

#[test]
fn test_input_type_fields_no_directives() {
    let mut user = user_type_with_key();
    // Even if a field_directives entry existed for an input type field name,
    // input types should never get federation directives.
    user.field_directives
        .insert("name".to_string(), FieldFederationDirectives::new().shareable());

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "\
type User {\n  id: ID!\n  name: String!\n}\n\n\
input CreateUserInput {\n  name: String!\n}";

    let sdl = generate_service_sdl(base_schema, &metadata);

    // The input type line must be untouched
    assert!(
        sdl.contains("input CreateUserInput {\n  name: String!\n}"),
        "Input type fields must not get federation directives: {sdl}"
    );
}

#[test]
fn test_interface_entity_pattern() {
    let node = FederatedType {
        name:                "Node".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: true,
        }],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![node],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "interface Node {\n  id: ID!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("interface Node @key(fields: \"id\") {"),
        "Interface entity must have @key on interface header: {sdl}"
    );
}

#[test]
fn test_non_resolvable_key() {
    let user = FederatedType {
        name:                "User".to_string(),
        keys:                vec![KeyDirective {
            fields:     vec!["id".to_string()],
            resolvable: false,
        }],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("@key(fields: \"id\", resolvable: false)"),
        "Non-resolvable key must include resolvable: false: {sdl}"
    );
}

#[test]
fn test_multiple_keys_on_one_type() {
    let user = FederatedType {
        name:                "User".to_string(),
        keys:                vec![
            KeyDirective {
                fields:     vec!["id".to_string()],
                resolvable: true,
            },
            KeyDirective {
                fields:     vec!["email".to_string()],
                resolvable: true,
            },
        ],
        is_extends:          false,
        external_fields:     Vec::new(),
        shareable_fields:    Vec::new(),
        inaccessible_fields: Vec::new(),
        field_directives:    HashMap::new(),
        type_shareable:      false,
    };

    let metadata = FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types: vec![user],
        remote_subscription_fields: HashMap::new(),
    };

    let base_schema = "type User {\n  id: ID!\n  email: String!\n}";
    let sdl = generate_service_sdl(base_schema, &metadata);

    assert!(
        sdl.contains("type User @key(fields: \"id\") @key(fields: \"email\") {"),
        "Multiple keys must be emitted: {sdl}"
    );
}

// --- Helper function unit tests ---

#[test]
fn test_serialize_field_path_single() {
    assert_eq!(serialize_field_path(&["age".to_string()]), "age");
}

#[test]
fn test_serialize_field_path_nested() {
    assert_eq!(
        serialize_field_path(&["profile".to_string(), "age".to_string()]),
        "profile { age }"
    );
}

#[test]
fn test_serialize_field_path_deeply_nested() {
    assert_eq!(
        serialize_field_path(&["a".to_string(), "b".to_string(), "c".to_string()]),
        "a { b { c } }"
    );
}

#[test]
fn test_serialize_field_path_empty() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(serialize_field_path(&empty), "");
}

#[test]
fn test_field_directive_suffix_empty() {
    let d = FieldFederationDirectives::default();
    assert_eq!(field_directive_suffix(&d), "");
}

#[test]
fn test_field_directive_suffix_all() {
    let d = FieldFederationDirectives {
        external:      true,
        shareable:     true,
        inaccessible:  true,
        override_from: Some("old".to_string()),
        requires:      vec![FieldPathSelection {
            path:     vec!["id".to_string()],
            typename: "X".to_string(),
        }],
        provides:      vec![FieldPathSelection {
            path:     vec!["name".to_string()],
            typename: "Y".to_string(),
        }],
    };

    let suffix = field_directive_suffix(&d);
    assert_eq!(
        suffix,
        " @external @shareable @inaccessible @override(from: \"old\") \
             @requires(fields: \"id\") @provides(fields: \"name\")"
    );
}

#[test]
fn test_resolve_federation_version() {
    assert_eq!(resolve_federation_version("v2"), "v2.0");
    assert_eq!(resolve_federation_version("v2.3"), "v2.3");
    assert_eq!(resolve_federation_version("v2.5"), "v2.5");
}

#[test]
fn link_directive_definition_uses_link_purpose_enum() {
    // The `@link` directive's `for` argument must be the `link__Purpose` enum, not
    // `String` — federation composition rejects `for: String`. The supporting
    // `link__Purpose` enum must be defined alongside it.
    let sdl = generate_service_sdl("type Query { test: String }", &enabled_metadata());
    assert!(sdl.contains("for: link__Purpose"), "@link `for` must be link__Purpose:\n{sdl}");
    assert!(!sdl.contains("for: String"), "@link must not declare `for: String`:\n{sdl}");
    assert!(sdl.contains("enum link__Purpose"), "link__Purpose enum must be defined:\n{sdl}");
}
