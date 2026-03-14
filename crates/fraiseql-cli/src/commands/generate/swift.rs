//! Swift code generator.

use super::{
    super::init::Language,
    SchemaGenerator,
    utils::{infer_sql_source, map_graphql_to_lang, to_camel_case, wrap_nullable},
};
use crate::schema::intermediate::{
    IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType,
};

// =============================================================================
// Swift generator
// =============================================================================

pub(super) struct SwiftGenerator;

impl SchemaGenerator for SwiftGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::new();

        for enum_def in &schema.enums {
            generate_swift_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_swift_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_swift_query(&mut out, query);
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

fn generate_swift_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("enum {}: String {{\n", enum_def.name));
    for val in &enum_def.values {
        out.push_str(&format!("    case {} = \"{}\"\n", val.name, val.name));
    }
    out.push_str("}\n\n");
}

fn generate_swift_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("@Type(sqlSource: \"{sql_source}\")\n"));
    out.push_str(&format!("struct {} {{\n", ty.name));

    for field in &ty.fields {
        let lang_type = map_graphql_to_lang(Language::Swift, &field.field_type);
        let type_str = if field.nullable {
            wrap_nullable(Language::Swift, &lang_type)
        } else {
            lang_type
        };
        out.push_str(&format!("    let {}: {type_str}\n", to_camel_case(&field.name)));
    }
    out.push_str("}\n\n");
}

fn generate_swift_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut params = vec![format!("returnType: {}.self", query.return_type)];
    if query.returns_list {
        params.push("returnArray: true".to_string());
    }
    params.push(format!("sqlSource: \"{sql_source}\""));

    out.push_str(&format!("@Query({})\n", params.join(", ")));

    let ret_type = if query.returns_list {
        format!("[{}]", query.return_type)
    } else {
        query.return_type.clone()
    };

    if query.arguments.is_empty() {
        out.push_str(&format!("func {}() -> {ret_type} {{ fatalError() }}\n\n", query.name));
    } else {
        let args: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let lang_type = map_graphql_to_lang(Language::Swift, &a.arg_type);
                format!("{}: {lang_type}", a.name)
            })
            .collect();
        out.push_str(&format!(
            "func {}({}) -> {ret_type} {{ fatalError() }}\n\n",
            query.name,
            args.join(", ")
        ));
    }
}
