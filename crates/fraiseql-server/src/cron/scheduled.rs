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

use chrono::{DateTime, Utc};
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

    /// The trigger payload of the firing at `now`.
    pub(crate) fn payload(&self, now: DateTime<Utc>) -> EventPayload {
        match &self.kind {
            ScheduledKind::Cron { .. } => CronTrigger {
                function_name: self.name.clone(),
                schedule:      self.schedule.expr.expression.clone(),
                timezone:      self.schedule.tz.name().to_owned(),
            }
            .build_payload(&now),
            #[cfg(feature = "sources")]
            ScheduledKind::Source { .. } => crate::sources::build_source_payload(
                &self.name,
                &self.schedule.expr.expression,
                self.schedule.tz.name(),
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

    /// Fire once, under the lease: skipped (another replica leads, or the lease could
    /// not be acquired), or ran with the guest's result. Every outcome is metered and
    /// logged here, with the firing's idempotency token.
    pub(crate) async fn fire_once(
        &self,
        now: DateTime<Utc>,
    ) -> RunOutcome<fraiseql_error::Result<FunctionResult>> {
        let payload = self.payload(now);
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
        let attempt = self
            .runner
            .run(|| async {
                let host = self.build_host(payload.clone(), &token);
                self.observer
                    .invoke_with_context(&self.module, payload.clone(), host, self.limits.clone())
                    .await
            })
            .await;
        let elapsed = started.elapsed().as_secs_f64();
        match attempt {
            Ok(RunOutcome::Ran(result)) => {
                self.report_ran(&result, &token, elapsed, now).await;
                RunOutcome::Ran(result)
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
                RunOutcome::SkippedNotLeader
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
                RunOutcome::SkippedNotLeader
            },
        }
    }

    /// Meter and log a run, and for a cron function record it durably.
    async fn report_ran(
        &self,
        result: &fraiseql_error::Result<FunctionResult>,
        token: &str,
        elapsed: f64,
        now: DateTime<Utc>,
    ) {
        match &self.kind {
            ScheduledKind::Cron { cron_state } => {
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
                        // Best effort: the function already ran, and a failed record
                        // must not stop the scheduler.
                        if let Err(error) = cron_state
                            .record_fire(&self.name, &self.schedule.expr.expression, now)
                            .await
                        {
                            warn!(
                                function = %self.name,
                                %error,
                                "cron state record failed: the function ran but the fire was not persisted"
                            );
                        }
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
            timezone = %self.schedule.tz.name(),
            "scheduler started"
        );
        loop {
            ticker.tick().await;
            let now = Utc::now();
            match self.schedule.decide(&self.state, &now) {
                CronDecision::Fire => {},
                // The normal idle tick.
                CronDecision::NotDue => continue,
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
                    continue;
                },
            }
            self.state.record_execution(now);
            let _ = self.fire_once(now).await;
        }
    }
}
