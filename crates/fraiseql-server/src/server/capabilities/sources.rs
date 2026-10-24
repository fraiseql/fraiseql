//! The scheduled-ingress source scheduler (#573).

#[cfg(feature = "sources")]
use tracing::info;

#[cfg(feature = "sources")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "sources")]
use crate::{
    capability::Started,
    server::{
        ServerError,
        provision::{BootCx, ProvisionStep},
    },
};

/// Enabled sources in the compiled schema, on a server with a database pool.
#[derive(Debug)]
pub struct Sources;

#[async_trait::async_trait]
impl Capability for Sources {
    #[cfg(feature = "sources")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.server.db_pool.is_some()
            && cx.app.executor().schema().sources.iter().any(|source| source.enabled)
    }

    /// The durable source-cursor table every poller resumes from.
    #[cfg(feature = "sources")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        let any_enabled = server.executor.schema().sources.iter().any(|source| source.enabled);
        match server.db_pool.as_ref() {
            Some(pool) if any_enabled => vec![ProvisionStep::new(
                "source cursor",
                pool,
                &[fraiseql_observers::migrations::source_cursor_sql()],
            )],
            _ => Vec::new(),
        }
    }

    /// One poller per enabled Model B (Deno) source, each firing its connector on its cron
    /// schedule under a single-firing lease, with a host bound to the source's durable
    /// cursor and its `run_as` executor. Native pull sources (poll-IMAP email) run through
    /// their own pollers.
    #[cfg(feature = "sources")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        let app_state = cx.app;
        let server = &mut *cx.server;
        let Some(db_pool) = server.db_pool.as_ref() else {
            return Ok(Started::Running);
        };
        let sources = app_state.executor().schema().sources.clone();
        let sources_config = server.config.sources.clone().unwrap_or_default();
        if !crate::sources::sources_enabled(&sources_config) {
            info!(
                count = sources.len(),
                "source scheduler disabled by config — sources not started"
            );
            return Ok(Started::Degraded);
        }
        let Some(hooks) = app_state.before_mutation_hooks.as_ref() else {
            // Provisioning refuses this case first (#1399); a backstop, so an enabled
            // source can never be left silently unscheduled.
            let Some(source) = sources.iter().find(|source| source.enabled) else {
                return Ok(Started::Running);
            };
            return Err(ServerError::ConfigError(format!(
                "source {:?} is enabled but no functions subsystem was built to run its \
                 connector {:?}",
                source.name, source.function
            )));
        };
        let host_config = crate::sources::source_host_config(&sources_config, &hooks.egress);
        let pollers = crate::sources::build_source_pollers(
            &sources,
            db_pool,
            &crate::query_bridge::ExecutorSource::of(app_state),
            hooks.as_ref(),
            &host_config,
            sources_config.log_payloads,
        )?;
        let started = pollers.len();
        for poller in pollers {
            server.tasks.spawn(async move { poller.run_forever().await });
        }
        info!(sources = started, "source scheduler started");
        Ok(Started::Running)
    }
}
