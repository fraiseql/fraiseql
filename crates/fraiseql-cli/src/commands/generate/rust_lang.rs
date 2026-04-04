//! Rust code generator.

use super::{
    super::init::Language,
    SchemaGenerator,
    utils::{infer_sql_source, map_graphql_to_lang, wrap_nullable},
};
use crate::schema::intermediate::{
    IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType,
};

// =============================================================================
// Rust generator
// =============================================================================

pub(super) struct RustGenerator;

impl SchemaGenerator for RustGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::from("use fraiseql::{type_, query};\n\n");

        for enum_def in &schema.enums {
            generate_rust_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_rust_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_rust_query(&mut out, query);
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

fn generate_rust_enum(out: &mut String, enum_def: &IntermediateEnum) {
    if let Some(desc) = &enum_def.description {
        out.push_str(&format!("/// {desc}\n"));
    }
    out.push_str(&format!("pub enum {} {{\n", enum_def.name));
    for val in &enum_def.values {
        out.push_str(&format!("    {},\n", val.name));
    }
    out.push_str("}\n\n");
}

fn generate_rust_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    let mut params = vec![format!("sql_source = \"{sql_source}\"")];
    if let Some(desc) = &ty.description {
        params.push(format!("description = \"{desc}\""));
    }
    out.push_str(&format!("#[type_({})]\n", params.join(", ")));
    out.push_str(&format!("pub struct {} {{\n", ty.name));

    for field in &ty.fields {
        let lang_type = map_graphql_to_lang(Language::Rust, &field.field_type);
        let type_str = if field.nullable {
            wrap_nullable(Language::Rust, &lang_type)
        } else {
            lang_type
        };
        out.push_str(&format!("    pub {}: {type_str},\n", field.name));
    }
    out.push_str("}\n\n");
}

fn generate_rust_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut params = vec![format!("return_type = \"{}\"", query.return_type)];
    if query.returns_list {
        params.push("return_array = true".to_string());
    }
    params.push(format!("sql_source = \"{sql_source}\""));

    out.push_str(&format!("#[query({})]\n", params.join(", ")));

    let ret_type = if query.returns_list {
        format!("Vec<{}>", query.return_type)
    } else {
        query.return_type.clone()
    };

    if query.arguments.is_empty() {
        out.push_str(&format!("pub fn {}() -> {ret_type} {{\n", query.name));
    } else {
        let args: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let lang_type = map_graphql_to_lang(Language::Rust, &a.arg_type);
                format!("{}: {lang_type}", a.name)
            })
            .collect();
        out.push_str(&format!("pub fn {}({}) -> {ret_type} {{\n", query.name, args.join(", ")));
    }
    // Emit a stub body. FraiseQL Rust SDK functions are authoring constructs
    // (compile-time decorators) — their bodies are never called at runtime.
    // Users may leave the stub in place or replace it with their own logic.
    out.push_str("    panic!(\"implement this resolver before deploying\")\n}\n\n");
}
