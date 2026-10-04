//! Assembling the source scheduler from the compiled schema (#573).
//!
//! [`build_source_pollers`] turns the compiled `sources` array into one
//! [`SourcePoller`] per enabled source, resolving each source's connector
//! module, `run_as` identity ([`SourceQueryExecutor`]), durable cursor, and
//! single-firing lease. The lifecycle spawns the returned pollers on the server's
//! `JoinSet`. [`sources_enabled`] and [`source_host_config`] resolve the `[sources]`
//! runtime config with environment overrides (env > TOML > default).

use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{runtime::Executor, schema::SourceDefinition};
use fraiseql_functions::{
    FunctionModule, ResourceLimits,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::CronSchedule,
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore};

use super::{SourcePoller, SourceQueryExecutor};
use crate::{ServerError, server_config::SourcesConfig, subsystems::BeforeMutationHooks};

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

/// The enabled sources, each with its connector module and parsed schedule. A disabled
/// source is skipped (compiled but intentionally not scheduled).
///
/// An enabled source that cannot run is an error, never a skip (#1399): it used to log
/// a warning and leave the source silently unscheduled. Its connector was loaded at
/// provisioning, which refuses the boot when it cannot be, so a missing module here
/// means the two steps disagree; an unparseable schedule passed a compiler whose
/// check was weaker than this parser.
fn schedulable<'a>(
    sources: &'a [SourceDefinition],
    modules: &HashMap<String, FunctionModule>,
) -> Result<Vec<(&'a SourceDefinition, FunctionModule, CronSchedule)>, ServerError> {
    sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| {
            let module = modules.get(&source.function).ok_or_else(|| {
                ServerError::ConfigError(format!(
                    "source {:?} runs the connector {:?}, which was not loaded — every \
                     enabled source's connector is loaded at boot from `[functions] \
                     module_dir`",
                    source.name, source.function
                ))
            })?;
            let schedule = CronSchedule::parse(&source.schedule).map_err(|error| {
                ServerError::ConfigError(format!(
                    "source {:?} has an invalid cron schedule {:?}: {error}",
                    source.name, source.schedule
                ))
            })?;
            Ok((source, module.clone(), schedule))
        })
        .collect()
}

/// Build one [`SourcePoller`] per enabled Model B source in the compiled schema.
///
/// Each poller runs under the source's `run_as` identity (via
/// [`SourceQueryExecutor`] over the hot-reloadable `executor`), reads/advances the
/// shared durable cursor store, and single-fires across replicas on a
/// `PostgreSQL` advisory lease keyed on the source name. The caller spawns each
/// returned poller's [`run_forever`](SourcePoller::run_forever) on the server's
/// `JoinSet`; the shared `_fraiseql_source_cursor` table must already be
/// initialized.
///
/// # Errors
///
/// Returns [`ServerError::ConfigError`] if an enabled source's connector is not in
/// the registry or its schedule does not parse.
// Reason: a wiring seam whose args are each a distinct runtime collaborator; a
// params struct would relocate the same fields without reducing coupling.
#[allow(clippy::too_many_arguments)]
pub fn build_source_pollers(
    sources: &[SourceDefinition],
    db_pool: &sqlx::PgPool,
    executor: &Arc<ArcSwap<Executor>>,
    hooks: &BeforeMutationHooks,
    host_config: &HostContextConfig,
    limits: &ResourceLimits,
    log_payloads: bool,
) -> Result<Vec<SourcePoller>, ServerError> {
    Ok(schedulable(sources, &hooks.module_registry)?
        .into_iter()
        .map(|(source, module, schedule)| {
            // The source's mutations run under its run_as ceiling; the
            // request-id correlates the source in the audit envelope.
            let identity = source.identity(source.name.as_str());
            let query_executor: Arc<dyn QueryExecutor> =
                Arc::new(SourceQueryExecutor::new(Arc::clone(executor), identity.clone()));
            SourcePoller::new(
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
                limits.clone(),
                hooks.idempotency_key.clone(),
                log_payloads,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests;
