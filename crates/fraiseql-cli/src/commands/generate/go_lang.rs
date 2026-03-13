//! Go code generator.

use super::super::init::Language;
use super::utils::{infer_sql_source, map_graphql_to_lang, to_pascal_case, wrap_nullable};
use super::SchemaGenerator;
use crate::schema::intermediate::{IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType};

// =============================================================================
// Go generator
// =============================================================================

pub(super) struct GoGenerator;

impl SchemaGenerator for GoGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::from("package schema\n\nimport \"fraiseql\"\n\n");

        for enum_def in &schema.enums {
            generate_go_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_go_type(&mut out, ty);
        }

        if !schema.queries.is_empty() {
            out.push_str("func init() {\n");
            for query in &schema.queries {
                generate_go_query(&mut out, query);
            }
            out.push_str("}\n");
        }

        while out.ends_with("\n\n") {
            out.pop();
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

fn generate_go_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("type {} string\n\n", enum_def.name));
    out.push_str("const (\n");
    for val in &enum_def.values {
        out.push_str(&format!(
            "\t{}{} {} = \"{}\"\n",
            enum_def.name, val.name, enum_def.name, val.name
        ));
    }
    out.push_str(")\n\n");
}

fn generate_go_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("// @Type(sqlSource = \"{sql_source}\")\n"));
    out.push_str(&format!("type {} struct {{\n", ty.name));

    for field in &ty.fields {
        let go_name = to_pascal_case(&field.name);
        let lang_type = map_graphql_to_lang(Language::Go, &field.field_type);
        let type_str = if field.nullable {
            wrap_nullable(Language::Go, &lang_type)
        } else {
            lang_type
        };
        out.push_str(&format!("\t{go_name} {type_str} `fraiseql:\"{}\"`\n", field.name));
    }
    out.push_str("}\n\n");
}

fn generate_go_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut fields = vec![format!("ReturnType: \"{}\"", query.return_type)];
    if query.returns_list {
        fields.push("ReturnArray: true".to_string());
    }
    fields.push(format!("SQLSource: \"{sql_source}\""));

    if !query.arguments.is_empty() {
        let arg_strs: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let required = if a.nullable { "false" } else { "true" };
                format!("{{Name: \"{}\", Type: \"{}\", Required: {required}}}", a.name, a.arg_type)
            })
            .collect();
        fields.push(format!("Args: []fraiseql.Arg{{{}}}", arg_strs.join(", ")));
    }

    out.push_str(&format!(
        "\tfraiseql.RegisterQuery(\"{}\", fraiseql.QueryDef{{{}}})\n",
        query.name,
        fields.join(", ")
    ));
}
