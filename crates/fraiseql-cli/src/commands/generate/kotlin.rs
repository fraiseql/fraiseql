//! Kotlin code generator.

use super::super::init::Language;
use super::utils::{infer_sql_source, map_graphql_to_lang, to_camel_case, wrap_nullable};
use super::SchemaGenerator;
use crate::schema::intermediate::{IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType};

// =============================================================================
// Kotlin generator
// =============================================================================

pub(super) struct KotlinGenerator;

impl SchemaGenerator for KotlinGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::new();

        for enum_def in &schema.enums {
            generate_kotlin_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_kotlin_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_kotlin_query(&mut out, query);
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

fn generate_kotlin_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("enum class {} {{\n", enum_def.name));
    let names: Vec<&str> = enum_def.values.iter().map(|v| v.name.as_str()).collect();
    out.push_str(&format!("    {}\n", names.join(", ")));
    out.push_str("}\n\n");
}

fn generate_kotlin_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("@Type(sqlSource = \"{sql_source}\")\n"));
    out.push_str(&format!("data class {}(\n", ty.name));

    for field in &ty.fields {
        let lang_type = map_graphql_to_lang(Language::Kotlin, &field.field_type);
        let type_str = if field.nullable {
            wrap_nullable(Language::Kotlin, &lang_type)
        } else {
            lang_type
        };
        out.push_str(&format!("    val {}: {type_str},\n", to_camel_case(&field.name)));
    }
    out.push_str(")\n\n");
}

fn generate_kotlin_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut params = vec![format!("returnType = {}::class", query.return_type)];
    if query.returns_list {
        params.push("returnArray = true".to_string());
    }
    params.push(format!("sqlSource = \"{sql_source}\""));

    out.push_str(&format!("@Query({})\n", params.join(", ")));

    let ret_type = if query.returns_list {
        format!("List<{}>", query.return_type)
    } else {
        query.return_type.clone()
    };

    if query.arguments.is_empty() {
        out.push_str(&format!("fun {}(): {ret_type} = TODO()\n\n", query.name));
    } else {
        let args: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let lang_type = map_graphql_to_lang(Language::Kotlin, &a.arg_type);
                format!("{}: {lang_type}", a.name)
            })
            .collect();
        out.push_str(&format!("fun {}({}): {ret_type} = TODO()\n\n", query.name, args.join(", ")));
    }
}
