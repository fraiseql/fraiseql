//! Entity resolution for federation _entities query.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use ::tracing::info;
use fraiseql_db::{traits::DatabaseAdapter, DatabaseType};
use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;
use uuid::Uuid;

use super::{
    database_resolver::DatabaseEntityResolver,
    logging::{FederationLogContext, FederationOperationType, ResolutionStrategy},
    representation::validate_representations,
    selection_parser::FieldSelection,
    tracing::{FederationSpan, FederationTraceContext},
    types::{EntityRepresentation, FederationResolver},
};

/// Maximum number of entity representations allowed in a single `_entities` query.
///
/// Requests exceeding this limit are rejected immediately with a validation error
/// to prevent unbounded memory and CPU consumption from oversized federation batches.
const MAX_ENTITIES_BATCH_SIZE: usize = 1_000;

/// Result of entity resolution
#[derive(Debug)]
pub struct EntityResolutionResult {
    /// Resolved entities in same order as input representations
    pub entities: Vec<Option<Value>>,

    /// Any errors encountered during resolution
    pub errors: Vec<String>,
}

/// Result of batch entity resolution with timing information
#[derive(Debug)]
pub struct EntityResolutionMetrics {
    /// Resolved entities in same order as input representations
    pub entities:    Vec<Option<Value>>,
    /// Any errors encountered during resolution
    pub errors:      Vec<String>,
    /// Duration of resolution in microseconds
    pub duration_us: u64,
    /// Whether resolution succeeded (no errors)
    pub success:     bool,
}

/// Deduplicate entity representations while preserving order
pub fn deduplicate_representations(reps: &[EntityRepresentation]) -> Vec<EntityRepresentation> {
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(reps.len());

    for rep in reps {
        // Create a key from typename + key_fields
        let key = format!("{}:{:?}", rep.typename, rep.key_fields);
        if seen.insert(key) {
            result.push(rep.clone());
        }
    }

    result
}

/// Group entities by typename and strategy
pub fn group_entities_by_typename(
    reps: &[EntityRepresentation],
) -> HashMap<String, Vec<EntityRepresentation>> {
    let mut groups: HashMap<String, Vec<EntityRepresentation>> = HashMap::new();

    for rep in reps {
        groups.entry(rep.typename.clone()).or_default().push(rep.clone());
    }

    groups
}

/// Construct a parameterized WHERE clause for batch entity queries.
///
/// Returns `(sql, params)` where `sql` uses placeholders appropriate for the given
/// `db_type` and `params` holds the corresponding bind values.
///
/// # Errors
///
/// Returns an empty clause (no error) when `representations` or `key_columns` is empty.
pub fn construct_batch_where_clause(
    representations: &[EntityRepresentation],
    key_columns: &[String],
    db_type: DatabaseType,
) -> Result<(String, Vec<Value>)> {
    if representations.is_empty() || key_columns.is_empty() {
        return Ok((String::new(), vec![]));
    }

    let mut conditions = Vec::new();
    let mut params: Vec<Value> = Vec::new();
    let mut param_idx = 1usize;

    for key_col in key_columns {
        let values: Vec<&Value> = representations
            .iter()
            .filter_map(|rep| rep.key_fields.get(key_col))
            .collect();

        if !values.is_empty() {
            let mut placeholders = Vec::with_capacity(values.len());
            for value in values {
                placeholders.push(db_type.placeholder(param_idx));
                params.push((*value).clone());
                param_idx += 1;
            }

            // Double-quote the column identifier so that reserved words and
            // mixed-case names are handled correctly in PostgreSQL.
            let quoted_col = format!("\"{}\"", key_col.replace('"', "\"\""));
            conditions.push(format!("{quoted_col} IN ({})", placeholders.join(", ")));
        }
    }

    if conditions.is_empty() {
        Ok((String::new(), vec![]))
    } else {
        Ok((format!("WHERE {}", conditions.join(" AND ")), params))
    }
}

/// Resolve entities for a specific typename from local database
pub async fn resolve_entities_from_db<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    typename: &str,
    adapter: Arc<A>,
    fed_resolver: &FederationResolver,
    selection: &FieldSelection,
) -> EntityResolutionResult {
    resolve_entities_from_db_with_tracing(
        representations,
        typename,
        adapter,
        fed_resolver,
        selection,
        None,
    )
    .await
}

