//! TypeScript code generator.

use super::SchemaGenerator;
use super::utils::infer_sql_source;
use crate::schema::intermediate::{IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType};

// =============================================================================
// TypeScript generator
// =============================================================================

pub(super) struct TypeScriptGenerator;

impl SchemaGenerator for TypeScriptGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::from("import { type_, query } from \"fraiseql\";\n\n");

        for enum_def in &schema.enums {
            generate_ts_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_ts_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_ts_query(&mut out, query);
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

fn generate_ts_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("export enum {} {{\n", enum_def.name));
    for val in &enum_def.values {
        out.push_str(&format!("  {} = \"{}\",\n", val.name, val.name));
    }
    out.push_str("}\n\n");
}

fn generate_ts_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("export const {} = type_(\"{}\", {{\n", ty.name, ty.name));
    out.push_str(&format!("  sqlSource: \"{sql_source}\",\n"));
    out.push_str("  fields: {\n");
    for field in &ty.fields {
        let nullable_str = if field.nullable { "true" } else { "false" };
        out.push_str(&format!(
            "    {}: {{ type: \"{}\", nullable: {nullable_str} }},\n",
            field.name, field.field_type
        ));
    }
    out.push_str("  },\n});\n\n");
}

fn generate_ts_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    out.push_str(&format!("export const {} = query(\"{}\", {{\n", query.name, query.name));
    out.push_str(&format!("  returnType: \"{}\",\n", query.return_type));
    out.push_str(&format!(
        "  returnArray: {},\n",
        if query.returns_list { "true" } else { "false" }
    ));
    out.push_str(&format!("  sqlSource: \"{sql_source}\",\n"));

    if !query.arguments.is_empty() {
        out.push_str("  args: [\n");
        for arg in &query.arguments {
            let required = if arg.nullable { "false" } else { "true" };
            out.push_str(&format!(
                "    {{ name: \"{}\", type: \"{}\", required: {required} }},\n",
                arg.name, arg.arg_type
            ));
        }
        out.push_str("  ],\n");
    }
    out.push_str("});\n\n");
}
