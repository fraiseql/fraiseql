//! Starting the background capabilities: one function every serve entry point calls.
//!
//! `serve_with_shutdown` used to start them inline, so `serve_on_listener` — the entry
//! point every in-process suite serves through — mounted their routes and started none
//! of them, and `serve_mcp_stdio` skipped even the request-serving workers. Which entry
//! point starts which capability is now each registry entry's `entry_points`, and this
//! loop is the one place that reads it.

use fraiseql_core::capability::EntryPoints;
use tracing::{error, info, warn};

use super::{Result, Server};
use crate::{
    capability::{REGISTRY, Started},
    routes::graphql::AppState,
    server::capabilities::{RunCx, ServeCx},
};

/// The ways a server is served. What each starts is declared per capability, in
/// [`CapabilityMeta::entry_points`](fraiseql_core::capability::CapabilityMeta::entry_points).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryPoint {
    /// [`Server::serve_with_shutdown`]: the binary and `fraiseql run`.
    Http,
    /// [`Server::serve_on_listener`]: a caller-bound listener, the in-process harness.
    Listener,
    /// `Server::serve_mcp_stdio`: one MCP client over stdin/stdout, a sidecar to the
    /// deployment's HTTP server. Besides what the registry declares for it, it runs the
    /// core workers its own requests need (async operations, the inbound dispatch sweep,
    /// the storage quota sweep) and no schema reload handler.
    McpStdio,
}

impl EntryPoint {
    /// Whether `declared` lists this entry point. `Http` is the binary's: `fraiseql run`
    /// serves through the same call, and its declaration is held equal to the binary's.
    pub const fn is_declared_in(self, declared: EntryPoints) -> bool {
        match self {
            Self::Http => declared.binary,
            Self::Listener => declared.listener,
            Self::McpStdio => declared.mcp_stdio,
        }
    }
}

impl Server {
    /// Start every capability this configuration enables that `entry` is declared for,
    /// in registry order, and record each configured one as started or not.
    ///
    /// Called once the router is built, because the pollers and schedulers dispatch
    /// through the function hooks `build_router` supplies.
    ///
    /// # Errors
    ///
    /// Returns [`crate::ServerError`] if a configured capability cannot start: its state
    /// tables cannot be created or read back, or its broker refuses in production.
    /// Callers box the future: it holds every capability's start state, which would
    /// push each serve future past clippy's 16-KiB `large_futures` threshold.
    pub(super) async fn start_capabilities(
        &mut self,
        app_state: &AppState,
        entry: EntryPoint,
    ) -> Result<()> {
        // The async-operation workers (#391), the inbound dispatch sweep and the storage
        // quota sweep, on the server's JoinSet so graceful shutdown aborts them (an
        // aborted execution's row goes stale and is reclaimed after the staleness
        // threshold — the P19 recovery path, exercised on every deploy).
        self.spawn_lifecycle_tasks(app_state);
        for registration in REGISTRY.iter().filter(|registration| registration.compiled_in) {
            let capability = registration.capability;
            let name = registration.meta.name;
            if !capability.configured(&ServeCx {
                server: self,
                app:    app_state,
            }) {
                continue;
            }
            if !entry.is_declared_in(registration.meta.entry_points) {
                info!(capability = name, ?entry, "configured, not started on this entry point");
                self.capability_starts.record(name, false);
                continue;
            }
            let started = capability
                .start(&mut RunCx {
                    server: self,
                    app: app_state,
                    entry,
                })
                .await?;
            self.capability_starts.record(name, started == Started::Running);
        }

        // Schema reload is deployment-wide too: the stdio sidecar leaves it to the HTTP
        // server beside it.
        if entry == EntryPoint::McpStdio {
            return Ok(());
        }

        // Spawn SIGUSR1 schema reload handler when running on Unix.
        // The handler loops forever, reloading on each signal, until the
        // server process exits — tracked on the server's JoinSet so graceful
        // shutdown awaits its termination.
        #[cfg(unix)]
        if let Some(ref schema_path) = app_state.schema_path {
            let reload_state = app_state.clone();
            let reload_path = schema_path.clone();
            self.tasks.spawn(async move {
                let mut sigusr1 = match tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::user_defined1(),
                ) {
                    Ok(s) => s,
                    Err(e) => {
                        warn!(error = %e, "Failed to install SIGUSR1 handler — schema hot-reload disabled");
                        return;
                    },
                };
                loop {
                    sigusr1.recv().await;
                    info!(
                        path = %reload_path.display(),
                        "Received SIGUSR1 — reloading schema"
                    );
                    match reload_state.reload_schema(&reload_path).await {
                        Ok(()) => {
                            let hash = reload_state.executor().schema().content_hash();
                            reload_state
                                .metrics
                                .schema_reloads_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            info!(schema_hash = %hash, "Schema reloaded successfully via SIGUSR1");
                        },
                        Err(e) => {
                            reload_state
                                .metrics
                                .schema_reload_errors_total
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            error!(
                                error = %e,
                                path = %reload_path.display(),
                                "Schema reload failed via SIGUSR1 — keeping previous schema"
                            );
                        },
                    }
                }
            });
            info!(
                path = %schema_path.display(),
                "SIGUSR1 schema reload handler installed"
            );
        }

        Ok(())
    }

    /// Stop what [`Self::start_capabilities`] started that needs more than an abort,
    /// within the shutdown timeout. The `JoinSet`'s tasks are aborted after this.
    pub(super) async fn stop_capabilities(&self) {
        let timeout = std::time::Duration::from_secs(self.config.shutdown_timeout_secs);
        let stopped = tokio::time::timeout(timeout, async {
            #[cfg(feature = "observers")]
            if let Some(ref runtime) = self.observer_runtime {
                let mut guard = runtime.write().await;
                if guard.health().running {
                    match guard.stop().await {
                        Ok(()) => info!("Observer runtime stopped cleanly"),
                        Err(e) => warn!("Observer runtime shutdown error: {e}"),
                    }
                }
            }
        })
        .await;
        if stopped.is_err() {
            warn!(
                timeout_secs = self.config.shutdown_timeout_secs,
                "Shutdown drain timed out; forcing exit"
            );
        } else {
            info!("Graceful shutdown complete");
        }
    }
}
