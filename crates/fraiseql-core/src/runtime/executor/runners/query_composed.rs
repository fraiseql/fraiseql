//! Composed direct reads: a read and everything embedded into it, as one statement.
//!
//! The REST `?select=` embed was a parent read plus one sub-read per parent row per
//! level, issued from the transport. It is now one statement: each embedded level is a
//! correlated `LATERAL` subquery in the parent's, with its own filter, ordering and page
//! (`fraiseql_db::ComposedLevel`).
//!
//! # Every level is a read, and is gated as one
//!
//! Composing moves the embedded levels out of reads of their own and into the parent's
//! statement, which is exactly where a gate attached to a *read* stops seeing them. So
//! each level is resolved through [`QueryRunner::resolve_direct_read`] — the function
//! every direct read goes through — against the **target's** list query, before anything
//! is sent:
//!
//! * operation authorization, `requires_role` and `requires_actor` of the target query;
//! * the #423 refusal of a policy-gated field selected at that level;
//! * field-level RBAC classified against the **target** type. A `Reject` refuses the request here;
//!   a `Mask` travels to the SQL, where the level's document carries the key as `NULL` and the
//!   value never leaves the database ([`LevelKeys::Only`]);
//! * the target's RLS predicate and `inject_params` scoping, which become that level's `WHERE`
//!   inside its `LATERAL` — not the parent's, which says nothing about the tenant of the rows a
//!   view embeds;
//! * the client's filter, refused where the target query does not accept one (#1283), and its page,
//!   capped by `max_page_size` (#421).
//!
//! The per-level resolutions are the plan ([`ComposedPlan`]); the statement is lowered
//! from it and from nothing else.
//!
//! # Every level is joined
//!
//! A nested level is always read from its target's own view here (case (b) of the
//! nested-type ruling), because that is where its security predicate can be evaluated.
//! Projecting a level out of materialised parent data instead, and skipping a predicate
//! the parent's implies, need the RLS policy to declare which paths it constrains — a
//! decision taken at executor construction, which this plan does not yet carry.
//!
//! # Scored once, as the tree it is
//!
//! `[security.cost_budget] per_request_max` is charged once, before the statement is
//! sent, over the nested [`DirectReadProjection`] — which scores exactly what the
//! fan-out charged read by read, so the ceiling bounds the same request it did.
//! `[validation] max_response_bytes` is charged on what the statement returns.
//!
//! [`LevelKeys::Only`]: crate::backend::LevelKeys::Only
//! [`DirectReadProjection`]: crate::graphql::DirectReadProjection

use std::collections::HashMap;

use super::{
    query::QueryRunner,
    query_projection::field_type_to_where_type,
    query_regular::{DirectReadCost, GatedFieldHandling, ResolvedDirectRead},
};
use crate::{
    backend::{
        COMPOSED_DOCUMENT_KEY, COMPOSED_EMBEDS_KEY, ComposedEmbed, ComposedLevel, EmbedShape,
        EmbedSource, JsonbValue, LevelKeys, ScalarFieldType,
    },
    error::{FraiseQLError, Result},
    graphql::DirectReadProjection,
    runtime::{CountSelection, EmbedSelection, ResultProjector, matcher::QueryMatch},
    schema::{CompiledSchema, QueryDefinition, Relationship, TypeDefinition},
    security::SecurityContext,
};

/// A read and the reads composed beneath it, each resolved through every gate a direct
/// read passes — the plan a composed statement is lowered from.
pub(super) struct ComposedPlan {
    /// The level's read, as built for its query.
    query_match: QueryMatch,
    /// The level's read, resolved: its predicate, page, ordering and field access.
    resolved:    ResolvedDirectRead,
    /// What is embedded into each of its rows.
    embeds:      Vec<PlannedEmbed>,
}

/// One embedded relationship of a [`ComposedPlan`] level.
struct PlannedEmbed {
    /// The key the value is written under.
    output_key:   String,
    /// The relationship followed, as declared on the parent type.
    relationship: Relationship,
    /// How the related rows are read.
    target:       EmbedTarget,
}

