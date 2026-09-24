//! A GraphQL selection into a nested type, classified as a read of that type.
//!
//! `{ users { orders { margin } } }` serves `Order` documents out of the `users` view's
//! materialised `data`. Field-level RBAC used to classify the root's projection against
//! the root type and nothing else, so what a read of `Order` would have masked or refused
//! reached the response through any type that embeds it. Every level is classified here,
//! against its own type, before the read — so a `Reject` at any depth never reaches the
//! database — and masked afterwards at the keys the response carries it under.
//!
//! # By field, not by response key
//!
//! A selection's alias is the client's choice of output key, not another field:
//! `{ orders { m: margin } }` reads `margin`. The planner lists the root's projection by
//! response key, which is what the projector needs, and the classifier used to be handed
//! that list — so an aliased field matched no declared field and passed through
//! unclassified. Classification here takes the field name; masking takes the key.

use std::collections::{HashMap, HashSet};

use super::query::QueryRunner;
use crate::{
    backend::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, EmbedShape,
        EmbedSource, LevelKeys, WhereClause,
    },
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::{field_filter::FieldAccessResult, projection::effective_selections},
    schema::{
        CompiledSchema, FieldDefinition, FieldType, QueryDefinition, Relationship, TypeDefinition,
    },
    security::{ConstrainedPaths, RLSPolicy, SecurityContext, rls_policy::RlsTarget},
};

/// What field-level RBAC decided for a whole selection tree.
pub(super) struct SelectionAccess {
    /// The root level, in the shape the projector takes: the projection's response keys,
    /// and the masked ones among them.
    pub(super) root: FieldAccessResult,
    /// Every `(type, field)` the caller may not read and that masks, at any level.
    masked:          HashSet<(String, String)>,
}

impl SelectionAccess {
    /// Classify every level of `root_fields` against the type it selects from.
    ///
    /// `projection_keys` is the planner's projection — the root's response keys, in
    /// order — returned unchanged as [`Self::root`]'s `projected`.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for a selected field, at any depth, that requires a
    /// scope the caller lacks and whose `on_deny` is `Reject`.
    pub(super) fn classify(
        schema: &CompiledSchema,
        root_type: &str,
        root_fields: &[FieldSelection],
        projection_keys: Vec<String>,
        security_context: Option<&SecurityContext>,
    ) -> Result<Self> {
        let mut masked = HashSet::new();
        classify_level(schema, root_type, root_fields, security_context, &mut masked)?;

        let masked_keys = effective_selections(root_fields, root_type, schema)
            .into_iter()
            .filter(|sel| masked.contains(&(root_type.to_string(), sel.name.clone())))
            .map(|sel| sel.response_key().to_string())
            .collect();
        Ok(Self {
            root: FieldAccessResult {
                projected: projection_keys,
                masked:    masked_keys,
            },
            masked,
        })
    }

    /// Null every masked field of a projected result, at every level, under the key the
    /// response carries it.
    pub(super) fn null_masked(
        &self,
        value: &mut serde_json::Value,
        root_type: &str,
        root_fields: &[FieldSelection],
        schema: &CompiledSchema,
    ) {
        if !self.masked.is_empty() {
            null_masked_at(value, root_type, root_fields, &self.masked, schema);
        }
    }
}

/// The object type a field holds — its element type for a list.
pub(super) fn object_type_of(field_type: &FieldType) -> Option<&str> {
    if field_type.is_scalar() {
        return None;
    }
    field_type.inner_type().unwrap_or(field_type).type_name()
}

/// Classify one level's selections against `type_name`, then every level beneath it.
fn classify_level(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    security_context: Option<&SecurityContext>,
    masked: &mut HashSet<(String, String)>,
) -> Result<()> {
    let level: Vec<&FieldSelection> = effective_selections(selections, type_name, schema)
        .into_iter()
        .filter(|sel| sel.name != "__typename")
        .collect();
    let names = level.iter().map(|sel| sel.name.clone()).collect();
    let access = super::super::support::security::classify_fields_for_read(
        schema,
        type_name,
        names,
        security_context,
    )?;
    masked.extend(access.masked.into_iter().map(|field| (type_name.to_string(), field)));

    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    for sel in level {
        let child = type_def
            .fields
            .iter()
            .find(|f| f.name == sel.name)
            .and_then(|f| object_type_of(&f.field_type));
        if let Some(child) = child {
            classify_level(schema, child, &sel.nested_fields, security_context, masked)?;
        }
    }
    Ok(())
}

