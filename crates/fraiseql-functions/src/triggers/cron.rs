//! Cron-scheduled triggers.
//!
//! Handles `cron:<expression>` triggers that fire on a schedule using POSIX cron expressions.
//!
//! ## Features
//!
//! - **Stateful**: Last execution time is persisted to `_fraiseql_cron_state` table
//! - **Missed Fire Detection**: Detects missed executions on restart
//! - **Configurable Replay**: Optionally replay missed fires on startup
//! - **Timezone Support**: Execute in specific timezone (defaults to UTC)
//!
//! ## Example Expressions
//!
//! - `0 * * * *` - Every hour
//! - `0 2 * * *` - Daily at 2 AM
//! - `*/5 * * * *` - Every 5 minutes
//! - `0 0 1 * *` - First day of every month

use crate::types::EventPayload;
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Serialize};

/// A parsed cron field that can be a number, wildcard, or step value.
#[derive(Debug, Clone)]
enum CronField {
    /// Wildcard (*) matches any value
    Any,
    /// Specific value
    Value(u32),
    /// Step value (*/5 matches every 5th)
    Step { base: Option<u32>, step: u32 },
    /// List of values (1,3,5)
    List(Vec<u32>),
    /// Range (1-5)
    Range { start: u32, end: u32 },
}

impl CronField {
    /// Check if this field matches the given value.
    fn matches(&self, value: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Value(v) => *v == value,
            CronField::Step { base, step } => {
                let start = base.unwrap_or(0);
                if value < start {
                    return false;
                }
                (value - start).is_multiple_of(*step)
            }
            CronField::List(values) => values.contains(&value),
            CronField::Range { start, end } => value >= *start && value <= *end,
        }
    }

    /// Parse a cron field from a string.
    fn parse(field: &str) -> Result<Self, String> {
        if field == "*" {
            return Ok(CronField::Any);
        }

        if let Some(step_idx) = field.find('/') {
            let (base_part, step_part) = field.split_at(step_idx);
            let step_str = &step_part[1..]; // Skip the '/'

            let step = step_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid step value: {}", step_str))?;

            let base = if base_part == "*" {
                None
            } else {
                Some(
                    base_part
                        .parse::<u32>()
                        .map_err(|_| format!("Invalid base value: {}", base_part))?,
                )
            };

            return Ok(CronField::Step { base, step });
        }

        if let Some(dash_idx) = field.find('-') {
            let (start_str, end_str) = field.split_at(dash_idx);
            let end_str = &end_str[1..]; // Skip the '-'

            let start = start_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid range start: {}", start_str))?;
            let end = end_str
                .parse::<u32>()
                .map_err(|_| format!("Invalid range end: {}", end_str))?;

            return Ok(CronField::Range { start, end });
        }

        if field.contains(',') {
            let values = field
                .split(',')
                .map(|v| {
                    v.parse::<u32>()
                        .map_err(|_| format!("Invalid list value: {}", v))
                })
                .collect::<Result<Vec<u32>, String>>()?;

            return Ok(CronField::List(values));
        }

        let value = field
            .parse::<u32>()
            .map_err(|_| format!("Invalid cron field value: {}", field))?;

        Ok(CronField::Value(value))
    }
}

/// A cron expression with validation.
#[derive(Debug, Clone)]
pub struct CronSchedule {
    /// The raw cron expression (e.g., "0 2 * * *").
    pub expression: String,
    /// Parsed minute field (0-59)
    minute: CronField,
    /// Parsed hour field (0-23)
    hour: CronField,
    /// Parsed day-of-month field (1-31)
    day: CronField,
    /// Parsed month field (1-12)
    month: CronField,
    /// Parsed day-of-week field (0-6, 0=Sunday)
    weekday: CronField,
}

impl CronSchedule {
    /// Parse and validate a cron expression.
    ///
    /// # Errors
    ///
    /// Returns an error if the cron expression is invalid.
    pub fn parse(expression: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expression.split_whitespace().collect();

        if parts.len() != 5 {
            return Err(format!(
                "Cron expression must have 5 fields, got {}",
                parts.len()
            ));
        }

        let minute = CronField::parse(parts[0])?;
        let hour = CronField::parse(parts[1])?;
        let day = CronField::parse(parts[2])?;
        let month = CronField::parse(parts[3])?;
        let weekday = CronField::parse(parts[4])?;

        Ok(CronSchedule {
            expression: expression.to_string(),
            minute,
            hour,
            day,
            month,
            weekday,
        })
    }

