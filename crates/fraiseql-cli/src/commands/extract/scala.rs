use indexmap::IndexMap;
use regex::Regex;

use super::{
    ExtractedSchema, Language, Result, SchemaExtractor, map_primitive_type, map_type,
    parse_annotation_params, to_snake_case,
};
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType,
};

pub(super) struct ScalaExtractor;

impl SchemaExtractor for ScalaExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") case class Author(...)
        let type_re = Regex::new(r"@Type\(([^)]*)\)\s*case\s+class\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) def name(
        let query_re = Regex::new(r"@Query\(([^)]*)\)\s*def\s+(\w+)")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_scala_fields(body);
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

            let arguments = extract_scala_query_args(
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

pub(super) fn extract_scala_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // name: Type, or name: Option[Type]
    let field_re = Regex::new(r"(\w+)\s*:\s*(Option\[\w+\]|\w+)").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let raw_name = &cap[1];
        let type_str = &cap[2];
        let field_name = to_snake_case(raw_name);
        let (graphql_type, nullable) = map_type(Language::Scala, type_str);

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
    fields
}

pub(super) fn extract_scala_query_args(source: &str, fn_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

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
