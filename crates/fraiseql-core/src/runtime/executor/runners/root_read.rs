//! The root read: one pipeline for every door a root query field is read through (#1646).
//!
//! A root read reaches the database through the authenticated or anonymous GraphQL
//! document, the direct read (REST, gRPC, exports, embedding), the count, the Relay
//! connection or the `node(id:)` lookup. Each door used to resolve its read in its own
//! copy, and the copies drifted: an actor gate one door lacked (#966), a role gate (#1122),
//! a tenant filter resolved and thrown away (#739), a count that counted rows its list
//! did not return. This module is the one place those decisions are made:
//!
//! * [`OperationGate`] — the operation's own gates, in their one order: the #422 authorizer, an
//!   expired token, `requires_role`, `requires_actor`;
//! * [`row_predicate`] — the rows a read of a query may see: the RLS policy's predicate AND-ed with
//!   the query's `inject_params`, or [`RowScope::Hidden`] when there is no principal to evaluate
//!   them for (#784);
//! * [`QueryRunner::resolve_root_read`] — everything a root read decides before the database: the
//!   gate, the row scope, field-level RBAC, the composed `WHERE`, the page, the ordering and the
//!   projection.
//!
//! What still differs between doors is named by [`Door`], never inferred from which
//! transport is calling.

use super::{
    query::QueryRunner,
    query_nested::{LevelAuthz, Over, SelectionAccess},
    query_params::{
        apply_pagination_order, client_where_argument, coerce_pagination_arg,
        combine_explicit_arg_where, enforce_max_offset, enforce_max_page_size,
        inject_param_where_clause, nearest_order_and_limit,
    },
    query_projection::enrich_order_by_clauses,
};
use crate::{
    backend::{ComposedEmbed, OrderByClause, WhereClause},
    error::{FraiseQLError, Result},
    runtime::{
        executor::context::ExecutorContext, field_filter::FieldAccessResult, matcher::QueryMatch,
    },
    schema::{CompiledSchema, QueryDefinition, SqlProjectionHint},
    security::{
        RLSPolicy, SecurityContext,
        authorizer::{AuthzNesting, AuthzOperation, OperationKind, enforce_authz},
        rls_policy::RlsTarget,
    },
};

/// What a direct read's caller can do with a selected policy-gated field (#423, #1351).
///
/// #423 is a per-row decision, so where it is enforced depends on whether the caller
/// still has the rows in hand when it can be asked. This names that capability instead
/// of letting `resolve_root_read` test which transport is calling — a transport check
/// is what a new transport gets wrong, and #1351 exists because one did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GatedFieldHandling {
    /// Refuse the read outright when a gated field is selected.
    ///
    /// The JSON projection path: it resolves before the read and projects afterwards
    /// through a path that does not carry the authorizer, so a gated field has never
    /// been servable there. Fail-closed, unchanged since #423.
    RefuseAsUnsupported,
    /// Adjudicate each row against the authorizer after the read.
    ///
    /// The row-shaped path. The caller **must** apply the gate the row read plan
    /// hands back; the gate is on the plan rather than rebuilt per arm so that
    /// obligation travels with the thing it applies to.
    AdjudicatePerRow,
}

/// Who scores a direct read against `[security.cost_budget] per_request_max`, and when.
///
/// A read is either a statement of its own, scored flat and charged as it resolves, or
/// one level of a composed statement — which is scored as the tree it is part of, once,
/// by whoever composes it. Charging each level flat as well would charge the composed
/// statement twice, and charging it only per level would score a nested level without
/// the page it is multiplied by (`DirectReadProjection`).
#[derive(Debug, Clone, Copy)]
pub(super) enum DirectReadCost<'a> {
    /// Score this read flat and charge it: against the caller's request budget when it
    /// lends one, against a budget of this read's own otherwise.
    Flat(Option<&'a crate::security::RequestBudget>),
    /// This read is one level of a composed statement. Its composer scores the whole
    /// tree and charges it once, before the statement is sent.
    Composed,
}

