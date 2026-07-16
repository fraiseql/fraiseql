//! Server-side `cron:` function scheduling (#595).
//!
//! Wires the compiled schema's `cron:` functions into a running server: one
//! [`CronPoller`] per cron function ticks on its schedule and — single-firing across
//! replicas via the sources' advisory lease — invokes the function on the phase-02
//! I/O-capable host (`fraiseql_query` under the function's `run_as` ceiling).
//!
//! A cron function is "a scheduled [`Source`](crate::sources) without a cursor": the
//! poller mirrors [`SourcePoller`](crate::sources) but drops the durable-cursor
//! binding and instead records each firing to `_fraiseql_cron_state` (observability +
//! a cross-restart "already fired this window" guard). It reuses `CronSchedule` /
//! `CronExecutionState` (schedule logic), [`LeaseGuardedRunner`] (the lease), and the
//! shared [`RunAsQueryExecutor`](crate::query_bridge) (the bridge).
//!
//! **Missed-tick policy: skip.** A server down over a scheduled instant does not
//! replay on next boot; the next matching window fires normally (cron has no
//! backlog/cursor to resume — unlike a source).

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
    triggers::{CronExecutionState, CronSchedule, CronTrigger},
};
use fraiseql_observers::{
    DispatchSource, LeaseGuardedRunner, RunOutcome, derive_idempotency_token,
};
use tracing::{debug, info, warn};

use crate::{query_bridge::RunAsQueryExecutor, subsystems::BeforeMutationHooks};

/// A minimal Postgres store for `_fraiseql_cron_state` (#595): the durable record of
/// when each cron function last fired and how many times.
#[derive(Clone)]
pub struct PgCronState {
    pool: sqlx::PgPool,
}

impl PgCronState {
    /// Wrap a pool.
    #[must_use]
    pub const fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Create the `_fraiseql_cron_state` table (idempotent DDL).
    ///
    /// # Errors
    ///
    /// Returns the sqlx error if the DDL cannot be applied.
    pub async fn init(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(fraiseql_functions::migrations::cron_migration_sql())
            .execute(&self.pool)
            .await
            .map(|_| ())
    }

    /// Record one firing: upsert `last_fired_at` and bump `fire_count`.
    ///
    /// # Errors
    ///
    /// Returns the sqlx error if the upsert fails.
    pub async fn record_fire(
        &self,
        function_name: &str,
        cron_expr: &str,
        fired_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO _fraiseql_cron_state \
                 (function_name, cron_expr, last_fired_at, fire_count) \
             VALUES ($1, $2, $3, 1) \
             ON CONFLICT (function_name, cron_expr) DO UPDATE SET \
                 last_fired_at = EXCLUDED.last_fired_at, \
                 fire_count = _fraiseql_cron_state.fire_count + 1, \
                 updated_at = now()",
        )
        .bind(function_name)
        .bind(cron_expr)
        .bind(fired_at)
        .execute(&self.pool)
        .await
        .map(|_| ())
    }
}

/// The per-cron-function scheduler loop (#595) — the cursor-less analogue of
/// [`SourcePoller`](crate::sources).
pub struct CronPoller {
    /// The function name — the advisory-lease key and cron-state row.
    function_name:   String,
    /// The parsed cron schedule.
    schedule:        CronSchedule,
    /// The raw cron expression (recorded in `_fraiseql_cron_state`).
    cron_expr:       String,
    /// The function module to invoke.
    module:          FunctionModule,
    /// The runtime-agnostic function observer.
    observer:        Arc<FunctionObserver>,
    /// The `fraiseql_query` bridge under the function's `run_as` identity (#594).
    executor:        Arc<dyn QueryExecutor>,
    /// The single-firing runner (advisory lease keyed on `cron:<function>`).
    runner:          LeaseGuardedRunner,
    /// Durable fire record.
    cron_state:      PgCronState,
    /// Host config (SSRF allowlist, timeouts).
    host_config:     HostContextConfig,
    /// Guest resource limits.
    limits:          ResourceLimits,
    /// HMAC subkey signing each firing's idempotency token.
    idempotency_key: Option<Arc<[u8]>>,
    /// In-memory fire-window state (deduplicates within a process; the lease
    /// deduplicates across replicas; `_fraiseql_cron_state` records the outcome).
    state:           CronExecutionState,
}

