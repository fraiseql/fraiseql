//! String transformation utilities for REST resource naming.

use fraiseql_core::schema::QueryDefinition;

use super::{Diagnostic, DiagnosticLevel};

/// Derive the resource name from a list query name or type name.
pub(super) fn derive_resource_name(
    type_name: &str,
    queries: &[&QueryDefinition],
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    // Prefer the list query name as resource name.
    if let Some(list_q) = queries.iter().find(|q| q.returns_list) {
        return list_q.name.clone();
    }

    // Fall back: strip CQRS prefix from sql_source if available, then pluralize.
    if let Some(q) = queries.first() {
        if let Some(ref sql) = q.sql_source {
            let stripped = strip_cqrs_prefix(sql);
            if !stripped.is_empty() {
                return simple_pluralize(stripped);
            }
        }
    }

    // Last resort: lowercase type name + simple pluralize.
    let base = type_name_to_snake(type_name);
    let name = simple_pluralize(&base);
    diagnostics.push(Diagnostic {
        level: DiagnosticLevel::Info,
        message: format!(
            "No list query for type '{type_name}'; derived resource name '{name}' from type name"
        ),
    });
    name
}

/// Strip CQRS prefixes (`v_`, `tv_`, `tb_`) from a SQL identifier.
pub(super) fn strip_cqrs_prefix(name: &str) -> &str {
    name.strip_prefix("v_")
        .or_else(|| name.strip_prefix("tv_"))
        .or_else(|| name.strip_prefix("tb_"))
        .unwrap_or(name)
}

/// Convert `PascalCase` type name to `snake_case`.
pub(super) fn type_name_to_snake(name: &str) -> String {
    let mut result = String::with_capacity(name.len() + 4);
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Very simple English pluralization (covers common cases).
pub(super) fn simple_pluralize(word: &str) -> String {
    if word.ends_with("ics") || word.ends_with("ies") {
        return word.to_string();
    }
    if word.ends_with("ss")
        || word.ends_with('x')
        || word.ends_with("ch")
        || word.ends_with("sh")
        || word.ends_with('s')
    {
        format!("{word}es")
    } else if word.ends_with('y')
        && !word.ends_with("ey")
        && !word.ends_with("ay")
        && !word.ends_with("oy")
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

/// Strip the type-name prefix from a mutation name and kebab-case the remainder.
///
/// `archiveUser` on type `User` → `archive`
/// `updateUserEmail` on type `User` → `update-email`
pub(super) fn derive_action_name(mutation_name: &str, type_name: &str) -> String {
    let lower_mutation = mutation_name.to_ascii_lowercase();
    let lower_type = type_name.to_ascii_lowercase();

    let without_type = if let Some(pos) = lower_mutation.find(&lower_type) {
        let before = &mutation_name[..pos];
        let after = &mutation_name[pos + type_name.len()..];
        format!("{before}{after}")
    } else {
        mutation_name.to_string()
    };

    camel_to_kebab(&without_type)
}

/// Convert a `camelCase` or `PascalCase` string to `kebab-case`.
pub(super) fn camel_to_kebab(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(ch.to_ascii_lowercase());
    }
    // Trim leading dash if first char was uppercase.
    if result.starts_with('-') {
        result.remove(0);
    }
    result
}
