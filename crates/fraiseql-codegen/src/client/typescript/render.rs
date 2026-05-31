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
        | FieldType::Scalar(_) => "string".to_string(),
        FieldType::Int | FieldType::Float => "number".to_string(),
        FieldType::Boolean => "boolean".to_string(),
        FieldType::Vector => "number[]".to_string(),
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

/// Whether a field is a `GraphQL` **leaf** (selectable without a sub-selection).
///
/// Scalars, enums, and lists-of-leaf are leaves and are fetched by the default
/// document; object/interface/union references (and lists thereof) are composite
/// and are omitted in v1 (see `DESIGN-CLIENT-TS.md` §2).
pub(super) fn is_leaf(ft: &FieldType) -> bool {
    match ft {
        FieldType::Object(_) | FieldType::Interface(_) | FieldType::Union(_) => false,
        FieldType::List(inner) => is_leaf(inner),
        _ => true,
    }
}

/// Render a [`FieldType`] argument as a `GraphQL` type reference for a variable
/// declaration, e.g. `ID!`, `UserFilter`, `[String]`.
pub(super) fn arg_graphql_type(ft: &FieldType, nullable: bool) -> String {
    let base = ft.to_graphql_string();
    if nullable { base } else { format!("{base}!") }
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
mod tests {
    use super::*;

    #[test]
    fn scalars_map_to_ts() {
        assert_eq!(field_type_ts(&FieldType::String), "string");
        assert_eq!(field_type_ts(&FieldType::Int), "number");
        assert_eq!(field_type_ts(&FieldType::Boolean), "boolean");
        assert_eq!(field_type_ts(&FieldType::Id), "string");
        assert_eq!(field_type_ts(&FieldType::Json), "unknown");
        assert_eq!(field_type_ts(&FieldType::DateTime), "string");
        assert_eq!(field_type_ts(&FieldType::Vector), "number[]");
    }

    #[test]
    fn references_pass_through_as_names() {
        assert_eq!(field_type_ts(&FieldType::Object("User".into())), "User");
        assert_eq!(field_type_ts(&FieldType::Enum("Role".into())), "Role");
        assert_eq!(
            field_type_ts(&FieldType::List(Box::new(FieldType::Object("Post".into())))),
            "Post[]"
        );
    }

    #[test]
    fn nullability_wraps() {
        assert_eq!(field_type_ts_nullable(&FieldType::String, false), "string");
        assert_eq!(field_type_ts_nullable(&FieldType::String, true), "string | null");
    }

    #[test]
    fn leaf_classification() {
        assert!(is_leaf(&FieldType::String));
        assert!(is_leaf(&FieldType::Enum("Role".into())));
        assert!(is_leaf(&FieldType::List(Box::new(FieldType::String))));
        assert!(!is_leaf(&FieldType::Object("User".into())));
        assert!(!is_leaf(&FieldType::List(Box::new(FieldType::Object("Post".into())))));
    }

    #[test]
    fn input_string_parsing_preserves_nullability() {
        let required = parse_input_type("String!");
        assert_eq!(required.ts, "string");
        assert!(required.required);

        let optional = parse_input_type("String");
        assert_eq!(optional.ts, "string");
        assert!(!optional.required);

        assert_eq!(parse_input_type("[String!]!").ts, "string[]");
        assert_eq!(parse_input_type("[String]!").ts, "(string | null)[]");
        assert!(parse_input_type("[String!]!").required);
        assert!(!parse_input_type("[String!]").required);
        assert_eq!(parse_input_type("UserRole").ts, "UserRole");
    }

    #[test]
    fn graphql_arg_types() {
        assert_eq!(arg_graphql_type(&FieldType::Id, false), "ID!");
        assert_eq!(arg_graphql_type(&FieldType::Input("UserFilter".into()), true), "UserFilter");
    }
}
