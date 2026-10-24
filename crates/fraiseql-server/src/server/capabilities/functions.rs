//! The function runtime, and the `cron:` scheduler (#595) that fires its scheduled
//! functions.

#[cfg(feature = "functions-runtime")]
use tracing::info;

#[cfg(feature = "functions-runtime")]
use super::{RunCx, ServeCx};
use crate::capability::Capability;
#[cfg(feature = "functions-runtime")]
use crate::capability::Started;
#[cfg(feature = "functions-runtime")]
use crate::server::{
    ServerError,
    provision::{BootCx, ProvisionStep},
    start::EntryPoint,
};

/// Declared functions. Their runtime is prepared during provisioning, so the hooks every
/// route dispatches through exist before the router is built.
#[derive(Debug)]
pub struct Functions;

#[async_trait::async_trait]
impl Capability for Functions {
    #[cfg(feature = "functions-runtime")]
    fn configured(&self, cx: &ServeCx<'_>) -> bool {
        cx.app.before_mutation_hooks.is_some()
    }

    /// `_fraiseql_cron_state`, the cross-restart fire guard, when a function is scheduled.
    #[cfg(feature = "functions-runtime")]
    fn provision(&self, cx: &BootCx<'_>) -> Vec<ProvisionStep> {
        let server = cx.server;
        let scheduled = server.functions_config.as_ref().is_some_and(|functions| {
            functions.definitions.iter().any(|definition| {
                fraiseql_functions::triggers::ParsedTrigger::parse(&definition.trigger)
                    .is_ok_and(|trigger| trigger.is_cron())
            })
        });
        match server.db_pool.as_ref() {
            Some(pool) if scheduled => vec![ProvisionStep::new(
                "cron state",
                pool,
                &[fraiseql_functions::migrations::cron_migration_sql()],
            )],
            _ => Vec::new(),
        }
    }

    /// Start the `cron:` scheduler: one leased poller per cron function in the compiled
    /// schema, each firing on its schedule under a single-firing advisory lease with the
    /// phase-02 `run_as` host. A cron function is a scheduled source without a cursor.
    ///
    /// The scheduler is deployment-wide work, so the stdio sidecar, which serves the
    /// functions to its own requests, leaves it to the HTTP server it runs beside.
    #[cfg(feature = "functions-runtime")]
    async fn start(&self, cx: &mut RunCx<'_>) -> crate::Result<Started> {
        if cx.entry == EntryPoint::McpStdio {
            return Ok(Started::Running);
        }
        let app_state = cx.app;
        let server = &mut *cx.server;
        let (Some(db_pool), Some(hooks)) =
            (server.db_pool.as_ref(), app_state.before_mutation_hooks.as_ref())
        else {
            return Ok(Started::Running);
        };
        if hooks.trigger_registry.cron_triggers.is_empty() {
            return Ok(Started::Running);
        }
        let host_config = crate::routes::after_mutation::host_context_config(&hooks.egress);
        let pollers = crate::cron::build_cron_pollers(
            db_pool,
            &crate::query_bridge::ExecutorSource::of(app_state),
            hooks.as_ref(),
            &host_config,
        )
        .await
        .map_err(|e| {
            // Fail loud: a cross-restart fire guard whose state cannot be read back is
            // not a guard (#796).
            ServerError::ConfigError(format!(
                "Failed to read back cron state (_fraiseql_cron_state): {e}"
            ))
        })?;
        let started = pollers.len();
        for poller in pollers {
            server.tasks.spawn(async move { poller.run_forever().await });
        }
        info!(cron_functions = started, "cron scheduler started");
        Ok(Started::Running)
    }
}
