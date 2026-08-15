//! Actor classification for the audit / Change-Spine envelope (#390).
//!
//! Every audited operation carries a first-class [`ActorType`] — was it a human
//! user, a service account, an autonomous agent acting for a user, or an internal
//! system job? For a delegated request (RFC 8693 token exchange), the row also
//! records the **underlying human** the agent acted for. This turns per-actor
//! forensics ("every action an automated process took on behalf of user X") into
//! a trivial query against the change-log / tenant audit tables.
//!
//! The classification is recorded on every audited operation, and since #966 it
//! is also **consumable as an authorization input**: an operation may declare
//! `requires_actor`, an allow-list of actor classes, enforced by
//! [`enforce_requires_actor`] in the same executor gate as `requires_role`. That
//! is a deliberate change from the earlier "recorded, never consumed" stance,
//! and it rests entirely on the classification being underivable from anything a
//! client controls — see the security note on [`derive_actor`].

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The JWT scope that marks a non-human service-account token.
const SERVICE_ACCOUNT_SCOPE: &str = "service_account";

/// The JWT claim (RFC 8693 token-exchange "actor") whose presence marks a
/// delegated request — an agent acting on behalf of the token subject.
const DELEGATION_CLAIM: &str = "act";

/// The kind of principal behind a request.
///
/// Serialized `snake_case` so it renders directly into the change-log
/// `actor_type TEXT` column and the tenant audit log (`"human_user"`,
/// `"service_account"`, `"ai_agent"`, `"system_job"`). [`Default`] is
/// [`HumanUser`](Self::HumanUser) — the safe, most-common classification and the
/// value a serde-defaulted [`SecurityContext`](crate::security::SecurityContext)
/// deserializes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    /// A human end user (the default classification for an ordinary user JWT).
    #[default]
    HumanUser,
    /// A non-human service account — an API key or a token carrying the
    /// `service_account` scope.
    ServiceAccount,
    /// An autonomous agent acting on behalf of a user, identified by an RFC 8693
    /// `act` delegation claim. The user being acted for is recorded separately
    /// (see [`derive_actor`]).
    AiAgent,
    /// An internal scheduled / system-triggered job. Never token-derived; set
    /// explicitly by internal callers.
    SystemJob,
}

impl ActorType {
    /// Every variant, in declaration order.
    ///
    /// The canonical roster consumed wherever the full domain is needed as data:
    /// the `fraiseql doctor` against-db actor check builds its `NOT IN (…)` list
    /// from it, and the CLI lockstep test asserts the change-log CHECK constraint
    /// (`chk_entity_change_log_actor_type`, migration 08) names exactly these
    /// tokens — so adding a variant without updating the constraint is a red
    /// test, not a runtime surprise.
    pub const ALL: [Self; 4] = [
        Self::HumanUser,
        Self::ServiceAccount,
        Self::AiAgent,
        Self::SystemJob,
    ];

    /// The stable `snake_case` token written to the `actor_type` column.
    ///
    /// Matches the [`Serialize`] representation; used on the change-log write
    /// path where a borrowed `&'static str` is wanted rather than a JSON round-trip.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HumanUser => "human_user",
            Self::ServiceAccount => "service_account",
            Self::AiAgent => "ai_agent",
            Self::SystemJob => "system_job",
        }
    }

    /// Parse the `snake_case` token produced by [`as_str`](Self::as_str) back into
    /// an [`ActorType`]. `None` for an unrecognised token, so a reader can fall
    /// back to [`Default`]. Inverse of [`as_str`](Self::as_str).
    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "human_user" => Some(Self::HumanUser),
            "service_account" => Some(Self::ServiceAccount),
            "ai_agent" => Some(Self::AiAgent),
            "system_job" => Some(Self::SystemJob),
            _ => None,
        }
    }
}

