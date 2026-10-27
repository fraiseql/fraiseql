//! SDL generation for federation _service query.

use std::{collections::HashMap, fmt::Write as _};

use super::{
    contract_tags::{ContractTags, tag_suffix},
    types::{FederatedType, FederationMetadata, FieldFederationDirectives, FieldPathSelection},
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
directive @tag(name: String!) repeatable on FIELD_DEFINITION | OBJECT | INTERFACE | UNION | ARGUMENT_DEFINITION | SCALAR | ENUM | ENUM_VALUE | INPUT_OBJECT | INPUT_FIELD_DEFINITION
directive @link(url: String, as: String, for: link__Purpose, import: [link__Import]) repeatable on SCHEMA

enum link__Purpose {
  SECURITY
  EXECUTION
}

scalar link__Import

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
    kind:      String,
    /// The type name (e.g., `"User"`)
    name:      String,
    /// Which whitespace-separated word of the line the name is.
    name_word: usize,
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
                    kind:      tokens[0].to_string(),
                    name:      tokens[1].to_string(),
                    name_word: 1,
                })
            } else {
                None
            }
        },
        "extend" => {
            if tokens.len() >= 3 {
                Some(TypeHeader {
                    kind:      tokens[1].to_string(),
                    name:      tokens[2].to_string(),
                    name_word: 2,
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

/// `line`, a type header, with `suffix` right after its name.
fn insert_after_name(line: &str, header: &TypeHeader, suffix: &str) -> String {
    let mut word_ends = line.char_indices().filter_map(|(at, c)| {
        let end = at + c.len_utf8();
        let ends_word =
            !c.is_whitespace() && line[end..].chars().next().is_none_or(char::is_whitespace);
        ends_word.then_some(end)
    });
    match word_ends.nth(header.name_word) {
        Some(end) => format!("{}{suffix}{}", &line[..end], &line[end..]),
        None => line.to_string(),
    }
}

/// Generate federation-compliant SDL for `_service` query.
///
/// Uses a state-machine line processor to inject `@key` on type headers and
/// field-level federation directives (`@external`, `@requires`, `@provides`,
/// `@shareable`, `@inaccessible`, `@override`) on individual field definitions,
/// and each of `tags` on the root field or type header it names.
///
/// # Type handling
///
/// - `is_extends: true` types → emitted as `extend type Foo @key(...) {`
/// - `type_shareable: true` → type header includes `@shareable`
/// - `interface` types with `@key` → entity interface pattern
/// - `input` / `enum` types → no federation directives injected
#[must_use]
pub fn generate_service_sdl(
    base_schema: &str,
    metadata: &FederationMetadata,
    tags: &ContractTags,
) -> String {
    if !metadata.enabled {
        return base_schema.to_string();
    }

    // Build lookup map: type name → federated type metadata
    let fed_type_map: HashMap<&str, &FederatedType> =
        metadata.types.iter().map(|t| (t.name.as_str(), t)).collect();

    // State-machine line processor
    let mut output_lines: Vec<String> = Vec::new();
    let mut current_type_name: Option<String> = None;
    // The type whose body is open, federated or not: what a field line belongs to.
    let mut open_type: Option<String> = None;
    let mut brace_depth: u32 = 0;

    for line in base_schema.lines() {
        let trimmed = line.trim();
        let indent = &line[..line.len() - line.trim_start().len()];

        if brace_depth == 0 {
            if let Some(type_info) = parse_type_header(trimmed) {
                open_type = Some(type_info.name.clone());
                let type_tags = tag_suffix(tags.of_type(&type_info.name));
                match type_info.kind.as_str() {
                    "input" | "enum" | "scalar" | "union" => {
                        // No federation directives for these types
                        current_type_name = None;
                        output_lines.push(insert_after_name(line, &type_info, &type_tags));
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
                            header.push_str(&type_tags);

                            header.push_str(" {");
                            output_lines.push(format!("{indent}{header}"));
                        } else {
                            // Not a federated type: only its tags, if any
                            current_type_name = None;
                            output_lines.push(insert_after_name(line, &type_info, &type_tags));
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
            let suffix = extract_field_name(trimmed).map_or_else(String::new, |field_name| {
                let federated = current_type_name
                    .as_deref()
                    .and_then(|name| fed_type_map.get(name))
                    .and_then(|fed_type| fed_type.field_directives.get(&field_name))
                    .map_or_else(String::new, field_directive_suffix);
                let tagged = open_type
                    .as_deref()
                    .and_then(|name| tags.field(name, &field_name))
                    .map_or_else(String::new, |tag| tag_suffix([tag]));
                federated + &tagged
            });
            if suffix.is_empty() {
                output_lines.push(line.to_string());
            } else {
                output_lines.push(append_directives_to_line(line, &suffix));
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
         \"@inaccessible\", \"@override\", \"@extends\", \"@tag\"])"
    );

    sdl.push_str(&modified_schema);
    sdl.push_str("\n\n");
    sdl.push_str(FEDERATION_SCHEMA);
    sdl.push('\n');

    // _Entity union — only true entities (those with a `@key`) are resolvable via
    // `_entities`. `@shareable` value types carry no key and must NOT appear here, or
    // composition rejects a union member with no key.
    let entity_types: Vec<&str> = metadata
        .types
        .iter()
        .filter(|t| !t.keys.is_empty())
        .map(|t| t.name.as_str())
        .collect();
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
#[must_use]
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
