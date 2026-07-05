//! Mutation execution runner.
//!
//! [`MutationRunner`] executes GraphQL mutations with compile-time capability enforcement.
//!
//! The [`execute_mutation_impl`] free function contains the core logic and is bounded only on
//! `A: DatabaseAdapter`, allowing it to be called from both the compile-time-checked
//! [`MutationRunner`] path and the runtime-guarded `execute_mutation_query` path on
//! [`Executor`](super::super::core::Executor).

use std::sync::Arc;

use fraiseql_db::{
    ChangeLogWrite, DirectMutationContext, DirectMutationOp, MutationStrategy, ViewName,
};

use super::{
    super::{context::ExecutorContext, resolve_inject_value},
    query_projection::selections_contain_field,
};
use crate::{
    db::traits::{DatabaseAdapter, SupportsMutations},
    error::{FraiseQLError, Result},
    graphql::{DirectiveEvaluator, FieldSelection},
    runtime::{
        ResultProjector,
        mutation_result::{MutationOutcome, parse_mutation_row},
        project_entity,
        projection::effective_selections,
        suggest_similar,
    },
    schema::{CompiledSchema, InputStyle, MutationOperation, NamingConvention},
    security::SecurityContext,
};

/// Enforce the dynamic field authorizer (#423) on a projected mutation payload.
///
/// `entity` is the full projected-from value (the `parent`); `projected` is the
/// response object, mutated in place. Fail-closed:
/// - a gated field selected with no authenticated principal → 403,
/// - a gated field selected with no authorizer configured → 403,
/// - a gated field nested in a sub-selection → 403 (top-level enforced in v1),
/// - a `Reject` decision or any policy error → 403.
///
/// No-op (and zero authorizer calls) when the selection set has no gated field.
fn enforce_mutation_field_authz<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    security_ctx: Option<&SecurityContext>,
    type_name: &str,
    selections: &[FieldSelection],
    entity: &serde_json::Value,
    projected: &mut serde_json::Value,
) -> Result<()> {
    use crate::security::field_authorizer as authz;

    if !authz::selection_set_selects_gated_field(&ctx.schema, type_name, selections) {
        return Ok(());
    }
    let Some(principal) = security_ctx else {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is required for a selected field on type \
                 '{type_name}' but the request is not authenticated"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    };
    let Some(authorizer) = ctx.config.field_authorizer.as_ref() else {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization is required for a selected field on type \
                 '{type_name}' but no field authorizer is configured"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    };
    if authz::selection_set_has_nested_gated_field(&ctx.schema, type_name, selections) {
        return Err(FraiseQLError::Authorization {
            message:  format!(
                "Field-level authorization of nested fields on type '{type_name}' is not \
                 supported in this version"
            ),
            action:   Some("read".to_string()),
            resource: Some(type_name.to_string()),
        });
    }
    let gated = authz::collect_top_level_gated_fields(&ctx.schema, type_name, selections);
    let pass = authz::FieldAuthzPass {
        authorizer: authorizer.as_ref(),
        principal,
        type_name,
        gated: &gated,
        // The mutation path has no static requires_scope gate, so nothing is pre-masked.
        statically_masked: &[],
    };
    authz::apply_field_authorizer_to_entity(&pass, entity, projected)
}

// ── Typed cascade payload projection (Phase 03: findings 1 + 5) ───────────────
//
// A `cascade = true` mutation returns a synthesized `<Name>Payload { entity,
// cascade, updatedFields }` (see `cli::converter::cascade_types`). The DB function
// emits the spec-nested cascade shape — `{ updated: [{ __typename, id, operation,
// entity }], deleted: [{ __typename, id, deletedAt }] }` — and the runtime builds
// the payload filtered to the client's selection set, projecting (camelCase) and
// field-authorizing the primary entity AND every cascade `entity` exactly like a
// queried entity. This is the load-bearing security fix: cascade entities no
// longer bypass the field authorizer and projection.

/// The synthesized cascade type names (mirror `cli::converter::cascade_types`).
const CASCADE_UPDATES_TYPE: &str = "CascadeUpdates";
const CASCADE_METADATA_TYPE: &str = "CascadeMetadata";
const UPDATED_ENTITY_TYPE: &str = "UpdatedEntity";
const DELETED_ENTITY_TYPE: &str = "DeletedEntity";

/// One mebibyte, for the `max_response_size_mb` cascade ceiling.
const MIB: usize = 1024 * 1024;

/// Resolve a cascade mutation's concrete payload type: the first non-error member
/// of the result union, or the return type itself when it is not a union.
fn resolve_payload_type(return_type: &str, schema: &CompiledSchema) -> String {
    schema
        .find_union(return_type)
        .and_then(|u| {
            u.member_types
                .iter()
                .find(|t| schema.find_type(t).is_none_or(|td| !td.is_error))
                .cloned()
        })
        .unwrap_or_else(|| return_type.to_string())
}

/// The concrete entity type a payload wraps, read from its `entity` field type.
fn payload_entity_type(payload_type: &str, schema: &CompiledSchema) -> Option<String> {
    schema
        .find_type(payload_type)?
        .fields
        .iter()
        .find(|f| f.name.as_str() == "entity")?
        .field_type
        .type_name()
        .map(std::string::ToString::to_string)
}

/// Build the typed cascade payload `{ entity, cascade, updatedFields }`, filtered
/// to the client's selection set. Projects + field-authorizes the primary entity
/// and delegates the cascade envelope to [`build_cascade_updates`].
fn build_cascade_payload<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    security_ctx: Option<&SecurityContext>,
    payload_type: &str,
    entity_type: &str,
    entity: &serde_json::Value,
    cascade: Option<&serde_json::Value>,
    updated_fields: &[String],
    selections: &[FieldSelection],
) -> Result<serde_json::Value> {
    let mut out = serde_json::Map::new();
    for sel in effective_selections(selections, payload_type, &ctx.schema) {
        match sel.name.as_str() {
            "__typename" => {
                out.insert(
                    sel.response_key().to_string(),
                    serde_json::Value::String(payload_type.to_string()),
                );
            },
            "entity" => {
                let mut projected =
                    project_entity(entity, entity_type, &sel.nested_fields, &ctx.schema);
                enforce_mutation_field_authz(
                    ctx,
                    security_ctx,
                    entity_type,
                    &sel.nested_fields,
                    entity,
                    &mut projected,
                )?;
                out.insert(sel.response_key().to_string(), projected);
            },
            "cascade" => {
                let built = build_cascade_updates(ctx, security_ctx, cascade, &sel.nested_fields)?;
                out.insert(sel.response_key().to_string(), built);
            },
            "updatedFields" => {
                out.insert(
                    sel.response_key().to_string(),
                    serde_json::Value::Array(
                        updated_fields.iter().cloned().map(serde_json::Value::String).collect(),
                    ),
                );
            },
            _ => {},
        }
    }
    Ok(serde_json::Value::Object(out))
}

/// Build the `CascadeUpdates` envelope (`updated` / `deleted` / `metadata`),
/// filtered to the client's selection set. Enforces the affected-entity ceiling
/// (truncating + flagging `metadata.truncated`) and the response-size ceiling
/// (rejecting an over-large cascade), per graphql-cascade `16_security`.
/// `invalidations` lands in Phase 05.
fn build_cascade_updates<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    security_ctx: Option<&SecurityContext>,
    cascade: Option<&serde_json::Value>,
    selections: &[FieldSelection],
) -> Result<serde_json::Value> {
    let cascade_obj = cascade.and_then(serde_json::Value::as_object);
    let empty: Vec<serde_json::Value> = Vec::new();
    let updated: &[serde_json::Value] = cascade_obj
        .and_then(|c| c.get("updated"))
        .and_then(serde_json::Value::as_array)
        .map_or(empty.as_slice(), Vec::as_slice);
    let deleted: &[serde_json::Value] = cascade_obj
        .and_then(|c| c.get("deleted"))
        .and_then(serde_json::Value::as_array)
        .map_or(empty.as_slice(), Vec::as_slice);

    // Affected-entity ceiling: truncate `updated`/`deleted` each to half the limit
    // and record it in metadata (spec `16_security` size-limiting).
    let limits = ctx.config.cascade_limits;
    let total = updated.len() + deleted.len();
    let truncated = total > limits.max_updated_entities;
    let (updated, deleted) = if truncated {
        let half = limits.max_updated_entities / 2;
        (&updated[..updated.len().min(half)], &deleted[..deleted.len().min(half)])
    } else {
        (updated, deleted)
    };
    let affected_count = updated.len() + deleted.len();

    let mut out = serde_json::Map::new();
    for sel in effective_selections(selections, CASCADE_UPDATES_TYPE, &ctx.schema) {
        match sel.name.as_str() {
            "__typename" => {
                out.insert(
                    sel.response_key().to_string(),
                    serde_json::Value::String(CASCADE_UPDATES_TYPE.to_string()),
                );
            },
            "updated" => {
                let arr = build_updated_entities(ctx, security_ctx, updated, &sel.nested_fields)?;
                out.insert(sel.response_key().to_string(), arr);
            },
            "deleted" => {
                let arr = build_deleted_entities(deleted, &sel.nested_fields, &ctx.schema)?;
                out.insert(sel.response_key().to_string(), arr);
            },
            "metadata" => {
                let meta = build_cascade_metadata(
                    &ctx.schema,
                    cascade_obj.and_then(|c| c.get("metadata")),
                    affected_count,
                    truncated,
                    truncated.then_some(total),
                    &sel.nested_fields,
                );
                out.insert(sel.response_key().to_string(), meta);
            },
            _ => {},
        }
    }
    let cascade_value = serde_json::Value::Object(out);

    // Response-size ceiling: reject an over-large cascade (spec `16_security`).
    let max_bytes = limits.max_response_size_mb.saturating_mul(MIB);
    if max_bytes > 0 {
        let size = serde_json::to_vec(&cascade_value).map_or(0, |v| v.len());
        if size > max_bytes {
            return Err(FraiseQLError::Validation {
                message: format!(
                    "cascade response too large: {size} bytes exceeds the \
                     max_response_size_mb={} ceiling",
                    limits.max_response_size_mb
                ),
                path:    Some("cascade".to_string()),
            });
        }
    }
    Ok(cascade_value)
}