/// How an embedded relationship's rows are read.
enum EmbedTarget {
    /// The related rows, as a level of their own.
    Rows(Box<ComposedPlan>),
    /// The number of related rows.
    Count(Box<ResolvedDirectRead>),
    /// The target type has no SQL-backed list query, so there is nothing to read and
    /// the relationship is answered empty — `[]`, `null` or `0` — as it always was.
    Unreadable,
}

impl QueryRunner {
    /// Execute `query_match` with `embeds` and `counts` composed into its statement.
    ///
    /// # Errors
    ///
    /// Everything `resolve_direct_read` returns, for the root read and for every embedded
    /// level; `FraiseQLError::Validation` for a relationship the parent type does not
    /// declare; the cost and response-bytes ceilings' refusals; and the adapter's errors,
    /// including `FraiseQLError::Unsupported` from an adapter that cannot compose.
    pub(in super::super) async fn execute_query_composed(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        request_budget: Option<&crate::security::RequestBudget>,
    ) -> Result<serde_json::Value> {
        let plan = self.resolve_composed_read(
            query_match,
            embeds,
            counts,
            variables,
            security_context,
            request_budget,
        )?;
        let read = plan.lower(&self.ctx.schema);
        let session_pairs = plan.resolved.session_pairs();

        // Refused from the capability the REST mount warned from, not from the adapter's
        // default method: the two answers are one flag. After every gate, so a caller the
        // gates refuse is told that first.
        if !self.ctx.adapter.supports_composed_reads() {
            return Err(crate::backend::composed_read_unsupported(&read.view));
        }
        let rows = self
            .ctx
            .adapter
            .execute_composed_with_session(
                &read,
                &session_pairs,
                query_match.query_def.read_routing,
            )
            .await?;

        // The response-bytes ceiling, charged on what came back — the same rule, and the
        // same shared-or-own budget, as `execute_query_direct`.
        let own_budget = if request_budget.is_none() {
            plan.resolved.budget()
        } else {
            None
        };
        if let Some(budget) = request_budget
            .and_then(crate::security::RequestBudget::bytes)
            .or(own_budget.as_ref())
        {
            budget.charge_jsonb_rows(&rows)?;
        }

        let composed: Vec<serde_json::Value> = rows.iter().map(|r| r.data.clone()).collect();
        let projected =
            self.project_composed(&plan, &composed, query_match.query_def.returns_list)?;
        Ok(ResultProjector::wrap_in_data_envelope(projected, query_match.response_key()))
    }

    /// Resolve the root read and every embedded level, then charge the cost of the
    /// tree — all before the statement exists.
    fn resolve_composed_read(
        &self,
        query_match: &QueryMatch,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
        request_budget: Option<&crate::security::RequestBudget>,
    ) -> Result<ComposedPlan> {
        let resolved = self.resolve_direct_read(
            query_match,
            variables,
            security_context,
            GatedFieldHandling::RefuseAsUnsupported,
            DirectReadCost::Composed,
        )?;
        let embeds = self.plan_embeds(
            &query_match.query_def.return_type,
            embeds,
            counts,
            variables,
            security_context,
        )?;
        let plan = ComposedPlan {
            query_match: query_match.clone(),
            resolved,
            embeds,
        };

        self.charge_direct_read_cost(
            &query_match.query_def.name,
            &plan.projection(),
            request_budget,
        )?;
        Ok(plan)
    }

