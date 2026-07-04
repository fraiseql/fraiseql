//! Enriched-identity resolution: a request-scoped `sub → DB → identity` mapping.
//!
//! Resolved once per request, cached, and fail-closed, it feeds both
//! read-scoping (session variables / injected params) and verified
//! sender-identity (`send_email`).
//!
//! # Phase P00 — proven core, ported verbatim
//!
//! This module currently holds only the pieces ported unchanged from #242
//! (`routes/enrichment.rs`, shipped in `v2.2.1` on `main`, never forward-ported
//! to `dev`):
//!
//! - `query::prepare_enrichment_query` — safe named-parameter binding (`$name` → positional `$N`,
//!   values bound out-of-band, never interpolated), plus its adversarial test suite (SQL injection
//!   / comment in a claim value, overlapping `$email`/`$email_verified`, unicode, `$1` passthrough,
//!   missing-param error);
//! - `cache::EnrichmentCache` — the in-memory TTL cache (`get`/`insert`/expiry).
//!
//! The resolver, the `IdentityResolution` failure model, the negative cache,
//! the bound-`$param`-tuple cache key, and the two call sites land in later
//! phases (see `.phases/539-enriched-identity/DESIGN.md`). Until the resolver
//! consumes them, these items have no non-test caller — hence the module-scoped
//! `dead_code` allow below, which P01 removes.
//!
//! Enrichment requires an authenticated subject, so the whole module is gated on
//! the `auth` feature (mirroring the `enrichment_pool` the resolver will use).

// Reason: P00 is a pure port of #242's proven core, wired to nothing yet; the
// resolver that consumes these items lands in P01, which removes this allow.
#![allow(dead_code)]

pub(crate) mod cache;
pub(crate) mod query;

#[cfg(test)]
mod tests;
