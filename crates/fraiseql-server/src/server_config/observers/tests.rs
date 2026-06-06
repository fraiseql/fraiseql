//! Tests for the `[observers]` / `[observers.runtime]` config contract (#342).
#![allow(clippy::unwrap_used)] // Reason: test code; parse failures should panic to surface the bug.

use super::{ObserverConfig, ObserverRuntimeSettings};

// ── `[observers.runtime]` is strict ─────────────────────────────────────────

#[test]
fn runtime_subtable_parses_all_known_keys() {
    let cfg: ObserverConfig = toml::from_str(
        "enabled = true\n\
         [runtime]\n\
         poll_interval_ms = 500\n\
         batch_size = 250\n\
         channel_capacity = 2000\n\
         auto_reload = false\n\
         reload_interval_secs = 30\n",
    )
    .unwrap();

    assert!(cfg.enabled);
    assert_eq!(cfg.runtime.poll_interval_ms, 500);
    assert_eq!(cfg.runtime.batch_size, 250);
    assert_eq!(cfg.runtime.channel_capacity, 2000);
    assert!(!cfg.runtime.auto_reload);
    assert_eq!(cfg.runtime.reload_interval_secs, 30);
    assert!(cfg.misplaced_runtime_keys().is_empty());
}

#[test]
fn runtime_pool_subtable_parses() {
    let cfg: ObserverConfig = toml::from_str(
        "[runtime.pool]\n\
         min_connections = 3\n\
         max_connections = 9\n\
         acquire_timeout_secs = 15\n",
    )
    .unwrap();

    assert_eq!(cfg.runtime.pool.min_connections, 3);
    assert_eq!(cfg.runtime.pool.max_connections, 9);
    assert_eq!(cfg.runtime.pool.acquire_timeout_secs, 15);
}

#[test]
fn runtime_typo_is_rejected() {
    // A genuine typo under `[observers.runtime]` must fail loud (deny_unknown_fields),
    // not be silently dropped — the #342 fail-loud contract.
    let err = toml::from_str::<ObserverConfig>("[runtime]\npol_interval_ms = 5\n");
    assert!(err.is_err(), "typo `pol_interval_ms` under [runtime] should be rejected");
}

#[test]
fn runtime_defaults_when_absent() {
    let cfg: ObserverConfig = toml::from_str("enabled = true\n").unwrap();
    let defaults = ObserverRuntimeSettings::default();

    assert_eq!(cfg.runtime.poll_interval_ms, defaults.poll_interval_ms);
    assert_eq!(cfg.runtime.batch_size, defaults.batch_size);
    assert_eq!(cfg.runtime.channel_capacity, defaults.channel_capacity);
    assert!(cfg.misplaced_runtime_keys().is_empty());
}

// ── `[observers]` tolerates the compiler-owned keys ─────────────────────────

#[test]
fn compiler_schema_keys_are_tolerated() {
    // Keys owned by the CLI's `ObserversConfig` (backend/redis_url/nats_url/handlers)
    // must NOT make the server refuse the shared `fraiseql.toml`.
    let cfg: ObserverConfig = toml::from_str(
        "enabled = true\n\
         backend = \"postgresql\"\n\
         redis_url = \"redis://localhost\"\n\
         nats_url = \"nats://localhost:4222\"\n\
         handlers = []\n",
    )
    .unwrap();

    assert!(cfg.enabled);
    assert!(cfg.misplaced_runtime_keys().is_empty());
}

// ── Migration traps: pre-#342 flat layout fails loud ────────────────────────

#[test]
fn flat_scalar_server_keys_are_trapped() {
    let cfg: ObserverConfig = toml::from_str(
        "enabled = true\n\
         poll_interval_ms = 500\n\
         batch_size = 100\n\
         channel_capacity = 1000\n\
         auto_reload = true\n\
         reload_interval_secs = 60\n",
    )
    .unwrap();

    // Parsing succeeds (captured as traps); the loud failure happens in load_config.
    assert_eq!(
        cfg.misplaced_runtime_keys(),
        vec![
            "poll_interval_ms",
            "batch_size",
            "channel_capacity",
            "auto_reload",
            "reload_interval_secs",
        ],
    );
}

#[test]
fn runtime_max_dlq_size_parses() {
    let cfg: ObserverConfig = toml::from_str("[runtime]\nmax_dlq_size = 10000\n").unwrap();
    assert_eq!(cfg.runtime.max_dlq_size, Some(10000));
    assert!(cfg.misplaced_runtime_keys().is_empty());

    // Absent → unbounded (None), back-compat default.
    let default: ObserverConfig = toml::from_str("enabled = true\n").unwrap();
    assert_eq!(default.runtime.max_dlq_size, None);
}

#[test]
fn flat_max_dlq_size_is_trapped() {
    // The pre-#342 docs showed `[observers] max_dlq_size`; it now lives under runtime.
    let cfg: ObserverConfig = toml::from_str("max_dlq_size = 10000\n").unwrap();
    assert_eq!(cfg.misplaced_runtime_keys(), vec!["max_dlq_size"]);
}

#[test]
fn flat_pool_table_is_trapped() {
    let cfg: ObserverConfig = toml::from_str(
        "[pool]\n\
         max_connections = 5\n",
    )
    .unwrap();

    assert_eq!(cfg.misplaced_runtime_keys(), vec!["pool"]);
}

// ── `[observers.runtime.transport]` (#350) ──────────────────────────────────

#[test]
fn runtime_transport_subtable_parses() {
    use fraiseql_observers::config::TransportKind;

    let cfg: ObserverConfig = toml::from_str(
        "enabled = true\n\
         [runtime.transport]\n\
         transport = \"nats\"\n\
         [runtime.transport.nats]\n\
         url = \"nats://broker:4222\"\n",
    )
    .unwrap();

    assert_eq!(cfg.runtime.transport.transport, TransportKind::Nats);
    assert_eq!(cfg.runtime.transport.nats.url, "nats://broker:4222");
    assert!(cfg.misplaced_runtime_keys().is_empty());
}

#[test]
fn runtime_transport_defaults_to_postgres() {
    use fraiseql_observers::config::TransportKind;

    let cfg: ObserverConfig = toml::from_str("enabled = true\n").unwrap();
    assert_eq!(cfg.runtime.transport.transport, TransportKind::Postgres);
}

#[test]
fn runtime_transport_typo_is_rejected() {
    // A typo directly under `[runtime]` is still caught by deny_unknown_fields,
    // even though `transport` is now a recognised sub-table.
    let err = toml::from_str::<ObserverConfig>("[runtime]\ntranport = \"nats\"\n");
    assert!(err.is_err(), "typo `tranport` under [runtime] should be rejected");
}

#[test]
fn new_layout_has_no_misplaced_keys() {
    let cfg: ObserverConfig = toml::from_str(
        "enabled = true\n\
         backend = \"postgresql\"\n\
         [runtime]\n\
         poll_interval_ms = 500\n\
         [runtime.pool]\n\
         max_connections = 7\n",
    )
    .unwrap();

    assert!(cfg.misplaced_runtime_keys().is_empty());
    assert_eq!(cfg.runtime.poll_interval_ms, 500);
    assert_eq!(cfg.runtime.pool.max_connections, 7);
}
