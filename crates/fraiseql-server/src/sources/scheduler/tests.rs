//! Tests for the source-scheduler assembly: the pure `schedulable` filter and the
//! env-overridable config resolution. The full poller-wiring (`build_source_pollers`)
//! is exercised by the lifecycle integration and the poller's `build_host` composition.
#![allow(clippy::unwrap_used)] // Reason: test module

use std::collections::HashMap;

use fraiseql_core::schema::SourceDefinition;
use fraiseql_functions::{FunctionModule, RuntimeType};

use super::{schedulable, source_host_config_from, sources_enabled_from};
use crate::server_config::SourcesConfig;

fn module(name: &str) -> FunctionModule {
    FunctionModule::from_source(name.to_string(), String::new(), RuntimeType::Deno)
}

/// A registry with one loaded Model B module, `pollOrders`.
fn registry() -> HashMap<String, FunctionModule> {
    HashMap::from([("pollOrders".to_string(), module("pollOrders"))])
}

#[test]
fn schedulable_keeps_only_enabled_backed_valid_sources() {
    let sources = vec![
        // Kept: enabled, module loaded, valid cron.
        SourceDefinition::new("orders", "*/5 * * * *", "pollOrders"),
        // Skipped: disabled.
        SourceDefinition::new("disabled", "*/5 * * * *", "pollOrders").disabled(),
        // Skipped: no loaded module (e.g. a native source).
        SourceDefinition::new("native", "*/5 * * * *", "nativeThing"),
        // Skipped: invalid cron.
        SourceDefinition::new("bad-cron", "not-a-cron", "pollOrders"),
    ];
    let kept = schedulable(&sources, &registry());
    let names: Vec<&str> = kept.iter().map(|(source, _, _)| source.name.as_str()).collect();
    assert_eq!(names, ["orders"], "only the enabled, backed, valid-cron source is scheduled");
    // The parsed schedule rides along.
    assert_eq!(kept[0].2.expression, "*/5 * * * *");
}

#[test]
fn schedulable_is_empty_when_nothing_qualifies() {
    let sources = vec![SourceDefinition::new("native", "*/5 * * * *", "unloaded")];
    assert!(schedulable(&sources, &registry()).is_empty());
}

#[test]
fn enabled_resolves_env_over_config() {
    let on = SourcesConfig {
        enabled: true,
        ..SourcesConfig::default()
    };
    let off = SourcesConfig {
        enabled: false,
        ..SourcesConfig::default()
    };

    // No env → the config value.
    assert!(sources_enabled_from(&on, |_| None));
    assert!(!sources_enabled_from(&off, |_| None));

    // Env overrides the config, either way.
    assert!(!sources_enabled_from(&on, |_| Some("false".to_string())));
    assert!(!sources_enabled_from(&on, |_| Some("OFF".to_string())));
    assert!(sources_enabled_from(&off, |_| Some("true".to_string())));
    assert!(sources_enabled_from(&off, |_| Some("1".to_string())));
}

#[test]
fn host_config_allowlist_resolves_env_over_config() {
    let config = SourcesConfig {
        enabled: true,
        allowed_domains: vec!["from-toml.example".to_string()],
        ..SourcesConfig::default()
    };

    // No env → the config allowlist.
    let host = source_host_config_from(&config, |_| None);
    assert_eq!(host.allowed_domains, vec!["from-toml.example".to_string()]);

    // Env overrides, comma-split and trimmed (key-aware: only the domains key).
    let host = source_host_config_from(&config, |key| {
        (key == "FRAISEQL_SOURCES_ALLOWED_DOMAINS").then(|| " a.example, b.example ".to_string())
    });
    assert_eq!(host.allowed_domains, vec!["a.example".to_string(), "b.example".to_string()]);

    // Deny-by-default when neither is set.
    let empty = SourcesConfig::default();
    assert!(source_host_config_from(&empty, |_| None).allowed_domains.is_empty());
}

/// #840: the env-var allowlist has a producer — `[sources] allowed_env_vars`
/// with a `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` override — where before nothing
/// in any shipped code path populated it (deny-by-default degenerated to
/// deny-always while the docs advertised granting secrets).
#[test]
fn host_config_env_var_allowlist_resolves_env_over_config() {
    let config = SourcesConfig {
        enabled: true,
        allowed_env_vars: vec!["QONTO_API_KEY".to_string()],
        ..SourcesConfig::default()
    };

    // No env → the config allowlist.
    let host = source_host_config_from(&config, |_| None);
    assert!(host.allowed_env_vars.contains("QONTO_API_KEY"));

    // Env overrides, comma-split and trimmed.
    let host = source_host_config_from(&config, |key| {
        (key == "FRAISEQL_SOURCES_ALLOWED_ENV_VARS")
            .then(|| " LLM_API_KEY, MAIL_API_KEY ".to_string())
    });
    assert!(host.allowed_env_vars.contains("LLM_API_KEY"));
    assert!(host.allowed_env_vars.contains("MAIL_API_KEY"));
    assert!(!host.allowed_env_vars.contains("QONTO_API_KEY"), "env replaces, not merges");

    // Deny-by-default when neither is set.
    assert!(
        source_host_config_from(&SourcesConfig::default(), |_| None)
            .allowed_env_vars
            .is_empty()
    );
}

// ===========================================================================
// #868 item 4 — the declared `cursor` override must reach the watermark store
// ===========================================================================

/// A source declaring `cursor = "…"` must advance **that** key, not its own name.
///
/// `SourceDefinition::cursor_name()` existed, the schema validator enforced uniqueness on it
/// with the rationale "a shared cursor name would let two sources clobber each other's
/// watermark", the converter compiled it, and `fraiseql sources` printed it — while
/// `build_source_pollers` passed `source.name` to the cursor store. The override was accepted,
/// validated, compiled, displayed, and inert.
///
/// The operational cost: renaming a source from `orders` to `orders_v2` with
/// `cursor = "orders"` to preserve the watermark advanced a brand-new row under `orders_v2`,
/// so the first tick re-ingested the entire history.
#[test]
fn a_declared_cursor_override_is_the_key_the_poller_advances() {
    let declared = SourceDefinition {
        name:     "orders_v2".to_string(),
        schedule: "*/5 * * * *".to_string(),
        cursor:   Some("orders".to_string()),
        function: "connector".to_string(),
        enabled:  true,
        options:  serde_json::Value::Null,
        run_as:   None,
    };
    assert_eq!(
        declared.cursor_name(),
        "orders",
        "cursor_name() must prefer the declared override"
    );

    let no_override = SourceDefinition {
        cursor: None,
        ..declared
    };
    assert_eq!(
        no_override.cursor_name(),
        "orders_v2",
        "with no override the cursor key falls back to the source name"
    );
}