/// The door a root read came through: what it decides that the others do not.
#[derive(Clone, Copy)]
pub(super) enum Door<'a> {
    /// A GraphQL document. The #422 authorizer was asked at dispatch, for every root of
    /// the document at once; a function-backed, Relay or count root is routed to its own
    /// runner after the operation gate; the pagination arguments are read only where the
    /// query's `auto_params` publish them.
    Document,
    /// A pre-resolved read: REST, gRPC, an export, an embedded level, a bulk selection.
    Direct {
        /// What happens to a selected policy-gated field (#423).
        gated:   GatedFieldHandling,
        /// Who scores the read against `per_request_max`.
        cost:    DirectReadCost<'a>,
        /// Where an embedded level sits, for the authorizer; `None` at the root.
        nesting: Option<&'a AuthzNesting>,
    },
    /// The cardinality of the rows a read would return: REST `count=exact`, a REST count
    /// embed and the GraphQL `<name>Count` sibling (#938).
    Count,
}

impl Door<'_> {
    /// Whether this door reads pagination arguments the query does not publish: a direct
    /// read's arguments are built by its transport, not by a client document.
    const fn pages_unconditionally(self) -> bool {
        matches!(self, Self::Direct { .. })
    }

    /// Whether this door reads the nested levels a selection scopes, as a composed
    /// statement rooted at this read. A level of a composed statement is planned by its
    /// composer, and a count selects nothing.
    const fn plans_nested_levels(self) -> bool {
        matches!(
            self,
            Self::Document
                | Self::Direct {
                    cost: DirectReadCost::Flat(_),
                    ..
                }
        )
    }
}

/// How the #422 authorizer is asked about an operation.
pub(in super::super) enum Ask {
    /// Ask it now, about this operation.
    Now(AuthzOperation),
    /// The caller asked it already, for this operation among others (the GraphQL document
    /// dispatch asks about every root before any runs).
    AtDispatch,
}

/// The gates an operation passes before anything is composed for it, in their one order.
///
/// The authorizer first, then an expired token, then `requires_role` — whose refusal is an
/// enumeration-hiding "not found" that no later gate may pre-empt
/// (`the_role_gate_still_runs_first`) — then `requires_actor`.
pub(in super::super) struct OperationGate<'a> {
    ctx: &'a ExecutorContext,
}

impl<'a> OperationGate<'a> {
    pub(in super::super) const fn new(ctx: &'a ExecutorContext) -> Self {
        Self { ctx }
    }

    /// Admit `query` for `principal`, or refuse it.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` when the authorizer denies or the actor type is not
    /// permitted; `FraiseQLError::Validation` for an expired token and, as "not found",
    /// for a missing role.
    pub(in super::super) fn check(
        &self,
        ask: Ask,
        query: &QueryDefinition,
        principal: Option<&SecurityContext>,
        input: Option<&serde_json::Value>,
    ) -> Result<()> {
        if let (Ask::Now(op), Some(authorizer)) = (ask, self.ctx.config.authorizer.as_ref()) {
            enforce_authz(authorizer.as_ref(), principal, &[op], input)?;
        }
        if principal.is_some_and(SecurityContext::is_expired) {
            return Err(FraiseQLError::Validation {
                message: "Security token has expired".to_string(),
                path:    Some("request.authorization".to_string()),
            });
        }
        crate::security::role_gate::enforce_requires_role(
            "Query",
            &query.name,
            query.requires_role.as_deref(),
            principal,
        )?;
        crate::security::actor_type::enforce_requires_actor(
            "Query",
            &query.name,
            &query.requires_actor,
            principal,
        )
    }
}

/// The rows a read of a query may see.
#[derive(Debug, Clone, PartialEq)]
pub(in super::super) enum RowScope {
    /// The rows this predicate selects; `None` is every row.
    Scoped(Option<WhereClause>),
    /// None: a predicate applies and there is no principal to evaluate it for.
    Hidden(HiddenBy),
}

/// Which predicate hid a read from an anonymous caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in super::super) enum HiddenBy {
    /// The RLS policy, which needs a principal to evaluate.
    Policy,
    /// The query's `inject_params`, which need a principal to resolve.
    InjectParams,
}

