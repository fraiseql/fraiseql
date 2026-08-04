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
//! The expression language the original issue sketched (`object.owner ==
//! jwt.sub`, `now() < object.expires_at`), time-bounded grants, and the
//! admin-REST hot-reload path are deliberately not here; they are tracked
//! separately.

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

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
}

impl PolicyPrincipal {
    /// Parse a configured principal: `owner`, `authenticated`, `anonymous`, or
    /// `role:<name>`.
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
            other => Err(format!(
                "unknown policy principal {other:?}; expected \"owner\", \"authenticated\", \
                 \"anonymous\" or \"role:<name>\""
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
            };
            if principal_matches {
                return true;
            }
        }
        false
    }
}
