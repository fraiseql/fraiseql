//! Row-shaped SQL view (`vr_*`) DDL generation for the gRPC transport.
//!
//! Generates `CREATE VIEW vr_<entity>` statements that extract individual
//! scalar fields from the JSONB `data` column into typed SQL columns.
//! These views are CQRS read projections optimized for protobuf wire encoding —
//! the database returns native typed columns instead of JSON.

use fraiseql_core::{db::dialect::SqlDialect, schema::TypeDefinition};

use super::proto_gen::graphql_to_row_view_type;

/// Generate the DDL for a row-shaped view from a type definition.
///
/// The view selects from the command-side table (`tb_<sql_source>`) and
/// extracts each scalar field as a typed column using the dialect's
/// `row_view_column_expr()` method.
///
/// # Arguments
///
/// * `dialect` — SQL dialect for type casting and DDL syntax.
/// * `type_def` — The GraphQL type definition containing field metadata.
///
/// # Returns
///
/// A complete DDL string (e.g., `CREATE OR REPLACE VIEW "vr_user" AS ...`).
pub fn generate_row_view_sql(dialect: &dyn SqlDialect, type_def: &TypeDefinition) -> String {
    let source_table = format!("tb_{}", type_def.sql_source);
    let view_name = format!("vr_{}", type_def.sql_source);

    let columns: Vec<(String, String)> = type_def
        .fields
        .iter()
        // Each field name is interpolated into the JSONB extraction path
        // (`data->>'{name}'`) by the dialect, so a name containing a single quote
        // would break out of that literal. A compiled schema should only carry
        // valid GraphQL identifiers; drop anything else defensively.
        .filter(|f| f.field_type.is_scalar() && is_safe_field_name(f.name.as_ref()))
        .map(|f| {
            let col_type = graphql_to_row_view_type(&f.field_type.to_graphql_string());
            let expr =
                dialect.row_view_column_expr(&type_def.jsonb_column, f.name.as_ref(), &col_type);
            (f.name.to_string(), expr)
        })
        .collect();

    dialect.create_row_view_ddl(&view_name, &source_table, &columns)
}

/// Generate DDL for all types in a compiled schema.
///
/// Returns one DDL statement per type, separated by blank lines.
/// Non-scalar-only types (those with no scalar fields) are skipped.
///
/// # Arguments
///
/// * `dialect` — SQL dialect for type casting and DDL syntax.
/// * `types` — Slice of type definitions to generate views for.
/// * `include_types` — Whitelist of type names (empty = all).
/// * `exclude_types` — Blacklist of type names.
pub fn generate_all_row_views(
    dialect: &dyn SqlDialect,
    types: &[TypeDefinition],
    include_types: &[String],
    exclude_types: &[String],
) -> String {
    let mut ddl_parts = Vec::new();

    for td in types {
        let name: &str = td.name.as_ref();
        if !include_types.is_empty() && !include_types.iter().any(|t| t == name) {
            continue;
        }
        if exclude_types.iter().any(|t| t == name) {
            continue;
        }

        let has_scalars = td.fields.iter().any(|f| f.field_type.is_scalar());
        if !has_scalars {
            continue;
        }

        ddl_parts.push(generate_row_view_sql(dialect, td));
    }

    ddl_parts.join("\n\n")
}

/// A field name is safe to interpolate into a row-view JSONB extraction path only
/// if it is a plain SQL identifier (`[A-Za-z0-9_]`, non-empty, ≤128 chars).
fn is_safe_field_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod field_name_tests {
    use super::is_safe_field_name;

    #[test]
    fn is_safe_field_name_accepts_identifiers_rejects_injection() {
        assert!(is_safe_field_name("email"));
        assert!(is_safe_field_name("created_at"));
        assert!(is_safe_field_name("_private"));

        assert!(!is_safe_field_name(""));
        assert!(!is_safe_field_name("evil'; DROP VIEW vr_user; --"));
        assert!(!is_safe_field_name("a'b"));
        assert!(!is_safe_field_name("with space"));
        assert!(!is_safe_field_name("dotted.path"));
    }
}