impl RowScope {
    /// The predicate, or the refusal a read of `query`'s rows answers an anonymous caller
    /// with: "not found" for a policy (#784), so the refusal does not tell a scoped query
    /// from an absent one, and the scoping named for `inject_params`, which the schema
    /// publishes anyway.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Validation` for a hidden scope.
    pub(in super::super) fn or_refuse(
        self,
        query: &str,
        target: &str,
    ) -> Result<Option<WhereClause>> {
        match self {
            Self::Scoped(clause) => Ok(clause),
            Self::Hidden(HiddenBy::Policy) => Err(FraiseQLError::Validation {
                message: format!("Query '{query}' not found in schema"),
                path:    None,
            }),
            Self::Hidden(HiddenBy::InjectParams) => Err(FraiseQLError::Validation {
                message: format!(
                    "Type '{target}' is scoped by the inject params of query '{query}', which \
                     a request without a security context cannot resolve"
                ),
                path:    None,
            }),
        }
    }
}

/// The rows a read of `target` through `read` may see: the policy's predicate, AND-ed with
/// `read`'s `inject_params`.
///
/// The one composition of the two. A root read, a Relay page, a node lookup, a nested
/// level, a subscription's after-image and a client filter crossing into a row-gated type
/// all ask it, so a read of a type is scoped the same way whichever door it came through.
/// `over` says whether a native `inject_params` column is read as the column (the type's
/// own rows) or as the document key of the same name (documents another view embeds).
///
/// # Errors
///
/// Whatever the policy returns, and an unresolvable `inject_params` source.
pub(in super::super) fn row_predicate(
    schema: &CompiledSchema,
    policy: Option<&dyn RLSPolicy>,
    read: &QueryDefinition,
    target: &str,
    over: Over,
    principal: Option<&SecurityContext>,
) -> Result<RowScope> {
    // An anonymous read of an injected query names the missing context before the
    // policy hides the query: the inject refusal is the one every door has answered.
    if principal.is_none() && !read.inject_params.is_empty() {
        return Ok(RowScope::Hidden(HiddenBy::InjectParams));
    }
    let mut conditions = Vec::with_capacity(read.inject_params.len() + 1);
    match (policy, principal) {
        (Some(policy), Some(principal)) => {
            if let Some(clause) =
                policy.evaluate(principal, &RlsTarget::query(&read.name, target))?
            {
                conditions.push(clause.into_where_clause());
            }
        },
        (Some(_), None) => return Ok(RowScope::Hidden(HiddenBy::Policy)),
        (None, _) => {},
    }
    if let (false, Some(principal)) = (read.inject_params.is_empty(), principal) {
        let in_documents = std::collections::HashMap::new();
        let native_columns = match over {
            Over::Rows => &read.native_columns,
            Over::Documents => &in_documents,
        };
        for (column, source) in &read.inject_params {
            let value = super::super::resolve_inject_value(
                column,
                source,
                principal,
                schema.tenant_claim(),
            )?;
            conditions.push(inject_param_where_clause(column, value, native_columns));
        }
    }
    Ok(RowScope::Scoped(and_all(conditions)))
}

/// `None`, the one clause, or their conjunction.
pub(super) fn and_all(mut conditions: Vec<WhereClause>) -> Option<WhereClause> {
    match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(WhereClause::And(conditions)),
    }
}

/// Where a document read goes after its operation gate.
pub(in super::super) enum RootRoute {
    /// A function-backed field (#1329): answered by its function, which issues no read.
    Function,
    /// A Relay connection: paged by the relay runner, which scopes its page through
    /// [`row_predicate`].
    Relay,
    /// A `<name>Count` sibling (#938): answered by `count_rows`.
    Count,
    /// Rows, read as resolved.
    Rows(Box<RootRead>),
}

