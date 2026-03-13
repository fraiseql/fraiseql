//! Python code generator.

use super::super::init::Language;
use super::utils::{infer_sql_source, map_graphql_to_lang, wrap_nullable};
use super::SchemaGenerator;
use crate::schema::intermediate::{IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType};

// =============================================================================
// Python generator
// =============================================================================

pub(super) struct PythonGenerator;

impl SchemaGenerator for PythonGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::from("import fraiseql\n\n");

        for enum_def in &schema.enums {
            generate_python_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_python_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_python_query(&mut out, query);
        }

        out.trim_end().to_string();
        // Ensure single trailing newline
        while out.ends_with("\n\n") {
            out.pop();
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

fn generate_python_enum(out: &mut String, enum_def: &IntermediateEnum) {
    if let Some(desc) = &enum_def.description {
        out.push_str(&format!("# {desc}\n"));
    }
    out.push_str(&format!("class {}(fraiseql.Enum):\n", enum_def.name));
    for val in &enum_def.values {
        out.push_str(&format!("    {} = \"{}\"\n", val.name, val.name));
    }
    out.push('\n');
}

fn generate_python_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    let mut params = vec![format!("sql_source=\"{sql_source}\"")];
    if let Some(desc) = &ty.description {
        params.push(format!("description=\"{desc}\""));
    }
    out.push_str(&format!("@fraiseql.type({})\n", params.join(", ")));
    out.push_str(&format!("class {}:\n", ty.name));

    if ty.fields.is_empty() {
        out.push_str("    pass\n");
    } else {
        for field in &ty.fields {
            let lang_type = map_graphql_to_lang(Language::Python, &field.field_type);
            let type_str = if field.nullable {
                wrap_nullable(Language::Python, &lang_type)
            } else {
                lang_type
            };
            out.push_str(&format!("    {}: {type_str}\n", field.name));
        }
    }
    out.push('\n');
}

fn generate_python_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut params = vec![format!("return_type={}", query.return_type)];
    if query.returns_list {
        params.push("return_array=True".to_string());
    }
    params.push(format!("sql_source=\"{sql_source}\""));

    out.push_str(&format!("@fraiseql.query({})\n", params.join(", ")));

    let ret_type = if query.returns_list {
        format!("list[{}]", query.return_type)
    } else {
        query.return_type.clone()
    };

    if query.arguments.is_empty() {
        out.push_str(&format!("def {}() -> {ret_type}:\n", query.name));
    } else {
        let args: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let lang_type = map_graphql_to_lang(Language::Python, &a.arg_type);
                format!("{}: {lang_type}", a.name)
            })
            .collect();
        out.push_str(&format!("def {}(*, {}) -> {ret_type}:\n", query.name, args.join(", ")));
    }
    out.push_str("    ...\n\n");
}