/// Resolve entities for a specific typename from local database with optional distributed tracing.
pub async fn resolve_entities_from_db_with_tracing<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    typename: &str,
    adapter: Arc<A>,
    fed_resolver: &FederationResolver,
    selection: &FieldSelection,
    trace_context: Option<FederationTraceContext>,
) -> EntityResolutionResult {
    if representations.is_empty() {
        return EntityResolutionResult {
            entities: Vec::new(),
            errors:   Vec::new(),
        };
    }

    // Create database entity resolver
    let db_resolver = DatabaseEntityResolver::new(adapter, fed_resolver.metadata.clone());

    // Resolve from database with tracing
    match db_resolver
        .resolve_entities_from_db_with_tracing(typename, representations, selection, trace_context)
        .await
    {
        Ok(entities) => EntityResolutionResult {
            entities,
            errors: Vec::new(),
        },
        Err(e) => EntityResolutionResult {
            entities: vec![None; representations.len()],
            errors:   vec![e.to_string()],
        },
    }
}

/// Batch load entities from database
pub async fn batch_load_entities<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
    adapter: Arc<A>,
    selection: &FieldSelection,
) -> Result<Vec<Option<Value>>> {
    batch_load_entities_with_tracing(representations, fed_resolver, adapter, selection, None).await
}

/// Batch load entities from database with optional distributed tracing and metrics.
pub async fn batch_load_entities_with_tracing<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
    adapter: Arc<A>,
    selection: &FieldSelection,
    trace_context: Option<FederationTraceContext>,
) -> Result<Vec<Option<Value>>> {
    let result = batch_load_entities_with_tracing_and_metrics(
        representations,
        fed_resolver,
        adapter,
        selection,
        trace_context,
    )
    .await?;
    Ok(result.entities)
}

/// Batch load entities with full metrics for observability.
///
/// Returns both entities and timing information for metrics recording.
pub async fn batch_load_entities_with_tracing_and_metrics<A: DatabaseAdapter>(
    representations: &[EntityRepresentation],
    fed_resolver: &FederationResolver,
    adapter: Arc<A>,
    selection: &FieldSelection,
    trace_context: Option<FederationTraceContext>,
) -> Result<EntityResolutionMetrics> {
    // Reject oversized batches immediately to prevent resource exhaustion.
    if representations.len() > MAX_ENTITIES_BATCH_SIZE {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Federation batch size {} exceeds maximum {}",
                representations.len(),
                MAX_ENTITIES_BATCH_SIZE
            ),
            path:    Some("_entities".into()),
        });
    }

    // Validate that all representations reference known types with required key fields.
    validate_representations(representations, &fed_resolver.metadata)?;

    let start_time = Instant::now();
    let query_id = Uuid::new_v4().to_string();

    if representations.is_empty() {
        return Ok(EntityResolutionMetrics {
            entities:    Vec::new(),
            errors:      Vec::new(),
            duration_us: 0,
            success:     true,
        });
    }

    // Create or use provided trace context
    let trace_ctx = trace_context.unwrap_or_default();

    // Create span for federation query
    let span = FederationSpan::new("federation.entities.batch_load", trace_ctx.clone())
        .with_attribute("entity_count", representations.len().to_string())
        .with_attribute("typename_count", count_unique_typenames(representations).to_string());

    // Log entity resolution start
    let log_ctx = FederationLogContext::new(
        FederationOperationType::EntityResolution,
        query_id.clone(),
        representations.len(),
    )
    .with_entity_count_unique(deduplicate_representations(representations).len())
    .with_trace_id(trace_ctx.trace_id.clone());

    info!(
        query_id = %query_id,
        entity_count = representations.len(),
        operation_type = "entity_resolution",
        status = "started",
        context = ?serde_json::to_value(&log_ctx).unwrap_or_default(),
        "Entity resolution operation started"
    );

    // Group by typename
    let grouped = group_entities_by_typename(representations);

    let mut all_results: Vec<(usize, Option<Value>)> = Vec::new();
    let mut current_index = 0;
    let mut all_errors = Vec::new();

    for (typename, reps) in grouped {
        let batch_start = Instant::now();

        // Create child span for this typename batch
        let child_span = span
            .create_child(format!("federation.entities.resolve.{}", typename))
            .with_attribute("typename", typename.clone())
            .with_attribute("batch_size", reps.len().to_string());

        // Resolve this batch using database with trace context
        let result = resolve_entities_from_db_with_tracing(
            &reps,
            &typename,
            Arc::clone(&adapter),
            fed_resolver,
            selection,
            Some(trace_ctx.clone()),
        )
        .await;

        // Record batch metrics
        let resolved_count = result.entities.iter().filter(|e| e.is_some()).count();
        let error_count = result.errors.len();
        let batch_duration_ms = batch_start.elapsed().as_secs_f64() * 1000.0;

        // Log batch completion
        let batch_log_ctx = FederationLogContext::new(
            FederationOperationType::ResolveDb,
            query_id.clone(),
            reps.len(),
        )
        .with_typename(typename.clone())
        .with_strategy(ResolutionStrategy::Db)
        .with_entity_count_unique(reps.len())
        .with_resolved_count(resolved_count)
        .with_trace_id(trace_ctx.trace_id.clone())
        .complete(batch_duration_ms);

        if error_count > 0 {
            info!(
                query_id = %query_id,
                typename = %typename,
                batch_size = reps.len(),
                resolved = resolved_count,
                errors = error_count,
                duration_ms = batch_duration_ms,
                operation_type = "resolve_db",
                status = "error",
                context = ?serde_json::to_value(&batch_log_ctx).unwrap_or_default(),
                "Entity batch resolution completed with errors"
            );
        } else {
            info!(
                query_id = %query_id,
                typename = %typename,
                batch_size = reps.len(),
                resolved = resolved_count,
                duration_ms = batch_duration_ms,
                operation_type = "resolve_db",
                status = "success",
                context = ?serde_json::to_value(&batch_log_ctx).unwrap_or_default(),
                "Entity batch resolution completed successfully"
            );
        }

        // Map results back to original indices with proper ordering
        for entity in result.entities {
            all_results.push((current_index, entity));
            current_index += 1;
        }

        // Collect errors
        all_errors.extend(result.errors.clone());

        // Drop child span
        drop(child_span);
    }

    // Sort by original index to preserve order
    all_results.sort_by_key(|(idx, _)| *idx);

    // Record final span attributes
    let _span_duration = span.duration_ms();
    let resolved_count = all_results.iter().filter(|(_, e)| e.is_some()).count();

    // Keep span alive until function returns
    drop(span);

    let duration_us = start_time.elapsed().as_micros() as u64;
    let duration_ms = start_time.elapsed().as_secs_f64() * 1000.0;
    let entities = all_results.into_iter().map(|(_, e)| e).collect();
    let success = all_errors.is_empty();

    // Log overall completion
    let final_log_ctx = if success {
        log_ctx.with_resolved_count(resolved_count).complete(duration_ms)
    } else {
        let error_message = if all_errors.is_empty() {
            "Unknown error".to_string()
        } else {
            all_errors.join("; ")
        };
        log_ctx.with_resolved_count(resolved_count).fail(duration_ms, error_message)
    };

    info!(
        query_id = %query_id,
        entity_count = representations.len(),
        resolved_count = resolved_count,
        error_count = all_errors.len(),
        duration_ms = duration_ms,
        operation_type = "entity_resolution",
        status = if success { "success" } else { "error" },
        context = ?serde_json::to_value(&final_log_ctx).unwrap_or_default(),
        "Entity resolution operation completed"
    );

    Ok(EntityResolutionMetrics {
        entities,
        errors: all_errors,
        duration_us,
        success,
    })
}

