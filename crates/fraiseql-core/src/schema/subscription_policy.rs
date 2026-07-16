//! Row-level visibility policy for subscription delivery (#596).
//!
//! The pull path (GraphQL queries) enforces per-row RLS; historically the push paths
//! (subscriptions) did not — any principal authorized to subscribe to an entity
//! received **every** row's after-images. A [`SubscriptionPolicy`] closes that: at
//! subscribe time it derives a **server-owned** owner condition from the connection's
//! enriched identity (#539, the forge-proof `fraiseql.enriched.*` namespace), so a
//! scoped subscriber only receives rows it owns.
//!
//! # One derivation, two seams
//!
//! [`SubscriptionPolicy::derive`] returns a seam-neutral [`OwnerCondition`]. Both push
//! seams are thin adapters over this single function:
//! - the graphql `/ws` path maps [`OwnerCondition::Eq`] to a `(field, value)` RLS condition on
//!   `subscribe_with_rls`;
//! - the realtime entity-stream maps it to a server-owned equality `FieldFilter`.
//!
//! Keeping the semantics (owner-path resolution, `bypass_roles`, unresolvable-identity
//! refusal) in *one* place is a security property: if the two seams grew their own
//! interpretations they would drift, and a divergence — e.g. `bypass_roles` honored on
//! one path but not the other — is itself a visibility bypass.
//!
//! It is **fail-closed for declaring entities**: a policy present but the identity field
//! unresolvable (no enrichment configured, NULL/absent field) yields
//! [`OwnerCondition::Refuse`], which the seam turns into a refused subscription rather
//! than delivering everything.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::security::ENRICHED_NAMESPACE_PREFIX;

/// An entity's row-visibility policy for subscription delivery (#596).
///
/// Declared per entity in the compiled schema
/// ([`TypeDefinition::subscription_policy`](super::TypeDefinition)):
///
/// ```jsonc
/// "subscription_policy": {
///   "owner_path": "$.owner_id",      // single-level path into the after-image
///   "identity_field": "user_id",     // the fraiseql.enriched.* field (#539)
///   "bypass_roles": ["admin"]        // roles that get full visibility
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubscriptionPolicy {
    /// The single-level JSON path into the after-image holding the owner id
    /// (e.g. `"$.owner_id"`). Only `$.<field>` is supported — the delivery matchers
    /// key on a flat field name.
    pub owner_path:     String,
    /// The enriched-identity field (in the `fraiseql.enriched.*` namespace, #539)
    /// whose value the owner must equal (e.g. `"user_id"`).
    pub identity_field: String,
    /// Roles that bypass the policy entirely (full visibility, e.g. `["admin"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bypass_roles:   Vec<String>,
}

/// The server-owned owner condition derived for one subscribe request — seam-neutral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerCondition {
    /// The principal holds a bypass role — full visibility, no added condition.
    Bypass,
    /// A server-owned equality condition: the principal sees only rows where
    /// `field == value`.
    Eq {
        /// The flat owner field name (the delivery matcher keys on it).
        field: String,
        /// The principal's server-resolved identity value.
        value: Value,
    },
    /// **Fail-closed**: the policy applies but the enriched identity is unresolvable
    /// (no enrichment configured, or a NULL/absent field) — refuse the subscription.
    Refuse(String),
}

impl SubscriptionPolicy {
    /// The flat owner field name the delivery matchers key on — `owner_path` with a
    /// leading `$.` stripped.
    #[must_use]
    pub fn owner_field(&self) -> &str {
        self.owner_path.trim_start_matches("$.")
    }

    /// Validate the policy at load time.
    ///
    /// # Errors
    ///
    /// - `owner_path` is empty or a nested path (only single-level `$.<field>`).
    /// - `identity_field` is empty.
    pub fn validate(&self) -> Result<(), String> {
        if self.identity_field.is_empty() {
            return Err("subscription_policy.identity_field must not be empty".to_string());
        }
        let field = self.owner_field();
        if field.is_empty() {
            return Err("subscription_policy.owner_path must name a field (e.g. \"$.owner_id\")"
                .to_string());
        }
        if field.contains('.') || field.contains('[') {
            return Err(format!(
                "subscription_policy.owner_path `{}` must be a single-level `$.<field>` — nested \
                 paths are not supported by the delivery matchers",
                self.owner_path
            ));
        }
        Ok(())
    }

    /// Derive the owner condition for a connection whose enriched identity is in
    /// `attributes` and whose roles are `roles` (#596). Bypass role →
    /// [`Bypass`](OwnerCondition::Bypass); a resolvable identity →
    /// [`Eq`](OwnerCondition::Eq); otherwise **fail-closed**
    /// [`Refuse`](OwnerCondition::Refuse).
    ///
    /// The identity value is read **only** from the server-resolved
    /// `fraiseql.enriched.*` namespace — the #539 resolver strips any inbound
    /// `fraiseql.*` claims, so a client-supplied plain attribute cannot widen
    /// visibility.
    #[must_use]
    pub fn derive(&self, attributes: &HashMap<String, Value>, roles: &[String]) -> OwnerCondition {
        if self.bypass_roles.iter().any(|bypass| roles.iter().any(|role| role == bypass)) {
            return OwnerCondition::Bypass;
        }
        let key = format!("{ENRICHED_NAMESPACE_PREFIX}{}", self.identity_field);
        match attributes.get(&key) {
            Some(value) if !value.is_null() => OwnerCondition::Eq {
                field: self.owner_field().to_string(),
                value: value.clone(),
            },
            _ => OwnerCondition::Refuse(format!(
                "subscription refused (fail-closed): enriched identity field `{}` is unresolvable \
                 — configure `[identity.enrichment]` so the owner boundary can be derived",
                self.identity_field
            )),
        }
    }
}

#[cfg(test)]
mod tests;
