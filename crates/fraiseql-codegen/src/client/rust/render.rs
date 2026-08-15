//! Rust-specific rendering: `FieldType` → Rust type expressions, input
//! type-string parsing, and identifier casing/escaping.
//!
//! The generated client targets the **2021 edition** and depends only on `serde`
//! and `serde_json`; the shared document machinery lives in `client::common`.

use fraiseql_core::schema::FieldType;

/// Map a `GraphQL` named-scalar to its Rust type, if it is a known scalar.
///
/// Returns `None` for names that are not built-in/rich scalars (object, enum,
/// input, union, interface names) — the caller passes those through verbatim.
pub(super) fn named_scalar_rs(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "string" | "id" | "uuid" | "decimal" | "datetime" | "timestamp" | "date" | "time" => {
            Some("String")
        },
        "int" | "integer" => Some("i32"),
        "float" | "double" => Some("f64"),
        "boolean" | "bool" => Some("bool"),
        "json" | "jsonb" => Some("serde_json::Value"),
        _ => None,
    }
}

/// Render a structured [`FieldType`] as a Rust type expression (no nullability).
pub(super) fn field_type_rs(ft: &FieldType) -> String {
    match ft {
        FieldType::String
        | FieldType::Id
        | FieldType::Uuid
        | FieldType::Decimal
        | FieldType::DateTime
        | FieldType::Date
        | FieldType::Time
        | FieldType::Scalar(_)
        // A bit vector arrives as its text form, a run of '0'/'1'; a sparse one as
        // pgvector's own `{1:0.5,7:0.25}/1000` (#959).
        | FieldType::BitVector
        | FieldType::SparseVector => "String".to_string(),
        // GraphQL's Int is a 32-bit signed integer; `i32` is the type that says so.
        FieldType::Int => "i32".to_string(),
        FieldType::Float => "f64".to_string(),
        FieldType::Boolean => "bool".to_string(),
        // Half precision is a storage choice, not a surface one: both are
        // `[Float!]!` in GraphQL.
        FieldType::Vector | FieldType::HalfVector => "Vec<f64>".to_string(),
        FieldType::List(inner) => format!("Vec<{}>", field_type_rs(inner)),
        FieldType::Enum(name)
        | FieldType::Object(name)
        | FieldType::Input(name)
        | FieldType::Interface(name)
        | FieldType::Union(name) => name.clone(),
        // Reason: `Json` maps to `serde_json::Value`; the wildcard also covers any
        // future #[non_exhaustive] scalar variant. `FieldType` is #[non_exhaustive]
        // and lives in another crate, so an exhaustive match is not available here
        // — `every_field_type_maps_to_its_own_surface` is what keeps a newly added
        // variant from silently degrading to an untyped `Value` (#959's shape).
        _ => "serde_json::Value".to_string(),
    }
}

/// Render a [`FieldType`] with outer nullability applied (`T` / `Option<T>`).
pub(super) fn field_type_rs_nullable(ft: &FieldType, nullable: bool) -> String {
    let base = field_type_rs(ft);
    if nullable {
        format!("Option<{base}>")
    } else {
        base
    }
}

/// Whether `ft` represents a custom/rich scalar — used to attach a
/// `// TODO: brand` note above the field, mirroring the other generators.
pub(super) fn custom_scalar_name(ft: &FieldType) -> Option<&str> {
    match ft {
        FieldType::Scalar(name) => Some(name),
        FieldType::List(inner) => custom_scalar_name(inner),
        _ => None,
    }
}

/// A parsed input-field `GraphQL` type string, rendered to Rust.
pub(super) struct ParsedInputType {
    /// Rust type expression (e.g. `String`, `Vec<Option<String>>`).
    pub rs:       String,
    /// Whether the outermost type is non-null (`!`) — drives `Option`/`skip`.
    pub required: bool,
}

/// Parse an input-field `GraphQL` type **string** (`"String!"`, `"[Int]"`,
/// `"UserRole"`) into a Rust type, preserving full `!`/`[]` nullability.
pub(super) fn parse_input_type(type_str: &str) -> ParsedInputType {
    let s = type_str.trim();
    let (s, required) = match s.strip_suffix('!') {
        Some(rest) => (rest.trim_end(), true),
        None => (s, false),
    };

    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let inner_parsed = parse_input_type(inner);
        let element = if inner_parsed.required {
            inner_parsed.rs
        } else {
            format!("Option<{}>", inner_parsed.rs)
        };
        return ParsedInputType {
            rs: format!("Vec<{element}>"),
            required,
        };
    }

    let rs = named_scalar_rs(s).map_or_else(|| s.to_string(), str::to_string);
    ParsedInputType { rs, required }
}

/// Rust's strict and reserved keywords (2015 + 2018 + 2021 editions).
///
/// A `GraphQL` name matches `[A-Za-z_][A-Za-z0-9_]*`, so once it is snake-cased
/// the only way it can fail to be an identifier is by landing on one of these.
const RS_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "gen",
];

/// Whether `name` is a Rust keyword (strict or reserved).
pub(super) fn is_rs_keyword(name: &str) -> bool {
    RS_KEYWORDS.contains(&name)
}

/// A keyword-safe identifier: `type` → `type_`, everything else verbatim.
///
/// The trailing underscore is used rather than a raw identifier because `r#`
/// is not legal for every keyword (`r#crate`, `r#self`, `r#super`, `r#Self` are
/// all rejected) — one rule that always works beats two that nearly do. The
/// `GraphQL` name is preserved on the wire by a `#[serde(rename)]` or the
/// variables map, so the escape never leaks into a request.
pub(super) fn escape_keyword(name: &str) -> String {
    if is_rs_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// `displayName` → `display_name`, `postsConnection` → `posts_connection`.
///
/// An uppercase run is kept together and broken before its final letter when a
/// lowercase one follows (`userIDValue` → `user_id_value`), which is the
/// convention `rustfmt`'s own casing lint expects.
pub(super) fn snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() {
            let prev_is_lower_or_digit =
                i > 0 && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit());
            let ends_an_uppercase_run = i > 0
                && chars[i - 1].is_ascii_uppercase()
                && chars.get(i + 1).is_some_and(char::is_ascii_lowercase);
            if (prev_is_lower_or_digit || ends_an_uppercase_run) && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// A snake-cased, keyword-safe Rust identifier for a field or parameter.
pub(super) fn rs_ident(name: &str) -> String {
    escape_keyword(&snake_case(name))
}

/// `ADMIN` → `Admin`, `IN_PROGRESS` → `InProgress` — a `GraphQL` enum value as a
/// Rust variant name.
///
/// `SELF` would land on the reserved `Self`, so the keyword escape applies here
/// too; the wire spelling is restored by `#[serde(rename)]` either way.
pub(super) fn pascal_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for part in value.split('_').filter(|p| !p.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    if out.is_empty() || !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert(0, 'V');
    }
    escape_keyword(&out)
}
