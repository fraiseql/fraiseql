//! A subscription's read plan (ruling AA 4, settled in AC 2–5).
//!
//! A subscription is a read of its type delivered by push, so it meets the gates a read of
//! that type through the same selection meets. The executor plans it at subscribe time, from
//! the client's resolved selection and the subscriber's principal — the transport hands the
//! document over and never classifies — and the plan is applied to every event's after-image
//! (the document the write returned: the only source, nothing is fetched).
//!
//! At subscribe time ([`Executor::plan_subscription`]) the plan refuses what a read would
//! refuse: a field the type does not declare, one outside the subscription's compile-time
//! field list, the type's own `requires_role`, whatever the read plan refuses (a `Reject`
//! field, a nested level's role, actor or #422 decision, a nested row policy that cannot be
//! evaluated over the document, the #423 refusals that need no document), and a filter on a
//! field the subscriber may not read (ruling AA 3).
//!
//! Per event ([`SubscriptionPlan::deliver`]): the root after-image must be a row the type's
//! own row policy admits for this subscriber (ruling AC 4 — a subscription is a read of its
//! type; a policy the document cannot answer refuses the subscription instead); nested
//! levels row-filtered, projected through
//! the selection, masked, then put to the #423 authorizer. Whatever refuses the event
//! suppresses it for this subscriber: no frame, nothing the subscriber can tell from no event
//! at all. Suppressions are counted in one aggregate figure
//! ([`suppressed_subscription_events`]).

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use super::{
    context::ExecutorContext,
    core::Executor,
    runners::{
        query_nested::{RootRowFilter, own_read, type_read_refusal, whole_object},
        read_plan::ReadPlan,
    },
    support::security::refuse_unreadable_where,
};
use crate::{
    error::{FraiseQLError, Result},
    graphql::{FieldSelection, ParsedQuery},
    runtime::projection::effective_selections,
    schema::{CompiledSchema, SubscriptionDefinition},
    security::SecurityContext,
};

/// Events a subscription plan suppressed, since the process started. One figure for the
/// whole process: a per-subscriber or per-type count would tell who is refused what.
static SUPPRESSED: AtomicU64 = AtomicU64::new(0);

/// How many events subscription plans have suppressed since the process started.
#[must_use]
pub fn suppressed_subscription_events() -> u64 {
    SUPPRESSED.load(Ordering::Relaxed)
}

/// A subscription planned for one subscriber: what each event is served as.
pub struct SubscriptionPlan {
    ctx:          Arc<ExecutorContext>,
    plan:         ReadPlan,
    root_rows:    Option<RootRowFilter>,
    principal:    Option<SecurityContext>,
    subscription: String,
    type_name:    String,
    selections:   Vec<FieldSelection>,
    variables:    HashMap<String, serde_json::Value>,
    /// The subscriber's request locale, captured when the plan was made (#1513).
    locale:       Option<String>,
}

impl std::fmt::Debug for SubscriptionPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionPlan")
            .field("subscription", &self.subscription)
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

impl SubscriptionPlan {
    /// The subscription this plan serves.
    #[must_use]
    pub fn subscription_name(&self) -> &str {
        &self.subscription
    }

    /// The subscriber's own identity in the form a Change-Spine `acting_for` is stamped in
    /// (a UUID `sub`, ruling AI): `None` for an anonymous plan or a subject that is not a
    /// UUID, which no delegation can name.
    #[must_use]
    pub(crate) fn subscriber_uuid(&self) -> Option<uuid::Uuid> {
        self.principal
            .as_ref()
            .and_then(|who| uuid::Uuid::parse_str(who.user_id.as_str()).ok())
    }

