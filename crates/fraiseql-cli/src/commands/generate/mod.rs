//! `fraiseql generate` — Generate authoring-language source from schema.json
//!
//! The inverse of `fraiseql extract`: reads a schema.json and produces annotated
//! source code in any of the 9 supported authoring languages.

use std::fs;

use anyhow::{Context, Result};
use tracing::info;

use super::init::Language;
use crate::schema::intermediate::IntermediateSchema;

mod csharp;
mod go_lang;
mod java;
mod kotlin;
mod php;
mod python;
mod rust_lang;
mod scala;
mod swift;
mod typescript;
mod utils;

#[cfg(test)]
mod tests;

// =============================================================================
// Trait
// =============================================================================

/// Trait for language-specific code generation from intermediate schema.
pub(super) trait SchemaGenerator {
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
        Language::Php => "schema.php".to_string(),
    }
}

fn dispatch_generator(lang: Language, schema: &IntermediateSchema) -> String {
    match lang {
        Language::Python => python::PythonGenerator.generate(schema),
        Language::TypeScript => typescript::TypeScriptGenerator.generate(schema),
        Language::Rust => rust_lang::RustGenerator.generate(schema),
        Language::Kotlin => kotlin::KotlinGenerator.generate(schema),
        Language::Swift => swift::SwiftGenerator.generate(schema),
        Language::Scala => scala::ScalaGenerator.generate(schema),
        Language::Java => java::JavaGenerator.generate(schema),
        Language::Go => go_lang::GoGenerator.generate(schema),
        Language::CSharp => csharp::CSharpGenerator.generate(schema),
        Language::Php => php::PhpGenerator.generate(schema),
    }
}
