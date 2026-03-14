//! PHP code generator.

use super::{
    super::init::Language,
    SchemaGenerator,
    utils::{infer_sql_source, map_graphql_to_lang, to_camel_case},
};
use crate::schema::intermediate::{
    IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType,
};

// =============================================================================
// PHP generator
// =============================================================================

pub(super) struct PhpGenerator;

impl SchemaGenerator for PhpGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::from(
            "<?php\n\ndeclare(strict_types=1);\n\nuse FraiseQL\\Attributes\\GraphQLType;\nuse FraiseQL\\Attributes\\GraphQLField;\n\n",
        );

        for enum_def in &schema.enums {
            generate_php_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_php_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_php_query(&mut out, query);
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

fn generate_php_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("enum {} : string\n{{\n", enum_def.name));
    for value in &enum_def.values {
        out.push_str(&format!("    case {} = '{}';\n", value.name, value.name.to_lowercase()));
    }
    out.push_str("}\n\n");
}

fn generate_php_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("#[GraphQLType(name: '{}', sqlSource: '{sql_source}')]\n", ty.name));
    out.push_str(&format!("final class {}\n{{\n", ty.name));

    for field in &ty.fields {
        let lang_type = map_graphql_to_lang(Language::Php, &field.field_type);
        let field_name = to_camel_case(&field.name);
        let nullable_attr = if field.nullable {
            ", nullable: true"
        } else {
            ""
        };
        let type_hint = if field.nullable {
            format!("?{lang_type}")
        } else {
            lang_type
        };
        out.push_str(&format!(
            "    #[GraphQLField(type: '{}'{nullable_attr})]\n    public {type_hint} ${field_name};\n\n",
            field.field_type,
        ));
    }

    out.push_str("}\n\n");
}

fn generate_php_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    // Use ById suffix for single-result queries with arguments (consistent with other generators)
    let fn_name = if !query.returns_list && !query.arguments.is_empty() {
        format!("{}ById", to_camel_case(&query.name))
    } else {
        to_camel_case(&query.name)
    };

    let mut attr_parts = vec![
        format!("returnType: '{}::class'", query.return_type),
        format!("sqlSource: '{sql_source}'"),
    ];
    if query.returns_list {
        attr_parts.push("returnArray: true".to_string());
    }

    if !query.arguments.is_empty() {
        let arg_strs: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let nullable_flag = if a.nullable { ", nullable: true" } else { "" };
                format!("new Arg(name: '{}', type: '{}'{nullable_flag})", a.name, a.arg_type)
            })
            .collect();
        attr_parts.push(format!("args: [{}]", arg_strs.join(", ")));
    }

    out.push_str(&format!("#[Query({})]\n", attr_parts.join(", ")));
    out.push_str(&format!("function {fn_name}(): void {{}}\n\n"));
}
