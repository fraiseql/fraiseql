use indexmap::IndexMap;
use regex::Regex;

use crate::schema::intermediate::{IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateType};

use super::{ExtractedSchema, Language, Result, SchemaExtractor, map_type, parse_annotation_params};

pub(super) struct GoExtractor;

impl SchemaExtractor for GoExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // // @Type(sqlSource = "v_author")
        // type Author struct {
        let type_re = Regex::new(r"//\s*@Type\(([^)]*)\)\s*\ntype\s+(\w+)\s+struct\s*\{")?;
        // fraiseql.RegisterQuery("name", fraiseql.QueryDef{...})
        let query_re = Regex::new(
            r#"RegisterQuery\(\s*"(\w+)"\s*,\s*fraiseql\.QueryDef\{([^}]*(?:\{[^}]*\}[^}]*)*)\}"#,
        )?;

        let lines: Vec<&str> = source.lines().collect();

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            let struct_line = source[..cap.get(0).expect("regex group 0 is always Some on a successful match").end()].lines().count() - 1;
            let fields = extract_go_struct_fields(&lines, struct_line + 1);

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
            let name = cap[1].to_string();
            let body = &cap[2];
            let params = parse_annotation_params(body);
            let return_type = params.get("ReturnType").cloned().unwrap_or_default();
            let returns_list = params.get("ReturnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("SQLSource").cloned();
            let arguments = extract_go_query_args(body);

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

pub(super) fn extract_go_struct_fields(lines: &[&str], start: usize) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Go struct field: Name Type `fraiseql:"field_name"`
    let field_re = Regex::new(r#"^\s+(\w+)\s+(\*?\w+)\s+`fraiseql:"(\w+)"`"#).expect("valid regex");

    for line in lines.iter().skip(start) {
        let trimmed = line.trim();
        if trimmed == "}" {
            break;
        }
        if let Some(cap) = field_re.captures(line) {
            let type_str = &cap[2];
            let tag_name = cap[3].to_string();
            let (graphql_type, nullable) = map_type(Language::Go, type_str);

            fields.push(IntermediateField {
                name: tag_name,
                field_type: graphql_type,
                nullable,
                description: None,
                directives: None,
                requires_scope: None,
                on_deny:        None,
            });
        }
    }
    fields
}

pub(super) fn extract_go_query_args(body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Args: []fraiseql.Arg{{Name: "id", Type: "ID", Required: true}}
    let arg_re = Regex::new(
        r#"Name\s*:\s*"(\w+)"\s*,\s*Type\s*:\s*"(\w+)"\s*,\s*Required\s*:\s*(true|false)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(body) {
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
