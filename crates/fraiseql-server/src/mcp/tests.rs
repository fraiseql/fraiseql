mod executor_tests {
    use fraiseql_core::schema::FieldType;

    use super::super::executor::{graphql_value, is_scalar_field_type, is_valid_graphql_name};

    #[test]
    fn test_graphql_value_string() {
        let v = serde_json::Value::String("hello".to_string());
        assert_eq!(graphql_value(&v), "\"hello\"");
    }

    #[test]
    fn test_graphql_value_string_escapes_quotes() {
        let v = serde_json::Value::String("say \"hi\"".to_string());
        assert_eq!(graphql_value(&v), r#""say \"hi\"""#);
    }

    #[test]
    fn test_graphql_value_string_escapes_backslash() {
        let v = serde_json::Value::String(r"a\b".to_string());
        assert_eq!(graphql_value(&v), r#""a\\b""#);
    }

    #[test]
    fn test_graphql_value_string_escapes_newline() {
        let v = serde_json::Value::String("line1\nline2".to_string());
        assert_eq!(graphql_value(&v), "\"line1\\nline2\"");
    }

    #[test]
    fn test_is_valid_graphql_name() {
        assert!(is_valid_graphql_name("limit"));
        assert!(is_valid_graphql_name("_private"));
        assert!(is_valid_graphql_name("field1"));
        assert!(!is_valid_graphql_name(""));
        assert!(!is_valid_graphql_name("1abc"));
        assert!(!is_valid_graphql_name("has space"));
        assert!(!is_valid_graphql_name("inject: bad"));
    }

    #[test]
    fn test_graphql_value_number() {
        let v = serde_json::json!(42);
        assert_eq!(graphql_value(&v), "42");
    }

    #[test]
    fn test_graphql_value_bool() {
        let v = serde_json::Value::Bool(true);
        assert_eq!(graphql_value(&v), "true");
    }

    #[test]
    fn test_graphql_value_array() {
        let v = serde_json::json!([1, 2, 3]);
        assert_eq!(graphql_value(&v), "[1, 2, 3]");
    }

    #[test]
    fn test_is_scalar_field_type() {
        assert!(is_scalar_field_type(&FieldType::String));
        assert!(is_scalar_field_type(&FieldType::Int));
        assert!(is_scalar_field_type(&FieldType::List(Box::new(FieldType::Int))));
        assert!(!is_scalar_field_type(&FieldType::Object("User".to_string())));
    }
}

mod tools_tests {
    #![allow(clippy::unwrap_used)]

    use fraiseql_core::schema::{ArgumentDefinition, FieldType};

    use super::super::{
        McpConfig,
        tools::{arguments_to_json_schema, field_type_to_json_schema, should_include},
    };

    fn make_config(include: Vec<String>, exclude: Vec<String>) -> McpConfig {
        McpConfig {
            enabled: true,
            transport: "http".to_string(),
            path: "/mcp".to_string(),
            require_auth: true,
            include,
            exclude,
        }
    }

    #[test]
    fn test_should_include_all_when_empty() {
        let config = make_config(vec![], vec![]);
        assert!(should_include("users", &config));
        assert!(should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_whitelist() {
        let config = make_config(vec!["users".to_string()], vec![]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_should_include_blacklist() {
        let config = make_config(vec![], vec!["createUser".to_string()]);
        assert!(should_include("users", &config));
        assert!(!should_include("createUser", &config));
    }

    #[test]
    fn test_field_type_to_json_schema() {
        let schema = field_type_to_json_schema(&FieldType::String);
        assert_eq!(schema, serde_json::json!({ "type": "string" }));

        let schema = field_type_to_json_schema(&FieldType::Int);
        assert_eq!(schema, serde_json::json!({ "type": "integer" }));

        let schema = field_type_to_json_schema(&FieldType::Boolean);
        assert_eq!(schema, serde_json::json!({ "type": "boolean" }));

        let schema = field_type_to_json_schema(&FieldType::List(Box::new(FieldType::Int)));
        assert_eq!(schema, serde_json::json!({ "type": "array", "items": { "type": "integer" } }));
    }

    #[test]
    fn test_arguments_to_json_schema() {
        let args = vec![
            ArgumentDefinition::new("id", FieldType::Id),
            ArgumentDefinition::optional("name", FieldType::String),
        ];

        let schema = arguments_to_json_schema(&args);
        let props = schema.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("id"));
        assert!(props.contains_key("name"));

        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "id");
    }
}
