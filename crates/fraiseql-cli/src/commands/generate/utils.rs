//! Shared utilities for language code generators.

use super::super::init::Language;
use crate::schema::intermediate::{IntermediateQuery};

// =============================================================================
// Shared utilities
// =============================================================================

/// Convert `snake_case` to `camelCase`.
pub(super) fn to_camel_case(s: &str) -> String {
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
pub(super) fn to_pascal_case(s: &str) -> String {
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
pub(super) fn map_graphql_to_lang(lang: Language, graphql_type: &str) -> String {
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
        Language::Php => match graphql_type {
            "Int" => "int".to_string(),
            "Float" => "float".to_string(),
            "Boolean" => "bool".to_string(),
            "String" => "string".to_string(),
            "ID" => "string".to_string(),
            "DateTime" => "string".to_string(),
            other => other.to_string(),
        },
    }
}

/// Wrap a type string with language-specific nullable syntax.
pub(super) fn wrap_nullable(lang: Language, type_str: &str) -> String {
    match lang {
        Language::Python => format!("{type_str} | None"),
        Language::Rust => format!("Option<{type_str}>"),
        Language::Kotlin | Language::Swift | Language::CSharp => format!("{type_str}?"),
        Language::Go => format!("*{type_str}"),
        Language::Scala => format!("Option[{type_str}]"),
        // TypeScript/Java handle nullable differently (not in type syntax)
        Language::TypeScript | Language::Java => type_str.to_string(),
        // PHP uses ?Type prefix
        Language::Php => format!("?{type_str}"),
    }
}

/// Derive a PascalCase class/interface name from a query.
/// List query "posts" → "Posts", single query "post" with args → "PostById".
pub(super) fn derive_class_name(query: &IntermediateQuery) -> String {
    let base = to_pascal_case(&query.name);
    if !query.returns_list && !query.arguments.is_empty() {
        format!("{base}ById")
    } else {
        base
    }
}

/// Infer a SQL source name from a type name: "Author" → "v_author".
pub(super) fn infer_sql_source(type_name: &str) -> String {
    let mut result = String::from("v_");
    for (i, ch) in type_name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}
