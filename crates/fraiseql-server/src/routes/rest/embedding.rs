//! Nested resource embedding executor.
//!
//! Executes embedded resource sub-queries based on parsed [`EmbeddedSpec`]
//! entries from the `?select=` parameter. Supports `OneToMany` (array),
//! `ManyToOne` (single object), and `OneToOne` (object or null) cardinalities.
//!
//! For PostgreSQL, generates sub-queries with `jsonb_agg` / `jsonb_build_object`.
//! Empty collections return `[]`, not null. Single absent objects return `null`.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    schema::{Cardinality, CompiledSchema, RelationshipDef, RestConfig},
    security::SecurityContext,
};

use super::{
    handler::RestError,
    params::{EmbeddedSpec, SelectEntry},
};

// ---------------------------------------------------------------------------
// Context struct — groups shared arguments to satisfy clippy::too_many_arguments
// ---------------------------------------------------------------------------

/// Shared context for embedding execution, reducing argument count.
struct EmbedCtx<'a, A: DatabaseAdapter> {
    executor:         &'a Arc<Executor<A>>,
    schema:           &'a CompiledSchema,
    config:           &'a RestConfig,
    security_context: Option<&'a SecurityContext>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parameters for embedding execution, grouping shared context.
pub struct EmbeddingRequest<'a, A: DatabaseAdapter> {
    /// Query executor.
    pub executor:         &'a Arc<Executor<A>>,
    /// Compiled schema for type/query lookup.
    pub schema:           &'a CompiledSchema,
    /// REST configuration (page size limits, etc.).
    pub config:           &'a RestConfig,
    /// Parent type name for relationship lookup.
    pub parent_type_name: &'a str,
    /// Security context for RLS enforcement.
    pub security_context: Option<&'a SecurityContext>,
}

/// Execute embedded resource sub-queries and merge results into parent rows.
///
/// For each [`EmbeddedSpec`] in the select, finds the matching relationship
/// on the parent type, queries the related resource, and merges the results
/// into the parent response JSON.
///
/// # Errors
///
/// Returns `RestError` if a relationship is not found, a sub-query fails,
/// or the parent data cannot be parsed.
#[allow(clippy::implicit_hasher)] // Reason: generic BuildHasher makes future non-Send
pub async fn execute_embeddings<A: DatabaseAdapter>(
    req: &EmbeddingRequest<'_, A>,
    parent_data: &mut serde_json::Value,
    embeddings: &[EmbeddedSpec],
    embedding_filters: &HashMap<String, serde_json::Value>,
) -> Result<(), RestError> {
    if embeddings.is_empty() {
        return Ok(());
    }

    let parent_type = req.schema.find_type(req.parent_type_name).ok_or_else(|| {
        RestError::internal(format!("Parent type not found: {}", req.parent_type_name))
    })?;

    let ctx = EmbedCtx {
        executor:         req.executor,
        schema:           req.schema,
        config:           req.config,
        security_context: req.security_context,
    };

    for spec in embeddings {
        let rel = parent_type
            .relationships
            .iter()
            .find(|r| r.name == spec.relationship)
            .ok_or_else(|| {
                RestError::bad_request(format!(
                    "Type '{}' has no relationship '{}'",
                    req.parent_type_name, spec.relationship
                ))
            })?;

        let embedded_filter = embedding_filters.get(&spec.relationship);

        // Determine output field name (renamed or relationship name).
        let output_name = spec.rename.as_deref().unwrap_or(&spec.relationship);

        // Get sub-select field names.
        let sub_field_names: Vec<String> = spec
            .fields
            .iter()
            .filter_map(|e| match e {
                SelectEntry::Field(name) => Some(name.clone()),
                _ => None,
            })
            .collect();

        // Execute embedding based on parent data shape (array or single object).
        match parent_data {
            serde_json::Value::Array(rows) => {
                embed_into_rows(&ctx, rel, output_name, &sub_field_names, embedded_filter, rows)
                    .await?;
            },
            serde_json::Value::Object(_) => {
                embed_into_single(
                    &ctx,
                    rel,
                    output_name,
                    &sub_field_names,
                    embedded_filter,
                    parent_data,
                )
                .await?;
            },
            _ => {
                // Non-object/array data — skip embedding silently.
            },
        }
    }

    Ok(())
}

