//! The function-dispatch ledger's server side (#1628): what makes `after:mutation` and
//! `after:capture` dispatch at-least-once.
//!
//! A write that triggers a durable function records one `pending` row per function in its
//! own transaction (see [`DispatchLedger`](fraiseql_core::db::traits::DispatchLedger)); a
//! captured change records its rows before the change-log reader moves past it. When a
//! dispatch ends, `settle` deletes its row, or keeps it `dead_lettered` with its reason. A
//! process that dies first leaves the row `pending`: once its lease runs out, the sweep
//! ([`run_sweeper`]: once at startup, then on an interval)
//! claims it on the executor that recorded it and runs it again, with the token it was recorded
//! with.
//!
//! So a dispatch runs at least once, and can run twice (a lease that expires while the first
//! run is still in flight). Handlers must be idempotent: each run hands them the same token.

use std::{sync::Arc, time::Duration};

use fraiseql_core::{
    db::{DispatchSettlement, DueDispatch},
    runtime::Executor,
    security::{Provenance, SecurityContext},
};

use crate::{
    query_bridge::ExecutorSource,
    routes::after_mutation::{
        DispatchOutcome, QueryExecutorFactory, make_query_executor_factory, replay_recorded,
    },
    subsystems::BeforeMutationHooks,
};

/// The ledger's `source` for an `after:mutation` dispatch.
pub const AFTER_MUTATION: &str = "after:mutation";
/// The ledger's `source` for an `after:capture` dispatch.
pub const AFTER_CAPTURE: &str = "after:capture";

/// The reason a dispatch that exhausted its retries is kept under.
const EXHAUSTED: &str = "exhausted its retries; its record is in the dead-letter queue";

/// How the ledger leases, sweeps and batches (`[function_dispatch]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedgerSettings {
    /// How long a dispatch may run before the sweep takes it over. Longer than a
    /// dispatch's own retries normally take, or a slow dispatch runs twice.
    pub lease:          Duration,
    /// How often the sweep looks for dispatches whose lease ran out.
    pub sweep_interval: Duration,
    /// How many due dispatches one sweep claims, per executor.
    pub batch_size:     u32,
}

impl Default for LedgerSettings {
    fn default() -> Self {
        Self {
            lease:          Duration::from_mins(5),
            sweep_interval: Duration::from_secs(30),
            batch_size:     100,
        }
    }
}

/// What a dispatch's row becomes: `None` leaves it `pending` for the sweep.
fn settlement_of(outcome: DispatchOutcome) -> Option<DispatchSettlement> {
    match outcome {
        DispatchOutcome::Dispatched => Some(DispatchSettlement::Dispatched),
        DispatchOutcome::DeadLettered => Some(DispatchSettlement::DeadLettered(EXHAUSTED.into())),
        DispatchOutcome::Unsettled => None,
    }
}

/// Settle the recorded dispatch of `function_name` for `(source, ref_key)` on `executor`,
/// per `outcome`. A failed settle is logged: the row stays `pending` and runs again.
pub(crate) async fn settle(
    executor: &Executor,
    source: &str,
    ref_key: &str,
    function_name: &str,
    outcome: DispatchOutcome,
) {
    if let Some(settlement) = settlement_of(outcome) {
        settle_as(executor, source, ref_key, function_name, &settlement).await;
    }
}

async fn settle_as(
    executor: &Executor,
    source: &str,
    ref_key: &str,
    function_name: &str,
    settlement: &DispatchSettlement,
) {
    if let Err(error) = executor.settle_dispatch(source, ref_key, function_name, settlement).await {
        tracing::warn!(
            %error,
            function = %function_name,
            "function dispatch ran but could not be settled; the sweep will run it again"
        );
    }
}

/// The caller a dispatch is recorded with (#1628): the principal, and for a service account
/// a fingerprint of the secret it was configured with.
///
/// Both fields are required, `credential` included (it is `null` for any other principal): a
/// row missing either reads as an unknown caller and is not acted for.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StoredCaller {
    principal:  SecurityContext,
    /// For a service account, a fingerprint of the secret it is configured with, taken from
    /// the authenticator that admitted the request, in the same request, before its write
    /// commits. That authenticator is built once at boot from the secret's environment
    /// variable and a schema reload never replaces it (`security` is boot-frozen), so the
    /// secret it fingerprints is the one the request presented: a rotation only takes effect
    /// on a restart, which is exactly what a replay is compared across.
    #[serde(deserialize_with = "Option::deserialize")]
    credential: Option<String>,
}