/// A root read resolved down to the statement it will run (#958, #1646).
///
/// Produced by [`QueryRunner::resolve_root_read`] and consumed by every arm that reads
/// rows: the document read, the buffered and streamed direct read, the row-shaped read and
/// the count. Owned rather than borrowed from the `QueryMatch`, because the streaming path
/// outlives the call that built it.
pub(in super::super) struct RootRead {
    /// The view to read.
    pub(super) sql_source:     String,
    /// The row scope, the server's parent scoping, the client's filter and the explicit
    /// arguments, AND-ed in that order: the security conditions first, so nothing a
    /// client sends can widen them.
    pub(super) composed_where: Option<WhereClause>,
    /// Enriched ORDER BY, or `None` for the view's own order.
    pub(super) order_by:       Option<Vec<OrderByClause>>,
    /// Page size, already capped by `max_page_size` (#421).
    pub(super) limit:          Option<u32>,
    /// Page offset, already capped by `max_offset` (#1306).
    pub(super) offset:         Option<u32>,
    /// Session variables to pin to the read's connection (#329).
    pub(super) session_vars:   Vec<(String, String)>,
    /// How many fields the read projects — what the cost estimate counts as this
    /// level's scalars.
    pub(super) leaf_fields:    usize,
    /// Field-level RBAC classification for the projection (#886): the root level.
    pub access:                FieldAccessResult,
    /// The same classification at every level of the selection (`query_nested`).
    pub(super) selection:      SelectionAccess,
    /// Projection for the read, or `None` to read the stored document whole.
    pub(super) projection:     Option<SqlProjectionHint>,
    /// The nested levels whose type scopes its rows, read as levels of a composed
    /// statement rooted at this read. Empty: the read is flat.
    pub(super) nested:         Vec<ComposedEmbed>,
    /// A policy-gated field (#423) is selected and is adjudicated per row after the read.
    pub(super) gated_present:  bool,
    /// The read is typed as an interface or a union (#1612): a document read projects
    /// each row as the object type it is.
    pub(super) abstract_read:  bool,
    /// The `[validation] max_response_bytes` ceiling in force for this read, or
    /// `None` when the operator declared none.
    ///
    /// Resolved here, with every other decision that belongs to the read rather
    /// than to its delivery, and charged by whichever arm delivers the rows. The
    /// value cannot be checked at this point — what a read weighs is not knowable
    /// until it has run — so what travels is the ceiling, not a verdict.
    pub(super) response_bytes: Option<u64>,
    /// A `nearest` search's `k` (#1314), or `None` for any other read.
    pub(super) nearest_k:      Option<u32>,
    /// The bound a `nearest` search counts its filter's matches up to, in the same
    /// statement (`verify` / `refuse`), or `None`.
    pub(super) nearest_count:  Option<u32>,
}

impl RootRead {
    /// Borrow the session variables in the shape the adapter takes.
    pub(super) fn session_pairs(&self) -> Vec<(&str, &str)> {
        self.session_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    /// A fresh budget for this read, or `None` when no ceiling is configured.
    ///
    /// One per read, not one per frame: a streamed arm keeps the returned value for
    /// the whole stream so the ceiling bounds the response rather than each frame
    /// independently.
    pub(super) const fn budget(&self) -> Option<crate::security::ResponseBudget> {
        crate::security::ResponseBudget::new(self.response_bytes)
    }

    /// The adapter call shape for this read.
    pub(super) fn projection_request(&self) -> crate::backend::ProjectionRequest<'_> {
        crate::backend::ProjectionRequest {
            view:          &self.sql_source,
            projection:    self.projection.as_ref(),
            where_clause:  self.composed_where.as_ref(),
            order_by:      self.order_by.as_deref(),
            limit:         self.limit,
            offset:        self.offset,
            matched_up_to: self.nearest_count,
        }
    }

    /// This read as the root of a composed statement carrying its nested levels.
    pub(super) fn composed_level(&mut self) -> crate::backend::ComposedLevel {
        let embeds = std::mem::take(&mut self.nested);
        crate::backend::ComposedLevel {
            view: self.sql_source.clone(),
            projection: self.projection.as_ref().map(|h| h.projection_template.clone()),
            where_clause: self.composed_where.clone(),
            order_by: self.order_by.clone(),
            limit: self.limit,
            offset: self.offset,
            keyset: None,
            keys: super::query_nested::root_keys(&embeds),
            embeds,
        }
    }
}

