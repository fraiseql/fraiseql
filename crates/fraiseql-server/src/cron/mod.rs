//! Server-side `cron:` function scheduling (#595).
//!
//! Wires the compiled schema's `cron:` functions into a running server: one
//! [`ScheduledPoller`](crate::cron::ScheduledPoller) per cron function ticks on its
//! schedule, read on the function's `timezone` (UTC by default), and invokes the
//! function on the I/O-capable host (`fraiseql_query` under the function's `run_as`
//! ceiling), firing once across replicas under an advisory lease.
//!
//! A cron function is a scheduled [`Source`](crate::sources) without a cursor: both run
//! on [`ScheduledPoller`](crate::cron::ScheduledPoller), and a cron firing is recorded in
//! `_fraiseql_cron_state` instead (observability, and the cross-restart "already fired
//! this window" guard).
//!
//! **Missed-tick policy: skip.** A server down over a scheduled instant does not
//! replay on next boot; the next matching window fires normally (cron has no
//! backlog/cursor to resume, unlike a source).

mod scheduled;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use fraiseql_functions::{
    FunctionModule, FunctionObserver, ResourceLimits,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::{CronExecutionState, CronTrigger, Schedule},
};
use fraiseql_observers::LeaseGuardedRunner;
use tracing::warn;

pub use self::scheduled::{ScheduledKind, ScheduledPoller};
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
        crate::migration_lock::run_migration(
            &self.pool,
            fraiseql_functions::migrations::cron_migration_sql(),
        )
        .await
    }

    /// Read back the last recorded firing for one `(function, expression)` pair.
    ///
    /// `None` when the function has never fired (or its expression changed —
    /// stale state for a different schedule must not suppress the new one).
    ///
    /// # Errors
    ///
    /// Returns the sqlx error if the read fails.
    pub async fn load_last_fired(
        &self,
        function_name: &str,
        cron_expr: &str,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT last_fired_at FROM _fraiseql_cron_state \
             WHERE function_name = $1 AND cron_expr = $2",
        )
        .bind(function_name)
        .bind(cron_expr)
        .fetch_optional(&self.pool)
        .await
    }

    /// Rebuild the in-memory fire-window state from the durable record — the
    /// documented cross-restart "already fired this window" guard (#796: the
    /// table was written on every firing and never read back, so a restart
    /// inside a fired window re-fired it).
    ///
    /// # Errors
    ///
    /// Returns the sqlx error if the read fails. Callers must treat that as
    /// fatal at boot: a guard that cannot load its state is not a guard.
    pub async fn resume_state(
        &self,
        function_name: &str,
        cron_expr: &str,
    ) -> Result<CronExecutionState, sqlx::Error> {
        Ok(self
            .load_last_fired(function_name, cron_expr)
            .await?
            .map_or_else(CronExecutionState::new, CronExecutionState::resuming_from))
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

impl ScheduledPoller {
    /// The poller of one `cron:` function. `runner` is leased on `cron:<function>`, and
    /// `executor` and `identity` are the function's `run_as`; `initial_state` is read
    /// back from `_fraiseql_cron_state`.
    // Reason: a constructor wiring a cron function's fixed runtime collaborators; a
    // params struct would relocate the same fields without reducing coupling.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cron(
        trigger: &CronTrigger,
        schedule: Schedule,
        module: FunctionModule,
        observer: Arc<FunctionObserver>,
        executor: Arc<dyn QueryExecutor>,
        identity: fraiseql_core::security::SecurityContext,
        runner: LeaseGuardedRunner,
        cron_state: PgCronState,
        host_config: HostContextConfig,
        limits: ResourceLimits,
        idempotency_key: Option<Arc<[u8]>>,
        initial_state: CronExecutionState,
    ) -> Self {
        Self {
            name: trigger.function_name.clone(),
            kind: ScheduledKind::Cron { cron_state },
            schedule,
            module,
            observer,
            executor,
            identity,
            runner,
            host_config,
            limits,
            idempotency_key,
            state: initial_state,
        }
    }
}

