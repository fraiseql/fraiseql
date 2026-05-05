//! Helper functions for `OpenAPI` specification generation.
//!
//! Contains utility functions for JSON schema generation, string transformations,
//! and HTTP method mapping used throughout `OpenAPI` spec building.

use fraiseql_core::schema::FieldType;
use serde_json::{json, Value};

use super::super::resource::{HttpMethod, RestRoute};

/// Bracket operators documented in filter parameter descriptions.
pub(super) const BRACKET_OPERATORS_DESC: &str = "eq, ne, gt, gte, lt, lte, in, nin, like, ilike, is_null, contains, icontains, startswith, endswith";

/// Map a `FieldType` to a JSON Schema type object.
pub(super) fn field_type_to_json_schema(ft: &FieldType) -> Value {
    match ft {
        FieldType::Int => json!({ "type": "integer" }),
        FieldType::Float => json!({ "type": "number" }),
        FieldType::Boolean => json!({ "type": "boolean" }),
        FieldType::Id | FieldType::Uuid => json!({ "type": "string", "format": "uuid" }),
        FieldType::DateTime => json!({ "type": "string", "format": "date-time" }),
        FieldType::Date => json!({ "type": "string", "format": "date" }),
        FieldType::Time => json!({ "type": "string", "format": "time" }),
        FieldType::Json => json!({ "type": "object" }),
        FieldType::Decimal => json!({ "type": "string", "format": "decimal" }),
        FieldType::Vector => json!({ "type": "array", "items": { "type": "number" } }),
        FieldType::Scalar(name) => scalar_to_json_schema(name),
        FieldType::List(inner) => {
            json!({ "type": "array", "items": field_type_to_json_schema(inner) })
        },
        FieldType::Object(name) | FieldType::Enum(name) | FieldType::Input(name) => {
            json!({ "$ref": format!("#/components/schemas/{name}") })
        },
        FieldType::Interface(name) | FieldType::Union(name) => {
            json!({ "type": "object", "description": format!("See {name}") })
        },
        // Reason: FieldType is #[non_exhaustive]; default to string for unknown variants.
        _ => json!({ "type": "string" }),
    }
}

/// Map well-known scalar names to JSON Schema.
pub(super) fn scalar_to_json_schema(name: &str) -> Value {
    match name {
        "Email" => json!({ "type": "string", "format": "email" }),
        "URL" | "Uri" => json!({ "type": "string", "format": "uri" }),
        "PhoneNumber" => json!({ "type": "string", "format": "phone" }),
        _ => json!({ "type": "string" }),
    }
}

/// Convert an HTTP method to its string representation.
pub(super) const fn method_to_string(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Patch => "patch",
        HttpMethod::Delete => "delete",
    }
}

/// Determine if a route should include a Prefer header in its `OpenAPI` documentation.
pub(super) fn should_have_prefer_header(route: &RestRoute) -> bool {
    match route.method {
        HttpMethod::Get => {
            // Collection GET endpoints (no path parameter).
            !route.path.contains('{')
        },
        HttpMethod::Post | HttpMethod::Patch | HttpMethod::Delete => true,
        HttpMethod::Put => false,
    }
}

/// Capitalize the first letter of a string.
pub(super) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Convert a string to `snake_case`.
pub(super) fn to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.extend(c.to_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Extract an action name from a mutation name by stripping the type prefix.
///
/// Example: `archiveUser` on type `User` → `archive`
pub(super) fn extract_action(mutation_name: &str, type_name: &str) -> String {
    // Try stripping type name suffix (e.g., `archiveUser` → `archive`).
    let lower_type = type_name.to_lowercase();
    let lower_name = mutation_name.to_lowercase();

    if let Some(prefix) = lower_name.strip_suffix(&lower_type) {
        if !prefix.is_empty() {
            return prefix.trim_end_matches('_').replace('_', "-");
        }
    }

    // Try stripping type name prefix (e.g., `userArchive` → `archive`).
    if let Some(suffix) = lower_name.strip_prefix(&lower_type) {
        let trimmed = suffix.trim_start_matches('_');
        if !trimmed.is_empty() {
            return trimmed.replace('_', "-");
        }
    }

    // Fallback: use the full mutation name kebab-cased.
    to_snake(mutation_name).replace('_', "-")
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code
mod tests {
    use super::*;

    #[test]
    fn capitalize_test() {
        assert_eq!(capitalize("users"), "Users");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("a"), "A");
    }

    #[test]
    fn to_snake_test() {
        assert_eq!(to_snake("Users"), "users");
        assert_eq!(to_snake("UserProfile"), "user_profile");
        assert_eq!(to_snake("API"), "a_p_i");
    }

    #[test]
    fn extract_action_suffix() {
        assert_eq!(extract_action("archiveUser", "User"), "archive");
        assert_eq!(extract_action("deleteUser", "User"), "delete");
    }

    #[test]
    fn extract_action_prefix() {
        assert_eq!(extract_action("userArchive", "User"), "archive");
        assert_eq!(extract_action("userDelete", "User"), "delete");
    }

    #[test]
    fn extract_action_fallback() {
        assert_eq!(extract_action("complexAction", "Other"), "complex-action");
    }

    #[test]
    fn method_to_string_all() {
        assert_eq!(method_to_string(HttpMethod::Get), "get");
        assert_eq!(method_to_string(HttpMethod::Post), "post");
        assert_eq!(method_to_string(HttpMethod::Put), "put");
        assert_eq!(method_to_string(HttpMethod::Patch), "patch");
        assert_eq!(method_to_string(HttpMethod::Delete), "delete");
    }

    #[test]
    fn should_have_prefer_header_get_collection() {
        let mut route = RestRoute {
            method:          HttpMethod::Get,
            path:            "/users".to_string(),
            source:          super::super::super::resource::RouteSource::Query {
                name: "users".to_string(),
            },
            update_coverage: None,
            success_status:  200,
        };
        assert!(should_have_prefer_header(&route));

        route.path = "/users/{id}".to_string();
        assert!(!should_have_prefer_header(&route));
    }

    #[test]
    fn should_have_prefer_header_post() {
        let route = RestRoute {
            method:          HttpMethod::Post,
            path:            "/users".to_string(),
            source:          super::super::super::resource::RouteSource::Mutation {
                name: "createUser".to_string(),
            },
            update_coverage: None,
            success_status:  201,
        };
        assert!(should_have_prefer_header(&route));
    }

    #[test]
    fn should_have_prefer_header_put() {
        let route = RestRoute {
            method:          HttpMethod::Put,
            path:            "/users/{id}".to_string(),
            source:          super::super::super::resource::RouteSource::Mutation {
                name: "updateUser".to_string(),
            },
            update_coverage: None,
            success_status:  200,
        };
        assert!(!should_have_prefer_header(&route));
    }
}
