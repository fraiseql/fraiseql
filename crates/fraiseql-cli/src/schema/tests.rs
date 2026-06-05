#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod database_validator_tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        db::{
            DatabaseType,
            introspector::{DatabaseIntrospector, RelationInfo},
        },
        schema::{
            AutoParams, CompiledSchema, CursorType, FieldDefinition, FieldType, MutationDefinition,
            QueryDefinition, TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    use super::super::database_validator::*;

    /// Mock introspector for unit tests.
    struct MockIntrospector {
        relations:    Vec<RelationInfo>,
        columns:      HashMap<String, Vec<(String, String, bool)>>,
        json_samples: HashMap<(String, String), Vec<serde_json::Value>>,
        db_type:      DatabaseType,
    }

    impl MockIntrospector {
        fn new(db_type: DatabaseType) -> Self {
            Self {
                relations: Vec::new(),
                columns: HashMap::new(),
                json_samples: HashMap::new(),
                db_type,
            }
        }

        fn with_relation(
            mut self,
            schema: &str,
            name: &str,
            kind: fraiseql_core::db::RelationKind,
        ) -> Self {
            self.relations.push(RelationInfo {
                schema: schema.to_string(),
                name: name.to_string(),
                kind,
            });
            self
        }

        fn with_columns(mut self, table: &str, cols: Vec<(&str, &str, bool)>) -> Self {
            self.columns.insert(
                table.to_string(),
                cols.into_iter()
                    .map(|(n, t, nullable)| (n.to_string(), t.to_string(), nullable))
                    .collect(),
            );
            self
        }

        fn with_json_samples(
            mut self,
            table: &str,
            column: &str,
            samples: Vec<serde_json::Value>,
        ) -> Self {
            self.json_samples.insert((table.to_string(), column.to_string()), samples);
            self
        }
    }

    impl DatabaseIntrospector for MockIntrospector {
        async fn list_fact_tables(&self) -> fraiseql_core::Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn get_columns(
            &self,
            table_name: &str,
        ) -> fraiseql_core::Result<Vec<(String, String, bool)>> {
            Ok(self.columns.get(table_name).cloned().unwrap_or_default())
        }

        async fn get_indexed_columns(
            &self,
            _table_name: &str,
        ) -> fraiseql_core::Result<Vec<String>> {
            Ok(Vec::new())
        }

        fn database_type(&self) -> DatabaseType {
            self.db_type
        }

        async fn list_relations(&self) -> fraiseql_core::Result<Vec<RelationInfo>> {
            Ok(self.relations.clone())
        }

        async fn get_sample_json_rows(
            &self,
            table_name: &str,
            column_name: &str,
            _limit: usize,
        ) -> fraiseql_core::Result<Vec<serde_json::Value>> {
            Ok(self
                .json_samples
                .get(&(table_name.to_string(), column_name.to_string()))
                .cloned()
                .unwrap_or_default())
        }
    }

    fn make_query(name: &str, return_type: &str, sql_source: &str) -> QueryDefinition {
        QueryDefinition {
            name:                name.to_string(),
            return_type:         return_type.to_string(),
            returns_list:        true,
            nullable:            false,
            arguments:           vec![],
            sql_source:          Some(sql_source.to_string()),
            description:         None,
            auto_params:         AutoParams::default(),
            deprecation:         None,
            jsonb_column:        "data".to_string(),
            relay:               false,
            relay_cursor_column: None,
            relay_cursor_type:   CursorType::default(),
            inject_params:       IndexMap::default(),
            cache_ttl_seconds:   None,
            additional_views:    vec![],
            requires_role:       None,
            rest_path:           None,
            rest_method:         None,
            native_columns:      HashMap::new(),
        }
    }

    fn make_type(name: &str, fields: Vec<(&str, FieldType)>) -> TypeDefinition {
        TypeDefinition {
            name:                name.into(),
            fields:              fields
                .into_iter()
                .map(|(n, ft)| FieldDefinition::new(n, ft))
                .collect(),
            description:         None,
            sql_source:          "".into(),
            jsonb_column:        "data".to_string(),
            sql_projection_hint: None,
            implements:          vec![],
            requires_role:       None,
            is_error:            false,
            relay:               false,
            relationships:       Vec::new(),
        }
    }

    fn make_schema(types: Vec<TypeDefinition>, queries: Vec<QueryDefinition>) -> CompiledSchema {
        CompiledSchema {
            types,
            queries,
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_valid_schema_no_warnings() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false), ("pk_user", "bigint", false)])
            .with_json_samples(
                "v_user",
                "data",
                vec![serde_json::json!({"name": "Alice", "email": "alice@example.com"})],
            );

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![("name", FieldType::String), ("email", FieldType::String)],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(
            report.warnings.is_empty(),
            "Expected no warnings, got: {:?}",
            report.warnings.len()
        );
    }

    #[tokio::test]
    async fn test_missing_relation() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);
        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "v_user")
        );
    }

    #[tokio::test]
    async fn test_missing_additional_view() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let mut query = make_query("users", "User", "v_user");
        query.additional_views = vec!["v_missing".to_string()];

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingAdditionalView { view_name, .. } if view_name == "v_missing")
        );
    }

    #[tokio::test]
    async fn test_missing_jsonb_column() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("pk_user", "bigint", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingJsonColumn { column_name, .. } if column_name == "data")
        );
    }

    #[tokio::test]
    async fn test_wrong_json_column_type() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "text", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::WrongJsonColumnType { actual_type, .. } if actual_type == "text")
        );
    }

    #[tokio::test]
    async fn test_sqlserver_nvarchar_no_warning() {
        let introspector = MockIntrospector::new(DatabaseType::SQLServer)
            .with_relation("dbo", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "nvarchar", false)]);

        let schema = make_schema(vec![], vec![make_query("users", "User", "v_user")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // SQL Server: nvarchar is always accepted for JSON columns
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::WrongJsonColumnType { .. }))
        );
    }

    #[tokio::test]
    async fn test_missing_cursor_column() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let mut query = make_query("users", "User", "v_user");
        query.relay = true;
        query.relay_cursor_column = Some("pk_user".to_string());

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingCursorColumn { column_name, .. } if column_name == "pk_user")));
    }

    #[tokio::test]
    async fn test_missing_json_key() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)])
            .with_json_samples("v_user", "data", vec![serde_json::json!({"name": "Alice"})]);

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![("name", FieldType::String), ("email", FieldType::String)],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.iter().any(|w| matches!(w, DatabaseWarning::MissingJsonKey { field_name, .. } if field_name == "email")));
    }

    #[tokio::test]
    async fn test_empty_json_sample_no_l3_warnings() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)]);

        let schema = make_schema(
            vec![make_type("User", vec![("name", FieldType::String)])],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // No L3 warnings because no sample data
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingJsonKey { .. }))
        );
    }

    #[tokio::test]
    async fn test_schema_qualified_match() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("etl_log", "v_foo", fraiseql_core::db::RelationKind::View)
            .with_columns("v_foo", vec![("data", "jsonb", false)]);

        let schema = make_schema(vec![], vec![make_query("foos", "Foo", "etl_log.v_foo")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // Should match
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingRelation { .. }))
        );
    }

    #[tokio::test]
    async fn test_schema_qualified_wrong_schema() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL).with_relation(
            "public",
            "v_foo",
            fraiseql_core::db::RelationKind::View,
        );

        let schema = make_schema(vec![], vec![make_query("foos", "Foo", "etl_log.v_foo")]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "etl_log.v_foo")
        );
    }

    #[tokio::test]
    async fn test_mutation_missing_sql_source() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);

        let mut schema = make_schema(vec![], vec![]);
        schema.mutations.push(MutationDefinition {
            name: "createUser".to_string(),
            sql_source: Some("fn_create_user".to_string()),
            ..MutationDefinition::new("createUser", "User")
        });

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert_eq!(report.warnings.len(), 1);
        assert!(
            matches!(&report.warnings[0], DatabaseWarning::MissingRelation { sql_source, .. } if sql_source == "fn_create_user")
        );
    }

    #[tokio::test]
    async fn test_query_no_sql_source_skipped() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL);

        let mut query = make_query("users", "User", "v_user");
        query.sql_source = None;

        let schema = make_schema(vec![], vec![query]);

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        assert!(report.warnings.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_samples_merge_keys() {
        let introspector = MockIntrospector::new(DatabaseType::PostgreSQL)
            .with_relation("public", "v_user", fraiseql_core::db::RelationKind::View)
            .with_columns("v_user", vec![("data", "jsonb", false)])
            .with_json_samples(
                "v_user",
                "data",
                vec![
                    serde_json::json!({"name": "Alice", "email": "alice@example.com"}),
                    serde_json::json!({"email": "bob@example.com", "age": 30}),
                ],
            );

        let schema = make_schema(
            vec![make_type(
                "User",
                vec![
                    ("name", FieldType::String),
                    ("email", FieldType::String),
                    ("age", FieldType::Int),
                ],
            )],
            vec![make_query("users", "User", "v_user")],
        );

        let report = validate_schema_against_database(&schema, &introspector).await.unwrap();
        // All keys present across both samples
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| matches!(w, DatabaseWarning::MissingJsonKey { .. }))
        );
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("firstName"), "first_name");
        assert_eq!(to_snake_case("name"), "name");
        assert_eq!(to_snake_case("HTMLParser"), "h_t_m_l_parser");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_is_json_type_postgres() {
        assert!(is_json_type("jsonb", DatabaseType::PostgreSQL));
        assert!(is_json_type("json", DatabaseType::PostgreSQL));
        assert!(!is_json_type("text", DatabaseType::PostgreSQL));
    }

    #[test]
    fn test_is_json_type_mysql() {
        assert!(is_json_type("json", DatabaseType::MySQL));
        assert!(!is_json_type("varchar", DatabaseType::MySQL));
    }

    #[test]
    fn test_is_json_type_sqlite() {
        assert!(is_json_type("json", DatabaseType::SQLite));
        assert!(is_json_type("JSON", DatabaseType::SQLite));
        assert!(!is_json_type("text", DatabaseType::SQLite));
    }

    #[test]
    fn test_is_json_type_sqlserver() {
        // SQL Server always returns true
        assert!(is_json_type("nvarchar", DatabaseType::SQLServer));
        assert!(is_json_type("varchar", DatabaseType::SQLServer));
    }

    #[test]
    fn test_display_warnings() {
        let warning = DatabaseWarning::MissingRelation {
            query_name: "users".to_string(),
            sql_source: "v_user".to_string(),
        };
        assert_eq!(
            warning.to_string(),
            "query `users`: sql_source `v_user` does not exist in database"
        );
    }
}