    /// Serve one event's after-image to this subscriber, or `None` when the plan suppresses
    /// it.
    #[must_use]
    pub fn deliver(&self, after_image: &serde_json::Value) -> Option<serde_json::Value> {
        if self.root_rows.as_ref().is_some_and(|rows| !rows.admits(after_image)) {
            SUPPRESSED.fetch_add(1, Ordering::Relaxed);
            return None;
        }
        if let Ok(served) = crate::runtime::with_request_locale_sync(self.locale.clone(), || {
            self.plan.serve(
                &self.ctx,
                self.principal.as_ref(),
                &self.type_name,
                &self.selections,
                after_image,
                &self.variables,
            )
        }) {
            Some(served)
        } else {
            SUPPRESSED.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

impl Executor {
    /// Plan a subscription for one subscriber: `document` is the subscription operation as
    /// the client sent it, `variables` its variables, `principal` the subscriber.
    ///
    /// # Errors
    ///
    /// `Validation` for a document that selects no subscription or more than one, an unknown
    /// subscription, a field the type does not declare or the subscription does not deliver;
    /// `Authorization` for the type's role, a filter on a field the subscriber may not read,
    /// and whatever the read plan refuses.
    pub fn plan_subscription(
        &self,
        document: &ParsedQuery,
        variables: Option<&serde_json::Value>,
        principal: Option<&SecurityContext>,
    ) -> Result<SubscriptionPlan> {
        let ctx = &self.ctx;
        let schema = &ctx.schema;
        // § 6.4.1 (#1504): an omitted variable takes its declared default, so a
        // defaulted argument binds its filter as a supplied one does.
        let defaults = crate::graphql::value_json::variable_defaults(&document.variables)?;
        let defaulted = crate::graphql::value_json::with_variable_defaults(&defaults, variables);
        let variables_map =
            crate::graphql::selection_set::variables_map(defaulted.as_ref().or(variables));
        let resolved = crate::graphql::selection_set::resolve_and_filter(
            &document.selections,
            &document.fragments,
            &variables_map,
            self.max_query_depth(),
        )?;
        let mut roots = resolved.iter().filter(|s| s.name != "__typename");
        let (Some(root), None) = (roots.next(), roots.next()) else {
            return Err(FraiseQLError::Validation {
                message: "a subscription operation selects exactly one root field".to_string(),
                path:    None,
            });
        };
        let definition =
            schema.find_subscription(&root.name).ok_or_else(|| FraiseQLError::Validation {
                message: format!("Subscription '{}' not found in schema", root.name),
                path:    None,
            })?;
        let type_name = definition.return_type.as_str();

        refuse_undeclared_selection(schema, type_name, &root.nested_fields)?;
        refuse_undelivered_selection(schema, definition, &root.nested_fields)?;
        // #1513: a localized field's `locale:` and translations sibling are adjudicated
        // here, as for a query, before the subscription registers.
        let mut selections = root.nested_fields.clone();
        crate::runtime::resolve_locale_arguments(
            schema,
            type_name,
            &mut selections,
            &variables_map,
            false,
        )?;
        self.refuse_unless_readable(&definition.name, type_name, principal)?;
        // The filter binds the arguments the document gave the root field, however spelled
        // (#1158): reading the variables by argument name missed an inline value and a
        // variable of another name, and with them this check.
        let arguments: HashMap<String, serde_json::Value> =
            crate::runtime::subscription::subscription_arguments(
                schema,
                document,
                variables,
                self.max_query_depth(),
            )?
            .into_iter()
            .collect();
        for path in active_filter_paths(definition, &arguments) {
            let condition = crate::db::WhereClause::Field {
                path,
                operator: crate::db::WhereOperator::Eq,
                value: serde_json::Value::Null,
            };
            refuse_unreadable_where(schema, type_name, &condition, principal)?;
        }
        self.plan_read(&definition.name, type_name, selections, variables_map, principal)
    }

    /// Plan a stream of `type_name`'s change events for one reader — the REST
    /// `/{resource}/stream` (ruling AC 7): a read of the type through the selection a `GET`
    /// of the resource with no `?select=` reads, every field the type declares, object
    /// fields expanded as their type.
    ///
    /// # Errors
    ///
    /// `Validation` for a type nesting an object deeper than a whole-object read expands;
    /// `Authorization` for the type's role and whatever the read plan refuses — a `Reject`
    /// field the reader may not read refuses the stream, as it refuses the `GET`.
    pub fn plan_type_stream(
        &self,
        type_name: &str,
        principal: Option<&SecurityContext>,
    ) -> Result<SubscriptionPlan> {
        let selections = whole_object(&self.ctx.schema, type_name, 0)?;
        // #422, as the resource's `GET` asks it: a root read of the type's own read.
        if let Some(authorizer) = self.ctx.config.authorizer.as_ref() {
            let name = own_read(&self.ctx.schema, type_name).map_or(type_name, |q| q.name.as_str());
            let ops = [crate::security::AuthzOperation::root(
                crate::security::OperationKind::Query,
                name,
                Some(type_name),
            )];
            crate::security::authorizer::enforce_authz(authorizer.as_ref(), principal, &ops, None)?;
        }
        self.refuse_unless_readable(type_name, type_name, principal)?;
        self.plan_read(type_name, type_name, selections, HashMap::new(), principal)
    }

    /// Refuse `type_name` to a reader its own read refuses: that read's `requires_role` (else
    /// the type's) and its `requires_actor` — the gates a nested level of the type applies.
    fn refuse_unless_readable(
        &self,
        read: &str,
        type_name: &str,
        principal: Option<&SecurityContext>,
    ) -> Result<()> {
        match type_read_refusal(&self.ctx.schema, type_name, principal) {
            None => Ok(()),
            Some(why) => Err(FraiseQLError::Authorization {
                message:  format!("'{read}' delivers '{type_name}', {why}"),
                action:   Some("read".to_string()),
                resource: Some(type_name.to_string()),
            }),
        }
    }

    /// The row predicate and the read plan of `selections` over `type_name`, for `read`.
    fn plan_read(
        &self,
        read: &str,
        type_name: &str,
        selections: Vec<FieldSelection>,
        variables: HashMap<String, serde_json::Value>,
        principal: Option<&SecurityContext>,
    ) -> Result<SubscriptionPlan> {
        let ctx = &self.ctx;
        let root_rows = RootRowFilter::plan(ctx, read, type_name, principal)?;
        let bound = serde_json::Value::Object(variables.clone().into_iter().collect());
        let plan = ReadPlan::classify(ctx, principal, Some(&bound), &[(type_name, &selections)])?;
        Ok(SubscriptionPlan {
            ctx: Arc::clone(ctx),
            plan,
            root_rows,
            principal: principal.cloned(),
            subscription: read.to_string(),
            type_name: type_name.to_string(),
            selections,
            variables,
            // #1513: events are delivered on the connection's task, outside any request
            // scope; the plan serves them in the locale it was planned in.
            locale: crate::runtime::scoped_request_locale(),
        })
    }
}

/// Refuse a selected field `type_name` does not declare, at every level, as `/graphql`
/// validation does. A level whose type the schema does not describe as an object (a union or
/// an interface, resolved per fragment) is left to its fragments.
fn refuse_undeclared_selection(
    schema: &CompiledSchema,
    type_name: &str,
    selections: &[FieldSelection],
) -> Result<()> {
    let Some(type_def) = schema.find_type(type_name) else {
        return Ok(());
    };
    for sel in effective_selections(selections, type_name, schema) {
        // A translations sibling is answered by the schema (#1513).
        if sel.name == "__typename" || type_def.translations_of(&sel.name).is_some() {
            continue;
        }
        let Some(field) = type_def.fields.iter().find(|f| f.name == sel.name) else {
            return Err(FraiseQLError::Validation {
                message: format!("Cannot query field '{}' on type '{type_name}'", sel.name),
                path:    None,
            });
        };
        if let Some(child) = field.field_type.inner_type().unwrap_or(&field.field_type).type_name()
        {
            refuse_undeclared_selection(schema, child, &sel.nested_fields)?;
        }
    }
    Ok(())
}

/// Refuse a top-level field outside the subscription's compile-time field list, when it
/// declares one: the list is an upper bound on what the subscription delivers.
fn refuse_undelivered_selection(
    schema: &CompiledSchema,
    definition: &SubscriptionDefinition,
    selections: &[FieldSelection],
) -> Result<()> {
    if definition.fields.is_empty() {
        return Ok(());
    }
    let delivered: Vec<&str> = definition
        .fields
        .iter()
        .filter_map(|f| f.trim_start_matches('/').split(['/', '.']).next())
        .collect();
    // A translations sibling is delivered with its localized field (#1513).
    let type_def = schema.find_type(&definition.return_type);
    let delivered_name = |name: &str| {
        let base = type_def.and_then(|t| t.translations_of(name)).map(|f| f.name.as_str());
        delivered.contains(&base.unwrap_or(name))
    };
    match selections.iter().find(|s| s.name != "__typename" && !delivered_name(&s.name)) {
        Some(sel) => Err(FraiseQLError::Validation {
            message: format!(
                "Subscription '{}' does not deliver field '{}'",
                definition.name, sel.name
            ),
            path:    None,
        }),
        None => Ok(()),
    }
}

/// The stored-key paths the subscription's filters read for this subscriber: a
/// `filter_fields` entry or an `argument_paths` entry whose argument the subscriber bound,
/// and every static filter.
fn active_filter_paths(
    definition: &SubscriptionDefinition,
    variables: &HashMap<String, serde_json::Value>,
) -> Vec<Vec<String>> {
    let bound = |argument: &str| variables.get(argument).is_some_and(|v| !v.is_null());
    let segments = |path: &str| -> Vec<String> {
        path.trim_start_matches('/')
            .split(['/', '.'])
            .filter(|s| !s.is_empty())
            .map(crate::utils::to_snake_case)
            .collect()
    };
    let mut paths: Vec<Vec<String>> = definition
        .filter_fields
        .iter()
        .filter(|field| bound(field))
        .map(|field| segments(field))
        .collect();
    if let Some(filter) = &definition.filter {
        paths.extend(
            filter
                .argument_paths
                .iter()
                .filter(|(argument, _)| bound(argument))
                .map(|(_, path)| segments(path)),
        );
        paths.extend(filter.static_filters.iter().map(|condition| segments(&condition.path)));
    }
    paths
}

#[cfg(test)]
#[path = "subscription_plan_tests.rs"]
mod tests;
