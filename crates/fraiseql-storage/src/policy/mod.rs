//! Per-bucket access policies (#371).
//!
//! A bucket's `access` mode (`private` / `public_read`) expresses two coarse
//! shapes. A policy expresses the rest: *"members of `role:auditor` may read
//! anything under `reports/`, but only the owner may delete"*.
//!
//! # The shape is deliberately not a DSL
//!
//! This codebase's history with policy evaluation is not encouraging — a
//! vacuous RLS gate (#762), an RLS bypass through a client-supplied table name
//! (#795), field RBAC skipped on one path (#743). Every one was an evaluator
//! that *looked* like it decided something. So a policy here is a list of
//! **permit rules** over a closed vocabulary — no expressions, no operators,
//! nothing to parse at request time:
//!
//! - **Deny is structural, not a rule.** [`BucketPolicy::permits`] returns `true` only from inside
//!   a matched permit rule; every other path falls through to `false`. There is no `effect =
//!   "deny"` to get the precedence of wrong, and no way to write a policy that fails open.
//! - **Unparseable is not "deny", it is "do not boot."** An unknown method, principal or role
//!   spelling is a startup error (`resolve_storage_section`), so an operator cannot ship a typo
//!   that silently denies everything at 3am — or, worse, that silently denies the *narrowing* rule
//!   in a policy whose remaining rules permit.
//! - **Policies replace the access mode, they do not layer under it.** A bucket with policies is
//!   governed by its policies alone (plus the admin bypass): a reader deciding "what can this
//!   caller do" reads one list.
//!
//! # Conditions are closed fields, not expressions (#974)
//!
//! The original issue sketched a CEL-style DSL (`object.expires_at`,
//! `jwt.<claim>`). What is here instead is the same expressive power as *more
//! closed rule fields*, so there is still nothing parsed or evaluated at
//! request time — only comparisons between values already in hand:
//!
//! - [`PolicyRule::not_before`] / [`PolicyRule::not_after`] — the grant's own validity window.
//! - [`PolicyRule::require_unexpired`] — the object's own expiry, `now < object.expires_at`.
//! - [`PolicyRule::require_claims`] — exact-match equality against the caller's token claims.
//!
//! Every condition **narrows**. A rule permits only when its method, key
//! prefix, *every* condition, and its principal all match; a rule that fails
//! any of them is skipped, and the next rule is considered. Since rules are
//! permit-only, skipping a rule can never widen access.
//!
//! `require_metadata` from the issue is deliberately absent: objects carry no
//! user-defined metadata yet, so a field matching against it would have
//! nothing to compare. Tracked separately.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A caller's token claims, normalised to strings for exact matching.
///
/// See [`normalise_claims`] for which JSON shapes become matchable.
pub type ClaimValues = BTreeMap<String, String>;

/// Normalise raw JWT claims into the exact-match view [`PolicyRule::require_claims`] compares
/// against.
///
/// The rule is closed and deliberately narrow, because a policy comparison
/// that silently coerces is a policy comparison nobody can predict:
///
/// - a JSON string matches its contents;
/// - a number or boolean matches its JSON rendering (`42`, `true`);
/// - **null, arrays and objects are dropped** — they are never matchable, so a rule requiring one
///   denies rather than guessing what equality should mean.
///
/// A dropped claim is not an error. It is absent, and an absent claim fails
/// the requirement (see [`PolicyRule::require_claims`]).
#[must_use]
pub fn normalise_claims<S: std::hash::BuildHasher>(
    raw: &std::collections::HashMap<String, serde_json::Value, S>,
) -> ClaimValues {
    raw.iter()
        .filter_map(|(name, value)| {
            let normalised = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Null
                | serde_json::Value::Array(_)
                | serde_json::Value::Object(_) => return None,
            };
            Some((name.clone(), normalised))
        })
        .collect()
}

/// An operation a policy rule can permit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyMethod {
    /// Download or render an object.
    Read,
    /// Create a NEW object (including resumable uploads and presigned PUTs).
    ///
    /// Deliberately does NOT include overwriting an existing object — see
    /// [`PolicyMethod::Overwrite`].
    Write,
    /// Replace an EXISTING object's content.
    ///
    /// Separate from [`Write`](PolicyMethod::Write) because the natural rule
    /// *"authenticated callers may write"* would otherwise let any authenticated
    /// caller clobber any other user's object by writing to its key — the H9/B4
    /// overwrite IDOR, re-entering through the policy door. Granting it is an
    /// explicit act; `write` alone is create-only.
    Overwrite,
    /// Delete an object.
    Delete,
    /// List a bucket's objects.
    List,
}