    /// Resolve what is embedded into each row of `parent_type`.
    fn plan_embeds(
        &self,
        parent_type: &str,
        embeds: &[EmbedSelection],
        counts: &[CountSelection],
        variables: Option<&serde_json::Value>,
        security_context: Option<&SecurityContext>,
    ) -> Result<Vec<PlannedEmbed>> {
        let schema = &self.ctx.schema;
        let mut planned = Vec::with_capacity(embeds.len() + counts.len());

        for selection in embeds {
            let relationship = declared_relationship(schema, parent_type, &selection.relationship)?;
            let target = match list_query_for_type(schema, &relationship.target_type) {
                None => EmbedTarget::Unreadable,
                Some(target_query) => {
                    let query_match = level_query_match(
                        schema,
                        target_query,
                        selection.fields.clone(),
                        selection.filter.as_ref(),
                        selection.limit,
                    )?;
                    let resolved = self.resolve_direct_read(
                        &query_match,
                        variables,
                        security_context,
                        GatedFieldHandling::RefuseAsUnsupported,
                        DirectReadCost::Composed,
                    )?;
                    let embeds = self.plan_embeds(
                        &relationship.target_type,
                        &selection.embeds,
                        &selection.counts,
                        variables,
                        security_context,
                    )?;
                    EmbedTarget::Rows(Box::new(ComposedPlan {
                        query_match,
                        resolved,
                        embeds,
                    }))
                },
            };
            planned.push(PlannedEmbed {
                output_key: selection.output_key.clone(),
                relationship,
                target,
            });
        }

        for selection in counts {
            let relationship = declared_relationship(schema, parent_type, &selection.relationship)?;
            let target = match list_query_for_type(schema, &relationship.target_type) {
                None => EmbedTarget::Unreadable,
                Some(target_query) => {
                    let query_match = level_query_match(
                        schema,
                        target_query,
                        Vec::new(),
                        selection.filter.as_ref(),
                        None,
                    )?;
                    EmbedTarget::Count(Box::new(self.resolve_direct_read(
                        &query_match,
                        variables,
                        security_context,
                        GatedFieldHandling::RefuseAsUnsupported,
                        DirectReadCost::Composed,
                    )?))
                },
            };
            planned.push(PlannedEmbed {
                output_key: selection.output_key.clone(),
                relationship,
                target,
            });
        }

        Ok(planned)
    }

    /// Project a level's composed rows as a flat read of the same query projects its
    /// rows, then attach each row's embedded values, projected by their own level.
    fn project_composed(
        &self,
        plan: &ComposedPlan,
        rows: &[serde_json::Value],
        returns_list: bool,
    ) -> Result<serde_json::Value> {
        let documents: Vec<JsonbValue> = rows
            .iter()
            .map(|row| {
                JsonbValue::new(
                    row.get(COMPOSED_DOCUMENT_KEY).cloned().unwrap_or(serde_json::Value::Null),
                )
            })
            .collect();
        let mut projected = self.project_direct_rows(
            &plan.query_match,
            &plan.resolved.access,
            &documents,
            returns_list,
        )?;

        match &mut projected {
            serde_json::Value::Array(items) => {
                for (item, row) in items.iter_mut().zip(rows) {
                    self.attach_embeds(plan, item, row.get(COMPOSED_EMBEDS_KEY))?;
                }
            },
            serde_json::Value::Object(_) => {
                if let Some(row) = rows.first() {
                    self.attach_embeds(plan, &mut projected, row.get(COMPOSED_EMBEDS_KEY))?;
                }
            },
            _ => {},
        }
        Ok(projected)
    }

    /// Write each embedded value of one composed row into its projected parent object.
    fn attach_embeds(
        &self,
        plan: &ComposedPlan,
        item: &mut serde_json::Value,
        embedded: Option<&serde_json::Value>,
    ) -> Result<()> {
        let Some(object) = item.as_object_mut() else {
            return Ok(());
        };
        for embed in &plan.embeds {
            let raw = embedded.and_then(|e| e.get(&embed.output_key));
            let to_one = embed.relationship.cardinality.is_to_one();
            let value = match &embed.target {
                EmbedTarget::Unreadable if to_one => serde_json::Value::Null,
                EmbedTarget::Unreadable => serde_json::json!([]),
                EmbedTarget::Count(_) => raw.cloned().unwrap_or_else(|| serde_json::json!(0)),
                EmbedTarget::Rows(child) if to_one => match raw {
                    Some(row @ serde_json::Value::Object(_)) => {
                        self.project_composed(child, std::slice::from_ref(row), false)?
                    },
                    _ => serde_json::Value::Null,
                },
                EmbedTarget::Rows(child) => {
                    let rows =
                        raw.and_then(serde_json::Value::as_array).map_or(&[][..], Vec::as_slice);
                    self.project_composed(child, rows, true)?
                },
            };
            object.insert(embed.output_key.clone(), value);
        }
        Ok(())
    }
}