    /// Check if a given datetime matches this cron schedule.
    pub fn matches(&self, datetime: &chrono::DateTime<chrono::Utc>) -> bool {
        let minute = datetime.minute();
        let hour = datetime.hour();
        let day = datetime.day();
        let month = datetime.month();
        let weekday = datetime.weekday().number_from_sunday();

        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day.matches(day)
            && self.month.matches(month)
            && self.weekday.matches(weekday)
    }
}

/// Execution state for a cron trigger (tracks last execution to prevent duplicates).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CronExecutionState {
    /// Last time this trigger was executed.
    pub last_executed: Option<chrono::DateTime<chrono::Utc>>,
}

impl CronExecutionState {
    /// Create a new execution state with no prior executions.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the trigger should execute at the given time.
    ///
    /// Returns true if:
    /// - Schedule matches the time
    /// - No execution has occurred in this schedule window
    pub fn should_execute(
        &self,
        schedule: &CronSchedule,
        exec_time: &chrono::DateTime<chrono::Utc>,
    ) -> bool {
        // Schedule must match the execution time
        if !schedule.matches(exec_time) {
            return false;
        }

        // If no prior execution, always execute
        let Some(last_exec) = self.last_executed else {
            return true;
        };

        // Find the start of the matching schedule window at exec_time
        let window_start = Self::find_schedule_window(schedule, exec_time);

        // Don't execute if we already executed in this window
        last_exec >= window_start
    }

    /// Find the start of the current schedule window for a given time.
    ///
    /// For example, if schedule is `"0 2 * * *"` (2 AM daily), the window for
    /// 2024-03-15 02:30:00 starts at 2024-03-15 02:00:00.
    fn find_schedule_window(
        schedule: &CronSchedule,
        time: &chrono::DateTime<chrono::Utc>,
    ) -> chrono::DateTime<chrono::Utc> {
        // Find the most recent time that matches the schedule at or before the given time
        let mut current = *time;

        // Go back minute by minute to find the matching window
        for _ in 0..60 {
            current -= chrono::Duration::minutes(1);
            if schedule.matches(&current) {
                // Found the start of the window
                return current;
            }
        }

        // Fallback: return time as-is if we don't find it (shouldn't happen)
        *time
    }

    /// Record an execution at the given time.
    #[allow(clippy::missing_const_for_fn)] // Reason: takes &mut self, cannot be const
    pub fn record_execution(&mut self, time: chrono::DateTime<chrono::Utc>) {
        self.last_executed = Some(time);
    }

    /// Find all missed execution times between two timestamps.
    ///
    /// Finds all times when the schedule would have executed after `since` and before `until`.
    pub fn find_missed_executions(
        &self,
        schedule: &CronSchedule,
        since: &chrono::DateTime<chrono::Utc>,
        until: &chrono::DateTime<chrono::Utc>,
    ) -> Vec<chrono::DateTime<chrono::Utc>> {
        let mut missed = Vec::new();
        // Start scanning from the minute AFTER since
        let mut current = *since + chrono::Duration::minutes(1);

        // Scan minute by minute (more fine-grained to catch all matches)
        while current < *until {
            if schedule.matches(&current) {
                missed.push(current);
            }

            // Always advance to next minute
            current += chrono::Duration::minutes(1);
        }

        missed
    }
}


/// A trigger that fires on a cron schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTrigger {
    /// Name of the function to invoke.
    pub function_name: String,
    /// Cron expression (e.g., `"0 2 * * *"`).
    pub schedule: String,
    /// Timezone for schedule evaluation (e.g., `"UTC"`, `"America/New_York"`).
    pub timezone: String,
}

impl CronTrigger {
    /// Build an `EventPayload` from a cron execution.
    pub fn build_payload(
        &self,
        exec_time: &chrono::DateTime<chrono::Utc>,
    ) -> EventPayload {
        let trigger_type = format!("cron:{}", self.function_name);

        let data = serde_json::json!({
            "schedule": self.schedule,
            "timezone": self.timezone,
            "executed_at": exec_time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });

        EventPayload {
            trigger_type,
            entity: "cron".to_string(),
            event_kind: "scheduled".to_string(),
            data,
            timestamp: chrono::Utc::now(),
        }
    }
}

// ── CronScheduler ─────────────────────────────────────────────────────────────