impl PolicyMethod {
    /// Parse a configured method name.
    ///
    /// # Errors
    ///
    /// Returns the offending spelling; the caller turns it into a boot error.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.to_ascii_lowercase().as_str() {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "overwrite" => Ok(Self::Overwrite),
            "delete" => Ok(Self::Delete),
            "list" => Ok(Self::List),
            other => Err(format!(
                "unknown policy method {other:?}; expected \"read\", \"write\", \
                 \"overwrite\", \"delete\" or \"list\""
            )),
        }
    }
}

/// Who a rule permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PolicyPrincipal {
    /// The object's owner (never matches an object with no owner, and never an
    /// unauthenticated caller).
    Owner,
    /// Any authenticated caller.
    Authenticated,
    /// Anyone, including unauthenticated callers.
    Anonymous,
    /// A caller carrying this role.
    Role(String),
    /// A request arriving through a signed URL, whoever the caller is.
    ///
    /// This is what expresses *"a public bucket whose objects are only served
    /// through signed URLs"*: the direct download route never matches this
    /// principal, and only the presign endpoint does. It deliberately says
    /// nothing about identity — a signed URL is a bearer grant, so requiring
    /// *both* a signature and an identity means writing two rules.
    SignedUrl,
}

impl PolicyPrincipal {
    /// Parse a configured principal: `owner`, `authenticated`, `anonymous`,
    /// `signed_url`, or `role:<name>`.
    ///
    /// # Errors
    ///
    /// Returns the offending spelling; the caller turns it into a boot error.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let trimmed = raw.trim();
        if let Some(role) = trimmed.strip_prefix("role:") {
            let role = role.trim();
            if role.is_empty() {
                return Err("policy principal \"role:\" names no role".to_string());
            }
            return Ok(Self::Role(role.to_string()));
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "owner" => Ok(Self::Owner),
            "authenticated" => Ok(Self::Authenticated),
            "anonymous" => Ok(Self::Anonymous),
            "signed_url" => Ok(Self::SignedUrl),
            other => Err(format!(
                "unknown policy principal {other:?}; expected \"owner\", \"authenticated\", \
                 \"anonymous\", \"signed_url\" or \"role:<name>\""
            )),
        }
    }
}

/// One permit rule. There is no deny rule by construction (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Operations this rule permits.
    pub methods:    Vec<PolicyMethod>,
    /// Who it permits them to.
    pub principal:  PolicyPrincipal,
    /// When set, the rule applies only to keys starting with this prefix.
    /// Absent means the whole bucket.
    pub key_prefix: Option<String>,

    /// The grant does not apply before this instant. Absent means no lower bound.
    #[serde(default)]
    pub not_before: Option<DateTime<Utc>>,

    /// The grant does not apply at or after this instant. Absent means no upper
    /// bound.
    ///
    /// The bound is exclusive: at exactly `not_after` the grant has ended. A
    /// half-open window is the only reading under which two adjacent grants
    /// (`..noon`, `noon..`) neither overlap nor leave a gap.
    #[serde(default)]
    pub not_after: Option<DateTime<Utc>>,

    /// The rule applies only to an object that has an expiry still in the
    /// future — `object.expires_at IS NOT NULL AND now < object.expires_at`.
    ///
    /// An object with **no** expiry does not satisfy this, which is the
    /// fail-closed reading and the one the field name states: `now <
    /// object.expires_at` is not true when there is no `expires_at`, exactly as
    /// the SQL comparison it mirrors is not true against `NULL`. An operator
    /// who wants "no expiry means never expires" writes a second rule without
    /// the condition, and does so visibly.
    ///
    /// For a request about no particular object (a `list`), there is no expiry
    /// to test, so a rule carrying this condition never permits it.
    #[serde(default)]
    pub require_unexpired: bool,

    /// Claims the caller's token must carry, compared for exact string equality.
    ///
    /// Every entry must match: a missing claim, a claim that normalised away
    /// (see [`normalise_claims`]), or a differing value all fail the rule. An
    /// empty map requires nothing.
    ///
    /// Claims are populated only on the OIDC validation path. Under static-token
    /// or API-key auth the claim set is empty, so any rule requiring a claim
    /// denies — deliberately, since the alternative is a rule that silently
    /// stops narrowing when the auth mode changes.
    #[serde(default)]
    pub require_claims: BTreeMap<String, String>,
}

