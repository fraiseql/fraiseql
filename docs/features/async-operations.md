# Async operations: durable submit / status / cancel

Some operations cannot finish inside a request timeout — large exports, batch
mutations, heavy retrievals. The `[async_operations]` subsystem gives them the
standard cloud fire-and-poll shape with durable state: submission returns an
`op_id` immediately, background workers execute **through the same pipeline as
`/graphql`** (RLS, session variables, cost gates, the change-log outbox — never
a second execution path), and status is read back from the stored row.

## Configuration

```toml
[async_operations]
operations = ["largeExport", "rebuildIndex"]  # required, non-empty allowlist
workers = 2
poll_interval_ms = 500
stuck_threshold_secs = 300
max_attempts = 1        # no automatic retry unless the operator opts in
result_ttl_secs = 86400
```

Presence mounts the surface and starts the workers; absence leaves both off.
The section is strict, the allowlist is fail-closed (an operation added to the
schema later is not silently submittable), and a configured section without a
database pool — or whose `_system.async_operations` table cannot be
initialised — **refuses to boot**.

## API

All three endpoints sit behind the deployment's configured auth layer (OIDC or
HS256, like every data-serving transport) and additionally hard-require an
authenticated caller — the submission snapshots the caller's security context
for the background execution, so an anonymous submission has nothing to
execute as.

```text
POST   /operations/v1/{operation}   body: {"query": "...", "variables": {...}}
       → 202 {"op_id": "...", "status": "queued"}
GET    /operations/v1/{op_id}
       → 200 {"op_id", "status", "attempts", "result"?, "error"?, ...}
DELETE /operations/v1/{op_id}
       → 200 {"status": "cancelled"}          (it was still queued — cancelled outright)
       → 202 {"status": "cancel_requested"}   (it is running — honoured at the next safe point)
       → 409                                  (already terminal)
```

- The document's root field must equal the `{operation}` path segment and be on
  the allowlist; subscriptions are refused (nothing to poll).
- `Idempotency-Key` on submission deduplicates: the same key with the same body
  replays the same `op_id`; a different body under the same key is a 422.
- Operation cost is charged against the tenant budget **at submission** — the
  queue is not a budget bypass.
- Status and cancel are scoped to the submitting principal; another caller's
  `op_id` reads as 404 (no existence oracle).
- A GraphQL envelope with `errors` is recorded as `failed` (envelope preserved
  in `result`) — a poller reads `status`, so an errored envelope must never be
  a "succeeded" with a surprise inside.

## Execution guarantees

Designed against the saga-recovery failure modes the remediation program fixed
in P19, each pinned by a test:

- **No double execution on recovery.** Terminal operations are never
  reclaimable; the state machine lives in the store's conditional UPDATEs.
- **Staleness-gated claiming.** Workers heartbeat while executing; a `running`
  row is only reclaimed when its heartbeat exceeds `stuck_threshold_secs` — a
  live long execution is never stolen, and a dead worker's operation recovers.
- **Claim-guarded completion.** Every attempt carries a claim token; a
  superseded worker's late result is a no-op, never a clobber.
- **Truthful cancellation.** A cancel that did not cancel is never reported as
  one: queued → cancelled outright; running → an explicit `cancel_requested`
  whose outcome you observe by polling.
- **Right-database execution.** The resolved tenant key is persisted at
  submission and execution dispatches through the same tenant seam as
  `/graphql` and MCP.
- **Real status.** `GET` reads the stored row — nothing is inferred.

The submitter's security context is validated again at execution time: if the
snapshot has expired before the worker picks the operation up, the operation
fails loudly ("resubmit with a live credential") rather than executing with an
unverifiable principal.
