//! Read-path consumer (consumer A): resolve the request subject's DB identity
//! and merge it into the security context under the forge-proof
//! `fraiseql.enriched.*` namespace (DESIGN §3).
//!
//! Fail-closed — a denial or a transient failure stops the request before
//! dispatch; the caller maps the coarse [`EnrichmentOutcome`] to an HTTP status.
//! The subject and any denial reason are logged server-side by the resolver
//! (DESIGN §5.4), so the caller's outward response stays generic (no actor-table
//! existence oracle).

use std::{collections::HashMap, sync::Arc};

// The outcome type and its two outward messages live in `fraiseql-core` (#1349): the
// Flight transport resolves through an object-safe seam from a crate that cannot depend
// on this one, and it must answer a denial exactly as the six transports here do. One
// type, so "generic body, no actor-table oracle" stays a single decision.
pub use fraiseql_core::security::EnrichmentOutcome;
use fraiseql_core::security::{
    ActorType, BoxFuture, ENRICHED_NAMESPACE_PREFIX, EnrichmentMark, IdentityEnricher,
    SecurityContext,
};

use super::{failure::IdentityResolution, resolver::IdentityResolver};
use crate::api::rbac_management::assigned_roles::AssignedRoles;

/// What a transport resolves a request's principal against.
///
/// Every transport holds one and hands it to `resolve_request_identity`, so the two
/// sources below are applied by the same call everywhere and a transport cannot carry
/// one and forget the other.
#[derive(Clone, Default)]
pub struct RequestIdentity {
    /// `[identity.enrichment]`'s resolver, when enrichment is enabled (#539).
    resolver:       Option<Arc<IdentityResolver>>,
    /// The roles assigned through the RBAC API and SCIM, when either can mount (#1632).
    assigned_roles: Option<Arc<AssignedRoles>>,
}

impl RequestIdentity {
    /// Resolve enriched identity through `resolver`.
    #[must_use]
    pub fn with_resolver(mut self, resolver: Arc<IdentityResolver>) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Grant human principals the roles `assigned_roles` reads.
    #[must_use]
    pub fn with_assigned_roles(mut self, assigned_roles: Arc<AssignedRoles>) -> Self {
        self.assigned_roles = Some(assigned_roles);
        self
    }

    /// The enrichment resolver, if enrichment is enabled.
    #[must_use]
    pub const fn resolver(&self) -> Option<&Arc<IdentityResolver>> {
        self.resolver.as_ref()
    }

    /// Whether there is nothing to resolve against.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.resolver.is_none() && self.assigned_roles.is_none()
    }
}

/// Add the roles assigned to a human principal to the ones its token carries (#1632).
///
/// Only a human user gains them. A service account or a system job holds exactly the
/// authority its ceiling declares, and an agent acting for a user (`act`) keeps the
/// token's roles only: a delegate never inherits what is stored for the person.
///
/// A failed lookup is [`EnrichmentOutcome::Unavailable`], not "no roles": a request is
/// either answered with the principal's whole authority or not at all.
async fn add_assigned_roles(
    assigned_roles: &AssignedRoles,
    ctx: &mut SecurityContext,
) -> Result<(), EnrichmentOutcome> {
    if ctx.actor_type() != ActorType::HumanUser {
        return Ok(());
    }
    let tenant = ctx.tenant_id.as_ref().map(|tenant| tenant.as_str());
    match assigned_roles.of(&ctx.user_id.0, tenant).await {
        Ok(roles) => {
            for role in roles {
                if !ctx.roles.contains(&role) {
                    ctx.roles.push(role);
                }
            }
            Ok(())
        },
        Err(e) => {
            tracing::warn!(sub = %ctx.user_id.0, error = %e, "assigned roles could not be read");
            Err(EnrichmentOutcome::Unavailable)
        },
    }
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
    let claims = claims_for_binding(ctx, resolver.tenant_claim());
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
/// it (#1336). It adds the principal's assigned roles (#1632), then resolves its
/// enriched identity.
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
/// ⚠ An absent resolver **does not** mark the context. A deployment whose schema
/// declares an enrichment consumer is refused at boot unless enrichment is enabled, so
/// "no resolver" and "a consumer to satisfy" cannot both be true — and if a transport's
/// state failed to carry the resolver, the engine refuses the request rather than
/// treating silence as permission.
pub async fn resolve_request_identity(
    identity: &RequestIdentity,
    security_context: Option<&mut SecurityContext>,
) -> EnrichmentOutcome {
    let Some(ctx) = security_context else {
        return EnrichmentOutcome::Proceed;
    };
    if let Some(assigned_roles) = identity.assigned_roles.as_deref() {
        if let Err(outcome) = add_assigned_roles(assigned_roles, ctx).await {
            return outcome;
        }
    }
    match identity.resolver.as_deref() {
        Some(resolver) => enrich_security_context(resolver, ctx).await,
        None => EnrichmentOutcome::Proceed,
    }
}

/// Build the claim map the resolver binds `$param`s from: every claim the token
/// carried, as [`SecurityContext::jwt_claim`] reads it — the raw forwarded
/// attributes, plus the registered claims the validator lifts into their own fields
/// (`sub`, `email`, `name`/`display_name`, `iss`). Exposing `iss` lets a
/// multi-issuer app bind `$iss` for cache correctness (DESIGN §6).
///
/// Only the schema's tenant claim can bind an assigned tenant, exactly as
/// [`SecurityContext::jwt_claim`] resolves it (#1388); any other name, `$org_id` and
/// `$tenant_id` included, is the principal's own claim or a missing bind.
fn claims_for_binding(
    ctx: &SecurityContext,
    tenant_claim: &str,
) -> HashMap<String, serde_json::Value> {
    let mut claims = ctx.attributes.clone();
    for name in ["sub", "email", "name", "display_name", "iss", tenant_claim] {
        if let Some(value) = ctx.jwt_claim(name, tenant_claim) {
            claims.entry(name.to_owned()).or_insert(value);
        }
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

/// The Flight transport's route to the same resolution every other transport uses
/// (#1349).
///
/// `fraiseql-arrow` cannot depend on this crate, so it holds an
/// [`IdentityEnricher`] and the server hands it this. The body is
/// `resolve_request_identity` verbatim — not a second implementation, which is the
/// shape #1336 was.
impl IdentityEnricher for RequestIdentity {
    fn enrich<'a>(&'a self, ctx: &'a mut SecurityContext) -> BoxFuture<'a, EnrichmentOutcome> {
        Box::pin(resolve_request_identity(self, Some(ctx)))
    }
}
