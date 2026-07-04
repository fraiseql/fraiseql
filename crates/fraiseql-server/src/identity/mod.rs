//! Enriched-identity resolution: a request-scoped `sub → DB → identity` mapping.
//!
//! Resolved once per request, cached, and fail-closed, it feeds both
//! read-scoping (session variables / injected params) and verified
//! sender-identity (`send_email`).
//!
//! # Structure
//!
//! - `query` — safe named-parameter binding (`$name` → positional `$N`, values bound out-of-band,
//!   never interpolated). Ported verbatim from #242, with the missing-param error refined to a
//!   structured `MissingParam`.
//! - `cache` — the identity cache (DESIGN §6): keyed on the bound-`$param` tuple, positive and
//!   negative TTL, `flush(sub)`.
//! - `failure` — the `IdentityResolution` model (DESIGN §5): `Resolved` / `Denied` / `Unavailable`,
//!   fail-closed at source.
//! - `resolver` — the shared `IdentityResolver`: bind → cache → fetch (≤2 rows) → classify → cache,
//!   with server-side denial logging.
//!
//! The read-path consumer (`apply::enrich_security_context`) is wired into the
//! `/graphql` handler, and the cache flush surface (`admin::identity_admin_router`)
//! into the admin API. The DB-backed sender is the one seam whose consumer lands
//! elsewhere (the hardening-train `send_email` op), so it carries a scoped
//! `dead_code` allow at its definition rather than a blanket module allow.
//!
//! Enrichment requires an authenticated subject, so the whole module is gated on
//! the `auth` feature (mirroring the `enrichment_pool` the resolver uses).

pub(crate) mod admin;
pub(crate) mod apply;
pub(crate) mod cache;
pub(crate) mod failure;
pub(crate) mod query;
pub(crate) mod resolver;
pub(crate) mod sender;

pub(crate) use admin::identity_admin_router;
pub(crate) use apply::{EnrichmentOutcome, enrich_security_context};
use fraiseql_core::schema::{CompiledSchema, InjectedParamSource, SessionVariableSource};
pub(crate) use resolver::{IdentityConfig, IdentityResolver};

/// Whether the compiled schema declares any consumer of enriched identity — a
/// `SessionVariableSource::Enrichment` or an `InjectedParamSource::Enrichment`.
///
/// Used only to decide whether an enabled-but-unused enrichment profile warrants
/// a loud startup warning (DESIGN §7). The per-request fail-closed boundary
/// itself never depends on this scan — that would reintroduce the exact
/// declaration-conditional silent-skip the design fights.
pub(crate) fn schema_declares_enrichment_consumer(schema: &CompiledSchema) -> bool {
    let in_session_vars = schema
        .session_variables
        .variables
        .iter()
        .any(|mapping| matches!(mapping.source, SessionVariableSource::Enrichment { .. }));
    let in_inject_params = schema
        .queries
        .iter()
        .flat_map(|q| q.inject_params.values())
        .chain(schema.mutations.iter().flat_map(|m| m.inject_params.values()))
        .any(|source| matches!(source, InjectedParamSource::Enrichment(_)));
    in_session_vars || in_inject_params
}

#[cfg(test)]
mod tests;
