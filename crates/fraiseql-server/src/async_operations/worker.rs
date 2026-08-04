//! The background execution loop for durable operations (#391).

use std::{sync::Arc, time::Duration};

use fraiseql_core::{db::traits::DatabaseAdapter, security::SecurityContext};
use serde_json::Value;
use tokio::time::MissedTickBehavior;
use tracing::{debug, warn};

use super::{AsyncOperationsRuntime, store::ClaimedOperation};
use crate::routes::graphql::{AppState, tenant_dispatch};

/// Run one worker loop until the task is aborted (graceful shutdown).
///
/// Each tick claims up to one operation (per worker — parallelism comes from
/// the worker count) and executes it. Also opportunistically sweeps expired
/// terminal rows.
pub async fn run<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    runtime: AsyncOperationsRuntime,
    state: AppState<A>,
) {
    let mut ticker = tokio::time::interval(Duration::from_millis(runtime.config.poll_interval_ms));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;

        let claimed = match runtime.store.claim(1, runtime.config.stuck_threshold_secs).await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "async-operations: claim failed");
                continue;
            },
        };
        for op in claimed {
            execute_claimed(&runtime, &state, op).await;
        }

        match runtime.store.sweep_finished(runtime.config.result_ttl_secs).await {
            Ok(0) => {},
            Ok(n) => debug!(swept = n, "async-operations: swept expired finished operations"),
            Err(e) => warn!(error = %e, "async-operations: sweep failed"),
        }
    }
}

/// Execute one claimed operation, heartbeating while it runs.
async fn execute_claimed<A: DatabaseAdapter + Clone + Send + Sync + 'static>(
    runtime: &AsyncOperationsRuntime,
    state: &AppState<A>,
    claimed: ClaimedOperation,
) {
    let ClaimedOperation { op, claim_token } = claimed;
    let store = Arc::clone(&runtime.store);

    // Honour a cancellation requested before execution began: the claim moved
    // the row to `running`, but nothing has run — this is the last safe point,
    // and cancelling here is truthful (#746).
    if op.cancellation_requested {
        match store.cancel_unstarted(op.op_id, claim_token).await {
            // Cancelled — or the claim was superseded and it is not ours to touch.
            Ok(_) => return,
            Err(e) => {
                warn!(error = %e, op_id = %op.op_id, "async-operations: cancel-unstarted failed");
                return;
            },
        }
    }

    // The snapshot must still be a valid principal: executing with an expired
    // context would be a write no live credential authorizes. Refuse loudly —
    // an error the submitter can read beats a silent grant.
    let ctx: SecurityContext =
        match serde_json::from_value(op.security_context.clone()) {
            Ok(ctx) => ctx,
            Err(e) => {
                record_failure(&store, op.op_id, claim_token, &format!(
                "stored security context no longer deserializes ({e}) — refusing to execute \
                 with an unverifiable principal"
            ), None)
            .await;
                return;
            },
        };
    if ctx.is_expired() {
        record_failure(
            &store,
            op.op_id,
            claim_token,
            "the submitter's security context expired before execution started — resubmit with \
             a live credential",
            None,
        )
        .await;
        return;
    }

    // Dispatch through the SAME tenant seam as /graphql and MCP (#858): the
    // tenant key persisted at submission picks the executor, so recovery can
    // never replay an operation against the wrong database (P19 mode 5).
    let dispatch = match tenant_dispatch::dispatch_to_tenant(state, op.tenant_key.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            record_failure(&store, op.op_id, claim_token, &format!("tenant dispatch: {e}"), None)
                .await;
            return;
        },
    };

    // Heartbeat while the execution runs, so a live long execution is never
    // mistaken for an abandoned one (P19: stuck means STALE).
    let hb_store = Arc::clone(&store);
    let hb_interval = Duration::from_secs((runtime.config.stuck_threshold_secs / 3).max(1));
    let op_id = op.op_id;
    let execution =
        dispatch
            .executor
            .execute_with_security(&op.document, op.variables.as_ref(), &ctx);
    tokio::pin!(execution);
    let mut hb_ticker = tokio::time::interval(hb_interval);
    hb_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    hb_ticker.tick().await; // consume the immediate tick
    let exec_result = loop {
        tokio::select! {
            result = &mut execution => break result,
            _ = hb_ticker.tick() => {
                match hb_store.heartbeat(op_id, claim_token).await {
                    Ok(true) => {},
                    Ok(false) => {
                        // The claim was superseded (we stalled past the
                        // threshold and another worker took over). Keep
                        // executing nothing further — and crucially, do not
                        // write any outcome: the guards below would no-op
                        // anyway, but stopping here avoids double work.
                        warn!(op_id = %op_id, "async-operations: claim lost mid-execution");
                        return;
                    },
                    Err(e) => warn!(error = %e, "async-operations: heartbeat failed"),
                }
            }
        }
    };

    match exec_result {
        Ok(result) => {
            // GraphQL's 200-plus-errors convention does not survive fire-and-poll:
            // a poller reads `status`, so an errored envelope must be `failed`
            // (with the envelope preserved), never a "succeeded" with a surprise
            // inside.
            let has_errors =
                result.get("errors").and_then(Value::as_array).is_some_and(|e| !e.is_empty());
            if has_errors {
                let rendered = result["errors"].to_string();
                record_failure(&store, op_id, claim_token, &rendered, Some(&result)).await;
            } else {
                match store.complete(op_id, claim_token, &result).await {
                    Ok(true) => {},
                    Ok(false) => {
                        warn!(op_id = %op_id, "async-operations: completion superseded (claim lost)");
                    },
                    Err(e) => {
                        warn!(error = %e, op_id = %op_id, "async-operations: complete failed");
                    },
                }
            }
        },
        Err(e) => {
            record_failure(&store, op_id, claim_token, &e.to_string(), None).await;
        },
    }
}

async fn record_failure(
    store: &super::AsyncOperationStore,
    op_id: uuid::Uuid,
    claim_token: uuid::Uuid,
    error: &str,
    partial: Option<&Value>,
) {
    match store.fail(op_id, claim_token, error, partial).await {
        Ok(true) => {},
        Ok(false) => warn!(op_id = %op_id, "async-operations: failure record superseded"),
        Err(e) => warn!(error = %e, op_id = %op_id, "async-operations: fail-record failed"),
    }
}