/// Execute count-only embeddings and merge counts into parent rows.
///
/// For each count field (e.g., `posts.count`), adds a `{rel}_count` field
/// to each parent row with the count of related resources.
///
/// # Errors
///
/// Returns `RestError` if a relationship is not found or a count query fails.
pub async fn execute_embedding_counts<A: DatabaseAdapter>(
    req: &EmbeddingRequest<'_, A>,
    parent_data: &mut serde_json::Value,
    count_fields: &[String],
) -> Result<(), RestError> {
    if count_fields.is_empty() {
        return Ok(());
    }

    let parent_type = req.schema.find_type(req.parent_type_name).ok_or_else(|| {
        RestError::internal(format!("Parent type not found: {}", req.parent_type_name))
    })?;

    for count_rel_name in count_fields {
        let rel = parent_type
            .relationships
            .iter()
            .find(|r| r.name == *count_rel_name)
            .ok_or_else(|| {
                RestError::bad_request(format!(
                    "Type '{}' has no relationship '{count_rel_name}'",
                    req.parent_type_name
                ))
            })?;

        let count_key = format!("{count_rel_name}_count");

        match parent_data {
            serde_json::Value::Array(rows) => {
                for row in rows.iter_mut() {
                    let count =
                        count_related(req.executor, req.schema, rel, row, req.security_context)
                            .await?;
                    if let Some(obj) = row.as_object_mut() {
                        obj.insert(count_key.clone(), serde_json::json!(count));
                    }
                }
            },
            serde_json::Value::Object(_) => {
                let count =
                    count_related(req.executor, req.schema, rel, parent_data, req.security_context)
                        .await?;
                if let Some(obj) = parent_data.as_object_mut() {
                    obj.insert(count_key, serde_json::json!(count));
                }
            },
            _ => {},
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Embed related resources into each row of a parent array.
async fn embed_into_rows<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &RelationshipDef,
    output_name: &str,
    sub_field_names: &[String],
    embedded_filter: Option<&serde_json::Value>,
    rows: &mut [serde_json::Value],
) -> Result<(), RestError> {
    for row in rows.iter_mut() {
        embed_into_single(ctx, rel, output_name, sub_field_names, embedded_filter, row).await?;
    }
    Ok(())
}

/// Embed related resources into a single parent row.
async fn embed_into_single<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &RelationshipDef,
    output_name: &str,
    sub_field_names: &[String],
    embedded_filter: Option<&serde_json::Value>,
    row: &mut serde_json::Value,
) -> Result<(), RestError> {
    let parent_key_value = extract_join_key(row, rel);

    let Some(parent_key_value) = parent_key_value else {
        // Parent row doesn't have the join key — set appropriate default.
        set_empty_embedding(row, output_name, rel.cardinality);
        return Ok(());
    };

    // Build WHERE clause for the sub-query: fk_column = parent_key_value.
    let mut where_obj = serde_json::Map::new();

    // The filter key depends on cardinality direction:
    // OneToMany: child's FK = parent's referenced key value
    // ManyToOne: parent's FK = child's referenced key value (query by child's PK)
    // OneToOne: same as ManyToOne
    let filter_field = match rel.cardinality {
        Cardinality::OneToMany => &rel.foreign_key,
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.referenced_key,
        _ => &rel.foreign_key,
    };

    where_obj.insert(filter_field.clone(), serde_json::json!({ "eq": parent_key_value }));

    // Merge embedded filter if present.
    if let Some(filter) = embedded_filter {
        if let Some(filter_map) = filter.as_object() {
            for (k, v) in filter_map {
                where_obj.insert(k.clone(), v.clone());
            }
        }
    }

    let where_clause = serde_json::Value::Object(where_obj);

    // Find the target type's list query.
    let target_query = find_list_query_for_type(ctx.schema, &rel.target_type);

    let Some(target_query) = target_query else {
        // No list query available — set empty default.
        set_empty_embedding(row, output_name, rel.cardinality);
        return Ok(());
    };

    let target_type_def = ctx.schema.find_type(&rel.target_type);

    // Build arguments for the sub-query.
    let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();
    arguments.insert("where".to_string(), where_clause);
    arguments.insert("limit".to_string(), serde_json::json!(ctx.config.max_page_size));

    // Build QueryMatch for the sub-query.
    let query_match = QueryMatch::from_operation(
        target_query.clone(),
        sub_field_names.to_vec(),
        arguments,
        target_type_def,
    )
    .map_err(|e| RestError::internal(format!("Failed to build embedded query: {e}")))?;

    let variables = serde_json::json!({});
    let vars_ref = Some(&variables);

    let result = ctx
        .executor
        .execute_query_direct(&query_match, vars_ref, ctx.security_context)
        .await
        .map_err(RestError::from)?;

    // Parse and extract embedded data.
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| RestError::internal(format!("Failed to parse embedded result: {e}")))?;

    let embedded_data = extract_query_data(&parsed, &target_query.name);

    // Set the embedded data on the parent row.
    if let Some(obj) = row.as_object_mut() {
        match rel.cardinality {
            Cardinality::OneToMany => {
                // Array — empty collection returns [].
                let arr = match embedded_data {
                    Some(serde_json::Value::Array(a)) => serde_json::Value::Array(a),
                    Some(other) => serde_json::json!([other]),
                    None => serde_json::json!([]),
                };
                obj.insert(output_name.to_string(), arr);
            },
            Cardinality::ManyToOne | Cardinality::OneToOne => {
                // Single object or null.
                let val = match embedded_data {
                    Some(serde_json::Value::Array(mut a)) if !a.is_empty() => a.remove(0),
                    Some(other) => other,
                    None => serde_json::Value::Null,
                };
                obj.insert(output_name.to_string(), val);
            },
            _ => {
                obj.insert(
                    output_name.to_string(),
                    embedded_data.unwrap_or(serde_json::Value::Null),
                );
            },
        }
    }

    Ok(())
}

