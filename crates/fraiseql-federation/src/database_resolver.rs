//! Database entity resolution for federation.
//!
//! Executes actual database queries to resolve entities from local databases,
//! replacing mock data with real results.

use std::sync::Arc;

use ::tracing::warn;
use fraiseql_db::{
    DatabaseType, GenericWhereGenerator, MySqlDialect, PostgresDialect, SqlServerDialect,
    SqliteDialect, WhereClause, traits::DatabaseAdapter,
};
use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{
    metadata_helpers::find_federation_type,
    query_builder::construct_where_in_clause,
    requires_provides_validator::RequiresProvidesRuntimeValidator,
    selection_parser::FieldSelection,
    sql_utils::is_safe_sql_identifier,
    tracing::FederationTraceContext,
    types::{EntityRepresentation, FederatedType, FederationMetadata},
};

/// Resolves federation entities from local databases.
pub struct DatabaseEntityResolver<A: DatabaseAdapter> {
    /// Database adapter for executing queries
    adapter:        Arc<A>,
    /// Federation metadata
    metadata:       FederationMetadata,
    /// Backing relation (`sql_source`) per entity type name. Empty → the resolver
    /// falls back to `lower(typename)` (#504).
    entity_sources: std::collections::HashMap<String, String>,
}

impl<A: DatabaseAdapter> DatabaseEntityResolver<A> {
    /// Create a new database entity resolver.
    ///
    /// The `_entities` `FROM` relation falls back to `lower(typename)` unless a
    /// per-type `sql_source` map is attached via
    /// [`with_entity_sources`](Self::with_entity_sources).
    #[must_use]
    pub fn new(adapter: Arc<A>, metadata: FederationMetadata) -> Self {
        Self {
            adapter,
            metadata,
            entity_sources: std::collections::HashMap::new(),
        }
    }

    /// Attach the per-entity-type backing relation map (`typename` → `sql_source`),
    /// so the resolver reads from the real view instead of `lower(typename)` (#504).
    #[must_use]
    pub fn with_entity_sources(
        mut self,
        entity_sources: std::collections::HashMap<String, String>,
    ) -> Self {
        self.entity_sources = entity_sources;
        self
    }

    /// Resolve entities from database.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name (e.g., "User")
    /// * `representations` - Entity representations with key field values
    /// * `selection` - Field selection from GraphQL query
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn resolve_entities_from_db(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        self.resolve_entities_from_db_with_tracing(typename, representations, selection, None)
            .await
    }

    /// Resolve entities from database with optional distributed tracing.
    ///
    /// # Arguments
    ///
    /// * `typename` - The entity type name (e.g., "User")
    /// * `representations` - Entity representations with key field values
    /// * `selection` - Field selection from GraphQL query
    /// * `trace_context` - Optional W3C trace context for span creation
    ///
    /// # Returns
    ///
    /// Vector of resolved entities (or None for missing entities)
    ///
    /// # Errors
    ///
    /// Returns error if database query fails
    pub async fn resolve_entities_from_db_with_tracing(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
        trace_context: Option<FederationTraceContext>,
    ) -> Result<Vec<Option<Value>>> {
        self.resolve_entities_from_db_enforced(
            typename,
            representations,
            selection,
            trace_context,
            None,
            &[],
        )
        .await
    }

