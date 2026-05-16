//! SDL generation for federation _service query.

use std::{collections::HashMap, fmt::Write as _};

use super::types::{
    FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection,
};

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
/// Order: `@external @shareable @inaccessible @override(from:) @requires(fields:)
/// @provides(fields:)`
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
        },
        "extend" => {
            if tokens.len() >= 3 {
                Some(TypeHeader {
                    kind: tokens[1].to_string(),
                    name: tokens[2].to_string(),
                })
            } else {
                None
            }
        },
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

    let name: String = trimmed.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();

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
    let fed_type_map: HashMap<&str, &FederatedType> =
        metadata.types.iter().map(|t| (t.name.as_str(), t)).collect();

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
                    },
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
                    },
                    _ => {
                        output_lines.push(line.to_string());
                    },
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
                },
                _ => {},
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
mod tests;
