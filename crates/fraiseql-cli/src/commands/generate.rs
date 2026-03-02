//! `fraiseql generate` — Generate authoring-language source from schema.json
//!
//! The inverse of `fraiseql extract`: reads a schema.json and produces annotated
//! source code in any of the 9 supported authoring languages.

use std::fs;

use anyhow::{Context, Result};
use tracing::info;

use super::init::Language;
use crate::schema::intermediate::{
    IntermediateEnum, IntermediateQuery, IntermediateSchema, IntermediateType,
};

// =============================================================================
// Trait
// =============================================================================

/// Trait for language-specific code generation from intermediate schema.
trait SchemaGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String;
}

// =============================================================================
// Public API
// =============================================================================

/// Run the generate command.
pub fn run(input: &str, language: Language, output: Option<&str>) -> Result<()> {
    let json = fs::read_to_string(input).with_context(|| format!("Failed to read {input}"))?;
    let schema: IntermediateSchema =
        serde_json::from_str(&json).with_context(|| format!("Failed to parse {input}"))?;

    let code = dispatch_generator(language, &schema);

    let out_path = match output {
        Some(p) => p.to_string(),
        None => default_output_path(language),
    };

    fs::write(&out_path, &code).with_context(|| format!("Failed to write {out_path}"))?;

    let type_count = schema.types.len();
    let query_count = schema.queries.len();
    let enum_count = schema.enums.len();
    info!("Generated {type_count} types, {query_count} queries, {enum_count} enums");
    println!(
        "Generated {type_count} types, {query_count} queries, {enum_count} enums → {out_path}",
    );

    Ok(())
}

fn default_output_path(lang: Language) -> String {
    match lang {
        Language::Python => "schema.py".to_string(),
        Language::TypeScript => "schema.ts".to_string(),
        Language::Rust => "schema.rs".to_string(),
        Language::Java => "Schema.java".to_string(),
        Language::Kotlin => "Schema.kt".to_string(),
        Language::Go => "schema.go".to_string(),
        Language::CSharp => "Schema.cs".to_string(),
        Language::Swift => "Schema.swift".to_string(),
        Language::Scala => "Schema.scala".to_string(),
    }
}

fn dispatch_generator(lang: Language, schema: &IntermediateSchema) -> String {
    match lang {
        Language::Python => PythonGenerator.generate(schema),
        Language::TypeScript => TypeScriptGenerator.generate(schema),
        Language::Rust => RustGenerator.generate(schema),
        Language::Kotlin => KotlinGenerator.generate(schema),
        Language::Swift => SwiftGenerator.generate(schema),
        Language::Scala => ScalaGenerator.generate(schema),
        Language::Java => JavaGenerator.generate(schema),
        Language::Go => GoGenerator.generate(schema),
        Language::CSharp => CSharpGenerator.generate(schema),
    }
}

// =============================================================================
// Shared utilities
// =============================================================================

