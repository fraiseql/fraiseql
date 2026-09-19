//! Execution of a function-backed root query field (#1329).
//!
//! The engine asks a [`QueryFunctionResolver`] for the field's *data* and does
//! everything else itself: the role and actor gates before, field-level RBAC,
//! selection projection, `__typename` stamping and the response cache after. That
//! division is the whole argument for resolving inside the engine rather than beside
//! it — a field answered outside would have to re-implement each of those and would
//! be wrong about one of them within a release.
//!
//! # What this path deliberately does not do
//!
//! * **No `sql_source`, so no WHERE composition.** RLS, `inject_params`, the client filter and the
//!   parent scoping predicate all lower into a statement this field does not issue. The compiler
//!   refuses each of them beside a `function` declaration rather than letting one compile into an
//!   artifact where nothing applies it — `inject_params` in particular, because dropping a scoping
//!   control does not break a field, it widens one.
//! * **No pagination.** `limit`/`offset`/`orderBy` are auto-params, refused for the same reason. A
//!   function that returns a page returns it whole.
//!
//! # The anonymous RLS gate does not apply here
//!
//! `execute_regular_query` refuses every anonymous read when an RLS policy is
//! configured, because the policy cannot be evaluated without a principal. This
//! field issues no read for a policy to govern: whatever the function reads goes
//! through the caller-scoped bridge, where that same gate applies to *that* read,
//! anonymously. So a function that reads fails closed on its own read, and one that
//! only computes still answers — which is the honest behaviour, and the only one
//! that lets an unauthenticated visitor be quoted a price.

use std::sync::Arc;

use tracing::debug;

use super::query::QueryRunner;
use crate::{
    backend::{traits::DatabaseAdapter, types::JsonbValue},
    error::{FraiseQLError, Result},
    runtime::{QueryFunctionRequest, ResultProjector, matcher::QueryMatch},
    security::SecurityContext,
};

