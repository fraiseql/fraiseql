//! Embedding executor: join key extraction, relationship traversal, and data merging.

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::{
    db::traits::DatabaseAdapter,
    runtime::{Executor, QueryMatch},
    schema::{Cardinality, CompiledSchema, Relationship, RestConfig},
    security::SecurityContext,
};

use crate::routes::rest::handler::RestError;

/// Shared context for embedding execution, reducing argument count.
pub(super) struct EmbedCtx<'a, A: DatabaseAdapter> {
    pub executor: &'a Arc<Executor<A>>,
    pub schema: &'a CompiledSchema,
    pub config: &'a RestConfig,
    pub security_context: Option<&'a SecurityContext>,
}

/// Embed related resources into each row of a parent array.
pub(super) async fn embed_into_rows<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &Relationship,
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
pub(super) async fn embed_into_single<A: DatabaseAdapter>(
    ctx: &EmbedCtx<'_, A>,
    rel: &Relationship,
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

    // Extract embedded data directly from the executor result.
    let embedded_data = extract_query_data(&result, &target_query.name);

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
pub(super) fn extract_join_key(
    row: &serde_json::Value,
    rel: &Relationship,
) -> Option<serde_json::Value> {
    // For OneToMany: use the parent's referenced key (e.g., pk_user).
    // For ManyToOne/OneToOne: use the parent's foreign key (e.g., fk_user).
    let key_field = match rel.cardinality {
        Cardinality::ManyToOne | Cardinality::OneToOne => &rel.foreign_key,
        _ => &rel.referenced_key,
    };

    row.get(key_field.as_str()).cloned().filter(|v| !v.is_null())
}

/// Set the appropriate empty default for an embedding.
pub(super) fn set_empty_embedding(
    row: &mut serde_json::Value,
    output_name: &str,
    cardinality: Cardinality,
) {
    if let Some(obj) = row.as_object_mut() {
        match cardinality {
            Cardinality::OneToMany => {
                obj.insert(output_name.to_string(), serde_json::json!([]));
            },
            _ => {
                obj.insert(output_name.to_string(), serde_json::Value::Null);
            },
        }
    }
}

/// Find a list query that returns the given type.
pub(super) fn find_list_query_for_type<'a>(
    schema: &'a CompiledSchema,
    type_name: &str,
) -> Option<&'a fraiseql_core::schema::QueryDefinition> {
    schema.queries.iter().find(|q| q.return_type == type_name && q.returns_list)
}

/// Extract data from executor query result envelope.
pub(super) fn extract_query_data(
    parsed: &serde_json::Value,
    query_name: &str,
) -> Option<serde_json::Value> {
    parsed.get("data").and_then(|d| d.get(query_name)).cloned()
}

/// Count related resources for a single parent row.
pub(super) async fn count_related<A: DatabaseAdapter>(
    executor: &Arc<Executor<A>>,
    schema: &CompiledSchema,
    rel: &Relationship,
    row: &serde_json::Value,
    security_context: Option<&SecurityContext>,
) -> Result<u64, RestError> {
    let parent_key_value = extract_join_key(row, rel);

    let Some(parent_key_value) = parent_key_value else {
        return Ok(0);
    };

    let filter_field = match rel.cardinality {
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
