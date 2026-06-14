//! Process-global `metrics`-facade recorder (Prometheus).
//!
//! The server exposes its own metrics through the hand-rolled [`MetricsCollector`]
//! (see [`crate::metrics_server`]), but several dependencies — most notably
//! `fraiseql-wire` — report observability through the `metrics` *facade* crate
//! (`counter!`/`gauge!`/`histogram!`). Those emissions go to whatever global
//! recorder the process installed; if none is installed they are silently
//! dropped. Historically no recorder was installed *and* the facade versions did
//! not match the exporter's, so ~40 wire metrics never surfaced (audit H45).
//!
//! This module installs a single Prometheus recorder at startup and exposes its
//! text rendering so the `/metrics` endpoint can append the facade metrics to its
//! hand-rolled output.

use std::sync::OnceLock;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// The installed Prometheus handle, set once at startup.
static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the process-global Prometheus recorder for `metrics`-facade emissions.
///
/// Idempotent: a second call is a no-op. If a recorder is already installed by
/// some other component (or installation fails), the error is logged and facade
/// metrics simply remain unexported — installation never aborts startup.
pub fn install() {
    if HANDLE.get().is_some() {
        return;
    }
    match PrometheusBuilder::new().install_recorder() {
        Ok(handle) => {
            // Ignore the (impossible, given the `get()` guard above) race where a
            // concurrent caller won the `set`.
            let _ = HANDLE.set(handle);
            tracing::info!("metrics: Prometheus facade recorder installed");
        },
        Err(e) => {
            tracing::warn!(
                error = %e,
                "metrics: failed to install Prometheus facade recorder; \
                 facade metrics (e.g. fraiseql-wire) will not be exported"
            );
        },
    }
}

/// Render the captured facade metrics in Prometheus text format.
///
/// Returns an empty string when no recorder is installed (e.g. the `metrics`
/// feature is built but `install()` was never called), so callers can append the
/// result unconditionally.
#[must_use]
pub fn render() -> String {
    HANDLE.get().map(PrometheusHandle::render).unwrap_or_default()
}