impl<A: DatabaseAdapter> QueryRunner<A> {
    /// Answer a root query field from the function it declares (#1329).
    ///
    /// The caller has already matched the query and enforced `requires_role` and
    /// `requires_actor`; those gates are what decide *who* may reach a function at
    /// all, and they are the same gates on both the anonymous and authenticated
    /// paths.
    ///
    /// # Errors
    ///
    /// - [`FraiseQLError::Validation`] — no resolver is wired, or the function returned a shape
    ///   this field cannot be: an array for a single-item field, or a non-array for a list.
    /// - [`FraiseQLError::Authorization`] — a selected field is denied to this caller under
    ///   `on_deny = Reject`.
    /// - Anything the resolver returns.
    pub(in super::super) async fn execute_function_backed_query(
        &self,
        query_match: &QueryMatch,
        security_context: Option<&SecurityContext>,
    ) -> Result<serde_json::Value> {
        let query_def = &query_match.query_def;
        let field = query_def.name.as_str();
        // The caller branches on `function.is_some()`, so this cannot be `None`. It
        // is an error rather than an `expect` because "unreachable" is a claim about
        // today's call sites, and the next one need not know it.
        let function = query_def.function.as_deref().ok_or_else(|| FraiseQLError::Internal {
            message: format!("`{field}` reached the function runner with no function declared"),
            source:  None,
        })?;

        let resolver = self.ctx.config.query_function_resolver.as_ref().ok_or_else(|| {
            FraiseQLError::Validation {
                message: format!(
                    "`{field}` is backed by the function `{function}`, and this server has no \
                         function resolver wired, so the field cannot be answered. Build the \
                         server with the `functions-runtime` feature, or remove the binding and \
                         recompile the schema."
                ),
                path:    Some(field.to_string()),
            }
        })?;

        // #423's **dynamic** gate, in the two shapes the SQL path uses. It is not
        // subsumed by the static RBAC below: `requires_scope` is a claim about the
        // caller, and a policy-gated field is a per-row decision an authorizer makes.
        // A read path that skipped it would be a hole in #423 no test of the SQL path
        // could see, because the type — and therefore its gated fields — is the same
        // one either path returns.
        //
        // An anonymous caller has no principal for a policy to decide about, so a
        // selected gated field is refused outright rather than authorized. Refused here,
        // before the invocation, so an unauthorized request spends no isolate.
        let root_fields =
            query_match.selections.first().map_or(&[][..], |r| r.nested_fields.as_slice());
        let gated_present = crate::security::field_authorizer::selection_set_selects_gated_field(
            &self.ctx.schema,
            &query_def.return_type,
            root_fields,
        );
        if security_context.is_none() {
            crate::security::field_authorizer::deny_if_gated_field_selected(
                &self.ctx.schema,
                &query_def.return_type,
                root_fields,
                "unauthenticated function-backed query",
            )?;
        }

        // The response cache, on the same terms as every other read (#1329 Cycle 4).
        // It is not a nicety here: an invocation costs ~5–8 ms before the guest does
        // any work, so a field that answers from cache is a 5 ms field only on a miss.
        // The key is the same derivation every other read uses, so a dimension added
        // there reaches this cache too — including the security hash, which is what
        // stops one caller's answer reaching another.
        //
        // ⚠ `fraiseql-server` installs no `ResponseCache` today, so in the shipped
        // binary this is a miss on every request — for a SQL-backed read as much as
        // for this one (#1344). It is here because the alternative is a read path that
        // stays uncacheable the day the cache is wired, and because the row cache the
        // server *does* run is keyed by view and cannot cover a field that reads none.
        // That is also why `cache_ttl_seconds`, a row-cache TTL, is a compile error
        // beside a `function` rather than a number accepted and ignored.
        //
        // Invalidation comes from `additional_views`: with no `sql_source` there is
        // nothing for the invalidator to infer a read set from, so a function-backed
        // field that reads relations declares them, and one that reads none is
        // invalidated by nothing — which is correct, because nothing it returns
        // depends on a row.
        //
        // A selected policy-gated field is **not** cached, for the reason the SQL path
        // does not cache one either (D5b): the decision is per row and per principal,
        // so an entry would serve one caller's verdict to the next.
        let cache_key = self
            .ctx
            .response_cache
            .as_ref()
            .filter(|rc| rc.is_enabled() && !gated_present)
            .map(|_| {
                (
                    Self::compute_response_cache_key(query_match),
                    crate::cache::response_cache::hash_security_context(security_context),
                )
            });
        let cache_fence = self.ctx.response_cache.as_ref().map(|rc| rc.invalidation_generation());

        if let (Some((query_key, sec_hash)), Some(rc)) =
            (cache_key, self.ctx.response_cache.as_ref())
        {
            if let Some(cached) = rc.get(query_key, sec_hash)? {
                debug!(
                    target: "fraiseql::cache::response",
                    event = "hit", query = %field, query_key, sec_hash,
                    "response cache hit (function-backed)"
                );
                return Ok(Arc::unwrap_or_clone(cached));
            }
        }

        // Static field-level RBAC, classified **before** the invocation on both paths:
        // an `on_deny = Reject` refuses here, so a caller who may not read a selected
        // field never spends an isolate. `Mask` needs the value to exist before it can
        // null a key, so its half is applied after the projection below — the same
        // split the SQL path makes.
        let plan = self.ctx.planner.plan(query_match)?;
        let access = match security_context {
            Some(ctx) => super::super::support::security::apply_field_rbac_filtering(
                &self.ctx.schema,
                &query_def.return_type,
                plan.projection_fields.clone(),
                ctx,
            )?,
            None => super::super::support::security::apply_anonymous_field_rbac_filtering(
                &self.ctx.schema,
                &query_def.return_type,
                &plan.projection_fields,
            )?,
        };

        // The caller-scoped read bridge (#1328), built from this executor and this
        // principal — the same object a `before:mutation` hook reads through, and
        // deliberately not a second one.
        let reader = Arc::new(super::super::support::hook_reader::CallerScopedReader::new(
            Arc::clone(&self.ctx),
            security_context,
        ));

        // The matcher holds the resolved arguments in a `HashMap`, whose iteration
        // order is not stable across processes. Sorted into a `Map` here so the guest
        // sees the same payload for the same request every time — a function that
        // hashes or logs its input would otherwise produce a different answer per
        // process for identical input.
        let arguments = serde_json::Value::Object(
            query_match
                .arguments
                .iter()
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        );

        let value = resolver
            .resolve(QueryFunctionRequest {
                function,
                field,
                arguments: &arguments,
                principal: security_context,
                reader,
            })
            .await?;

        // The type's declared field names, for the "this document is not of that type"
        // refusal below. Through `find_type` so it uses the schema's own index rather
        // than a scan, and so a name the schema resolves case-insensitively resolves
        // the same way here.
        let declared: Vec<&str> = self
            .ctx
            .schema
            .find_type(&query_def.return_type)
            .map(|t| t.fields.iter().map(|f| f.name.as_str()).collect())
            .unwrap_or_default();
        let rows = Self::rows_from_function_result(
            field,
            function,
            value,
            query_def.returns_list,
            &query_def.return_type,
            &declared,
        )?;

        // The same projector the SQL path uses, over documents that came from a
        // function instead of a `data` column. Reused rather than re-implemented so
        // a function-backed field recases, stamps `__typename` and honours declared
        // scalars identically — the alternative is two projections that agree until
        // one of them is changed.
        let projector = ResultProjector::new(access.projected.clone())
            .with_declared_scalars(&self.ctx.schema, &query_def.return_type)
            .configure_typename_from_selections(&query_match.selections, &query_def.return_type);
        let mut projected = projector.project_results(&rows, query_def.returns_list)?;

        crate::runtime::project_nested_lists(
            &mut projected,
            &query_def.return_type,
            root_fields,
            &self.ctx.schema,
        );
        crate::runtime::stamp_nested_typenames(
            &mut projected,
            &query_def.return_type,
            root_fields,
            &self.ctx.schema,
        );
        if !access.masked.is_empty() {
            super::super::null_masked_fields(&mut projected, &access.masked);
        }

        // The dynamic authorizer runs last, over the rows the function returned, and
        // AND-composes with the static gate above — a field is shown only if both
        // allow. The anonymous arm never reaches here: it was refused before the
        // invocation.
        if let (true, Some(ctx)) = (gated_present, security_context) {
            self.apply_dynamic_field_authorizer(query_match, ctx, &access, &rows, &mut projected)?;
        }

        let response =
            ResultProjector::wrap_in_data_envelope(projected, query_match.response_key());

        if let (Some((query_key, sec_hash)), Some(rc)) =
            (cache_key, self.ctx.response_cache.as_ref())
        {
            let accessed = crate::cache::extract_accessed_views(query_def);
            let cached = Arc::new(response);
            let _ = rc.put(query_key, sec_hash, Arc::clone(&cached), accessed, cache_fence);
            return Ok(Arc::unwrap_or_clone(cached));
        }

        Ok(response)
    }

