//! Tests for `utils/` modules.
//! Re-export items not in `crate::utils::*` so submodules reach them via `use super::*`.
#![allow(unused_imports)] // Reason: blanket re-exports for test convenience

pub use crate::utils::{
    operators::{OPERATOR_REGISTRY, get_operators_by_category},
    vector::VectorInsertQuery,
};

mod casing_tests {

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_simple_camel_to_snake() {
        assert_eq!(to_snake_case("userId"), "user_id");
        assert_eq!(to_snake_case("userName"), "user_name");
        assert_eq!(to_snake_case("firstName"), "first_name");
    }

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(to_snake_case("UserId"), "user_id");
        assert_eq!(to_snake_case("FirstName"), "first_name");
    }

    #[test]
    fn test_consecutive_capitals() {
        assert_eq!(to_snake_case("HTTPResponse"), "http_response");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("IOError"), "io_error");
    }

    #[test]
    fn test_already_snake_case() {
        assert_eq!(to_snake_case("user_id"), "user_id");
        assert_eq!(to_snake_case("first_name"), "first_name");
        assert_eq!(to_snake_case("http_response"), "http_response");
    }

    #[test]
    fn test_mixed_formats() {
        assert_eq!(to_snake_case("user_Id"), "user_id"); // Convert mixed formats
        assert_eq!(to_snake_case("HTTPStatus_Code"), "http_status_code");
    }

    #[test]
    fn test_single_char() {
        assert_eq!(to_snake_case("a"), "a");
        assert_eq!(to_snake_case("A"), "a");
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(to_snake_case(""), "");
    }

    #[test]
    fn test_numbers() {
        assert_eq!(to_snake_case("user2FA"), "user2fa"); // Numbers don't trigger underscore
        assert_eq!(to_snake_case("level99Boss"), "level99boss");
    }

    #[test]
    fn test_simple_snake_to_camel() {
        assert_eq!(to_camel_case("user_id"), "userId");
        assert_eq!(to_camel_case("first_name"), "firstName");
        assert_eq!(to_camel_case("http_response"), "httpResponse");
    }

    #[test]
    fn test_already_camel_case() {
        assert_eq!(to_camel_case("userId"), "userId");
        assert_eq!(to_camel_case("firstName"), "firstName");
    }

    #[test]
    fn test_multiple_underscores() {
        assert_eq!(to_camel_case("user__id"), "userId");
        assert_eq!(to_camel_case("http___response"), "httpResponse");
    }

    #[test]
    fn test_trailing_underscore() {
        assert_eq!(to_camel_case("user_id_"), "userId");
        assert_eq!(to_camel_case("first_name_"), "firstName");
    }

    #[test]
    fn test_normalize_field_path_simple() {
        assert_eq!(normalize_field_path("userId"), "user_id");
        assert_eq!(normalize_field_path("createdAt"), "created_at");
    }

    #[test]
    fn test_normalize_field_path_nested() {
        assert_eq!(normalize_field_path("user.createdAt"), "user.created_at");
        assert_eq!(
            normalize_field_path("device.sensorData.currentValue"),
            "device.sensor_data.current_value"
        );
    }

    #[test]
    fn test_normalize_field_path_already_snake() {
        assert_eq!(normalize_field_path("user_id"), "user_id");
        assert_eq!(normalize_field_path("user.created_at"), "user.created_at");
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = "userId";
        let snake = to_snake_case(original);
        let back = to_camel_case(&snake);
        assert_eq!(back, original);

        let original2 = "HTTPResponse";
        let snake2 = to_snake_case(original2);
        assert_eq!(snake2, "http_response");
        let back2 = to_camel_case(&snake2);
        assert_eq!(back2, "httpResponse"); // Note: loses the capitalization pattern
    }

    #[test]
    fn test_real_world_examples() {
        // Common GraphQL field names
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("updatedAt"), "updated_at");
        assert_eq!(to_snake_case("deletedAt"), "deleted_at");
        assert_eq!(to_snake_case("isActive"), "is_active");
        assert_eq!(to_snake_case("isDeleted"), "is_deleted");
        assert_eq!(to_snake_case("machineId"), "machine_id");
        assert_eq!(to_snake_case("deviceType"), "device_type");
    }
}