impl ComposedPlan {
    /// The level's projection, as the cost estimator scores it.
    ///
    /// A count is a level projecting nothing over no page, which scores 1 — so a count
    /// costs one unit per parent row. The fan-out's count sub-reads were charged
    /// nothing (`count_rows` has no cost gate); composed into the statement, the work is
    /// the statement's and is charged with it.
    fn projection(&self) -> DirectReadProjection {
        DirectReadProjection {
            leaf_fields: self.resolved.leaf_fields,
            limit:       self.resolved.limit,
            nested:      self
                .embeds
                .iter()
                .filter_map(|embed| match &embed.target {
                    EmbedTarget::Rows(child) => Some(child.projection()),
                    EmbedTarget::Count(_) => Some(DirectReadProjection::flat(0, None)),
                    EmbedTarget::Unreadable => None,
                })
                .collect(),
        }
    }

    /// Lower the plan to the adapter's call shape. The root reads its whole document,
    /// because it is projected afterwards exactly as a flat read is; every level beneath
    /// it reads only the keys its field access allows.
    fn lower(&self, schema: &CompiledSchema) -> ComposedLevel {
        self.lower_level(schema, LevelKeys::Whole)
    }

    fn lower_level(&self, schema: &CompiledSchema, keys: LevelKeys) -> ComposedLevel {
        let parent_type = &self.query_match.query_def.return_type;
        ComposedLevel {
            view: self.resolved.sql_source.clone(),
            projection: self.resolved.projection.as_ref().map(|h| h.projection_template.clone()),
            where_clause: self.resolved.composed_where.clone(),
            order_by: self.resolved.order_by.clone(),
            limit: self.resolved.limit,
            offset: self.resolved.offset,
            keys,
            embeds: self
                .embeds
                .iter()
                .filter_map(|embed| embed.lower(schema, parent_type))
                .collect(),
        }
    }
}

impl PlannedEmbed {
    /// The embedded level and its correlation, or `None` for a relationship there is
    /// nothing to read for.
    fn lower(&self, schema: &CompiledSchema, parent_type: &str) -> Option<ComposedEmbed> {
        let rel = &self.relationship;
        let (shape, level) = match &self.target {
            EmbedTarget::Unreadable => return None,
            EmbedTarget::Rows(child) => {
                let shape = if rel.cardinality.is_to_one() {
                    EmbedShape::One
                } else {
                    EmbedShape::Many
                };
                (shape, child.lower_level(schema, level_keys(&child.resolved)))
            },
            EmbedTarget::Count(resolved) => (
                EmbedShape::Count,
                ComposedLevel {
                    view:         resolved.sql_source.clone(),
                    projection:   None,
                    where_clause: resolved.composed_where.clone(),
                    order_by:     None,
                    limit:        None,
                    offset:       None,
                    keys:         LevelKeys::Only {
                        kept:   Vec::new(),
                        masked: Vec::new(),
                    },
                    embeds:       Vec::new(),
                },
            ),
        };

        Some(ComposedEmbed {
            output_key: self.output_key.clone(),
            shape,
            source: correlated_source(schema, rel, parent_type),
            level,
        })
    }
}