/// Build the `CascadeMetadata` object, filtered to the client's selection set.
/// The runtime owns `affectedCount` / `truncated` / `originalCount` (computed from
/// what it actually returns); `timestamp` / `transactionId` / `depth` pass through
/// from the function's `cascade.metadata` (`depth` defaults to 0 if absent).
fn build_cascade_metadata(
    schema: &CompiledSchema,
    db_metadata: Option<&serde_json::Value>,
    affected_count: usize,
    truncated: bool,
    original_count: Option<usize>,
    selections: &[FieldSelection],
) -> serde_json::Value {
    let db = db_metadata.and_then(serde_json::Value::as_object);
    let mut out = serde_json::Map::new();
    for sel in effective_selections(selections, CASCADE_METADATA_TYPE, schema) {
        let key = sel.response_key().to_string();
        match sel.name.as_str() {
            "__typename" => {
                out.insert(key, serde_json::Value::String(CASCADE_METADATA_TYPE.to_string()));
            },
            "affectedCount" => {
                out.insert(key, serde_json::Value::from(affected_count as u64));
            },
            "truncated" => {
                out.insert(key, serde_json::Value::Bool(truncated));
            },
            "originalCount" => {
                if let Some(oc) = original_count {
                    out.insert(key, serde_json::Value::from(oc as u64));
                }
            },
            "depth" => {
                let depth = db
                    .and_then(|m| m.get("depth"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::from(0u64));
                out.insert(key, depth);
            },
            "timestamp" => {
                if let Some(v) = db.and_then(|m| m.get("timestamp")) {
                    out.insert(key, v.clone());
                }
            },
            "transactionId" => {
                if let Some(v) =
                    db.and_then(|m| m.get("transactionId").or_else(|| m.get("transaction_id")))
                {
                    out.insert(key, v.clone());
                }
            },
            _ => {},
        }
    }
    serde_json::Value::Object(out)
}

/// Build the `updated: [UpdatedEntity!]!` array, projecting + field-authorizing
/// each entry's `entity` under its concrete `__typename`.
///
/// Fail-closed on any malformed entry (Phase 04 strict validation): a missing or
/// unknown `__typename`, a missing `id`, or a missing/invalid `operation` aborts
/// the response rather than shipping an unprojectable entity or an SDL-invalid
/// non-null violation — regardless of what the client selected.
fn build_updated_entities<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    security_ctx: Option<&SecurityContext>,
    entries: &[serde_json::Value],
    selections: &[FieldSelection],
) -> Result<serde_json::Value> {
    let effective = effective_selections(selections, UPDATED_ENTITY_TYPE, &ctx.schema);
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(entry_obj) = entry.as_object() else {
            return Err(FraiseQLError::Validation {
                message: "cascade.updated entry is not an object".to_string(),
                path:    Some("cascade.updated".to_string()),
            });
        };
        let Some(typename) = entry_obj.get("__typename").and_then(serde_json::Value::as_str) else {
            return Err(FraiseQLError::Validation {
                message: "cascade.updated entry is missing __typename".to_string(),
                path:    Some("cascade.updated.__typename".to_string()),
            });
        };
        // Fail-closed on an unknown type — we cannot project or authorize it.
        if ctx.schema.find_type(typename).is_none() {
            return Err(FraiseQLError::Validation {
                message: format!("cascade.updated entry has unknown __typename '{typename}'"),
                path:    Some("cascade.updated.__typename".to_string()),
            });
        }
        // Non-null `id: ID!` and a valid `operation: CascadeOperation!` are required.
        if !entry_obj.contains_key("id") {
            return Err(FraiseQLError::Validation {
                message: "cascade.updated entry is missing id".to_string(),
                path:    Some("cascade.updated.id".to_string()),
            });
        }
        match entry_obj.get("operation").and_then(serde_json::Value::as_str) {
            Some("CREATED" | "UPDATED" | "DELETED") => {},
            other => {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "cascade.updated entry has a missing or invalid operation: {other:?} \
                         (expected CREATED, UPDATED, or DELETED)"
                    ),
                    path: Some("cascade.updated.operation".to_string()),
                });
            },
        }
        let mut item = serde_json::Map::new();
        for sel in &effective {
            match sel.name.as_str() {
                "__typename" => {
                    item.insert(
                        sel.response_key().to_string(),
                        serde_json::Value::String(UPDATED_ENTITY_TYPE.to_string()),
                    );
                },
                "id" => {
                    if let Some(id) = entry_obj.get("id") {
                        item.insert(sel.response_key().to_string(), id.clone());
                    }
                },
                "operation" => {
                    if let Some(op) = entry_obj.get("operation") {
                        item.insert(sel.response_key().to_string(), op.clone());
                    }
                },
                "entity" => {
                    let entity_blob =
                        entry_obj.get("entity").cloned().unwrap_or(serde_json::Value::Null);
                    let mut projected =
                        project_entity(&entity_blob, typename, &sel.nested_fields, &ctx.schema);
                    enforce_mutation_field_authz(
                        ctx,
                        security_ctx,
                        typename,
                        &sel.nested_fields,
                        &entity_blob,
                        &mut projected,
                    )?;
                    item.insert(sel.response_key().to_string(), projected);
                },
                _ => {},
            }
        }
        result.push(serde_json::Value::Object(item));
    }
    Ok(serde_json::Value::Array(result))
}