mod opaque_id_tests {

    use super::super::*;
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_opaque_id_creation() {
        let opaque = OpaqueId::new("12345");
        assert!(!opaque.as_str().is_empty());
        // Opaque ID should not contain the original ID in plain text
        assert!(!opaque.as_str().contains("12345"));
    }

    #[test]
    fn test_opaque_id_decode() {
        let db_id = "user_42";
        let opaque = OpaqueId::new(db_id);
        let decoded = opaque.decode();
        assert_eq!(decoded, Some(db_id.to_string()));
    }

    #[test]
    fn test_opaque_id_with_signature() {
        let db_id = "12345";
        let secret = b"secret_key";
        let opaque = OpaqueId::with_signature(db_id, secret);

        // Should be able to verify with correct secret
        assert!(opaque.verify_signature(secret));

        // Should fail with wrong secret
        assert!(!opaque.verify_signature(b"wrong_secret"));
    }

    #[test]
    fn test_opaque_id_signature_tampering() {
        let db_id = "sensitive_id_789";
        let secret = b"super_secret";
        let mut opaque = OpaqueId::with_signature(db_id, secret);

        // Verify original
        assert!(opaque.verify_signature(secret));

        // Tamper with the opaque ID
        opaque.id = opaque.id.chars().rev().collect();

        // Should fail verification
        assert!(!opaque.verify_signature(secret));
    }

    #[test]
    fn test_opaque_id_equality() {
        let opaque1 = OpaqueId::new("same_id");
        let opaque2 = OpaqueId::new("same_id");
        assert_eq!(opaque1, opaque2);

        let opaque3 = OpaqueId::new("different_id");
        assert_ne!(opaque1, opaque3);
    }

    #[test]
    fn test_opaque_id_prevents_enumeration() {
        let ids: Vec<String> = (1..=5).map(|i| i.to_string()).collect();
        let opaque_ids: Vec<OpaqueId> = ids.iter().map(OpaqueId::new).collect();

        // Even though original IDs are sequential, opaque IDs should look random
        for i in 1..opaque_ids.len() {
            // Check that opaque IDs don't follow a predictable pattern
            assert_ne!(opaque_ids[i].as_str(), opaque_ids[i - 1].as_str());
        }

        // Verify that opaque IDs are different from the original sequential pattern
        for i in 0..opaque_ids.len() {
            let original = ids[i].as_str();
            let opaque = opaque_ids[i].as_str();
            // Opaque ID should not contain the original ID in plain text
            assert!(!opaque.contains(original));
        }
    }
}

mod operators_tests {

    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use super::{super::*, *};

    #[test]
    fn test_operator_registry_initialized() {
        // Should have all 40+ operators
        assert!(OPERATOR_REGISTRY.len() >= 40);
    }