fn null_masked_at(
    value: &mut serde_json::Value,
    type_name: &str,
    selections: &[FieldSelection],
    masked: &HashSet<(String, String)>,
    schema: &CompiledSchema,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                null_masked_at(item, type_name, selections, masked, schema);
            }
        },
        serde_json::Value::Object(object) => {
            let type_def = schema.find_type(type_name);
            for sel in effective_selections(selections, type_name, schema) {
                let key = sel.response_key();
                if masked.contains(&(type_name.to_string(), sel.name.clone())) {
                    if let Some(slot) = object.get_mut(key) {
                        *slot = serde_json::Value::Null;
                    }
                    continue;
                }
                let child = type_def
                    .and_then(|t| t.fields.iter().find(|f| f.name == sel.name))
                    .and_then(|f| object_type_of(&f.field_type));
                if let (Some(child), Some(nested)) = (child, object.get_mut(key)) {
                    null_masked_at(nested, child, &sel.nested_fields, masked, schema);
                }
            }
        },
        _ => {},
    }
}

// ── Row security of a nested level ───────────────────────────────────────────
//
// A read of `Order` is filtered by `Order`'s RLS predicate and `inject_params`. The
// `Order` documents `v_user` embeds were filtered by `User`'s, which says nothing about
// which orders the principal may read: a principal-scoped policy is invisible to a view
// composed for no principal, and a tenant policy is only implied where the view's join
// carries the tenant. So a nested level whose type scopes its rows is read as a level
// of a composed statement (`fraiseql_db::ComposedLevel`) carrying that type's predicate,
// either over the documents the parent's view materialised or over the type's own view.
//
// Which of the two is decided per `(parent type, field)` when the executor is built, from
// the schema and the policy's declared paths (`RLSPolicy::constrained_paths`): evaluating
// a predicate over an embedded document is sound only when the document carries every key
// the predicate reads. A policy that does not say is joined through a declared
// relationship, and refused where there is none.

/// How a nested object field's rows are gated by its type's row security.
#[derive(Debug, Clone)]
pub(in super::super) enum NestedRowGate {
    /// The type's predicate reads only `paths`, every one a key the type declares: it is
    /// evaluated over the documents the parent's view embedded.
    Project {
        /// The type's read whose policy target and `inject_params` apply.
        query: String,
        /// The stored keys the predicate may read.
        paths: Vec<String>,
    },
    /// The type's rows are read from its own view, through the parent's relationship.
    Join {
        /// The type's list query: its view, policy target and `inject_params`.
        query:        String,
        /// That query's view.
        view:         String,
        /// The parent's relationship the field is declared as.
        relationship: Relationship,
    },
    /// Neither is possible: a predicate for the type refuses the selection.
    Refuse {
        /// The type's read whose policy target and `inject_params` apply.
        query: String,
    },
}

/// Every nested object field whose rows its type scopes, and how — built once, with the
/// executor. A field absent from it is ungated: its type has no SQL-backed read of its
/// own (a value object, part of its parent's row and gated by its parent's predicate), or
/// no policy and no `inject_params` scope that read.
#[derive(Debug, Default)]
pub(in super::super) struct NestedRowGates {
    by_field: HashMap<(String, String), NestedRowGate>,
}

impl NestedRowGates {
    /// Classify every object field of every type in `schema`.
    pub(in super::super) fn build(schema: &CompiledSchema, policy: Option<&dyn RLSPolicy>) -> Self {
        let mut by_field = HashMap::new();
        for parent in &schema.types {
            for field in &parent.fields {
                let Some(target) = object_type_of(&field.field_type) else {
                    continue;
                };
                if let Some(gate) = row_gate(schema, policy, parent, field, target) {
                    by_field.insert((parent.name.to_string(), field.name.to_string()), gate);
                }
            }
        }
        Self { by_field }
    }

    fn get(&self, parent: &str, field: &str) -> Option<&NestedRowGate> {
        self.by_field.get(&(parent.to_string(), field.to_string()))
    }
}

/// The SQL-backed read of `type_name` a nested level of it is gated as: its list query
/// when it has one — the read REST embeds it through — otherwise any.
fn own_read<'a>(schema: &'a CompiledSchema, type_name: &str) -> Option<&'a QueryDefinition> {
    let reads = |q: &&QueryDefinition| {
        q.return_type == type_name
            && q.function.is_none()
            && q.sql_source.is_some()
            && !q.returns_count
    };
    schema
        .queries
        .iter()
        .filter(reads)
        .find(|q| q.returns_list)
        .or_else(|| schema.queries.iter().find(reads))
}