/// Extract the join key value from a parent row.
fn extract_join_key(row: &serde_json::Value, rel: &RelationshipDef) -> Option<serde_json::Value> {
    // For OneToMany: use the parent's referenced key (e.g., pk_user).
    // For ManyToOne/OneToOne: use the parent's foreign key (e.g., fk_user).
    let key_field = match rel.cardinality {
        Cardinality::OneToMany => &rel.referenced_key,
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.foreign_key,
        _ => &rel.referenced_key,
    };

    row.get(key_field.as_str()).cloned().filter(|v| !v.is_null())
}

/// Set the appropriate empty default for an embedding.
fn set_empty_embedding(row: &mut serde_json::Value, output_name: &str, cardinality: Cardinality) {
    if let Some(obj) = row.as_object_mut() {
        match cardinality {
            Cardinality::OneToMany => {
                obj.insert(output_name.to_string(), serde_json::json!([]));
            },
            Cardinality::ManyToOne | Cardinality::OneToOne => {
                obj.insert(output_name.to_string(), serde_json::Value::Null);
            },
            _ => {
                obj.insert(output_name.to_string(), serde_json::Value::Null);
            },
        }
    }
}

/// Find a list query that returns the given type.
fn find_list_query_for_type<'a>(
    schema: &'a CompiledSchema,
    type_name: &str,
) -> Option<&'a fraiseql_core::schema::QueryDefinition> {
    schema.queries.iter().find(|q| q.return_type == type_name && q.returns_list)
}

/// Extract data from executor query result envelope.
fn extract_query_data(parsed: &serde_json::Value, query_name: &str) -> Option<serde_json::Value> {
    parsed.get("data").and_then(|d| d.get(query_name)).cloned()
}