/// Service that drives scheduled function execution.
///
/// Each registered [`CronTrigger`] is checked once per minute. When a trigger's
/// schedule matches the current time and it has not already fired in this
/// scheduling window, the function is invoked via the provided
/// [`FunctionObserver`].
///
/// # Lifecycle
///
/// 1. Build a scheduler with [`CronScheduler::new`].
/// 2. Call [`CronScheduler::start`] to spawn the background task; this returns
///    a [`CronSchedulerHandle`] that can be used to stop the task.
/// 3. On server shutdown, drop (or explicitly call `stop()` on) the handle.
pub struct CronScheduler {
    /// Triggers paired with their per-trigger execution state.
    triggers: Vec<(CronTrigger, CronExecutionState)>,
}

impl CronScheduler {
    /// Create a new scheduler for the given cron triggers.
    ///
    /// Each trigger is paired with a fresh [`CronExecutionState`] (no prior
    /// executions recorded). Call [`Self::start`] to begin scheduling.
    #[must_use]
    pub fn new(triggers: Vec<CronTrigger>) -> Self {
        let triggers = triggers
            .into_iter()
            .map(|t| (t, CronExecutionState::new()))
            .collect();
        Self { triggers }
    }

    /// Returns the number of cron triggers registered.
    #[must_use]
    pub const fn trigger_count(&self) -> usize {
        self.triggers.len()
    }

    /// Start the scheduler as a background tokio task.
    ///
    /// Spawns a task that ticks once per minute. On each tick, triggers whose
    /// schedule matches the current time and have not fired in the current
    /// window are dispatched via `observer.invoke()` (fire-and-forget).
    ///
    /// Returns a [`CronSchedulerHandle`] — drop it (or call `stop()`) to
    /// cancel the background task.
    ///
    /// # Panics
    ///
    /// Panics if called outside a tokio runtime context.
    #[must_use]
    pub fn start(
        self,
        observer: std::sync::Arc<crate::observer::FunctionObserver>,
        module_registry: std::collections::HashMap<String, crate::types::FunctionModule>,
    ) -> CronSchedulerHandle {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(cron_scheduler_task(self, observer, module_registry, shutdown_rx));
        CronSchedulerHandle {
            shutdown_tx: Some(shutdown_tx),
        }
    }
}

/// Inner async loop for the cron scheduler.
async fn cron_scheduler_task(
    mut scheduler: CronScheduler,
    observer: std::sync::Arc<crate::observer::FunctionObserver>,
    module_registry: std::collections::HashMap<String, crate::types::FunctionModule>,
    mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let mut interval =
        tokio::time::interval(tokio::time::Duration::from_secs(60));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the initial immediate tick so the scheduler doesn't fire on startup.
    interval.tick().await;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now = chrono::Utc::now();
                for (trigger, state) in &mut scheduler.triggers {
                    let schedule = match CronSchedule::parse(&trigger.schedule) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!(
                                function = %trigger.function_name,
                                expression = %trigger.schedule,
                                error = %e,
                                "Invalid cron expression — skipping trigger"
                            );
                            continue;
                        }
                    };
                    if !state.should_execute(&schedule, &now) {
                        continue;
                    }
                    state.record_execution(now);

                    // Look up the function module; log and skip if not found.
                    let Some(module) = module_registry.get(&trigger.function_name) else {
                        tracing::warn!(
                            function = %trigger.function_name,
                            "Cron trigger fired but function module not found — skipping"
                        );
                        continue;
                    };

                    let payload = trigger.build_payload(&now);
                    let observer_clone = std::sync::Arc::clone(&observer);
                    let module_clone = module.clone();
                    let fn_name = trigger.function_name.clone();

                    // Invoke fire-and-forget; failures are logged but don't stop the scheduler.
                    tokio::spawn(async move {
                        let host = crate::host::NoopHostContext::new(payload.clone());
                        match observer_clone
                            .invoke(
                                &module_clone,
                                payload,
                                &host,
                                crate::types::ResourceLimits::default(),
                            )
                            .await
                        {
                            Ok(_) => {
                                tracing::debug!(function = %fn_name, "Cron function completed");
                            }
                            Err(e) => {
                                tracing::error!(
                                    function = %fn_name,
                                    error = %e,
                                    "Cron function invocation failed"
                                );
                            }
                        }
                    });
                }
            }
            _ = &mut shutdown_rx => {
                tracing::debug!("Cron scheduler received shutdown signal — stopping");
                break;
            }
        }
    }
}

/// Handle for a running [`CronScheduler`] background task.
///
/// Drop this handle (or call [`stop`][Self::stop]) to cancel the scheduler.
pub struct CronSchedulerHandle {
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl CronSchedulerHandle {
    /// Stop the cron scheduler.
    ///
    /// Sends a shutdown signal to the background task. The task stops at its
    /// next scheduling boundary.
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for CronSchedulerHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}
