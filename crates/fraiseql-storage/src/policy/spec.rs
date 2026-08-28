//! The wire form of a policy, and the one function that turns it into a
//! [`BucketPolicy`] (#974).
//!
//! A policy reaches the runtime through two doors — `[[storage.<name>.policies]]`
//! at boot and `PUT /api/v1/admin/storage/{bucket}/policies` at runtime — and
//! both must accept exactly the same set of policies. Two parsers would be two
//! implementations of one rule, which in this crate has already produced two
//! MIME allow-lists that disagreed in both directions (see
//! [`BucketConfig::allows_mime`](crate::config::BucketConfig::allows_mime)). So
//! there is one spec type and one [`parse_policy`], and each door only differs
//! in what it does with the error: boot refuses to start, the admin endpoint
//! answers `400` and leaves the running policy alone.
//!
//! # Unknown fields are refused, not ignored
//!
//! [`PolicyRuleSpec`] is `deny_unknown_fields`. A misspelt `require_unexpird`
//! would otherwise deserialize into a rule with no such condition — a rule that
//! permits strictly more than it reads as permitting, which is the failure mode
//! the whole closed-vocabulary design exists to prevent.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{BucketPolicy, PolicyMethod, PolicyPrincipal, PolicyRule};

/// One permit rule as an operator writes it: strings, before validation.
///
/// This is the shape of a `[[storage.<name>.policies]]` TOML entry and of an
/// element of the admin endpoint's `rules` array. There is deliberately no
/// `effect` field — rules permit, denial is the fallthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRuleSpec {
    /// Operations permitted: `read` | `write` | `overwrite` | `delete` | `list`.
    pub methods:           Vec<String>,
    /// Who they are permitted to: `owner` | `authenticated` | `anonymous` |
    /// `signed_url` | `role:<name>`.
    pub principal:         String,
    /// Restrict the rule to keys starting with this prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_prefix:        Option<String>,
    /// RFC3339 instant before which this grant does not apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before:        Option<String>,
    /// RFC3339 instant at and after which this grant no longer applies. The
    /// bound is exclusive — see [`PolicyRule::not_after`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_after:         Option<String>,
    /// Require the object to carry an expiry still in the future.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub require_unexpired: bool,
    /// Token claims the caller must carry, compared for exact equality.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub require_claims:    BTreeMap<String, String>,
    /// Object metadata the object must carry, compared for exact equality
    /// (#1099). Holds only for a caller who could not have written it — see
    /// [`PolicyRule::require_metadata`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub require_metadata:  BTreeMap<String, String>,
}

/// Why a policy was refused, and by which rule.
///
/// `rule_index` is carried separately from the message so each door can render
/// it in its own idiom — boot prefixes the bucket section, the admin endpoint
/// puts it in a JSON field an operator's tooling can read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySpecError {
    /// Which rule is at fault, when the fault is attributable to one.
    pub rule_index: Option<usize>,
    /// What is wrong with it.
    pub message:    String,
}

impl PolicySpecError {
    fn rule(index: usize, message: impl Into<String>) -> Self {
        Self {
            rule_index: Some(index),
            message:    message.into(),
        }
    }
}

