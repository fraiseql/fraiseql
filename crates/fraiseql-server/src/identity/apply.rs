//! Read-path consumer (consumer A): resolve the request subject's DB identity
//! and merge it into the security context under the forge-proof
//! `fraiseql.enriched.*` namespace (DESIGN §3).
//!
//! Fail-closed — a denial or a transient failure stops the request before
//! dispatch; the caller maps the coarse [`EnrichmentOutcome`] to an HTTP status.
//! The subject and any denial reason are logged server-side by the resolver
//! (DESIGN §5.4), so the caller's outward response stays generic (no actor-table
//! existence oracle).

use std::collections::HashMap;

use fraiseql_core::security::{ENRICHED_NAMESPACE_PREFIX, EnrichmentMark, SecurityContext};

use super::{failure::IdentityResolution, resolver::IdentityResolver};

/// What the caller should do after an enrichment attempt (DESIGN §3.1, §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichmentOutcome {
    /// Identity resolved and merged — continue to dispatch.
    Proceed,
    /// Permanent denial — fail closed (HTTP 403) before any data query runs.
    Denied,
    /// Transient resolver failure — fail the request (HTTP 503), never fall
    /// through to an unscoped query.
    Unavailable,
}

impl EnrichmentOutcome {
    /// The outward body for a denial. Generic by design (DESIGN §5.4): the precise
    /// `DenyReason` is logged server-side, never surfaced, so a caller cannot use the
    /// response as an actor-table existence oracle. One constant rather than four
    /// literals, because a transport that phrased it differently would leak the
    /// difference between "unknown subject" and "denied subject" (#1323's shape).
    pub(crate) const DENIED_MESSAGE: &'static str = "Access denied";
    /// The outward body for a transient resolver failure — distinct from a denial,
    /// because a client may retry this one and must not retry the other.
    pub(crate) const UNAVAILABLE_MESSAGE: &'static str =
        "Identity resolution temporarily unavailable";
}

/// Resolve `ctx`'s DB identity and, on success, merge every mapped field into
/// `ctx.attributes` under the reserved namespace. All-or-nothing: on a denial or
/// a transient failure nothing is merged and the caller stops the request.
pub async fn enrich_security_context(
    resolver: &IdentityResolver,
    ctx: &mut SecurityContext,
) -> EnrichmentOutcome {
    // ADR-0018 decision 5: a principal that already carries server-injected
    // `fraiseql.enriched.*` fields (a service account's `static_enriched`) is enriched
    // **in lieu of** the DB resolve — skip re-resolution. Inbound `fraiseql.*` claims are
    // stripped by the request extractor, so a human principal never reaches here with
    // enriched attributes present; only a server-injected set does.
    if ctx.attributes.keys().any(|k| k.starts_with(ENRICHED_NAMESPACE_PREFIX)) {
        ctx.mark_enrichment(EnrichmentMark::Resolved);
        return EnrichmentOutcome::Proceed;
    }
    let sub = ctx.user_id.0.clone();
    let claims = claims_for_binding(ctx);
    match resolver.resolve(&sub, &claims).await {
        IdentityResolution::Resolved(fields) => {
            for (field, value) in fields {
                ctx.attributes.insert(format!("{ENRICHED_NAMESPACE_PREFIX}{field}"), value);
            }
            ctx.mark_enrichment(EnrichmentMark::Resolved);
            EnrichmentOutcome::Proceed
        },
        IdentityResolution::Denied(_) => EnrichmentOutcome::Denied,
        IdentityResolution::Unavailable(_) => EnrichmentOutcome::Unavailable,
    }
}

/// The one call a transport makes between authenticating a request and dispatching
/// it (#1336).
///
/// `[identity.enrichment]`'s contract is "when enrichment is enabled, **every**
/// authenticated request resolves and fail-closes". Before this existed the rule was
/// a per-transport responsibility, and four transports out of five did not discharge
/// it: REST, MCP and gRPC built a `SecurityContext` and dispatched it unresolved, so
/// an enriched read failed for every caller and an unknown subject was served where
/// `/graphql` answered 403. That is the same shape as #810 (`require_auth` honoured by
/// one handler out of six) and it has the same answer: make the resolution part of
/// obtaining a usable context, so a transport cannot forget it by omission.
///
/// Placement is deliberately **above** the engine rather than inside it. The engine is
/// not below every transport — gRPC's read arms go adapter-direct (#1348) — and
/// `claims_for_binding` reads `ctx.attributes`, which only exist once the shared
/// context builder has run. A seam on `RuntimeConfig` would also have been inert for
/// every tenant-keyed request (#1333), because tenant executors are built with
/// `RuntimeConfig::default()`. None of that applies here: the resolver comes from the
/// server's state.
///
/// Returns [`EnrichmentOutcome::Proceed`] for an anonymous request — there is no
/// subject to resolve — and leaves it unmarked, which is correct: the engine's guard
/// asks about principals.
///
/// ⚠ An absent `resolver` **does not** mark the context. A deployment whose schema
/// declares an enrichment consumer is refused at boot unless enrichment is enabled, so
/// "no resolver" and "a consumer to satisfy" cannot both be true — and if a transport's
/// state failed to carry the resolver, the engine refuses the request rather than
/// treating silence as permission.
pub async fn resolve_request_identity(
    resolver: Option<&IdentityResolver>,
    security_context: Option<&mut SecurityContext>,
) -> EnrichmentOutcome {
    let (Some(resolver), Some(ctx)) = (resolver, security_context) else {
        return EnrichmentOutcome::Proceed;
    };
    enrich_security_context(resolver, ctx).await
}

/// Build the claim map the resolver binds `$param`s from: the raw forwarded
/// attributes, plus the well-known identity fields under their conventional
/// names (attributes win on a collision, mirroring the `Jwt` source). Exposing
/// `iss` lets a multi-issuer app bind `$iss` for cache correctness (DESIGN §6).
fn claims_for_binding(ctx: &SecurityContext) -> HashMap<String, serde_json::Value> {
    let mut claims = ctx.attributes.clone();
    claims
        .entry("sub".to_owned())
        .or_insert_with(|| serde_json::Value::String(ctx.user_id.0.clone()));
    if let Some(tenant) = &ctx.tenant_id {
        let value = serde_json::Value::String(tenant.0.clone());
        claims.entry("tenant_id".to_owned()).or_insert_with(|| value.clone());
        claims.entry("org_id".to_owned()).or_insert(value);
    }
    if let Some(email) = &ctx.email {
        claims
            .entry("email".to_owned())
            .or_insert_with(|| serde_json::Value::String(email.clone()));
    }
    if let Some(name) = &ctx.display_name {
        let value = serde_json::Value::String(name.clone());
        claims.entry("name".to_owned()).or_insert_with(|| value.clone());
        claims.entry("display_name".to_owned()).or_insert(value);
    }
    if let Some(iss) = &ctx.issuer {
        claims
            .entry("iss".to_owned())
            .or_insert_with(|| serde_json::Value::String(iss.clone()));
    }
    // `$claims` — the whole set as one JSON object, so a provisioning statement
    // can hand an IdP's token to a function that stores what it chooses (#1324)
    // instead of the query naming every claim it might ever want. Built last, so
    // it holds the well-known fields too; it does not hold itself.
    let snapshot: serde_json::Map<String, serde_json::Value> =
        claims.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    claims.entry("claims".to_owned()).or_insert(serde_json::Value::Object(snapshot));
    claims
}
