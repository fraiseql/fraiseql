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

/// Returns `true` unless the operator has positively declared a development
/// environment.
///
/// Re-exported from [`fraiseql_guard::deployment`], the workspace's single
/// production detector. The local copy this replaced read the same
/// `FRAISEQL_ENV` variable as `ServerConfig::is_production_mode()` but defaulted
/// an unset value to *not* production, so on any non-Kubernetes deployment the
/// server believed it was in production while this guard honoured the bypass
/// (#836).
pub use fraiseql_guard::deployment::is_production as is_production_environment;

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
    #[cfg(test)]
    if let Some(forced) = test_override::current() {
        return forced;
    }

    if !fraiseql_guard::deployment::env_opt_in(ALLOW_INSECURE_ENV) {
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
    if !fraiseql_guard::deployment::env_opt_in(NATS_ALLOW_PLAINTEXT_ENV) {
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

/// The injectable test source for the bypass decision (#907).
///
/// The guard reads process env live, and tests used to *mutate* that env
/// (`temp_env`) — so a test calling an SSRF guard bare could execute inside a
/// setter's closure, see the bypass active, and fail its rejection assertion
/// intermittently. Tests now PIN the decision instead of mutating global
/// state: [`force`] holds a lock for the guard's lifetime and answers the
/// bypass question directly, so concurrent test modules cannot observe each
/// other. Tests that exercise the real env-parsing path (this module's own
/// tests) hold the same lock via [`env_passthrough_lock`] so a pinned decision
/// is never active while they read the environment.
#[cfg(test)]
pub(crate) mod test_override {
    use std::sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU8, Ordering},
    };

    const NONE: u8 = 0;
    const FORCE_OFF: u8 = 1;
    const FORCE_ON: u8 = 2;

    static DECISION: AtomicU8 = AtomicU8::new(NONE);
    static LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> MutexGuard<'static, ()> {
        LOCK.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pin the bypass decision for the guard's lifetime; restores the live
    /// env-reading path on drop. Serialised against every other pin and
    /// against [`env_passthrough_lock`] holders.
    pub fn force(allowed: bool) -> BypassGuard {
        let guard = lock();
        DECISION.store(if allowed { FORCE_ON } else { FORCE_OFF }, Ordering::SeqCst);
        BypassGuard { _lock: guard }
    }

    /// Serialise against pins WITHOUT overriding — for tests whose subject is
    /// the real env-parsing/production-marker path.
    pub fn env_passthrough_lock() -> MutexGuard<'static, ()> {
        let guard = lock();
        debug_assert_eq!(DECISION.load(Ordering::SeqCst), NONE);
        guard
    }

    pub(super) fn current() -> Option<bool> {
        match DECISION.load(Ordering::SeqCst) {
            FORCE_OFF => Some(false),
            FORCE_ON => Some(true),
            _ => None,
        }
    }

    /// RAII pin; dropping restores the env-reading path before the lock frees.
    pub struct BypassGuard {
        _lock: MutexGuard<'static, ()>,
    }

    impl Drop for BypassGuard {
        fn drop(&mut self) {
            DECISION.store(NONE, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests;
