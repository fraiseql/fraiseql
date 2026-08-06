//! The one way a test may call an SSRF guard directly.
//!
//! `validate_outbound_url` consults `is_outbound_insecure_allowed`, whose
//! production path reads process env live. Tests used to MUTATE that env
//! (`temp_env`), so a test calling a guard bare could execute inside a
//! setter's closure, see the bypass active, and fail its rejection assertion
//! intermittently (the `10.x should be rejected: Ok(...)` CI flake — #907).
//!
//! The env mutation is gone: this wrapper PINS the bypass decision to
//! "guards engaged" via the injectable test source
//! (`insecure_guard::test_override`), serialised against every test that pins
//! it to "allowed" (the wiremock loopback dispatch tests) and against the
//! guard's own env-parsing tests. Nothing mutates the process environment, so
//! there is no race left to serialise — the lock only scopes the pin.

/// Run `f` with the SSRF bypass pinned OFF — rejection assertions cannot be
/// broken by any concurrent test, and ambient runner env (a leg exporting
/// `FRAISEQL_OBSERVERS_ALLOW_INSECURE=true`) cannot leak in.
pub fn with_ssrf_env_cleared<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
    let _pin = crate::insecure_guard::test_override::force(false);
    f();
}
