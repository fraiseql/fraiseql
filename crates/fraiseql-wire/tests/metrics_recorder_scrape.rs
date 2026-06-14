#![allow(clippy::unwrap_used, clippy::expect_used)] // Reason: test code, panics acceptable

//! Audit H45: a wire `metrics`-facade emission must be captured by an installed
//! Prometheus recorder and appear in its scrape output.
//!
//! This is the regression guard for the facade-version mismatch: when `fraiseql-wire`
//! emitted via `metrics` 0.22 but the only available recorder (the server's
//! `metrics-exporter-prometheus`) was built against `metrics` 0.24, the emission
//! and the recorder bound to two different process-global statics, so every wire
//! metric was silently dropped. With both on `metrics` 0.24 the counter renders.

use fraiseql_wire::metrics;
use metrics_exporter_prometheus::PrometheusBuilder;

#[test]
fn wire_counter_appears_in_prometheus_scrape() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("install Prometheus recorder");

    // Emit a real wire counter via the `metrics` facade.
    metrics::counters::query_success("scrape_test_entity");

    let scrape = handle.render();
    assert!(
        scrape.contains("fraiseql_query_success_total"),
        "the wire counter must appear in the scrape output:\n{scrape}"
    );
    assert!(
        scrape.contains("scrape_test_entity"),
        "the entity label must be present in the scrape output:\n{scrape}"
    );
}
