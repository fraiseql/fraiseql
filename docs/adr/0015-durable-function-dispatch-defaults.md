# ADR-0015: Durable Function Dispatch — Retry/DLQ Defaults

## Status: Accepted

This ADR records the default-policy decision for **after:mutation function-trigger
dispatch**: whether a dispatched function is retried and dead-lettered by default, or
left fire-and-forget by default. It also records the mechanism (a shared dispatch
policy + dead-letter queue reused from the observer subsystem) and the error
classification used to decide what is retried.

Related: ADR-0010 (async mutation handlers), ADR-0012 (async-trait retention),
`docs/architecture/functions.md`, the native-runtime migration.

---

## Context

FraiseQL runs in-process serverless functions triggered by events. An
`after:mutation` function fires once a mutation has committed and runs on a live,
I/O-capable host context, so it can call external services (send email, call a
payments API such as Qonto, score a deal with an LLM).

Before this change, `after:mutation` dispatch was **fire-and-forget**: each function
was spawned as a detached task and any error was only `tracing::error!`-logged. That is
acceptable for best-effort work but not for money- or send-path work: a transient
network blip silently drops the invocation with no retry and no record.

The observer subsystem already solved the equivalent problem for its actions
(webhooks, Slack, email): a retry loop with backoff + jitter, a transient/permanent
error split, and a dead-letter queue (DLQ) with a size cap. The durable-dispatch work
in the native-runtime migration gives function dispatch the same durability by
**reusing that machinery** rather than building a parallel one.

## The decision

### 1. Durable by default; `re_runnable` is the opt-out

After-mutation dispatch is **durable by default**:

- a transient failure is retried with backoff up to `max_attempts`;
- on exhaustion (or a permanent failure) the invocation is pushed to a DLQ, where it
  is inspectable and replayable.

A function can opt **out** into fire-and-forget dispatch by setting
`re_runnable = true` in its definition. Re-runnable dispatch makes a single attempt and
logs (but never retries or dead-letters) a failure.

### 2. Why durable-by-default, not fire-and-forget-by-default

The two candidates were:

- **Off by default (explicit):** dispatch stays fire-and-forget unless the author opts
  into durability. Safe against surprising retries of non-idempotent side effects, but
  the failure mode is **silent loss** — the author must remember to opt in on exactly
  the paths that matter (payments, email), and forgetting is invisible until money or a
  message goes missing.
- **On by default (durable):** dispatch retries + dead-letters unless the author opts
  out. The failure mode is a **retry** (bounded, transient-only) or a **DLQ row** — both
  loud and recoverable.

We chose **on by default**. The whole reason durable dispatch exists is that silent loss on the
money/send path is the worst outcome; a bounded retry or an inspectable DLQ row is
strictly better than a dropped invocation. The author opts a function *out* only when a
dropped invocation is genuinely acceptable because the work can simply be re-run later —
which is exactly what `re_runnable` names (e.g. LLM scoring, which is idempotent and
re-derivable). Making the *safe* behaviour the default and the *lossy* behaviour the
explicit choice matches the rest of the platform's fail-loud posture.

### 3. Mitigating retry of non-idempotent side effects

The honest objection to durable-by-default is that retrying a non-idempotent side
effect (e.g. "send email") can double-send. Three things bound that risk:

- **Transient-only retry.** Only transient failures are retried. A `4xx`-class error
  (`FraiseQLError::is_client_error()` — a malformed request the upstream rejected) is
  treated as **permanent** and dead-lettered immediately without retry. `5xx`,
  timeouts, connection failures, and execution errors are transient and retried. This
  mirrors the observer subsystem, which treats its own `ActionExecutionFailed` as
  transient.
- **Bounded attempts + backoff.** Retries are capped (`max_attempts`, default 3) with
  exponential backoff + jitter, so a persistently-failing endpoint is not hammered.
- **Author control.** Idempotency of the *effect* remains the function author's
  responsibility (e.g. an idempotency key on the payments call); genuinely
  un-retryable, drop-on-failure work is marked `re_runnable`.

### 4. Reuse, not a parallel implementation

Durability is built from the observer crate's pieces:

- **`DispatchPolicy`** (`fraiseql-observers`) bundles the retry config and failure
  policy. Backoff timing lives on `RetryConfig::backoff_delay`, the single source of
  truth that `ObserverExecutor::calculate_backoff` also delegates to, so retries age
  identically in both subsystems.
- **`run_with_retry`** is a runtime-agnostic retry driver shared by the function
  dispatcher.
- **The `DeadLetterQueue` trait** is extended (default methods `push_function` /
  `get_pending_functions`) rather than duplicated — no new `#[async_trait]` (ADR-0012
  ratchet unchanged). Function failures land in the same store as observer-action
  failures, discriminated by a `DispatchSource` field (`AfterMutation`; the enum is
  `#[non_exhaustive]` for the future `after:ingest` source), under the same size-cap /
  drop-newest / overflow policy. No new table.

## Consequences

- Running `after:mutation` functions (`functions-runtime`) now compiles the observer
  subsystem (`observers`), because the DLQ store is reused from it. This is a compile-
  time coupling only — `fraiseql-functions` already depended on `fraiseql-observers`
  transitively; the observer *runtime* is mounted only when separately wired.
- The DLQ used by function dispatch is currently the in-memory `InMemoryDlq` (parity
  with the observer subsystem's own store, which is also in-memory). A durable
  PostgreSQL-backed DLQ — surviving restarts — is deliberately out of scope for this
  phase and tracked as follow-up; the `DeadLetterQueue` trait is the seam that will
  accept it without touching the dispatch path.
- Per-function retry policy and the `re_runnable` flag round-trip from the compiled
  schema (`FunctionDefinition`), with environment-variable overrides for the DLQ size
  cap and default retry knobs.
