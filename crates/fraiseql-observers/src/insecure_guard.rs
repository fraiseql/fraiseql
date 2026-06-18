//! Centralised guard for `FRAISEQL_OBSERVERS_ALLOW_INSECURE`.
//!
//! The env var disables every outbound SSRF check (scheme allowlist,
//! private-IP blocklist, DNS-rebinding defence) in the observer outbound
//! dispatch path.  Pre-v2.4.0 it was checked independently at four sites
//! (`actions.rs`, `ssrf.rs` x2, `executor/dispatch.rs`) and warned exactly
//! once on first use (`std::sync::Once`).
//!
//! This module is the single source of truth.  All four sites now call
//! [`is_outbound_insecure_allowed`] which:
//!
//! - **Refuses** to honor the bypass when any production-marker env var is set, logging a
//!   structured `ERROR` once per process and a per-call `WARN` so the bypass attempt is visible in
//!   the log stream.
//! - **Honors** the bypass in development/test environments only when the env var is set to `1` or
//!   `true` (case-insensitive), and emits a `WARN` on *every* dispatch — operators must see the
//!   bypass active in the log stream, not just once at startup (#347).
//!
//! ## Production markers
//!
//! Any of the following env vars indicate a production deployment and
//! cause the bypass to be refused even when the bypass var is set:
//!
//! - `KUBERNETES_SERVICE_HOST` — automatic in any Kubernetes pod.
//! - `FRAISEQL_ENV=production` (case-insensitive).
//! - `FRAISEQL_PROFILE=production` (case-insensitive).
//!
//! Adding new production markers: extend [`is_production_environment`].

use std::sync::atomic::{AtomicBool, Ordering};

use tracing::{error, warn};

/// Env var that requests the SSRF bypass.  Accepted values: `1`, `true`
/// (case-insensitive).  Any other value (or absence) leaves SSRF guards
/// engaged.
pub const ALLOW_INSECURE_ENV: &str = "FRAISEQL_OBSERVERS_ALLOW_INSECURE";

/// Whether the production refusal has already been logged at ERROR.  We
/// only want the structured ERROR once per process (it would otherwise
/// fire on every webhook dispatch and overwhelm log aggregation), but the
/// per-call `WARN` from [`is_outbound_insecure_allowed`] still fires
/// every time so operators see the bypass attempt at every dispatch.
static PRODUCTION_REFUSAL_LOGGED: AtomicBool = AtomicBool::new(false);

/// Returns `true` when any production-marker env var is set.
///
/// Markers are intentionally broad — false-positives (refusing the
/// bypass in something that "looks like" production) are acceptable;
/// false-negatives (silently allowing the bypass in real production)
/// are not.
#[must_use]
pub fn is_production_environment() -> bool {
    if std::env::var_os("KUBERNETES_SERVICE_HOST").is_some() {
        return true;
    }
    if matches_production(&std::env::var("FRAISEQL_ENV").unwrap_or_default()) {
        return true;
    }
    if matches_production(&std::env::var("FRAISEQL_PROFILE").unwrap_or_default()) {
        return true;
    }
    false
}

// Reason: `eq_ignore_ascii_case` is not const-stable yet (rust-lang/rust#129041);
// keeping this fn non-const lets us use the idiomatic str method.
#[allow(clippy::missing_const_for_fn)]
fn matches_production(value: &str) -> bool {
    value.eq_ignore_ascii_case("production") || value.eq_ignore_ascii_case("prod")
}

/// Returns `true` only when the bypass env var is set AND no production
/// marker is present.
///
/// In production with the bypass set, this logs a structured `ERROR`
/// once per process plus a per-call `WARN` so the refused bypass is
/// visible at every dispatch.
///
/// In dev with the bypass set, this emits a `WARN` on every call (the
/// old `std::sync::Once` warn-once was too easy to miss in a streaming
/// log aggregator after the first webhook).
#[must_use]
pub fn is_outbound_insecure_allowed() -> bool {
    let requested = std::env::var(ALLOW_INSECURE_ENV)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    if !requested {
        return false;
    }

    if is_production_environment() {
        if PRODUCTION_REFUSAL_LOGGED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            error!(
                "{ALLOW_INSECURE_ENV}=true requested in a production environment \
                 (KUBERNETES_SERVICE_HOST set, FRAISEQL_ENV=production, or \
                 FRAISEQL_PROFILE=production). SSRF guards remain engaged. \
                 This bypass is intended for local development and integration \
                 testing only and is refused in production."
            );
        }
        warn!(
            target: "fraiseql_observers::insecure_guard",
            "Refused {ALLOW_INSECURE_ENV} bypass in production environment"
        );
        return false;
    }

    warn!(
        target: "fraiseql_observers::insecure_guard",
        "{ALLOW_INSECURE_ENV}=true — SSRF guards bypassed for this outbound \
         dispatch. This MUST NOT be set in production."
    );
    true
}

/// Env var that allows plaintext `nats://` NATS connections (no transport TLS).
/// Accepted values: `1`, `true` (case-insensitive).  Refused in production
/// regardless of the value.
pub const NATS_ALLOW_PLAINTEXT_ENV: &str = "FRAISEQL_NATS_ALLOW_PLAINTEXT";

/// Whether the NATS-plaintext production refusal has already been logged at
/// ERROR (once per process; see [`PRODUCTION_REFUSAL_LOGGED`] for the rationale).
static NATS_PLAINTEXT_REFUSAL_LOGGED: AtomicBool = AtomicBool::new(false);

/// Returns `true` only when [`NATS_ALLOW_PLAINTEXT_ENV`] is set AND no production
/// marker is present.
///
/// Plaintext `nats://` carries change-log events with no transport encryption.
/// It is refused by default; this escape hatch mirrors
/// [`is_outbound_insecure_allowed`] (honoured in dev/test only, refused in
/// production via [`is_production_environment`]) but is a **separate** flag so
/// allowing plaintext NATS does not also disable the outbound SSRF guards.
#[must_use]
pub fn is_nats_plaintext_allowed() -> bool {
    let requested = std::env::var(NATS_ALLOW_PLAINTEXT_ENV)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    if !requested {
        return false;
    }

    if is_production_environment() {
        if NATS_PLAINTEXT_REFUSAL_LOGGED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            error!(
                "{NATS_ALLOW_PLAINTEXT_ENV}=true requested in a production environment \
                 (KUBERNETES_SERVICE_HOST set, FRAISEQL_ENV=production, or \
                 FRAISEQL_PROFILE=production). Plaintext nats:// remains refused — use \
                 tls:// for the NATS transport. This bypass is for local development only."
            );
        }
        warn!(
            target: "fraiseql_observers::insecure_guard",
            "Refused {NATS_ALLOW_PLAINTEXT_ENV} bypass in production environment"
        );
        return false;
    }

    warn!(
        target: "fraiseql_observers::insecure_guard",
        "{NATS_ALLOW_PLAINTEXT_ENV}=true — NATS transport allowed over plaintext nats:// \
         (no TLS). This MUST NOT be set in production."
    );
    true
}

#[cfg(test)]
mod tests;