    /// Resolve entities from database with an optional per-row enforcement filter
    /// and connection-affine session variables (Phase 03 C1b/R1 follow-up).
    ///
    /// This is the per-row-enforced counterpart of
    /// [`resolve_entities_from_db_with_tracing`](Self::resolve_entities_from_db_with_tracing).
    /// It closes the federation `_entities` per-row gap: the caller (the core
    /// runtime, which holds the `SecurityContext`) composes a `row_filter`
    /// predicate — a columnar `NativeField` equality such as `"tenant_id" = $N`
    /// derived from the backing query's `inject_params` (tenant/owner scoping) —
    /// and this resolver adds it to the key `IN` clause so a direct `_entities`
    /// hit with arbitrary ids is still row-filtered.
    ///
    /// * `row_filter` — an already-composed WHERE predicate to AND onto the key lookup. Its bind
    ///   placeholders are renumbered to start **after** the key `IN`-clause parameters, and its
    ///   parameters are appended in order, so the combined parameter vector stays positionally
    ///   aligned with the SQL.
    /// * `session_vars` — session variables applied transaction-locally on the aggregate's
    ///   connection (`current_setting()` DB-native RLS), mirroring the regular query path (#329).
    ///   An empty slice runs the plain aggregate.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Validation`] if the typename is not a safe SQL
    /// identifier; propagates rendering errors from the `row_filter`; returns
    /// [`FraiseQLError::Database`] if the underlying query fails.
    pub async fn resolve_entities_from_db_enforced(
        &self,
        typename: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
        _trace_context: Option<FederationTraceContext>,
        row_filter: Option<&WhereClause>,
        session_vars: &[(&str, &str)],
    ) -> Result<Vec<Option<Value>>> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // Validate typename is a safe SQL identifier before any SQL interpolation.
        // Defense-in-depth: find_federation_type already acts as a whitelist, but
        // we also reject names that would be unsafe if somehow they passed validation.
        if !is_safe_sql_identifier(typename) {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "Federation entity type '{}' contains unsafe characters for SQL interpolation",
                    typename
                ),
                path:    None,
            });
        }

        // Find type definition using metadata helpers (whitelist check)
        let fed_type = find_federation_type(typename, &self.metadata)?;

        // Resolve the backing relation. FraiseQL entities are view-backed, so the
        // FROM relation is the type's configured `sql_source` (e.g. `v_organization`),
        // not `lower(typename)` — which names a relation that does not exist and made
        // every view-backed cross-subgraph join return null (#504). Falls back to
        // `lower(typename)` only when no source map is attached (unit paths).
        let quoted_table = match self.entity_sources.get(typename) {
            Some(source) => quote_relation(source)?,
            None => quote_relation(&typename.to_lowercase())?,
        };

        // Build the parameterized WHERE IN clause. Key-field values are bound (not
        // interpolated), so the dialect must match the executing adapter.
        let db_type = self.adapter.database_type();
        let (where_clause, mut params) =
            construct_where_in_clause(typename, representations, &self.metadata, db_type)?;

        // Compose the per-row enforcement predicate (tenant/owner scoping) onto the
        // key lookup. Placeholders are offset past the IN-clause params, and the
        // filter's bound values are appended so positions stay aligned.
        let where_clause = match row_filter {
            Some(filter) => {
                let (fragment, mut filter_params) =
                    render_row_filter(filter, db_type, params.len())?;
                params.append(&mut filter_params);
                format!("({where_clause}) AND ({fragment})")
            },
            None => where_clause,
        };

        // Build the SELECT list. Each requested field must be a safe SQL identifier
        // AND exposed: `@inaccessible` / `@external` fields are never selected, so a
        // client naming them cannot exfiltrate them via `_entities` (M-fed-select-
        // list). The selection comes from a text scanner, so any token that is not a
        // plain identifier is dropped here too. Fields are interpolated unquoted (the
        // entity views rely on PostgreSQL case-folding); the charset guard is what
        // keeps that interpolation injection-safe.
        let select_fields = build_select_fields(selection, fed_type);

        let sql = format!(
            "SELECT {} FROM {} WHERE {}",
            select_fields.join(", "),
            quoted_table,
            where_clause
        );

        // Execute with bound parameters (no value interpolation), pinning session
        // variables to the read's connection so `current_setting()` RLS is effective.
        let rows = self
            .adapter
            .execute_parameterized_aggregate_with_session(&sql, &params, session_vars)
            .await?;

        // Project results maintaining order
        project_results(&rows, representations, fed_type, typename)
    }
}

/// Quote a (possibly schema-qualified) relation name for use as a `FROM` target.
///
/// Splits on `.` so a qualified `sql_source` like `app.v_organization` becomes
/// `"app"."v_organization"` rather than a single mis-quoted identifier. Each
/// segment is validated as a safe SQL identifier (defense-in-depth — `sql_source`
/// is compiler-authored, not client input) and quoted, doubling any inner quote.
/// Returns a [`FraiseQLError::Validation`] if a segment is empty or unsafe.
fn quote_relation(relation: &str) -> Result<String> {
    let segments: Vec<&str> = relation.split('.').collect();
    if segments.iter().any(|s| s.is_empty()) || !segments.iter().all(|s| is_safe_sql_identifier(s))
    {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Federation entity relation '{relation}' is not a valid (optionally \
                 schema-qualified) SQL identifier"
            ),
            path:    None,
        });
    }
    Ok(segments
        .iter()
        .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join("."))
}

