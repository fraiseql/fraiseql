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

use fraiseql_core::security::{ENRICHED_NAMESPACE_PREFIX, SecurityContext};

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

/// Resolve `ctx`'s DB identity and, on success, merge every mapped field into
/// `ctx.attributes` under the reserved namespace. All-or-nothing: on a denial or
/// a transient failure nothing is merged and the caller stops the request.
pub async fn enrich_security_context(
    resolver: &IdentityResolver,
    ctx: &mut SecurityContext,
) -> EnrichmentOutcome {
    let sub = ctx.user_id.0.clone();
    let claims = claims_for_binding(ctx);
    match resolver.resolve(&sub, &claims).await {
        IdentityResolution::Resolved(fields) => {
            for (field, value) in fields {
                ctx.attributes.insert(format!("{ENRICHED_NAMESPACE_PREFIX}{field}"), value);
            }
            EnrichmentOutcome::Proceed
        },
        IdentityResolution::Denied(_) => EnrichmentOutcome::Denied,
        IdentityResolution::Unavailable(_) => EnrichmentOutcome::Unavailable,
    }
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
    claims
}