/// The correlation that attaches `rel`'s target rows to a row of `parent_type`: the two
/// join columns' stored keys, compared as the target key's declared type. REST's embeds
/// and a GraphQL selection joined through its relationship attach by this one rule.
pub(super) fn correlated_source(
    schema: &CompiledSchema,
    rel: &Relationship,
    parent_type: &str,
) -> EmbedSource {
    let target_field = schema
        .find_type(&rel.target_type)
        .and_then(|t| t.field_for_column(rel.target_join_column()));
    let key_type = target_field
        .map_or(ScalarFieldType::Text, |field| field_type_to_where_type(&field.field_type));
    EmbedSource::Correlated {
        target_key: vec![stored_key(
            schema,
            &rel.target_type,
            rel.target_join_column(),
        )],
        parent_key: vec![stored_key(schema, parent_type, rel.parent_join_column())],
        key_type,
    }
}

/// The keys an embedded level may return: every projected field's stored spellings,
/// with the masked fields' written as `NULL`.
fn level_keys(resolved: &ResolvedDirectRead) -> LevelKeys {
    let access = &resolved.access;
    let mut kept = Vec::new();
    let mut masked = Vec::new();
    for field in &access.projected {
        let into = if access.masked.contains(field) {
            &mut masked
        } else {
            &mut kept
        };
        let (stored, fallback) = crate::runtime::stored_key_candidates(field);
        into.push(stored);
        into.extend(fallback);
    }
    LevelKeys::Only { kept, masked }
}

/// The stored document key for `column` on `type_name`: the field that publishes the
/// column, in the `snake_case` spelling the `where` parser derives — the key a flat
/// read's scoping predicate on that column read.
fn stored_key(schema: &CompiledSchema, type_name: &str, column: &str) -> String {
    let declared = schema
        .find_type(type_name)
        .and_then(|t: &TypeDefinition| t.field_for_column(column))
        .map_or(column, |field| field.name.as_str());
    crate::utils::to_snake_case(declared)
}

/// The relationship `name` on `parent_type`.
fn declared_relationship(
    schema: &CompiledSchema,
    parent_type: &str,
    name: &str,
) -> Result<Relationship> {
    let parent = schema.find_type(parent_type).ok_or_else(|| FraiseQLError::Validation {
        message: format!("Type '{parent_type}' not found in schema"),
        path:    None,
    })?;
    parent.relationships.iter().find(|r| r.name == name).cloned().ok_or_else(|| {
        FraiseQLError::Validation {
            message: format!("Type '{parent_type}' has no relationship '{name}'"),
            path:    None,
        }
    })
}

/// A SQL-backed list query returning `type_name`.
///
/// A **function-backed** query has no relation to embed from, so it is skipped rather
/// than chosen (#1329): a type may declare both, and this picks the one that can be
/// read. A type whose only list query is function-backed embeds nothing.
fn list_query_for_type<'a>(
    schema: &'a CompiledSchema,
    type_name: &str,
) -> Option<&'a QueryDefinition> {
    schema
        .queries
        .iter()
        .find(|q| q.return_type == type_name && q.returns_list && q.function.is_none())
}

/// The read of one embedded level: the target's list query, over `fields`, narrowed by
/// the client's `filter` and paged by `limit`.
fn level_query_match(
    schema: &CompiledSchema,
    target_query: &QueryDefinition,
    fields: Vec<String>,
    filter: Option<&serde_json::Value>,
    limit: Option<u32>,
) -> Result<QueryMatch> {
    let mut arguments: HashMap<String, serde_json::Value> = HashMap::new();
    if let Some(filter) = filter.filter(|f| f.as_object().is_some_and(|m| !m.is_empty())) {
        arguments.insert("where".to_string(), filter.clone());
    }
    if let Some(limit) = limit {
        arguments.insert("limit".to_string(), serde_json::json!(limit));
    }
    QueryMatch::from_operation(
        target_query.clone(),
        fields,
        arguments,
        schema.find_type(&target_query.return_type),
    )
}

#[cfg(test)]
#[path = "query_composed_tests.rs"]
mod query_composed_tests;