/// The request a policy decision is made about.
#[derive(Debug, Clone, Copy)]
pub struct PolicyRequest<'a> {
    /// The authenticated caller, if any.
    pub user_id:  Option<&'a str>,
    /// The caller's roles.
    pub roles:    &'a [String],
    /// The object key. For `list`, the requested prefix (`""` for a whole-bucket
    /// listing).
    pub key:      &'a str,
    /// The object's owner, when the request is about an existing object.
    pub owner_id: Option<&'a str>,

    /// The instant this request is decided at.
    ///
    /// Passed in rather than read from the clock inside the evaluator, so a
    /// time-bounded grant is testable without waiting for wall-clock time to
    /// pass — and so one request cannot straddle two different "now"s.
    pub now: DateTime<Utc>,

    /// The object's own expiry, when the request is about an existing object
    /// that has one. `None` covers both "no expiry set" and "no object"
    /// (a listing); [`PolicyRule::require_unexpired`] rejects both.
    pub expires_at: Option<DateTime<Utc>>,

    /// The caller's normalised token claims. Empty when the auth path supplies
    /// none.
    pub claims: &'a ClaimValues,

    /// Whether this request arrived through the signed-URL path.
    pub via_signed_url: bool,
}

impl PolicyRule {
    /// Whether every condition on this rule holds for `request`.
    ///
    /// Conditions are conjunctive and each one only ever narrows; a rule with
    /// no conditions holds vacuously, which is what keeps #371's policies
    /// behaving exactly as they did.
    fn conditions_hold(&self, request: &PolicyRequest<'_>) -> bool {
        if let Some(not_before) = self.not_before {
            if request.now < not_before {
                return false;
            }
        }
        if let Some(not_after) = self.not_after {
            if request.now >= not_after {
                return false;
            }
        }
        if self.require_unexpired {
            // `None` is both "no expiry" and "no object"; neither satisfies
            // `now < expires_at`, exactly as the SQL comparison against NULL
            // does not.
            match request.expires_at {
                Some(expires_at) if request.now < expires_at => {},
                _ => return false,
            }
        }
        self.require_claims.iter().all(|(name, expected)| {
            request.claims.get(name).is_some_and(|actual| actual == expected)
        })
    }
}

/// A bucket's policy: an ordered list of permit rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BucketPolicy {
    /// The permit rules. An empty list permits nothing.
    pub rules: Vec<PolicyRule>,
}

impl BucketPolicy {
    /// Whether this policy permits `method` for `request`.
    ///
    /// Permits only from inside a matched rule; every other path is a denial.
    #[must_use]
    pub fn permits(&self, method: PolicyMethod, request: &PolicyRequest<'_>) -> bool {
        for rule in &self.rules {
            if !rule.methods.contains(&method) {
                continue;
            }
            if let Some(ref prefix) = rule.key_prefix {
                if !request.key.starts_with(prefix.as_str()) {
                    continue;
                }
            }
            // Conditions narrow, and every one of them must hold. A rule that
            // fails a condition is skipped rather than denying outright: the
            // rules are permit-only, so the next rule can still permit and no
            // skip can widen access.
            if !rule.conditions_hold(request) {
                continue;
            }
            let principal_matches = match rule.principal {
                // An unauthenticated caller is nobody's owner, and an object
                // with no owner has no owner to match — both are denials.
                PolicyPrincipal::Owner => match (request.user_id, request.owner_id) {
                    (Some(user), Some(owner)) => user == owner,
                    _ => false,
                },
                PolicyPrincipal::Authenticated => request.user_id.is_some(),
                PolicyPrincipal::Anonymous => true,
                PolicyPrincipal::Role(ref role) => request.roles.iter().any(|r| r == role),
                PolicyPrincipal::SignedUrl => request.via_signed_url,
            };
            if principal_matches {
                return true;
            }
        }
        false
    }
}
