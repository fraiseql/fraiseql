//! SDL generation for federation _service query.

use super::types::FederationMetadata;

/// Generate federation-compliant SDL for _service query
pub fn generate_service_sdl(base_schema: &str, metadata: &FederationMetadata) -> String {
    if !metadata.enabled {
        return base_schema.to_string();
    }

    let mut sdl = String::new();

    // Inject @key directives inline into type definition headers.
    // Replaces `type Foo {` with `type Foo @key(fields: "id") {` for each federated type.
    let mut modified_schema = base_schema.to_string();
    for fed_type in &metadata.types {
        if !fed_type.keys.is_empty() {
            let key_directives: String = fed_type
                .keys
                .iter()
                .map(|key| {
                    let fields_str = key.fields.join(" ");
                    if key.resolvable {
                        format!("@key(fields: \"{fields_str}\") ")
                    } else {
                        format!("@key(fields: \"{fields_str}\", resolvable: false) ")
                    }
                })
                .collect();

            let old = format!("type {} {{", fed_type.name);
            let new = format!("type {} {}{{", fed_type.name, key_directives);
            modified_schema = modified_schema.replace(&old, &new);
        }
    }

    // Add federation schema directives
    let federation_schema = r"
directive @key(fields: String!, resolvable: Boolean = true) repeatable on OBJECT
directive @extends on OBJECT
directive @external on FIELD_DEFINITION
directive @requires(fields: String!) on FIELD_DEFINITION
directive @provides(fields: String!) on FIELD_DEFINITION
directive @shareable on FIELD_DEFINITION | OBJECT
directive @link(url: String!, as: String, for: String, import: [String]) repeatable on SCHEMA

type _Service {
  sdl: String!
}

scalar _Any
";

    // Build _Entity union with all federated types
    let entity_types: Vec<&str> = metadata.types.iter().map(|t| t.name.as_str()).collect();

    let union_str = if !entity_types.is_empty() {
        format!("union _Entity = {}\n", entity_types.join(" | "))
    } else {
        "union _Entity\n".to_string()
    };

    // Merge federation fields into existing Query type if present,
    // otherwise use `extend type Query`.
    let federation_fields = "  _service: _Service!\n  _entities(representations: [_Any!]!): [_Entity]!\n";
    let has_query_type = modified_schema.contains("type Query {");

    if has_query_type {
        // Insert federation fields before the closing brace of `type Query { ... }`
        modified_schema = modified_schema.replacen(
            "type Query {",
            &format!("type Query {{\n{federation_fields}"),
            1,
        );
        // Remove the double newline that results from the original fields starting with \n
        modified_schema = modified_schema.replace(
            &format!("{federation_fields}\n"),
            federation_fields,
        );
    }

    // Build complete schema
    sdl.push_str(&modified_schema);
    sdl.push_str("\n\n");
    sdl.push_str(federation_schema);
    sdl.push('\n');
    sdl.push_str(&union_str);

    if !has_query_type {
        sdl.push_str("\nextend type Query {\n");
        sdl.push_str(federation_fields);
        sdl.push_str("}\n");
    }

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
    use super::*;

    #[test]
    fn test_generate_service_sdl_empty() {
        let metadata = FederationMetadata::default();
        let base_schema = "type Query { test: String }";

        let sdl = generate_service_sdl(base_schema, &metadata);
        assert_eq!(sdl, base_schema);
    }

    #[test]
    fn test_generate_service_sdl_with_federation() {
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![],
        };

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
        use crate::types::{FederatedType, KeyDirective};

        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             "User".to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec!["id".to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  Vec::new(),
                shareable_fields: Vec::new(),
                field_directives: std::collections::HashMap::new(),
            }],
        };

        let base_schema =
            "type User {\n  id: ID!\n  name: String!\n}\n\ntype Query {\n  user(id: ID!): User\n}";
        let sdl = generate_service_sdl(base_schema, &metadata);

        assert!(sdl.contains("type User @key(fields: \"id\") {"), "SDL: {}", sdl);
        assert!(!sdl.contains("# @key"), "must not contain commented @key: {}", sdl);
    }

    #[test]
    fn test_federation_fields_merged_into_existing_query_type() {
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![],
        };

        let base_schema = "type Query {\n  users: [User!]!\n}\n";
        let sdl = generate_service_sdl(base_schema, &metadata);

        // Federation fields should be merged into the existing Query type
        assert!(
            sdl.contains("_service: _Service!"),
            "SDL should contain _service field:\n{sdl}"
        );
        assert!(
            sdl.contains("_entities(representations: [_Any!]!): [_Entity]!"),
            "SDL should contain _entities field:\n{sdl}"
        );
        // Should NOT have a separate `extend type Query`
        assert!(
            !sdl.contains("extend type Query"),
            "SDL should merge into existing Query, not extend:\n{sdl}"
        );
        // Original fields should still be present
        assert!(
            sdl.contains("users: [User!]!"),
            "SDL should keep original query fields:\n{sdl}"
        );
    }

    #[test]
    fn test_federation_extend_query_when_no_query_type() {
        let metadata = FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![],
        };

        let base_schema = "type User {\n  id: ID!\n}\n";
        let sdl = generate_service_sdl(base_schema, &metadata);

        // Should use `extend type Query` when no Query type exists
        assert!(
            sdl.contains("extend type Query"),
            "SDL should extend Query when none exists:\n{sdl}"
        );
    }
}