    #[test]
    fn test_comparison_operators() {
        let operators = ["eq", "ne", "gt", "gte", "lt", "lte", "in", "nin"];

        for op_name in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "Operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::Comparison);
            assert!(!op.jsonb_operator);
        }
    }

    #[test]
    fn test_string_operators() {
        let operators = [
            "like", "ilike", "nlike", "nilike", "regex", "iregex", "nregex", "niregex",
        ];

        for op_name in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "String operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::String);
        }
    }

    #[test]
    fn test_null_operators() {
        let op1 = get_operator_info("is_null").unwrap();
        assert_eq!(op1.sql_op, "IS NULL");
        assert_eq!(op1.category, OperatorCategory::Null);

        let op2 = get_operator_info("is_not_null").unwrap();
        assert_eq!(op2.sql_op, "IS NOT NULL");
        assert_eq!(op2.category, OperatorCategory::Null);
    }

    #[test]
    fn test_containment_operators() {
        let operators = [
            "contains",
            "contained_in",
            "has_key",
            "has_any_keys",
            "has_all_keys",
        ];

        for op_name in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "Containment operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::Containment);
            assert!(op.jsonb_operator, "{op_name} should be JSONB operator");
        }
    }

    #[test]
    fn test_array_operators() {
        let operators = ["array_contains", "array_contained_in", "array_overlaps"];

        for op_name in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "Array operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::Array);
        }
    }

    #[test]
    fn test_vector_operators() {
        let operators = [
            ("cosine_distance", "<=>"),
            ("l2_distance", "<->"),
            ("inner_product", "<#>"),
            ("l1_distance", "<+>"),
            ("hamming_distance", "<~>"),
            ("jaccard_distance", "<%>"),
        ];

        for (op_name, expected_sql) in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "Vector operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::Vector);
            assert_eq!(op.sql_op, *expected_sql);
        }
    }

    #[test]
    fn test_fulltext_operators() {
        let operators = [
            "search",
            "plainto_tsquery",
            "phraseto_tsquery",
            "websearch_to_tsquery",
        ];

        for op_name in &operators {
            let op = get_operator_info(op_name);
            assert!(op.is_some(), "Fulltext operator {op_name} should exist");

            let op = op.unwrap();
            assert_eq!(op.category, OperatorCategory::Fulltext);
            assert_eq!(op.sql_op, "@@");
        }
    }

    #[test]
    fn test_is_operator() {
        assert!(is_operator("eq"));
        assert!(is_operator("contains"));
        assert!(is_operator("cosine_distance"));
        assert!(!is_operator("invalid_operator"));
        assert!(!is_operator(""));
    }

    #[test]
    fn test_get_operators_by_category() {
        let comparison_ops = get_operators_by_category(OperatorCategory::Comparison);
        assert!(comparison_ops.len() >= 8);

        let vector_ops = get_operators_by_category(OperatorCategory::Vector);
        assert!(vector_ops.len() >= 6);

        let fulltext_ops = get_operators_by_category(OperatorCategory::Fulltext);
        assert!(fulltext_ops.len() >= 4);
    }

    #[test]
    fn test_requires_array_flag() {
        // IN and NOT IN require arrays
        assert!(get_operator_info("in").unwrap().requires_array);
        assert!(get_operator_info("nin").unwrap().requires_array);

        // Most operators don't require arrays
        assert!(!get_operator_info("eq").unwrap().requires_array);
        assert!(!get_operator_info("like").unwrap().requires_array);
    }

    #[test]
    fn test_jsonb_operator_flag() {
        // Containment operators are JSONB-specific
        assert!(get_operator_info("contains").unwrap().jsonb_operator);
        assert!(get_operator_info("has_key").unwrap().jsonb_operator);

        // Most operators are not JSONB-specific
        assert!(!get_operator_info("eq").unwrap().jsonb_operator);
        assert!(!get_operator_info("like").unwrap().jsonb_operator);
    }
}

mod vector_tests {

    use super::{super::*, *};
    use crate::schema::{DistanceMetric, VectorConfig};

    #[test]
    fn test_similarity_search_basic() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents")
            .with_embedding_column("embedding")
            .with_limit(10);

