//! Python-specific rendering helpers: `FieldType` → Python type expressions,
//! input type-string parsing, and identifier safety.
//!
//! The generated client targets **Python ≥ 3.12** (PEP 695 `type` aliases and
//! generic `TypedDict`s); the shared document machinery lives in
//! `client::common`.

use fraiseql_core::schema::FieldType;

/// Map a `GraphQL` named-scalar to its Python type, if it is a known scalar.
///
/// Returns `None` for names that are not built-in/rich scalars (object, enum,
/// input, union, interface names) — the caller passes those through verbatim.
pub(super) fn named_scalar_py(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "string" | "id" | "uuid" | "decimal" | "datetime" | "timestamp" | "date" | "time" => {
            Some("str")
        },
        "int" | "integer" => Some("int"),
        "float" | "double" => Some("float"),
        "boolean" | "bool" => Some("bool"),
        "json" | "jsonb" => Some("Any"),
        _ => None,
    }
}

/// Render a structured [`FieldType`] as a Python type expression (no nullability).
pub(super) fn field_type_py(ft: &FieldType) -> String {
    match ft {
        FieldType::String
        | FieldType::Id
        | FieldType::Uuid
        | FieldType::Decimal
        | FieldType::DateTime
        | FieldType::Date
        | FieldType::Time
        | FieldType::Scalar(_)
        // A bit vector arrives as its text form, a run of '0'/'1' (#959).
        | FieldType::BitVector => "str".to_string(),
        FieldType::Int => "int".to_string(),
        FieldType::Float => "float".to_string(),
        FieldType::Boolean => "bool".to_string(),
        FieldType::Vector => "list[float]".to_string(),
        FieldType::List(inner) => format!("list[{}]", field_type_py(inner)),
        FieldType::Enum(name)
        | FieldType::Object(name)
        | FieldType::Input(name)
        | FieldType::Interface(name)
        | FieldType::Union(name) => name.clone(),
        // Reason: `Json` maps to `Any`; the wildcard also covers any future
        // #[non_exhaustive] scalar variant, falling back to `Any`.
        _ => "Any".to_string(),
    }
}

/// Render a [`FieldType`] with outer nullability applied (`T` / `T | None`).
pub(super) fn field_type_py_nullable(ft: &FieldType, nullable: bool) -> String {
    let base = field_type_py(ft);
    if nullable {
        format!("{base} | None")
    } else {
        base
    }
}

/// Whether `ft` represents a custom/rich scalar — used to attach a
/// `# TODO: brand` note above the field, mirroring the TS generator.
pub(super) fn custom_scalar_name(ft: &FieldType) -> Option<&str> {
    match ft {
        FieldType::Scalar(name) => Some(name),
        FieldType::List(inner) => custom_scalar_name(inner),
        _ => None,
    }
}

/// A parsed input-field `GraphQL` type string, rendered to Python.
pub(super) struct ParsedInputType {
    /// Python type expression (e.g. `str`, `list[str | None]`).
    pub py:       String,
    /// Whether the outermost type is non-null (`!`) — drives `NotRequired`.
    pub required: bool,
}

/// Parse an input-field `GraphQL` type **string** (`"String!"`, `"[Int]"`,
/// `"UserRole"`) into a Python type, preserving full `!`/`[]` nullability.
pub(super) fn parse_input_type(type_str: &str) -> ParsedInputType {
    let s = type_str.trim();
    let (s, required) = match s.strip_suffix('!') {
        Some(rest) => (rest.trim_end(), true),
        None => (s, false),
    };

    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let inner_parsed = parse_input_type(inner);
        let element = if inner_parsed.required {
            inner_parsed.py
        } else {
            format!("{} | None", inner_parsed.py)
        };
        return ParsedInputType {
            py: format!("list[{element}]"),
            required,
        };
    }

    let py = named_scalar_py(s).map_or_else(|| s.to_string(), str::to_string);
    ParsedInputType { py, required }
}

/// Python keywords that cannot be used as parameter or class-field identifiers.
///
/// `GraphQL` names match `[A-Za-z_][A-Za-z0-9_]*`, so a name is a valid Python
/// identifier unless it collides with a keyword. (Soft keywords — `match`,
/// `case`, `type`, `_` — are legal identifiers and deliberately absent.)
const PY_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield",
];

/// Whether `name` is a (hard) Python keyword.
pub(super) fn is_py_keyword(name: &str) -> bool {
    PY_KEYWORDS.contains(&name)
}

/// A keyword-safe parameter name: `from` → `from_`, everything else verbatim.
///
/// The `GraphQL` variables dictionary is always built with the *original* name,
/// so the escape never leaks into the wire request.
pub(super) fn py_param_name(name: &str) -> String {
    if is_py_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}