mod lookup_data_tests {
    use super::super::lookup_data::*;

    #[test]
    fn test_build_lookup_data() {
        let data = build_lookup_data();

        assert!(data.get("countries").is_some());
        assert!(data.get("currencies").is_some());
        assert!(data.get("timezones").is_some());
        assert!(data.get("languages").is_some());
    }

    #[test]
    fn test_countries_have_required_fields() {
        let countries = build_countries_lookup();

        for (code, data) in countries {
            assert!(data.get("name").is_some(), "Country {code} missing name");
            assert!(data.get("continent").is_some(), "Country {code} missing continent");
            assert!(data.get("in_eu").is_some(), "Country {code} missing in_eu");
            assert!(data.get("in_schengen").is_some(), "Country {code} missing in_schengen");
        }
    }

    #[test]
    fn test_currencies_have_required_fields() {
        let currencies = build_currencies_lookup();

        for (code, data) in currencies {
            assert!(data.get("name").is_some(), "Currency {code} missing name");
            assert!(data.get("symbol").is_some(), "Currency {code} missing symbol");
            assert!(data.get("decimal_places").is_some(), "Currency {code} missing decimal_places");
        }
    }

    #[test]
    fn test_timezones_have_required_fields() {
        let timezones = build_timezones_lookup();

        for (code, data) in timezones {
            assert!(data.get("offset_minutes").is_some(), "Timezone {code} missing offset_minutes");
            assert!(data.get("has_dst").is_some(), "Timezone {code} missing has_dst");
        }
    }

