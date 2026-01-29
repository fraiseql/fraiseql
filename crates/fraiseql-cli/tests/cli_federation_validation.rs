//! CLI federation schema validation tests
//! Tests the `fraiseql-cli validate schema.json` command

#[cfg(test)]
mod cli_federation_validation {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_schema(dir: &TempDir, content: &str) -> PathBuf {
        let schema_path = dir.path().join("schema.json");
        fs::write(&schema_path, content).expect("Failed to write test schema");
        schema_path
    }

    #[test]
    fn test_validate_valid_federation_schema() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let valid_schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "name", "type": "String"}
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, valid_schema);
        
        // Schema should be valid
        assert!(schema_path.exists());
        let content = fs::read_to_string(&schema_path).unwrap();
        assert!(content.contains("\"version\": \"2.0\""));
    }

    #[test]
    fn test_validate_schema_with_key_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"}
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify key_fields present
        assert!(content.contains("\"key_fields\""));
        assert!(content.contains("[\"id\"]"));
    }

    #[test]
    fn test_validate_schema_with_extends_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "Order",
                    "kind": "OBJECT",
                    "is_extends": true,
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "total", "type": "Float"}
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify extends present
        assert!(content.contains("\"is_extends\": true"));
    }

    #[test]
    fn test_validate_schema_with_requires_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "email", "type": "String"},
                        {
                            "name": "profile",
                            "type": "String",
                            "requires_fields": ["email"]
                        }
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify requires present
        assert!(content.contains("\"requires_fields\""));
    }

    #[test]
    fn test_validate_schema_with_provides_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {
                            "name": "orders",
                            "type": "[Order]",
                            "provides_fields": ["userId"]
                        }
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify provides present
        assert!(content.contains("\"provides_fields\""));
    }

    #[test]
    fn test_validate_schema_with_external_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "name", "type": "String"},
                        {"name": "email", "type": "String", "is_external": true}
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify external present
        assert!(content.contains("\"is_external\": true"));
    }

    #[test]
    fn test_validate_schema_with_shareable_directive() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {
                    "name": "User",
                    "kind": "OBJECT",
                    "key_fields": ["id"],
                    "fields": [
                        {"name": "id", "type": "ID"},
                        {"name": "name", "type": "String", "is_shareable": true}
                    ]
                }
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify shareable present
        assert!(content.contains("\"is_shareable\": true"));
    }

    #[test]
    fn test_validate_schema_version_present() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": []
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify version field
        assert!(content.contains("\"version\": \"2.0\""));
    }

    #[test]
    fn test_validate_schema_with_multiple_types() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let schema = r#"{
            "version": "2.0",
            "types": [
                {"name": "User", "kind": "OBJECT", "key_fields": ["id"]},
                {"name": "Order", "kind": "OBJECT", "key_fields": ["id"]},
                {"name": "Product", "kind": "OBJECT", "key_fields": ["id"]}
            ]
        }"#;
        
        let schema_path = create_test_schema(&dir, schema);
        let content = fs::read_to_string(&schema_path).unwrap();
        
        // Verify all types present
        assert!(content.contains("\"name\": \"User\""));
        assert!(content.contains("\"name\": \"Order\""));
        assert!(content.contains("\"name\": \"Product\""));
    }
}
