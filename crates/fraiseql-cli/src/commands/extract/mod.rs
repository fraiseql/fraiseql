//! `fraiseql extract` — Parse annotated source files to schema.json
//!
//! Extracts FraiseQL type and query definitions from annotated source files
//! in any of the 9 supported authoring languages. Pure text processing,
//! no language runtime needed.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use regex::Regex;
use tracing::info;

use super::init::Language;
use crate::schema::intermediate::{IntermediateQuery, IntermediateSchema, IntermediateType};

mod csharp;
mod go;
mod java;
mod kotlin;
mod python;
mod rust;
mod scala;
mod swift;
#[cfg(test)]
mod tests;
mod typescript;

use self::{
    csharp::CSharpExtractor, go::GoExtractor, java::JavaExtractor, kotlin::KotlinExtractor,
    python::PythonExtractor, rust::RustExtractor, scala::ScalaExtractor, swift::SwiftExtractor,
    typescript::TypeScriptExtractor,
};

// =============================================================================
// Core types
// =============================================================================

/// Extracted schema from a single source file.
struct ExtractedSchema {
    types: Vec<IntermediateType>,
    queries: Vec<IntermediateQuery>,
}

/// Trait for language-specific schema extraction.
trait SchemaExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema>;
}

// =============================================================================
// Public API
// =============================================================================

/// Run the extract command.
///
/// # Errors
///
/// Returns an error if no source files are found, file I/O fails, or schema extraction
/// encounters invalid syntax in the input files.
pub fn run(
    inputs: &[String],
    language_override: Option<&str>,
    recursive: bool,
    output: &str,
) -> Result<()> {
    let override_lang = language_override
        .map(|s| s.parse::<Language>().map_err(|e| anyhow::anyhow!(e)))
        .transpose()?;

    let mut all_types: Vec<IntermediateType> = Vec::new();
    let mut all_queries: Vec<IntermediateQuery> = Vec::new();

    let files = collect_files(inputs, recursive)?;

    if files.is_empty() {
        anyhow::bail!("No source files found in the provided input paths");
    }

    for file in &files {
        let lang = match override_lang {
            Some(l) => l,
            None => detect_language(file)?,
        };

        let source = fs::read_to_string(file)
            .with_context(|| format!("Failed to read {}", file.display()))?;

        let extracted = dispatch_extractor(lang, &source)
            .with_context(|| format!("Failed to extract from {}", file.display()))?;

        for t in extracted.types {
            if !all_types.iter().any(|existing| existing.name == t.name) {
                all_types.push(t);
            }
        }
        for q in extracted.queries {
            if !all_queries.iter().any(|existing| existing.name == q.name) {
                all_queries.push(q);
            }
        }
    }

    let schema = IntermediateSchema {
        version: "2.0.0".to_string(),
        types: all_types,
        queries: all_queries,
        ..IntermediateSchema::default()
    };

    let json = serde_json::to_string_pretty(&schema).context("Failed to serialize schema")?;
    fs::write(output, &json).with_context(|| format!("Failed to write {output}"))?;

    info!("Extracted {} types and {} queries", schema.types.len(), schema.queries.len());
    println!(
        "Extracted {} types, {} queries → {}",
        schema.types.len(),
        schema.queries.len(),
        output,
    );

    Ok(())
}

// =============================================================================
// File collection
// =============================================================================

fn collect_files(inputs: &[String], recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        let path = PathBuf::from(input);
        if path.is_file() {
            files.push(path);
        } else if path.is_dir() {
            if recursive {
                collect_dir_recursive(&path, &mut files)?;
            } else {
                collect_dir_flat(&path, &mut files)?;
            }
        } else {
            anyhow::bail!("Path does not exist: {input}");
        }
    }
    Ok(files)
}

fn collect_dir_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        let path = entry.path();
        if path.is_file() && is_known_extension(path) {
            files.push(path.to_path_buf());
        }
    }
    Ok(())
}

fn collect_dir_flat(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).context("Failed to read directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_known_extension(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_known_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .and_then(Language::from_extension)
        .is_some()
}

fn detect_language(path: &Path) -> Result<Language> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| anyhow::anyhow!("File has no extension: {}", path.display()))?;
    Language::from_extension(ext)
        .ok_or_else(|| anyhow::anyhow!("Unsupported file extension: .{ext}"))
}

fn dispatch_extractor(lang: Language, source: &str) -> Result<ExtractedSchema> {
    match lang {
        Language::Python => PythonExtractor.extract(source),
        Language::TypeScript => TypeScriptExtractor.extract(source),
        Language::Rust => RustExtractor.extract(source),
        Language::Java => JavaExtractor.extract(source),
        Language::Kotlin => KotlinExtractor.extract(source),
        Language::Go => GoExtractor.extract(source),
        Language::CSharp => CSharpExtractor.extract(source),
        Language::Swift => SwiftExtractor.extract(source),
        Language::Scala => ScalaExtractor.extract(source),
        Language::Php => anyhow::bail!(
            "PHP extraction is handled by the PHP SDK binary (`vendor/bin/fraiseql export`). Run that first to produce schema.json, then use `fraiseql compile`."
        ),
    }
}

// =============================================================================
// Shared utilities
// =============================================================================

