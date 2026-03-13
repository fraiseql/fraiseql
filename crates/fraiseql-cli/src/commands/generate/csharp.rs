//! C# code generator.

use super::super::init::Language;
use super::utils::{derive_class_name, infer_sql_source, map_graphql_to_lang, to_pascal_case};
use super::SchemaGenerator;
use crate::schema::intermediate::{IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType};

// =============================================================================
// C# generator
// =============================================================================

pub(super) struct CSharpGenerator;

impl SchemaGenerator for CSharpGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::new();

        for enum_def in &schema.enums {
            generate_csharp_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_csharp_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_csharp_query(&mut out, query);
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

fn generate_csharp_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("public enum {} {{\n", enum_def.name));
    let names: Vec<&str> = enum_def.values.iter().map(|v| v.name.as_str()).collect();
    out.push_str(&format!("    {}\n", names.join(", ")));
    out.push_str("}\n\n");
}

fn generate_csharp_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("[Type(SqlSource = \"{sql_source}\")]\n"));
    out.push_str(&format!("public record {}(\n", ty.name));

    for (i, field) in ty.fields.iter().enumerate() {
        let lang_type = map_graphql_to_lang(Language::CSharp, &field.field_type);
        let field_name = to_pascal_case(&field.name);
        let nullable_suffix = if field.nullable { "?" } else { "" };
        let comma = if i + 1 < ty.fields.len() { "," } else { "" };
        out.push_str(&format!("    {lang_type}{nullable_suffix} {field_name}{comma}\n"));
    }
    out.push_str(");\n\n");
}

fn generate_csharp_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let class_name = derive_class_name(query);

    let mut params = vec![format!("ReturnType = typeof({})", query.return_type)];
    if query.returns_list {
        params.push("ReturnArray = true".to_string());
    }
    params.push(format!("SqlSource = \"{sql_source}\""));

    if !query.arguments.is_empty() {
        let arg_strs: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let required = if a.nullable { "false" } else { "true" };
                format!(
                    "Arg(Name = \"{}\", Type = \"{}\", Required = {required})",
                    a.name, a.arg_type
                )
            })
            .collect();
        params.extend(arg_strs);
    }

    out.push_str(&format!("[Query({})]\n", params.join(", ")));
    out.push_str(&format!("public static partial class {class_name};\n\n"));
}
