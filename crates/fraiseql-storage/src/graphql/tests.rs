//! Tests for GraphQL schema type generation.

use crate::config::{BucketAccess, BucketConfig};
use super::StorageSchemaTypes;

fn sample_bucket() -> BucketConfig {
    BucketConfig {
        name: "avatars".to_string(),
        max_object_bytes: Some(5_000_000),
        allowed_mime_types: Some(vec!["image/jpeg".to_string(), "image/png".to_string()]),
        access: BucketAccess::PublicRead,
        transform_presets: None,
    }
}

// --- bucket_type_name ---

#[test]
fn test_bucket_type_name_snake_case() {
    assert_eq!(
        StorageSchemaTypes::bucket_type_name("user_avatars"),
        "UserAvatars"
    );
}

#[test]
fn test_bucket_type_name_kebab_case() {
    assert_eq!(
        StorageSchemaTypes::bucket_type_name("product-images"),
        "ProductImages"
    );
}

#[test]
fn test_bucket_type_name_single_word() {
    assert_eq!(StorageSchemaTypes::bucket_type_name("files"), "Files");
}

// --- storage_object_type ---

#[test]
fn test_storage_object_type_definition() {
    let bucket = sample_bucket();
    let type_def = StorageSchemaTypes::storage_object_type(&bucket);

    assert_eq!(type_def["name"], "AvatarsStorageObject");
    assert_eq!(type_def["sql_source"], "t_storage_avatars");
    assert_eq!(type_def["jsonb_column"], "data");

    let fields = type_def["fields"].as_array().unwrap();
    let field_names: Vec<_> = fields.iter().filter_map(|f| f["name"].as_str()).collect();
    assert_eq!(
        field_names,
        &["key", "size", "content_type", "created_at", "updated_at"]
    );

    // Scalar field_type is a bare string, not a nested object
    assert_eq!(fields[0]["field_type"], "String");
    assert_eq!(fields[1]["field_type"], "Int");
    assert_eq!(fields[3]["field_type"], "DateTime");

    // Only updated_at is nullable
    assert!(fields[4]["nullable"].as_bool().unwrap());
    assert!(fields[0].get("nullable").is_none());
}

// --- upload_url_mutation ---

#[test]
fn test_upload_url_mutation_definition() {
    let bucket = sample_bucket();
    let mutation = StorageSchemaTypes::upload_url_mutation(&bucket);

    assert_eq!(mutation["name"], "generateAvatarsUploadUrl");
    assert_eq!(mutation["operation"], "Custom");
    assert_eq!(mutation["return_type"], "PresignedUrlResponse");

    // Arguments use arg_type, not field_type
    let args = mutation["arguments"].as_array().unwrap();
    let arg_names: Vec<_> = args.iter().filter_map(|a| a["name"].as_str()).collect();
    assert_eq!(arg_names, &["key", "content_type"]);
    assert_eq!(args[0]["arg_type"], "String");
}

// --- list_query ---

#[test]
fn test_list_query_definition() {
    let bucket = sample_bucket();
    let query = StorageSchemaTypes::list_query(&bucket);

    assert_eq!(query["name"], "listAvatarsObjects");
    assert_eq!(query["return_type"], "AvatarsStorageObject");
    assert!(query["returns_list"].as_bool().unwrap());
    assert_eq!(query["sql_source"], "t_storage_avatars");

    let args = query["arguments"].as_array().unwrap();
    let arg_names: Vec<_> = args.iter().filter_map(|a| a["name"].as_str()).collect();
    assert_eq!(arg_names, &["prefix", "limit", "cursor"]);

    // All query arguments are nullable
    for arg in args {
        assert!(arg["nullable"].as_bool().unwrap());
    }

    // Arguments use arg_type
    assert_eq!(args[0]["arg_type"], "String");
    assert_eq!(args[1]["arg_type"], "Int");
}

// --- generate (batch) ---

#[test]
fn test_generate_empty_buckets_returns_empty() {
    let entries = StorageSchemaTypes::generate(&[]);
    assert!(entries.types.is_empty());
    assert!(entries.mutations.is_empty());
    assert!(entries.queries.is_empty());
}

#[test]
fn test_generate_produces_entries_per_bucket() {
    let buckets = vec![
        sample_bucket(),
        BucketConfig {
            name: "documents".to_string(),
            max_object_bytes: None,
            allowed_mime_types: None,
            access: BucketAccess::Private,
            transform_presets: None,
        },
    ];

    let entries = StorageSchemaTypes::generate(&buckets);
    assert_eq!(entries.types.len(), 2);
    assert_eq!(entries.mutations.len(), 2);
    assert_eq!(entries.queries.len(), 2);

    assert_eq!(entries.types[0]["name"], "AvatarsStorageObject");
    assert_eq!(entries.types[1]["name"], "DocumentsStorageObject");
}

// --- multi-bucket ---

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

    assert_eq!(type1["name"], "UserAvatarsStorageObject");
    assert_eq!(type2["name"], "ProductImagesStorageObject");
    assert_ne!(type1["name"], type2["name"]);
}

// --- determinism ---

#[test]
fn test_mutation_and_query_names_are_deterministic() {
    let bucket = sample_bucket();

    let m1 = StorageSchemaTypes::upload_url_mutation(&bucket);
    let m2 = StorageSchemaTypes::upload_url_mutation(&bucket);
    assert_eq!(m1["name"], m2["name"]);

    let q1 = StorageSchemaTypes::list_query(&bucket);
    let q2 = StorageSchemaTypes::list_query(&bucket);
    assert_eq!(q1["name"], q2["name"]);
}