/// Parse annotation parameters from a string like `key = "value", key2 = true`.
fn parse_annotation_params(s: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    // Match key = "value", key: "value", key = true, key = false, key = ClassName
    // Also matches typeof(X) for C# and classOf[X] for Scala
    let re = Regex::new(
        r#"(\w+)\s*[=:]\s*(?:"([^"]*)"|'([^']*)'|(true|false)|(\w[\w.<>\[\]:]*(?:::class|\.class|\.self)?(?:\([^)]*\))?))"#,
    )
    .expect("valid regex");

    for cap in re.captures_iter(s) {
        let key = cap[1].to_string();
        let value = if let Some(m) = cap.get(2) {
            m.as_str().to_string()
        } else if let Some(m) = cap.get(3) {
            m.as_str().to_string()
        } else if let Some(m) = cap.get(4) {
            m.as_str().to_string()
        } else if let Some(m) = cap.get(5) {
            strip_class_ref(m.as_str())
        } else {
            continue;
        };
        params.insert(key, value);
    }
    params
}

/// Strip language-specific class references to get the bare type name.
fn strip_class_ref(s: &str) -> String {
    // Post.class → Post, classOf[Post] → Post, typeof(Post) → Post,
    // Post.self → Post, Post::class → Post
    let s = s
        .trim_end_matches(".class")
        .trim_end_matches(".self")
        .trim_end_matches("::class");

    // classOf[Post] → Post
    if let Some(inner) = s.strip_prefix("classOf[").and_then(|s| s.strip_suffix(']')) {
        return inner.to_string();
    }
    // typeof(Post) → Post
    if let Some(inner) = s.strip_prefix("typeof(").and_then(|s| s.strip_suffix(')')) {
        return inner.to_string();
    }

    s.to_string()
}

/// Convert `camelCase` or `PascalCase` to `snake_case`.
fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

/// Map a language-specific type string to (GraphQL type, nullable).
fn map_type(lang: Language, type_str: &str) -> (String, bool) {
    // Handle nullable wrappers first
    let (inner, nullable) = extract_nullable(lang, type_str);
    let graphql = map_primitive_type(&inner);
    (graphql, nullable)
}

fn extract_nullable(lang: Language, type_str: &str) -> (String, bool) {
    let trimmed = type_str.trim();

    match lang {
        Language::Python => {
            // `str | None` or `int | None`
            if let Some(base) =
                trimmed.strip_suffix("| None").or_else(|| trimmed.strip_suffix("|None"))
            {
                return (base.trim().to_string(), true);
            }
            // `Optional[str]`
            if let Some(inner) = trimmed.strip_prefix("Optional[").and_then(|s| s.strip_suffix(']'))
            {
                return (inner.trim().to_string(), true);
            }
        },
        Language::Rust => {
            if let Some(inner) = trimmed.strip_prefix("Option<").and_then(|s| s.strip_suffix('>')) {
                return (inner.trim().to_string(), true);
            }
        },
        Language::Kotlin | Language::Swift | Language::CSharp => {
            if let Some(base) = trimmed.strip_suffix('?') {
                return (base.to_string(), true);
            }
        },
        Language::Go => {
            if let Some(base) = trimmed.strip_prefix('*') {
                return (base.to_string(), true);
            }
        },
        Language::Scala => {
            if let Some(inner) = trimmed.strip_prefix("Option[").and_then(|s| s.strip_suffix(']')) {
                return (inner.trim().to_string(), true);
            }
        },
        Language::Java => {
            // Nullable is handled via @Nullable annotation, not type syntax
        },
        Language::TypeScript => {
            // TypeScript uses explicit `nullable: true` in the object literal
        },
        Language::Php => {
            // PHP uses ?Type prefix for nullable
            if let Some(base) = trimmed.strip_prefix('?') {
                return (base.to_string(), true);
            }
        },
    }

    (trimmed.to_string(), false)
}

/// Derive query name from interface/class name.
/// Posts → posts, PostById → post, Authors → authors, AuthorById → author, Tags → tags
fn derive_query_name(interface_name: &str) -> String {
    // "ById" suffix → singular, without ById
    if let Some(base) = interface_name.strip_suffix("ById") {
        return to_snake_case(base).to_lowercase();
    }
    // Otherwise just lowercase the whole thing
    to_snake_case(interface_name).to_lowercase()
}

fn map_primitive_type(s: &str) -> String {
    match s {
        // Integer types
        "int" | "i32" | "i64" | "Int" | "Integer" | "long" | "Long" | "int32" | "int64" => {
            "Int".to_string()
        },
        // Float types
        "float" | "f32" | "f64" | "Float" | "Double" | "double" | "decimal" | "Decimal"
        | "Float32" | "Float64" => "Float".to_string(),
        // Boolean types
        "bool" | "boolean" | "Boolean" | "Bool" | "BIT" => "Boolean".to_string(),
        // String types
        "str" | "String" | "string" | "&str" | "NVARCHAR" => "String".to_string(),
        // ID type
        "ID" => "ID".to_string(),
        // DateTime
        "DateTime" | "Instant" | "LocalDateTime" | "ZonedDateTime" | "Date" => {
            "DateTime".to_string()
        },
        // Unknown → assume it's a custom type name and pass through
        other => other.to_string(),
    }
}