/// What a dispatch records about the principal whose write triggered it.
pub(crate) fn stored_caller(
    principal: &SecurityContext,
    service_accounts: Option<&crate::service_account::ServiceAccountAuthenticator>,
) -> Option<serde_json::Value> {
    let credential = match principal.provenance() {
        Some(Provenance::ServiceAccount) => {
            service_accounts.and_then(|accounts| accounts.credential_fingerprint(principal))
        },
        _ => None,
    };
    serde_json::to_value(StoredCaller {
        principal: principal.clone(),
        credential,
    })
    .ok()
}

/// What a replay re-verifies a stored caller against: the request path's revocation check,
/// and the service accounts configured now.
#[derive(Clone, Copy, Default)]
pub(crate) struct CallerChecks<'a> {
    /// The request path's revocation manager, when the server checks revocation.
    pub revocation:       Option<&'a crate::token_revocation::TokenRevocationManager>,
    /// The service accounts configured now.
    pub service_accounts: Option<&'a crate::service_account::ServiceAccountAuthenticator>,
}

/// The principal a recorded dispatch acts for when it runs again: the caller stored with it,
/// only while that caller could still act, by the rule of the authenticator it came from
/// (D1, "caller, bounded").
///
/// - A bearer token: not expired at `now`, and it passes the request path's revocation check
///   (`check_token`, single-token and revoke-all) with the `jti` and `iat` it carried.
/// - A service account: still configured, with the same ceiling and the same secret.
/// - Any other authenticator (an API key, a `run_as` job, an impersonation preview, a Flight
///   session), or none recorded: not re-verifiable here, so not acted for.
///
/// No stored caller is `Ok(None)`: the dispatch runs as the function's `run_as` identity.
///
/// # Errors
///
/// Why the caller is not acted for, naming its authenticator: the row is dead-lettered with
/// it.
pub(crate) async fn replay_caller(
    caller: Option<&serde_json::Value>,
    checks: CallerChecks<'_>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Option<SecurityContext>, String> {
    let Some(caller) = caller else {
        return Ok(None);
    };
    let StoredCaller {
        principal,
        credential,
    } = serde_json::from_value(caller.clone())
        .map_err(|_| "the recorded caller is not in a form this server reads".to_string())?;
    match principal.provenance() {
        Some(Provenance::BearerToken) => {
            if principal.expires_at <= now {
                return Err(format!(
                    "the caller's bearer token expired at {}",
                    principal.expires_at
                ));
            }
            if let Some(revocation) = checks.revocation {
                let jti = principal.attributes.get("jti").and_then(serde_json::Value::as_str);
                let iat = principal.attributes.get("iat").and_then(serde_json::Value::as_i64);
                revocation.check_token(jti, principal.user_id.as_str(), iat).await.map_err(
                    |_| {
                        "the caller's bearer token no longer passes the revocation check"
                            .to_string()
                    },
                )?;
            }
        },
        Some(Provenance::ServiceAccount) => {
            checks
                .service_accounts
                .ok_or_else(|| "no service accounts are configured now".to_string())?
                .still_grants(&principal, credential.as_deref())?;
        },
        Some(other) => {
            return Err(format!(
                "the caller was authenticated by {}, which is not re-verified to act again",
                other.describe()
            ));
        },
        None => return Err("the caller's authenticator is not recorded".to_string()),
    }
    Ok(Some(principal))
}

/// Every executor a dispatch may have been recorded on, with its tenant: the serving one,
/// and each active tenant's. A suspended tenant's dispatches wait for it to resume.
fn sweep_executors(source: &ExecutorSource) -> Vec<(Option<String>, Arc<Executor>)> {
    if let Some(serving) = source.serving() {
        return vec![(None, serving.load_full())];
    }
    let Some(registry) = source.registry() else {
        return Vec::new();
    };
    let mut executors = vec![(None, Arc::clone(&*registry.default_executor()))];
    for key in registry.tenant_keys() {
        if let Ok(executor) = registry.executor_for(Some(&key)) {
            executors.push((Some(key), Arc::clone(&*executor)));
        }
    }
    executors
}