/// Count unique typenames in representations
fn count_unique_typenames(representations: &[EntityRepresentation]) -> usize {
    let mut typenames = HashSet::new();
    for rep in representations {
        typenames.insert(&rep.typename);
    }
    typenames.len()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use serde_json::json;

    use super::*;

    #[test]
    fn test_deduplicate_representations() {
        let reps = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("123"));
                    m
                },
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("123"));
                    m
                },
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: {
                    let mut m = HashMap::new();
                    m.insert("id".to_string(), json!("456"));
                    m
                },
                all_fields: HashMap::new(),
            },
        ];

        let deduped = deduplicate_representations(&reps);
        assert_eq!(deduped.len(), 2);
    }

    #[test]
    fn test_group_entities_by_typename() {
        let reps = vec![
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "Order".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
            EntityRepresentation {
                typename:   "User".to_string(),
                key_fields: HashMap::new(),
                all_fields: HashMap::new(),
            },
        ];

        let grouped = group_entities_by_typename(&reps);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["User"].len(), 2);
        assert_eq!(grouped["Order"].len(), 1);
    }

    #[test]
    fn test_construct_batch_where_clause_parameterized() {
        let mut rep1 = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        };
        rep1.key_fields.insert("id".to_string(), json!("123"));

        let mut rep2 = EntityRepresentation {
            typename:   "User".to_string(),
            key_fields: HashMap::new(),
            all_fields: HashMap::new(),
        };
        rep2.key_fields.insert("id".to_string(), json!("456"));

        let reps = vec![rep1, rep2];
        let (sql, params) =
            construct_batch_where_clause(&reps, &["id".to_string()], DatabaseType::PostgreSQL)
                .unwrap();

        assert!(sql.contains("WHERE"));
        // Column name must be double-quoted for PostgreSQL identifier safety.
        assert!(sql.contains("\"id\" IN"), "expected quoted column \"id\" IN");
        // Values must be in params, not in SQL
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(!sql.contains("123"));
        assert!(!sql.contains("456"));
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], json!("123"));
        assert_eq!(params[1], json!("456"));
    }
}
