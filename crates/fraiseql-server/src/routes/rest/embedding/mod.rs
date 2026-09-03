//! Nested resource embedding executor.
//!
//! Executes embedded resource sub-queries based on parsed [`EmbeddedSpec`]
//! entries from the `?select=` parameter. Supports `OneToMany` (array),
//! `ManyToOne` (single object), and `OneToOne` (object or null) cardinalities.
//!
//! For PostgreSQL, generates sub-queries with `jsonb_agg` / `jsonb_build_object`.
//! Empty collections return `[]`, not null. Single absent objects return `null`.

pub mod executor;

#[cfg(test)]
mod tests;

use std::{collections::HashMap, sync::Arc};

use executor::{EmbedCtx, count_related, declared_key, embed_into_rows, embed_into_single};
use fraiseql_core::{
    db::traits::DatabaseAdapter,
    schema::{CompiledSchema, RestConfig},
    security::SecurityContext,
};

use super::{
    handler::RestError,
    params::{EmbeddedSpec, SelectEntry},
};

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

/// The parent-row keys an embedded selection needs projected, in the spelling the
/// projected row will carry.
///
/// An embed is resolved by reading a join key off the **already-projected** parent
/// row, so a key the projection omits is a key the embed cannot follow. That was
/// #1230: `?select=id,author(name)` never projected `fk_author`, `extract_join_key`
/// found nothing, and every post came back `"author": null` under a 200 —
/// indistinguishable from a post that genuinely has no author. Selecting the foreign
/// key made the same request work, so the *shape of the request* silently decided the
/// *content of the response*.
///
/// The projection is the server's decision and the join key is an implementation
/// detail of the embed, so the server adds what it needs and
/// [`strip_projected_keys`] takes it back out again. Refusing the request instead
/// ("select `fk_author` to embed `author`") would put the same detail into the
/// client's contract permanently.
///
/// Counts are included: `count_related` extracts the identical key, so
/// `?select=name,posts.count` counted zero for every parent.
///
/// Returns declared spellings, deduplicated, in selection order. Empty when the
/// parent type is unknown or nothing is embedded.
#[must_use]
pub fn required_join_keys(
    schema: &CompiledSchema,
    parent_type_name: &str,
    embeddings: &[EmbeddedSpec],
    embedding_counts: &[String],
) -> Vec<String> {
    let Some(parent_type) = schema.find_type(parent_type_name) else {
        return Vec::new();
    };

    let mut keys: Vec<String> = Vec::new();
    let named = embeddings
        .iter()
        .map(|e| e.relationship.as_str())
        .chain(embedding_counts.iter().map(String::as_str));

    for rel_name in named {
        // An unknown relationship is the parameter extractor's 400 to raise, not
        // this function's; it is skipped rather than guessed at.
        let Some(rel) = parent_type.relationships.iter().find(|r| r.name == rel_name) else {
            continue;
        };
        let key = declared_key(schema, parent_type_name, rel.parent_join_column());
        if !keys.contains(&key) {
            keys.push(key);
        }
    }

    keys
}

/// Extend `projection` with every key in `required` it does not already carry, and
/// return exactly the keys added.
///
/// The return value is what [`strip_projected_keys`] must remove afterwards, so it
/// names the server's additions and nothing else: a key the client selected itself is
/// already present, is therefore not added, and is therefore never stripped.
#[must_use]
pub fn project_missing_join_keys(projection: &mut Vec<String>, required: &[String]) -> Vec<String> {
    let mut added = Vec::new();
    for key in required {
        if !projection.iter().any(|f| f == key) {
            projection.push(key.clone());
            added.push(key.clone());
        }
    }
    added
}

/// Remove keys the server projected for its own use from a response document.
///
/// `data` is either an array of rows or a single row; anything else is left alone.
/// A no-op for the common case of an empty `keys`.
pub fn strip_projected_keys(data: &mut serde_json::Value, keys: &[String]) {
    if keys.is_empty() {
        return;
    }

    let strip_row = |row: &mut serde_json::Value| {
        if let Some(obj) = row.as_object_mut() {
            for key in keys {
                obj.remove(key.as_str());
            }
        }
    };

    match data {
        serde_json::Value::Array(rows) => rows.iter_mut().for_each(strip_row),
        serde_json::Value::Object(_) => strip_row(data),
        _ => {},
    }
}

/// A sub-select's entries, separated by kind.
///
/// The whole of `?select=posts(title,author(name),comments.count)` after the
/// parser: flat fields, nested embeds, nested counts.
///
/// **Why this type exists rather than three `filter_map`s.** A sub-select used
/// to be read by a single `filter_map` matching `SelectEntry::Field` with
/// `_ => None` for the rest, so nested embeds and nested counts were parsed,
/// depth-validated and then silently discarded — the response simply lacked the
/// key, under a 200, and a client could not tell "nothing related" from "the
/// server dropped my selection". #864 fixed the embed half and left the count
/// half in the wildcard, with a comment that named both. #1267 is that second
/// half, found three releases later.
///
/// [`Self::split`] matches **exhaustively**: there is no `_` arm, so a fourth
/// `SelectEntry` variant cannot be added without this function failing to
/// compile. That is the point — the two defects above were both a wildcard
/// quietly absorbing a case nobody had handled.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct SubSelect {
    /// Flat field names, in selection order.
    pub fields: Vec<String>,
    /// Nested embedded resources, in selection order.
    pub embeds: Vec<EmbeddedSpec>,
    /// Nested count-only relationships, in selection order.
    pub counts: Vec<String>,
}

