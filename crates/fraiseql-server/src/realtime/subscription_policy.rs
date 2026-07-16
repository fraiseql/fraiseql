//! Realtime-seam adapter over the shared subscription row-visibility policy (#596).
//!
//! The policy type and its derivation live in `fraiseql-core`
//! ([`fraiseql_core::schema::SubscriptionPolicy`] → [`OwnerCondition`]) so this realtime
//! entity-stream seam and the graphql `/ws` seam consume **identical** semantics — a
//! divergence (e.g. `bypass_roles` honored on one path but not the other) would itself
//! be a visibility bypass. This module only *adapts* the seam-neutral [`OwnerCondition`]
//! to the realtime delivery matcher's [`FieldFilter`] and defines the per-subscription
//! [`OwnerEnforcement`] state the delivery pipeline consults.
//!
//! # Fail-closed on a dormant seam
//!
//! This realtime subsystem is **not assembled by any production binary** (see #605).
//! The one property that must survive is fail-closed-by-construction: a policy-declaring
//! entity can never come up deliver-all by accident. So delivery denies a policy
//! entity's events to any subscription that did not resolve an explicit
//! [`OwnerEnforcement`] (`Bypass` or `Scoped`) at subscribe time — even a future
//! assembler who skips the subscribe-time wiring cannot leak rows.

pub use fraiseql_core::schema::{OwnerCondition, SubscriptionPolicy};

use super::subscriptions::{FieldFilter, FilterOperator};

/// Per-subscription row-visibility enforcement for a policy-declaring entity (#596).
///
/// Resolved at subscribe time from the entity's [`SubscriptionPolicy`] and the
/// connection's enriched identity, and stored on the subscription so the delivery
/// pipeline can enforce it without re-deriving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OwnerEnforcement {
    /// The entity declares no policy — unchanged behavior (the default).
    #[default]
    None,
    /// A bypass role — full visibility for a policy-declaring entity.
    Bypass,
    /// Scoped to a server-owned owner filter — the principal sees only its rows.
    Scoped(FieldFilter),
}

/// Map a derived [`OwnerCondition`] to the realtime seam's [`OwnerEnforcement`], or a
/// refusal reason.
///
/// # Errors
///
/// Returns the refusal reason when the condition is [`OwnerCondition::Refuse`] — the
/// subscribe path turns this into an error and does not register the subscription
/// (fail-closed).
pub fn owner_enforcement(condition: OwnerCondition) -> Result<OwnerEnforcement, String> {
    match condition {
        OwnerCondition::Bypass => Ok(OwnerEnforcement::Bypass),
        OwnerCondition::Eq { field, value } => Ok(OwnerEnforcement::Scoped(FieldFilter {
            field,
            operator: FilterOperator::Eq,
            value,
        })),
        OwnerCondition::Refuse(reason) => Err(reason),
    }
}

#[cfg(test)]
mod tests;
