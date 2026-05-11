#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;
    use crate::types::{FederatedType, FederationMetadata, KeyDirective};

    fn make_metadata(typename: &str, key_field: &str) -> FederationMetadata {
        FederationMetadata {
            enabled: true,
            version: "v2".to_string(),
            types:   vec![FederatedType {
                name:             typename.to_string(),
                keys:             vec![KeyDirective {
                    fields:     vec![key_field.to_string()],
                    resolvable: true,
                }],
                is_extends:       false,
                external_fields:  Vec::new(),
                shareable_fields: Vec::new(),
                    inaccessible_fields: Vec::new(),
                field_directives: std::collections::HashMap::new(),
                type_shareable:  false,
            }],
            remote_subscription_fields: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_build_update_query() {
        let meta = make_metadata("Order", "id");
        let vars = json!({ "id": "42", "status": "shipped" });
        let sql = build_update_query("Order", &vars, &meta).unwrap();
        assert!(sql.contains("UPDATE"), "missing UPDATE keyword: {sql}");
        assert!(sql.contains("\"order\""), "table name must be quoted: {sql}");
        assert!(sql.contains("SET"), "missing SET clause: {sql}");
        assert!(sql.contains("\"status\""), "column must be quoted: {sql}");
        assert!(sql.contains("WHERE"), "missing WHERE clause: {sql}");
        assert!(sql.contains("\"id\""), "key column must be quoted: {sql}");

        // Single-quote escaping in values
        let vars_with_apostrophe = json!({ "id": "1", "name": "O'Brien" });
        let sql2 = build_update_query("Order", &vars_with_apostrophe, &meta).unwrap();
        assert!(sql2.contains("O''Brien"), "apostrophe must be escaped: {sql2}");
    }

    #[test]
    fn test_build_insert_query() {
        let meta = make_metadata("Group", "id");
        let vars = json!({ "id": "7", "name": "Admins" });
        let sql = build_insert_query("Group", &vars, &meta).unwrap();
        assert!(sql.contains("INSERT INTO"), "missing INSERT INTO: {sql}");
        assert!(sql.contains("\"group\""), "table name must be quoted: {sql}");
        assert!(sql.contains("VALUES"), "missing VALUES clause: {sql}");
        assert!(sql.contains("\"id\""), "column must be quoted: {sql}");
        assert!(sql.contains("\"name\""), "column must be quoted: {sql}");

        // Single-quote escaping in values
        let vars_apostrophe = json!({ "id": "2", "label": "O'Hara's Team" });
        let sql2 = build_insert_query("Group", &vars_apostrophe, &meta).unwrap();
        assert!(sql2.contains("O''Hara''s Team"), "apostrophe must be escaped: {sql2}");
    }

    #[test]
    fn test_build_delete_query() {
        let meta = make_metadata("User", "id");
        let vars = json!({ "id": "99" });
        let sql = build_delete_query("User", &vars, &meta).unwrap();
        assert!(sql.contains("DELETE FROM"), "missing DELETE FROM: {sql}");
        assert!(sql.contains("\"user\""), "table name must be quoted: {sql}");
        assert!(sql.contains("WHERE"), "missing WHERE clause: {sql}");
        assert!(sql.contains("\"id\""), "key column must be quoted: {sql}");
        assert!(sql.contains("'99'"), "key value must appear in SQL: {sql}");
    }

    #[test]
    fn test_value_to_sql_literal_string() {
        let result = value_to_sql_literal(&Value::String("John".to_string())).unwrap();
        assert_eq!(result, "'John'");

        // Test SQL injection prevention
        let result = value_to_sql_literal(&Value::String("O'Brien".to_string())).unwrap();
        assert_eq!(result, "'O''Brien'");
    }

    #[test]
    fn test_value_to_sql_literal_number() {
        let result = value_to_sql_literal(&Value::Number(123.into())).unwrap();
        assert_eq!(result, "123");
    }

    #[test]
    fn test_value_to_sql_literal_null() {
        let result = value_to_sql_literal(&Value::Null).unwrap();
        assert_eq!(result, "NULL");
    }
