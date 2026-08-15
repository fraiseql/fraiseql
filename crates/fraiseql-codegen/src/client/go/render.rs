//! Go-specific rendering: `FieldType` → Go type expressions, input type-string
//! parsing, and identifier mapping.
//!
//! The generated client targets **Go ≥ 1.21** (generics for the Relay
//! `Connection[T]`); the shared document machinery lives in `client::common`.

use fraiseql_core::schema::FieldType;

/// Map a `GraphQL` named-scalar to its Go type, if it is a known scalar.
///
/// Returns `None` for names that are not built-in/rich scalars (object, enum,
/// input, union, interface names) — the caller passes those through verbatim.
pub(super) fn named_scalar_go(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "string" | "id" | "uuid" | "decimal" | "datetime" | "timestamp" | "date" | "time" => {
            Some("string")
        },
        "int" | "integer" => Some("int"),
        "float" | "double" => Some("float64"),
        "boolean" | "bool" => Some("bool"),
        "json" | "jsonb" => Some("any"),
        _ => None,
    }
}

/// Render a structured [`FieldType`] as a Go type expression (no nullability).
pub(super) fn field_type_go(ft: &FieldType) -> String {
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
        FieldType::Int => "int".to_string(),
        FieldType::Float => "float64".to_string(),
        FieldType::Boolean => "bool".to_string(),
        // Half precision is a storage choice, not a surface one: both are
        // `[Float!]!` in GraphQL.
        FieldType::Vector | FieldType::HalfVector => "[]float64".to_string(),
        FieldType::List(inner) => format!("[]{}", field_type_go(inner)),
        FieldType::Enum(name)
        | FieldType::Object(name)
        | FieldType::Input(name)
        | FieldType::Interface(name)
        | FieldType::Union(name) => name.clone(),
        // Reason: `Json` maps to `any`; the wildcard also covers any future
        // #[non_exhaustive] scalar variant, falling back to `any`.
        // `FieldType` is #[non_exhaustive] and lives in another crate, so an
        // exhaustive match is not available here — `every_field_type_maps_to_
        // its_own_surface` is what keeps a newly added variant from silently
        // degrading to `any` (#959's shape).
        _ => "any".to_string(),
    }
}

/// Whether a rendered Go type already carries `nil` as a value.
///
/// Slices, maps and `any` are nilable in Go; making them `*[]T` would add a
/// second, redundant level of absence that JSON round-trips but no caller wants.
/// Everything else takes a pointer to express `null`.
pub(super) fn is_nilable_go(go_type: &str) -> bool {
    go_type == "any" || go_type.starts_with("[]") || go_type.starts_with("map[")
}

/// Render a [`FieldType`] with outer nullability applied (`T` / `*T`).
pub(super) fn field_type_go_nullable(ft: &FieldType, nullable: bool) -> String {
    let base = field_type_go(ft);
    if nullable && !is_nilable_go(&base) {
        format!("*{base}")
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

/// A parsed input-field `GraphQL` type string, rendered to Go.
pub(super) struct ParsedInputType {
    /// Go type expression (e.g. `string`, `[]*string`).
    pub go:       String,
    /// Whether the outermost type is non-null (`!`) — drives `omitempty`.
    pub required: bool,
}

/// Parse an input-field `GraphQL` type **string** (`"String!"`, `"[Int]"`,
/// `"UserRole"`) into a Go type, preserving full `!`/`[]` nullability.
pub(super) fn parse_input_type(type_str: &str) -> ParsedInputType {
    let s = type_str.trim();
    let (s, required) = match s.strip_suffix('!') {
        Some(rest) => (rest.trim_end(), true),
        None => (s, false),
    };

    if let Some(inner) = s.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
        let inner_parsed = parse_input_type(inner);
        let element = if inner_parsed.required || is_nilable_go(&inner_parsed.go) {
            inner_parsed.go
        } else {
            format!("*{}", inner_parsed.go)
        };
        return ParsedInputType {
            go: format!("[]{element}"),
            required,
        };
    }

    let go = named_scalar_go(s).map_or_else(|| s.to_string(), str::to_string);
    ParsedInputType { go, required }
}

/// A `GraphQL` name as an **exported** Go identifier.
///
/// The mapping is deliberately mechanical — uppercase the first letter, keep the
/// rest verbatim — so a reader can always recover the wire name from the Go one.
/// Names that do not start with an ASCII letter (`_internal`, legal in `GraphQL`)
/// are prefixed with `X`: an identifier Go does not consider exported is invisible
/// to `encoding/json`, which would drop the field silently.
pub(super) fn go_export(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        },
        Some(_) => format!("X{name}"),
        None => "X".to_string(),
    }
}

/// A `GraphQL` name as an **unexported** Go identifier (parameters, locals).
///
/// Lowercases the first letter and escapes Go's keywords, which are all
/// lowercase and so can only ever collide at this casing.
pub(super) fn go_param_name(name: &str) -> String {
    let mut chars = name.chars();
    let lowered = match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {
            format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
        },
        Some(_) => format!("x{name}"),
        None => "x".to_string(),
    };
    if is_go_keyword(&lowered) {
        format!("{lowered}_")
    } else {
        lowered
    }
}

/// Go's 25 reserved keywords (Go spec §Keywords).
const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

/// Whether `name` is a Go keyword.
pub(super) fn is_go_keyword(name: &str) -> bool {
    GO_KEYWORDS.contains(&name)
}

/// A `GraphQL` enum value as a Go constant suffix: `IN_PROGRESS` → `InProgress`.
///
/// Constants are named `<EnumName><Suffix>`, so the enum name already namespaces
/// them; only the suffix is derived here.
pub(super) fn go_enum_suffix(value: &str) -> String {
    let mut out = String::new();
    for part in value.split('_').filter(|p| !p.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(&chars.as_str().to_ascii_lowercase());
        }
    }
    if out.is_empty() || !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        format!("X{out}")
    } else {
        out
    }
}
