//! Row-level visibility policies for `/ws` subscription delivery (#596).
//!
//! The pull path (GraphQL queries) enforces per-row RLS; historically the push path
//! (`/ws`) did not — any principal authorized to subscribe to an entity received
//! **every** row's after-images. A [`SubscriptionPolicy`] closes that: at subscribe
//! time it derives a **server-owned** owner filter from the connection's enriched
//! identity (#539, the forge-proof `fraiseql.enriched.*` namespace), so a scoped
//! subscriber only receives rows it owns. It is **fail-closed for declaring
//! entities**: a policy present but the identity field unresolvable **refuses** the
//! subscription rather than delivering everything.
//!
//! The derived filter is a plain equality [`FieldFilter`] enforced by the existing
//! delivery matcher ([`evaluate_field_filters`](super::delivery::evaluate_field_filters)) —
//! the owner boundary is one `eq` condition. Entities with no policy keep today's
//! behavior (no back-compat break). The multi-tenant gate is unchanged and composes
//! with this (AND). `/realtime/v1` (the app-published channel pubsub) is out of scope.

use std::collections::HashMap;

use fraiseql_core::security::ENRICHED_NAMESPACE_PREFIX;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::subscriptions::{FieldFilter, FilterOperator};

/// An entity's row-visibility policy for `/ws` delivery (#596).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionPolicy {
    /// The single-level JSON path into the after-image holding the owner id
    /// (e.g. `"$.owner_id"`). Only `$.<field>` is supported — the delivery matcher
    /// keys on a flat field name.
    pub owner_path:     String,
    /// The enriched-identity field (in the `fraiseql.enriched.*` namespace, #539)
    /// whose value the owner must equal (e.g. `"user_id"`).
    pub identity_field: String,
    /// Roles that bypass the policy entirely (full visibility, e.g. `["admin"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bypass_roles:   Vec<String>,
}

/// The result of deriving an owner filter for one subscribe request.
#[derive(Debug, Clone)]
pub enum OwnerFilterOutcome {
    /// The principal holds a bypass role — full visibility, no added filter.
    Bypass,
    /// A server-owned owner filter to enforce (the principal sees only its rows).
    Filter(FieldFilter),
    /// **Fail-closed**: the policy applies but the enriched identity is unresolvable
    /// (no enrichment configured, or a NULL/absent field) — refuse the subscription.
    Refuse(String),
}

impl SubscriptionPolicy {
    /// The flat owner field name the delivery matcher keys on — `owner_path` with a
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
                 paths are not supported by the delivery matcher",
                self.owner_path
            ));
        }
        Ok(())
    }

    /// Derive the owner filter for a connection whose enriched identity is `ctx`
    /// (#596). Bypass role → [`Bypass`]; resolvable identity → an `eq`
    /// [`Filter`]; otherwise **fail-closed** [`Refuse`].
    ///
    /// [`Bypass`]: OwnerFilterOutcome::Bypass
    /// [`Filter`]: OwnerFilterOutcome::Filter
    /// [`Refuse`]: OwnerFilterOutcome::Refuse
    #[must_use]
    pub fn derive(
        &self,
        attributes: &HashMap<String, Value>,
        roles: &[String],
    ) -> OwnerFilterOutcome {
        if self.bypass_roles.iter().any(|bypass| roles.iter().any(|role| role == bypass)) {
            return OwnerFilterOutcome::Bypass;
        }
        // The identity value must come from the server-resolved enriched namespace —
        // the #539 resolver strips any inbound `fraiseql.*` claims, so this is
        // forge-proof.
        let key = format!("{ENRICHED_NAMESPACE_PREFIX}{}", self.identity_field);
        match attributes.get(&key) {
            Some(value) if !value.is_null() => OwnerFilterOutcome::Filter(FieldFilter {
                field:    self.owner_field().to_string(),
                operator: FilterOperator::Eq,
                value:    value.clone(),
            }),
            _ => OwnerFilterOutcome::Refuse(format!(
                "subscription refused (fail-closed): enriched identity field `{}` is unresolvable \
                 — configure `[identity.enrichment]` so the owner boundary can be derived",
                self.identity_field
            )),
        }
    }
}

#[cfg(test)]
mod tests;