impl QueryRunner {
    /// Resolve a root read down to the statement it will run — everything that happens
    /// *before* the database, for every door, in one place (#958, #1646).
    ///
    /// # Why this is one function
    ///
    /// Every security decision — the operation gate, the field-authorization gate, the
    /// row scope, the field-level RBAC classification, the page caps — belongs to the
    /// *read*, not to the door it came through. Four copies of this function used to
    /// make them, and the copies disagreed: one composed the tenant filter and another
    /// resolved it and threw it away (#739), one carried an actor gate and its peer did
    /// not (#1166), a count ignored the arguments its list was filtered by. A door that
    /// reads rows resolves through this function or it does not exist.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` when the operation, a gated field or a
    /// scope-restricted field is refused; `FraiseQLError::Validation` for an expired
    /// token, a missing role ("not found"), a query with no SQL source, a row scope with
    /// no principal to evaluate it for (#784), an argument that does not parse or a page
    /// over its ceiling.
    pub(super) fn resolve_root_read(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        principal: Option<&SecurityContext>,
        door: Door<'_>,
    ) -> Result<RootRoute> {
        let query_def = &query_match.query_def;
        let schema = &self.ctx.schema;
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());

        // 1. The operation gate. A document's authorizer question was asked at dispatch; any other
        //    door asks it here — the REST, gRPC, export and embedding reads and the count all
        //    funnel through this function, so gating here is leak-proof. An embedded level is asked
        //    as a nested read of its type.
        let ask = match door {
            Door::Document => Ask::AtDispatch,
            Door::Direct {
                nesting: Some(nesting),
                ..
            } => Ask::Now(AuthzOperation::nested(
                &query_def.name,
                &query_def.return_type,
                nesting.clone(),
            )),
            Door::Direct { .. } | Door::Count => Ask::Now(AuthzOperation::root(
                OperationKind::Query,
                &query_def.name,
                Some(query_def.return_type.as_str()),
            )),
        };
        OperationGate::new(&self.ctx).check(ask, query_def, principal, variables)?;

        // 2. A document's root that is not a plain read goes to its own runner, gated above and
        //    before anything composes a statement for it.
        if matches!(door, Door::Document) {
            if query_def.function.is_some() {
                return Ok(RootRoute::Function);
            }
            if query_def.relay {
                return Ok(RootRoute::Relay);
            }
            if query_def.returns_count {
                return Ok(RootRoute::Count);
            }
        }

        // 3. A selected policy-gated field (#423): what happens to it depends on what the caller
        //    can do with the rows, never on which transport it is. A document read with a principal
        //    adjudicates each row after the read; the row-shaped read does the same against the row
        //    it encodes; every other read cannot — the decision is per row and needs a principal —
        //    and fails closed.
        let gated_present = match door {
            Door::Document if principal.is_some() => {
                crate::security::field_authorizer::selection_set_selects_gated_field(
                    schema,
                    &query_def.return_type,
                    root_fields,
                )
            },
            Door::Document => {
                crate::security::field_authorizer::deny_if_gated_field_selected(
                    schema,
                    &query_def.return_type,
                    root_fields,
                    "unauthenticated query",
                )?;
                false
            },
            Door::Direct {
                gated: GatedFieldHandling::RefuseAsUnsupported,
                ..
            } => {
                crate::security::field_authorizer::deny_if_gated_field_selected(
                    schema,
                    &query_def.return_type,
                    root_fields,
                    "REST",
                )?;
                false
            },
            Door::Direct {
                gated: GatedFieldHandling::AdjudicatePerRow,
                ..
            }
            | Door::Count => false,
        };

        // 4. #1612: a document read typed as an interface or a union is a read of each object type
        //    its rows may be (`query_abstract`).
        let abstract_read = match door {
            Door::Document => super::query_abstract::AbstractRead::of(schema, query_def),
            Door::Direct { .. } | Door::Count => None,
        };
        if let Some(read) = &abstract_read {
            read.refuse_gated_fields(schema, root_fields)?;
        }

        // 5. The row scope, before anything reads the selection: a caller a policy hides the rows
        //    from learns "not found" before any field gate can answer otherwise.
        let row_scope = row_predicate(
            schema,
            self.ctx.config.rls_policy.as_deref(),
            query_def,
            &query_def.return_type,
            Over::Rows,
            principal,
        )?
        .or_refuse(&query_def.name, &query_def.return_type)?;

