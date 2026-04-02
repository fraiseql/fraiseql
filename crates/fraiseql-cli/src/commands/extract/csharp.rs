use indexmap::IndexMap;
use regex::Regex;

use super::{
    ExtractedSchema, Result, SchemaExtractor, derive_query_name, map_primitive_type,
    parse_annotation_params, to_snake_case,
};
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType,
};

pub(super) struct CSharpExtractor;

impl SchemaExtractor for CSharpExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // [Type(SqlSource = "v_author")] public record Author(...)
        let type_re = Regex::new(r"\[Type\(([^)]*)\)\]\s*public\s+record\s+(\w+)\s*\(([^)]*)\)")?;
        // [Query(...)] public static partial class Name — handle nested parens from typeof(...)
        let query_re = Regex::new(
            r"\[Query\(([^)]*(?:\([^)]*\)[^)]*)*)\)\]\s*public\s+static\s+partial\s+class\s+(\w+)",
        )?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_csharp_record_fields(body);
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
            let class_name = &cap[2];
            let query_name = derive_query_name(class_name);
            let return_type = params.get("ReturnType").cloned().unwrap_or_default();
            let returns_list = params.get("ReturnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("SqlSource").cloned();

            let arguments = extract_csharp_query_args(&cap[1]);

            queries.push(IntermediateQuery {
                name: query_name,
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

pub(super) fn extract_csharp_query_args(annotation_body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Arg(Name = "id", Type = "ID", Required = true)
    let arg_re = Regex::new(
        r#"Arg\(\s*Name\s*=\s*"(\w+)"\s*,\s*Type\s*=\s*"(\w+)"\s*,\s*Required\s*=\s*(true|false)\s*\)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(annotation_body) {
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

pub(super) fn extract_csharp_record_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // C# record: Type? Name, or Type Name
    let field_re = Regex::new(r"(\w+)(\??)\s+(\w+)\s*[,)]?").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let type_str = &cap[1];
        let nullable_marker = &cap[2];
        let raw_name = &cap[3];
        let field_name = to_snake_case(raw_name);
        let graphql_type = map_primitive_type(type_str);
        let nullable = nullable_marker == "?";

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