/// Render an already-composed per-row WHERE predicate to a dialect-native SQL
/// fragment plus its bound parameters, numbering placeholders so they start
/// **after** `param_offset` existing parameters.
///
/// The predicate is built by the core runtime (which holds the `SecurityContext`)
/// as a columnar [`WhereClause::NativeField`] equality, so it renders to
/// `"column" = $N` (or a dialect cast). Reusing the audited
/// [`GenericWhereGenerator`] keeps cast/quoting/dialect handling consistent with
/// the regular query path rather than re-implementing SQL assembly here.
fn render_row_filter(
    filter: &WhereClause,
    db_type: DatabaseType,
    param_offset: usize,
) -> Result<(String, Vec<Value>)> {
    match db_type {
        DatabaseType::PostgreSQL => GenericWhereGenerator::new(PostgresDialect)
            .generate_with_param_offset(filter, param_offset),
        DatabaseType::MySQL => GenericWhereGenerator::new(MySqlDialect)
            .generate_with_param_offset(filter, param_offset),
        DatabaseType::SQLite => GenericWhereGenerator::new(SqliteDialect)
            .generate_with_param_offset(filter, param_offset),
        DatabaseType::SQLServer => GenericWhereGenerator::new(SqlServerDialect)
            .generate_with_param_offset(filter, param_offset),
    }
}

/// Build the validated, exposure-filtered SELECT field list for an `_entities`
/// query.
///
/// A requested field is kept only if it is a safe SQL identifier AND exposed:
/// `@inaccessible` (via either `field_directives` or the `inaccessible_fields`
/// list) and `@external` fields are dropped, so a client naming them cannot
/// exfiltrate them via `_entities` (M-fed-select-list). `__typename` and any
/// non-identifier scanner token are dropped too. The type's key fields are
/// always appended — they are schema-defined identifiers and `project_results`
/// needs them to match rows to representations.
fn build_select_fields(selection: &FieldSelection, fed_type: &FederatedType) -> Vec<String> {
    let mut select_fields: Vec<String> = Vec::new();
    for field in &selection.fields {
        if field == "__typename" {
            continue;
        }
        if !is_safe_sql_identifier(field)
            || fed_type.field_is_inaccessible(field)
            || fed_type.inaccessible_fields.iter().any(|f| f == field)
            || fed_type.external_fields.iter().any(|e| e == field)
        {
            continue;
        }
        if !select_fields.contains(field) {
            select_fields.push(field.clone());
        }
    }
    for key in &fed_type.keys {
        for field in &key.fields {
            if is_safe_sql_identifier(field) && !select_fields.contains(field) {
                select_fields.push(field.clone());
            }
        }
    }
    select_fields
}

/// Project database results to federation format, maintaining order of representations.
fn project_results(
    rows: &[std::collections::HashMap<String, Value>],
    representations: &[EntityRepresentation],
    fed_type: &FederatedType,
    typename: &str,
) -> Result<Vec<Option<Value>>> {
    use std::collections::HashMap as StdHashMap;

    // Build a map of key values -> row data for quick lookup
    // Key is constructed from the key fields of the federation type
    let mut row_map: StdHashMap<Vec<String>, StdHashMap<String, Value>> = StdHashMap::new();

    for row in rows {
        // Build key from key fields
        let key_values: Result<Vec<String>> = fed_type
            .keys
            .first()
            .ok_or_else(|| fraiseql_error::FraiseQLError::Validation {
                message: format!("Type '{}' has no key fields", typename),
                path:    None,
            })?
            .fields
            .iter()
            .map(|field| {
                row.get(field)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| row.get(field).map(|v| v.to_string()))
                    .ok_or_else(|| fraiseql_error::FraiseQLError::Validation {
                        message: format!("Key field '{}' not found in row", field),
                        path:    None,
                    })
            })
            .collect();

        if let Ok(key) = key_values {
            row_map.insert(key, row.clone());
        }
    }

    // Map representations to results, preserving order
    let mut results = Vec::new();
    for rep in representations {
        // Extract key values from representation
        let key_values: Vec<String> = fed_type
            .keys
            .first()
            .map(|k| {
                k.fields
                    .iter()
                    .filter_map(|field| {
                        rep.key_fields.get(field).and_then(|v| {
                            v.as_str().map(|s| s.to_string()).or_else(|| Some(v.to_string()))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Look up row in map
        if let Some(row) = row_map.get(&key_values) {
            let mut entity = row.clone();
            entity.insert("__typename".to_string(), Value::String(typename.to_string()));

            // Validate @requires/@provides directives are satisfied
            if let Err(validation_errors) =
                RequiresProvidesRuntimeValidator::validate_entity_against_type(
                    typename, &entity, fed_type,
                )
            {
                // Log validation errors but continue processing
                for error in validation_errors {
                    warn!("Federation directive validation error for {}: {}", typename, error);
                }
            }

            results.push(Some(Value::Object(serde_json::Map::from_iter(entity))));
        } else {
            // Entity not found in database
            results.push(None);
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests;
