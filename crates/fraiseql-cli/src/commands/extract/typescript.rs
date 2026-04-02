use std::collections::HashMap;

use indexmap::IndexMap;
use regex::Regex;

use super::{ExtractedSchema, Result, SchemaExtractor};
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType,
};

pub(super) struct TypeScriptExtractor;

impl SchemaExtractor for TypeScriptExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // Find type_("Name" and extract balanced braces body
        let type_start_re = Regex::new(r#"type_\(\s*"(\w+)"\s*,"#)?;
        // Find query("name" and extract balanced braces body
        let query_start_re = Regex::new(r#"query\(\s*"(\w+)"\s*,"#)?;

        for cap in type_start_re.captures_iter(source) {
            let name = cap[1].to_string();
            let after_match =
                cap.get(0).expect("regex group 0 is always Some on a successful match").end();
            if let Some(body) = extract_balanced_braces(&source[after_match..]) {
                let fields = extract_ts_fields(&body);
                types.push(IntermediateType {
                    name,
                    fields,
                    description: None,
                    implements: Vec::new(),
                    requires_role: None,
                    is_error: false,
                    relay: false,
                });
            }
        }

        for cap in query_start_re.captures_iter(source) {
            let name = cap[1].to_string();
            let after_match =
                cap.get(0).expect("regex group 0 is always Some on a successful match").end();
            if let Some(body) = extract_balanced_braces(&source[after_match..]) {
                let params = parse_ts_query_params(&body);
                let return_type = params.get("returnType").cloned().unwrap_or_default();
                let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
                let sql_source = params.get("sqlSource").cloned();
                let arguments = extract_ts_query_args(&body);

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
        }

        Ok(ExtractedSchema { types, queries })
    }
}

/// Extract text inside balanced braces `{ ... }` from the start of `s`.
pub(super) fn extract_balanced_braces(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth = 0;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            },
            _ => {},
        }
    }
    None
}

pub(super) fn extract_ts_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Match: fieldName: { type: "Type", nullable: bool }
    let field_re =
        Regex::new(r#"(\w+)\s*:\s*\{\s*type\s*:\s*"(\w+)"\s*,\s*nullable\s*:\s*(true|false)\s*\}"#)
            .expect("valid regex");

    for cap in field_re.captures_iter(body) {
        fields.push(IntermediateField {
            name: cap[1].to_string(),
            field_type: cap[2].to_string(),
            nullable: &cap[3] == "true",
            description: None,
            directives: None,
            requires_scope: None,
            on_deny: None,
        });
    }
    fields
}

pub(super) fn parse_ts_query_params(body: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    // returnType: "Type"
    let str_re = Regex::new(r#"(\w+)\s*:\s*"(\w+)""#).expect("valid regex");
    for cap in str_re.captures_iter(body) {
        params.insert(cap[1].to_string(), cap[2].to_string());
    }
    // returnArray: true/false
    let bool_re = Regex::new(r"(\w+)\s*:\s*(true|false)").expect("valid regex");
    for cap in bool_re.captures_iter(body) {
        params.insert(cap[1].to_string(), cap[2].to_string());
    }
    params
}

pub(super) fn extract_ts_query_args(body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // args: [{ name: "id", type: "ID", required: true }]
    let arg_re = Regex::new(
        r#"name\s*:\s*"(\w+)"\s*,\s*type\s*:\s*"(\w+)"\s*,\s*required\s*:\s*(true|false)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(body) {
        args.push(IntermediateArgument {
            name: cap[1].to_string(),
            arg_type: cap[2].to_string(),
            nullable: &cap[3] != "true",
            default: None,
            deprecated: None,
        });
    }
    args
}
