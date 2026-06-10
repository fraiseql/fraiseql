//! Actor classification for the audit / Change-Spine envelope (#390).
//!
//! Every audited operation carries a first-class [`ActorType`] — was it a human
//! user, a service account, an autonomous agent acting for a user, or an internal
//! system job? For a delegated request (RFC 8693 token exchange), the row also
//! records the **underlying human** the agent acted for. This turns per-actor
//! forensics ("every action an automated process took on behalf of user X") into
//! a trivial query against the change-log / tenant audit tables.
//!
//! The classification is *recorded*, not an authorization input — see the
//! security note on [`derive_actor`].

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
/// The result is *recorded* (audit / change-log envelope), never consumed by an
/// authorization decision in the engine. The `act` claim is only honoured on
/// signature-verified tokens, so an unauthenticated or unsigned request cannot
/// inject a delegation. An application that chooses to trust `actor_type` in its
/// own [`Authorizer`](crate::security::Authorizer) is trusting its `IdP`'s `act`
/// issuance.
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

#[cfg(test)]
mod tests;
