//! GraphQL schema type generation for storage operations.
//!
//! This module generates GraphQL type definitions that are embedded in the
//! compiled schema, allowing storage buckets to be represented as first-class
//! types in the GraphQL API.
//!
//! The generated JSON matches the compiled schema format used by `fraiseql-core`:
//! - `field_type` uses bare scalars (`"String"`) and tagged variants (`{"Object": "Foo"}`)
//! - Mutations use `"arguments"` (not `"parameters"`) and top-level `"return_type"` / `"operation"`
//! - Queries use `"returns_list"` as a sibling boolean, not nested `{"List": ...}`

use serde_json::{json, Value};
use crate::config::BucketConfig;

/// Generates GraphQL type definitions for storage operations.
///
/// Each bucket produces three schema entries:
/// - A `TypeDefinition` for the storage object type
/// - A `MutationDefinition` for presigned upload URL generation
/// - A `QueryDefinition` for listing objects
pub struct StorageSchemaTypes;

/// All generated schema entries for a set of storage buckets.
#[derive(Debug, Default)]
pub struct StorageSchemaEntries {
    /// Type definitions for inclusion in the compiled schema `types` array.
    pub types: Vec<Value>,
    /// Mutation definitions for inclusion in the compiled schema `mutations` array.
    pub mutations: Vec<Value>,
    /// Query definitions for inclusion in the compiled schema `queries` array.
    pub queries: Vec<Value>,
}

impl StorageSchemaTypes {
    /// Generate all schema entries for the given buckets.
    ///
    /// Returns empty vectors when `buckets` is empty — no storage types are
    /// emitted unless at least one bucket is configured.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_storage::graphql::StorageSchemaTypes;
    /// use fraiseql_storage::config::{BucketConfig, BucketAccess};
    ///
    /// let buckets = vec![BucketConfig {
    ///     name: "avatars".to_string(),
    ///     max_object_bytes: Some(5_000_000),
    ///     allowed_mime_types: Some(vec!["image/jpeg".to_string()]),
    ///     access: BucketAccess::PublicRead,
    ///     transform_presets: None,
    /// }];
    ///
    /// let entries = StorageSchemaTypes::generate(&buckets);
    /// assert_eq!(entries.types.len(), 1);
    /// assert_eq!(entries.mutations.len(), 1);
    /// assert_eq!(entries.queries.len(), 1);
    /// ```
    pub fn generate(buckets: &[BucketConfig]) -> StorageSchemaEntries {
        let mut entries = StorageSchemaEntries::default();
        for bucket in buckets {
            entries.types.push(Self::storage_object_type(bucket));
            entries.mutations.push(Self::upload_url_mutation(bucket));
            entries.queries.push(Self::list_query(bucket));
        }
        entries
    }

    /// Generate a storage object type for a bucket.
    ///
    /// Creates a `TypeDefinition` JSON object representing files in the bucket
    /// with fields for metadata (key, size, content_type, created_at, updated_at).
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
                    "field_type": "String",
                    "description": "Object key in the bucket"
                },
                {
                    "name": "size",
                    "field_type": "Int",
                    "description": "Size in bytes"
                },
                {
                    "name": "content_type",
                    "field_type": "String",
                    "description": "MIME type"
                },
                {
                    "name": "created_at",
                    "field_type": "DateTime",
                    "description": "Upload timestamp"
                },
                {
                    "name": "updated_at",
                    "field_type": "DateTime",
                    "nullable": true,
                    "description": "Last modified timestamp"
                }
            ]
        })
    }

    /// Generate an upload URL mutation for a bucket.
    ///
    /// Creates a `MutationDefinition` JSON object that generates presigned
    /// upload URLs for direct client-to-storage uploads.
    pub fn upload_url_mutation(bucket: &BucketConfig) -> Value {
        let mutation_name = format!("generate{}UploadUrl", Self::bucket_type_name(&bucket.name));

        json!({
            "name": mutation_name,
            "description": format!("Generate presigned upload URL for {} bucket", bucket.name),
            "operation": "Custom",
            "return_type": "PresignedUrlResponse",
            "arguments": [
                {
                    "name": "key",
                    "arg_type": "String",
                    "description": "Object key"
                },
                {
                    "name": "content_type",
                    "arg_type": "String",
                    "description": "MIME type of the file being uploaded"
                }
            ]
        })
    }

    /// Generate a list query for a bucket.
    ///
    /// Creates a `QueryDefinition` JSON object that lists objects in the bucket
    /// with optional prefix filtering and cursor-based pagination.
    pub fn list_query(bucket: &BucketConfig) -> Value {
        let query_name = format!("list{}Objects", Self::bucket_type_name(&bucket.name));
        let return_type_name = format!("{}StorageObject", Self::bucket_type_name(&bucket.name));

        json!({
            "name": query_name,
            "description": format!("List objects in {} bucket", bucket.name),
            "return_type": return_type_name,
            "returns_list": true,
            "sql_source": format!("t_storage_{}", bucket.name),
            "jsonb_column": "data",
            "arguments": [
                {
                    "name": "prefix",
                    "arg_type": "String",
                    "nullable": true,
                    "description": "Filter by key prefix"
                },
                {
                    "name": "limit",
                    "arg_type": "Int",
                    "nullable": true,
                    "description": "Maximum number of results"
                },
                {
                    "name": "cursor",
                    "arg_type": "String",
                    "nullable": true,
                    "description": "Pagination cursor"
                }
            ]
        })
    }

    /// Convert a bucket name to a valid GraphQL type name.
    ///
    /// Converts snake_case or kebab-case bucket names to `PascalCase` for use in
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