impl CronPoller {
    /// Assemble a poller. `runner`, `executor`, and `cron_state` are all keyed on /
    /// scoped to `function_name`.
    // Reason: a constructor wiring a cron function's fixed runtime collaborators; a
    // params struct would relocate the same fields without reducing coupling.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        trigger: &CronTrigger,
        schedule: CronSchedule,
        module: FunctionModule,
        observer: Arc<FunctionObserver>,
        executor: Arc<dyn QueryExecutor>,
        runner: LeaseGuardedRunner,
        cron_state: PgCronState,
        host_config: HostContextConfig,
        limits: ResourceLimits,
        idempotency_key: Option<Arc<[u8]>>,
    ) -> Self {
        Self {
            function_name: trigger.function_name.clone(),
            schedule,
            cron_expr: trigger.schedule.clone(),
            module,
            observer,
            executor,
            runner,
            cron_state,
            host_config,
            limits,
            idempotency_key,
            state: CronExecutionState::new(),
        }
    }

    /// The per-firing idempotency token — a stable hash of the function identity and
    /// this firing's payload, signed with the server HMAC subkey when configured.
    fn idempotency_token(&self, payload: &EventPayload) -> String {
        derive_idempotency_token(
            self.idempotency_key.as_deref(),
            // No dedicated Cron dispatch source; `Source` is the closest background
            // salt and the function name + trigger disambiguate the token.
            DispatchSource::Source,
            &self.module.name,
            &payload.trigger_type,
            &payload.data,
        )
    }

    /// Build the host for one firing: the phase-02 `run_as` executor + the firing's
    /// idempotency token. No cursor (cron functions have no watermark).
    fn build_host(
        &self,
        payload: EventPayload,
        idempotency_token: &str,
    ) -> Arc<dyn DynHostContext> {
        Arc::new(
            LiveHostContext::new(payload, self.host_config.clone())
                .with_executor(Arc::clone(&self.executor))
                .with_idempotency_token(idempotency_token.to_string()),
        )
    }

    /// Fire the cron function once, under the lease. Returns the outcome: skipped
    /// (another replica leads) or ran (with the guest's result). On a successful run
    /// the firing is recorded to `_fraiseql_cron_state`.
    async fn fire_once(
        &self,
        now: DateTime<Utc>,
    ) -> RunOutcome<fraiseql_error::Result<FunctionResult>> {
        let trigger = CronTrigger {
            function_name: self.function_name.clone(),
            schedule:      self.cron_expr.clone(),
            timezone:      "UTC".to_string(),
        };
        let payload = trigger.build_payload(&now);
        let token = self.idempotency_token(&payload);
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
                match &result {
                    Ok(_) => {
                        info!(
                            function = %self.function_name,
                            idempotency_token = %token,
                            duration_ms = elapsed * 1000.0,
                            "cron function fired"
                        );
                        // Record the firing durably (best-effort — a record failure
                        // must not crash the scheduler; the function already ran).
                        if let Err(error) = self
                            .cron_state
                            .record_fire(&self.function_name, &self.cron_expr, now)
                            .await
                        {
                            warn!(
                                function = %self.function_name,
                                %error,
                                "cron state record failed — the function ran but the fire was not persisted"
                            );
                        }
                    },
                    Err(error) => warn!(
                        function = %self.function_name,
                        idempotency_token = %token,
                        %error,
                        "cron function invocation failed"
                    ),
                }
                RunOutcome::Ran(result)
            },
            Ok(RunOutcome::SkippedNotLeader) => {
                debug!(
                    function = %self.function_name,
                    "cron function skipped — another replica leads"
                );
                RunOutcome::SkippedNotLeader
            },
            Err(error) => {
                warn!(
                    function = %self.function_name,
                    %error,
                    "cron lease acquire failed — skipping tick"
                );
                RunOutcome::SkippedNotLeader
            },
        }
    }

    /// Run forever: tick once a minute, fire when the schedule window opens. Shutdown
    /// is by task abort — the lifecycle drives the poller on its `JoinSet`.
    pub async fn run_forever(mut self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Skip the immediate initial tick so a cron function does not fire on startup.
        ticker.tick().await;
        info!(
            function = %self.function_name,
            schedule = %self.cron_expr,
            "cron scheduler started"
        );
        loop {
            ticker.tick().await;
            let now = Utc::now();
            if !self.state.should_execute(&self.schedule, &now) {
                continue;
            }
            self.state.record_execution(now);
            let _ = self.fire_once(now).await;
        }
    }
}

/// Build one [`CronPoller`] per `cron:` function in the compiled schema (#595).
///
/// Each poller runs under the function's `run_as` identity (via
/// [`RunAsQueryExecutor`] over the hot-reloadable `executor`) and single-fires across
/// replicas on a PostgreSQL advisory lease keyed on the function name. A cron trigger
/// whose module never loaded, or whose expression does not parse, is skipped with a
/// warning. The `_fraiseql_cron_state` table must already be initialized
/// ([`PgCronState::init`]).
pub fn build_cron_pollers<A>(
    db_pool: &sqlx::PgPool,
    executor: &Arc<arc_swap::ArcSwap<fraiseql_core::runtime::Executor<A>>>,
    hooks: &BeforeMutationHooks,
    host_config: &HostContextConfig,
    limits: &ResourceLimits,
) -> Vec<CronPoller>
where
    A: fraiseql_core::db::traits::DatabaseAdapter + Send + Sync + 'static,
{
    let cron_state = PgCronState::new(db_pool.clone());
    hooks
        .trigger_registry
        .cron_triggers
        .iter()
        .filter_map(|trigger| {
            let schedule = match CronSchedule::parse(&trigger.schedule) {
                Ok(schedule) => schedule,
                Err(error) => {
                    warn!(
                        function = %trigger.function_name,
                        expression = %trigger.schedule,
                        %error,
                        "invalid cron schedule — skipping function"
                    );
                    return None;
                },
            };
            let module = hooks.module_registry.get(&trigger.function_name)?.clone();

            // The function's mutations run under its `run_as` ceiling (fail-closed
            // when absent); the request-id correlates the function in the audit
            // envelope, matching the sources pattern.
            let run_as = hooks.run_as.get(&trigger.function_name).cloned().unwrap_or_default();
            let identity = run_as.identity(&trigger.function_name, &trigger.function_name);
            let query_executor: Arc<dyn QueryExecutor> =
                Arc::new(RunAsQueryExecutor::new(Arc::clone(executor), identity));

            Some(CronPoller::new(
                trigger,
                schedule,
                module,
                Arc::clone(&hooks.observer),
                query_executor,
                LeaseGuardedRunner::postgres(
                    db_pool.clone(),
                    format!("cron:{}", trigger.function_name),
                ),
                cron_state.clone(),
                host_config.clone(),
                limits.clone(),
                hooks.idempotency_key.clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
