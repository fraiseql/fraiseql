use indexmap::IndexMap;
use regex::Regex;

use super::{
    ExtractedSchema, Language, Result, SchemaExtractor, map_primitive_type, map_type,
    parse_annotation_params,
};
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType,
};

pub(super) struct RustExtractor;

impl SchemaExtractor for RustExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // #[type_(key = "value")] pub struct Name {
        let type_re = Regex::new(r"#\[type_\(([^)]*)\)\]\s*pub\s+struct\s+(\w+)\s*\{")?;
        // #[query(key = "value")] pub fn name
        let query_re = Regex::new(r"#\[query\(([^)]*)\)\]\s*pub\s+fn\s+(\w+)")?;
        let field_re = Regex::new(r"^\s*pub\s+(\w+)\s*:\s*(.+?)\s*,?\s*$")?;

        let lines: Vec<&str> = source.lines().collect();

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            let struct_line = source
                [..cap.get(0).expect("regex group 0 is always Some on a successful match").start()]
                .lines()
                .count();
            let mut fields = Vec::new();
            for line in lines.iter().skip(struct_line + 1) {
                let trimmed = line.trim();
                if trimmed == "}" {
                    break;
                }
                if let Some(fcap) = field_re.captures(line) {
                    let field_name = fcap[1].to_string();
                    let type_str = fcap[2].to_string();
                    let (graphql_type, nullable) = map_type(Language::Rust, &type_str);
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

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("return_type").cloned().unwrap_or_default();
            let returns_list = params.get("return_array").is_some_and(|v| v == "true");
            let sql_source = params.get("sql_source").cloned();

            let arguments = extract_rust_query_args(
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

pub(super) fn extract_rust_query_args(source: &str, fn_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

    let arg_re = Regex::new(r"(\w+)\s*:\s*(\S+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = cap[2].to_string();
        let graphql_type = map_primitive_type(&type_str);
        args.push(IntermediateArgument {
            name,
            arg_type: graphql_type,
            nullable: false,
            default: None,
            deprecated: None,
        });
    }
    args
}