fn row_gate(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    parent: &TypeDefinition,
    field: &FieldDefinition,
    target: &str,
) -> Option<NestedRowGate> {
    let read = own_read(schema, target)?;
    if policy.is_none() && read.inject_params.is_empty() {
        return None;
    }

    // The keys the type's predicate may read: the policy's, and each `inject_params`
    // column held in the document. A native column is not in an embedded document.
    let mut paths = match policy.map(|p| p.constrained_paths(&RlsTarget::query(&read.name, target)))
    {
        None => Some(Vec::new()),
        Some(ConstrainedPaths::Declared(paths)) => Some(paths),
        Some(ConstrainedPaths::Opaque) => None,
    };
    for column in read.inject_params.keys() {
        if read.native_columns.contains_key(column) {
            paths = None;
        } else if let Some(paths) = paths.as_mut() {
            paths.push(crate::utils::to_snake_case(column));
        }
    }
    let declared = schema.find_type(target);
    let carried = paths.filter(|paths| {
        paths.iter().all(|path| {
            declared.is_some_and(|t| {
                t.fields.iter().any(|f| crate::utils::to_snake_case(f.name.as_str()) == *path)
            })
        })
    });
    if let Some(paths) = carried {
        return Some(NestedRowGate::Project {
            query: read.name.clone(),
            paths,
        });
    }

    let is_list = field.field_type.is_list();
    let relationship = parent.relationships.iter().find(|r| {
        r.name == field.name && r.target_type == target && r.cardinality.is_to_one() != is_list
    });
    let list = schema.queries.iter().find(|q| {
        q.return_type == target && q.returns_list && q.function.is_none() && !q.returns_count
    });
    Some(match (relationship, list.and_then(|q| q.sql_source.as_ref().map(|v| (q, v)))) {
        (Some(relationship), Some((list, view))) => NestedRowGate::Join {
            query:        list.name.clone(),
            view:         view.clone(),
            relationship: relationship.clone(),
        },
        _ => NestedRowGate::Refuse {
            query: read.name.clone(),
        },
    })
}

impl SelectionAccess {
    fn masks(&self, type_name: &str, field: &str) -> bool {
        self.masked.contains(&(type_name.to_string(), field.to_string()))
    }
}

impl QueryRunner {
    /// The nested levels of `root_fields` read as levels of a composed statement: every one
    /// whose type's predicate applies, and every one above such a level, which has to be a
    /// level to carry it. Empty when no nested level is scoped — the read stays flat.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` when a nested type's predicate cannot be applied
    /// ([`NestedRowGate::Refuse`], or a declared-path predicate that reads an undeclared
    /// key); the policy's own refusals; `FraiseQLError::Validation` for an `inject_params`
    /// read with no principal to resolve it from.
    pub(super) fn plan_nested_reads(
        &self,
        root_type: &str,
        root_fields: &[FieldSelection],
        security_context: Option<&SecurityContext>,
        access: &SelectionAccess,
    ) -> Result<Vec<ComposedEmbed>> {
        self.plan_nested_level(root_type, root_fields, security_context, access)
    }