    #[test]
    fn test_eu_member_states() {
        let countries = build_countries_lookup();

        // Check some known EU members
        assert!(countries["FR"]["in_eu"].as_bool().unwrap());
        assert!(countries["DE"]["in_eu"].as_bool().unwrap());
        assert!(countries["IT"]["in_eu"].as_bool().unwrap());

        // Check non-EU
        assert!(!countries["US"]["in_eu"].as_bool().unwrap());
        assert!(!countries["GB"]["in_eu"].as_bool().unwrap());
    }

    #[test]
    fn test_schengen_members() {
        let countries = build_countries_lookup();

        // Check some known Schengen members
        assert!(countries["FR"]["in_schengen"].as_bool().unwrap());
        assert!(countries["DE"]["in_schengen"].as_bool().unwrap());
        assert!(countries["CH"]["in_schengen"].as_bool().unwrap());

        // Check non-Schengen
        assert!(!countries["US"]["in_schengen"].as_bool().unwrap());
        assert!(!countries["GB"]["in_schengen"].as_bool().unwrap());
    }
}

mod merger_tests {
    use std::fs;

    use tempfile::TempDir;

    use super::super::merger::*;

    #[test]
    fn test_merge_toml_only() {
        let toml_content = r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[types.User.fields.name]
type = "String"

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
"#;

        // Write temp file
        let tmp = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        std::fs::write(tmp.path(), toml_content).unwrap();

        // Merge
        let result = SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap());
        result.unwrap_or_else(|e| panic!("expected Ok from merge_toml_only: {e}"));
    }

    #[test]
    fn test_merge_with_includes() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create schema files
        let user_types = serde_json::json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        fs::write(temp_dir.path().join("user.json"), user_types.to_string())?;

        let post_types = serde_json::json!({
            "types": [{"name": "Post", "fields": []}],
            "queries": [],
            "mutations": []
        });
        fs::write(temp_dir.path().join("post.json"), post_types.to_string())?;

        // Create TOML with includes
        let toml_content = format!(
            r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[includes]
types = ["{}/*.json"]
queries = []
mutations = []
"#,
            temp_dir.path().to_string_lossy()
        );

        let toml_path = temp_dir.path().join("fraiseql.toml");
        fs::write(&toml_path, toml_content)?;

        // Merge
        let result = SchemaMerger::merge_with_includes(toml_path.to_str().unwrap());
        let schema = result.unwrap_or_else(|e| panic!("expected Ok from merge_with_includes: {e}"));
        assert_eq!(schema.types.len(), 2);

        Ok(())
    }

    #[test]
    fn test_merge_with_includes_missing_files() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let toml_content = r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[includes]
