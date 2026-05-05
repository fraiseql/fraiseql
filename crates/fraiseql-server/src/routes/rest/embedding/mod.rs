//! Nested resource embedding executor.
//!
//! Executes embedded resource sub-queries based on parsed [`EmbeddedSpec`]
//! entries from the `?select=` parameter. Supports `OneToMany` (array),
//! `ManyToOne` (single object), and `OneToOne` (object or null) cardinalities.
//!
//! For PostgreSQL, generates sub-queries with `jsonb_agg` / `jsonb_build_object`.
//! Empty collections return `[]`, not null. Single absent objects return `null`.

pub mod executor;

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    schema::{CompiledSchema, RestConfig},
    security::SecurityContext,
};

use super::{
    handler::RestError,
    params::{EmbeddedSpec, SelectEntry},
};
use executor::{count_related, embed_into_rows, embed_into_single, EmbedCtx};

/// Parameters for embedding execution, grouping shared context.
pub struct EmbeddingRequest<'a, A: DatabaseAdapter> {
    /// Query executor.
    pub executor:         &'a Arc<fraiseql_core::runtime::Executor<A>>,
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