    fn plan_nested_level(
        &self,
        parent_type: &str,
        selections: &[FieldSelection],
        security_context: Option<&SecurityContext>,
        access: &SelectionAccess,
    ) -> Result<Vec<ComposedEmbed>> {
        let schema = &self.ctx.schema;
        let Some(parent_def) = schema.find_type(parent_type) else {
            return Ok(Vec::new());
        };

        // One level per field, over every selection of it: `a: orders { id }` and
        // `b: orders { total }` read the same rows.
        let mut fields: Vec<(&FieldDefinition, Vec<FieldSelection>)> = Vec::new();
        for sel in effective_selections(selections, parent_type, schema) {
            let Some(field) = parent_def.fields.iter().find(|f| f.name == sel.name) else {
                continue;
            };
            if object_type_of(&field.field_type).is_none() {
                continue;
            }
            match fields.iter_mut().find(|(f, _)| f.name == field.name) {
                Some((_, subs)) => subs.extend(sel.nested_fields.iter().cloned()),
                None => fields.push((field, sel.nested_fields.clone())),
            }
        }

        let mut embeds = Vec::new();
        for (field, subs) in fields {
            let Some(target) = object_type_of(&field.field_type) else {
                continue;
            };
            let children = self.plan_nested_level(target, &subs, security_context, access)?;
            let gate = self.ctx.nested_row_gates.get(parent_type, field.name.as_str());

            let (predicate, source, view) = match gate {
                None => (None, None, None),
                Some(NestedRowGate::Project { query, paths }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    if let Some(clause) = predicate.as_ref() {
                        if !reads_only(clause, paths) {
                            return Err(FraiseQLError::Authorization {
                                message:  format!(
                                    "the row-security predicate for '{target}' reads a key its \
                                     policy does not declare, so it cannot be evaluated over the \
                                     '{target}' documents '{parent_type}.{}' embeds; refusing \
                                     rather than serving them unfiltered",
                                    field.name
                                ),
                                action:   Some("read".to_string()),
                                resource: Some(target.to_string()),
                            });
                        }
                    }
                    (predicate, None, None)
                },
                Some(NestedRowGate::Join {
                    query,
                    view,
                    relationship,
                }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    let source =
                        super::query_composed::correlated_source(schema, relationship, parent_type);
                    (predicate, Some(source), Some(view.clone()))
                },
                Some(NestedRowGate::Refuse { query }) => {
                    let predicate = self.nested_predicate(query, target, security_context)?;
                    if predicate.is_some() {
                        return Err(FraiseQLError::Authorization {
                            message:  format!(
                                "'{parent_type}.{}' embeds '{target}', whose row-security policy \
                                 does not declare the keys it reads, and '{parent_type}' declares \
                                 no relationship '{}' to read '{target}' through; refusing rather \
                                 than serving '{target}' rows unfiltered (declare the \
                                 relationship, or implement `RLSPolicy::constrained_paths`)",
                                field.name, field.name
                            ),
                            action:   Some("read".to_string()),
                            resource: Some(target.to_string()),
                        });
                    }
                    (None, None, None)
                },
            };

            // A joined level is always joined, so which rows it holds does not depend on
            // who asks. A materialised one is only a level when it has to be.
            if source.is_none() && predicate.is_none() && children.is_empty() {
                continue;
            }
            let (stored, fallback) = crate::runtime::stored_key_candidates(field.name.as_str());
            let source = source.unwrap_or_else(|| EmbedSource::Materialised {
                keys: std::iter::once(stored.clone()).chain(fallback).collect(),
            });
            let view = view.unwrap_or_else(|| {
                schema.find_type(target).map_or_else(String::new, |t| t.sql_source.to_string())
            });
            let keys = level_keys(schema, target, &subs, &children, access);
            embeds.push(ComposedEmbed {
                output_key: stored,
                shape: if field.field_type.is_list() {
                    EmbedShape::Many
                } else {
                    EmbedShape::One
                },
                source,
                level: ComposedLevel {
                    view,
                    projection: None,
                    where_clause: predicate,
                    order_by: None,
                    limit: None,
                    offset: None,
                    keys,
                    embeds: children,
                },
            });
        }
        Ok(embeds)
    }

    /// The row predicate a read of `target` through `query` carries: the policy's, AND-ed
    /// with the query's `inject_params` — what a root read of that query composes.
    fn nested_predicate(
        &self,
        query: &str,
        target: &str,
        security_context: Option<&SecurityContext>,
    ) -> Result<Option<WhereClause>> {
        let Some(read) = self.ctx.schema.queries.iter().find(|q| q.name == query) else {
            return Ok(None);
        };
        let mut conditions = Vec::new();
        match (&self.ctx.config.rls_policy, security_context) {
            (Some(policy), Some(ctx)) => {
                if let Some(clause) = policy.evaluate(ctx, &RlsTarget::query(&read.name, target))? {
                    conditions.push(clause.into_where_clause());
                }
            },
            // Fail closed (#784), as a root read with a policy and no principal does.
            (Some(_), None) => {
                return Err(FraiseQLError::Validation {
                    message: format!("Query '{}' not found in schema", read.name),
                    path:    None,
                });
            },
            (None, _) => {},
        }
        if !read.inject_params.is_empty() {
            let Some(ctx) = security_context else {
                return Err(FraiseQLError::Validation {
                    message: format!(
                        "Type '{target}' is scoped by the inject params of query '{}', which a \
                         request without a security context cannot resolve",
                        read.name
                    ),
                    path:    None,
                });
            };
            for (column, source) in &read.inject_params {
                let value = super::super::resolve_inject_value(column, source, ctx)?;
                conditions.push(super::query_params::inject_param_where_clause(
                    column,
                    value,
                    &read.native_columns,
                ));
            }
        }
        Ok(match conditions.len() {
            0 => None,
            1 => conditions.pop(),
            _ => Some(WhereClause::And(conditions)),
        })
    }
}

/// Whether `clause` reads only the top-level stored keys in `paths`.
fn reads_only(clause: &WhereClause, paths: &[String]) -> bool {
    match clause {
        WhereClause::Field { path, .. } => path.first().is_some_and(|key| paths.contains(key)),
        WhereClause::And(all) | WhereClause::Or(all) => all.iter().all(|c| reads_only(c, paths)),
        WhereClause::Not(inner) | WhereClause::Typed { inner, .. } => reads_only(inner, paths),
        // A native column is not in a document, and a shape this does not know reads
        // something it cannot vouch for.
        _ => false,
    }
}

