//! The per-source scheduler loop (#573).
//!
//! One [`SourcePoller`] drives one Model B (Deno) source: on its cron schedule it
//! fires the connector, single-firing across replicas via the advisory lease, under
//! a host bound to both the source's durable cursor and its `run_as`
//! query executor. It is the Model B analogue of the poll-IMAP
//! `MailboxPoller` — cron-tick instead of a fixed interval, and a Deno guest with a
//! cursor + executor host instead of a native `PullSource`.
//!
//! The tick loop mirrors the functions `CronScheduler` (a 60-second tick, missed
//! ticks skipped, in-memory [`CronExecutionState`] windowing) but replaces its
//! fire-and-forget `NoopHostContext` path with the leased, cursor+executor host a
//! source needs to read its watermark and mutate.

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
    triggers::{CronExecutionState, CronSchedule},
};
use fraiseql_observers::{
    DispatchSource, LeaseGuardedRunner, PostgresSourceCursorStore, RunOutcome,
    derive_idempotency_token,
};
use tracing::{debug, info, warn};

use super::metrics;

/// The collaborators one [`SourcePoller`] drives — assembled by the lifecycle from
/// a compiled [`SourceDefinition`](fraiseql_core::schema::SourceDefinition).
pub struct SourcePoller {
    /// The source name — the cursor row and advisory-lease key.
    source_name:     String,
    /// The parsed cron schedule the source fires on.
    schedule:        CronSchedule,
    /// The Deno connector module to invoke.
    module:          FunctionModule,
    /// The runtime-agnostic function observer that dispatches the module.
    observer:        Arc<FunctionObserver>,
    /// The durable cursor store, bound onto each firing's host.
    cursor_store:    PostgresSourceCursorStore,
    /// The query-executor bridge (the source's `run_as` identity), bound onto each
    /// firing's host. A trait object so the lifecycle passes a `SourceQueryExecutor`
    /// while tests pass a stub.
    executor:        Arc<dyn QueryExecutor>,
    /// The single-firing runner (advisory lease keyed on the source name).
    runner:          LeaseGuardedRunner,
    /// Host config (SSRF allowlist, timeouts) for the connector's outbound I/O.
    host_config:     HostContextConfig,
    /// Guest resource limits.
    limits:          ResourceLimits,
    /// The HMAC subkey that signs each firing's idempotency token, from the server
    /// HMAC secret. `Some` ⇒ the token is unforgeable, matching every other dispatch
    /// path (`after:mutation` / `after:ingest`); `None` ⇒ an unsigned digest (the
    /// zero-config default).
    idempotency_key: Option<Arc<[u8]>>,
    /// Whether to log the trigger payload on each firing (default off). Off-by-default
    /// mirrors the observer `log_payloads` gate: even though a source's trigger
    /// payload carries only schedule context — the external data the connector fetches
    /// never reaches the poller — payload logging stays opt-in for a uniform PII stance.
    log_payloads:    bool,
    /// In-memory fire-window state (the durable cursor is the real resume point, so
    /// missed-fire catch-up across restarts is unnecessary — the next fire resumes
    /// from the cursor).
    state:           CronExecutionState,
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
        idempotency_key: Option<Arc<[u8]>>,
        log_payloads: bool,
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
            idempotency_key,
            log_payloads,
            state: CronExecutionState::new(),
        }
    }

    /// The per-firing idempotency token: a stable hash of the source's identity and
    /// this firing's trigger payload (which carries the scheduled instant), so each
    /// scheduled tick has its own correlation id threaded through the metrics-adjacent
    /// logs and made available to the connector via `ctx.idempotencyToken` for
    /// idempotent writes. Never wall-clock/random *at the token site* — derived from
    /// the already-built payload so a resume re-derives the same value. Signed with
    /// the server HMAC subkey when one is configured (matching `after:mutation` /
    /// `after:ingest`), otherwise an unsigned digest.
    fn idempotency_token(&self, payload: &EventPayload) -> String {
        derive_idempotency_token(
            self.idempotency_key.as_deref(),
            DispatchSource::Source,
            &self.module.name,
            &payload.trigger_type,
            &payload.data,
        )
    }

    /// Build the Model B host for one firing: a live host bound to the source's
    /// durable cursor, its `run_as` executor, and this firing's idempotency token, so
    /// the guest can read/advance its watermark, issue `fraiseql_query` mutations, and
    /// key idempotent writes.
    fn build_host(
        &self,
        payload: EventPayload,
        idempotency_token: &str,
    ) -> Arc<dyn DynHostContext> {
        Arc::new(
            LiveHostContext::new(payload, self.host_config.clone())
                .with_source_cursor(self.source_name.clone(), self.cursor_store.clone())
                .with_executor(Arc::clone(&self.executor))
                .with_idempotency_token(idempotency_token.to_string()),
        )
    }

    /// Fire the source once, under the lease. Returns the outcome: skipped (another
    /// replica leads), or ran with the guest's own result. An acquire failure is
    /// logged and reported as a skip (the guest did not run this tick).
    ///
    /// This is the observability seam (#573): it meters every firing
    /// (`fraiseql_source_fires_total` / `_run_duration_seconds` / `_skips_not_leader_total`)
    /// and emits the structured fire/skip/error logs — all with the `source` and the
    /// per-firing `idempotency_token` — so the whole per-outcome record lives here
    /// rather than being split with the caller.
    async fn fire_once(
        &self,
        now: DateTime<Utc>,
    ) -> RunOutcome<fraiseql_error::Result<FunctionResult>> {
        let payload = build_source_payload(&self.source_name, &self.schedule.expression, now);
        let token = self.idempotency_token(&payload);
        if self.log_payloads {
            debug!(
                source = %self.source_name,
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
                let label = match &result {
                    Ok(_) => metrics::RESULT_OK,
                    Err(_) => metrics::RESULT_ERROR,
                };
                metrics::record_fire(&self.source_name, label, elapsed);
                match &result {
                    Ok(_) => info!(
                        source = %self.source_name,
                        idempotency_token = %token,
                        duration_ms = elapsed * 1000.0,
                        "source fired"
                    ),
                    Err(error) => warn!(
                        source = %self.source_name,
                        idempotency_token = %token,
                        duration_ms = elapsed * 1000.0,
                        %error,
                        "source invocation failed — re-runs from the last cursor next tick"
                    ),
                }
                RunOutcome::Ran(result)
            },
            Ok(RunOutcome::SkippedNotLeader) => {
                metrics::record_skip_not_leader(&self.source_name);
                debug!(
                    source = %self.source_name,
                    idempotency_token = %token,
                    "source skipped — another replica leads"
                );
                RunOutcome::SkippedNotLeader
            },
            Err(error) => {
                // A lease *acquire* error is a DB fault, not "another replica leads",
                // so it is not counted in skips_not_leader (which would then spike
                // misleadingly during a database outage) — the warn log carries it.
                warn!(
                    source = %self.source_name,
                    idempotency_token = %token,
                    %error,
                    "source lease acquire failed — skipping tick"
                );
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
            // `fire_once` owns the per-outcome metrics and structured logs.
            let _ = self.fire_once(now).await;
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
