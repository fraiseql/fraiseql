//! The one loop behind every schedule the server keeps: `cron:` functions and sources.
//!
//! A [`ScheduledPoller`] ticks once a minute, decides on its [`Schedule`] (a cron
//! expression read on a timezone's wall clock) whether this tick opens a window that has
//! not fired, and fires its module once under an advisory lease, so that across replicas
//! only one fires. What differs by [`ScheduledKind`] is small: the payload the guest
//! receives, whether its host carries a cursor, what is recorded after a run, and the
//! metrics and logs.
//!
//! The constructors live with their kinds: [`ScheduledPoller::cron`] in
//! [`crate::cron`], and `ScheduledPoller::source` in `crate::sources`.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use fraiseql_functions::{
    EventPayload, FunctionModule, FunctionObserver, FunctionResult, ResourceLimits,
    host::{
        dyn_context::DynHostContext,
        live::{HostContextConfig, LiveHostContext, QueryExecutor},
    },
    triggers::{CronDecision, CronExecutionState, CronTrigger, Schedule},
};
use fraiseql_observers::{
    DispatchSource, LeaseGuardedRunner, RunOutcome, derive_idempotency_token,
};
use fraiseql_time::Timestamp;
use tracing::{debug, info, warn};

use super::PgCronState;

/// What a [`ScheduledPoller`] fires, and what it keeps between firings.
pub enum ScheduledKind {
    /// A `cron:` function. A successful run is recorded in `_fraiseql_cron_state`, which
    /// a restart reads back so it does not fire the same window twice.
    Cron {
        /// The durable fire record.
        cron_state: PgCronState,
    },
    /// A source's connector. Its cursor is the resume point, so nothing else is kept.
    #[cfg(feature = "sources")]
    Source {
        /// The cursor key the connector advances: the declared `cursor`, or the
        /// source's name.
        cursor_name:  String,
        /// The durable cursor store bound onto each firing's host.
        cursor_store: fraiseql_observers::PostgresSourceCursorStore,
        /// Whether each firing's trigger payload is logged (default off).
        log_payloads: bool,
    },
}

/// One scheduled module: the tick loop, the lease, and the invocation.
pub struct ScheduledPoller {
    /// The function or source name: the label in logs, metrics and the lease.
    pub(crate) name:            String,
    pub(crate) kind:            ScheduledKind,
    /// The cron expression and the timezone it is read in.
    pub(crate) schedule:        Schedule,
    /// The module to invoke.
    pub(crate) module:          FunctionModule,
    /// The runtime-agnostic observer that dispatches the module.
    pub(crate) observer:        Arc<FunctionObserver>,
    /// The `fraiseql_query` bridge under the module's `run_as` identity.
    pub(crate) executor:        Arc<dyn QueryExecutor>,
    /// The `run_as` identity: a scheduled firing has no request caller, so the host's
    /// `auth_context` is the module's own granted authority, never an anonymous one.
    pub(crate) identity:        fraiseql_core::security::SecurityContext,
    /// The single-firing runner (an advisory lease keyed per poller).
    pub(crate) runner:          LeaseGuardedRunner,
    /// Host config (SSRF allowlist, timeouts) for the module's outbound I/O.
    pub(crate) host_config:     HostContextConfig,
    /// Guest resource limits.
    pub(crate) limits:          ResourceLimits,
    /// The HMAC subkey signing each firing's idempotency token; `None` leaves the
    /// token an unsigned digest.
    pub(crate) idempotency_key: Option<Arc<[u8]>>,
    /// In-memory window state: deduplicates within a process, as the lease does across
    /// replicas.
    pub(crate) state:           CronExecutionState,
}