/// Classify a request's actor from its (signature-verified) JWT material.
///
/// Returns the [`ActorType`] plus, for a delegated request, the **underlying
/// human** the agent acts for — the token's top-level `sub` (passed here as
/// `user_id`), parsed as a [`Uuid`] (`None` when absent or not UUID-shaped). Per
/// RFC 8693 the token `sub` is the subject (the human) and the `act` claim names
/// the acting agent; the agent's own identity therefore stays in `act` (available
/// via [`SecurityContext.attributes`](crate::security::SecurityContext)) and is
/// not what `acting_for` records.
///
/// Rules, first match wins:
/// 1. an `act` delegation claim is present → [`AiAgent`](ActorType::AiAgent), `acting_for = sub`.
/// 2. a `service_account` scope is present → [`ServiceAccount`](ActorType::ServiceAccount).
/// 3. otherwise → [`HumanUser`](ActorType::HumanUser).
///
/// [`SystemJob`](ActorType::SystemJob) is never derived from a token. The API-key
/// path classifies [`ServiceAccount`](ActorType::ServiceAccount) explicitly at its
/// construction site rather than relying on a token marker.
///
/// # Security
///
/// The result is recorded in the audit / change-log envelope **and**, since
/// #966, is an authorization input wherever an operation declares
/// `requires_actor` (see [`enforce_requires_actor`]). What makes that safe is
/// that no client can choose its own classification:
///
/// * the `act` claim is honoured only on **signature-verified** tokens, so an unauthenticated or
///   unsigned request cannot inject a delegation;
/// * the derived value is stamped into the reserved `fraiseql.` attribute namespace, which the HTTP
///   extractor **strips** from token claims — a caller cannot supply `fraiseql.actor_type` and have
///   it survive;
/// * the API-key, gRPC and Flight construction paths never forward `extra_claims` at all; and
/// * nothing deserializes a [`SecurityContext`](crate::security::SecurityContext) from an untrusted
///   payload.
///
/// Weaken any one of those and `requires_actor` becomes a gate the caller sets
/// for itself. A deployment trusting `AiAgent` restrictions is still trusting its
/// `IdP`'s `act` issuance, exactly as `requires_role` trusts its role claims.
#[must_use]
pub fn derive_actor<S: std::hash::BuildHasher>(
    user_id: &str,
    scopes: &[String],
    extra_claims: &HashMap<String, serde_json::Value, S>,
) -> (ActorType, Option<Uuid>) {
    if extra_claims.get(DELEGATION_CLAIM).is_some_and(|v| !v.is_null()) {
        return (ActorType::AiAgent, Uuid::parse_str(user_id).ok());
    }
    if scopes.iter().any(|s| s == SERVICE_ACCOUNT_SCOPE) {
        return (ActorType::ServiceAccount, None);
    }
    (ActorType::HumanUser, None)
}

/// Refuse an operation whose `requires_actor` allow-list excludes this request's
/// actor class (#966).
///
/// `Ok(())` when `required` is empty — the overwhelmingly common case, and the
/// reason this is a cheap slice check rather than a policy lookup.
///
/// # One implementation, every transport
///
/// This is the whole predicate, called from each executor gate that already
/// enforces `requires_role` (regular queries, the relay node lookup, mutations,
/// and the federation `_entities` resolver). Living inside the executor is what
/// makes "every transport" true rather than asserted: GraphQL, REST, MCP, gRPC
/// and the functions bridge all reach the database through those gates, so none
/// of them can be the one that forgot (#808's lesson). A per-transport copy
/// would be four more places to be wrong.
///
/// # Ordering
///
/// Call this **after** `requires_role`. The role gate answers "not found" to
/// avoid role enumeration; running it first means a caller who lacks the role
/// learns nothing new here. A caller who *has* the role and is refused on class
/// gets a plain `Authorization` error naming the class — which leaks nothing,
/// since a caller already knows what kind of principal it is, and a "not found"
/// would send an agent's author hunting a schema bug that does not exist.
///
/// # Errors
///
/// Returns [`FraiseQLError::Authorization`](crate::error::FraiseQLError::Authorization)
/// when the request's actor class is
/// not in a non-empty `required` list, and when there is no security context at
/// all — an unclassifiable request is refused rather than defaulted, so the gate
/// does not depend on [`ActorType::default`] being the benign variant.
pub fn enforce_requires_actor(
    operation_kind: &str,
    operation_name: &str,
    required: &[ActorType],
    security_context: Option<&crate::security::SecurityContext>,
) -> crate::error::Result<()> {
    if required.is_empty() {
        return Ok(());
    }
    let permitted = || required.iter().map(|a| a.as_str()).collect::<Vec<_>>().join(", ");
    let Some(ctx) = security_context else {
        return Err(crate::error::FraiseQLError::Authorization {
            message:  format!(
                "{operation_kind} '{operation_name}' is restricted to actor types [{}], and an \
                 unauthenticated request has no actor classification",
                permitted()
            ),
            action:   Some(operation_kind.to_ascii_lowercase()),
            resource: Some(operation_name.to_string()),
        });
    };
    let actor = ctx.actor_type();
    if required.contains(&actor) {
        return Ok(());
    }
    Err(crate::error::FraiseQLError::Authorization {
        message:  format!(
            "{operation_kind} '{operation_name}' is not available to actor type '{}'; permitted: \
             [{}]",
            actor.as_str(),
            permitted()
        ),
        action:   Some(operation_kind.to_ascii_lowercase()),
        resource: Some(operation_name.to_string()),
    })
}

#[cfg(test)]
mod tests;
