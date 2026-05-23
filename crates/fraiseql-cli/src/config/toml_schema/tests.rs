#![allow(clippy::unwrap_used, clippy::panic)] // Reason: test code, panics acceptable
#![allow(clippy::wildcard_imports)] // Reason: test modules use wildcard imports for conciseness

mod toml_schema_tests {
    use super::super::*;

    #[test]
    fn test_parse_toml_schema() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.User]
sql_source = "v_user"

[types.User.fields.id]
type = "ID"
nullable = false

[types.User.fields.name]
type = "String"
nullable = false

[queries.users]
return_type = "User"
return_array = true
sql_source = "v_user"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.schema.name, "myapp");
        assert!(schema.types.contains_key("User"));
    }

    #[test]
    fn test_validate_schema() {
        let schema = TomlSchema::default();
        schema.validate().unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
    }

    // --- Issue #38: nats_url ---

    #[test]
    fn test_observers_config_nats_url_round_trip() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[observers]
enabled = true
backend = "nats"
nats_url = "nats://localhost:4222"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.observers.backend, "nats");
        assert_eq!(schema.observers.nats_url.as_deref(), Some("nats://localhost:4222"));
        assert!(schema.observers.redis_url.is_none());
    }

    #[test]
    fn test_observers_config_redis_url_unchanged() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[observers]
enabled = true
backend = "redis"
redis_url = "redis://localhost:6379"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.observers.backend, "redis");
        assert_eq!(schema.observers.redis_url.as_deref(), Some("redis://localhost:6379"));
        assert!(schema.observers.nats_url.is_none());
    }

    #[test]
    fn test_observers_config_nats_url_default_is_none() {
        let config = ObserversConfig::default();
        assert!(config.nats_url.is_none());
    }

    // --- Issue #39: federation circuit breaker ---

    #[test]
    fn test_federation_circuit_breaker_round_trip() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true
apollo_version = 2

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 3
recovery_timeout_secs = 60
success_threshold = 1
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let cb = schema.federation.circuit_breaker.as_ref().expect("Expected circuit_breaker");
        assert!(cb.enabled);
        assert_eq!(cb.failure_threshold, 3);
        assert_eq!(cb.recovery_timeout_secs, 60);
        assert_eq!(cb.success_threshold, 1);
        assert!(cb.per_database.is_empty());
    }

    #[test]
    fn test_federation_circuit_breaker_zero_failure_threshold_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[federation]
enabled = true

[federation.circuit_breaker]
enabled = true
failure_threshold = 0
recovery_timeout_secs = 30
success_threshold = 2
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("failure_threshold"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_zero_recovery_timeout_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[federation]
enabled = true

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 0
success_threshold = 2
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("recovery_timeout_secs"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_per_database_unknown_entity_rejected() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 30
success_threshold = 2

[[federation.circuit_breaker.per_database]]
database = "NonExistentEntity"
failure_threshold = 3
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        let err = schema.validate().unwrap_err();
        assert!(err.to_string().contains("NonExistentEntity"), "{err}");
    }

    #[test]
    fn test_federation_circuit_breaker_per_database_valid() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Product]
sql_source = "v_product"

[federation]
enabled = true

[[federation.entities]]
name = "Product"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 30
success_threshold = 2

[[federation.circuit_breaker.per_database]]
database = "Product"
failure_threshold = 3
recovery_timeout_secs = 15
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        schema.validate().unwrap_or_else(|e| panic!("expected Ok from validate: {e:?}"));
        let cb = schema.federation.circuit_breaker.as_ref().unwrap();
        assert_eq!(cb.per_database.len(), 1);
        assert_eq!(cb.per_database[0].database, "Product");
        assert_eq!(cb.per_database[0].failure_threshold, Some(3));
        assert_eq!(cb.per_database[0].recovery_timeout_secs, Some(15));
    }

    #[test]
    fn test_toml_schema_parses_server_section() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[server]
host = "127.0.0.1"
port = 9999

[server.cors]
origins = ["https://example.com"]
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.server.host, "127.0.0.1");
        assert_eq!(schema.server.port, 9999);
        assert_eq!(schema.server.cors.origins, ["https://example.com"]);
    }

    #[test]
    fn test_toml_schema_database_uses_runtime_config() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[database]
url      = "postgresql://localhost/mydb"
pool_min = 5
pool_max = 30
ssl_mode = "require"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        assert_eq!(schema.database.url, Some("postgresql://localhost/mydb".to_string()));
        assert_eq!(schema.database.pool_min, 5);
        assert_eq!(schema.database.pool_max, 30);
        assert_eq!(schema.database.ssl_mode, "require");
    }

    #[test]
    fn test_env_var_expansion_in_toml_schema() {
        temp_env::with_var("SCHEMA_TEST_DB_URL", Some("postgres://test/fraiseql"), || {
            let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[database]
url = "${SCHEMA_TEST_DB_URL}"
"#;
            let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
            assert_eq!(schema.database.url, Some("postgres://test/fraiseql".to_string()));
        });
    }

    #[test]
    fn test_toml_schema_defaults_without_server_section() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"
