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
                        name: field_name,
                        field_type: graphql_type,
                        nullable,
                        description: None,
                        directives: None,
                        requires_scope: None,
                        on_deny: None,
                    });
                }
            }

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
                requires_role: None,
                is_error: false,
                relay: false,
            });
        }

        // Extract queries
        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("return_type").cloned().unwrap_or_default();
            let returns_list =
                params.get("return_array").is_some_and(|v| v == "true" || v == "True");
            let sql_source = params.get("sql_source").cloned();

            // Parse function arguments (skip self, *, etc.)
            let arguments = extract_python_query_args(
                source,
                cap.get(0).expect("regex group 0 is always Some on a successful match").end(),
            );

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
                relay: false,
                inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                relay_cursor_type: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
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
            name:       name.to_string(),
            arg_type:   graphql_type,
            nullable:   false,
            default:    None,
            deprecated: None,
        });
    }
    args
}
