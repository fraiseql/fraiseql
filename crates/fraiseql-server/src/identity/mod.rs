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
//! The resolver is not yet wired into a request path — that is P02 (the
//! `/graphql` handler step and server construction), which removes the
//! module-scoped `dead_code` allow below. Until then these items have no
//! non-test caller.
//!
//! Enrichment requires an authenticated subject, so the whole module is gated on
//! the `auth` feature (mirroring the `enrichment_pool` the resolver uses).

// Reason: the resolver and its supporting types are exercised only by tests until
// P02 wires them into the `/graphql` handler and server construction; that phase
// removes this allow.
#![allow(dead_code)]

pub(crate) mod cache;
pub(crate) mod failure;
pub(crate) mod query;
pub(crate) mod resolver;

#[cfg(test)]
mod tests;
