//! The read plan: a selection's read gates, decided before the data exists and applied to
//! each document when it arrives (ruling AA 2).
//!
//! A read of a type through a selection meets the same gates wherever the documents come
//! from — a view, the document a write function returned, a change event:
//!
//! * `requires_scope`, at every level, the root included — a `Mask` nulls the field, a `Reject`
//!   refuses the request;
//! * a nested level's read gates — its type's `requires_role` and `requires_actor`, and the #422
//!   authorizer, asked of it as a read of its type;
//! * a nested level's row security, over the documents the level embeds ([`DocumentRowFilter`]);
//! * the #423 field authorizer: its refusals that follow from the selection, the principal and the
//!   configuration are decided with the plan; its decision over a document when the document is
//!   served.
//!
//! [`ReadPlan::classify`] decides everything that can refuse from the selection and the
//! principal alone; [`ReadPlan::serve`] applies the rest to one document. Mutation payloads
//! are its first user (`mutation::payload_gates`).

use std::borrow::Cow;

use super::{
    super::context::ExecutorContext,
    query_nested::{DocumentRowFilter, LevelAuthz, SelectionAccess},
};
use crate::{
    error::{FraiseQLError, Result},
    graphql::FieldSelection,
    runtime::project_entity,
    security::SecurityContext,
};

/// What the static gates decided for a selection over one or more root types.
pub(in super::super) struct ReadPlan {
    access: SelectionAccess,
    rows:   DocumentRowFilter,
}

impl ReadPlan {
    /// Classify each `(type, selections)` root: every level against its own type.
    ///
    /// # Errors
    ///
    /// `FraiseQLError::Authorization` for what the read path refuses — a `Reject` field, a
    /// nested level whose read the caller may not make, one the #422 authorizer denies — for
    /// a nested level whose row security cannot be evaluated over the document, and for a
    /// gated field the #423 authorizer cannot be asked about ([`field_authz_inputs`]).
    pub(in super::super) fn classify(
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        variables: Option<&serde_json::Value>,
        roots: &[(&str, &[FieldSelection])],
    ) -> Result<Self> {
        let access = SelectionAccess::classify_roots(
            &ctx.schema,
            roots,
            security_ctx,
            LevelAuthz::from_config(&ctx.config, variables),
        )?;
        let mut rows = DocumentRowFilter::default();
        for (type_name, sels) in roots {
            rows.plan(ctx, type_name, sels, security_ctx)?;
        }
        // The #423 field authorizer's refusals that do not need the document: no principal,
        // no authorizer, a nested gated field, unreadable arguments. Its decision waits for
        // the document; these do not. An argument's value decides none of them — a variable
        // the request did not bind reads as null, never as an error — so none is bound here.
        let unbound = std::collections::HashMap::new();
        for (type_name, sels) in roots {
            field_authz_inputs(ctx, security_ctx, type_name, sels, &unbound)?;
        }
        Ok(Self { access, rows })
    }

    /// Serve `document`, a `type_name`, through `selections`: its nested levels
    /// row-filtered, projected, masked, then put to the #423 field authorizer.
    ///
    /// # Errors
    ///
    /// What the #423 authorizer refuses over the document ([`enforce_field_authz`]).
    pub(in super::super) fn serve(
        &self,
        ctx: &ExecutorContext,
        security_ctx: Option<&SecurityContext>,
        type_name: &str,
        selections: &[FieldSelection],
        document: &serde_json::Value,
        variables: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let mut source = Cow::Borrowed(document);
        if !self.rows.is_empty() {
            self.rows.apply(source.to_mut(), type_name, selections, &ctx.schema);
        }
        let mut projected = project_entity(&source, type_name, selections, &ctx.schema);
        self.access.null_masked(&mut projected, type_name, selections, &ctx.schema);
        let masked = self.access.masked_fields(type_name);
        enforce_field_authz(
            ctx,
            security_ctx,
            type_name,
            selections,
            &source,
            &mut projected,
            variables,
            &masked,
        )?;
        Ok(projected)
    }
}

/// What the dynamic field authorizer (#423) needs to decide the gated fields a payload
/// selection reaches on a `type_name`: who asks, who answers, and which fields.
struct FieldAuthzInputs<'a> {
    principal:  &'a SecurityContext,
    authorizer: &'a dyn crate::security::FieldAuthorizer,
    gated:      Vec<crate::security::field_authorizer::GatedField>,
}

/// Resolve the field authorizer's inputs for `selections` on a `type_name`, from the
/// selection, the principal and the configuration alone. `None` when the selection reaches
/// no gated field (and then the authorizer is never called).
///
/// Everything here is known before the document exists, so the plan asks it then
/// ([`ReadPlan::classify`]) — for a write, before the function runs. Only the
/// authorizer's own decision needs the document.
///
/// # Errors
///
/// Fail-closed, `Authorization` (403): a gated field selected with no authenticated
/// principal, with no authorizer configured, or nested in a sub-selection (top-level is
/// enforced in v1). `Internal`: a gated field's arguments cannot be read.
fn field_authz_inputs<'a>(
    ctx: &'a ExecutorContext,
    security_ctx: Option<&'a SecurityContext>,
    type_name: &str,
    selections: &[FieldSelection],
    variables: &std::collections::HashMap<String, serde_json::Value>,
) -> Result<Option<FieldAuthzInputs<'a>>> {
    use crate::security::field_authorizer as authz;

    if !authz::selection_set_selects_gated_field(&ctx.schema, type_name, selections) {
        return Ok(None);
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
    let gated =
        authz::collect_top_level_gated_fields(&ctx.schema, type_name, selections, variables)?;
    Ok(Some(FieldAuthzInputs {
        principal,
        authorizer: authorizer.as_ref(),
        gated,
    }))
}

/// Enforce the dynamic field authorizer (#423) on a projected document.
///
/// `entity` is the full projected-from value (the `parent`); `projected` is the
/// response object, mutated in place. Fail-closed: a `Reject` decision or any policy
/// error → 403, and what [`field_authz_inputs`] refuses.
///
/// No-op (and zero authorizer calls) when the selection set has no gated field.
// Reason: the principal, the entity and its projection, the selection, its variables and
// what the static gate masked are each a separate input to one decision; a struct would
// only relocate them.
#[allow(clippy::too_many_arguments)]
fn enforce_field_authz(
    ctx: &ExecutorContext,
    security_ctx: Option<&SecurityContext>,
    type_name: &str,
    selections: &[FieldSelection],
    entity: &serde_json::Value,
    projected: &mut serde_json::Value,
    variables: &std::collections::HashMap<String, serde_json::Value>,
    statically_masked: &[String],
) -> Result<()> {
    let Some(inputs) = field_authz_inputs(ctx, security_ctx, type_name, selections, variables)?
    else {
        return Ok(());
    };
    let pass = crate::security::field_authorizer::FieldAuthzPass {
        authorizer: inputs.authorizer,
        principal: inputs.principal,
        type_name,
        gated: &inputs.gated,
        // AND-composition with the static `requires_scope` gate (the plan's masks), as on
        // the query path: a field it already masked is not put to the authorizer.
        statically_masked,
    };
    crate::security::field_authorizer::apply_field_authorizer_to_entity(&pass, entity, projected)
}
