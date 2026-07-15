//! The per-source scheduler loop (#573 D6, Phase 06 Step 3).
//!
//! One [`SourcePoller`] drives one Model B (Deno) source: on its cron schedule it
//! fires the connector, single-firing across replicas via the advisory lease, under
//! a host bound to both the source's durable cursor (Phase 04) and its `run_as`
//! query executor (Step 2). It is the Model B analogue of the poll-IMAP
//! `MailboxPoller` — cron-tick instead of a fixed interval, and a Deno guest with a
//! cursor + executor host instead of a native `PullSource`.
//!
//! The tick loop mirrors the functions `CronScheduler` (a 60-second tick, missed
//! ticks skipped, in-memory [`CronExecutionState`] windowing) but replaces its
//! fire-and-forget `NoopHostContext` path with the leased, cursor+executor host a
//! source needs to read its watermark and mutate.

use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use fraiseql_functions::{
    EventPayload, FunctionModule, FunctionObserver, FunctionResult, ResourceLimits,
    host::{
        dyn_context::DynHostContext,
        live::{HostContextConfig, LiveHostContext, QueryExecutor},
    },
    triggers::{CronExecutionState, CronSchedule},
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore, RunOutcome};
use tracing::{debug, info, warn};

/// The collaborators one [`SourcePoller`] drives — assembled by the lifecycle from
/// a compiled [`SourceDefinition`](fraiseql_core::schema::SourceDefinition).
pub struct SourcePoller {
    /// The source name — the cursor row and advisory-lease key.
    source_name:  String,
    /// The parsed cron schedule the source fires on.
    schedule:     CronSchedule,
    /// The Deno connector module to invoke.
    module:       FunctionModule,
    /// The runtime-agnostic function observer that dispatches the module.
    observer:     Arc<FunctionObserver>,
    /// The durable cursor store, bound onto each firing's host.
    cursor_store: PostgresSourceCursorStore,
    /// The query-executor bridge (the source's `run_as` identity), bound onto each
    /// firing's host. A trait object so the lifecycle passes a `SourceQueryExecutor`
    /// while tests pass a stub.
    executor:     Arc<dyn QueryExecutor>,
    /// The single-firing runner (advisory lease keyed on the source name).
    runner:       LeaseGuardedRunner,
    /// Host config (SSRF allowlist, timeouts) for the connector's outbound I/O.
    host_config:  HostContextConfig,
    /// Guest resource limits.
    limits:       ResourceLimits,
    /// In-memory fire-window state (the durable cursor is the real resume point, so
    /// missed-fire catch-up across restarts is unnecessary — the next fire resumes
    /// from the cursor).
    state:        CronExecutionState,
}

impl SourcePoller {
    /// Assemble a poller. `runner`, `cursor_store`, and `executor` must all be keyed
    /// on / scoped to `source_name`.
    // Reason: a constructor wiring a source's fixed set of runtime collaborators; a
    // params struct would relocate the same fields without reducing coupling.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        source_name: impl Into<String>,
        schedule: CronSchedule,
        module: FunctionModule,
        observer: Arc<FunctionObserver>,
        cursor_store: PostgresSourceCursorStore,
        executor: Arc<dyn QueryExecutor>,
        runner: LeaseGuardedRunner,
        host_config: HostContextConfig,
        limits: ResourceLimits,
    ) -> Self {
        Self {
            source_name: source_name.into(),
            schedule,
            module,
            observer,
            cursor_store,
            executor,
            runner,
            host_config,
            limits,
            state: CronExecutionState::new(),
        }
    }

    /// Build the Model B host for one firing: a live host bound to the source's
    /// durable cursor *and* its `run_as` executor, so the guest can read/advance its
    /// watermark and issue `fraiseql_query` mutations.
    fn build_host(&self, payload: EventPayload) -> Arc<dyn DynHostContext> {
        Arc::new(
            LiveHostContext::new(payload, self.host_config.clone())
                .with_source_cursor(self.source_name.clone(), self.cursor_store.clone())
                .with_executor(Arc::clone(&self.executor)),
        )
    }

    /// Fire the source once, under the lease. Returns the outcome: skipped (another
    /// replica leads), or ran with the guest's own result. An acquire failure is
    /// logged and reported as a skip (the guest did not run this tick).
    async fn fire_once(
        &self,
        now: DateTime<Utc>,
    ) -> RunOutcome<fraiseql_error::Result<FunctionResult>> {
        let payload = build_source_payload(&self.source_name, &self.schedule.expression, now);
        let attempt = self
            .runner
            .run(|| async {
                let host = self.build_host(payload.clone());
                self.observer
                    .invoke_with_context(&self.module, payload.clone(), host, self.limits.clone())
                    .await
            })
            .await;
        match attempt {
            Ok(outcome) => outcome,
            Err(error) => {
                warn!(source = %self.source_name, %error, "source lease acquire failed — skipping tick");
                RunOutcome::SkippedNotLeader
            },
        }
    }

    /// Run the source forever: tick once a minute, fire when the schedule window
    /// opens (and has not already fired in it). Shutdown is by task abort — the
    /// lifecycle drives the poller on its `JoinSet`.
    pub async fn run_forever(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Skip the immediate initial tick so a source does not fire on startup.
        ticker.tick().await;
        info!(
            source = %self.source_name,
            schedule = %self.schedule.expression,
            "source scheduler started"
        );
        loop {
            ticker.tick().await;
            let now = Utc::now();
            if !self.state.should_execute(&self.schedule, &now) {
                continue;
            }
            self.state.record_execution(now);
            match self.fire_once(now).await {
                RunOutcome::Ran(Ok(_)) => {
                    info!(source = %self.source_name, "source fired");
                },
                RunOutcome::Ran(Err(error)) => {
                    warn!(source = %self.source_name, %error, "source invocation failed");
                },
                RunOutcome::SkippedNotLeader => {
                    debug!(source = %self.source_name, "source skipped — another replica leads");
                },
            }
        }
    }
}

/// Build the event payload for one source firing. The connector reads its trigger
/// context from `ctx.eventPayload`; the durable cursor (`ctx.cursor`) carries the
/// resume point, not this payload.
fn build_source_payload(source_name: &str, schedule: &str, now: DateTime<Utc>) -> EventPayload {
    EventPayload {
        trigger_type: format!("source:{source_name}"),
        entity:       "source".to_string(),
        event_kind:   "scheduled".to_string(),
        data:         serde_json::json!({
            "source": source_name,
            "schedule": schedule,
            "scheduled_at": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        }),
        timestamp:    now,
    }
}

#[cfg(test)]
mod tests;