/// Build the `deleted: [DeletedEntity!]!` array. Deleted entries carry no entity
/// body (the row is gone) — only `id` + `deletedAt` — so there is nothing to
/// project or field-authorize.
///
/// Fail-closed (Phase 04 strict validation): an entry missing the non-null `id` or
/// `deletedAt` aborts the response rather than emitting an SDL-invalid violation.
fn build_deleted_entities(
    entries: &[serde_json::Value],
    selections: &[FieldSelection],
    schema: &CompiledSchema,
) -> Result<serde_json::Value> {
    let effective = effective_selections(selections, DELETED_ENTITY_TYPE, schema);
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(entry_obj) = entry.as_object() else {
            return Err(FraiseQLError::Validation {
                message: "cascade.deleted entry is not an object".to_string(),
                path:    Some("cascade.deleted".to_string()),
            });
        };
        if !entry_obj.contains_key("id") {
            return Err(FraiseQLError::Validation {
                message: "cascade.deleted entry is missing id".to_string(),
                path:    Some("cascade.deleted.id".to_string()),
            });
        }
        let deleted_at = entry_obj.get("deletedAt").or_else(|| entry_obj.get("deleted_at"));
        let Some(deleted_at) = deleted_at else {
            return Err(FraiseQLError::Validation {
                message: "cascade.deleted entry is missing deletedAt".to_string(),
                path:    Some("cascade.deleted.deletedAt".to_string()),
            });
        };
        let mut item = serde_json::Map::new();
        for sel in &effective {
            match sel.name.as_str() {
                "__typename" => {
                    item.insert(
                        sel.response_key().to_string(),
                        serde_json::Value::String(DELETED_ENTITY_TYPE.to_string()),
                    );
                },
                "id" => {
                    if let Some(id) = entry_obj.get("id") {
                        item.insert(sel.response_key().to_string(), id.clone());
                    }
                },
                "deletedAt" => {
                    item.insert(sel.response_key().to_string(), deleted_at.clone());
                },
                _ => {},
            }
        }
        result.push(serde_json::Value::Object(item));
    }
    Ok(serde_json::Value::Array(result))
}

/// Executes GraphQL mutations with compile-time capability enforcement.
///
/// Only constructible when `A: SupportsMutations`. This means calling mutation
/// methods on an executor backed by `SqliteAdapter` (which does not implement
/// `SupportsMutations`) is a compiler error, not a runtime failure.
pub(in super::super) struct MutationRunner<A: DatabaseAdapter + SupportsMutations> {
    ctx: Arc<ExecutorContext<A>>,
}

impl<A: DatabaseAdapter + SupportsMutations> MutationRunner<A> {
    /// Create a new `MutationRunner` from a shared executor context.
    ///
    /// Zero-cost: `Arc` is already shared — this is just a newtype wrapper.
    pub(in super::super) const fn new(ctx: Arc<ExecutorContext<A>>) -> Self {
        Self { ctx }
    }

    /// Execute a GraphQL mutation with compile-time [`SupportsMutations`] enforcement.
    ///
    /// # Errors
    ///
    /// Same as [`execute_mutation_impl`].
    pub(in super::super) async fn execute_mutation(
        &self,
        mutation_name: &str,
        variables: Option<&serde_json::Value>,
        selections: &[FieldSelection],
    ) -> Result<serde_json::Value> {
        // The typed SupportsMutations API supplies the input via `variables`; it
        // has no inline-literal root arguments to resolve.
        execute_mutation_impl(&self.ctx, mutation_name, variables, None, selections, &[]).await
    }
}

/// Re-case a mutation input payload's keys from the GraphQL surface naming
/// convention to canonical `snake_case`, recursing into nested input objects and
/// arrays of input objects.
///
/// The compiled schema stores field names in their GraphQL *surface* form: SDKs
/// pre-case them (the Python SDK camelCases at `registry.py:233`; the
/// `TypeScript` SDK stores whatever the author wrote, idiomatically
/// `camelCase`), and the
/// introspection layer renders them verbatim. With
/// [`NamingConvention::CamelCase`] a client therefore sends `camelCase` keys. The
/// Insert path maps those to positional SQL args by name (casing handled
/// implicitly), but the Update / `input_style = jsonb` path forwards the whole
/// object as one JSONB arg — so without this the SQL function receives
/// surface-cased keys it cannot read (#400, #456).
///
/// The input type's per-field map is used to *match* the incoming surface key and
/// to drive recursion into nested input types; every emitted canonical key is
/// then normalised with the engine's acronym-aware
/// [`to_snake_case`](crate::utils::to_snake_case) so writes round-trip exactly as
/// reads do (`s3Key → s3_key`, `dns1Id → dns_1_id`) — the same transform the
/// fallback below uses, regardless of which SDK authored the schema (#456).
///
/// When no per-field map is available — `input_type_name` is `None` (a raw `JSON`
/// input arg) or names a type absent from the compiled schema — it falls back to
/// the same key transform ([`recase_jsonb_keys_to_snake`]) so a single-JSONB
/// payload still reaches the function as `snake_case` (#400). A
/// [`NamingConvention::Preserve`] schema is always left untouched.
fn recase_input_payload(
    value: serde_json::Value,
    input_type_name: Option<&str>,
    schema: &CompiledSchema,
) -> serde_json::Value {
    // Preserve convention: the GraphQL surface already uses the canonical names.
    if schema.naming_convention != NamingConvention::CamelCase {
        return value;
    }
    // No per-field name map (raw `JSON` arg, or an Input type missing from the
    // compiled schema): recase the keys directly with the canonical reverse so the
    // single-JSONB path is not left forwarding verbatim camelCase keys (#400).
    let Some(input_type) = input_type_name.and_then(|n| schema.find_input_type(n)) else {
        return recase_jsonb_keys_to_snake(value);
    };
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                // Match the incoming surface key against each field's surface name,
                // then emit the canonical key in `snake_case`. The stored field
                // name is the GraphQL *surface* name (SDKs pre-case it — e.g. the
                // Python SDK camelCases at `registry.py:233`), so it cannot be
                // forwarded verbatim; normalising with the engine's acronym-aware
                // `to_snake_case` — the same transform the raw-`JSON` fallback and
                // the read path use — is what reaches the function as `snake_case`
                // regardless of which SDK authored the schema (#456 / #400).
                let field = input_type.fields.iter().find(|f| schema.display_name(&f.name) == key);
                let canonical = field.map_or_else(
                    || crate::utils::to_snake_case(&key),
                    |f| crate::utils::to_snake_case(&f.name),
                );
                let recased = match field {
                    Some(f) => recase_input_field_value(val, &f.field_type, schema),
                    None => val,
                };
                out.insert(canonical, recased);
            }
            serde_json::Value::Object(out)
        },
        other => other,
    }
}

/// Recase one input field's *value*: when `field_type` names a nested input
/// object (or a list of them), recurse into the object's keys; scalars, enums,
/// free-form JSON, and lists of scalars are returned untouched.
///
/// Shared by the Update path (the whole payload is one JSONB arg) and the Insert
/// path (each composite field is one positional JSONB arg), so both recase nested
/// composite keys identically — without it, a `jsonb_populate_record(NULL::config,
/// $arg)` on the Insert path sees the surface-cased keys it cannot read (#400).
fn recase_input_field_value(
    value: serde_json::Value,
    field_type: &str,
    schema: &CompiledSchema,
) -> serde_json::Value {
    let Some(nested) = nested_input_type_name(field_type, schema) else {
        return value;
    };
    match value {
        serde_json::Value::Object(_) => recase_input_payload(value, Some(nested.as_str()), schema),
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(|it| match it {
                    serde_json::Value::Object(_) => {
                        recase_input_payload(it, Some(nested.as_str()), schema)
                    },
                    other => other,
                })
                .collect(),
        ),
        other => other,
    }
}