types = ["/nonexistent/path/*.json"]
queries = []
mutations = []
"#;

        let toml_path = temp_dir.path().join("fraiseql.toml");
        fs::write(&toml_path, toml_content)?;

        // Should succeed but with no files loaded (glob matches nothing)
        let result = SchemaMerger::merge_with_includes(toml_path.to_str().unwrap());
        let schema = result.unwrap_or_else(|e| {
            panic!("expected Ok from merge_with_includes (missing files): {e}")
        });
        assert_eq!(schema.types.len(), 0);

        Ok(())
    }

    #[test]
    fn test_merge_from_domains() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let schema_dir = temp_dir.path().join("schema");
        fs::create_dir(&schema_dir)?;

        // Create domain structure
        fs::create_dir(schema_dir.join("auth"))?;
        fs::create_dir(schema_dir.join("products"))?;

        let auth_types = serde_json::json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        fs::write(schema_dir.join("auth/types.json"), auth_types.to_string())?;

        let product_types = serde_json::json!({
            "types": [{"name": "Product", "fields": []}],
            "queries": [{"name": "getProduct", "return_type": "Product"}],
            "mutations": []
        });
        fs::write(schema_dir.join("products/types.json"), product_types.to_string())?;

        // Create TOML with domain discovery (use absolute path)
        let schema_dir_str = schema_dir.to_string_lossy().to_string();
        let toml_content = format!(
            r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[domain_discovery]
enabled = true
root_dir = "{schema_dir_str}"
"#
        );

        let toml_path = temp_dir.path().join("fraiseql.toml");
        fs::write(&toml_path, toml_content)?;

        // Merge
        let schema = SchemaMerger::merge_from_domains(toml_path.to_str().unwrap())
            .unwrap_or_else(|e| panic!("expected Ok from merge_from_domains: {e}"));

        // Should have 2 types (from both domains)
        assert_eq!(schema.types.len(), 2);
        // Should have 2 queries (from both domains)
        assert_eq!(schema.queries.len(), 2);

        Ok(())
    }

    #[test]
    fn test_merge_from_domains_alphabetical_order() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let schema_dir = temp_dir.path().join("schema");
        fs::create_dir(&schema_dir)?;

        // Create domains in non-alphabetical order
        fs::create_dir(schema_dir.join("zebra"))?;
        fs::create_dir(schema_dir.join("alpha"))?;
        fs::create_dir(schema_dir.join("middle"))?;

        for domain in &["zebra", "alpha", "middle"] {
            let types = serde_json::json!({
                "types": [{"name": domain.to_uppercase(), "fields": []}],
                "queries": [],
                "mutations": []
            });
            fs::write(schema_dir.join(format!("{domain}/types.json")), types.to_string())?;
        }

        let schema_dir_str = schema_dir.to_string_lossy().to_string();
        let toml_content = format!(
            r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[domain_discovery]
enabled = true
root_dir = "{schema_dir_str}"
"#
        );

        let toml_path = temp_dir.path().join("fraiseql.toml");
        fs::write(&toml_path, toml_content)?;

        let schema = SchemaMerger::merge_from_domains(toml_path.to_str().unwrap())
            .unwrap_or_else(|e| panic!("expected Ok from merge_from_domains (alphabetical): {e}"));

        // Types should be loaded in alphabetical order: ALPHA, MIDDLE, ZEBRA
        let type_names: Vec<String> = schema.types.iter().map(|t| t.name.clone()).collect();

        assert_eq!(type_names[0], "ALPHA");
        assert_eq!(type_names[1], "MIDDLE");
        assert_eq!(type_names[2], "ZEBRA");

        Ok(())
    }

    #[test]
    fn test_merge_toml_only_with_validation_config() {
        let toml_content = r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[validation]
max_query_depth = 3
max_query_complexity = 25
max_page_size = 750
"#;

        let tmp = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        std::fs::write(tmp.path(), toml_content).unwrap();

        let schema = SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap())
            .unwrap_or_else(|e| panic!("expected Ok from merge_toml_only (with validation): {e}"));

        // validation_config should be populated
        let vc = schema.validation_config.as_ref().expect("validation_config should be set");
        assert_eq!(vc.max_query_depth, Some(3));
        assert_eq!(vc.max_query_complexity, Some(25));
        // #421: the page-size ceiling flows TOML → compiled schema.
        assert_eq!(vc.max_page_size, Some(750));
    }

    #[test]
    fn test_merge_toml_only_without_validation_config() {
        let toml_content = r#"
[schema]
name = "test"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "postgresql://localhost/test"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"
"#;

        let tmp = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        std::fs::write(tmp.path(), toml_content).unwrap();

        let schema = SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap())
            .unwrap_or_else(|e| panic!("expected Ok from merge_toml_only (no validation): {e}"));

        // validation_config should be None when no [validation] section
        assert!(schema.validation_config.is_none());
    }

    // ── CRUD naming config ─────────────────────────────────────────────────────

    #[test]
    fn pascal_to_snake_single_word() {
        assert_eq!(pascal_to_snake("User"), "user");
    }

    #[test]
    fn pascal_to_snake_compound_type() {
        assert_eq!(pascal_to_snake("UserProfile"), "user_profile");
    }

    #[test]
    fn pascal_to_snake_already_lower() {
        assert_eq!(pascal_to_snake("user"), "user");
    }

    #[test]
    fn pascal_to_snake_three_words() {
        assert_eq!(pascal_to_snake("DnsServerConfig"), "dns_server_config");
    }

    fn write_temp_toml(content: &str) -> tempfile::NamedTempFile {
        let tmp = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        std::fs::write(tmp.path(), content).unwrap();
        tmp
    }

    #[test]
    fn crud_trinity_resolves_create_mutation() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[crud]