/// Convert `snake_case` to `camelCase`.
fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    for (i, ch) in s.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
            capitalize_next = false;
        } else if i == 0 {
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Convert `snake_case` to `PascalCase`.
fn to_pascal_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = true;
    for ch in s.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Map a GraphQL type name to a language-specific type name.
fn map_graphql_to_lang(lang: Language, graphql_type: &str) -> String {
    match lang {
        Language::Python => match graphql_type {
            "Int" => "int".to_string(),
            "Float" => "float".to_string(),
            "Boolean" => "bool".to_string(),
            "String" => "str".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::TypeScript => graphql_type.to_string(),
        Language::Rust => match graphql_type {
            "Int" => "i32".to_string(),
            "Float" => "f64".to_string(),
            "Boolean" => "bool".to_string(),
            "String" => "String".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::Java => match graphql_type {
            "Int" => "int".to_string(),
            "Float" => "double".to_string(),
            "Boolean" => "boolean".to_string(),
            "String" => "String".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::Kotlin => match graphql_type {
            "Int" => "Int".to_string(),
            "Float" => "Double".to_string(),
            "Boolean" => "Boolean".to_string(),
            "String" => "String".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::Go => match graphql_type {
            "Int" => "int".to_string(),
            "Float" => "float64".to_string(),
            "Boolean" => "bool".to_string(),
            "String" => "string".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::CSharp => match graphql_type {
            "Int" => "int".to_string(),
            "Float" => "double".to_string(),
            "Boolean" => "bool".to_string(),
            "String" => "string".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::Swift => match graphql_type {
            "Int" => "Int".to_string(),
            "Float" => "Double".to_string(),
            "Boolean" => "Bool".to_string(),
            "String" => "String".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
        Language::Scala => match graphql_type {
            "Int" => "Int".to_string(),
            "Float" => "Double".to_string(),
            "Boolean" => "Boolean".to_string(),
            "String" => "String".to_string(),
            "ID" => "ID".to_string(),
            "DateTime" => "DateTime".to_string(),
            other => other.to_string(),
        },
    }
}

/// Wrap a type string with language-specific nullable syntax.
fn wrap_nullable(lang: Language, type_str: &str) -> String {
    match lang {
        Language::Python => format!("{type_str} | None"),
        Language::Rust => format!("Option<{type_str}>"),
        Language::Kotlin | Language::Swift | Language::CSharp => format!("{type_str}?"),
        Language::Go => format!("*{type_str}"),
        Language::Scala => format!("Option[{type_str}]"),
        // TypeScript/Java handle nullable differently (not in type syntax)
        Language::TypeScript | Language::Java => type_str.to_string(),
    }
}

/// Derive a PascalCase class/interface name from a query.
/// List query "posts" → "Posts", single query "post" with args → "PostById".
fn derive_class_name(query: &IntermediateQuery) -> String {
    let base = to_pascal_case(&query.name);
    if !query.returns_list && !query.arguments.is_empty() {
        format!("{base}ById")
    } else {
        base
    }
}

// =============================================================================
// Python generator
// =============================================================================

struct PythonGenerator;

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

// =============================================================================
// TypeScript generator
// =============================================================================

struct TypeScriptGenerator;

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

// =============================================================================
// Rust generator
// =============================================================================

struct RustGenerator;

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
    out.push_str("    unimplemented!()\n}\n\n");
}

// =============================================================================
// Kotlin generator
// =============================================================================

struct KotlinGenerator;

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

// =============================================================================
// Swift generator
// =============================================================================

struct SwiftGenerator;

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

// =============================================================================
// Scala generator
// =============================================================================

struct ScalaGenerator;

impl SchemaGenerator for ScalaGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::new();

        for enum_def in &schema.enums {
            generate_scala_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_scala_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_scala_query(&mut out, query);
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

fn generate_scala_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("object {} extends Enumeration {{\n", enum_def.name));
    let names: Vec<String> = enum_def
        .values
        .iter()
        .map(|v| format!("val {} = Value(\"{}\")", v.name, v.name))
        .collect();
    out.push_str(&format!("  {}\n", names.join("; ")));
    out.push_str("}\n\n");
}

fn generate_scala_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("@Type(sqlSource = \"{sql_source}\")\n"));
    out.push_str(&format!("case class {}(\n", ty.name));

    for (i, field) in ty.fields.iter().enumerate() {
        let lang_type = map_graphql_to_lang(Language::Scala, &field.field_type);
        let type_str = if field.nullable {
            wrap_nullable(Language::Scala, &lang_type)
        } else {
            lang_type
        };
        let comma = if i + 1 < ty.fields.len() { "," } else { "" };
        out.push_str(&format!("  {}: {type_str}{comma}\n", to_camel_case(&field.name)));
    }
    out.push_str(")\n\n");
}

fn generate_scala_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let mut params = vec![format!("returnType = classOf[{}]", query.return_type)];
    if query.returns_list {
        params.push("returnArray = true".to_string());
    }
    params.push(format!("sqlSource = \"{sql_source}\""));

    out.push_str(&format!("@Query({})\n", params.join(", ")));

    let ret_type = if query.returns_list {
        format!("List[{}]", query.return_type)
    } else {
        query.return_type.clone()
    };

    if query.arguments.is_empty() {
        out.push_str(&format!("def {}(): {ret_type} = ???\n\n", query.name));
    } else {
        let args: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let lang_type = map_graphql_to_lang(Language::Scala, &a.arg_type);
                format!("{}: {lang_type}", a.name)
            })
            .collect();
        out.push_str(&format!("def {}({}): {ret_type} = ???\n\n", query.name, args.join(", ")));
    }
}

// =============================================================================
// Java generator
// =============================================================================

struct JavaGenerator;

impl SchemaGenerator for JavaGenerator {
    fn generate(&self, schema: &IntermediateSchema) -> String {
        let mut out = String::new();

        for enum_def in &schema.enums {
            generate_java_enum(&mut out, enum_def);
        }

        for ty in &schema.types {
            generate_java_type(&mut out, ty);
        }

        for query in &schema.queries {
            generate_java_query(&mut out, query);
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

fn generate_java_enum(out: &mut String, enum_def: &IntermediateEnum) {
    out.push_str(&format!("public enum {} {{\n", enum_def.name));
    let names: Vec<&str> = enum_def.values.iter().map(|v| v.name.as_str()).collect();
    out.push_str(&format!("    {}\n", names.join(", ")));
    out.push_str("}\n\n");
}

fn generate_java_type(out: &mut String, ty: &IntermediateType) {
    let sql_source = infer_sql_source(&ty.name);
    out.push_str(&format!("@Type(sqlSource = \"{sql_source}\")\n"));
    out.push_str(&format!("public record {}(\n", ty.name));

    for (i, field) in ty.fields.iter().enumerate() {
        let lang_type = map_graphql_to_lang(Language::Java, &field.field_type);
        let field_name = to_camel_case(&field.name);
        let nullable_prefix = if field.nullable { "@Nullable " } else { "" };
        let comma = if i + 1 < ty.fields.len() { "," } else { "" };
        out.push_str(&format!("    {nullable_prefix}{lang_type} {field_name}{comma}\n"));
    }
    out.push_str(") {}\n\n");
}

fn generate_java_query(out: &mut String, query: &IntermediateQuery) {
    let sql_source = query.sql_source.as_deref().unwrap_or("v_unknown");
    let class_name = derive_class_name(query);

    let mut params = vec![format!("returnType = {}.class", query.return_type)];
    if query.returns_list {
        params.push("returnArray = true".to_string());
    }
    params.push(format!("sqlSource = \"{sql_source}\""));

    if !query.arguments.is_empty() {
        let arg_strs: Vec<String> = query
            .arguments
            .iter()
            .map(|a| {
                let required = if a.nullable { "false" } else { "true" };
                format!(
                    "args = @Arg(name = \"{}\", type = \"{}\", required = {required})",
                    a.name, a.arg_type
                )
            })
            .collect();
        params.extend(arg_strs);
    }

    out.push_str(&format!("@Query({})\n", params.join(", ")));
    out.push_str(&format!("public interface {class_name} {{}}\n\n"));
}

// =============================================================================
// Go generator
// =============================================================================

struct GoGenerator;

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

// =============================================================================
// C# generator
// =============================================================================

struct CSharpGenerator;

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

// =============================================================================
// Shared helpers
// =============================================================================

/// Infer a SQL source name from a type name: "Author" → "v_author".
fn infer_sql_source(type_name: &str) -> String {
    let mut result = String::from("v_");
    for (i, ch) in type_name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::schema::intermediate::{
        IntermediateArgument, IntermediateEnum, IntermediateEnumValue, IntermediateField,
    };

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("created_at"), "createdAt");
        assert_eq!(to_camel_case("post_id"), "postId");
        assert_eq!(to_camel_case("id"), "id");
        assert_eq!(to_camel_case("name"), "name");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("created_at"), "CreatedAt");
        assert_eq!(to_pascal_case("post_id"), "PostId");
        assert_eq!(to_pascal_case("id"), "Id");
        assert_eq!(to_pascal_case("name"), "Name");
    }

    #[test]
    fn test_map_graphql_to_lang_python() {
        assert_eq!(map_graphql_to_lang(Language::Python, "Int"), "int");
        assert_eq!(map_graphql_to_lang(Language::Python, "String"), "str");
        assert_eq!(map_graphql_to_lang(Language::Python, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Python, "Float"), "float");
        assert_eq!(map_graphql_to_lang(Language::Python, "ID"), "ID");
    }

    #[test]
    fn test_map_graphql_to_lang_rust() {
        assert_eq!(map_graphql_to_lang(Language::Rust, "Int"), "i32");
        assert_eq!(map_graphql_to_lang(Language::Rust, "String"), "String");
        assert_eq!(map_graphql_to_lang(Language::Rust, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Rust, "Float"), "f64");
    }

    #[test]
    fn test_map_graphql_to_lang_go() {
        assert_eq!(map_graphql_to_lang(Language::Go, "Int"), "int");
        assert_eq!(map_graphql_to_lang(Language::Go, "String"), "string");
        assert_eq!(map_graphql_to_lang(Language::Go, "Boolean"), "bool");
        assert_eq!(map_graphql_to_lang(Language::Go, "Float"), "float64");
    }

    #[test]
    fn test_wrap_nullable() {
        assert_eq!(wrap_nullable(Language::Python, "str"), "str | None");
        assert_eq!(wrap_nullable(Language::Rust, "String"), "Option<String>");
        assert_eq!(wrap_nullable(Language::Kotlin, "String"), "String?");
        assert_eq!(wrap_nullable(Language::Swift, "String"), "String?");
        assert_eq!(wrap_nullable(Language::CSharp, "string"), "string?");
        assert_eq!(wrap_nullable(Language::Go, "string"), "*string");
        assert_eq!(wrap_nullable(Language::Scala, "String"), "Option[String]");
    }

    #[test]
    fn test_derive_class_name() {
        let list_query = IntermediateQuery {
            name:         "authors".to_string(),
            return_type:  "Author".to_string(),
            returns_list: true,
            nullable:     false,
            arguments:    vec![],
            description:  None,
            sql_source:   None,
            auto_params:  None,
            deprecated:   None,
            jsonb_column: None,
            relay: false,
             inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
            relay_cursor_type: None,
        };
        assert_eq!(derive_class_name(&list_query), "Authors");

        let single_query = IntermediateQuery {
            name:         "author".to_string(),
            return_type:  "Author".to_string(),
            returns_list: false,
            nullable:     false,
            arguments:    vec![IntermediateArgument {
                name:       "id".to_string(),
                arg_type:   "ID".to_string(),
                nullable:   false,
                default:    None,
                deprecated: None,
            }],
            description:  None,
            sql_source:   None,
            auto_params:  None,
            deprecated:   None,
            jsonb_column: None,
            relay: false,
             inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
            relay_cursor_type: None,
        };
        assert_eq!(derive_class_name(&single_query), "AuthorById");
    }

    #[test]
    fn test_infer_sql_source() {
        assert_eq!(infer_sql_source("Author"), "v_author");
        assert_eq!(infer_sql_source("BlogPost"), "v_blog_post");
        assert_eq!(infer_sql_source("User"), "v_user");
    }

    fn sample_schema() -> IntermediateSchema {
        IntermediateSchema {
            version: "2.0.0".to_string(),
            types: vec![IntermediateType {
                name:        "Author".to_string(),
                fields:      vec![
                    IntermediateField {
                        name:           "pk".to_string(),
                        field_type:     "Int".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "id".to_string(),
                        field_type:     "ID".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "name".to_string(),
                        field_type:     "String".to_string(),
                        nullable:       false,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                    IntermediateField {
                        name:           "bio".to_string(),
                        field_type:     "String".to_string(),
                        nullable:       true,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                        on_deny:        None,
                    },
                ],
                description: None,
                implements:  Vec::new(),
                requires_role: None,
                is_error:    false,
                relay:    false,
            }],
            queries: vec![
                IntermediateQuery {
                    name:         "authors".to_string(),
                    return_type:  "Author".to_string(),
                    returns_list: true,
                    nullable:     false,
                    arguments:    vec![],
                    description:  None,
                    sql_source:   Some("v_author".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
                IntermediateQuery {
                    name:         "author".to_string(),
                    return_type:  "Author".to_string(),
                    returns_list: false,
                    nullable:     false,
                    arguments:    vec![IntermediateArgument {
                        name:       "id".to_string(),
                        arg_type:   "ID".to_string(),
                        nullable:   false,
                        default:    None,
                        deprecated: None,
                    }],
                    description:  None,
                    sql_source:   Some("v_author".to_string()),
                    auto_params:  None,
                    deprecated:   None,
                    jsonb_column: None,
                    relay: false,
                     inject: IndexMap::default(),
                cache_ttl_seconds: None,
                additional_views: vec![],
                requires_role: None,
                    relay_cursor_type: None,
                },
            ],
            enums: vec![IntermediateEnum {
                name:        "Status".to_string(),
                values:      vec![
                    IntermediateEnumValue {
                        name:        "ACTIVE".to_string(),
                        description: None,
                        deprecated:  None,
                    },
                    IntermediateEnumValue {
                        name:        "INACTIVE".to_string(),
                        description: None,
                        deprecated:  None,
                    },
                ],
                description: None,
            }],
            ..IntermediateSchema::default()
        }
    }

    #[test]
    fn test_python_generator() {
        let schema = sample_schema();
        let code = PythonGenerator.generate(&schema);
        assert!(code.contains("import fraiseql"));
        assert!(code.contains("@fraiseql.type(sql_source=\"v_author\")"));
        assert!(code.contains("class Author:"));
        assert!(code.contains("    pk: int"));
        assert!(code.contains("    id: ID"));
        assert!(code.contains("    name: str"));
        assert!(code.contains("    bio: str | None"));
        assert!(code.contains(
            "@fraiseql.query(return_type=Author, return_array=True, sql_source=\"v_author\")"
        ));
        assert!(code.contains("def authors() -> list[Author]:"));
        assert!(code.contains("def author(*, id: ID) -> Author:"));
    }

    #[test]
    fn test_typescript_generator() {
        let schema = sample_schema();
        let code = TypeScriptGenerator.generate(&schema);
        assert!(code.contains("import { type_, query } from \"fraiseql\""));
        assert!(code.contains("type_(\"Author\""));
        assert!(code.contains("pk: { type: \"Int\", nullable: false }"));
        assert!(code.contains("bio: { type: \"String\", nullable: true }"));
        assert!(code.contains("query(\"authors\""));
        assert!(code.contains("returnArray: true"));
        assert!(code.contains("{ name: \"id\", type: \"ID\", required: true }"));
    }

    #[test]
    fn test_rust_generator() {
        let schema = sample_schema();
        let code = RustGenerator.generate(&schema);
        assert!(code.contains("use fraiseql::{type_, query}"));
        assert!(code.contains("#[type_(sql_source = \"v_author\")]"));
        assert!(code.contains("pub struct Author {"));
        assert!(code.contains("    pub pk: i32,"));
        assert!(code.contains("    pub id: ID,"));
        assert!(code.contains("    pub name: String,"));
        assert!(code.contains("    pub bio: Option<String>,"));
        assert!(code.contains("#[query(return_type = \"Author\", return_array = true"));
        assert!(code.contains("pub fn authors() -> Vec<Author>"));
        assert!(code.contains("pub fn author(id: ID) -> Author"));
    }

    #[test]
    fn test_kotlin_generator() {
        let schema = sample_schema();
        let code = KotlinGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("data class Author("));
        assert!(code.contains("    val pk: Int,"));
        assert!(code.contains("    val id: ID,"));
        assert!(code.contains("    val name: String,"));
        assert!(code.contains("    val bio: String?,"));
        assert!(code.contains("@Query(returnType = Author::class"));
        assert!(code.contains("fun authors(): List<Author> = TODO()"));
        assert!(code.contains("fun author(id: ID): Author = TODO()"));
    }

    #[test]
    fn test_swift_generator() {
        let schema = sample_schema();
        let code = SwiftGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource: \"v_author\")"));
        assert!(code.contains("struct Author {"));
        assert!(code.contains("    let pk: Int"));
        assert!(code.contains("    let id: ID"));
        assert!(code.contains("    let name: String"));
        assert!(code.contains("    let bio: String?"));
        assert!(code.contains("@Query(returnType: Author.self"));
        assert!(code.contains("func authors() -> [Author]"));
        assert!(code.contains("func author(id: ID) -> Author"));
    }

    #[test]
    fn test_scala_generator() {
        let schema = sample_schema();
        let code = ScalaGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("case class Author("));
        assert!(code.contains("  pk: Int,"));
        assert!(code.contains("  id: ID,"));
        assert!(code.contains("  name: String,"));
        assert!(code.contains("  bio: Option[String]"));
        assert!(code.contains("@Query(returnType = classOf[Author]"));
        assert!(code.contains("def authors(): List[Author] = ???"));
        assert!(code.contains("def author(id: ID): Author = ???"));
    }

    #[test]
    fn test_java_generator() {
        let schema = sample_schema();
        let code = JavaGenerator.generate(&schema);
        assert!(code.contains("@Type(sqlSource = \"v_author\")"));
        assert!(code.contains("public record Author("));
        assert!(code.contains("    int pk,"));
        assert!(code.contains("    ID id,"));
        assert!(code.contains("    String name,"));
        assert!(code.contains("    @Nullable String bio"));
        assert!(code.contains("@Query(returnType = Author.class, returnArray = true"));
        assert!(code.contains("public interface Authors {}"));
        assert!(code.contains("@Arg(name = \"id\", type = \"ID\", required = true)"));
        assert!(code.contains("public interface AuthorById {}"));
    }

    #[test]
    fn test_go_generator() {
        let schema = sample_schema();
        let code = GoGenerator.generate(&schema);
        assert!(code.contains("package schema"));
        assert!(code.contains("import \"fraiseql\""));
        assert!(code.contains("// @Type(sqlSource = \"v_author\")"));
        assert!(code.contains("type Author struct {"));
        assert!(code.contains("\tPk int `fraiseql:\"pk\"`"));
        assert!(code.contains("\tId ID `fraiseql:\"id\"`"));
        assert!(code.contains("\tName string `fraiseql:\"name\"`"));
        assert!(code.contains("\tBio *string `fraiseql:\"bio\"`"));
        assert!(code.contains("func init() {"));
        assert!(code.contains("RegisterQuery(\"authors\""));
        assert!(code.contains("ReturnArray: true"));
        assert!(code.contains("{Name: \"id\", Type: \"ID\", Required: true}"));
    }

    #[test]
    fn test_csharp_generator() {
        let schema = sample_schema();
        let code = CSharpGenerator.generate(&schema);
        assert!(code.contains("[Type(SqlSource = \"v_author\")]"));
        assert!(code.contains("public record Author("));
        assert!(code.contains("    int Pk,"));
        assert!(code.contains("    ID Id,"));
        assert!(code.contains("    string Name,"));
        assert!(code.contains("    string? Bio"));
        assert!(code.contains("[Query(ReturnType = typeof(Author), ReturnArray = true"));
        assert!(code.contains("public static partial class Authors;"));
        assert!(code.contains("Arg(Name = \"id\", Type = \"ID\", Required = true)"));
        assert!(code.contains("public static partial class AuthorById;"));
    }

    #[test]
    fn test_empty_schema() {
        let schema = IntermediateSchema::default();
        let code = PythonGenerator.generate(&schema);
        assert!(code.contains("import fraiseql"));
        assert!(!code.contains("class "));
        assert!(!code.contains("def "));
    }
}
