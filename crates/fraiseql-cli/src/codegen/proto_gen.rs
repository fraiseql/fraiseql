//! GraphQL → Protobuf type mapping for `.proto` file generation.

use fraiseql_core::db::dialect::RowViewColumnType;

/// Map a GraphQL type name to a Protobuf type name.
///
/// Returns the protobuf scalar or well-known type for the given GraphQL type.
/// Unknown types fall back to `"string"`.
///
/// # Examples
///
/// ```
/// use fraiseql_cli::codegen::proto_gen::graphql_to_proto_type;
///
/// assert_eq!(graphql_to_proto_type("String"), "string");
/// assert_eq!(graphql_to_proto_type("Int"), "int32");
/// assert_eq!(graphql_to_proto_type("DateTime"), "google.protobuf.Timestamp");
/// ```
#[must_use]
pub fn graphql_to_proto_type(graphql_type: &str) -> &'static str {
    match graphql_type {
        "String" => "string",
        "Int" => "int32",
        "Float" => "double",
        "Boolean" => "bool",
        "ID" => "string",
        "DateTime" => "google.protobuf.Timestamp",
        "Date" => "string",
        "BigInt" => "int64",
        "JSON" => "google.protobuf.Struct",
        _ => "string", // Custom scalars fall back to string
    }
}

/// Map a GraphQL type name to a [`RowViewColumnType`] for SQL view generation.
///
/// Used by the row-shaped view DDL generator to determine the target SQL type
/// for each field extracted from the JSON column.
///
/// # Examples
///
/// ```
/// use fraiseql_cli::codegen::proto_gen::graphql_to_row_view_type;
/// use fraiseql_core::db::dialect::RowViewColumnType;
///
/// assert_eq!(graphql_to_row_view_type("String"), RowViewColumnType::Text);
/// assert_eq!(graphql_to_row_view_type("Int"), RowViewColumnType::Int32);
/// ```
#[must_use]
pub fn graphql_to_row_view_type(graphql_type: &str) -> RowViewColumnType {
    match graphql_type {
        "String" | "Date" => RowViewColumnType::Text,
        "Int" => RowViewColumnType::Int32,
        "BigInt" => RowViewColumnType::Int64,
        "Float" => RowViewColumnType::Float64,
        "Boolean" => RowViewColumnType::Boolean,
        "ID" => RowViewColumnType::Uuid,
        "DateTime" => RowViewColumnType::Timestamptz,
        "JSON" => RowViewColumnType::Json,
        _ => RowViewColumnType::Text, // Custom scalars → text
    }
}

/// Returns `true` if the given protobuf type requires an import of a
/// well-known type `.proto` file.
#[must_use]
pub fn needs_well_known_import(proto_type: &str) -> bool {
    matches!(
        proto_type,
        "google.protobuf.Timestamp" | "google.protobuf.Struct"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── graphql_to_proto_type ───────────────────────────────────────────

    #[test]
    fn test_proto_type_string() {
        assert_eq!(graphql_to_proto_type("String"), "string");
    }

    #[test]
    fn test_proto_type_int() {
        assert_eq!(graphql_to_proto_type("Int"), "int32");
    }

    #[test]
    fn test_proto_type_float() {
        assert_eq!(graphql_to_proto_type("Float"), "double");
    }

    #[test]
    fn test_proto_type_boolean() {
        assert_eq!(graphql_to_proto_type("Boolean"), "bool");
    }

    #[test]
    fn test_proto_type_id() {
        assert_eq!(graphql_to_proto_type("ID"), "string");
    }

    #[test]
    fn test_proto_type_datetime() {
        assert_eq!(graphql_to_proto_type("DateTime"), "google.protobuf.Timestamp");
    }

    #[test]
    fn test_proto_type_date() {
        assert_eq!(graphql_to_proto_type("Date"), "string");
    }

    #[test]
    fn test_proto_type_bigint() {
        assert_eq!(graphql_to_proto_type("BigInt"), "int64");
    }

    #[test]
    fn test_proto_type_json() {
        assert_eq!(graphql_to_proto_type("JSON"), "google.protobuf.Struct");
    }

    #[test]
    fn test_proto_type_custom_scalar_fallback() {
        assert_eq!(graphql_to_proto_type("Email"), "string");
        assert_eq!(graphql_to_proto_type("PhoneNumber"), "string");
    }

    // ── graphql_to_row_view_type ────────────────────────────────────────

    #[test]
    fn test_row_view_type_string() {
        assert_eq!(graphql_to_row_view_type("String"), RowViewColumnType::Text);
    }

    #[test]
    fn test_row_view_type_int() {
        assert_eq!(graphql_to_row_view_type("Int"), RowViewColumnType::Int32);
    }

    #[test]
    fn test_row_view_type_bigint() {
        assert_eq!(graphql_to_row_view_type("BigInt"), RowViewColumnType::Int64);
    }

    #[test]
    fn test_row_view_type_float() {
        assert_eq!(graphql_to_row_view_type("Float"), RowViewColumnType::Float64);
    }

    #[test]
    fn test_row_view_type_boolean() {
        assert_eq!(graphql_to_row_view_type("Boolean"), RowViewColumnType::Boolean);
    }

    #[test]
    fn test_row_view_type_id() {
        assert_eq!(graphql_to_row_view_type("ID"), RowViewColumnType::Uuid);
    }

    #[test]
    fn test_row_view_type_datetime() {
        assert_eq!(graphql_to_row_view_type("DateTime"), RowViewColumnType::Timestamptz);
    }

    #[test]
    fn test_row_view_type_json() {
        assert_eq!(graphql_to_row_view_type("JSON"), RowViewColumnType::Json);
    }

    #[test]
    fn test_row_view_type_date_is_text() {
        // Date is stored as ISO 8601 text string
        assert_eq!(graphql_to_row_view_type("Date"), RowViewColumnType::Text);
    }

    #[test]
    fn test_row_view_type_custom_scalar_fallback() {
        assert_eq!(graphql_to_row_view_type("Email"), RowViewColumnType::Text);
    }

    // ── needs_well_known_import ─────────────────────────────────────────

    #[test]
    fn test_needs_import_timestamp() {
        assert!(needs_well_known_import("google.protobuf.Timestamp"));
    }

    #[test]
    fn test_needs_import_struct() {
        assert!(needs_well_known_import("google.protobuf.Struct"));
    }

    #[test]
    fn test_no_import_for_scalars() {
        assert!(!needs_well_known_import("string"));
        assert!(!needs_well_known_import("int32"));
        assert!(!needs_well_known_import("bool"));
    }
}
