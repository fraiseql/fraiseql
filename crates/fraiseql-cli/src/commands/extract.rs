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
use crate::schema::intermediate::{
    IntermediateArgument, IntermediateField, IntermediateQuery, IntermediateSchema,
    IntermediateType,
};

// =============================================================================
// Core types
// =============================================================================

/// Extracted schema from a single source file.
struct ExtractedSchema {
    types:   Vec<IntermediateType>,
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
pub fn run(
    inputs: &[String],
    language_override: Option<&str>,
    recursive: bool,
    output: &str,
) -> Result<()> {
    let override_lang = language_override
        .map(|s| {
            s.parse::<Language>()
                .map_err(|e| anyhow::anyhow!(e))
        })
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

    let json = serde_json::to_string_pretty(&schema)
        .context("Failed to serialize schema")?;
    fs::write(output, &json)
        .with_context(|| format!("Failed to write {output}"))?;

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
    }
}

// =============================================================================
// Shared utilities
// =============================================================================

/// Parse annotation parameters from a string like `key = "value", key2 = true`.
fn parse_annotation_params(s: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    // Match key = "value", key: "value", key = true, key = false, key = ClassName
    let re = Regex::new(
        r#"(\w+)\s*[=:]\s*(?:"([^"]*)"|'([^']*)'|(true|false)|(\w[\w.<>\[\]:]*(?:::class|\.class|\.self)?))"#,
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
            if let Some(base) = trimmed.strip_suffix("| None").or_else(|| trimmed.strip_suffix("|None")) {
                return (base.trim().to_string(), true);
            }
            // `Optional[str]`
            if let Some(inner) = trimmed.strip_prefix("Optional[").and_then(|s| s.strip_suffix(']')) {
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
    }

    (trimmed.to_string(), false)
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

// =============================================================================
// Python extractor
// =============================================================================

struct PythonExtractor;

impl SchemaExtractor for PythonExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        let type_re =
            Regex::new(r"@fraiseql\.type\(([^)]*)\)\s*\nclass\s+(\w+)")?;
        let field_re = Regex::new(r"^\s+(\w+):\s*(.+?)\s*$")?;
        let query_re =
            Regex::new(r"@fraiseql\.query\(([^)]*)\)\s*\ndef\s+(\w+)")?;

        let lines: Vec<&str> = source.lines().collect();

        // Extract types
        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            // Find class body: lines after "class Name:" that are indented
            // Match ends after "class Name", skip to next line for body
            let match_end = cap.get(0).unwrap().end();
            let body_start_line = source[..match_end].lines().count();
            let mut fields = Vec::new();
            for line in lines.iter().skip(body_start_line) {
                // Skip blank lines and docstrings
                let trimmed = line.trim();
                if trimmed.is_empty()
                    || trimmed.starts_with('#')
                    || trimmed.starts_with("\"\"\"")
                    || trimmed.starts_with("'''")
                {
                    continue;
                }
                // Stop at next class/function/decorator at column 0
                if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
                    break;
                }
                if let Some(fcap) = field_re.captures(line) {
                    let field_name = fcap[1].to_string();
                    let type_str = fcap[2].to_string();
                    let (graphql_type, nullable) = map_type(Language::Python, &type_str);
                    fields.push(IntermediateField {
                        name:           field_name,
                        field_type:     graphql_type,
                        nullable,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                    });
                }
            }

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        // Extract queries
        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params
                .get("return_type")
                .cloned()
                .unwrap_or_default();
            let returns_list = params
                .get("return_array")
                .is_some_and(|v| v == "true" || v == "True");
            let sql_source = params.get("sql_source").cloned();

            // Parse function arguments (skip self, *, etc.)
            let arguments = extract_python_query_args(source, cap.get(0).unwrap().end());

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_python_query_args(source: &str, fn_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Find the function signature between parens
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

    let arg_re = Regex::new(r"(\w+):\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = &cap[1];
        // Skip 'self' and bare '*'
        if name == "self" {
            continue;
        }
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
        args.push(IntermediateArgument {
            name:       name.to_string(),
            arg_type:   graphql_type,
            nullable:   false,
            default:    None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// TypeScript extractor
// =============================================================================

struct TypeScriptExtractor;

impl SchemaExtractor for TypeScriptExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // Find type_("Name" and extract balanced braces body
        let type_start_re = Regex::new(r#"type_\(\s*"(\w+)"\s*,"#)?;
        // Find query("name" and extract balanced braces body
        let query_start_re = Regex::new(r#"query\(\s*"(\w+)"\s*,"#)?;

        for cap in type_start_re.captures_iter(source) {
            let name = cap[1].to_string();
            let after_match = cap.get(0).unwrap().end();
            if let Some(body) = extract_balanced_braces(&source[after_match..]) {
                let fields = extract_ts_fields(&body);
                types.push(IntermediateType {
                    name,
                    fields,
                    description: None,
                    implements: Vec::new(),
                });
            }
        }

        for cap in query_start_re.captures_iter(source) {
            let name = cap[1].to_string();
            let after_match = cap.get(0).unwrap().end();
            if let Some(body) = extract_balanced_braces(&source[after_match..]) {
                let params = parse_ts_query_params(&body);
                let return_type = params
                    .get("returnType")
                    .cloned()
                    .unwrap_or_default();
                let returns_list = params
                    .get("returnArray")
                    .is_some_and(|v| v == "true");
                let sql_source = params.get("sqlSource").cloned();
                let arguments = extract_ts_query_args(&body);

                queries.push(IntermediateQuery {
                    name,
                    return_type,
                    returns_list,
                    nullable: false,
                    arguments,
                    description: None,
                    sql_source,
                    auto_params: None,
                    deprecated: None,
                    jsonb_column: None,
                });
            }
        }

        Ok(ExtractedSchema { types, queries })
    }
}

/// Extract text inside balanced braces `{ ... }` from the start of `s`.
fn extract_balanced_braces(s: &str) -> Option<String> {
    let start = s.find('{')?;
    let mut depth = 0;
    for (i, ch) in s[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            },
            _ => {},
        }
    }
    None
}

fn extract_ts_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Match: fieldName: { type: "Type", nullable: bool }
    let field_re = Regex::new(
        r#"(\w+)\s*:\s*\{\s*type\s*:\s*"(\w+)"\s*,\s*nullable\s*:\s*(true|false)\s*\}"#,
    )
    .expect("valid regex");

    for cap in field_re.captures_iter(body) {
        fields.push(IntermediateField {
            name:           cap[1].to_string(),
            field_type:     cap[2].to_string(),
            nullable:       &cap[3] == "true",
            description:    None,
            directives:     None,
            requires_scope: None,
        });
    }
    fields
}

fn parse_ts_query_params(body: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    // returnType: "Type"
    let str_re =
        Regex::new(r#"(\w+)\s*:\s*"(\w+)""#).expect("valid regex");
    for cap in str_re.captures_iter(body) {
        params.insert(cap[1].to_string(), cap[2].to_string());
    }
    // returnArray: true/false
    let bool_re =
        Regex::new(r"(\w+)\s*:\s*(true|false)").expect("valid regex");
    for cap in bool_re.captures_iter(body) {
        params.insert(cap[1].to_string(), cap[2].to_string());
    }
    params
}

fn extract_ts_query_args(body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // args: [{ name: "id", type: "ID", required: true }]
    let arg_re = Regex::new(
        r#"name\s*:\s*"(\w+)"\s*,\s*type\s*:\s*"(\w+)"\s*,\s*required\s*:\s*(true|false)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(body) {
        args.push(IntermediateArgument {
            name:       cap[1].to_string(),
            arg_type:   cap[2].to_string(),
            nullable:   &cap[3] != "true",
            default:    None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// Rust extractor
// =============================================================================

struct RustExtractor;

impl SchemaExtractor for RustExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // #[type_(key = "value")] pub struct Name {
        let type_re =
            Regex::new(r"#\[type_\(([^)]*)\)\]\s*pub\s+struct\s+(\w+)\s*\{")?;
        // #[query(key = "value")] pub fn name
        let query_re =
            Regex::new(r"#\[query\(([^)]*)\)\]\s*pub\s+fn\s+(\w+)")?;
        let field_re = Regex::new(r"^\s*pub\s+(\w+)\s*:\s*(.+?)\s*,?\s*$")?;

        let lines: Vec<&str> = source.lines().collect();

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            let struct_line = source[..cap.get(0).unwrap().start()].lines().count();
            let mut fields = Vec::new();
            for line in lines.iter().skip(struct_line + 1) {
                let trimmed = line.trim();
                if trimmed == "}" {
                    break;
                }
                if let Some(fcap) = field_re.captures(line) {
                    let field_name = fcap[1].to_string();
                    let type_str = fcap[2].to_string();
                    let (graphql_type, nullable) = map_type(Language::Rust, &type_str);
                    fields.push(IntermediateField {
                        name:           field_name,
                        field_type:     graphql_type,
                        nullable,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                    });
                }
            }

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("return_type").cloned().unwrap_or_default();
            let returns_list = params.get("return_array").is_some_and(|v| v == "true");
            let sql_source = params.get("sql_source").cloned();

            let arguments = extract_rust_query_args(source, cap.get(0).unwrap().end());

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_rust_query_args(source: &str, fn_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

    let arg_re = Regex::new(r"(\w+)\s*:\s*(\S+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = cap[2].to_string();
        let graphql_type = map_primitive_type(&type_str);
        args.push(IntermediateArgument {
            name,
            arg_type: graphql_type,
            nullable: false,
            default: None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// Java extractor
// =============================================================================

struct JavaExtractor;

impl SchemaExtractor for JavaExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") public record Author(...)
        let type_re =
            Regex::new(r"@Type\(([^)]*)\)\s*public\s+record\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) public interface Name — handle nested parens from @Arg(...) and .class
        let query_re =
            Regex::new(r"@Query\(([^)]*(?:\([^)]*\)[^)]*)*)\)\s*public\s+interface\s+(\w+)")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let record_body = &cap[3];

            let fields = extract_java_record_fields(record_body);

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name_from_interface = &cap[2];
            // Query name: derive from interface name (e.g., Posts → posts, PostById → post)
            let query_name = derive_query_name(name_from_interface);
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();
            let arguments = extract_java_query_args(&cap[1]);

            queries.push(IntermediateQuery {
                name: query_name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_java_record_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Each line: `Type name,` or `@Nullable Type name,`
    let field_re = Regex::new(
        r"(@Nullable\s+)?(\w+)\s+(\w+)\s*[,)]?",
    )
    .expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let nullable = cap.get(1).is_some();
        let type_str = &cap[2];
        let raw_name = &cap[3];
        let field_name = to_snake_case(raw_name);
        let graphql_type = map_primitive_type(type_str);

        fields.push(IntermediateField {
            name:           field_name,
            field_type:     graphql_type,
            nullable,
            description:    None,
            directives:     None,
            requires_scope: None,
        });
    }
    fields
}

fn extract_java_query_args(annotation_body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // @Arg(name = "id", type = "ID", required = true)
    let arg_re = Regex::new(
        r#"@Arg\(\s*name\s*=\s*"(\w+)"\s*,\s*type\s*=\s*"(\w+)"\s*,\s*required\s*=\s*(true|false)\s*\)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(annotation_body) {
        args.push(IntermediateArgument {
            name:       cap[1].to_string(),
            arg_type:   cap[2].to_string(),
            nullable:   &cap[3] != "true",
            default:    None,
            deprecated: None,
        });
    }
    args
}

/// Derive query name from interface/class name.
/// Posts → posts, PostById → post, Authors → authors, AuthorById → author, Tags → tags
fn derive_query_name(interface_name: &str) -> String {
    // "ById" suffix → singular, without ById
    if let Some(base) = interface_name.strip_suffix("ById") {
        return to_snake_case(base)
            .to_lowercase();
    }
    // Otherwise just lowercase the whole thing
    to_snake_case(interface_name).to_lowercase()
}

// =============================================================================
// Kotlin extractor
// =============================================================================

struct KotlinExtractor;

impl SchemaExtractor for KotlinExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") data class Author(...)
        let type_re =
            Regex::new(r"@Type\(([^)]*)\)\s*data\s+class\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) fun name(
        let query_re =
            Regex::new(r"@Query\(([^)]*)\)\s*fun\s+(\w+)\s*\(")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_kotlin_fields(body);
            let description = params.get("description").cloned();

            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();

            let arguments = extract_kotlin_query_args(source, cap.get(0).unwrap().end());

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_kotlin_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // val name: Type, or val name: Type?,
    let field_re = Regex::new(r"val\s+(\w+)\s*:\s*(\w+\??)").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let raw_name = &cap[1];
        let type_str = &cap[2];
        let field_name = to_snake_case(raw_name);
        let (graphql_type, nullable) = map_type(Language::Kotlin, type_str);

        fields.push(IntermediateField {
            name:           field_name,
            field_type:     graphql_type,
            nullable,
            description:    None,
            directives:     None,
            requires_scope: None,
        });
    }
    fields
}

fn extract_kotlin_query_args(source: &str, fn_paren_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // The regex already consumed up to "(", so we're right after it
    let rest = &source[fn_paren_start..];
    let Some(close) = rest.find(')') else {
        return args;
    };
    let sig = &rest[..close];

    let arg_re = Regex::new(r"(\w+)\s*:\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
        args.push(IntermediateArgument {
            name,
            arg_type: graphql_type,
            nullable: false,
            default: None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// Go extractor
// =============================================================================

struct GoExtractor;

impl SchemaExtractor for GoExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // // @Type(sqlSource = "v_author")
        // type Author struct {
        let type_re = Regex::new(
            r"//\s*@Type\(([^)]*)\)\s*\ntype\s+(\w+)\s+struct\s*\{",
        )?;
        // fraiseql.RegisterQuery("name", fraiseql.QueryDef{...})
        let query_re = Regex::new(
            r#"RegisterQuery\(\s*"(\w+)"\s*,\s*fraiseql\.QueryDef\{([^}]*(?:\{[^}]*\}[^}]*)*)\}"#,
        )?;

        let lines: Vec<&str> = source.lines().collect();

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            let struct_line = source[..cap.get(0).unwrap().end()].lines().count() - 1;
            let fields = extract_go_struct_fields(&lines, struct_line + 1);

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let name = cap[1].to_string();
            let body = &cap[2];
            let params = parse_annotation_params(body);
            let return_type = params.get("ReturnType").cloned().unwrap_or_default();
            let returns_list = params.get("ReturnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("SQLSource").cloned();
            let arguments = extract_go_query_args(body);

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_go_struct_fields(lines: &[&str], start: usize) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // Go struct field: Name Type `fraiseql:"field_name"`
    let field_re = Regex::new(
        r#"^\s+(\w+)\s+(\*?\w+)\s+`fraiseql:"(\w+)"`"#,
    )
    .expect("valid regex");

    for line in lines.iter().skip(start) {
        let trimmed = line.trim();
        if trimmed == "}" {
            break;
        }
        if let Some(cap) = field_re.captures(line) {
            let type_str = &cap[2];
            let tag_name = cap[3].to_string();
            let (graphql_type, nullable) = map_type(Language::Go, type_str);

            fields.push(IntermediateField {
                name:           tag_name,
                field_type:     graphql_type,
                nullable,
                description:    None,
                directives:     None,
                requires_scope: None,
            });
        }
    }
    fields
}

fn extract_go_query_args(body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Args: []fraiseql.Arg{{Name: "id", Type: "ID", Required: true}}
    let arg_re = Regex::new(
        r#"Name\s*:\s*"(\w+)"\s*,\s*Type\s*:\s*"(\w+)"\s*,\s*Required\s*:\s*(true|false)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(body) {
        args.push(IntermediateArgument {
            name:       cap[1].to_string(),
            arg_type:   cap[2].to_string(),
            nullable:   &cap[3] != "true",
            default:    None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// C# extractor
// =============================================================================

struct CSharpExtractor;

impl SchemaExtractor for CSharpExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // [Type(SqlSource = "v_author")] public record Author(...)
        let type_re =
            Regex::new(r"\[Type\(([^)]*)\)\]\s*public\s+record\s+(\w+)\s*\(([^)]*)\)")?;
        // [Query(...)] public static partial class Name — handle nested parens from typeof(...)
        let query_re = Regex::new(
            r"\[Query\(([^)]*(?:\([^)]*\)[^)]*)*)\)\]\s*public\s+static\s+partial\s+class\s+(\w+)",
        )?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_csharp_record_fields(body);
            let description = params.get("description").cloned();

            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let class_name = &cap[2];
            let query_name = derive_query_name(class_name);
            let return_type = params.get("ReturnType").cloned().unwrap_or_default();
            let returns_list = params.get("ReturnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("SqlSource").cloned();

            let arguments = extract_csharp_query_args(&cap[1]);

            queries.push(IntermediateQuery {
                name: query_name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_csharp_query_args(annotation_body: &str) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    // Arg(Name = "id", Type = "ID", Required = true)
    let arg_re = Regex::new(
        r#"Arg\(\s*Name\s*=\s*"(\w+)"\s*,\s*Type\s*=\s*"(\w+)"\s*,\s*Required\s*=\s*(true|false)\s*\)"#,
    )
    .expect("valid regex");

    for cap in arg_re.captures_iter(annotation_body) {
        args.push(IntermediateArgument {
            name:       cap[1].to_string(),
            arg_type:   cap[2].to_string(),
            nullable:   &cap[3] != "true",
            default:    None,
            deprecated: None,
        });
    }
    args
}

fn extract_csharp_record_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // C# record: Type? Name, or Type Name
    let field_re = Regex::new(r"(\w+)(\??)\s+(\w+)\s*[,)]?").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let type_str = &cap[1];
        let nullable_marker = &cap[2];
        let raw_name = &cap[3];
        let field_name = to_snake_case(raw_name);
        let graphql_type = map_primitive_type(type_str);
        let nullable = nullable_marker == "?";

        fields.push(IntermediateField {
            name:           field_name,
            field_type:     graphql_type,
            nullable,
            description:    None,
            directives:     None,
            requires_scope: None,
        });
    }
    fields
}

// =============================================================================
// Swift extractor
// =============================================================================

struct SwiftExtractor;

impl SchemaExtractor for SwiftExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource: "v_author") struct Author {
        let type_re =
            Regex::new(r"@Type\(([^)]*)\)\s*struct\s+(\w+)\s*\{")?;
        // @Query(...) func name(
        let query_re =
            Regex::new(r"@Query\(([^)]*)\)\s*func\s+(\w+)\s*\(")?;
        let field_re = Regex::new(r"^\s*let\s+(\w+)\s*:\s*(\w+\??)")?;

        let lines: Vec<&str> = source.lines().collect();

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();

            let struct_line = source[..cap.get(0).unwrap().end()].lines().count() - 1;
            let mut fields = Vec::new();
            for line in lines.iter().skip(struct_line + 1) {
                let trimmed = line.trim();
                if trimmed == "}" {
                    break;
                }
                if let Some(fcap) = field_re.captures(line) {
                    let raw_name = &fcap[1];
                    let type_str = &fcap[2];
                    let field_name = to_snake_case(raw_name);
                    let (graphql_type, nullable) = map_type(Language::Swift, type_str);

                    fields.push(IntermediateField {
                        name:           field_name,
                        field_type:     graphql_type,
                        nullable,
                        description:    None,
                        directives:     None,
                        requires_scope: None,
                    });
                }
            }

            let description = params.get("description").cloned();
            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();

            let arguments = extract_swift_query_args(source, cap.get(0).unwrap().end());

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_swift_query_args(source: &str, fn_paren_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    let rest = &source[fn_paren_start..];
    let Some(close) = rest.find(')') else {
        return args;
    };
    let sig = &rest[..close];

    // Swift: id: String
    let arg_re = Regex::new(r"(\w+)\s*:\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
        args.push(IntermediateArgument {
            name,
            arg_type: graphql_type,
            nullable: false,
            default: None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// Scala extractor
// =============================================================================

struct ScalaExtractor;

impl SchemaExtractor for ScalaExtractor {
    fn extract(&self, source: &str) -> Result<ExtractedSchema> {
        let mut types = Vec::new();
        let mut queries = Vec::new();

        // @Type(sqlSource = "v_author") case class Author(...)
        let type_re =
            Regex::new(r"@Type\(([^)]*)\)\s*case\s+class\s+(\w+)\s*\(([^)]*)\)")?;
        // @Query(...) def name(
        let query_re =
            Regex::new(r"@Query\(([^)]*)\)\s*def\s+(\w+)")?;

        for cap in type_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let body = &cap[3];

            let fields = extract_scala_fields(body);
            let description = params.get("description").cloned();

            types.push(IntermediateType {
                name,
                fields,
                description,
                implements: Vec::new(),
            });
        }

        for cap in query_re.captures_iter(source) {
            let params = parse_annotation_params(&cap[1]);
            let name = cap[2].to_string();
            let return_type = params.get("returnType").cloned().unwrap_or_default();
            let returns_list = params.get("returnArray").is_some_and(|v| v == "true");
            let sql_source = params.get("sqlSource").cloned();

            let arguments = extract_scala_query_args(source, cap.get(0).unwrap().end());

            queries.push(IntermediateQuery {
                name,
                return_type,
                returns_list,
                nullable: false,
                arguments,
                description: None,
                sql_source,
                auto_params: None,
                deprecated: None,
                jsonb_column: None,
            });
        }

        Ok(ExtractedSchema { types, queries })
    }
}

fn extract_scala_fields(body: &str) -> Vec<IntermediateField> {
    let mut fields = Vec::new();
    // name: Type, or name: Option[Type]
    let field_re = Regex::new(r"(\w+)\s*:\s*(Option\[\w+\]|\w+)").expect("valid regex");

    for cap in field_re.captures_iter(body) {
        let raw_name = &cap[1];
        let type_str = &cap[2];
        let field_name = to_snake_case(raw_name);
        let (graphql_type, nullable) = map_type(Language::Scala, type_str);

        fields.push(IntermediateField {
            name:           field_name,
            field_type:     graphql_type,
            nullable,
            description:    None,
            directives:     None,
            requires_scope: None,
        });
    }
    fields
}

fn extract_scala_query_args(source: &str, fn_start: usize) -> Vec<IntermediateArgument> {
    let mut args = Vec::new();
    let rest = &source[fn_start..];
    let Some(open) = rest.find('(') else {
        return args;
    };
    let Some(close) = rest[open..].find(')') else {
        return args;
    };
    let sig = &rest[open + 1..open + close];

    let arg_re = Regex::new(r"(\w+)\s*:\s*(\w+)").expect("valid regex");
    for cap in arg_re.captures_iter(sig) {
        let name = cap[1].to_string();
        let type_str = &cap[2];
        let graphql_type = map_primitive_type(type_str);
        args.push(IntermediateArgument {
            name,
            arg_type: graphql_type,
            nullable: false,
            default: None,
            deprecated: None,
        });
    }
    args
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("AuthorName"), "author_name");
        assert_eq!(to_snake_case("postId"), "post_id");
        assert_eq!(to_snake_case("id"), "id");
        assert_eq!(to_snake_case("PK"), "p_k");
        assert_eq!(to_snake_case("pk"), "pk");
    }

    #[test]
    fn test_strip_class_ref() {
        assert_eq!(strip_class_ref("Post.class"), "Post");
        assert_eq!(strip_class_ref("Post.self"), "Post");
        assert_eq!(strip_class_ref("Post::class"), "Post");
        assert_eq!(strip_class_ref("classOf[Post]"), "Post");
        assert_eq!(strip_class_ref("typeof(Post)"), "Post");
        assert_eq!(strip_class_ref("Post"), "Post");
    }

    #[test]
    fn test_parse_annotation_params() {
        let params = parse_annotation_params(r#"sql_source = "v_author", description = "test""#);
        assert_eq!(params.get("sql_source").unwrap(), "v_author");
        assert_eq!(params.get("description").unwrap(), "test");
    }

    #[test]
    fn test_parse_annotation_params_boolean() {
        let params = parse_annotation_params("return_array = true, nullable = false");
        assert_eq!(params.get("return_array").unwrap(), "true");
        assert_eq!(params.get("nullable").unwrap(), "false");
    }

    #[test]
    fn test_parse_annotation_params_class_refs() {
        let params = parse_annotation_params("returnType = Post.class, returnArray = true");
        assert_eq!(params.get("returnType").unwrap(), "Post");
        assert_eq!(params.get("returnArray").unwrap(), "true");

        let params2 = parse_annotation_params("returnType = classOf[Post]");
        assert_eq!(params2.get("returnType").unwrap(), "Post");
    }

    #[test]
    fn test_map_type_python() {
        assert_eq!(map_type(Language::Python, "int"), ("Int".to_string(), false));
        assert_eq!(
            map_type(Language::Python, "str | None"),
            ("String".to_string(), true)
        );
        assert_eq!(map_type(Language::Python, "bool"), ("Boolean".to_string(), false));
    }

    #[test]
    fn test_map_type_rust() {
        assert_eq!(map_type(Language::Rust, "i32"), ("Int".to_string(), false));
        assert_eq!(
            map_type(Language::Rust, "Option<String>"),
            ("String".to_string(), true)
        );
        assert_eq!(map_type(Language::Rust, "bool"), ("Boolean".to_string(), false));
    }

    #[test]
    fn test_map_type_kotlin() {
        assert_eq!(map_type(Language::Kotlin, "Int"), ("Int".to_string(), false));
        assert_eq!(
            map_type(Language::Kotlin, "String?"),
            ("String".to_string(), true)
        );
    }

    #[test]
    fn test_map_type_go() {
        assert_eq!(map_type(Language::Go, "int"), ("Int".to_string(), false));
        assert_eq!(
            map_type(Language::Go, "*string"),
            ("String".to_string(), true)
        );
    }

    #[test]
    fn test_map_type_scala() {
        assert_eq!(
            map_type(Language::Scala, "Option[String]"),
            ("String".to_string(), true)
        );
        assert_eq!(map_type(Language::Scala, "Int"), ("Int".to_string(), false));
    }

    #[test]
    fn test_derive_query_name() {
        assert_eq!(derive_query_name("Posts"), "posts");
        assert_eq!(derive_query_name("PostById"), "post");
        assert_eq!(derive_query_name("Authors"), "authors");
        assert_eq!(derive_query_name("AuthorById"), "author");
        assert_eq!(derive_query_name("Tags"), "tags");
    }

    #[test]
    fn test_python_extractor() {
        let source = r#"
import fraiseql

@fraiseql.type(sql_source="v_author")
class Author:
    pk: int
    id: ID
    name: str
    bio: str | None

@fraiseql.query(return_type=Author, return_array=True, sql_source="v_author")
def authors() -> list[Author]:
    ...

@fraiseql.query(return_type=Author, sql_source="v_author")
def author(*, id: ID) -> Author:
    ...
"#;
        let result = PythonExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].name, "Author");
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].name, "id");
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].name, "name");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert_eq!(result.types[0].fields[3].name, "bio");
        assert_eq!(result.types[0].fields[3].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert_eq!(result.queries[0].name, "authors");
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].name, "author");
        assert!(!result.queries[1].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].name, "id");
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_typescript_extractor() {
        let source = r#"
import { type_, query } from "fraiseql";

export const Author = type_("Author", {
  sqlSource: "v_author",
  fields: {
    pk: { type: "Int", nullable: false },
    id: { type: "ID", nullable: false },
    name: { type: "String", nullable: false },
    bio: { type: "String", nullable: true },
  },
});

export const authors = query("authors", {
  returnType: "Author",
  returnArray: true,
  sqlSource: "v_author",
});

export const author = query("author", {
  returnType: "Author",
  returnArray: false,
  sqlSource: "v_author",
  args: [{ name: "id", type: "ID", required: true }],
});
"#;
        let result = TypeScriptExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].name, "Author");
        assert_eq!(result.types[0].fields.len(), 4);
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
    }

    #[test]
    fn test_rust_extractor() {
        let source = r#"
use fraiseql::{type_, query};

#[type_(sql_source = "v_author")]
pub struct Author {
    pub pk: i32,
    pub id: ID,
    pub name: String,
    pub bio: Option<String>,
}

#[query(return_type = "Author", return_array = true, sql_source = "v_author")]
pub fn authors() -> Vec<Author> {
    unimplemented!()
}

#[query(return_type = "Author", sql_source = "v_author")]
pub fn author(id: ID) -> Author {
    unimplemented!()
}
"#;
        let result = RustExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].name, "Author");
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].name, "id");
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].name, "name");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_java_extractor() {
        let source = r#"
@Type(sqlSource = "v_author")
public record Author(
    int pk,
    ID id,
    String name,
    @Nullable String bio
) {}

@Query(returnType = Author.class, returnArray = true, sqlSource = "v_author")
public interface Authors {}

@Query(returnType = Author.class, sqlSource = "v_author", args = @Arg(name = "id", type = "ID", required = true))
public interface AuthorById {}
"#;
        let result = JavaExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].name, "Author");
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert_eq!(result.queries[0].name, "authors");
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].name, "author");
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_kotlin_extractor() {
        let source = r#"
@Type(sqlSource = "v_author")
data class Author(
    val pk: Int,
    val id: ID,
    val name: String,
    val bio: String?,
)

@Query(returnType = Author::class, returnArray = true, sqlSource = "v_author")
fun authors(): List<Author> = TODO()

@Query(returnType = Author::class, sqlSource = "v_author")
fun author(id: ID): Author = TODO()
"#;
        let result = KotlinExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_go_extractor() {
        let source = r#"
package schema

import "fraiseql"

// @Type(sqlSource = "v_author")
type Author struct {
	PK   int     `fraiseql:"pk"`
	ID   ID      `fraiseql:"id"`
	Name string  `fraiseql:"name"`
	Bio  *string `fraiseql:"bio"`
}

func init() {
	fraiseql.RegisterQuery("authors", fraiseql.QueryDef{ReturnType: "Author", ReturnArray: true, SQLSource: "v_author"})
	fraiseql.RegisterQuery("author", fraiseql.QueryDef{ReturnType: "Author", SQLSource: "v_author", Args: []fraiseql.Arg{{Name: "id", Type: "ID", Required: true}}})
}
"#;
        let result = GoExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
    }

    #[test]
    fn test_csharp_extractor() {
        let source = r#"
[Type(SqlSource = "v_author")]
public record Author(
    int Pk,
    ID Id,
    string Name,
    string? Bio
);

[Query(ReturnType = typeof(Author), ReturnArray = true, SqlSource = "v_author")]
public static partial class Authors;

[Query(ReturnType = typeof(Author), SqlSource = "v_author", Arg(Name = "id", Type = "ID", Required = true))]
public static partial class AuthorById;
"#;
        let result = CSharpExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert_eq!(result.queries[0].name, "authors");
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].name, "author");
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].name, "id");
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_swift_extractor() {
        let source = r#"
@Type(sqlSource: "v_author")
struct Author {
    let pk: Int
    let id: ID
    let name: String
    let bio: String?
}

@Query(returnType: Author.self, returnArray: true, sqlSource: "v_author")
func authors() -> [Author] { fatalError() }

@Query(returnType: Author.self, sqlSource: "v_author")
func author(id: ID) -> Author { fatalError() }
"#;
        let result = SwiftExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_scala_extractor() {
        let source = r#"
@Type(sqlSource = "v_author")
case class Author(
  pk: Int,
  id: ID,
  name: String,
  bio: Option[String]
)

@Query(returnType = classOf[Author], returnArray = true, sqlSource = "v_author")
def authors(): List[Author] = ???

@Query(returnType = classOf[Author], sqlSource = "v_author")
def author(id: ID): Author = ???
"#;
        let result = ScalaExtractor.extract(source).unwrap();
        assert_eq!(result.types.len(), 1);
        assert_eq!(result.types[0].fields.len(), 4);
        assert_eq!(result.types[0].fields[1].field_type, "ID");
        assert_eq!(result.types[0].fields[2].field_type, "String");
        assert!(result.types[0].fields[3].nullable);

        assert_eq!(result.queries.len(), 2);
        assert!(result.queries[0].returns_list);
        assert_eq!(result.queries[1].arguments.len(), 1);
        assert_eq!(result.queries[1].arguments[0].arg_type, "ID");
    }

    #[test]
    fn test_empty_source() {
        let result = PythonExtractor.extract("# no schema here").unwrap();
        assert!(result.types.is_empty());
        assert!(result.queries.is_empty());
    }
}