/// Recase every object key of a single-JSONB input payload to canonical
/// `snake_case` with the engine's one canonical acronym-aware reverse
/// ([`to_snake_case`](crate::utils::to_snake_case)), recursing into nested objects
/// and arrays; scalar values are untouched.
///
/// The fallback for [`recase_input_payload`] when no registered Input type
/// supplies a per-field name map — a custom `mutation(input: JSON)`, or an Update
/// whose Input type is absent from the compiled schema. Sharing `to_snake_case`
/// with the read path makes write keys round-trip exactly as reads do: `dns1Id` →
/// `dns_1_id` (digit split), `s3Key` → `s3_key` (registered acronym kept whole).
fn recase_jsonb_keys_to_snake(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, val) in map {
                out.insert(crate::utils::to_snake_case(&key), recase_jsonb_keys_to_snake(val));
            }
            serde_json::Value::Object(out)
        },
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(recase_jsonb_keys_to_snake).collect())
        },
        other => other,
    }
}

/// If `field_type` (e.g. `"BillingAddressInput"`, `"[TagInput!]"`) names a known
/// input object type, return that type's bare name (`[`, `]`, `!` stripped) so
/// [`recase_input_payload`] can recurse; otherwise `None` (scalars, enums, and
/// unknown types are left untouched).
fn nested_input_type_name(field_type: &str, schema: &CompiledSchema) -> Option<String> {
    let base = field_type.trim_matches(|c| c == '[' || c == ']' || c == '!');
    schema.find_input_type(base).map(|_| base.to_string())
}

