//! Assembling the source scheduler from the compiled schema (#573 Phase 06 Step 4).
//!
//! [`build_source_pollers`] turns the compiled `sources` array into one
//! [`SourcePoller`] per enabled Model B source, resolving each source's function
//! module, `run_as` identity ([`SourceQueryExecutor`]), durable cursor, and
//! single-firing lease. The lifecycle spawns the returned pollers on the server's
//! `JoinSet`. [`sources_enabled`] and [`source_host_config`] resolve the `[sources]`
//! runtime config with environment overrides (env > TOML > default).

use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use fraiseql_core::{db::traits::DatabaseAdapter, runtime::Executor, schema::SourceDefinition};
use fraiseql_functions::{
    FunctionModule, ResourceLimits,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::CronSchedule,
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore};
use tracing::warn;

use super::{SourcePoller, SourceQueryExecutor};
use crate::{server_config::SourcesConfig, subsystems::BeforeMutationHooks};

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

/// The host config for source connectors: the SSRF allowlist from
/// `FRAISEQL_SOURCES_ALLOWED_DOMAINS` (comma-separated) overriding `[sources]
/// allowed_domains`, deny-by-default.
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
    HostContextConfig {
        allowed_domains,
        ..HostContextConfig::default()
    }
}

/// The enabled sources that are schedulable Model B connectors: each is `enabled`,
/// resolves to a loaded function module, and has a valid cron schedule. A disabled
/// source is skipped silently (compiled but intentionally not scheduled); a source
/// whose function has no loaded module (a native source, or a module that never
/// loaded) or an unparseable schedule is logged and skipped.
fn schedulable<'a>(
    sources: &'a [SourceDefinition],
    modules: &HashMap<String, FunctionModule>,
) -> Vec<(&'a SourceDefinition, FunctionModule, CronSchedule)> {
    sources
        .iter()
        .filter_map(|source| {
            if !source.enabled {
                return None;
            }
            let Some(module) = modules.get(&source.function) else {
                warn!(
                    source = %source.name,
                    function = %source.function,
                    "source function has no loaded module — skipping (native source, or the \
                     module did not load)"
                );
                return None;
            };
            match CronSchedule::parse(&source.schedule) {
                Ok(schedule) => Some((source, module.clone(), schedule)),
                Err(error) => {
                    warn!(
                        source = %source.name,
                        schedule = %source.schedule,
                        %error,
                        "invalid cron schedule — skipping source"
                    );
                    None
                },
            }
        })
        .collect()
}

/// Build one [`SourcePoller`] per enabled Model B source in the compiled schema.
///
/// Each poller runs under the source's `run_as` identity (via
/// [`SourceQueryExecutor`] over the hot-reloadable `executor`), reads/advances the
/// shared durable cursor store, and single-fires across replicas on a
/// PostgreSQL advisory lease keyed on the source name. The caller spawns each
/// returned poller's [`run_forever`](SourcePoller::run_forever) on the server's
/// `JoinSet`; the shared `_fraiseql_source_cursor` table must already be
/// initialized.
pub fn build_source_pollers<A: DatabaseAdapter + Send + Sync + 'static>(
    sources: &[SourceDefinition],
    db_pool: &sqlx::PgPool,
    executor: &Arc<ArcSwap<Executor<A>>>,
    hooks: &BeforeMutationHooks,
    host_config: &HostContextConfig,
    limits: &ResourceLimits,
) -> Vec<SourcePoller> {
    schedulable(sources, &hooks.module_registry)
        .into_iter()
        .map(|(source, module, schedule)| {
            // The source's mutations run under its run_as ceiling (D6); the
            // request-id correlates the source in the audit envelope.
            let identity = source.identity(source.name.as_str());
            let query_executor: Arc<dyn QueryExecutor> =
                Arc::new(SourceQueryExecutor::new(Arc::clone(executor), identity));
            SourcePoller::new(
                source.name.clone(),
                schedule,
                module,
                Arc::clone(&hooks.observer),
                PostgresSourceCursorStore::new(db_pool.clone()),
                query_executor,
                LeaseGuardedRunner::postgres(db_pool.clone(), source.name.clone()),
                host_config.clone(),
                limits.clone(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests;
