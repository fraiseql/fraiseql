//! GraphQL schema type generation for storage operations.
//!
//! This module generates GraphQL type definitions that are embedded in the
//! compiled schema, allowing storage buckets to be represented as first-class
//! types in the GraphQL API.

use serde_json::{json, Value};
use crate::config::BucketConfig;

/// Generates GraphQL type definitions for storage operations.
pub struct StorageSchemaTypes;

impl StorageSchemaTypes {
    /// Generate a storage object type for a bucket.
    ///
    /// Creates a GraphQL object type representing files in the bucket with fields
    /// for metadata (key, size, content-type, created_at, updated_at).
    ///
    /// # Returns
    ///
    /// A JSON object suitable for inclusion in the compiled schema's `types` array.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_storage::graphql::StorageSchemaTypes;
    /// use fraiseql_storage::config::{BucketConfig, BucketAccess};
    ///
    /// let bucket = BucketConfig {
    ///     name: "avatars".to_string(),
    ///     max_object_bytes: Some(5_000_000),
    ///     allowed_mime_types: Some(vec!["image/jpeg".to_string()]),
    ///     access: BucketAccess::PublicRead,
    ///     transform_presets: None,
    /// };
    ///
    /// let type_def = StorageSchemaTypes::storage_object_type(&bucket);
    /// assert_eq!(type_def["name"], "AvatarsStorageObject");
    /// ```
    pub fn storage_object_type(bucket: &BucketConfig) -> Value {
        let type_name = format!("{}StorageObject", Self::bucket_type_name(&bucket.name));

        json!({
            "name": type_name,
            "sql_source": format!("t_storage_{}", bucket.name),
            "jsonb_column": "data",
            "description": format!("Storage object in the {} bucket", bucket.name),
            "fields": [
                {
                    "name": "key",
                    "field_type": { "type": "String" },
                    "nullable": false,
                    "description": "Object key in the bucket"
                },
                {
                    "name": "size",
                    "field_type": { "type": "Int" },
                    "nullable": false,
                    "description": "Size in bytes"
                },
                {
                    "name": "content_type",
                    "field_type": { "type": "String" },
                    "nullable": false,
                    "description": "MIME type"
                },
                {
                    "name": "created_at",
                    "field_type": { "type": "DateTime" },
                    "nullable": false,
                    "description": "Upload timestamp"
                },
                {
                    "name": "updated_at",
                    "field_type": { "type": "DateTime" },
                    "nullable": true,
                    "description": "Last modified timestamp"
                }
            ]
        })
    }

    /// Generate an upload URL mutation for a bucket.
    ///
    /// Creates a GraphQL mutation that generates presigned upload URLs for direct
    /// client-to-storage uploads.
    ///
    /// # Returns
    ///
    /// A JSON object suitable for inclusion in the compiled schema's `mutations` array.
    pub fn upload_url_mutation(bucket: &BucketConfig) -> Value {
        let mutation_name = format!("generate{}UploadUrl", Self::bucket_type_name(&bucket.name));

        json!({
            "name": mutation_name,
            "description": format!("Generate presigned upload URL for {} bucket", bucket.name),
            "parameters": [
                {
                    "name": "key",
                    "field_type": { "type": "String" },
                    "nullable": false,
                    "description": "Object key"
                },
                {
                    "name": "content_type",
                    "field_type": { "type": "String" },
                    "nullable": false,
                    "description": "MIME type of the file being uploaded"
                }
            ],
            "return_type": {
                "type": "Object",
                "object_name": "PresignedUrlResponse",
                "nullable": false
            }
        })
    }

    /// Generate a list query for a bucket.
    ///
    /// Creates a GraphQL query that lists objects in the bucket with optional
    /// prefix filtering and pagination.
    ///
    /// # Returns
    ///
    /// A JSON object suitable for inclusion in the compiled schema's `queries` array.
    pub fn list_query(bucket: &BucketConfig) -> Value {
        let query_name = format!("list{}Objects", Self::bucket_type_name(&bucket.name));
        let return_type_name = format!("{}StorageObject", Self::bucket_type_name(&bucket.name));

        json!({
            "name": query_name,
            "description": format!("List objects in {} bucket", bucket.name),
            "parameters": [
                {
                    "name": "prefix",
                    "field_type": { "type": "String" },
                    "nullable": true,
                    "description": "Filter by key prefix"
                },
                {
                    "name": "limit",
                    "field_type": { "type": "Int" },
                    "nullable": true,
                    "description": "Maximum number of results"
                },
                {
                    "name": "cursor",
                    "field_type": { "type": "String" },
                    "nullable": true,
                    "description": "Pagination cursor"
                }
            ],
            "return_type": {
                "type": "List",
                "of": {
                    "type": "Object",
                    "object_name": return_type_name
                },
                "nullable": false
            }
        })
    }

    /// Convert a bucket name to a valid GraphQL type name.
    ///
    /// Converts snake_case or kebab-case bucket names to PascalCase for use in
    /// GraphQL type names.
    ///
    /// # Examples
    ///
    /// ```
    /// use fraiseql_storage::graphql::StorageSchemaTypes;
    ///
    /// assert_eq!(StorageSchemaTypes::bucket_type_name("user_avatars"), "UserAvatars");
    /// assert_eq!(StorageSchemaTypes::bucket_type_name("product-images"), "ProductImages");
    /// ```
    pub fn bucket_type_name(bucket_name: &str) -> String {
        bucket_name
            .split(['_', '-'])
            .filter(|s| !s.is_empty())
            .map(|s| {
                let mut chars = s.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod helper_tests {
    use super::*;

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
        assert_eq!(
            StorageSchemaTypes::bucket_type_name("files"),
            "Files"
        );
    }
}
