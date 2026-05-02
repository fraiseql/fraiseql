//! SDL generation for federation _service query.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::types::{FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection};

const FEDERATION_SCHEMA: &str = r"
directive @key(fields: String!, resolvable: Boolean = true) repeatable on OBJECT
directive @extends on OBJECT
directive @external on FIELD_DEFINITION
directive @requires(fields: String!) on FIELD_DEFINITION
directive @provides(fields: String!) on FIELD_DEFINITION
directive @shareable on FIELD_DEFINITION | OBJECT
directive @inaccessible on FIELD_DEFINITION | OBJECT | INTERFACE | UNION | ARGUMENT_DEFINITION | SCALAR | ENUM | ENUM_VALUE | INPUT_OBJECT | INPUT_FIELD_DEFINITION
directive @override(from: String!) on FIELD_DEFINITION
directive @link(url: String!, as: String, for: String, import: [String]) repeatable on SCHEMA

type _Service {
  sdl: String!
}

scalar _Any
";

/// Serialize a single field path to a GraphQL field set string.
///
/// `["age"]` → `"age"`, `["profile", "age"]` → `"profile { age }"`.
fn serialize_field_path(path: &[String]) -> String {
    match path.len() {
        0 => String::new(),
        1 => path[0].clone(),
        _ => format!("{} {{ {} }}", path[0], serialize_field_path(&path[1..])),
    }
}