function_schema = "app"
function_naming = "trinity"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[mutations.create_user]
return_type = "User"
operation = "CREATE"
"#;
        let tmp = write_temp_toml(toml);
        let schema = SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap())
            .expect("should merge with crud naming");
        let mutation = schema.mutations.iter().find(|m| m.name == "create_user").unwrap();
        assert_eq!(mutation.sql_source.as_deref(), Some("app.create_user"));
    }

    #[test]
    fn crud_trinity_resolves_pascal_return_type() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[crud]
function_naming = "trinity"

[types.UserProfile]
sql_source = "v_user_profile"

[types.UserProfile.fields.id]
type = "ID"

[mutations.create_user_profile]
return_type = "UserProfile"
operation = "CREATE"
"#;
        let tmp = write_temp_toml(toml);
        let schema =
            SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap()).expect("should merge");
        let mutation = schema.mutations.iter().find(|m| m.name == "create_user_profile").unwrap();
        assert_eq!(mutation.sql_source.as_deref(), Some("create_user_profile"));
    }

    #[test]
    fn explicit_sql_source_wins_over_crud() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[crud]
function_schema = "app"
function_naming = "trinity"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[mutations.create_user]
return_type = "User"
operation = "CREATE"
sql_source = "custom_create_user_fn"
"#;
        let tmp = write_temp_toml(toml);
        let schema =
            SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap()).expect("should merge");
        let mutation = schema.mutations.iter().find(|m| m.name == "create_user").unwrap();
        assert_eq!(mutation.sql_source.as_deref(), Some("custom_create_user_fn"));
    }

    #[test]
    fn no_sql_source_no_crud_errors_with_mutation_name() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[mutations.create_user]
return_type = "User"
operation = "CREATE"
"#;
        let tmp = write_temp_toml(toml);
        let err = SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap())
            .expect_err("should fail without sql_source and no crud config");
        let msg = format!("{err}");
        assert!(msg.contains("create_user"), "error should name the mutation, got: {msg}");
        assert!(msg.contains("sql_source") || msg.contains("crud"), "got: {msg}");
    }

    #[test]
    fn crud_custom_template_resolved_in_merger() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[crud]
function_schema = "app"
create_template = "insert_{entity}"

[types.Order]
sql_source = "v_order"

[types.Order.fields.id]
type = "ID"

[mutations.create_order]
return_type = "Order"
operation = "CREATE"
"#;
        let tmp = write_temp_toml(toml);
        let schema =
            SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap()).expect("should merge");
        let mutation = schema.mutations.iter().find(|m| m.name == "create_order").unwrap();
        assert_eq!(mutation.sql_source.as_deref(), Some("app.insert_order"));
    }

    #[test]
    fn crud_update_and_delete_resolved() {
        let toml = r#"
[schema]
name = "test"
version = "1.0.0"

[crud]
function_schema = "app"
function_naming = "trinity"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"

[mutations.update_user]
return_type = "User"
operation = "UPDATE"

[mutations.delete_user]
return_type = "User"
operation = "DELETE"
"#;
        let tmp = write_temp_toml(toml);
        let schema =
            SchemaMerger::merge_toml_only(tmp.path().to_str().unwrap()).expect("should merge");
        let update = schema.mutations.iter().find(|m| m.name == "update_user").unwrap();
        let delete = schema.mutations.iter().find(|m| m.name == "delete_user").unwrap();
        assert_eq!(update.sql_source.as_deref(), Some("app.update_user"));
        assert_eq!(delete.sql_source.as_deref(), Some("app.delete_user"));
    }
}

