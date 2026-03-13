use indexmap::IndexMap;
use regex::Regex;

use crate::schema::intermediate::{IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType};

use super::{
    ExtractedSchema, Result, SchemaExtractor, derive_query_name, map_primitive_type,
    parse_annotation_params, to_snake_case,
};

pub(super) struct JavaExtractor;

impl SchemaExtractor for JavaExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") public record Author(...)
        let type_re = Regex::new(r"@Type\(([^)]*)\)\s*public\s+record\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) public interface Name — handle nested parens from @Arg(...) and .class
        let query_re =
            Regex::new(r"@Query\(([^)]*(?:\([^)]*\)[^)]*)*)\)\s*public\s+interface\s+(\w+)")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let record_body = &cap[3];

            let fields = extract_java_record_fields(record_body);

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
            let name_from_interface = &cap[2];
            // Query name: derive from interface name (e.g., Posts → posts, PostById → post)
            let query_name = derive_query_name(name_from_interface);
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();
            let arguments = extract_java_query_args(&cap[1]);

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

pub(super) fn extract_java_record_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Each line: `Type name,` or `@Nullable Type name,`
    let field_re = Regex::new(r"(@Nullable\s+)?(\w+)\s+(\w+)\s*[,)]?").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let nullable = cap.get(1).is_some();
        let type_str = &cap[2];
        let raw_name = &cap[3];
        let field_name = to_snake_case(raw_name);
        let graphql_type = map_primitive_type(type_str);

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

pub(super) fn extract_java_query_args(annotation_body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // @Arg(name = "id", type = "ID", required = true)
    let arg_re = Regex::new(
        r#"@Arg\(\s*name\s*=\s*"(\w+)"\s*,\s*type\s*=\s*"(\w+)"\s*,\s*required\s*=\s*(true|false)\s*\)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(annotation_body) {
        args.push(IntermediateArgument {
            name:       cap[1].to_string(),
            arg_type:   cap[2].to_string(),
            nullable:   &cap[3] != "true",
            default:    None,
            deprecated: None,
        });
    }
    args
}