/// A `cron:` function the server can schedule: its trigger, parsed schedule, loaded
/// module, and the limits it runs under (its declared `timeout_ms`, #1627).
struct Schedulable<'a> {
    trigger:  &'a CronTrigger,
    schedule: Schedule,
    module:   FunctionModule,
    limits:   ResourceLimits,
}

/// The `cron:` functions in `hooks` that can be scheduled: a trigger whose expression
/// does not parse is skipped with a warning, and one whose module never loaded is
/// skipped. Pure, so what a poller is built from is testable without a database.
fn schedulable(hooks: &BeforeMutationHooks) -> Vec<Schedulable<'_>> {
    hooks
        .trigger_registry
        .cron_triggers
        .iter()
        .filter_map(|trigger| {
            // The trigger registry refused a bad expression or timezone at compile and at
            // boot, so this only fails for a registry built some other way.
            let schedule = match Schedule::parse(&trigger.schedule, Some(&trigger.timezone)) {
                Ok(schedule) => schedule,
                Err(error) => {
                    warn!(
                        function = %trigger.function_name,
                        expression = %trigger.schedule,
                        %error,
                        "invalid cron schedule: skipping function"
                    );
                    return None;
                },
            };
            let module = hooks.module_registry.get(&trigger.function_name).cloned()?;
            Some(Schedulable {
                trigger,
                schedule,
                module,
                limits: hooks.limits_for(&trigger.function_name),
            })
        })
        .collect()
}

/// Build one [`ScheduledPoller`] per `cron:` function in the compiled schema (#595).
///
/// Each poller runs under the function's `run_as` identity (via
/// [`RunAsQueryExecutor`] over the hot-reloadable `executor`) and single-fires across
/// replicas on a PostgreSQL advisory lease keyed on the function name. A cron trigger
/// whose module never loaded, or whose expression does not parse, is skipped with a
/// warning. The `_fraiseql_cron_state` table must already be initialized
/// ([`PgCronState::init`]); each poller's fire-window state is **read back** from it
/// ([`PgCronState::resume_state`]) so a restart inside an already-fired window does
/// not re-fire it (#796).
///
/// # Errors
///
/// Returns the sqlx error if reading `_fraiseql_cron_state` fails — fatal at boot: a
/// cross-restart guard that cannot load its state is not a guard.
pub async fn build_cron_pollers(
    db_pool: &sqlx::PgPool,
    executor: &crate::query_bridge::ExecutorSource,
    hooks: &BeforeMutationHooks,
    host_config: &HostContextConfig,
) -> Result<Vec<ScheduledPoller>, sqlx::Error> {
    let cron_state = PgCronState::new(db_pool.clone());
    let mut pollers = Vec::new();
    for Schedulable {
        trigger,
        schedule,
        module,
        limits,
    } in schedulable(hooks)
    {
        // The function's mutations run under its `run_as` ceiling (fail-closed
        // when absent); the request-id correlates the function in the audit
        // envelope, matching the sources pattern.
        let run_as = hooks.run_as.get(&trigger.function_name).cloned().unwrap_or_default();
        let identity = run_as.identity(&trigger.function_name, &trigger.function_name);
        let query_executor: Arc<dyn QueryExecutor> =
            Arc::new(RunAsQueryExecutor::new(executor.clone(), identity.clone()));

        // The cross-restart guard: resume this function's window state from the
        // durable record before the first tick.
        let initial_state =
            cron_state.resume_state(&trigger.function_name, &trigger.schedule).await?;

        pollers.push(ScheduledPoller::cron(
            trigger,
            schedule,
            module,
            Arc::clone(&hooks.observer),
            query_executor,
            identity,
            LeaseGuardedRunner::postgres(
                db_pool.clone(),
                format!("cron:{}", trigger.function_name),
            ),
            cron_state.clone(),
            host_config.clone(),
            limits,
            hooks.idempotency_key.clone(),
            initial_state,
        ));
    }
    Ok(pollers)
}

#[cfg(test)]
mod tests;
