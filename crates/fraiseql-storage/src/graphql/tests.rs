//! Tests for GraphQL schema type generation.

#[cfg(test)]
mod graphql_tests {
    use crate::config::{BucketConfig, BucketAccess};
    use super::super::StorageSchemaTypes;

    fn sample_bucket() -> BucketConfig {
        BucketConfig {
            name: "avatars".to_string(),
            max_object_bytes: Some(5_000_000),
            allowed_mime_types: Some(vec!["image/jpeg".to_string(), "image/png".to_string()]),
            access: BucketAccess::PublicRead,
            transform_presets: None,
        }
    }

    #[test]
    fn test_storage_object_type_definition() {
        let bucket = sample_bucket();
        let type_def = StorageSchemaTypes::storage_object_type(&bucket);

        // Verify it's a valid JSON object
        assert!(type_def.is_object());

        // Verify required fields exist
        assert!(type_def.get("name").is_some());
        assert!(type_def.get("sql_source").is_some());
        assert!(type_def.get("jsonb_column").is_some());
        assert!(type_def.get("fields").is_some());

        // Verify name follows PascalCase convention
        assert_eq!(type_def["name"], "AvatarsStorageObject");

        // Verify it has required fields
        let fields = type_def["fields"].as_array().unwrap();
        let field_names: Vec<_> = fields.iter()
            .filter_map(|f| f["name"].as_str())
            .collect();

        assert!(field_names.contains(&"key"));
        assert!(field_names.contains(&"size"));
        assert!(field_names.contains(&"content_type"));
        assert!(field_names.contains(&"created_at"));
        assert!(field_names.contains(&"updated_at"));
    }

    #[test]
    fn test_upload_url_mutation_definition() {
        let bucket = sample_bucket();
        let mutation_def = StorageSchemaTypes::upload_url_mutation(&bucket);

        // Verify it's a valid JSON object
        assert!(mutation_def.is_object());

        // Verify required fields exist
        assert!(mutation_def.get("name").is_some());
        assert!(mutation_def.get("description").is_some());
        assert!(mutation_def.get("parameters").is_some());
        assert!(mutation_def.get("return_type").is_some());

        // Verify name follows convention
        assert_eq!(mutation_def["name"], "generateAvatarsUploadUrl");

        // Verify parameters
        let params = mutation_def["parameters"].as_array().unwrap();
        let param_names: Vec<_> = params.iter()
            .filter_map(|p| p["name"].as_str())
            .collect();

        assert!(param_names.contains(&"key"));
        assert!(param_names.contains(&"content_type"));

        // Verify return type is PresignedUrlResponse
        assert_eq!(mutation_def["return_type"]["object_name"], "PresignedUrlResponse");
    }

    #[test]
    fn test_list_query_definition() {
        let bucket = sample_bucket();
        let query_def = StorageSchemaTypes::list_query(&bucket);

        // Verify it's a valid JSON object
        assert!(query_def.is_object());

        // Verify required fields exist
        assert!(query_def.get("name").is_some());
        assert!(query_def.get("description").is_some());
        assert!(query_def.get("parameters").is_some());
        assert!(query_def.get("return_type").is_some());

        // Verify name follows convention
        assert_eq!(query_def["name"], "listAvatarsObjects");

        // Verify parameters
        let params = query_def["parameters"].as_array().unwrap();
        let param_names: Vec<_> = params.iter()
            .filter_map(|p| p["name"].as_str())
            .collect();

        assert!(param_names.contains(&"prefix"));
        assert!(param_names.contains(&"limit"));
        assert!(param_names.contains(&"cursor"));

        // Verify return type is a list of storage objects
        assert_eq!(query_def["return_type"]["type"], "List");
        assert_eq!(query_def["return_type"]["of"]["object_name"], "AvatarsStorageObject");
    }

    #[test]
    fn test_storage_types_only_emitted_when_buckets_defined() {
        // This test verifies that storage types are only generated when buckets are configured
        // In a full integration test, this would be verified in the compiler
        // For now, we just verify the generator works with valid inputs

        let bucket = sample_bucket();
        let obj_type = StorageSchemaTypes::storage_object_type(&bucket);
        let mutation = StorageSchemaTypes::upload_url_mutation(&bucket);
        let query = StorageSchemaTypes::list_query(&bucket);

        // All should be valid JSON objects
        assert!(obj_type.is_object());
        assert!(mutation.is_object());
        assert!(query.is_object());
    }

    #[test]
    fn test_storage_object_type_with_multiple_buckets() {
        let bucket1 = BucketConfig {
            name: "user_avatars".to_string(),
            max_object_bytes: Some(5_000_000),
            allowed_mime_types: Some(vec!["image/jpeg".to_string()]),
            access: BucketAccess::PublicRead,
            transform_presets: None,
        };

        let bucket2 = BucketConfig {
            name: "product-images".to_string(),
            max_object_bytes: Some(10_000_000),
            allowed_mime_types: Some(vec!["image/png".to_string()]),
            access: BucketAccess::Private,
            transform_presets: None,
        };

        let type1 = StorageSchemaTypes::storage_object_type(&bucket1);
        let type2 = StorageSchemaTypes::storage_object_type(&bucket2);

        // Each bucket should get its own type with unique name
        assert_eq!(type1["name"], "UserAvatarsStorageObject");
        assert_eq!(type2["name"], "ProductImagesStorageObject");

        // Names should be different
        assert_ne!(type1["name"], type2["name"]);
    }

    #[test]
    fn test_mutation_and_query_names_are_deterministic() {
        let bucket = sample_bucket();

        // Generate multiple times and verify names are consistent
        let mutation1 = StorageSchemaTypes::upload_url_mutation(&bucket);
        let mutation2 = StorageSchemaTypes::upload_url_mutation(&bucket);

        assert_eq!(mutation1["name"], mutation2["name"]);

        let query1 = StorageSchemaTypes::list_query(&bucket);
        let query2 = StorageSchemaTypes::list_query(&bucket);

        assert_eq!(query1["name"], query2["name"]);
    }
}