mod multi_file_loader_tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use super::super::multi_file_loader::*;

    fn create_test_file(dir: &std::path::Path, name: &str, content: &str) -> anyhow::Result<()> {
        let path = dir.join(name);
        fs::write(path, content)?;
        Ok(())
    }

    #[test]
    fn test_load_single_type_file() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "types.json", &schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 1);
        assert_eq!(result["types"][0]["name"], "User");
        assert_eq!(result["queries"].as_array().unwrap().len(), 0);
        assert_eq!(result["mutations"].as_array().unwrap().len(), 0);

        Ok(())
    }

    #[test]
    fn test_merge_multiple_type_files() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let user_schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "user.json", &user_schema.to_string())?;

        let post_schema = json!({
            "types": [
                {"name": "Post", "fields": []}
            ],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "post.json", &post_schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);
        let type_names: Vec<&str> = result["types"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();
        assert!(type_names.contains(&"User"));
        assert!(type_names.contains(&"Post"));

        Ok(())
    }

    #[test]
    fn test_merge_respects_alphabetical_order() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let c_schema = json!({
            "types": [{"name": "C", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "c.json", &c_schema.to_string())?;

        let a_schema = json!({
            "types": [{"name": "A", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "a.json", &a_schema.to_string())?;

        let b_schema = json!({
            "types": [{"name": "B", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "b.json", &b_schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        let type_names: Vec<&str> = result["types"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t["name"].as_str())
            .collect();

        // Should be ordered by file load order (a.json, b.json, c.json alphabetically)
        assert_eq!(type_names[0], "A");
        assert_eq!(type_names[1], "B");
        assert_eq!(type_names[2], "C");

        Ok(())
    }

    #[test]
    fn test_merge_queries_and_mutations() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let schema = json!({
            "types": [
                {"name": "User", "fields": []}
            ],
            "queries": [
                {"name": "getUser", "return_type": "User"}
            ],
            "mutations": [
                {"name": "createUser", "return_type": "User"}
            ]
        });
        create_test_file(temp_dir.path(), "schema.json", &schema.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 1);
        assert_eq!(result["queries"].as_array().unwrap().len(), 1);
        assert_eq!(result["queries"][0]["name"], "getUser");
        assert_eq!(result["mutations"].as_array().unwrap().len(), 1);
        assert_eq!(result["mutations"][0]["name"], "createUser");

        Ok(())
    }

    #[test]
    fn test_nested_directory_structure() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        // Create nested structure
        fs::create_dir_all(temp_dir.path().join("types"))?;
        fs::create_dir_all(temp_dir.path().join("queries"))?;

        let user_type = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("types").as_path(),
            "user.json",
            &user_type.to_string(),
        )?;

        let post_type = json!({
            "types": [{"name": "Post", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("types").as_path(),
            "post.json",
            &post_type.to_string(),
        )?;

        let user_queries = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(
            temp_dir.path().join("queries").as_path(),
            "user_queries.json",
            &user_queries.to_string(),
        )?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);
        assert_eq!(result["queries"].as_array().unwrap().len(), 1);

        Ok(())
    }

    #[test]
    fn test_duplicate_type_names_error() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let file1 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file1.json", &file1.to_string())?;

        let file2 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file2.json", &file2.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(result.is_err(), "expected Err, got: {result:?}");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Duplicate type 'User'"));
        assert!(err_msg.contains("file1.json"));
        assert!(err_msg.contains("file2.json"));

        Ok(())
    }

    #[test]
    fn test_duplicate_query_names_error() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let file1 = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file1.json", &file1.to_string())?;

        let file2 = json!({
            "types": [],
            "queries": [{"name": "getUser", "return_type": "User"}],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "file2.json", &file2.to_string())?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap());

        assert!(result.is_err(), "expected Err, got: {result:?}");
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Duplicate query 'getUser'"));

        Ok(())
    }

    #[test]
    fn test_empty_directory() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let result = MultiFileLoader::load_from_directory(temp_dir.path().to_str().unwrap())?;

        assert_eq!(result["types"].as_array().unwrap().len(), 0);
        assert_eq!(result["queries"].as_array().unwrap().len(), 0);
        assert_eq!(result["mutations"].as_array().unwrap().len(), 0);

        Ok(())
    }

    #[test]
    fn test_nonexistent_directory() {
        let result = MultiFileLoader::load_from_directory("/nonexistent/path/to/schema");
        assert!(result.is_err(), "expected Err for nonexistent directory, got: {result:?}");
    }

    #[test]
    fn test_load_from_paths() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;

        let schema1 = json!({
            "types": [{"name": "User", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "schema1.json", &schema1.to_string())?;

        let schema2 = json!({
            "types": [{"name": "Post", "fields": []}],
            "queries": [],
            "mutations": []
        });
        create_test_file(temp_dir.path(), "schema2.json", &schema2.to_string())?;

        let paths = vec![
            temp_dir.path().join("schema1.json"),
            temp_dir.path().join("schema2.json"),
        ];

        let result = MultiFileLoader::load_from_paths(&paths)?;

        assert_eq!(result["types"].as_array().unwrap().len(), 2);

        Ok(())
    }

    #[test]
    fn test_directory_file_count_limit_exceeded() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;

        // Create MAX_SCHEMA_FILES + 1 JSON files — should trip the limit.
        let schema = json!({"types": [], "queries": [], "mutations": []});
        let content = schema.to_string();
        for i in 0..=MAX_SCHEMA_FILES {
            create_test_file(temp_dir.path(), &format!("schema_{i:04}.json"), &content)?;
        }

        let result =
            MultiFileLoader::load_from_directory_with_tracking(temp_dir.path().to_str().unwrap());
        assert!(result.is_err(), "expected error when file count exceeds limit");
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("more than"), "error should mention the limit: {msg}");
        Ok(())
    }
}

mod optimizer_tests {
    use std::collections::HashMap;

    use fraiseql_core::{
        db::types::DatabaseType,
        schema::{
            ArgumentDefinition, AutoParams, CompiledSchema, CursorType, FieldDefinition,
            FieldDenyPolicy, FieldType, QueryDefinition, TypeDefinition,
        },
        validation::CustomTypeRegistry,
    };
    use indexmap::IndexMap;

    use super::super::optimizer::*;

    #[test]
    fn test_optimize_empty_schema() {
        let mut schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert_eq!(report.total_hints(), 0);
    }

    #[test]
    fn test_index_hint_for_list_query() {
        let mut schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![QueryDefinition {
                name:                "users".to_string(),
                return_type:         "User".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![ArgumentDefinition {
                    name:          "status".to_string(),
                    arg_type:      FieldType::String,
                    nullable:      false,
                    default_value: None,
                    description:   None,
                    deprecation:   None,
                }],
                sql_source:          Some("users".to_string()),
                description:         None,
                auto_params:         AutoParams::default(),
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
            }],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report.total_hints() > 0);
        assert!(!report.index_hints.is_empty());
        assert_eq!(report.index_hints[0].query_name, "users");
    }

    #[test]
    fn test_pagination_note() {
        let mut schema = CompiledSchema {
            types: vec![],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![QueryDefinition {
                name:                "products".to_string(),
                return_type:         "Product".to_string(),
                returns_list:        true,
                nullable:            false,
                arguments:           vec![],
                sql_source:          Some("products".to_string()),
                description:         None,
                auto_params:         AutoParams {
                    has_where:    false,
                    has_order_by: false,
                    has_limit:    true,
                    has_offset:   true,
                },
                deprecation:         None,
                jsonb_column:        "data".to_string(),
                relay:               false,
                relay_cursor_column: None,
                relay_cursor_type:   CursorType::default(),
                inject_params:       IndexMap::default(),
                cache_ttl_seconds:   None,
                additional_views:    vec![],
                requires_role:       None,
                rest_path:           None,
                rest_method:         None,
                native_columns:      HashMap::new(),
            }],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report.optimization_notes.iter().any(|note| note.contains("pagination")));
    }

    #[test]
    fn test_large_type_warning() {
        let mut schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "BigType".into(),
                sql_source:          String::new().into(),
                jsonb_column:        String::new(),
                fields:              (0..25)
                    .map(|i| FieldDefinition {
                        name:           format!("field{i}").into(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        encryption:     None,
                        hierarchy:      None,
                    })
                    .collect(),
                description:         None,
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       Vec::new(),
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();
        assert!(report.optimization_notes.iter().any(|note| note.contains("25 fields")));
    }

    #[test]
    fn test_projection_hint_for_large_type() {
        let mut schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "User".into(),
                sql_source:          "users".into(),
                jsonb_column:        "data".to_string(),
                fields:              (0..15)
                    .map(|i| FieldDefinition {
                        name:           format!("field{i}").into(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        encryption:     None,
                        hierarchy:      None,
                    })
                    .collect(),
                description:         None,
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       Vec::new(),
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();

        // Type with 15 fields and JSONB column should get projection hint
        assert!(!report.projection_hints.is_empty());
        assert_eq!(report.projection_hints[0].type_name, "User");
        assert_eq!(report.projection_hints[0].field_count, 15);

        // Type should have sql_projection_hint set
        assert!(schema.types[0].has_sql_projection());
        let hint = schema.types[0].sql_projection_hint.as_ref().unwrap();
        assert_eq!(hint.database, DatabaseType::PostgreSQL);
        assert!(hint.estimated_reduction_percent > 0);
    }

    #[test]
    fn test_projection_not_applied_without_jsonb() {
        let mut schema = CompiledSchema {
            types: vec![TypeDefinition {
                name:                "SmallType".into(),
                sql_source:          "small_table".into(),
                jsonb_column:        String::new(), // No JSONB column
                fields:              (0..15)
                    .map(|i| FieldDefinition {
                        name:           format!("field{i}").into(),
                        field_type:     FieldType::String,
                        nullable:       false,
                        default_value:  None,
                        description:    None,
                        vector_config:  None,
                        alias:          None,
                        deprecation:    None,
                        requires_scope: None,
                        on_deny:        FieldDenyPolicy::default(),
                        encryption:     None,
                        hierarchy:      None,
                    })
                    .collect(),
                description:         None,
                sql_projection_hint: None,
                implements:          vec![],
                requires_role:       None,
                is_error:            false,
                relay:               false,
                relationships:       Vec::new(),
            }],
            enums: vec![],
            input_types: vec![],
            interfaces: vec![],
            unions: vec![],
            queries: vec![],
            mutations: vec![],
            subscriptions: vec![],
            directives: vec![],
            observers: Vec::new(),
            fact_tables: HashMap::default(),
            federation: None,
            security: None,
            observers_config: None,
            subscriptions_config: None,
            validation_config: None,
            debug_config: None,
            mcp_config: None,
            schema_sdl: None,
            schema_format_version: None,
            custom_scalars: CustomTypeRegistry::default(),
            ..Default::default()
        };

        let report = SchemaOptimizer::optimize(&mut schema).unwrap();

        // Type without JSONB column should not get projection hint
        assert!(report.projection_hints.is_empty());
        assert!(!schema.types[0].has_sql_projection());
    }
}

mod rich_filters_tests {
    use super::super::rich_filters::*;

    #[test]
    fn test_rich_types_list() {
        let types = get_all_rich_types();
        assert!(types.contains(&"EmailAddress".to_string()));
        assert!(types.contains(&"VIN".to_string()));
        assert!(types.contains(&"IBAN".to_string()));
    }

    #[test]
    fn test_generate_where_input_name() {
        let where_input_name = "EmailAddressWhereInput";
        assert!(where_input_name.ends_with("WhereInput"));
    }
}

mod sql_templates_tests {
    use super::super::sql_templates::*;

    #[test]
    fn test_extract_operator_templates() {
        let templates = extract_operator_templates("domainEq");

        // Should have templates for all 4 databases
        assert_eq!(templates.len(), 4);
        assert!(templates.contains_key("postgres"));
        assert!(templates.contains_key("mysql"));
        assert!(templates.contains_key("sqlite"));
        assert!(templates.contains_key("sqlserver"));

        // Verify templates are correct
        assert!(templates["postgres"].contains("SPLIT_PART"));
        assert!(templates["mysql"].contains("SUBSTRING_INDEX"));
    }

    #[test]
    fn test_build_sql_templates_metadata() {
        let operators = vec!["domainEq", "wmiEq"];
        let metadata = build_sql_templates_metadata(&operators);

        assert!(metadata.get("operators").is_some());
        let ops = metadata["operators"].as_object().unwrap();
        assert_eq!(ops.len(), 2);
        assert!(ops.contains_key("domainEq"));
        assert!(ops.contains_key("wmiEq"));
    }

    #[test]
    fn test_extract_vin_templates() {
        let templates = extract_operator_templates("wmiEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("SUBSTRING"));
        assert!(templates["mysql"].contains("SUBSTRING"));
    }

    #[test]
    fn test_geospatial_templates() {
        let templates = extract_operator_templates("distanceWithin");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("ST_DWithin"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("ST_Distance_Sphere"));

        assert!(templates.contains_key("sqlite"));
        assert!(templates["sqlite"].contains("Haversine") || templates["sqlite"].contains("ACOS"));

        assert!(templates.contains_key("sqlserver"));
        assert!(templates["sqlserver"].contains("geography"));
    }

    #[test]
    fn test_phone_templates() {
        let templates = extract_operator_templates("phoneCountryCodeEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates.contains_key("mysql"));
        assert!(templates.contains_key("sqlite"));
        assert!(templates.contains_key("sqlserver"));
    }

    #[test]
    fn test_date_range_templates() {
        let templates = extract_operator_templates("durationGte");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("EXTRACT"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("DATEDIFF"));
    }

    #[test]
    fn test_duration_templates() {
        let templates = extract_operator_templates("totalSecondsEq");

        assert!(templates.contains_key("postgres"));
        assert!(templates["postgres"].contains("EPOCH"));

        assert!(templates.contains_key("mysql"));
        assert!(templates["mysql"].contains("REPLACE"));
    }
}