        let embedding = vec![0.1, 0.2, 0.3];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("SELECT *"));
        assert!(sql.contains("FROM documents"));
        assert!(sql.contains("ORDER BY embedding <=>"));
        assert!(sql.contains("$1::vector"));
        assert!(sql.contains("LIMIT $2"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_similarity_search_with_columns() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_select_columns(vec!["id".to_string(), "content".to_string()])
            .with_distance_score()
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("SELECT id, content,"));
        assert!(sql.contains("AS distance"));
        assert!(sql.contains("LIMIT $2"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn test_similarity_search_with_where() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents")
            .with_where("metadata->>'type' = 'article'")
            .with_limit(10);

        let embedding = vec![0.1, 0.2, 0.3];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("WHERE metadata->>'type' = 'article'"));
    }

    #[test]
    fn test_similarity_search_with_offset() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("documents").with_limit(10).with_offset(20);

        let embedding = vec![0.1, 0.2];
        let (sql, params) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("OFFSET $3"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_similarity_search_l2_distance() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_distance_metric(DistanceMetric::L2)
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("<->"));
    }

    #[test]
    fn test_similarity_search_inner_product() {
        let builder = VectorQueryBuilder::new();
        let query = VectorSearchQuery::new("docs")
            .with_distance_metric(DistanceMetric::InnerProduct)
            .with_limit(5);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("<#>"));
    }

    #[test]
    fn test_insert_one_basic() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents").with_columns(vec![
            "id".to_string(),
            "content".to_string(),
            "embedding".to_string(),
        ]);

        let values = vec![
            VectorParam::String("doc1".to_string()),
            VectorParam::String("Hello world".to_string()),
            VectorParam::Vector(vec![0.1, 0.2, 0.3]),
        ];

        let (sql, params) = builder.insert_one(&query, &values);

        assert!(sql.contains("INSERT INTO documents (id, content, embedding)"));
        assert!(sql.contains("VALUES ($1, $2, $3::vector)"));
        assert!(sql.contains("RETURNING id"));
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn test_insert_upsert() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents")
            .with_columns(vec![
                "id".to_string(),
                "content".to_string(),
                "embedding".to_string(),
            ])
            .with_upsert(vec!["id".to_string()]);

        let values = vec![
            VectorParam::String("doc1".to_string()),
            VectorParam::String("Hello world".to_string()),
            VectorParam::Vector(vec![0.1, 0.2, 0.3]),
        ];

        let (sql, _) = builder.insert_one(&query, &values);

        assert!(sql.contains("ON CONFLICT (id) DO UPDATE SET"));
        assert!(sql.contains("content = EXCLUDED.content"));
        assert!(sql.contains("embedding = EXCLUDED.embedding"));
    }

    #[test]
    fn test_insert_batch() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents")
            .with_columns(vec!["id".to_string(), "embedding".to_string()]);

        let rows = vec![
            vec![
                VectorParam::String("doc1".to_string()),
                VectorParam::Vector(vec![0.1, 0.2]),
            ],
            vec![
                VectorParam::String("doc2".to_string()),
                VectorParam::Vector(vec![0.3, 0.4]),
            ],
        ];

        let (sql, params) = builder.insert_batch(&query, &rows);

        assert!(sql.contains("INSERT INTO documents (id, embedding)"));
        assert!(sql.contains("($1, $2::vector)"));
        assert!(sql.contains("($3, $4::vector)"));
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn test_insert_batch_empty() {
        let builder = VectorQueryBuilder::new();
        let query = VectorInsertQuery::new("documents").with_columns(vec!["id".to_string()]);

        let (sql, params) = builder.insert_batch(&query, &[]);

        assert!(sql.is_empty());
        assert!(params.is_empty());
    }

    #[test]
    fn test_create_index_hnsw() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::openai();

        let sql = builder.create_index(&config, "documents", "embedding");

        assert_eq!(
            sql,
            Some("CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)".to_string())
        );
    }

    #[test]
    fn test_create_index_ivfflat() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::new(1536)
            .with_index(crate::schema::VectorIndexType::IvfFlat)
            .with_distance(DistanceMetric::L2);

        let sql = builder.create_index(&config, "docs", "vec");

        assert_eq!(sql, Some("CREATE INDEX ON docs USING ivfflat (vec vector_l2_ops)".to_string()));
    }

    #[test]
    fn test_create_index_none() {
        let builder = VectorQueryBuilder::new();
        let config = VectorConfig::new(1536).with_index(crate::schema::VectorIndexType::None);

        let sql = builder.create_index(&config, "documents", "embedding");

        assert_eq!(sql, None);
    }

    #[test]
    fn test_vector_param_to_sql_literal() {
        let vec_param = VectorParam::Vector(vec![0.1, 0.2, 0.3]);
        assert_eq!(vec_param.to_sql_literal(), "'[0.1,0.2,0.3]'::vector");

        let int_param = VectorParam::Int(42);
        assert_eq!(int_param.to_sql_literal(), "42");

        let str_param = VectorParam::String("hello".to_string());
        assert_eq!(str_param.to_sql_literal(), "'hello'");

        let str_param_escape = VectorParam::String("it's a test".to_string());
        assert_eq!(str_param_escape.to_sql_literal(), "'it''s a test'");
    }

    #[test]
    fn test_question_mark_placeholders() {
        let builder = VectorQueryBuilder::with_question_marks();
        let query = VectorSearchQuery::new("docs").with_limit(10);

        let embedding = vec![0.1, 0.2];
        let (sql, _) = builder.similarity_search(&query, &embedding);

        assert!(sql.contains("?::vector"));
        assert!(!sql.contains("$1"));
    }
}
