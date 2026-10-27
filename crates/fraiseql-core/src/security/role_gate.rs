//! The one definition of what `requires_role` means (#1122).
//!
//! `QueryDefinition::requires_role` and `MutationDefinition::requires_role` are
//! documented as "only users whose `SecurityContext.roles` contains this role",
//! and callers who lack it receive the operation's *absence* — `"not found in
//! schema"`, never `FORBIDDEN` — so that a refusal cannot be used to enumerate
//! which roles exist.
//!
//! Both halves of that sentence used to be re-implemented at each gate. Five
//! copies agreed; a sixth, in the REST resolver, tested `SecurityContext.scopes`
//! instead of `roles` and answered `403`. It got both halves wrong in opposite
//! directions: a token holding the *role* was refused a query it is entitled to,
//! and a token holding a same-named *scope* was served one it is not — the REST
//! read chokepoints (`resolve_root_read`, `count_rows`) carried no role gate of
//! their own, so nothing downstream caught it.
//!
//! Hence a function rather than a convention. A gate that means different things
//! on different transports is not a gate.

use crate::{
    error::{FraiseQLError, Result},
    schema::CompiledSchema,
    security::SecurityContext,
};

/// Whether a caller may run an operation gated by `required`: `None` is ungated, and an
/// anonymous caller holds no role.
///
/// [`enforce_requires_role`] refuses on it, and introspection lists an operation on it,
/// so an operation a caller is told does not exist is also one it is not shown.
#[must_use]
pub fn caller_holds(required: Option<&str>, security_context: Option<&SecurityContext>) -> bool {
    required
        .is_none_or(|role| security_context.is_some_and(|ctx| ctx.roles.iter().any(|r| r == role)))
}

/// The schema as `caller` may see it.
///
/// It lacks the queries and mutations the caller may not run, the types whose role it
/// lacks, and those types' union memberships. A stored SDL names every declaration, so it
/// is dropped when anything is hidden.
///
/// Introspection and the schema exports describe the schema through this one answer, so
/// what a caller is shown and what it may run agree.
///
/// Loading lowers a type's role onto every operation that returns it, and refuses a field
/// that reaches a role-gated type from a type not gated the same way. Every operation or
/// field that reaches a hidden type is therefore itself hidden, and an interface's possible
/// types are read from `types`, so neither needs a pass of its own.
#[must_use]
pub fn visible_schema(schema: &CompiledSchema, caller: Option<&SecurityContext>) -> CompiledSchema {
    visible_schema_where(schema, |required| caller_holds(required, caller))
}

/// The schema as seen by whoever `holds` the roles it answers `true` for, as
/// [`visible_schema`] describes; `holds` is asked about `None` too, and must answer `true`.
#[must_use]
pub(crate) fn visible_schema_where(
    schema: &CompiledSchema,
    holds: impl Fn(Option<&str>) -> bool,
) -> CompiledSchema {
    let mut visible = schema.clone();
    visible.queries.retain(|q| holds(q.requires_role.as_deref()));
    visible.mutations.retain(|m| holds(m.requires_role.as_deref()));
    visible.types.retain(|t| holds(t.requires_role.as_deref()));
    let hidden: Vec<&str> = schema
        .types
        .iter()
        .filter(|t| !holds(t.requires_role.as_deref()))
        .map(|t| t.name.as_str())
        .collect();
    for union in &mut visible.unions {
        union.member_types.retain(|member| !hidden.contains(&member.as_str()));
    }
    if visible.queries.len() < schema.queries.len()
        || visible.mutations.len() < schema.mutations.len()
        || !hidden.is_empty()
    {
        visible.schema_sdl = None;
    }
    visible
}

/// Refuse the operation unless the caller holds `required`, hiding its existence.
///
/// `operation_kind` is `"Query"` or `"Mutation"`, used verbatim in the message a
/// client reads. `None` for `required` is an ungated operation and always passes.
///
/// # Errors
///
/// Returns [`FraiseQLError::Validation`] reporting the operation as absent — not
/// [`FraiseQLError::Authorization`]. The distinction is the point: `FORBIDDEN`
/// confirms that the named operation exists and that some role reaches it, which
/// is exactly what role enumeration needs.
pub fn enforce_requires_role(
    operation_kind: &str,
    operation_name: &str,
    required: Option<&str>,
    security_context: Option<&SecurityContext>,
) -> Result<()> {
    if caller_holds(required, security_context) {
        return Ok(());
    }
    Err(FraiseQLError::Validation {
        message: format!("{operation_kind} '{operation_name}' not found in schema"),
        path:    None,
    })
}

#[cfg(test)]
mod tests;