/// Core mutation execution logic, bounded only on `A: DatabaseAdapter`.
///
/// Called from:
/// - [`MutationRunner::execute_mutation`] — compile-time [`SupportsMutations`] path
/// - `Executor::execute_mutation_query` — runtime-guarded path (raw GraphQL dispatch)
/// - `execute_with_security_internal` — authenticated GraphQL dispatch
///
/// The caller is responsible for ensuring the adapter supports mutations before calling
/// this function (either via the compile-time `SupportsMutations` bound or a runtime
/// `supports_mutations()` guard).
///
/// # Errors
///
/// * [`FraiseQLError::Validation`] — mutation not found, no `sql_source`, missing security context
///   for `inject` params, or database function returned no rows.
/// * [`FraiseQLError::Database`] — the adapter's `execute_function_call` failed.
pub(in super::super) async fn execute_mutation_impl<A: DatabaseAdapter>(
    ctx: &ExecutorContext<A>,
    mutation_name: &str,
    variables: Option<&serde_json::Value>,
    security_ctx: Option<&SecurityContext>,
    selections: &[FieldSelection],
    inline_arguments: &[crate::graphql::GraphQLArgument],
) -> Result<serde_json::Value> {
    // 1. Locate the mutation definition
    let mutation_def = ctx.schema.find_mutation(mutation_name).ok_or_else(|| {
        let display_names: Vec<String> =
            ctx.schema.mutations.iter().map(|m| ctx.schema.display_name(&m.name)).collect();
        let candidate_refs: Vec<&str> = display_names.iter().map(String::as_str).collect();
        let suggestion = suggest_similar(mutation_name, &candidate_refs);
        let message = match suggestion.as_slice() {
            [s] => {
                format!("Mutation '{mutation_name}' not found in schema. Did you mean '{s}'?")
            },
            [a, b] => format!(
                "Mutation '{mutation_name}' not found in schema. Did you mean '{a}' or '{b}'?"
            ),
            [a, b, c, ..] => format!(
                "Mutation '{mutation_name}' not found in schema. Did you mean '{a}', '{b}', or \
                 '{c}'?"
            ),
            _ => format!("Mutation '{mutation_name}' not found in schema"),
        };
        FraiseQLError::Validation {
            message,
            path: None,
        }
    })?;

    // 1a. Operation-level authorization (#422): the universal mutation chokepoint.
    //     EVERY mutation entry path converges here — the two `*_internal` GraphQL
    //     branches, `execute_mutation_query`, and the direct `SupportsMutations` API
    //     used by the anonymous-REST write path (which bypasses both chokepoints).
    //     Runs after `find_mutation` so an unknown name keeps its "not found" message
    //     (no enumeration leak), and before `requires_role` (AND-composition).
    //     Fail-closed: a `Deny` or any policy error returns 403.
    if let Some(authorizer) = ctx.config.authorizer.as_ref() {
        let ops =
            [(crate::security::authorizer::OperationKind::Mutation, mutation_name.to_string())];
        crate::security::authorizer::enforce_authz(
            authorizer.as_ref(),
            security_ctx,
            &ops,
            variables,
        )?;
    }

    // 1b. Enforce requires_role — return "not found" (not "forbidden") to prevent
    //     enumeration, mirroring the query-level check in query_regular.rs.
    if let Some(required_role) = mutation_def.requires_role.as_deref() {
        let has_role = security_ctx.is_some_and(|c| c.roles.iter().any(|r| r == required_role));
        if !has_role {
            return Err(FraiseQLError::Validation {
                message: format!("Mutation '{mutation_name}' not found in schema"),
                path:    None,
            });
        }
    }

    // 2. Require a sql_source (PostgreSQL function name).
    //
    // Fall back to the operation's table field when sql_source is absent.
    // The CLI compiler stores the SQL function name in both places
    // (sql_source and operation.{Insert|Update|Delete}.table), but older or
    // alternate compilation paths (e.g. fraiseql-core's own codegen) may only
    // populate operation.table and leave sql_source as None.
    let sql_source_owned: String;
    let sql_source: &str = if let Some(src) = mutation_def.sql_source.as_deref() {
        src
    } else {
        match &mutation_def.operation {
            MutationOperation::Insert { table }
            | MutationOperation::Update { table }
            | MutationOperation::Delete { table }
                if !table.is_empty() =>
            {
                sql_source_owned = table.clone();
                &sql_source_owned
            },
            _ => {
                return Err(FraiseQLError::Validation {
                    message: format!("Mutation '{mutation_name}' has no sql_source configured"),
                    path:    None,
                });
            },
        }
    };

    // 3. Build positional args Vec from variables in ArgumentDefinition order. Validate that every
    //    required (non-nullable, no default) argument is present.
    //
    //    Input object unwrapping: when the mutation has a single argument named "input"
    //    whose type is an Input type, AND the client sends a JSON object for that argument,
    //    unwrap the object's fields and pass them positionally in the order defined by the
    //    input type's field list.  This keeps the SQL function signature flat while letting
    //    the GraphQL API use the standard input object pattern.
    //
    //    Merge the root field's inline arguments (e.g. `createMachine(input: { ... })`)
    //    into the variables view first, so an inline-literal input — including one whose
    //    fields reference nested `$var`s — is visible below. This mirrors the query path
    //    (QueryMatcher merges inline args; whole-arg request variables take precedence).
    //    `merged_variables` owns the value the shadowed `variables` borrows for the rest
    //    of the function; it is `Some` only when an inline arg added a new key.
    let merged_variables: Option<serde_json::Value> = if inline_arguments.is_empty() {
        None
    } else {
        let var_map = crate::runtime::matcher::QueryMatcher::extract_arguments(variables);
        let mut obj = match variables {
            Some(serde_json::Value::Object(m)) => m.clone(),
            _ => serde_json::Map::new(),
        };
        let mut added = false;
        for arg in inline_arguments {
            if !obj.contains_key(&arg.name) {
                if let Some(val) =
                    crate::runtime::matcher::QueryMatcher::resolve_inline_arg(arg, &var_map)
                {
                    obj.insert(arg.name.clone(), val);
                    added = true;
                }
            }
        }
        added.then_some(serde_json::Value::Object(obj))
    };
    let variables = merged_variables.as_ref().or(variables);

    let vars_obj = variables.and_then(|v| v.as_object());

    let mut missing_required: Vec<&str> = Vec::new();
    let total_args = mutation_def.arguments.len() + mutation_def.inject_params.len();
    let mut args: Vec<serde_json::Value> = Vec::with_capacity(total_args);

    // Column names parallel to `args`, populated alongside the value pushes below and
    // consumed only by the DirectSql (e.g. SQLite) strategy to build INSERT/DELETE
    // column lists. Left empty on the single-JSONB path, which DirectSql rejects.
    let mut direct_columns: Vec<String> = Vec::new();
    let mut direct_inject_columns: Vec<String> = Vec::new();

    // Detect the single-`input`-object pattern: exactly one argument named "input".
    // `input_type_name` is its declared Input type when it has one (vs a raw `JSON`
    // scalar — a custom `mutation(input: JSON)` whose SQL function takes
    // `(input jsonb, …)`); it drives the field-level flatten/recase path below.
    let single_input_arg =
        mutation_def.arguments.len() == 1 && mutation_def.arguments[0].name == "input";
    // The compiler emits an input-type reference as `FieldType::Object(name)`, never
    // `FieldType::Input` (the converter's `parse_field_type` has no `Input` variant),
    // so an `Object` whose name resolves to a registered input type IS a structured
    // input arg. Without recognising it here the arg matches neither the
    // single-JSONB nor the flatten branch and falls through to the verbatim
    // standard-arg path — forwarding camelCase keys to the SQL function with no
    // recasing, regardless of `naming_convention` (#456). The `find_input_type`
    // guard is exact: only names registered as input types match, so output-object
    // args are never misclassified.
    let input_type_name = if single_input_arg {
        match &mutation_def.arguments[0].arg_type {
            crate::schema::FieldType::Input(name) => Some(name.as_str()),
            crate::schema::FieldType::Object(name)
                if ctx.schema.find_input_type(name).is_some() =>
            {
                Some(name.as_str())
            },
            _ => None,
        }
    } else {
        None
    };
    // A single `input` arg shaped as a JSON payload — a structured Input type
    // (`FieldType::Input`, or the `Object` the compiler actually emits for one) or a
    // raw `JSON` scalar — as opposed to a plain scalar (`input: String`), which is a
    // positional arg, not a JSONB blob.
    let input_arg_is_structured = single_input_arg
        && match &mutation_def.arguments[0].arg_type {
            crate::schema::FieldType::Input(_) | crate::schema::FieldType::Json => true,
            crate::schema::FieldType::Object(name) => ctx.schema.find_input_type(name).is_some(),
            _ => false,
        };

    // Update mutations pass the entire input object as a single JSONB arg, which
    // preserves all three field states that typed positional args cannot express:
    //   - key absent            → leave the database value unchanged
    //   - key present, null     → SET field = NULL
    //   - key present, value    → SET field = <value>
    // SQL update functions use `input_payload ? 'field'` to test key presence.
    //
    // Insert / Delete / Custom flatten the Input type fields to positional args as
    // before (no three-state problem: absent ≡ NULL for creates; deletes need only
    // the PK).
    let is_update = matches!(&mutation_def.operation, MutationOperation::Update { .. });

    // An explicit `input_style = jsonb` opt-in (orthogonal to the DML verb):
    // forward the whole input as one JSONB arg regardless of the operation, so a
    // backend using the single-`jsonb`-wrapper convention can register the real
    // verb (Insert/Delete/Custom) instead of being forced to Update purely to opt
    // into single-JSONB passing — letting the Change Spine record the true
    // `modification_type` (#400 / `input_style`).
    let jsonb_input_style = matches!(mutation_def.input_style, InputStyle::Jsonb);

    // The whole `input` object is forwarded as ONE JSONB arg — never flattened to
    // positional columns — when the operation is Update (three-state semantics
    // above), the mutation opts in via `input_style = jsonb`, OR the structured
    // `input` arg is not a known Input type (a custom `mutation(input: JSON)`, or
    // an Update whose Input type is absent from the compiled schema). On that path
    // the keys must be recased from the camelCase GraphQL surface to canonical
    // snake_case so the function can read them — field-driven when the Input type
    // is known (preserves intentional names), acronym-aware key-driven
    // `to_snake_case` otherwise (#400). The flatten path below gets recasing for
    // free via positional args + recase_input_field_value.
    let known_input_type = input_type_name.and_then(|n| ctx.schema.find_input_type(n)).is_some();
    let pass_input_as_single_jsonb =
        input_arg_is_structured && (is_update || jsonb_input_style || !known_input_type);

    if pass_input_as_single_jsonb {
        // Forward the whole `input` value as ONE JSONB arg, re-cased from the
        // GraphQL surface naming convention to the schema's canonical (stored)
        // field names so the SQL function — which reads `payload->>'snake_field'`
        // / jsonb_populate_record — sees the values (#400). The Insert path below
        // gets this for free via positional args; this path must do it explicitly.
        //
        // Usually an object (an Input type or `input: JSON`); a raw `input: JSON`
        // arg may also be an array or scalar, so recase recurses into objects and
        // lists and leaves scalars untouched. An absent or explicit-null value on a
        // non-null no-default arg is a missing required argument; otherwise the
        // arg's default (or SQL NULL) is forwarded so the function keeps its arity.
        let arg = &mutation_def.arguments[0];
        match vars_obj.and_then(|obj| obj.get("input")) {
            Some(value) if !value.is_null() => {
                args.push(recase_input_payload(value.clone(), input_type_name, &ctx.schema));
            },
            _ if !arg.nullable && arg.default_value.is_none() => missing_required.push("input"),
            _ => args
                .push(arg.default_value.as_ref().map_or(serde_json::Value::Null, |v| v.to_json())),
        }
    } else if let Some(input_type) = input_type_name.and_then(|n| ctx.schema.find_input_type(n)) {
        // Insert / Delete / Custom: flatten Input type fields to positional typed args.
        let input_obj = vars_obj.and_then(|obj| obj.get("input")).and_then(|v| v.as_object());
        if let Some(input_obj) = input_obj {
            // #414: enforce required (non-null, no-default) input fields before the
            // database call, rejecting an omitted-or-explicit-null required field
            // with a GraphQL validation error instead of passing SQL NULL through.
            //
            // Look up each field by its GraphQL surface name (`display_name`):
            // under `NamingConvention::CamelCase` the client sends camelCase keys
            // and `field.name` is itself the surface name (`display_name` is then a
            // no-op), so matching on it makes the required check correct and finds
            // the value to forward. The canonical column name handed to DirectSql is
            // re-derived as `snake_case` below — see `recase_input_payload` (#456).
            let mut missing_input_fields: Vec<&str> = Vec::new();
            for field in &input_type.fields {
                let key = ctx.schema.display_name(&field.name);
                let value = input_obj.get(&key);
                if field.is_required() && value.is_none_or(serde_json::Value::is_null) {
                    missing_input_fields.push(field.name.as_str());
                }
                // Top-level keys map to columns positionally (casing implicit), but a
                // field whose type is a nested input object is passed as one JSONB arg
                // — recase its keys so the SQL function can read them (#400).
                let raw = value.cloned().unwrap_or(serde_json::Value::Null);
                args.push(recase_input_field_value(raw, &field.field_type, &ctx.schema));
                // DirectSql (SQLite) builds its INSERT/DELETE column list from this:
                // the column is `snake_case` in the table, while `field.name` is the
                // GraphQL surface name (camelCase under `CamelCase`), so normalise it
                // the same way the JSONB path normalises its keys (#456).
                direct_columns.push(crate::utils::to_snake_case(&field.name));
            }
            if !missing_input_fields.is_empty() {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Mutation '{mutation_name}': required input field(s) not provided or \
                         null: {}",
                        missing_input_fields.join(", ")
                    ),
                    path:    None,
                });
            }
        } else if !mutation_def.arguments[0].nullable {
            missing_required.push("input");
        }
    } else {
        // Standard argument handling (flat arguments, no input object)
        args.extend(mutation_def.arguments.iter().map(|arg| {
            let value = vars_obj.and_then(|obj| obj.get(&arg.name)).cloned();
            if let Some(v) = value {
                v
            } else {
                if !arg.nullable && arg.default_value.is_none() {
                    missing_required.push(&arg.name);
                }
                arg.default_value.as_ref().map_or(serde_json::Value::Null, |v| v.to_json())
            }
        }));
        direct_columns.extend(mutation_def.arguments.iter().map(|arg| arg.name.clone()));
    }

    if !missing_required.is_empty() {
        return Err(FraiseQLError::Validation {
            message: format!(
                "Mutation '{mutation_name}' is missing required argument(s): {}",
                missing_required.join(", ")
            ),
            path:    None,
        });
    }

    // 3a. Append server-injected parameters (after client args, in injection order).
    //
    // CONTRACT: inject params are always the *last* positional parameters of the SQL
    // function, in the order they appear in `inject_params` (insertion-ordered IndexMap).
    // The SQL function signature in the database MUST declare injected parameters after
    // all client-supplied parameters. Violating this order silently passes inject values
    // to the wrong SQL parameters. The CLI compiler (`fraiseql-cli compile`) validates
    // inject key names and source syntax when producing `schema.compiled.json`, but
    // cannot verify SQL function arity — that remains a developer responsibility.
    if !mutation_def.inject_params.is_empty() {
        let sec_ctx = security_ctx.ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Mutation '{}' requires inject params but no security context is available \
                 (unauthenticated request)",
                mutation_name
            ),
            path:    None,
        })?;
        for (param_name, source) in &mutation_def.inject_params {
            args.push(resolve_inject_value(param_name, source, sec_ctx)?);
            direct_inject_columns.push(param_name.clone());
        }
    }

    // 4. Dispatch by the adapter's mutation strategy: a stored-function call (PostgreSQL / MySQL /
    //    SQL Server) or direct SQL (SQLite). The FunctionCall branch below is unchanged; DirectSql
    //    builds INSERT/DELETE from the contract.
    let outcome = if matches!(ctx.adapter.mutation_strategy(), MutationStrategy::DirectSql) {
        // Direct-SQL adapters (SQLite) generate INSERT/DELETE directly from the
        // mutation contract. Update and single-JSONB input styles cannot be
        // expressed as positional columns here, and stored-function (`fn_*`)
        // mutations are unavailable, so both are rejected with a clear error.
        if pass_input_as_single_jsonb || direct_columns.is_empty() {
            return Err(FraiseQLError::Unsupported {
                message: format!(
                    "Mutation '{mutation_name}': direct-SQL adapters (e.g. SQLite) require flat \
                     positional input columns and do not support Update / single-JSONB input \
                     styles. Use PostgreSQL, MySQL, or SQL Server for those mutations."
                ),
            });
        }
        let (operation, table) = match &mutation_def.operation {
            MutationOperation::Insert { table } => (DirectMutationOp::Insert, table.as_str()),
            MutationOperation::Delete { table } => (DirectMutationOp::Delete, table.as_str()),
            MutationOperation::Update { .. } | MutationOperation::Custom => {
                return Err(FraiseQLError::Unsupported {
                    message: format!(
                        "Mutation '{mutation_name}': direct-SQL adapters (e.g. SQLite) support \
                         Insert and Delete mutations only; Update and custom / stored-procedure \
                         mutations require PostgreSQL, MySQL, or SQL Server."
                    ),
                });
            },
        };
        let direct_ctx = DirectMutationContext {
            operation,
            table,
            columns: &direct_columns,
            values: &args,
            inject_columns: &direct_inject_columns,
            return_type: &mutation_def.return_type,
        };
        let rows = ctx.adapter.execute_direct_mutation(&direct_ctx).await?;
        let row_value = rows.into_iter().next().ok_or_else(|| FraiseQLError::Validation {
            message: format!("Mutation '{mutation_name}': direct mutation affected no rows"),
            path:    None,
        })?;
        let direct_obj = row_value.as_object().ok_or_else(|| FraiseQLError::Validation {
            message: format!(
                "Mutation '{mutation_name}': direct mutation result was not a JSON object"
            ),
            path:    None,
        })?;
        // The DirectSql adapter returns a compact `{status, entity_id, entity_type,
        // entity, …}` envelope; reshape it into the canonical `mutation_response`
        // that `parse_mutation_row` expects. The adapter already errors on a zero-row
        // mutation, so reaching here means the write succeeded and changed state.
        // `entity_id` is forwarded only when UUID-shaped (integer SQLite PKs are not
        // UUIDs and would fail the `Option<Uuid>` field).
        let mut response = serde_json::Map::new();
        response.insert("succeeded".to_string(), serde_json::Value::Bool(true));
        response.insert("state_changed".to_string(), serde_json::Value::Bool(true));
        if let Some(entity) = direct_obj.get("entity") {
            response.insert("entity".to_string(), entity.clone());
        }
        response.insert(
            "entity_type".to_string(),
            direct_obj
                .get("entity_type")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String(mutation_def.return_type.clone())),
        );
        if let Some(id) = direct_obj
            .get("entity_id")
            .and_then(serde_json::Value::as_str)
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
        {
            response.insert("entity_id".to_string(), serde_json::Value::String(id.to_string()));
        }
        let row_map: std::collections::HashMap<String, serde_json::Value> =
            response.into_iter().collect();
        parse_mutation_row(&row_map)?
    } else {
        // 3b. Resolve session variables once and pass them to the adapter call so
        //     they are applied on the same connection / transaction as the function
        //     (fixes #329 — set_config(..., true) is transaction-local, so applying
        //     it on a separate pooled connection left it invisible to the function).
        //
        // Only resolved when there are variables to inject or inject_started_at is
        // enabled, and only on the authenticated path (security context present).
        // The no-op default on non-PostgreSQL adapters means an empty slice here is
        // effectively free there.
        let resolved_session_vars = {
            let sv = &ctx.schema.session_variables;
            match security_ctx {
                Some(sec_ctx) if !sv.variables.is_empty() || sv.inject_started_at => {
                    crate::runtime::executor::security::resolve_session_variables(sv, sec_ctx)?
                },
                _ => Vec::new(),
            }
        };
        let session_pairs: Vec<(&str, &str)> =
            resolved_session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();

        // 4. Call the database function (session variables pinned to its connection) AND write the
        //    change-log outbox row in the same transaction — the Change Spine transactional outbox.
        //    The framework owns this write by default; apps drop their hand-rolled
        //    per-mutation-function inserts on upgrade (a documented breaking change). The adapter
        //    reads the changed-entity columns (object_id / object_data / updated_fields / cascade)
        //    from the function's own mutation_response row; only the DML verb and a NOT-NULL
        //    object_type fallback (the GraphQL return type) are threaded down here. Non-PostgreSQL
        //    adapters ignore the change-log descriptor (multi-DB parity lands in phase-03).
        //
        //    Opt-out (default-on): a row is written only when the global switch
        //    (`RuntimeConfig.changelog_enabled`) is on AND this mutation is not
        //    individually opted out (`MutationDefinition.changelog`). Passing `None`
        //    makes the adapter behave exactly like the session-affine path.
        let modification_type = mutation_def.operation.kind_str().to_uppercase();
        let write_changelog = ctx.config.changelog_enabled && mutation_def.changelog;
        // Envelope stamp (phase-03): stamp the tenant partition id EXPLICITLY from the
        // SecurityContext — never reconstructed from connection / RLS state, because
        // out-of-session spine consumers (poller, NATS bridge) bypass RLS and must
        // re-authz fan-out from the row itself. `tenant_id` is the Trinity
        // public-facing UUID; a request with no tenant, or a tenant identifier that
        // is not UUID-shaped, leaves it NULL (we never abort a user's mutation over a
        // log-row stamp). `trace_id` is the originating request's W3C trace id (#375),
        // stamped onto the SecurityContext from the inbound `traceparent` header — NULL
        // for a request with no trace context. `schema_version` is the compiled
        // schema's content hash (#377), a per-deployment constant precomputed once on
        // the ExecutorContext — NOT request-scoped — so a row records which deployment
        // produced it (the #378 replay-correctness handle). `trace_context` is the
        // request's full W3C trace context (#375), serialized JSON from the
        // SecurityContext — NULL for a request with no trace context. `actor_type`
        // is the request's actor classification and `acting_for` the delegated human
        // an agent acts for (#390), both derived onto the SecurityContext at auth time
        // — NULL for an unauthenticated mutation (no SecurityContext to stamp).
        let tenant_uuid = security_ctx
            .and_then(|c| c.tenant_id.as_ref())
            .and_then(|t| uuid::Uuid::parse_str(t.as_str()).ok());
        let trace_id = security_ctx.and_then(SecurityContext::trace_id);
        // Serialize the trace context once, into a binding that outlives the write call
        // (ChangeLogWrite borrows it as JSON text).
        let trace_context_json = security_ctx
            .and_then(SecurityContext::trace_context)
            .map(serde_json::Value::to_string);
        let actor_type = security_ctx.map(|c| c.actor_type().as_str());
        let acting_for = security_ctx.and_then(SecurityContext::acting_for);
        let changelog = write_changelog.then(|| {
            ChangeLogWrite::new(&mutation_def.return_type, &modification_type)
                .with_tenant_id(tenant_uuid)
                .with_trace_id(trace_id)
                .with_schema_version(Some(&ctx.schema_version))
                .with_trace_context(trace_context_json.as_deref())
                .with_actor_type(actor_type)
                .with_acting_for(acting_for)
                // Opt-in pre-image: when this mutation sets `changelog_pre_image`, the
                // outbox CTE also records the entity's before-state (from the
                // function's `entity_before`) into `object_data_before`. Off by
                // default → no extra column, byte-for-byte today's behavior.
                .with_pre_image(mutation_def.changelog_pre_image)
        });
        let rows = if ctx.config.dry_run_mutations {
            // Validate-bind-without-commit (#501): run the function inside a
            // transaction the adapter rolls back, so nothing persists and no
            // outbox row is written. The `changelog` descriptor above is unused
            // on this path. PostgreSQL implements the rollback; other adapters
            // return `Unsupported` rather than silently committing.
            ctx.adapter
                .execute_function_call_dry_run(sql_source, &args, &session_pairs)
                .await?
        } else {
            ctx.adapter
                .execute_function_call_with_changelog(
                    sql_source,
                    &args,
                    &session_pairs,
                    changelog.as_ref(),
                )
                .await?
        };

        // 5. Expect at least one row
        let row = rows.into_iter().next().ok_or_else(|| FraiseQLError::Validation {
            message: format!("Mutation '{mutation_name}': function returned no rows"),
            path:    None,
        })?;

        // 6. Parse the mutation_response row
        parse_mutation_row(&row)?
    };

    // 6a. Bump fact table versions after a successful mutation.
    //
    // This invalidates cached aggregation results for any fact tables listed
    // in `MutationDefinition.invalidates_fact_tables`.  We bump versions on
    // Success only — an Error outcome means no data was written, so caches
    // remain valid.  Non-cached adapters return Ok(()) from the default trait
    // implementation (no-op); only `CachedDatabaseAdapter` performs actual work.
    if matches!(outcome, MutationOutcome::Success { .. })
        && !mutation_def.invalidates_fact_tables.is_empty()
    {
        ctx.adapter
            .bump_fact_table_versions(&mutation_def.invalidates_fact_tables)
            .await?;
    }

    // Invalidate query result cache for views/entities touched by this mutation.
    //
    // Strategy:
    // - UPDATE/DELETE with entity_id: entity-aware eviction only (precise, no false positives).
    //   Evicts only the cache entries that actually contain the mutated entity UUID.
    // - CREATE or explicit invalidates_views: view-level flush. For CREATE the new entity isn't in
    //   any existing cache entry, so entity-aware is a no-op. View-level ensures list queries
    //   return the new row.
    // - No entity_id and no views declared: infer view from return type (backward-compat).
    if let MutationOutcome::Success {
        entity_type,
        entity_id,
        ..
    } = &outcome
    {
        // Entity-aware path: precise eviction for UPDATE/DELETE.
        if let (Some(etype), Some(eid)) = (entity_type.as_deref(), entity_id.as_deref()) {
            ctx.adapter.invalidate_by_entity(etype, eid).await?;

            // The response cache doesn't have entity-level granularity, so
            // invalidate by the inferred view for this entity type.
            if let Some(ref rc) = ctx.response_cache {
                let inferred_view = ctx
                    .schema
                    .types
                    .iter()
                    .find(|t| t.name == etype)
                    .filter(|t| !t.sql_source.as_str().is_empty())
                    .map(|t| t.sql_source.to_string());
                if let Some(view) = inferred_view {
                    let _ = rc.invalidate_views(&[ViewName::from(view)]);
                }
            }
        }

        // View-level path: needed when entity_id is absent (CREATE) or when the developer
        // explicitly declared invalidates_views to also refresh list queries.
        if entity_id.is_none() || !mutation_def.invalidates_views.is_empty() {
            // Promote the schema's `Vec<String>` view list into `Vec<ViewName>`
            // once — every downstream invalidator borrows the same Arc<str>.
            let views_to_invalidate: Vec<ViewName> = if mutation_def.invalidates_views.is_empty() {
                ctx.schema
                    .types
                    .iter()
                    .find(|t| t.name == mutation_def.return_type)
                    .filter(|t| !t.sql_source.as_str().is_empty())
                    .map(|t| ViewName::from(t.sql_source.as_str()))
                    .into_iter()
                    .collect()
            } else {
                mutation_def.invalidates_views.iter().map(ViewName::from).collect()
            };
            if !views_to_invalidate.is_empty() {
                if entity_id.is_none() {
                    // CREATE: the new entity is absent from all existing cache entries,
                    // so point-lookup entries for other entities remain valid.  Only
                    // list queries need eviction (the new row must appear in results).
                    ctx.adapter.invalidate_list_queries(&views_to_invalidate).await?;
                } else {
                    // Developer-declared invalidates_views on an UPDATE/DELETE: honour
                    // the explicit annotation with a full view sweep.
                    ctx.adapter.invalidate_views(&views_to_invalidate).await?;
                }
                // Also invalidate the response cache for these views
                if let Some(ref rc) = ctx.response_cache {
                    let _ = rc.invalidate_views(&views_to_invalidate);
                }
            }
        }
    }

    // Clone name and return_type to avoid borrow issues after schema lookups
    let mutation_return_type = mutation_def.return_type.clone();
    let mutation_name_owned = mutation_name.to_string();
    // Whether this mutation exposes the typed cascade payload surface (Phase 03).
    let is_cascade = mutation_def.cascade;

    // Evaluate @skip / @include against the request variables before projecting, so
    // conditional fields are honoured exactly as on the query path. (Named fragment
    // spreads were already resolved at classification time, where the document's
    // fragment definitions are available.)
    let variables_map: std::collections::HashMap<String, serde_json::Value> = match variables {
        Some(serde_json::Value::Object(map)) => {
            map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        },
        _ => std::collections::HashMap::new(),
    };
    let filtered_selections = DirectiveEvaluator::filter_selections(selections, &variables_map)
        .map_err(|e| FraiseQLError::Validation {
            message: e.to_string(),
            path:    Some("directives".to_string()),
        })?;
    let selections: &[FieldSelection] = &filtered_selections;

    let result_json = match outcome {
        MutationOutcome::Success {
            entity,
            entity_type,
            cascade,
            updated_fields,
            ..
        } if is_cascade => {
            // Cascade mutation: build the typed payload `{ entity, cascade,
            // updatedFields }`, projecting + field-authorizing the primary entity
            // and every cascade entity (findings 1, 5). The payload type is the
            // success member of the (possibly error-union) return type; the
            // concrete entity type is the DB-stamped `entity_type`, else the
            // payload's `entity` field type.
            let payload_type = resolve_payload_type(&mutation_return_type, &ctx.schema);
            let entity_type_name = entity_type
                .clone()
                .or_else(|| payload_entity_type(&payload_type, &ctx.schema))
                .unwrap_or_else(|| mutation_return_type.clone());
            build_cascade_payload(
                ctx,
                security_ctx,
                &payload_type,
                &entity_type_name,
                &entity,
                cascade.as_ref(),
                &updated_fields,
                selections,
            )?
        },
        MutationOutcome::Success {
            entity,
            entity_type,
            cascade,
            updated_fields,
            ..
        } => {
            // Resolve the concrete GraphQL type of the success entity: the
            // mutation_response's entity_type, else the first non-error union
            // member, else the declared return type.
            let typename = entity_type
                .or_else(|| {
                    ctx.schema
                        .find_union(&mutation_return_type)
                        .and_then(|u| {
                            u.member_types
                                .iter()
                                .find(|t| ctx.schema.find_type(t).is_none_or(|td| !td.is_error))
                        })
                        .cloned()
                })
                .unwrap_or_else(|| mutation_return_type.clone());

            // Project the entity through the single canonical projector — the same
            // snake_case source keys, surface output keys, depth-aware recursion and
            // selection-gated __typename as the query path — so a mutation's success
            // payload and a query over the same entity return an identical shape.
            let mut projected = project_entity(&entity, &typename, selections, &ctx.schema);

            // Enforce the dynamic field authorizer (#423) on the success entity, per
            // the resolved concrete type, before surfacing it. Fail-closed.
            enforce_mutation_field_authz(
                ctx,
                security_ctx,
                &typename,
                selections,
                &entity,
                &mut projected,
            )?;

            // Cascade is opt-in (`cascade = true`, handled by the guarded arm
            // above): a non-cascade mutation never surfaces cascade, even if its
            // function returns a `cascade` JSONB. This ends the unrequested,
            // undeclared injection the cascade evaluation flagged (finding 3) —
            // `cascade` is now a typed, selection-gated payload field or nothing.
            let _ = cascade;

            // Surface `updated_fields` (the GraphQL field names this mutation
            // changed) as `updatedFields`, selection-gated — present only when the
            // client selects it, so a mutation that does not ask for it keeps an
            // exact projected shape (#433). An empty list (noop) still surfaces as
            // `[]` when selected.
            if selections_contain_field(selections, "updatedFields") {
                if let serde_json::Value::Object(ref mut map) = projected {
                    map.insert(
                        "updatedFields".to_string(),
                        serde_json::Value::Array(
                            updated_fields.into_iter().map(serde_json::Value::String).collect(),
                        ),
                    );
                }
            }

            projected
        },
        MutationOutcome::Error {
            error_class,
            message,
            http_status,
            entity_type,
            metadata,
        } => {
            let status = error_class.as_str();

            // Build the error projection source from the error_detail JSONB, enriched
            // with the composite's first-class fields under snake_case keys so a
            // declared error type can surface them as ordinary projected fields
            // (project_entity omits any field whose source key is absent). The
            // always-injected `status` is attached after projection, below.
            let mut source_map = match metadata {
                serde_json::Value::Object(map) => map,
                _ => serde_json::Map::new(),
            };
            if !message.is_empty() {
                source_map
                    .entry("message".to_string())
                    .or_insert_with(|| serde_json::Value::String(message));
            }
            if let Some(code) = http_status {
                source_map
                    .entry("http_status".to_string())
                    .or_insert_with(|| serde_json::json!(code));
            }
            source_map
                .entry("error_class".to_string())
                .or_insert_with(|| serde_json::Value::String(status.to_string()));
            let source = serde_json::Value::Object(source_map);

            // Resolve the concrete error type to project — symmetric with the
            // success arm's typename resolution (#465). The function stamps the
            // declared error type it produced onto `entity_type`, so prefer it when
            // it names a known `is_error` type: this routes onto the *specific*
            // error member (e.g. `DuplicateEmailError` vs `ValidationError`) and,
            // crucially, surfaces the declared error type even when the mutation's
            // return type is the bare success entity rather than a union (the
            // `Entity`-return + declared-error-types pattern, where `find_union`
            // finds nothing and the result previously leaked the success typename).
            // Fall back to the return union's first `is_error` member otherwise.
            let error_type = entity_type
                .as_deref()
                .and_then(|name| ctx.schema.find_type(name))
                .filter(|td| td.is_error)
                .or_else(|| {
                    ctx.schema.find_union(&mutation_return_type).and_then(|u| {
                        u.member_types.iter().find_map(|t| {
                            let td = ctx.schema.find_type(t)?;
                            if td.is_error { Some(td) } else { None }
                        })
                    })
                });

            // Project the error source through the same canonical projector when the
            // schema declares a matching error type. Otherwise emit just __typename
            // (only when selected, matching the query contract); status is attached
            // below in both cases.
            let mut result = if let Some(td) = error_type {
                project_entity(&source, td.name.as_str(), selections, &ctx.schema)
            } else {
                let mut map = serde_json::Map::new();
                // Scan recursively: `__typename` may be nested inside an inline
                // fragment (`... on T { __typename }`), not just at the top level.
                if selections_contain_field(selections, "__typename") {
                    map.insert(
                        "__typename".to_string(),
                        serde_json::Value::String(mutation_return_type.clone()),
                    );
                }
                serde_json::Value::Object(map)
            };

            // Enforce the dynamic field authorizer (#423) on error metadata too, so a
            // gated field on an error type cannot leak through the error arm.
            if let Some(td) = error_type {
                enforce_mutation_field_authz(
                    ctx,
                    security_ctx,
                    td.name.as_str(),
                    selections,
                    &source,
                    &mut result,
                )?;
            }

            // Inject the synthetic `status` field — not part of the type definition,
            // but required by clients to discriminate error outcomes.
            if let serde_json::Value::Object(ref mut map) = result {
                map.insert("status".to_string(), serde_json::Value::String(status.to_string()));
            }

            result
        },
    };

    // 7. Emit structured mutation audit event when audit_mutations is enabled.
    //
    // This is the single chokepoint for all mutation paths (GraphQL handler,
    // REST handler, typed execute_mutation, bulk filter). Zero-cost when disabled:
    // the branch is not taken and no string formatting or allocation occurs.
    if ctx.config.audit_mutations {
        tracing::info!(
            target: "fraiseql::mutation_audit",
            mutation_name = mutation_name,
            entity_type = %mutation_def.return_type,
            operation = %mutation_def.operation.kind_str(),
            tenant_id = %security_ctx
                .and_then(|c| c.tenant_id.as_ref().map(|t| t.as_str()))
                .unwrap_or(""),
            "mutation.executed"
        );
    }

    let response = ResultProjector::wrap_in_data_envelope(result_json, &mutation_name_owned);
    Ok(response)
}

#[cfg(test)]
mod tests;
