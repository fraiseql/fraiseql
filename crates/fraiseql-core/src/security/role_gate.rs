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
//! read chokepoints (`resolve_direct_read`, `count_rows`) carried no role gate of
//! their own, so nothing downstream caught it.
//!
//! Hence a function rather than a convention. A gate that means different things
//! on different transports is not a gate.

use crate::{
    error::{FraiseQLError, Result},
    security::SecurityContext,
};

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
    let Some(required_role) = required else {
        return Ok(());
    };
    let holds = security_context.is_some_and(|ctx| ctx.roles.iter().any(|r| r == required_role));
    if holds {
        return Ok(());
    }
    Err(FraiseQLError::Validation {
        message: format!("{operation_kind} '{operation_name}' not found in schema"),
        path:    None,
    })
}

#[cfg(test)]
mod tests;
