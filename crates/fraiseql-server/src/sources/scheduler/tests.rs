//! Tests for the source-scheduler assembly: the pure `schedulable` resolution and the
//! env-overridable config resolution. The full poller-wiring (`build_source_pollers`)
//! is exercised by the lifecycle integration and the poller's `build_host` composition.
#![allow(clippy::unwrap_used)] // Reason: test module

use std::{collections::HashMap, sync::Arc};

use fraiseql_core::schema::SourceDefinition;
use fraiseql_functions::{
    FunctionModule, FunctionObserver, ResourceLimits, RuntimeType, TriggerRegistry,
};

use super::{schedulable, source_host_config_from, sources_enabled_from};
use crate::{server_config::SourcesConfig, subsystems::BeforeMutationHooks};

fn module(name: &str) -> FunctionModule {
    FunctionModule::from_source(name.to_string(), String::new(), RuntimeType::Deno)
}

/// Hooks with one loaded Model B module, `pollOrders`.
fn hooks() -> BeforeMutationHooks {
    BeforeMutationHooks::new(
        TriggerRegistry::new(),
        HashMap::from([("pollOrders".to_string(), module("pollOrders"))]),
        Arc::new(FunctionObserver::new()),
    )
}

#[test]
fn schedulable_keeps_every_enabled_source_and_skips_a_disabled_one() {
    let sources = vec![
        SourceDefinition::new("orders", "*/5 * * * *", "pollOrders"),
        // Disabled: compiled but intentionally not scheduled, so its connector is
        // never consulted.
        SourceDefinition::new("disabled", "*/5 * * * *", "unloaded").disabled(),
    ];
    let kept = schedulable(&sources, &hooks()).unwrap();
    let names: Vec<&str> = kept.iter().map(|kept| kept.source.name.as_str()).collect();
    assert_eq!(names, ["orders"]);
    // The parsed schedule rides along.
    assert_eq!(kept[0].schedule.expr.expression, "*/5 * * * *");
}

/// #1399: an enabled source whose connector is not loaded refuses, naming both. It
/// used to log a warning and never run.
#[test]
fn an_enabled_source_with_no_loaded_connector_is_an_error() {
    let sources = vec![SourceDefinition::new(
        "invoices",
        "*/5 * * * *",
        "pollInvoices",
    )];
    let message = schedulable(&sources, &hooks()).err().unwrap().to_string();
    assert!(
        message.contains("\"invoices\"") && message.contains("\"pollInvoices\""),
        "{message}"
    );
}

/// An enabled source whose schedule does not parse refuses rather than being skipped.
#[test]
fn an_enabled_source_with_an_invalid_schedule_is_an_error() {
    let sources = vec![SourceDefinition::new("orders", "61 * * * *", "pollOrders")];
    let message = schedulable(&sources, &hooks()).err().unwrap().to_string();
    assert!(message.contains("\"orders\"") && message.contains("61 * * * *"), "{message}");
}

/// A compiled schema is input the server did not produce, so the boot refuses what the
/// compiler refuses: an unknown timezone, and a `:` in a source's name.
#[test]
fn an_enabled_source_with_an_unknown_timezone_is_an_error() {
    let mut source = SourceDefinition::new("orders", "*/5 * * * *", "pollOrders");
    source.timezone = Some("Europe/Pariss".to_string());
    let message = schedulable(&[source], &hooks()).err().unwrap().to_string();
    assert!(message.contains("\"orders\"") && message.contains("Europe/Pariss"), "{message}");
}

#[test]
fn a_source_named_with_a_colon_is_an_error() {
    let sources = vec![SourceDefinition::new(
        "cron:nightly",
        "*/5 * * * *",
        "pollOrders",
    )];
    let message = schedulable(&sources, &hooks()).err().unwrap().to_string();
    assert!(message.contains("\"cron:nightly\"") && message.contains("':'"), "{message}");
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

/// A connector calls out under the `[egress]` list functions share, and nothing else:
/// the variable that used to set a sources-only list is refused when `[egress]` is
/// resolved, so it cannot widen this one.
#[test]
fn a_connector_calls_out_under_the_list_functions_share() {
    let egress = vec!["api.example.com".to_string()];
    let connector = source_host_config_from(&SourcesConfig::default(), &egress, |key| {
        (key == "FRAISEQL_SOURCES_ALLOWED_DOMAINS").then(|| "other.example".to_string())
    });
    let function = crate::routes::after_mutation::host_context_config(&egress);
    assert_eq!(connector.allowed_domains, egress);
    assert_eq!(connector.allowed_domains, function.allowed_domains);
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
    let host = source_host_config_from(&config, &[], |_| None);
    assert!(host.allowed_env_vars.contains("QONTO_API_KEY"));

    // Env overrides, comma-split and trimmed.
    let host = source_host_config_from(&config, &[], |key| {
        (key == "FRAISEQL_SOURCES_ALLOWED_ENV_VARS")
            .then(|| " LLM_API_KEY, MAIL_API_KEY ".to_string())
    });
    assert!(host.allowed_env_vars.contains("LLM_API_KEY"));
    assert!(host.allowed_env_vars.contains("MAIL_API_KEY"));
    assert!(!host.allowed_env_vars.contains("QONTO_API_KEY"), "env replaces, not merges");

    // Deny-by-default when neither is set.
    assert!(
        source_host_config_from(&SourcesConfig::default(), &[], |_| None)
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
        timezone: None,
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

/// #1627: a connector runs under its function's declared limits, not the 5 s default
/// every poller used to get.
#[test]
fn a_source_connector_runs_under_its_declared_timeout() {
    let mut hooks = hooks();
    hooks.function_limits.insert(
        "pollOrders".to_string(),
        ResourceLimits {
            max_duration: std::time::Duration::from_secs(30),
            ..ResourceLimits::default()
        },
    );
    let sources = vec![SourceDefinition::new("orders", "*/5 * * * *", "pollOrders")];
    let kept = schedulable(&sources, &hooks).unwrap();
    assert_eq!(kept[0].limits.max_duration, std::time::Duration::from_secs(30));
}
