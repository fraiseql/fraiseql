//! Assembling the source scheduler from the compiled schema (#573).
//!
//! [`build_source_pollers`] turns the compiled `sources` array into one
//! `ScheduledPoller` per enabled source, resolving each source's connector
//! module, `run_as` identity ([`SourceQueryExecutor`]), durable cursor, and
//! single-firing lease. The lifecycle spawns the returned pollers on the server's
//! `JoinSet`. [`sources_enabled`] and [`source_host_config`] resolve the `[sources]`
//! runtime config with environment overrides (env > TOML > default).

use std::sync::Arc;

use fraiseql_core::schema::SourceDefinition;
use fraiseql_functions::{
    FunctionModule, ResourceLimits,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::Schedule,
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore};

use super::SourceQueryExecutor;
use crate::{
    ServerError, cron::ScheduledPoller, server_config::SourcesConfig,
    subsystems::BeforeMutationHooks,
};

/// Whether the source scheduler runs: `FRAISEQL_SOURCES_ENABLED` overrides the
/// `[sources] enabled` config (env > TOML > default `true`). Any of
/// `false`/`0`/`no`/`off` (case-insensitive) disables it.
#[must_use]
pub fn sources_enabled(config: &SourcesConfig) -> bool {
    sources_enabled_from(config, |key| std::env::var(key).ok())
}

fn sources_enabled_from(config: &SourcesConfig, get: impl Fn(&str) -> Option<String>) -> bool {
    match get("FRAISEQL_SOURCES_ENABLED") {
        Some(value) => {
            !matches!(value.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no" | "off")
        },
        None => config.enabled,
    }
}

/// The host config for source connectors, deny-by-default.
///
/// The SSRF allowlist comes from `FRAISEQL_SOURCES_ALLOWED_DOMAINS` and the
/// env-var allowlist from `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` (both
/// comma-separated), each overriding its `[sources]` config key.
#[must_use]
pub fn source_host_config(config: &SourcesConfig) -> HostContextConfig {
    source_host_config_from(config, |key| std::env::var(key).ok())
}

fn source_host_config_from(
    config: &SourcesConfig,
    get: impl Fn(&str) -> Option<String>,
) -> HostContextConfig {
    let allowed_domains = match get("FRAISEQL_SOURCES_ALLOWED_DOMAINS") {
        Some(value) => value
            .split(',')
            .map(str::trim)
            .filter(|domain| !domain.is_empty())
            .map(String::from)
            .collect(),
        None => config.allowed_domains.clone(),
    };
    // #840: the env-var allowlist producer — without one, `allowed_env_vars`
    // was empty in every shipped process and the documented `fraiseql_env_var`
    // capability could never return a value.
    let allowed_env_vars = match get("FRAISEQL_SOURCES_ALLOWED_ENV_VARS") {
        Some(value) => value
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(String::from)
            .collect(),
        None => config.allowed_env_vars.iter().cloned().collect(),
    };
    HostContextConfig {
        allowed_domains,
        allowed_env_vars,
        ..HostContextConfig::default()
    }
}

/// An enabled source the scheduler runs: its connector module, parsed schedule, and the
/// limits the connector runs under (its function's declared `timeout_ms` when the
/// connector is also a declared function, #1627).
struct Schedulable<'a> {
    source:   &'a SourceDefinition,
    module:   FunctionModule,
    schedule: Schedule,
    limits:   ResourceLimits,
}

/// The enabled sources, each with its connector module, parsed schedule and limits. A
/// disabled source is skipped (compiled but intentionally not scheduled).
///
/// An enabled source that cannot run is an error, never a skip (#1399): it used to log
/// a warning and leave the source silently unscheduled. Its connector was loaded at
/// provisioning, which refuses the boot when it cannot be, so a missing module here
/// means the two steps disagree; an unparseable schedule passed a compiler whose
/// check was weaker than this parser.
fn schedulable<'a>(
    sources: &'a [SourceDefinition],
    hooks: &BeforeMutationHooks,
) -> Result<Vec<Schedulable<'a>>, ServerError> {
    sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| {
            // A cron function's lease is `cron:<name>`, so a source named `cron:x` would
            // share the lease of the cron function `x`. The compiler refuses the name too.
            if source.name.contains(':') {
                return Err(ServerError::ConfigError(format!(
                    "source {:?} has a ':' in its name, which the scheduler reserves for \
                     cron functions' leases; rename the source",
                    source.name
                )));
            }
            let module = hooks.module_registry.get(&source.function).ok_or_else(|| {
                ServerError::ConfigError(format!(
                    "source {:?} runs the connector {:?}, which was not loaded — every \
                     enabled source's connector is loaded at boot from `[functions] \
                     module_dir`",
                    source.name, source.function
                ))
            })?;
            let schedule =
                Schedule::parse(&source.schedule, source.timezone.as_deref()).map_err(|error| {
                    ServerError::ConfigError(format!(
                        "source {:?} has an invalid schedule {:?}: {error}",
                        source.name, source.schedule
                    ))
                })?;
            Ok(Schedulable {
                source,
                module: module.clone(),
                schedule,
                limits: hooks.limits_for(&source.function),
            })
        })
        .collect()
}

/// Build one [`ScheduledPoller`] per enabled Model B source in the compiled schema.
///
/// Each poller runs under the source's `run_as` identity (via
/// [`SourceQueryExecutor`] over the server's executor source: the serving executor, or a
/// message's tenant's under the tenancy runtime, #1550), reads/advances the
/// shared durable cursor store, and single-fires across replicas on a
/// `PostgreSQL` advisory lease keyed on the source name. The caller spawns each
/// returned poller's [`run_forever`](ScheduledPoller::run_forever) on the server's
/// `JoinSet`; the shared `_fraiseql_source_cursor` table must already be
/// initialized.
///
/// # Errors
///
/// Returns [`ServerError::ConfigError`] if an enabled source's connector is not in
/// the registry, or its schedule or timezone does not parse.
pub fn build_source_pollers(
    sources: &[SourceDefinition],
    db_pool: &sqlx::PgPool,
    executor: &crate::query_bridge::ExecutorSource,
    hooks: &BeforeMutationHooks,
    host_config: &HostContextConfig,
    log_payloads: bool,
) -> Result<Vec<ScheduledPoller>, ServerError> {
    Ok(schedulable(sources, hooks)?
        .into_iter()
        .map(
            |Schedulable {
                 source,
                 module,
                 schedule,
                 limits,
             }| {
                // The source's mutations run under its run_as ceiling; the
                // request-id correlates the source in the audit envelope.
                let identity = source.identity(source.name.as_str());
                let query_executor: Arc<dyn QueryExecutor> =
                    Arc::new(SourceQueryExecutor::for_source(executor.clone(), identity.clone()));
                ScheduledPoller::source(
                    source.name.clone(),
                    // The declared `cursor` override, falling back to the source name. Keeps the
                    // watermark key under the author's control while the lease and metric labels
                    // below stay keyed on the source name (#868 item 4).
                    source.cursor_name().to_string(),
                    schedule,
                    module,
                    Arc::clone(&hooks.observer),
                    PostgresSourceCursorStore::new(db_pool.clone()),
                    query_executor,
                    identity,
                    LeaseGuardedRunner::postgres(db_pool.clone(), source.name.clone()),
                    host_config.clone(),
                    limits,
                    hooks.idempotency_key.clone(),
                    log_payloads,
                )
            },
        )
        .collect())
}

#[cfg(test)]
mod tests;
