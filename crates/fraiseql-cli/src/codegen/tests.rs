#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod proto_gen_tests {
    use fraiseql_core::schema::{
        CompiledSchema, EnumDefinition, EnumValueDefinition, FieldDenyPolicy, FieldType,
        TypeDefinition,
    };

    use super::super::proto_gen::*;

    fn make_field(
        name: &str,
        ft: FieldType,
        nullable: bool,
    ) -> fraiseql_core::schema::FieldDefinition {
        fraiseql_core::schema::FieldDefinition {
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
            hierarchy: None,
        }
    }

    fn make_type(
        name: &str,
        fields: Vec<fraiseql_core::schema::FieldDefinition>,
    ) -> TypeDefinition {
        TypeDefinition {
            name: name.into(),
            sql_source: String::new().into(),
            jsonb_column: "data".to_string(),
            fields,
            description: None,
            sql_projection_hint: None,
            implements: vec![],
            requires_role: None,
            is_error: false,
            relay: false,
            relationships: Vec::new(),
        }
    }

    /// Build a query via JSON deserialization to leverage `#[serde(default)]`.
    fn make_query(
        name: &str,
        return_type: &str,
        returns_list: bool,
    ) -> fraiseql_core::schema::QueryDefinition {
        let json = serde_json::json!({
            "name": name,
            "return_type": return_type,
            "returns_list": returns_list,
        });
        serde_json::from_value(json).expect("test query definition")
    }

    /// Build a mutation via JSON deserialization.
    fn make_mutation(
        name: &str,
        args: Vec<fraiseql_core::schema::ArgumentDefinition>,
    ) -> fraiseql_core::schema::MutationDefinition {
        let mut m: fraiseql_core::schema::MutationDefinition =
            serde_json::from_value(serde_json::json!({
                "name": name,
                "return_type": "MutationResponse",
            }))
            .expect("test mutation definition");
        m.arguments = args;
        m
    }

    fn make_arg(
        name: &str,
        ft: FieldType,
        nullable: bool,
    ) -> fraiseql_core::schema::ArgumentDefinition {
        fraiseql_core::schema::ArgumentDefinition {
            name: name.to_string(),
            arg_type: ft,
            nullable,
            default_value: None,
            description: None,
            deprecation: None,
        }
    }

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
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("String"), RowViewColumnType::Text);
    }

    #[test]
    fn test_row_view_type_int() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("Int"), RowViewColumnType::Int32);
    }

    #[test]
    fn test_row_view_type_bigint() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("BigInt"), RowViewColumnType::Int64);
    }

    #[test]
    fn test_row_view_type_float() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("Float"), RowViewColumnType::Float64);
    }

    #[test]
    fn test_row_view_type_boolean() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("Boolean"), RowViewColumnType::Boolean);
    }

    #[test]
    fn test_row_view_type_id() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("ID"), RowViewColumnType::Uuid);
    }

    #[test]
    fn test_row_view_type_datetime() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("DateTime"), RowViewColumnType::Timestamptz);
    }

    #[test]
    fn test_row_view_type_json() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("JSON"), RowViewColumnType::Json);
    }

    #[test]
    fn test_row_view_type_date() {
        use fraiseql_core::db::dialect::RowViewColumnType;
        assert_eq!(graphql_to_row_view_type("Date"), RowViewColumnType::Date);
    }

    #[test]
    fn test_row_view_type_custom_scalar_fallback() {
        use fraiseql_core::db::dialect::RowViewColumnType;
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

    // ── to_pascal_case ──────────────────────────────────────────────────

    #[test]
    fn test_pascal_case_snake() {
        assert_eq!(to_pascal_case("get_user"), "GetUser");
    }

    #[test]
    fn test_pascal_case_single() {
        assert_eq!(to_pascal_case("users"), "Users");
    }

    #[test]
    fn test_pascal_case_already() {
        assert_eq!(to_pascal_case("User"), "User");
    }

    // ── to_screaming_snake ──────────────────────────────────────────────

    #[test]
    fn test_screaming_snake() {
        assert_eq!(to_screaming_snake("OrderStatus"), "ORDER_STATUS");
    }

    // ── should_include_type ─────────────────────────────────────────────

    #[test]
    fn test_include_all_when_empty() {
        assert!(should_include_type("User", &[], &[]));
    }

    #[test]
    fn test_include_whitelist() {
        assert!(should_include_type("User", &["User".to_string()], &[]));
        assert!(!should_include_type("Post", &["User".to_string()], &[]));
    }

    #[test]
    fn test_exclude_blacklist() {
        assert!(!should_include_type("Secret", &[], &["Secret".to_string()]));
        assert!(should_include_type("User", &[], &["Secret".to_string()]));
    }

    // ── generate_proto_file ─────────────────────────────────────────────

    #[test]
    fn test_generate_proto_basic_type() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("name", FieldType::String, false),
                make_field("email", FieldType::String, true),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));
        schema.queries.push(make_query("list_users", "User", true));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("syntax = \"proto3\";"));
        assert!(proto.contains("package fraiseql.v1;"));
        assert!(proto.contains("message User {"));
        // Fields sorted alphabetically: email=1, id=2, name=3
        assert!(proto.contains("optional string email = 1;"));
        assert!(proto.contains("string id = 2;"));
        assert!(proto.contains("string name = 3;"));
        // Service
        assert!(proto.contains("service FraiseqlService {"));
        assert!(proto.contains("rpc GetUser(GetUserRequest) returns (User);"));
        assert!(proto.contains("rpc ListUsers(ListUsersRequest) returns (stream User);"));
    }

    #[test]
    fn test_generate_proto_with_datetime_import() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "Post",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("created_at", FieldType::DateTime, false),
            ],
        ));
        schema.queries.push(make_query("get_post", "Post", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("import \"google/protobuf/timestamp.proto\";"));
        assert!(proto.contains("google.protobuf.Timestamp created_at = 1;"));
    }

    #[test]
    fn test_generate_proto_with_mutations() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema.mutations.push(make_mutation(
            "create_user",
            vec![
                make_arg("name", FieldType::String, false),
                make_arg("email", FieldType::String, false),
            ],
        ));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("message MutationResponse {"));
        assert!(proto.contains("message CreateUserRequest {"));
        // Args sorted: email=1, name=2
        assert!(proto.contains("string email = 1;"));
        assert!(proto.contains("string name = 2;"));
        assert!(proto.contains("rpc CreateUser(CreateUserRequest) returns (MutationResponse);"));
    }

    #[test]
    fn test_generate_proto_with_enum() {
        let mut schema = CompiledSchema::new();
        schema.enums.push(EnumDefinition {
            name: "OrderStatus".to_string(),
            values: vec![
                EnumValueDefinition {
                    name: "PENDING".to_string(),
                    description: None,
                    deprecation: None,
                },
                EnumValueDefinition {
                    name: "SHIPPED".to_string(),
                    description: None,
                    deprecation: None,
                },
            ],
            description: None,
        });
        schema.types.push(make_type(
            "Order",
            vec![
                make_field("id", FieldType::Id, false),
                make_field("status", FieldType::Enum("OrderStatus".to_string()), false),
            ],
        ));
        schema.queries.push(make_query("get_order", "Order", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("enum OrderStatus {"));
        assert!(proto.contains("ORDER_STATUS_UNSPECIFIED = 0;"));
        assert!(proto.contains("PENDING = 1;"));
        assert!(proto.contains("SHIPPED = 2;"));
        assert!(proto.contains("OrderStatus status = 2;"));
    }

    #[test]
    fn test_generate_proto_exclude_types() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema
            .types
            .push(make_type("Secret", vec![make_field("id", FieldType::Id, false)]));
        schema.queries.push(make_query("get_user", "User", false));
        schema.queries.push(make_query("get_secret", "Secret", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &["Secret".to_string()]);

        assert!(proto.contains("message User {"));
        assert!(!proto.contains("message Secret {"));
        assert!(proto.contains("rpc GetUser"));
        assert!(!proto.contains("rpc GetSecret"));
    }

    #[test]
    fn test_generate_proto_list_query_pagination() {
        let mut schema = CompiledSchema::new();
        schema
            .types
            .push(make_type("User", vec![make_field("id", FieldType::Id, false)]));
        schema.queries.push(make_query("list_users", "User", true));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        // Pagination fields added to list request
        assert!(proto.contains("optional int32 limit = 1;"));
        assert!(proto.contains("optional int32 offset = 2;"));
        // Server-streaming: no ListUsersResponse wrapper, returns stream User
        assert!(proto.contains("rpc ListUsers(ListUsersRequest) returns (stream User);"));
        assert!(!proto.contains("ListUsersResponse"), "No response wrapper for streaming RPCs");
    }

    #[test]
    fn test_generate_proto_nullable_field() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![
                make_field("name", FieldType::String, false),
                make_field("bio", FieldType::String, true),
            ],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("optional string bio = 1;"));
        assert!(proto.contains("string name = 2;"));
    }

    #[test]
    fn test_generate_proto_list_field() {
        let mut schema = CompiledSchema::new();
        schema.types.push(make_type(
            "User",
            vec![make_field(
                "tags",
                FieldType::List(Box::new(FieldType::String)),
                false,
            )],
        ));
        schema.queries.push(make_query("get_user", "User", false));

        let proto = generate_proto_file(&schema, "fraiseql.v1", &[], &[]);

        assert!(proto.contains("repeated string tags = 1;"));
    }
}

mod row_views_tests {
    use fraiseql_core::{
        db::dialect::{MySqlDialect, PostgresDialect, SqlServerDialect, SqliteDialect},
        schema::{FieldDefinition, FieldDenyPolicy, FieldType, TypeDefinition},
    };

    use super::super::row_views::*;

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
            hierarchy: None,
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