impl std::fmt::Display for PolicySpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.rule_index {
            Some(index) => write!(f, "policy rule {index}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for PolicySpecError {}

/// Turn operator-written rules into a policy the evaluator can run.
///
/// An empty list is accepted and permits nothing — that is a deliberate
/// lock-down, distinct from *no* policy (which leaves the coarse `access` mode
/// governing).
///
/// # Errors
///
/// [`PolicySpecError`] on an unknown method or principal spelling, a rule that
/// lists no methods, a malformed RFC3339 bound, or a window that can never be
/// open. Every one of these is refused rather than dropped: a rule that quietly
/// loses its narrowing condition permits more than it reads as permitting.
pub fn parse_policy(specs: &[PolicyRuleSpec]) -> Result<BucketPolicy, PolicySpecError> {
    let mut rules = Vec::with_capacity(specs.len());
    for (index, spec) in specs.iter().enumerate() {
        let methods = spec
            .methods
            .iter()
            .map(|m| PolicyMethod::parse(m))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PolicySpecError::rule(index, e))?;
        if methods.is_empty() {
            return Err(PolicySpecError::rule(
                index,
                "lists no methods; a rule that permits nothing is a configuration mistake, not a \
                 denial",
            ));
        }
        let principal =
            PolicyPrincipal::parse(&spec.principal).map_err(|e| PolicySpecError::rule(index, e))?;
        let not_before = parse_instant(spec.not_before.as_deref(), "not_before", index)?;
        let not_after = parse_instant(spec.not_after.as_deref(), "not_after", index)?;
        if let (Some(start), Some(end)) = (not_before, not_after) {
            if start >= end {
                return Err(PolicySpecError::rule(
                    index,
                    format!(
                        "not_before ({start}) is not before not_after ({end}), so the rule can \
                         never permit anything"
                    ),
                ));
            }
        }
        // #1099: `require_metadata` is answered against whether the gated
        // caller may WRITE this object's metadata, which is another decision by
        // this same policy. A rule that grants `set_metadata` and also carries
        // `require_metadata` would make that decision depend on itself.
        //
        // Refused here rather than guarded at evaluation time, for the reason
        // every other malformed policy is refused at the door: a runtime guard
        // is a rule whose behaviour an operator cannot read off their config.
        // Such a rule is self-defeating anyway — the condition can never hold
        // for anyone the rule grants `set_metadata` to.
        if methods.contains(&PolicyMethod::SetMetadata) && !spec.require_metadata.is_empty() {
            return Err(PolicySpecError::rule(
                index,
                "grants \"set_metadata\" and also carries `require_metadata`; that condition is \
                 answered by asking whether the caller may set metadata, so the rule would \
                 decide itself. It can never hold for a caller this rule grants set_metadata \
                 to — split it into two rules",
            ));
        }
        let require_metadata = crate::policy::validate_metadata(&spec.require_metadata)
            .map_err(|e| PolicySpecError::rule(index, e))?;
        rules.push(PolicyRule {
            methods,
            principal,
            key_prefix: spec.key_prefix.clone(),
            not_before,
            not_after,
            require_unexpired: spec.require_unexpired,
            require_claims: spec.require_claims.clone(),
            require_metadata,
        });
    }
    Ok(BucketPolicy { rules })
}

/// Render a policy back into the spec form, so a `GET` can show the policy that
/// is actually governing whatever source it came from.
///
/// [`parse_policy`] round-trips this exactly; the property is asserted in the
/// tests, which is what lets an operator `GET` a config-file policy, edit it,
/// and `PUT` it back without the meaning shifting.
#[must_use]
pub fn policy_to_specs(policy: &BucketPolicy) -> Vec<PolicyRuleSpec> {
    policy
        .rules
        .iter()
        .map(|rule| PolicyRuleSpec {
            methods:           rule.methods.iter().map(|m| m.as_str().to_string()).collect(),
            principal:         rule.principal.render(),
            key_prefix:        rule.key_prefix.clone(),
            not_before:        rule.not_before.map(render_instant),
            not_after:         rule.not_after.map(render_instant),
            require_unexpired: rule.require_unexpired,
            require_claims:    rule.require_claims.clone(),
            require_metadata:  rule.require_metadata.clone(),
        })
        .collect()
}

/// RFC3339 with a `Z` offset — the spelling [`parse_instant`] accepts back.
fn render_instant(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn parse_instant(
    raw: Option<&str>,
    field: &str,
    index: usize,
) -> Result<Option<DateTime<Utc>>, PolicySpecError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| Some(parsed.with_timezone(&Utc)))
        .map_err(|e| {
            PolicySpecError::rule(
                index,
                format!(
                    "{field} = {raw:?} is not a valid RFC3339 timestamp ({e}); expected e.g. \
                 \"2026-01-31T00:00:00Z\""
                ),
            )
        })
}