/// Claim `tenant`'s due dispatches on `executor`, run each, and settle it. Returns how many
/// were claimed.
///
/// # Errors
///
/// When the claim fails.
pub(crate) async fn sweep_executor(
    executor: &Executor,
    tenant: Option<&str>,
    replay: &Replay,
    query_executor_factory: Option<&QueryExecutorFactory>,
    settings: LedgerSettings,
) -> fraiseql_error::Result<usize> {
    let due = executor
        .claim_due_dispatches(tenant, settings.batch_size, settings.lease)
        .await?;
    let claimed = due.len();
    futures::future::join_all(due.iter().map(|dispatch| async move {
        replay_one(executor, tenant, replay, query_executor_factory.cloned(), dispatch).await;
    }))
    .await;
    Ok(claimed)
}

/// What a replay needs besides the row: the function hooks, and the revocation check the
/// stored caller passes again (D1).
pub struct Replay {
    /// The server's function hooks.
    pub hooks:            Arc<BeforeMutationHooks>,
    /// The request path's revocation manager, when the server checks revocation.
    pub revocation:       Option<Arc<crate::token_revocation::TokenRevocationManager>>,
    /// The service accounts configured now.
    pub service_accounts: Option<Arc<crate::service_account::ServiceAccountAuthenticator>>,
}

impl Replay {
    pub(crate) fn caller_checks(&self) -> CallerChecks<'_> {
        CallerChecks {
            revocation:       self.revocation.as_deref(),
            service_accounts: self.service_accounts.as_deref(),
        }
    }
}

async fn replay_one(
    executor: &Executor,
    tenant: Option<&str>,
    replay: &Replay,
    query_executor_factory: Option<QueryExecutorFactory>,
    dispatch: &DueDispatch,
) {
    tracing::info!(
        source = %dispatch.source,
        function = %dispatch.function_name,
        attempts = dispatch.attempts,
        "function dispatch replayed: its lease ran out before it settled"
    );
    let (source, ref_key, function) =
        (&dispatch.source, &dispatch.ref_key, &dispatch.function_name);
    match replay_recorded(replay, query_executor_factory, dispatch, tenant).await {
        Ok(outcome) => settle(executor, source, ref_key, function, outcome).await,
        Err(reason) => {
            // Nothing left to run, and sweeping it forever would only repeat this line.
            // Terminal, and said loudly.
            tracing::warn!(
                function = %function,
                %reason,
                "function dispatch replay cannot run; recorded as dead-lettered"
            );
            settle_as(
                executor,
                source,
                ref_key,
                function,
                &DispatchSettlement::DeadLettered(reason),
            )
            .await;
        },
    }
}

/// One sweep over every executor `source` serves. Returns how many dispatches it claimed.
pub(crate) async fn sweep_once(
    source: &ExecutorSource,
    replay: &Replay,
    settings: LedgerSettings,
) -> usize {
    let factory = make_query_executor_factory(source.clone());
    let mut claimed = 0;
    for (tenant, executor) in sweep_executors(source) {
        match sweep_executor(&executor, tenant.as_deref(), replay, Some(&factory), settings).await {
            Ok(n) => claimed += n,
            Err(error) => tracing::warn!(
                %error,
                tenant = tenant.as_deref().unwrap_or(""),
                "function dispatch sweep failed; retrying next interval"
            ),
        }
    }
    claimed
}

/// Sweep once now (the startup scan), then every `settings.sweep_interval`, forever.
///
/// Run on the server's task set, so graceful shutdown stops it; a dispatch it leaves
/// unsettled is swept again after the next start.
pub async fn run_sweeper(source: ExecutorSource, replay: Replay, settings: LedgerSettings) {
    loop {
        sweep_once(&source, &replay, settings).await;
        tokio::time::sleep(settings.sweep_interval).await;
    }
}

#[cfg(test)]
mod tests;
