//! Shared rendering helpers: `GraphQL`/`FieldType` → `TypeScript`, leaf
//! classification, input type-string parsing, and document-selection helpers.
//!
//! These are the pure, well-tested core the per-file emitters build on. See
//! `DESIGN-CLIENT-TS.md` §2–§5 for the rules implemented here.

use fraiseql_core::schema::FieldType;

/// Map a `GraphQL` named-scalar to its `TypeScript` type, if it is a known scalar.
///
/// Returns `None` for names that are not built-in/rich scalars (object, enum,
/// input, union, interface names) — the caller passes those through verbatim.
pub(super) fn named_scalar_ts(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "string" | "id" | "uuid" | "decimal" | "datetime" | "timestamp" | "date" | "time" => {
            Some("string")
        },
        "int" | "integer" | "float" | "double" => Some("number"),
        "boolean" | "bool" => Some("boolean"),
        "json" | "jsonb" => Some("unknown"),
        _ => None,
    }
}

/// Render a structured [`FieldType`] as a `TypeScript` type (no nullability).
///
/// Scalars map per the scalar table; `Enum`/`Object`/`Interface`/`Union`/`Input`
/// references render as the bare type name; lists wrap the inner type in `[]`.
pub(super) fn field_type_ts(ft: &FieldType) -> String {
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
        | FieldType::SparseVector => "string".to_string(),
        FieldType::Int | FieldType::Float => "number".to_string(),
        FieldType::Boolean => "boolean".to_string(),
        // Half precision is a storage choice, not a surface one: both are
        // `[Float!]!` in GraphQL.
        FieldType::Vector | FieldType::HalfVector => "number[]".to_string(),
        FieldType::List(inner) => format!("{}[]", field_type_ts(inner)),
        FieldType::Enum(name)
        | FieldType::Object(name)
        | FieldType::Input(name)
        | FieldType::Interface(name)
        | FieldType::Union(name) => name.clone(),
        // Reason: `Json` maps to `unknown`; the wildcard also covers any future
        // #[non_exhaustive] scalar variant, falling back to `unknown`.
        _ => "unknown".to_string(),
    }
}

/// Render a [`FieldType`] with outer nullability applied.
///
/// `T` for non-null, `T | null` for nullable. Lists are `T[]` / `T[] | null`.
/// The structured `FieldType` model carries outer nullability only, so inner-list
/// nullability is not expressible here (documented v1 simplification).
pub(super) fn field_type_ts_nullable(ft: &FieldType, nullable: bool) -> String {
    let base = field_type_ts(ft);
    if nullable {
        format!("{base} | null")
    } else {
        base
    }
}

/// Whether `ft` represents a custom/rich scalar — used to attach a `// TODO: brand`
/// note above the field.
pub(super) fn custom_scalar_name(ft: &FieldType) -> Option<&str> {
    match ft {
        FieldType::Scalar(name) => Some(name),
        FieldType::List(inner) => custom_scalar_name(inner),
        _ => None,
    }
}

/// Words that cannot name a function in a generated client.
///
/// Generated clients are ES modules, so the strict-mode and module-only
/// reservations (`await`, `yield`, `let`, `static`, `implements` …) apply
/// alongside the unconditional keywords. Contextual `TypeScript` words that are
/// legal function names — `type`, `as`, `any`, `namespace` — are deliberately
/// absent: escaping them would rename identifiers for no reason.
///
/// This list is **not** the Python one. `delete`, `new` and `function` break
/// `TypeScript` while being ordinary Python identifiers, and `from`, `del` and
/// `lambda` do the reverse — which is why #1035 is two fixes, not one.
const TS_RESERVED: &[&str] = &[
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// A reserved-word-safe `TypeScript` identifier: `delete` → `delete_`.
///
/// Only the *declaration* site needs this. The `GraphQL` document, the response
/// key (`data.delete`) and the inline result type (`{ delete: T }`) are all legal
/// property positions and keep the original name, so the escape never reaches
/// the wire — the same guarantee `py_param_name` already gives on the Python
/// side.
pub(super) fn ts_ident(name: &str) -> String {
    if TS_RESERVED.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

/// A parsed input-field `GraphQL` type string, rendered to `TypeScript`.
pub(super) struct ParsedInputType {
    /// `TypeScript` type expression (e.g. `string`, `(string | null)[]`).
    pub ts:       String,
    /// Whether the outermost type is non-null (`!`) — drives `?` on the field.
    pub required: bool,
}

/// Parse an input-field `GraphQL` type **string** (`"String!"`, `"[Int]"`,
/// `"UserRole"`) into a `TypeScript` type, preserving full `!`/`[]` nullability.
pub(super) fn parse_input_type(type_str: &str) -> ParsedInputType {
    let s = type_str.trim();
    let (s, required) = match s.strip_suffix('!') {
        Some(rest) => (rest.trim_end(), true),
        None => (s, false),
    };

    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let inner_parsed = parse_input_type(inner);
        let element = if inner_parsed.required {
            inner_parsed.ts
        } else {
            format!("({} | null)", inner_parsed.ts)
        };
        return ParsedInputType {
            ts: format!("{element}[]"),
            required,
        };
    }

    let ts = named_scalar_ts(s).map_or_else(|| s.to_string(), str::to_string);
    ParsedInputType { ts, required }
}

#[cfg(test)]
mod tests;
