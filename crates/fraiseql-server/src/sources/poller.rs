//! The poller of one source (#573).
//!
//! A source is a Model B (Deno) connector fired on a schedule by a
//! [`ScheduledPoller`], the loop `cron:` functions run on too. Its host is bound to the
//! source's durable cursor and its `run_as` query executor, so the connector can read and
//! advance its watermark and write through `fraiseql_query`.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use fraiseql_functions::{
    EventPayload, FunctionModule, FunctionObserver, ResourceLimits,
    host::live::{HostContextConfig, QueryExecutor},
    triggers::{CronExecutionState, Schedule},
};
use fraiseql_observers::{LeaseGuardedRunner, PostgresSourceCursorStore};

use crate::cron::{ScheduledKind, ScheduledPoller};

impl ScheduledPoller {
    /// The poller of one source. `runner` is leased on the source name, and `executor`
    /// and `identity` are the source's `run_as`. `cursor_name` is the cursor row the
    /// connector advances: the declared `cursor`, or the source name.
    // Reason: a constructor wiring a source's fixed set of runtime collaborators; a
    // params struct would relocate the same fields without reducing coupling.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn source(
        source_name: impl Into<String>,
        cursor_name: impl Into<String>,
        schedule: Schedule,
        module: FunctionModule,
        observer: Arc<FunctionObserver>,
        cursor_store: PostgresSourceCursorStore,
        executor: Arc<dyn QueryExecutor>,
        identity: fraiseql_core::security::SecurityContext,
        runner: LeaseGuardedRunner,
        host_config: HostContextConfig,
        limits: ResourceLimits,
        idempotency_key: Option<Arc<[u8]>>,
        log_payloads: bool,
    ) -> Self {
        Self {
            name: source_name.into(),
            kind: ScheduledKind::Source {
                cursor_name: cursor_name.into(),
                cursor_store,
                log_payloads,
            },
            schedule,
            module,
            observer,
            executor,
            identity,
            runner,
            host_config,
            limits,
            idempotency_key,
            // The durable cursor is the resume point, so a restart needs no window state.
            state: CronExecutionState::new(),
        }
    }
}

/// Build the event payload for one source firing. The connector reads its trigger
/// context from `ctx.eventPayload`; the durable cursor (`ctx.cursor`) carries the
/// resume point, not this payload.
pub fn build_source_payload(
    source_name: &str,
    schedule: &str,
    timezone: &str,
    now: DateTime<Utc>,
) -> EventPayload {
    EventPayload {
        trigger_type: format!("source:{source_name}"),
        entity:       "source".to_string(),
        event_kind:   "scheduled".to_string(),
        data:         serde_json::json!({
            "source": source_name,
            "schedule": schedule,
            "timezone": timezone,
            "scheduled_at": now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        }),
        timestamp:    now,
    }
}

#[cfg(test)]
mod tests;
