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
        .filter(|f| f.field_type.is_scalar())
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

#[cfg(test)]
mod tests {
    use fraiseql_core::{
        db::dialect::{MySqlDialect, PostgresDialect, SqlServerDialect, SqliteDialect},
        schema::{FieldDefinition, FieldDenyPolicy, FieldType},
    };

    use super::*;

    fn make_user_type() -> TypeDefinition {
        TypeDefinition {
            name: "user".into(),
            sql_source: "user".into(),
            jsonb_column: "data".to_string(),
            fields: vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
                make_field("email", FieldType::String, true),
                make_field("created_at", FieldType::DateTime, false),
            ],
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: Vec::new(),
        }
    }

    fn make_field(name: &str, ft: FieldType, nullable: bool) -> FieldDefinition {
        FieldDefinition {
            name: name.into(),
            field_type: ft,
            nullable,
            description: None,
            default_value: None,
            vector_config: None,
            alias: None,
            deprecation: None,
            requires_scope: None,
            on_deny: FieldDenyPolicy::default(),
            encryption: None,
        }
    }

    // ── PostgreSQL ──────────────────────────────────────────────────────

    #[test]
    fn test_postgres_row_view_ddl() {
        let td = make_user_type();
        let ddl = generate_row_view_sql(&PostgresDialect, &td);

        assert!(ddl.contains("CREATE OR REPLACE VIEW \"vr_user\""));
        assert!(ddl.contains("FROM \"tb_user\""));
        assert!(ddl.contains("(data->>'id')::uuid AS \"id\""));
        assert!(ddl.contains("(data->>'name')::text AS \"name\""));
        assert!(ddl.contains("(data->>'email')::text AS \"email\""));
        assert!(ddl.contains("(data->>'created_at')::timestamptz AS \"created_at\""));
    }

    // ── MySQL ───────────────────────────────────────────────────────────

    #[test]
    fn test_mysql_row_view_ddl() {
        let td = make_user_type();
        let ddl = generate_row_view_sql(&MySqlDialect, &td);

        assert!(ddl.contains("CREATE OR REPLACE VIEW `vr_user`"));
        assert!(ddl.contains("FROM `tb_user`"));
        assert!(ddl.contains("CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.id')) AS CHAR) AS `id`"));
        assert!(ddl.contains("CAST(JSON_UNQUOTE(JSON_EXTRACT(data, '$.name')) AS CHAR) AS `name`"));
    }

    // ── SQLite ──────────────────────────────────────────────────────────

    #[test]
    fn test_sqlite_row_view_ddl() {
        let td = make_user_type();
        let ddl = generate_row_view_sql(&SqliteDialect, &td);

        assert!(ddl.contains("DROP VIEW IF EXISTS \"vr_user\""));
        assert!(ddl.contains("CREATE VIEW \"vr_user\""));
        assert!(ddl.contains("FROM \"tb_user\""));
        assert!(ddl.contains("CAST(json_extract(data, '$.id') AS TEXT) AS \"id\""));
        assert!(ddl.contains("CAST(json_extract(data, '$.name') AS TEXT) AS \"name\""));
    }

    // ── SQL Server ──────────────────────────────────────────────────────

    #[test]
    fn test_sqlserver_row_view_ddl() {
        let td = make_user_type();
        let ddl = generate_row_view_sql(&SqlServerDialect, &td);

        assert!(ddl.contains("CREATE OR ALTER VIEW [vr_user]"));
        assert!(ddl.contains("FROM [tb_user]"));
        assert!(ddl.contains("CAST(JSON_VALUE(data, '$.id') AS UNIQUEIDENTIFIER) AS [id]"));
        assert!(ddl.contains("CAST(JSON_VALUE(data, '$.name') AS NVARCHAR(MAX)) AS [name]"));
    }

    // ── Scalar filter ───────────────────────────────────────────────────

    #[test]
    fn test_non_scalar_fields_excluded() {
        let td = TypeDefinition {
            name: "post".into(),
            sql_source: "post".into(),
            jsonb_column: "data".to_string(),
            fields: vec![
                make_field("id", FieldType::Id, false),
                make_field("title", FieldType::String, false),
                // Object reference — should be excluded from vr_* view
                make_field("author", FieldType::Object("User".to_string()), false),
                // List — should be excluded
                make_field("tags", FieldType::List(Box::new(FieldType::String)), false),
            ],
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: Vec::new(),
        };

        let ddl = generate_row_view_sql(&PostgresDialect, &td);

        // Scalar fields included
        assert!(ddl.contains("\"id\""));
        assert!(ddl.contains("\"title\""));
        // Non-scalar fields excluded
        assert!(!ddl.contains("\"author\""));
        assert!(!ddl.contains("\"tags\""));
    }

    // ── Custom jsonb_column ─────────────────────────────────────────────

    #[test]
    fn test_custom_jsonb_column() {
        let mut td = make_user_type();
        td.jsonb_column = "payload".to_string();

        let ddl = generate_row_view_sql(&PostgresDialect, &td);

        assert!(ddl.contains("(payload->>'id')::uuid"));
        assert!(!ddl.contains("data"));
    }

    // ── generate_all_row_views ──────────────────────────────────────────

    #[test]
    fn test_generate_all_with_exclude() {
        let types = vec![
            make_user_type(),
            TypeDefinition {
                name: "secret".into(),
                sql_source: "secret".into(),
                jsonb_column: "data".to_string(),
                fields: vec![make_field("id", FieldType::Id, false)],
                description: None,
                sql_projection_hint: None,
                implements: vec![],
                requires_role: None,
                is_error: false,
                relay: false,
                relationships: Vec::new(),
            },
        ];

        let ddl = generate_all_row_views(&PostgresDialect, &types, &[], &["secret".to_string()]);

        assert!(ddl.contains("vr_user"));
        assert!(!ddl.contains("vr_secret"));
    }

    // ── Source table is tb_*, not v_* ────────────────────────────────────

    #[test]
    fn test_source_table_is_command_side() {
        let td = make_user_type();
        let ddl = generate_row_view_sql(&PostgresDialect, &td);

        // Must reference tb_user (command-side), not v_user (JSON-shaped view)
        assert!(ddl.contains("tb_user"));
        assert!(!ddl.contains("v_user"));
    }
}