/// The keys a nested level returns: every selected field's stored spellings, masked ones
/// as `NULL`, and none of those its own embedded levels replace.
fn level_keys(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
    embedded: &[ComposedEmbed],
    access: &SelectionAccess,
) -> LevelKeys {
    let mut kept = Vec::new();
    let mut masked = Vec::new();
    for sel in effective_selections(selections, type_name, schema) {
        if sel.name == "__typename" {
            continue;
        }
        let (stored, fallback) = crate::runtime::stored_key_candidates(&sel.name);
        if embedded.iter().any(|e| e.output_key == stored) {
            continue;
        }
        let into = if access.masks(type_name, &sel.name) {
            &mut masked
        } else {
            &mut kept
        };
        for key in std::iter::once(stored).chain(fallback) {
            if !into.contains(&key) {
                into.push(key);
            }
        }
    }
    LevelKeys::Only { kept, masked }
}

/// The root's keys when `embedded` replace some of them: its whole document, less every
/// stored spelling of each — the embedded value is the gated one.
pub(super) fn root_keys(embedded: &[ComposedEmbed]) -> LevelKeys {
    LevelKeys::Without(
        embedded
            .iter()
            .flat_map(|e| match &e.source {
                EmbedSource::Materialised { keys } => keys.clone(),
                EmbedSource::Correlated { .. } => {
                    let (stored, fallback) = crate::runtime::stored_key_candidates(&e.output_key);
                    std::iter::once(stored).chain(fallback).collect()
                },
            })
            .collect(),
    )
}

/// The stored document of one composed row, with each embedded value written under its
/// stored key in place of what the parent's view stored there.
pub(super) fn merge_composed_row(
    row: &serde_json::Value,
    embeds: &[ComposedEmbed],
) -> serde_json::Value {
    let mut document = row.get(COMPOSED_DOCUMENT_KEY).cloned().unwrap_or(serde_json::Value::Null);
    let embedded = row.get(COMPOSED_EMBEDS_KEY);
    if let serde_json::Value::Object(object) = &mut document {
        for embed in embeds {
            let raw = embedded.and_then(|e| e.get(&embed.output_key));
            let value = match (embed.shape, raw) {
                (EmbedShape::Many, Some(serde_json::Value::Array(rows))) => {
                    serde_json::Value::Array(
                        rows.iter().map(|r| merge_composed_row(r, &embed.level.embeds)).collect(),
                    )
                },
                (EmbedShape::Many, _) => serde_json::Value::Array(Vec::new()),
                (EmbedShape::One, Some(row @ serde_json::Value::Object(_))) => {
                    merge_composed_row(row, &embed.level.embeds)
                },
                _ => serde_json::Value::Null,
            };
            object.insert(embed.output_key.clone(), value);
        }
    }
    document
}

impl QueryRunner {
    /// Read `root` as one composed statement and return its rows as stored documents, each
    /// with its gated nested levels merged in — the shape a flat read of the root returns.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Unsupported` from an adapter that cannot compose, refused from its
    /// capability flag as REST's composed read is; the adapter's own errors.
    pub(super) async fn execute_composed_document_read(
        &self,
        root: ComposedLevel,
        session_pairs: &[(&str, &str)],
        routing: crate::backend::types::ReadRouting,
    ) -> Result<std::sync::Arc<Vec<crate::backend::JsonbValue>>> {
        if !self.ctx.adapter.supports_composed_reads() {
            return Err(crate::backend::composed_read_unsupported(&root.view));
        }
        let rows = self
            .ctx
            .adapter
            .execute_composed_with_session(&root, session_pairs, routing)
            .await?;
        Ok(std::sync::Arc::new(
            rows.iter()
                .map(|row| {
                    crate::backend::JsonbValue::new(merge_composed_row(&row.data, &root.embeds))
                })
                .collect(),
        ))
    }
}

/// Project stored documents through `root_fields` at `root_type`, every level included.
pub(super) fn project_documents(
    documents: &[crate::backend::JsonbValue],
    root_type: &str,
    root_fields: &[FieldSelection],
    schema: &CompiledSchema,
    returns_list: bool,
) -> serde_json::Value {
    let project = |document: &crate::backend::JsonbValue| {
        crate::runtime::project_entity(&document.data, root_type, root_fields, schema)
    };
    if returns_list {
        serde_json::Value::Array(documents.iter().map(project).collect())
    } else {
        documents.first().map_or(serde_json::Value::Null, project)
    }
}