    /// Turn a function's return value into the rows the projector takes.
    ///
    /// The guest returns the field's **data**, not a GraphQL envelope: an entity
    /// document for a single-item field, an array of them for a list, `null` for an
    /// absent single item. Anything else is refused by name rather than coerced —
    /// a `null` silently accepted for a non-null list, or an object flattened into a
    /// one-element list, is a schema violation the client would see as a working
    /// response with the wrong shape.
    fn rows_from_function_result(
        field: &str,
        function: &str,
        value: serde_json::Value,
        returns_list: bool,
        return_type: &str,
        declared: &[&str],
    ) -> Result<Vec<JsonbValue>> {
        let refuse = |found: &str| FraiseQLError::Validation {
            message: format!(
                "`{field}` is backed by the function `{function}`, which returned {found}. A \
                 function-backed field returns the field's data: {} Return that, not a GraphQL \
                 response envelope.",
                if returns_list {
                    "this one is a list, so an array of objects."
                } else {
                    "this one is a single item, so one object (or null)."
                }
            ),
            path:    Some(field.to_string()),
        };

        // An object that shares **no** key with the declared type is not a document of
        // that type, and it is the one wrong shape the projector renders without
        // complaint: every selected field is absent, so the client receives
        // `{id: null, total: null}` and nothing says why. The commonest way to reach
        // it is returning a GraphQL envelope — `{data: {…}}` — which is an object, so
        // the shape check above passes it.
        //
        // Keyed on the type's declared fields rather than the selection set, so a
        // `{ __typename }`-only request does not refuse a perfectly good document.
        // One shared key is enough: a partially-populated document is the author's
        // business, an unrelated one is a bug.
        let unrelated = |object: &serde_json::Map<String, serde_json::Value>| {
            !declared.is_empty() && !object.keys().any(|k| declared.contains(&k.as_str()))
        };
        let refuse_unrelated = |object: &serde_json::Map<String, serde_json::Value>| {
            let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
            keys.sort_unstable();
            FraiseQLError::Validation {
                message: format!(
                    "`{field}` is backed by the function `{function}`, which returned an object \
                     with none of `{return_type}`'s fields: it has [{}] and the type declares \
                     [{}]. Every selected field would render as null. A function returns the \
                     entity document itself, not a `{{\"data\": …}}` envelope.",
                    keys.join(", "),
                    declared.join(", "),
                ),
                path:    Some(field.to_string()),
            }
        };

        match (value, returns_list) {
            (serde_json::Value::Array(items), true) => items
                .into_iter()
                .map(|item| match item {
                    serde_json::Value::Object(object) if unrelated(&object) => {
                        Err(refuse_unrelated(&object))
                    },
                    object @ serde_json::Value::Object(_) => Ok(JsonbValue::new(object)),
                    // A null element is not an empty row: the projector would render
                    // `{}` for it and the client would receive a list one item longer
                    // than the function meant to return.
                    _ => Err(refuse("an array containing a non-object element")),
                })
                .collect(),
            (serde_json::Value::Array(_), false) => Err(refuse("an array")),
            // An empty vector is how `project_results` spells "no row" for a
            // single-item field, and it is what a nullable field returns as `null`.
            (serde_json::Value::Null, false) => Ok(Vec::new()),
            (serde_json::Value::Null, true) => Err(refuse("null")),
            (serde_json::Value::Object(object), false) if unrelated(&object) => {
                Err(refuse_unrelated(&object))
            },
            (object @ serde_json::Value::Object(_), false) => Ok(vec![JsonbValue::new(object)]),
            (_, _) => Err(refuse("a scalar")),
        }
    }
}

#[cfg(test)]
#[path = "query_function_tests.rs"]
mod tests;