        let sql_source =
            query_def.sql_source.as_ref().ok_or_else(|| FraiseQLError::Validation {
                message: "Query has no SQL source".to_string(),
                path:    None,
            })?;

        // A count reads no field, so it has no projection to plan.
        let plan = match door {
            Door::Count => None,
            Door::Document | Door::Direct { .. } => Some(self.ctx.planner.plan(query_match)?),
        };

        // 6. Field-level RBAC at every level of the selection, each against its own type and by
        //    field name (`query_nested`). Before the read, so a `Reject` anywhere in the tree never
        //    reaches the database. A count selects nothing.
        let authz = LevelAuthz::from_config(&self.ctx.config, variables);
        let selection = match (&abstract_read, &plan) {
            (Some(read), _) => read.classify(schema, root_fields, principal, authz)?,
            (None, Some(plan)) => SelectionAccess::classify(
                schema,
                &query_def.return_type,
                root_fields,
                plan.projection_fields.clone(),
                principal,
                authz,
            )?,
            (None, None) => SelectionAccess::classify(
                schema,
                &query_def.return_type,
                &[],
                Vec::new(),
                principal,
                authz,
            )?,
        };

        // 7. The nested levels whose type scopes its rows, each read with that type's predicate as
        //    a level of a composed statement rooted at this read.
        let nested = if door.plans_nested_levels() {
            self.plan_nested_reads(&query_def.return_type, root_fields, principal, &selection)?
        } else {
            Vec::new()
        };

        // 8. The composed `WHERE`: the row scope, then the server's own parent scoping (#1170),
        //    then the client's filter — refused, never dropped, where the query does not accept one
        //    (#1283) — then the explicit arguments (`user(id:)`). Security first, so a
        //    client-supplied condition can only narrow the rows.
        let scoped = self.and_scope_where(row_scope, query_match)?;
        let client = client_where_argument(
            schema,
            query_def,
            &query_match.arguments,
            self.ctx.config.rls_policy.as_deref(),
            principal,
        )?;
        let composed_where = combine_explicit_arg_where(
            and_all(scoped.into_iter().chain(client).collect()),
            &query_def.arguments,
            &query_match.arguments,
            &query_def.native_columns,
        );

        let session_vars = self.resolve_session_vars(principal)?;
        let Some(plan) = plan else {
            return Ok(RootRoute::Rows(Box::new(RootRead {
                sql_source: sql_source.clone(),
                composed_where,
                order_by: None,
                limit: None,
                offset: None,
                session_vars,
                leaf_fields: 0,
                access: selection.root.clone(),
                selection,
                projection: None,
                nested,
                gated_present,
                abstract_read: false,
                response_bytes: None,
                nearest_k: None,
                nearest_count: None,
            })));
        };

        // 9. The page: capped by `max_page_size` (#421) and `max_offset` (#1306) before any
        //    statement. A document reads the arguments its query publishes.
        let pages = |published: bool| door.pages_unconditionally() || published;
        let limit = enforce_max_page_size(
            if pages(query_def.auto_params.has_limit) {
                coerce_pagination_arg("limit", query_match.arguments.get("limit"))?
            } else {
                None
            },
            self.ctx.config.max_page_size,
            "limit",
        )?;
        let offset = enforce_max_offset(
            if pages(query_def.auto_params.has_offset) {
                coerce_pagination_arg("offset", query_match.arguments.get("offset"))?
            } else {
                None
            },
            self.ctx.config.max_offset,
            "offset",
        )?;

        // 10. The ordering, each clause enriched with its field's type so the SQL casts it
        //     (`(data->>'amount')::numeric`).
        let order_by = if pages(query_def.auto_params.has_order_by) {
            query_match
                .arguments
                .get("orderBy")
                .map(OrderByClause::from_graphql_json)
                .transpose()?
                .map(|clauses| {
                    enrich_order_by_clauses(
                        clauses,
                        schema,
                        &query_def.return_type,
                        &query_def.native_columns,
                        principal,
                    )
                })
                .transpose()?
        } else {
            None
        };