/// Serialize multiple field path selections to a space-joined field set string.
fn serialize_field_selections(selections: &[FieldPathSelection]) -> String {
    selections
        .iter()
        .map(|s| serialize_field_path(&s.path))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the directive suffix for a field based on its federation directives.
///
/// Order: `@external @shareable @inaccessible @override(from:) @requires(fields:) @provides(fields:)`
pub(crate) fn field_directive_suffix(d: &FieldFederationDirectives) -> String {
    let mut parts = Vec::new();

    if d.external {
        parts.push("@external".to_string());
    }
    if d.shareable {
        parts.push("@shareable".to_string());
    }
    if d.inaccessible {
        parts.push("@inaccessible".to_string());
    }
    if let Some(ref from) = d.override_from {
        parts.push(format!("@override(from: \"{from}\")"));
    }
    if !d.requires.is_empty() {
        let fields = serialize_field_selections(&d.requires);
        parts.push(format!("@requires(fields: \"{fields}\")"));
    }
    if !d.provides.is_empty() {
        let fields = serialize_field_selections(&d.provides);
        parts.push(format!("@provides(fields: \"{fields}\")"));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(" {}", parts.join(" "))
    }
}

/// Resolve federation version string to URL path component.
///
/// `"v2"` → `"v2.0"`, `"v2.3"` → `"v2.3"`, anything else passed through.
fn resolve_federation_version(version: &str) -> String {
    if version == "v2" {
        "v2.0".to_string()
    } else {
        version.to_string()
    }
}

/// Parsed type header from a schema line.
struct TypeHeader {
    /// `"type"`, `"interface"`, `"input"`, `"enum"`, `"scalar"`, `"union"`
    kind: String,
    /// The type name (e.g., `"User"`)
    name: String,
}

/// Try to parse a type header from a trimmed line.
///
/// Matches patterns: `type Foo {`, `extend type Foo {`, `interface Foo {`,
/// `input Foo {`, `enum Foo {`, `scalar Foo`, `union Foo`.
fn parse_type_header(trimmed: &str) -> Option<TypeHeader> {
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    match tokens[0] {
        "type" | "interface" | "input" | "enum" | "scalar" | "union" => {
            if tokens.len() >= 2 {
                Some(TypeHeader {
                    kind: tokens[0].to_string(),
                    name: tokens[1].to_string(),
                })
            } else {
                None
            }
        }
        "extend" => {
            if tokens.len() >= 3 {
                Some(TypeHeader {
                    kind: tokens[1].to_string(),
                    name: tokens[2].to_string(),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract the field name from a GraphQL field definition line.
///
/// The field name is the first alphanumeric/underscore token before `:` or `(`.
/// Returns `None` for closing braces, comments, or empty lines.
fn extract_field_name(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "}" || trimmed == "{" {
        return None;
    }

    let name: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    if name.is_empty() { None } else { Some(name) }
}

/// Append directive suffix to a field line, inserting before any trailing comment.
fn append_directives_to_line(line: &str, suffix: &str) -> String {
    // Look for a # comment that isn't inside a string
    if let Some(comment_pos) = line.find(" #") {
        format!("{}{}{}", &line[..comment_pos], suffix, &line[comment_pos..])
    } else {
        format!("{line}{suffix}")
    }
}

/// Generate federation-compliant SDL for `_service` query.
///
/// Uses a state-machine line processor to inject `@key` on type headers and
/// field-level federation directives (`@external`, `@requires`, `@provides`,
/// `@shareable`, `@inaccessible`, `@override`) on individual field definitions.
///
/// # Type handling
///
/// - `is_extends: true` types → emitted as `extend type Foo @key(...) {`
/// - `type_shareable: true` → type header includes `@shareable`
/// - `interface` types with `@key` → entity interface pattern
/// - `input` / `enum` types → no federation directives injected
pub fn generate_service_sdl(base_schema: &str, metadata: &FederationMetadata) -> String {
    if !metadata.enabled {
        return base_schema.to_string();
    }

    // Build lookup map: type name → federated type metadata
    let fed_type_map: HashMap<&str, &FederatedType> = metadata
        .types
        .iter()
        .map(|t| (t.name.as_str(), t))
        .collect();

    // State-machine line processor
    let mut output_lines: Vec<String> = Vec::new();
    let mut current_type_name: Option<String> = None;
    let mut brace_depth: u32 = 0;

    for line in base_schema.lines() {
        let trimmed = line.trim();
        let indent = &line[..line.len() - line.trim_start().len()];

        if brace_depth == 0 {
            if let Some(type_info) = parse_type_header(trimmed) {
                match type_info.kind.as_str() {
                    "input" | "enum" | "scalar" | "union" => {
                        // No federation directives for these types
                        current_type_name = None;
                        output_lines.push(line.to_string());
                    }
                    "type" | "interface" => {
                        if let Some(fed_type) = fed_type_map.get(type_info.name.as_str()) {
                            current_type_name = Some(type_info.name.clone());

                            // Build the type header prefix
                            let prefix = if fed_type.is_extends {
                                format!("extend {}", type_info.kind)
                            } else {
                                type_info.kind.clone()
                            };

                            let mut header = format!("{prefix} {}", type_info.name);

                            // Inject @key directives
                            for key in &fed_type.keys {
                                let fields_str = key.fields.join(" ");
                                if key.resolvable {
                                    let _ = write!(header, " @key(fields: \"{fields_str}\")");
                                } else {
                                    let _ = write!(
                                        header,
                                        " @key(fields: \"{fields_str}\", resolvable: false)"
                                    );
                                }
                            }

                            // Type-level @shareable
                            if fed_type.type_shareable {
                                header.push_str(" @shareable");
                            }

                            header.push_str(" {");
                            output_lines.push(format!("{indent}{header}"));
                        } else {
                            // Not a federated type — pass through unchanged
                            current_type_name = None;
                            output_lines.push(line.to_string());
                        }
                    }
                    _ => {
                        output_lines.push(line.to_string());
                    }
                }
            } else {
                output_lines.push(line.to_string());
            }
        } else if brace_depth == 1 {
            if let Some(ref type_name) = current_type_name {
                if let Some(fed_type) = fed_type_map.get(type_name.as_str()) {
                    if let Some(field_name) = extract_field_name(trimmed) {
                        if let Some(directives) = fed_type.field_directives.get(&field_name) {
                            let suffix = field_directive_suffix(directives);
                            if suffix.is_empty() {
                                output_lines.push(line.to_string());
                            } else {
                                output_lines.push(append_directives_to_line(line, &suffix));
                            }
                        } else {
                            output_lines.push(line.to_string());
                        }
                    } else {
                        output_lines.push(line.to_string());
                    }
                } else {
                    output_lines.push(line.to_string());
                }
            } else {
                output_lines.push(line.to_string());
            }
        } else {
            output_lines.push(line.to_string());
        }

        // Update brace depth after processing the line
        for ch in trimmed.chars() {
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        current_type_name = None;
                    }
                }
                _ => {}
            }
        }
    }

    let modified_schema = output_lines.join("\n");

    // Build complete SDL
    let mut sdl = String::new();

    // @link directive with dynamic version URL
    let version_url = resolve_federation_version(&metadata.version);
    let _ = writeln!(
        sdl,
        "extend schema @link(url: \"https://specs.apollo.dev/federation/{version_url}\", \
         import: [\"@key\", \"@external\", \"@requires\", \"@provides\", \"@shareable\", \
         \"@inaccessible\", \"@override\", \"@extends\"])"
    );

    sdl.push_str(&modified_schema);
    sdl.push_str("\n\n");
    sdl.push_str(FEDERATION_SCHEMA);
    sdl.push('\n');

    // _Entity union
    let entity_types: Vec<&str> = metadata.types.iter().map(|t| t.name.as_str()).collect();
    if entity_types.is_empty() {
        sdl.push_str("union _Entity\n");
    } else {
        let _ = writeln!(sdl, "union _Entity = {}", entity_types.join(" | "));
    }

    sdl.push_str("\nextend type Query {\n");
    sdl.push_str("  _service: _Service!\n");
    sdl.push_str("  _entities(representations: [_Any!]!): [_Entity]!\n");
    sdl.push_str("}\n");

    sdl
}

/// Parse SDL to check if it's valid GraphQL
pub fn validate_sdl(sdl: &str) -> bool {
    // Basic validation - check for required federation elements
    sdl.contains("directive @key")
        && sdl.contains("scalar _Any")
        && sdl.contains("union _Entity")
        && sdl.contains("_service")
        && sdl.contains("_entities")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::*;
    use crate::types::KeyDirective;

    /// Helper: build a basic `FederationMetadata` with federation enabled.
    fn enabled_metadata() -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   Vec::new(),
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
            types:   vec![user_type_with_key()],
            remote_subscription_fields: HashMap::new(),
        };

        let base_schema =
            "type User {\n  id: ID!\n  name: String!\n}\n\ntype Query {\n  user(id: ID!): User\n}";
        let sdl = generate_service_sdl(base_schema, &metadata);

        assert!(
            sdl.contains("type User @key(fields: \"id\") {"),
            "SDL: {sdl}"
        );
        assert!(
            !sdl.contains("# @key"),
            "must not contain commented @key: {sdl}"
        );
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
            sdl.contains(
                "extend schema @link(url: \"https://specs.apollo.dev/federation/v2.0\","
            ),
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
        user.field_directives.insert(
            "email".to_string(),
            FieldFederationDirectives::new().external(),
        );

        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![user],
            remote_subscription_fields: HashMap::new(),
        };

        let base_schema = "type User {\n  id: ID!\n  email: String!\n}";
        let sdl = generate_service_sdl(base_schema, &metadata);

        assert!(
            sdl.contains("email: String! @external"),
            "Field must have @external: {sdl}"
        );
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
            types:   vec![user],
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
            types:   vec![user],
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
            types:   vec![user],
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
        user.field_directives.insert(
            "name".to_string(),
            FieldFederationDirectives::new().shareable(),
        );

        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![user],
            remote_subscription_fields: HashMap::new(),
        };

        let base_schema = "type User {\n  id: ID!\n  name: String!\n}";
        let sdl = generate_service_sdl(base_schema, &metadata);

        assert!(
            sdl.contains("name: String! @shareable"),
            "Field must have @shareable: {sdl}"
        );
    }

    #[test]
    fn test_field_inaccessible_directive() {
        let mut user = user_type_with_key();
        user.field_directives.insert(
            "ssn".to_string(),
            FieldFederationDirectives::new().inaccessible(),
        );

        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![user],
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
            types:   vec![user],
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
            types:   vec![user],
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
            types:   vec![user],
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
            types:   vec![user],
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
            types:   Vec::new(),
            remote_subscription_fields: HashMap::new(),
        };

        let sdl = generate_service_sdl("type Query { x: Int }", &metadata);
        assert!(
            sdl.contains("federation/v2.0\""),
            "v2 must resolve to v2.0: {sdl}"
        );
    }

    #[test]
    fn test_link_url_version_v2_3() {
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2.3".to_string(),
            types:   Vec::new(),
            remote_subscription_fields: HashMap::new(),
        };

        let sdl = generate_service_sdl("type Query { x: Int }", &metadata);
        assert!(
            sdl.contains("federation/v2.3\""),
            "v2.3 must stay as v2.3: {sdl}"
        );
    }

    #[test]
    fn test_link_import_includes_extends() {
        let metadata = enabled_metadata();
        let sdl = generate_service_sdl("type Query { x: Int }", &metadata);

        assert!(
            sdl.contains("\"@extends\""),
            "@extends must be in @link import list: {sdl}"
        );
    }

    #[test]
    fn test_cross_type_field_collision() {
        // Two types both have a field named "name" — directives must only
        // apply to the correct type's field.
        let mut user = user_type_with_key();
        user.field_directives.insert(
            "name".to_string(),
            FieldFederationDirectives::new().shareable(),
        );

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
            types:   vec![user, product],
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
        let product_section = sdl
            .split("type Product")
            .nth(1)
            .expect("Product type must exist in SDL");
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
        user.field_directives.insert(
            "name".to_string(),
            FieldFederationDirectives::new().shareable(),
        );

        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![user],
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
            types:   vec![node],
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
            types:   vec![user],
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
            types:   vec![user],
            remote_subscription_fields: HashMap::new(),
        };

        let base_schema = "type User {\n  id: ID!\n  email: String!\n}";
        let sdl = generate_service_sdl(base_schema, &metadata);

        assert!(
            sdl.contains(
                "type User @key(fields: \"id\") @key(fields: \"email\") {"
            ),
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
            serialize_field_path(&[
                "a".to_string(),
                "b".to_string(),
                "c".to_string()
            ]),
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
}
