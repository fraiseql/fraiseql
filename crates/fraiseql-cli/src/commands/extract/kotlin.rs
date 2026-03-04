use indexmap::IndexMap;
use regex::Regex;

use crate::schema::intermediate::{IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType};

use super::{
    ExtractedSchema, Language, Result, SchemaExtractor, map_primitive_type, map_type,
    parse_annotation_params, to_snake_case,
};

pub(super) struct KotlinExtractor;

impl SchemaExtractor for KotlinExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") data class Author(...)
        let type_re = Regex::new(r"@Type\(([^)]*)\)\s*data\s+class\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) fun name(
        let query_re = Regex::new(r"@Query\(([^)]*)\)\s*fun\s+(\w+)\s*\(")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_kotlin_fields(body);
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
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();

            let arguments = extract_kotlin_query_args(source, cap.get(0).expect("regex group 0 is always Some on a successful match").end());

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

pub(super) fn extract_kotlin_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // val name: Type, or val name: Type?,
    let field_re = Regex::new(r"val\s+(\w+)\s*:\s*(\w+\??)").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let raw_name = &cap[1];
        let type_str = &cap[2];
        let field_name = to_snake_case(raw_name);
        let (graphql_type, nullable) = map_type(Language::Kotlin, type_str);

        fields.push(IntermediateField {
            name: field_name,
            field_type: graphql_type,
            nullable,
            description: None,
            directives: None,
            requires_scope: None,
            on_deny:        None,
        });
    }
    fields
}

pub(super) fn extract_kotlin_query_args(source: &str, fn_paren_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // The regex already consumed up to "(", so we're right after it
    let rest = &source[fn_paren_start..];
    let Some(close) = rest.find(')') else {
        return args;
    };
    let sig = &rest[..close];

    let arg_re = Regex::new(r"(\w+)\s*:\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
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