impl SubSelect {
    /// Separate `entries` by kind, preserving selection order within each.
    pub(super) fn split(entries: &[SelectEntry]) -> Self {
        let mut out = Self::default();
        for entry in entries {
            match entry {
                SelectEntry::Field(name) => out.fields.push(name.clone()),
                SelectEntry::Embedded(spec) => out.embeds.push(spec.clone()),
                SelectEntry::Count(name) => out.counts.push(name.clone()),
            }
        }
        out
    }
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
        parent_type:      req.parent_type_name,
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

        // Every kind of entry the sub-select carries, separated in one exhaustive
        // pass. See [`SubSelect`] for why this is not three `filter_map`s.
        let SubSelect {
            fields: mut sub_field_names,
            embeds: nested,
            counts: nested_counts,
        } = SubSelect::split(&spec.fields);

        // A nested embedding joins on a key of the *child* row, so that key has to be
        // projected even when the client did not ask for it. Without this the recursion
        // below would find no join value and set every nested collection empty — the
        // same defect one level down.
        //
        // #1230: it also has to be taken back out. #864 added the key and returned it,
        // so `posts(title,author(name))` answered with an `fk_author` nobody named —
        // the response shape depending on which relationships the schema declares
        // rather than on what was asked for. Same rule as the root projection, same
        // pair of helpers, so the two levels cannot drift.
        let injected = project_missing_join_keys(
            &mut sub_field_names,
            &required_join_keys(req.schema, &rel.target_type, &nested, &nested_counts),
        );

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

        // #864: recurse into the embedded rows so a validated depth actually executes.
        // The parser builds arbitrarily nested `EmbeddedSpec`s and
        // `validate_embedding_depth` bounds them by `max_embedding_depth`, so the depth
        // reached here is already the validated one — validator and executor now agree by
        // construction rather than by two separate opinions.
        //
        // Nested filters are not addressable in the `?rel.field=value` syntax (it is flat,
        // one segment deep), so the recursion passes an empty filter map rather than
        // silently reusing the parent's.
        // #1267: the condition is `nested` OR `nested_counts`. Gating on `nested`
        // alone would leave a count-only sub-select unexecuted *and* leak the join
        // key this level injected for it — `strip_projected_keys` lives inside this
        // block, so a branch that skips the recursion also skips the cleanup.
        if !nested.is_empty() || !nested_counts.is_empty() {
            let nested_req = EmbeddingRequest {
                executor:         req.executor,
                schema:           req.schema,
                config:           req.config,
                parent_type_name: &rel.target_type,
                security_context: req.security_context,
            };
            let no_filters = HashMap::new();

            // Each parent row holds its own embedded collection, so the recursion runs
            // per row over the value just written. The join keys this level injected
            // are stripped only *after* the recursion and the counts have read them:
            // `count_related` extracts the identical key, so stripping between the two
            // would reintroduce #1230 for counts alone.
            match parent_data {
                serde_json::Value::Array(rows) => {
                    for row in rows.iter_mut() {
                        if let Some(child) = row.get_mut(output_name) {
                            Box::pin(execute_embeddings(&nested_req, child, &nested, &no_filters))
                                .await?;
                            Box::pin(execute_embedding_counts(&nested_req, child, &nested_counts))
                                .await?;
                            strip_projected_keys(child, &injected);
                        }
                    }
                },
                serde_json::Value::Object(_) => {
                    if let Some(child) = parent_data.get_mut(output_name) {
                        Box::pin(execute_embeddings(&nested_req, child, &nested, &no_filters))
                            .await?;
                        Box::pin(execute_embedding_counts(&nested_req, child, &nested_counts))
                            .await?;
                        strip_projected_keys(child, &injected);
                    }
                },
                _ => {},
            }
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

    let ctx = EmbedCtx {
        executor:         req.executor,
        schema:           req.schema,
        config:           req.config,
        parent_type:      req.parent_type_name,
        security_context: req.security_context,
    };

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
                    let count = count_related(&ctx, rel, row).await?;
                    if let Some(obj) = row.as_object_mut() {
                        obj.insert(count_key.clone(), serde_json::json!(count));
                    }
                }
            },
            serde_json::Value::Object(_) => {
                let count = count_related(&ctx, rel, parent_data).await?;
                if let Some(obj) = parent_data.as_object_mut() {
                    obj.insert(count_key, serde_json::json!(count));
                }
            },
            _ => {},
        }
    }

    Ok(())
}
