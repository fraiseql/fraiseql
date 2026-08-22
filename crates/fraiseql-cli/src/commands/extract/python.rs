use indexmap::IndexMap;
use regex::Regex;

use super::{
    ExtractedSchema, Language, Result, SchemaExtractor, map_primitive_type, map_type,
    parse_annotation_params,
};
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType,
};

pub(super) struct PythonExtractor;

impl SchemaExtractor for PythonExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        let type_re = Regex::new(r"@fraiseql\.type\(([^)]*)\)\s*\nclass\s+(\w+)")?;
        let field_re = Regex::new(r"^\s+(\w+):\s*(.+?)\s*$")?;
        let query_re = Regex::new(r"@fraiseql\.query\(([^)]*)\)\s*\ndef\s+(\w+)")?;

        let lines: Vec<&str> = source.lines().collect();

        // Extract types
        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            // Find class body: lines after "class Name:" that are indented
            // Match ends after "class Name", skip to next line for body
            let match_end =
                cap.get(0).expect("regex group 0 is always Some on a successful match").end();
            let body_start_line = source[..match_end].lines().count();
            let mut fields = Vec::new();
            for line in lines.iter().skip(body_start_line) {
                // Skip blank lines and docstrings
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("\"\"\"")
                    || trimmed.starts_with("'''")
                {
                    continue;
                }
                // Stop at next class/function/decorator at column 0
                if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
                    break;
                }
                if let Some(fcap) = field_re.captures(line) {
                    let field_name = fcap[1].to_string();
                    let type_str = fcap[2].to_string();
                    let (graphql_type, nullable) = map_type(Language::Python, &type_str);
                    fields.push(IntermediateField {
                        vector_config: None,
                        vector_distance: None,
                        name: field_name,
                        field_type: graphql_type,
                        nullable,
                        description: None,
                        directives: None,
                        requires_scope: None,
                        on_deny: None,
                        authorize: None,
                        hierarchy: None,
                    });
                }
            }

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                sql_source: None,
                name,
                fields,
                description,
                implements: Vec::new(),
                requires_role: None,
                is_error: false,
                is_input: false,
                relay: false,
                embedded: false,
                subscribable_tables: None,
                subscribable_pre_image: false,
                inject_params: indexmap::IndexMap::new(),
            });
        }

        // Extract queries
        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let fn_end =
                cap.get(0).expect("regex group 0 is always Some on a successful match").end();

            // The real SDK derives the return type, list-ness and nullability
            // from the function's return annotation (`-> list[Post]`,
            // `-> Post | None`, `-> Post`) — it has no `return_type=` /
            // `return_array=` decorator kwargs. The kwargs are kept as
            // overrides so pre-SDK-dialect sources still extract.
            let (ann_type, ann_list, ann_nullable) = parse_python_return_annotation(source, fn_end);
            let return_type = params.get("return_type").cloned().or(ann_type).unwrap_or_default();
            let returns_list =
                params.get("return_array").map_or(ann_list, |v| v == "true" || v == "True");
            let sql_source = params.get("sql_source").cloned();

            // Parse function arguments (skip self, *, etc.)
            let arguments = extract_python_query_args(source, fn_end);

            queries.push(IntermediateQuery {
                count: false,
                name,
                return_type,
                returns_list,
                nullable: ann_nullable,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
                relay: false,
                inject: IndexMap::default(),
                read_routing: fraiseql_core::db::types::ReadRouting::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                requires_actor: Vec::new(),
                relay_cursor_type: None,
                // `extract` scans SDK source text; it does not parse REST annotations.
                rest: None,
                rest_stream: false,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

/// Infer `(return_type, returns_list, nullable)` from the Python return
/// annotation following the function signature: `-> list[Post]` is a list,
/// `-> Post | None` is nullable, `-> Post` is a plain non-null result.
/// Returns `(None, false, false)` when no annotation is present.
fn parse_python_return_annotation(source: &str, fn_start: usize) -> (Option<String>, bool, bool) {
    let rest = &source[fn_start..];
    // Annotation lives between the signature's `->` and the `:` that opens the body.
    let Some(arrow) = rest.find("->") else {
        return (None, false, false);
    };
    let after_arrow = &rest[arrow + 2..];
    let Some(colon) = after_arrow.find(':') else {
        return (None, false, false);
    };
    let ann = after_arrow[..colon].trim();

    if let Some(inner) = ann.strip_prefix("list[").and_then(|s| s.strip_suffix(']')) {
        (Some(inner.trim().to_string()), true, false)
    } else if let Some(t) = ann.strip_suffix("None").and_then(|s| s.trim_end().strip_suffix('|')) {
        (Some(t.trim().to_string()), false, true)
    } else if ann.is_empty() {
        (None, false, false)
    } else {
        (Some(ann.to_string()), false, false)
    }
}

pub(super) fn extract_python_query_args(
    source: &str,
    fn_start: usize,
) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Find the function signature between parens
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

    let arg_re = Regex::new(r"(\w+):\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = &cap[1];
        // Skip 'self' and bare '*'
        if name == "self" {
            continue;
        }
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
        args.push(IntermediateArgument {
            name:        name.to_string(),
            arg_type:    graphql_type,
            nullable:    false,
            default:     None,
            description: None,
            deprecated:  None,
        });
    }
    args
}
