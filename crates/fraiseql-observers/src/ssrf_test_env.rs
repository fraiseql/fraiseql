//! The one way a test may call an SSRF guard directly.
//!
//! `validate_outbound_url` reads `FRAISEQL_OBSERVERS_ALLOW_INSECURE` from the
//! **process** environment on every call, and the `insecure_guard` and `actions`
//! test modules set it to `true` inside `temp_env::with_vars` closures. Since
//! `cargo test` runs those modules concurrently with everything else in the
//! binary, a test that calls a guard directly can execute *inside* a setter's
//! closure, see the bypass active, and watch its rejection assertion fail.
//!
//! `temp_env` serialises on a global lock, so going through this wrapper both
//! clears the variables and orders the call against every setter.
//!
//! This lived as a private helper in `crate::tests` and was applied there only —
//! `executor::tests`'s three SSRF cases called `resolve_url` bare and flaked in
//! CI accordingly (`10.x should be rejected: Ok(...)`, which is reachable by no
//! other path than the bypass). It is crate-visible now so there is one copy
//! rather than one per test module that remembers.

/// Run `f` with every SSRF-guard environment variable cleared, serialised
/// against the tests that set them.
pub fn with_ssrf_env_cleared<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
    temp_env::with_vars(
        [
            (crate::insecure_guard::ALLOW_INSECURE_ENV, None::<&str>),
            ("FRAISEQL_ENV", None),
            ("FRAISEQL_PROFILE", None),
            ("KUBERNETES_SERVICE_HOST", None),
        ],
        f,
    );
}
