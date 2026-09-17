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
// `enrich_security_context` is deliberately NOT re-exported: since #1336 every caller
// outside this module goes through `resolve_request_identity`, which is the seam the
// transports share and the build gate pins. Re-exporting the inner function would offer
// a second door to the same rule, which is the shape this issue was.
pub(crate) use apply::{EnrichmentOutcome, resolve_request_identity};
use fraiseql_core::schema::CompiledSchema;
// Public because `ServerConfig.identity` is a public field of this type: before
// #1336 an embedder could not name it, so `[identity.enrichment]` was configurable
// from TOML and unreachable from Rust — a public field with no way to build a value
// for it. `IdentityResolver` comes with it, since `AppState::with_identity_resolver`
// is public and takes one.
pub use resolver::{EnrichmentQueryConfig, IdentityConfig, IdentityResolver};

/// Whether the compiled schema declares any consumer of enriched identity.
///
/// Delegates to the engine's own scan so the startup warning and the engine's
/// enrichment backstop can never disagree about what "declares a consumer" means.
pub(crate) fn schema_declares_enrichment_consumer(schema: &CompiledSchema) -> bool {
    schema.declares_enrichment_consumer()
}

// `pub(crate)` so a transport's own tests can drive a resolver without a second
// mock store: each transport owns a producer (#1336), and a mock per producer is
// how two of them would come to disagree about what a denial looks like.
#[cfg(test)]
pub(crate) mod tests;