impl ScheduledPoller {
    /// The kind's label in logs.
    const fn kind_label(&self) -> &'static str {
        match self.kind {
            ScheduledKind::Cron { .. } => "cron",
            #[cfg(feature = "sources")]
            ScheduledKind::Source { .. } => "source",
        }
    }

    /// The trigger payload of the firing the tick at `now` opens.
    pub(crate) fn payload(&self, now: fraiseql_time::Timestamp) -> EventPayload {
        // The scheduled instant: the start of the minute the tick fell in, which is the
        // window it fires. Replicas tick at different seconds, so a window fired twice
        // (a replica died before recording it) carries the same instant, and so the
        // same idempotency token, both times.
        let now = fraiseql_time::Timestamp::from_second(now.as_second().div_euclid(60) * 60)
            .unwrap_or(now);
        match &self.kind {
            ScheduledKind::Cron { .. } => CronTrigger {
                function_name: self.name.clone(),
                schedule:      self.schedule.expr.expression.clone(),
                timezone:      self.schedule.zone_name().to_owned(),
            }
            .build_payload(&now),
            #[cfg(feature = "sources")]
            ScheduledKind::Source { .. } => crate::sources::build_source_payload(
                &self.name,
                &self.schedule.expr.expression,
                self.schedule.zone_name(),
                now,
            ),
        }
    }

    /// The per-firing idempotency token: a stable hash of the module and this firing's
    /// payload (which carries the scheduled instant), signed when a key is configured.
    /// Derived from the payload, never from the clock at this site, so a resume derives
    /// the same token.
    pub(crate) fn idempotency_token(&self, payload: &EventPayload) -> String {
        derive_idempotency_token(
            self.idempotency_key.as_deref(),
            // No dedicated cron dispatch source: `Source` is the background salt, and the
            // module and trigger type tell the two kinds apart.
            DispatchSource::Source,
            &self.module.name,
            &payload.trigger_type,
            &payload.data,
        )
    }

    /// The host of one firing: the `run_as` executor and identity, the idempotency
    /// token, and for a source its cursor.
    pub(crate) fn build_host(
        &self,
        payload: EventPayload,
        idempotency_token: &str,
    ) -> Arc<dyn DynHostContext> {
        let host = LiveHostContext::new(payload, self.host_config.clone());
        let host = match &self.kind {
            ScheduledKind::Cron { .. } => host,
            #[cfg(feature = "sources")]
            ScheduledKind::Source {
                cursor_name,
                cursor_store,
                ..
            } => host.with_source_cursor(cursor_name.clone(), cursor_store.clone()),
        };
        Arc::new(
            host.with_executor(Arc::clone(&self.executor))
                .with_idempotency_token(idempotency_token.to_string())
                .with_security_context(self.identity.clone()),
        )
    }

    /// The key of this poller's window record in `_fraiseql_cron_state`: a cron function's
    /// name, or `source:` and a source's name, which no function name can be.
    fn window_key(&self) -> String {
        match self.kind {
            ScheduledKind::Cron { .. } => self.name.clone(),
            #[cfg(feature = "sources")]
            ScheduledKind::Source { .. } => format!("source:{}", self.name),
        }
    }

    /// Under the lease, on the session holding it: whether another process already fired
    /// the window `now` falls in. `Ok(false)` without a PostgreSQL lease (one process).
    async fn window_fired_elsewhere(
        &self,
        key: &str,
        now: fraiseql_time::Timestamp,
    ) -> Result<bool, String> {
        let expression = self.schedule.expr.expression.clone();
        let key = key.to_string();
        let last = self
            .runner
            .on_held_session(move |conn| {
                Box::pin(async move { super::load_last_fired_on(conn, &key, &expression).await })
            })
            .await
            .map_err(|e| e.to_string())?;
        let last = last
            .flatten()
            .map(fraiseql_time::Timestamp::try_from)
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(last.is_some_and(|last| {
            matches!(
                self.schedule.decide(&CronExecutionState::resuming_from(last), &now),
                CronDecision::AlreadyFired { .. }
            )
        }))
    }

    /// Record, on the session holding the lease, that the window `now` falls in fired: what
    /// tells another process ticking later in the window not to fire it again. A cron
    /// function without a PostgreSQL lease records through its pool, for its restarts.
    async fn record_window(&self, key: &str, now: Timestamp) {
        let expression = self.schedule.expr.expression.clone();
        let owned_key = key.to_string();
        let recorded = self
            .runner
            .on_held_session(move |conn| {
                Box::pin(
                    async move { super::record_fire_on(conn, &owned_key, &expression, now).await },
                )
            })
            .await;
        let result = match (recorded, &self.kind) {
            (Ok(Some(())), _) => Ok(()),
            (Ok(None), ScheduledKind::Cron { cron_state }) => cron_state
                .record_fire(key, &self.schedule.expr.expression, now)
                .await
                .map_err(|e| e.to_string()),
            #[cfg(feature = "sources")]
            (Ok(None), ScheduledKind::Source { .. }) => Ok(()),
            (Err(error), _) => Err(error.to_string()),
        };
        if let Err(error) = result {
            // Best effort: the module already ran, and a failed record must not stop the
            // scheduler. Another process ticking later in this window may fire it again.
            warn!(
                kind = self.kind_label(),
                name = %self.name,
                %error,
                "window record failed: the module ran but its window was not persisted"
            );
        }
    }

    /// Fire once, under the lease: skipped (another replica leads, the lease could not be
    /// acquired, or another process already fired this window), or ran with the guest's
    /// result. A run spends its window whatever the result, as on one process, and the
    /// window is recorded before the lease is released. Every outcome is metered and
    /// logged here, with the firing's idempotency token.
    pub(crate) async fn fire_once(&self, at: fraiseql_time::Timestamp) -> Tick {
        let payload = self.payload(at);
        let now = at;
        let token = self.idempotency_token(&payload);
        #[cfg(feature = "sources")]
        if let ScheduledKind::Source {
            log_payloads: true, ..
        } = self.kind
        {
            debug!(
                source = %self.name,
                idempotency_token = %token,
                payload = %payload.data,
                "source firing (payload logging enabled)"
            );
        }
        let started = Instant::now();
        let key = self.window_key();
        let attempt = self
            .runner
            .run(|| async {
                match self.window_fired_elsewhere(&key, at).await {
                    Ok(false) => {},
                    Ok(true) => return Window::FiredElsewhere,
                    Err(error) => return Window::Unread(error),
                }
                let host = self.build_host(payload.clone(), &token);
                let result = self
                    .observer
                    .invoke_with_context(&self.module, payload.clone(), host, self.limits.clone())
                    .await;
                self.record_window(&key, now).await;
                Window::Ran(result)
            })
            .await;
        let elapsed = started.elapsed().as_secs_f64();
        match attempt {
            Ok(RunOutcome::Ran(Window::Ran(result))) => {
                self.report_ran(&result, &token, elapsed);
                Tick::Ran {
                    succeeded: result.is_ok(),
                }
            },
            Ok(RunOutcome::Ran(Window::FiredElsewhere)) => {
                debug!(
                    kind = self.kind_label(),
                    name = %self.name,
                    idempotency_token = %token,
                    "scheduled firing skipped: another process fired this window"
                );
                Tick::AlreadyFired
            },
            Ok(RunOutcome::Ran(Window::Unread(error))) => {
                // Without the record the lease cannot say whether this window fired, and a
                // session that cannot be read may no longer hold the lease: nothing runs.
                warn!(
                    kind = self.kind_label(),
                    name = %self.name,
                    idempotency_token = %token,
                    %error,
                    "scheduled firing skipped: the window record could not be read under the lease"
                );
                Tick::WindowUnread
            },
            Ok(RunOutcome::SkippedNotLeader) => {
                #[cfg(feature = "sources")]
                if matches!(self.kind, ScheduledKind::Source { .. }) {
                    crate::sources::metrics::record_skip_not_leader(&self.name);
                }
                debug!(
                    kind = self.kind_label(),
                    name = %self.name,
                    idempotency_token = %token,
                    "scheduled firing skipped: another replica leads"
                );
                Tick::SkippedNotLeader
            },
            Err(error) => {
                // An acquire error is a database fault, not another replica leading, so
                // it is not counted as a skip (which would spike during an outage).
                warn!(
                    kind = self.kind_label(),
                    name = %self.name,
                    idempotency_token = %token,
                    %error,
                    "scheduled firing skipped: the lease could not be acquired"
                );
                Tick::SkippedNotLeader
            },
        }
    }

    /// Meter and log a run.
    fn report_ran(
        &self,
        result: &fraiseql_error::Result<FunctionResult>,
        token: &str,
        elapsed: f64,
    ) {
        match &self.kind {
            ScheduledKind::Cron { .. } => {
                let metric_result = match result {
                    Ok(_) => crate::function_metrics::RESULT_OK,
                    // A failed cron firing is retried by the next window, like a source,
                    // and never dead-lettered, so it is `error`.
                    Err(_) => crate::function_metrics::RESULT_ERROR,
                };
                crate::function_metrics::record_dispatch(
                    &self.name,
                    crate::function_metrics::KIND_CRON,
                    metric_result,
                    elapsed,
                );
                match result {
                    Ok(_) => {
                        info!(
                            function = %self.name,
                            idempotency_token = %token,
                            duration_ms = elapsed * 1000.0,
                            "cron function fired"
                        );
                    },
                    Err(error) => warn!(
                        function = %self.name,
                        idempotency_token = %token,
                        %error,
                        "cron function invocation failed"
                    ),
                }
            },
            #[cfg(feature = "sources")]
            ScheduledKind::Source { .. } => {
                let label = match result {
                    Ok(_) => crate::sources::metrics::RESULT_OK,
                    Err(_) => crate::sources::metrics::RESULT_ERROR,
                };
                crate::sources::metrics::record_fire(&self.name, label, elapsed);
                match result {
                    Ok(_) => info!(
                        source = %self.name,
                        idempotency_token = %token,
                        duration_ms = elapsed * 1000.0,
                        "source fired"
                    ),
                    Err(error) => warn!(
                        source = %self.name,
                        idempotency_token = %token,
                        duration_ms = elapsed * 1000.0,
                        %error,
                        "source invocation failed: re-runs from the last cursor next tick"
                    ),
                }
            },
        }
    }

    /// Run forever: tick once a minute and fire when the schedule opens a window that has
    /// not fired. Nothing fires on startup. Shutdown is by task abort: the lifecycle
    /// drives the poller on its `JoinSet`.
    pub async fn run_forever(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_mins(1));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        ticker.tick().await;
        info!(
            kind = self.kind_label(),
            name = %self.name,
            schedule = %self.schedule.expr.expression,
            timezone = %self.schedule.zone_name(),
            "scheduler started"
        );
        loop {
            ticker.tick().await;
            self.tick(fraiseql_time::Timestamp::now()).await;
        }
    }

    /// One tick at `at`: fire when the schedule opens a window that has not fired.
    pub(crate) async fn tick(&mut self, at: fraiseql_time::Timestamp) -> Tick {
        match self.schedule.decide(&self.state, &at) {
            CronDecision::Fire => {},
            // The normal idle tick.
            CronDecision::NotDue => return Tick::NotDue,
            // Never silent: the window guard suppressing a due tick is expected at
            // most once per window, after a restart mid-window.
            CronDecision::AlreadyFired {
                window_start,
                last_executed,
            } => {
                warn!(
                    kind = self.kind_label(),
                    name = %self.name,
                    schedule = %self.schedule.expr.expression,
                    %window_start,
                    %last_executed,
                    "scheduled tick skipped: this window already fired"
                );
                return Tick::AlreadyFired;
            },
        }
        self.state.record_execution(at);
        self.fire_once(at).await
    }
}

/// What one [`ScheduledPoller::tick`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// The schedule does not open a window at this tick.
    NotDue,
    /// The window this tick falls in already fired.
    AlreadyFired,
    /// Another process held the lease; the module did not run.
    SkippedNotLeader,
    /// The module ran (its result is metered and logged by the poller).
    Ran {
        /// Whether the module returned without error.
        succeeded: bool,
    },
    /// The window record could not be read under the lease; the module did not run.
    WindowUnread,
}

/// What a firing did under the lease.
enum Window {
    /// Another process already fired this window.
    FiredElsewhere,
    /// The window record could not be read on the lease's session.
    Unread(String),
    /// The module ran, with its result.
    Ran(fraiseql_error::Result<FunctionResult>),
}