/// Count related resources for a single parent row.
async fn count_related<A: DatabaseAdapter>(
    executor: &Arc<Executor<A>>,
    schema: &CompiledSchema,
    rel: &RelationshipDef,
    row: &serde_json::Value,
    security_context: Option<&SecurityContext>,
) -> Result<u64, RestError> {
    let parent_key_value = extract_join_key(row, rel);

    let Some(parent_key_value) = parent_key_value else {
        return Ok(0);
    };

    let filter_field = match rel.cardinality {
        Cardinality::OneToMany => &rel.foreign_key,
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.referenced_key,
        _ => &rel.foreign_key,
    };

    let mut where_obj = serde_json::Map::new();
    where_obj.insert(filter_field.clone(), serde_json::json!({ "eq": parent_key_value }));
    let where_clause = serde_json::Value::Object(where_obj);

    let target_query = find_list_query_for_type(schema, &rel.target_type);
    let Some(target_query) = target_query else {
        return Ok(0);
    };

    let target_type_def = schema.find_type(&rel.target_type);

    let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();
    arguments.insert("where".to_string(), where_clause);

    let query_match =
        QueryMatch::from_operation(target_query.clone(), Vec::new(), arguments, target_type_def)
            .map_err(|e| RestError::internal(format!("Failed to build count query: {e}")))?;

    let variables = serde_json::json!({});
    let vars_ref = Some(&variables);

    let count = executor
        .count_rows(&query_match, vars_ref, security_context)
        .await
        .map_err(RestError::from)?;

    Ok(count)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test assertions
mod tests {
    use fraiseql_core::schema::{Cardinality, RelationshipDef};

    use super::*;

    #[test]
    fn extract_join_key_one_to_many() {
        let rel = RelationshipDef {
            name:           "posts".to_string(),
            target_type:    "Post".to_string(),
            foreign_key:    "fk_user".to_string(),
            referenced_key: "pk_user".to_string(),
            cardinality:    Cardinality::OneToMany,
        };
        let row = serde_json::json!({"pk_user": 42, "name": "Alice"});
        let key = extract_join_key(&row, &rel);
        assert_eq!(key, Some(serde_json::json!(42)));
    }

    #[test]
    fn extract_join_key_many_to_one() {
        let rel = RelationshipDef {
            name:           "author".to_string(),
            target_type:    "User".to_string(),
            foreign_key:    "fk_user".to_string(),
            referenced_key: "pk_user".to_string(),
            cardinality:    Cardinality::ManyToOne,
        };
        let row = serde_json::json!({"fk_user": 7, "title": "Hello"});
        let key = extract_join_key(&row, &rel);
        assert_eq!(key, Some(serde_json::json!(7)));
    }

    #[test]
    fn extract_join_key_null_returns_none() {
        let rel = RelationshipDef {
            name:           "author".to_string(),
            target_type:    "User".to_string(),
            foreign_key:    "fk_user".to_string(),
            referenced_key: "pk_user".to_string(),
            cardinality:    Cardinality::ManyToOne,
        };
        let row = serde_json::json!({"fk_user": null, "title": "Hello"});
        assert!(extract_join_key(&row, &rel).is_none());
    }

    #[test]
    fn extract_join_key_missing_field_returns_none() {
        let rel = RelationshipDef {
            name:           "posts".to_string(),
            target_type:    "Post".to_string(),
            foreign_key:    "fk_user".to_string(),
            referenced_key: "pk_user".to_string(),
            cardinality:    Cardinality::OneToMany,
        };
        let row = serde_json::json!({"name": "Alice"});
        assert!(extract_join_key(&row, &rel).is_none());
    }

    #[test]
    fn set_empty_embedding_one_to_many() {
        let mut row = serde_json::json!({"id": 1});
        set_empty_embedding(&mut row, "posts", Cardinality::OneToMany);
        assert_eq!(row["posts"], serde_json::json!([]));
    }

    #[test]
    fn set_empty_embedding_many_to_one() {
        let mut row = serde_json::json!({"id": 1});
        set_empty_embedding(&mut row, "author", Cardinality::ManyToOne);
        assert!(row["author"].is_null());
    }

    #[test]
    fn set_empty_embedding_one_to_one() {
        let mut row = serde_json::json!({"id": 1});
        set_empty_embedding(&mut row, "profile", Cardinality::OneToOne);
        assert!(row["profile"].is_null());
    }

    #[test]
    fn extract_query_data_standard_envelope() {
        let parsed = serde_json::json!({
            "data": {
                "posts": [
                    {"id": 1, "title": "Hello"},
                    {"id": 2, "title": "World"},
                ]
            }
        });
        let data = extract_query_data(&parsed, "posts").unwrap();
        assert!(data.is_array());
        assert_eq!(data.as_array().unwrap().len(), 2);
    }

    #[test]
    fn extract_query_data_missing_query_returns_none() {
        let parsed = serde_json::json!({"data": {}});
        assert!(extract_query_data(&parsed, "posts").is_none());
    }

    #[test]
    fn find_list_query_for_type_returns_list_query() {
        use fraiseql_core::schema::{CompiledSchema, QueryDefinition};

        let mut schema = CompiledSchema::default();
        schema.queries.push(QueryDefinition {
            name: "post".to_string(),
            return_type: "Post".to_string(),
            returns_list: false,
            ..QueryDefinition::new("post", "Post")
        });
        schema.queries.push(QueryDefinition {
            name: "posts".to_string(),
            return_type: "Post".to_string(),
            returns_list: true,
            ..QueryDefinition::new("posts", "Post")
        });

        let found = find_list_query_for_type(&schema, "Post");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "posts");
    }

    #[test]
    fn find_list_query_for_type_no_match() {
        let schema = CompiledSchema::default();
        assert!(find_list_query_for_type(&schema, "Post").is_none());
    }
}