"#;
        let schema = TomlSchema::parse_toml(toml).expect("Failed to parse");
        // Defaults should apply
        assert_eq!(schema.server.host, "0.0.0.0");
        assert_eq!(schema.server.port, 8080);
        assert_eq!(schema.database.pool_min, 2);
        assert_eq!(schema.database.pool_max, 20);
        assert!(schema.database.url.is_none());
    }

    #[test]
    fn test_rate_limiting_config_parses_per_user_rps() {
        let toml = r"
[security.rate_limiting]
enabled = true
requests_per_second = 100
requests_per_second_per_user = 250
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let rl = schema.security.rate_limiting.unwrap();
        assert_eq!(rl.requests_per_second_per_user, Some(250));
    }

    #[test]
    fn test_rate_limiting_config_per_user_rps_defaults_to_none() {
        let toml = r"
[security.rate_limiting]
enabled = true
requests_per_second = 50
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let rl = schema.security.rate_limiting.unwrap();
        assert_eq!(rl.requests_per_second_per_user, None);
    }

    #[test]
    fn test_validation_config_parses_limits() {
        let toml = r"
[validation]
max_query_depth = 5
max_query_complexity = 50
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, Some(5));
        assert_eq!(schema.validation.max_query_complexity, Some(50));
    }

    #[test]
    fn test_validation_config_defaults_to_none() {
        let toml = "";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, None);
        assert_eq!(schema.validation.max_query_complexity, None);
    }

    #[test]
    fn test_validation_config_partial() {
        let toml = r"
[validation]
max_query_depth = 3
";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert_eq!(schema.validation.max_query_depth, Some(3));
        assert_eq!(schema.validation.max_query_complexity, None);
    }

    // --- Issue #250: hierarchy config ---

    #[test]
    fn test_hierarchy_config_deserializes_from_toml() {
        let toml = r#"
[hierarchies.category]
table = "tb_category"
path_column = "category_path"

[hierarchies.location]
table = "tb_location"
path_column = "location_path"
"#;
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let hierarchies = schema.hierarchies.as_ref().expect("hierarchies should be Some");
        assert_eq!(hierarchies.len(), 2);

        let cat = &hierarchies["category"];
        assert_eq!(cat.table, "tb_category");
        assert_eq!(cat.path_column, "category_path");

        let loc = &hierarchies["location"];
        assert_eq!(loc.table, "tb_location");
        assert_eq!(loc.path_column, "location_path");
    }

    #[test]
    fn test_hierarchy_config_absent_defaults_to_none() {
        let toml = "";
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        assert!(schema.hierarchies.is_none());
    }

    #[test]
    fn test_hierarchy_config_rejects_empty_table() {
        let toml = r#"
[hierarchies.bad]
table = ""
path_column = "some_path"
"#;
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let hierarchies = schema.hierarchies.as_ref().unwrap();
        let bad = &hierarchies["bad"];
        assert!(bad.validate().is_err());
    }

    #[test]
    fn test_hierarchy_field_reference_validated() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[hierarchies.category]
table = "tb_category"
path_column = "category_path"

[types.Category]
sql_source = "v_category"

[types.Category.fields.id]
type = "ID"

[types.Category.fields.category_path]
type = "String"
hierarchy = "category"

[queries.categories]
return_type = "Category"
return_array = true
sql_source = "v_category"
"#;
        let schema = TomlSchema::parse_toml(toml).unwrap();
        schema.validate().unwrap();
    }

    #[test]
    fn test_hierarchy_field_reference_rejects_invalid_name() {
        let toml = r#"
[schema]
name = "myapp"
version = "1.0.0"
database_target = "postgresql"

[types.Category]
sql_source = "v_category"

[types.Category.fields.id]
type = "ID"

[types.Category.fields.category_path]
type = "String"
hierarchy = "nonexistent"

[queries.categories]
return_type = "Category"
return_array = true
sql_source = "v_category"
"#;
        let schema = TomlSchema::parse_toml(toml).unwrap();
        let err = schema.validate().unwrap_err();
        assert!(
            err.to_string().contains("nonexistent"),
            "Error should mention the invalid hierarchy name: {err}"
        );
    }

    #[test]
    fn test_hierarchy_config_rejects_empty_path_column() {
        let toml = r#"
[hierarchies.bad]
table = "tb_something"
path_column = ""
"#;
        let schema: TomlSchema = toml::from_str(toml).unwrap();
        let hierarchies = schema.hierarchies.as_ref().unwrap();
        let bad = &hierarchies["bad"];
        assert!(bad.validate().is_err());
    }
}