        // `nearest` similarity search (#386): a vector-distance ORDER BY + LIMIT k. Its
        // conflicts with `limit`/`orderBy` are refused inside the helper, so overriding
        // both here cannot discard a client value.
        let (limit, order_by, nearest_k) =
            match nearest_order_and_limit(&query_match.arguments, schema, query_def, principal)? {
                Some((clause, k)) => (
                    enforce_max_page_size(Some(k), self.ctx.config.max_page_size, "nearest.k")?,
                    Some(vec![clause]),
                    Some(k),
                ),
                None => (limit, order_by, None),
            };

        // 11. #379: a direct read never had a document for GATE-1 to score, so it is scored here,
        //     before the database — flat, against the caller's request budget when it lends one. A
        //     level of a composed statement is scored as the tree it is part of, once, by its
        //     composer (`DirectReadCost`).
        if let Door::Direct {
            cost: DirectReadCost::Flat(request_budget),
            ..
        } = door
        {
            self.charge_direct_read_cost(
                &query_def.name,
                &crate::graphql::DirectReadProjection::flat(plan.projection_fields.len(), limit),
                request_budget,
            )?;
        }

        // Full-text relevance (#1284): a `?search=` read with no sort of its own is ranked
        // by `ts_rank`. A server value (`QueryMatch::search_relevance`), not a client one,
        // and a clause list that exists wins, so a client's sort is never replaced.
        let order_by = match (order_by, query_match.search_relevance.as_ref()) {
            (None, Some(relevance)) => Some(vec![OrderByClause::by_relevance(relevance.clone())]),
            (existing, _) => existing,
        };

        // 12. The projection, after the `nearest` lowering so a selected distance field is
        //     projected from the clause that ordered the rows. A direct read projects in Rust (its
        //     selection may name fields the generator does not model), and so does a document read
        //     whose rows are adjudicated, merged with their nested levels or typed abstractly: each
        //     reads the stored document whole.
        let full_row = match door {
            Door::Document => gated_present || !nested.is_empty() || abstract_read.is_some(),
            Door::Direct { .. } | Door::Count => true,
        };
        let projection = self.build_projection_hint(
            query_match,
            &plan,
            full_row,
            order_by.as_ref().and_then(|c| c.first()),
        )?;

        // 13. #1303: the total order this query declares, when the read is paged. After the
        //     projection, which reads the ordering's first clause to project a distance.
        let order_by = apply_pagination_order(order_by, query_def, limit, offset);

        // #1314: a composed read has no counting statement; its short result is signalled
        // unverified.
        let nearest_count = if nested.is_empty() {
            self.nearest_count_bound(nearest_k)
        } else {
            None
        };

        Ok(RootRoute::Rows(Box::new(RootRead {
            sql_source: sql_source.clone(),
            composed_where,
            order_by,
            limit,
            offset,
            session_vars,
            leaf_fields: plan.projection_fields.len(),
            access: selection.root.clone(),
            selection,
            projection,
            nested,
            gated_present,
            abstract_read: abstract_read.is_some(),
            response_bytes: self.ctx.config.max_response_bytes,
            nearest_k,
            nearest_count,
        })))
    }

    /// [`resolve_root_read`](Self::resolve_root_read) for a door that only ever reads
    /// rows (a direct read, a count): its route is always [`RootRoute::Rows`].
    ///
    /// # Errors
    ///
    /// As [`resolve_root_read`](Self::resolve_root_read).
    pub(super) fn resolve_rows(
        &self,
        query_match: &QueryMatch,
        variables: Option<&serde_json::Value>,
        principal: Option<&SecurityContext>,
        door: Door<'_>,
    ) -> Result<RootRead> {
        match self.resolve_root_read(query_match, variables, principal, door)? {
            RootRoute::Rows(read) => Ok(*read),
            RootRoute::Function | RootRoute::Relay | RootRoute::Count => {
                Err(FraiseQLError::Internal {
                    message: format!(
                        "the read of '{}' was routed away from its rows on a door that only \
                         reads rows — this is a bug",
                        query_match.query_def.name
                    ),
                    source:  None,
                })
            },
        }
    }
}
