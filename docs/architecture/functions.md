# Functions Architecture

The `fraiseql-functions` crate provides a serverless functions runtime for
FraiseQL, enabling event-driven triggers, scheduled tasks, and custom logic
execution alongside the GraphQL engine.

## Overview

```
Mutation → Observer Event → Trigger Registry → Function Execution
                                    ↓
                              Cron Scheduler → Periodic Function Execution
```

Functions are **not** part of the hot query path. They execute asynchronously in
response to mutation events or on a cron schedule.

## Components

### Trigger Registry

The `TriggerRegistry` maps mutation events to function handlers. When a mutation
completes, the observer pipeline checks the registry for matching triggers and
dispatches execution.

- Supports `before_mutation` and `after_mutation` hooks
- `BeforeMutationChain` allows ordered execution of pre-mutation logic; it is run by the engine at
  the mutation chokepoint, not by a transport — see [Before-Mutation
  Enforcement](#before-mutation-enforcement-1327)
- Triggers are registered at server startup from the compiled schema
- `after:capture` triggers (#366) fire on **externally-captured** writes (a
  third-party daemon / `psql` INSERT) via the change-log reader — the ingress dual
  of `after:mutation`; see [external-write-capture.md](./external-write-capture.md).
- `request:query` (#1329) is the one trigger that is **not an event**: it names a
  capability, and the binding lives on the query that declares
  `function = "<name>"`. See [Answering a request from a
  function](#answering-a-request-from-a-function-1329).

### Before-Mutation Enforcement (#1327)

`before:mutation` is the synchronous hook that can rewrite a mutation's input or
abort it, so it is where a validation or business rule goes. That makes it
**enforcement**, and the contract follows from that one word:

> Every route that executes a mutation runs its `before:mutation` chain, once per
> executed root field, in document order, immediately before that root writes, with
> the arguments the write will actually run with.

It is enforced in the engine, not in a transport: the chain is consulted from
`execute_mutation_impl` (`fraiseql-core`), the single point every mutation entry path
converges on — both GraphQL branches, the direct `SupportsMutations` API the
anonymous REST write uses, and `execute_mutation_with_security` — next to
`requires_role`, `requires_actor` (#966) and the operation `Authorizer` (#422), for
the same reason those live there. A transport cannot reach a write without passing
it, so a *new* route needs no wiring and cannot forget any.

The seam is [`BeforeMutationGate`](../../crates/fraiseql-core/src/security/mutation_gate.rs),
installed on the executor's `RuntimeConfig`. `fraiseql-server` implements it with
`FunctionChainGate`, which runs the compiled schema's chain; an embedder can install
its own rule engine instead, with or without functions compiled in.

**Why it moved.** Until #1327 the chain ran in the GraphQL handler, once per HTTP
request, keyed on `parse_query(…).root_field` and handed `request.variables`. Three
request shapes executed a mutation without running its chain:

| Bypass | Why it worked |
|---|---|
| `mutation { harmless(…) { id } guarded(…) { id } }` | the handler keyed on the **first** root; since #759 the executor runs every root serially, so `guarded` wrote after only `harmless`'s chain had run |
| `guarded(input: { … })` with no variables | the chain was handed `request.variables`, so an inline literal was invisible to it, and `Proceed(modified)` rewrote only variables |
| the REST write route | it dispatched `after:mutation` only; no before-chain existed on that path |

Each of the three is pinned by a test in
`runners/mutation/tests.rs::before_mutation_enforcement`, asserting the **write**
(whether the mutation's SQL function was called), not the response envelope.

**Semantics.**

- **Keyed on the field name, never the response alias.** Two roots calling the same mutation differ
  only by alias, so keying on the alias would run one chain twice and skip the other.
- **Resolved arguments.** The chain sees request variables merged with the root field's inline
  literals, nested `$var` references already substituted — the same view the engine binds the SQL
  function's arguments from.
- **A rewrite reaches the write.** `{"input": {…}}` returned by a function replaces the argument
  view the engine binds from, and the view the field authorizer resolves against — not just the
  request variables.
- **Fail-closed.** An abort refuses the write with the rule's own message
  (`FraiseQLError::Validation`). A chain that *fails* — a missing module, a runtime error, a
  decision this build does not recognise — also refuses it; it never falls through to "proceed with
  the original input", which would run a mutation the chain declined to approve.
- **Zero overhead when unused.** A mutation with no registered trigger costs one `HashMap::get`,
  and a build with no gate installed costs an `Option` check.
- **Ordering against the static gates.** The chain runs *after* authorization, `requires_role`,
  `requires_actor`, selection-set validation (#1005) and argument-name validation (#1154), so an
  unauthorized or malformed request never reaches app-authored rule code.

In a multi-root document an aborted root is reported in `errors` under its own
response key with `data.<key>: null`, and the remaining roots still execute — the
# 759 partial-outcome contract, unchanged.

**What a hook may read, and what that makes it (#1328).** A rule that depends on
data — a credit limit, a price, a quota, the target row's current state — needs to
read. It can: `fraiseql_query` is available to a before-hook as a **read-only
bridge executed as the requesting principal**. Two words, two properties:

- **read-only** — a document the engine would execute as a write is refused by name, so the hook
  cannot become a second write path and an abort cannot leave a half-applied change behind;
- **as the caller** — not under a `run_as` ceiling, so a hook can never surface a row the caller
  could not have read itself. An anonymous write reads anonymously, which under an RLS policy means
  the read fails closed rather than being promoted.

The read runs **outside the mutation's transaction**. That is deliberate: holding a
Postgres transaction, its row locks and a pooled connection open across a V8 isolate
running user-supplied JavaScript would turn function latency into database lock
time, reachable by anyone who can author a function. It has a consequence, and the
consequence is the contract:

> `before:mutation` is **unbypassable** — every route that executes a mutation runs it
> (#1327). For anything derivable from its **input**, it is authoritative. For anything
> requiring a **read**, it is a fast, friendly rejection: the read is not in the
> mutation's transaction, so the authoritative rule must still be a constraint or the
> SQL function.

**A hook author who believes a read-backed check is authoritative has written a
check-then-act race and does not know it.** Two concurrent orders can each read a
credit limit with room left and each be approved. Use the hook for the fast, clear
error message; keep the unique index, the check constraint or the
`fraiseql.mutation_err` in the SQL function as the thing that is actually true.

**Latency budget.** The whole chain for one mutation runs inside a wall-clock
ceiling — **500 ms** by default, overridable with
`FRAISEQL_FUNCTIONS_BEFORE_MUTATION_BUDGET_MS` (`0` disables it, and the server logs
a warning at startup when it is disabled). The same ceiling is passed down as each
hook's isolate watchdog, so a runaway guest is stopped by its own runtime rather
than only waited out. An overrun **refuses the write**, with a diagnosis naming the
budget and the mutation rather than the generic hook-failure message — the one cause
an operator can act on by raising a limit.

**One deployment mode where the contract does not hold yet.** A per-tenant executor is
built by `create_tenant_executor` with `RuntimeConfig::default()`, so a tenant-keyed
request in a `[tenancy.runtime] enabled = true` deployment carries no gate — along with
no operation `Authorizer`, RLS policy or field filter, which is the same defect and is
why it is tracked on its own: **#1333**. Single-tenant deployments dispatch to the
server's own executor and are unaffected.


### Durable After-Mutation Dispatch

`after:mutation` function dispatch is **durable by default** (ADR 0015): a
transient failure is retried with backoff and, once retries are exhausted, the
invocation is dead-lettered so money- and send-path work is never silently lost.

- **Retry.** Transient failures (5xx, timeouts, execution errors) are retried up
  to `max_attempts` with exponential backoff + jitter. A `4xx` client error is
  treated as permanent and dead-lettered without retry. Backoff and the
  transient/permanent split reuse the observer subsystem via a shared
  `DispatchPolicy` (`fraiseql-observers`), so retries age identically in both.
- **Dead-letter queue.** An exhausted or permanently-failed dispatch is pushed to
  the shared dead-letter queue (`DeadLetterQueue::push_function`), tagged with a
  `DispatchSource` discriminator, under the same size-cap / drop-newest policy as
  observer-action failures. Inspect the count via `function_dlq_count` on the
  observer delivery-health endpoint.
- **Durable DLQ (#598).** The store is selectable via `[functions] dlq_store`:
  `"memory"` (the default — fast, but dead-letters vanish on restart) or
  `"postgres"` (persisted to `_fraiseql_function_dlq`, so a dead-lettered dispatch
  survives a restart and stays listable/replayable). `FRAISEQL_FUNCTIONS_DLQ_STORE`
  overrides the compiled value for production. The Postgres table is a server-owned
  operational table (no RLS; a dead-letter has no single tenant) whose `payload` can
  contain row data — treat it as sensitive: it is never logged at default level and
  is retention-bounded by `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`.
- **Fire-and-forget opt-out.** Set `re_runnable = true` on a function definition
  for re-runnable/idempotent work (e.g. LLM scoring): such dispatch stays
  fire-and-forget with no retry or dead-letter overhead.
- **Configuration.** The retry policy round-trips per-function from the compiled
  schema (`FunctionDefinition.retry`); server-level defaults are overridable via
  `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS`,
  `FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS`,
  `FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS`, and `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`.

Durable dispatch requires the `functions-runtime` feature, which enables the
`observers` subsystem (whose dead-letter-queue store is reused).

### Cron Scheduler (#595)

A `cron:` function fires from a running server: at startup the lifecycle builds one
leased `CronPoller` per cron function (server-side, `crate::cron`) and spawns it on
the server's task set. A cron function is **"a scheduled source without a cursor"** —
the poller reuses the sources' machinery:

- **Cron expressions parsed at startup**; each poller ticks once a minute.
- **Single-firing across replicas** via the sources' PostgreSQL advisory lease
  (`LeaseGuardedRunner`, keyed `cron:<function>`): N replicas → exactly one firing per
  scheduled window. This is the "leader election" the docs previously claimed but did
  not implement.
- **State persisted** to `_fraiseql_cron_state` (`last_fired_at`, `fire_count`) — a
  durable fire record + a cross-restart "already fired this window" guard.
- **Authority:** the function runs on the phase-02 I/O host, so `fraiseql_query` works
  under the function's `run_as` ceiling (fail-closed when absent) — a purge/report job
  can query and mutate.
- **Missed-tick policy: skip.** A server down over a scheduled instant does not replay
  on next boot; the next matching window fires normally (cron has no cursor/backlog to
  resume, unlike a source).

> **Design note (A vs B).** We *implemented* `cron:` (variant A) rather than retiring it
> in favor of cron-scheduled sources (variant B): the `_fraiseql_cron_state` migration
> and the sources' lease were already in place, so the delta was small and the "a
> scheduled job is just a `cron:` function" authoring story is cleaner. The requires
> a DB pool (the lease + state table). The legacy in-process `CronScheduler` firing
> path (`NoopHostContext`, no lease) is superseded by `CronPoller` for production.

### Function runtimes

A function module runs on one of two interchangeable backends, selected per
module by its `runtime` field:

| Runtime | Feature | Authoring | Notes |
|---------|---------|-----------|-------|
| **WASM** (`RuntimeType::Wasm`) | `functions-runtime` | any language → `wasm32-wasip2` component | wasmtime, component model |
| **Deno** (`RuntimeType::Deno`) | `functions-runtime-deno` | JavaScript & TypeScript | V8 isolate. `TypeScript` types are stripped to JS before execution (`deno_ast`/swc), gated by `DenoConfig.enable_typescript` (on by default). |

Both reach the same **I/O-capable host surface** through
`FunctionObserver::invoke_with_context`, which dispatches by the module's runtime.
The host surface (`HostContext`) exposes:

- `http_request` — outbound HTTP, **deny-by-default SSRF allowlist**
  (`FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`), redirects disabled, DNS-rebind checks.
- `query` (`fraiseql_query`) — execute a GraphQL query or mutation back into the
  engine, under the function's **`run_as`** ceiling (see below). Wired for
  `after:mutation` and scheduled sources; `after:ingest` is a tracked follow-up.
  A function with no `run_as` can *read* only what an anonymous principal can and
  can *write* nothing (fail-closed). **`before:mutation` is the exception**: its
  bridge is read-only and runs as the *requesting principal*, not a `run_as`
  ceiling (#1328) — see the per-trigger table below.
- `storage_get` / `storage_put` — object storage.
- `env_var` — read allowlisted secrets/config (granted via
  `FRAISEQL_FUNCTIONS_ALLOWED_ENV_VARS`, or `[sources] allowed_env_vars` /
  `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` for sources). A non-allowlisted name is a
  loud authorization error, never a silent `null`.
- `sql_query` — **not implemented**: statements are classified but never
  executed; every call fails loud. Use `query` instead.
- `auth_context` — the caller's authenticated context (RLS-aware execution).
- `log` — structured logging captured into the function result.

#### Which host calls each trigger kind gets (#1328)

The list above is the *union*. No trigger kind has all of it, and until #1328 the
docs never said which had what — `before:mutation` in particular had **none**, and
nothing wrote that down.

| host call | `before:mutation` | `request:query` | `after:mutation` / `after:capture` | `after:ingest` | `cron` / scheduled source |
|---|---|---|---|---|---|
| `fraiseql_query` | **read-only, as the caller** | **read-only, as the caller** | yes, under `run_as` | yes, under `run_as` | yes, under `run_as` |
| `fraiseql_auth_context` | the caller's; refused on an anonymous write | the caller's; refused on an anonymous request | the dispatch identity | the dispatch identity | the `run_as` identity |
| `fraiseql_log` | yes | yes | yes | yes | yes |
| the event payload | the mutation's resolved **arguments** | the field's resolved **arguments** | `{event_kind, old, new}` | the inbound message | the schedule/source context |
| `fraiseql_http_request` | **refused** | yes (SSRF allowlist) | yes (SSRF allowlist) | yes | yes |
| `fraiseql_storage_get` / `_put` | **refused** | **refused** | yes | yes | yes |
| `fraiseql_send_email` | **refused** | **refused** | yes (when wired) | yes | yes |
| `fraiseql_env_var` | **refused** | **refused** | allowlisted | allowlisted | allowlisted |
| `fraiseql_sql_query` | refused | refused | not implemented (fails loud) | not implemented | not implemented |
| `fraiseql_idempotency_token` | none (not a durable dispatch) | none (not a durable dispatch) | yes | yes | yes |
| `fraiseql_cursor_*` | no binding: `get` answers `null`, `advance` refuses | same | same | same | scheduled sources only |

`before:mutation`'s column is narrow on purpose, and the reason is one sentence: it
is the only kind that runs **synchronously on the write path**, and its side effects
are **not rolled back** when a later hook aborts the write it was deciding on. An
outbound call, an upload or an email from a hook that then refuses the write has
already happened. Those belong in `after:mutation`, which is durable, retried and
dead-lettered.

`request:query` is narrow for a *different* reason, which is why its column differs
(#1329). It decides nothing and rolls nothing back, so the write-path argument does
not apply — it gets the outbound call, and #1329's own examples (an LLM-backed
answer, a BFF-style aggregation) are outbound calls. What it must not do is **cause**
anything: it answers a read, on a path that is cached, that clients retry freely, and
that anyone who can issue the query can reach. So no store, no mail, no env.

Both narrow surfaces are types — `BeforeMutationHost` and `RequestQueryHost` — not
wiring conventions: every op is written out, and the refused ones name themselves, so
the table above is checked against code rather than maintained by hand. The outbound
op itself is one implementation (`host::outbound_http::perform`) shared by every
surface that has one, so "the SSRF allowlist, unchanged" is a fact about the binary
rather than a promise.

### Declarative `when` predicates (#597)

An `after:mutation` (or `after:capture`, #366) function can declare *when* it fires,
evaluated by the dispatcher on the row images **before** any runtime spins — a false
predicate produces no dispatch record at all (not a skipped/failed dispatch):

```jsonc
{
  "name": "notify_approved",
  "trigger": "after:mutation:Order:update",
  "when": [                                             // conjunction; omitted = always
    { "field": "status", "changed_to": "approved" },    // transition test (UPDATE-only)
    { "field": "kind",   "eq": "standard" }             // state test (INSERT + UPDATE)
  ]
}
```

- **`eq`** — the field currently equals the value, evaluated on the after-image
  (INSERT/UPDATE) or the pre-image (DELETE). A missing field never equals a value.
- **`changed_to`** — `old.field != v && new.field == v`. UPDATE-only (`changed_to` on
  a non-`update` trigger is a **load error**). A DELETE never matches.
- The list is a **conjunction** (all must hold); an empty/absent `when` always fires
  (back-compat). Exactly one operator per predicate; unknown keys are a load error.
  This is a dispatch filter, not a rules engine — anything richer stays guest code.

> **Pre-image caveat.** The after:mutation **route** path carries only the after-image
> (the mutation response), so `changed_to` there gates on `new.field == v` and cannot
> distinguish a real transition from a re-save. Full transition detection needs the
> pre-image — the `after:capture` path (backed by the change log) with `pre_image=True`.

### Function authority — `run_as` (#594)

A function's `fraiseql_query` writes run under an explicit least-privilege
**ceiling**, exactly the model scheduled sources use (see
[sources.md](./sources.md)). It is declared on the function definition in the
compiled schema:

```jsonc
{
  "name": "recordApproval",
  "trigger": "after:mutation:Order:update",
  "runtime": "Deno",
  "run_as": { "roles": ["order_writer"], "scopes": ["write:order"], "tenant": "acme" }
}
```

- **Fail-closed.** A function with **no `run_as`** runs its bridge under an
  anonymous `system_job` identity — no roles, no scopes, no tenant — so RLS and
  field-authorization deny every write until an operator grants a ceiling. Granting
  authority is a deliberate act, never a default (same words as the sources docs).
- **Audited.** A function-authored write is stamped `system_job:<function-name>`
  under `ActorType::SystemJob` in the change log — the same audit envelope a source
  write carries — so a bridge write is attributable to the function that issued it.
- **Bridge-write asymmetry (deliberate).** A write a function issues through
  `fraiseql_query` does **not** itself fire `after:mutation` functions: after-mutation
  dispatch is invoked only from the GraphQL/REST route handlers, and the bridge wraps
  the core executor, bypassing them. So a bridge-written `Order` update does **not**
  fire `notify_approved`. This is an invariant, not a race — there is no
  bridge→after:mutation loop to guard against.

From a TypeScript guest these are `Deno.core.ops.fraiseql_*` (typed via the
`FRAISEQL_HOST_TYPES` declarations); from a WASM guest they are the
`fraiseql:host/io` imports. Both share **one** `DynHostContext` bridge, so the
SSRF/validation policy is defined once, not per runtime. A host op invoked on a
path that has no live host context (the sync `invoke` path) **fails loud** rather
than silently returning empty data.

## Observability (#598)

Function dispatch is observable on `/metrics` (Prometheus facade; exported when the
server is built with the `metrics` feature, like the source and wire metrics) and in
the structured logs. Emitted per background dispatch:

| Metric | Type | Labels | Meaning |
|--------|------|--------|---------|
| `fraiseql_function_dispatches_total` | counter | `function`, `trigger_kind`, `result` | One background dispatch that ran. `trigger_kind` ∈ {`after:mutation`, `after:ingest`, `after:capture`, `cron`}; `result` ∈ {`ok`, `error`, `dead_lettered`}. A fire-and-forget (`re_runnable`) single-attempt failure is `error`; a durable dispatch that exhausted its retries is `dead_lettered`. |
| `fraiseql_function_run_duration_seconds` | histogram | `function` | Wall-clock of a dispatch that ran (all retry attempts included). |
| `fraiseql_function_predicate_skips_total` | counter | `function` | A `when` predicate (#597) evaluated false, so **no isolate spun** — the zero-cost-skip made visible. |
| `fraiseql_function_dlq_size` | gauge | — | Current function-dispatch DLQ depth (this replica's store view). |
| `fraiseql_function_dlq_evictions_total` | counter | — | Function dead-letters dropped because the DLQ was at capacity (drop-newest). |

**Trigger kinds not metered here.** `before:mutation` runs synchronously at the write
chokepoint (its outcome is the mutation's own success/failure, already on the
GraphQL/HTTP metrics); `http` triggers are **rejected at load** (no route mounting exists yet, so a
declared `http:` function would silently never serve — #871); `after:storage` has no
runtime dispatch path yet and is likewise rejected at load. None is a background
dispatch, so none is a `fraiseql_function_dispatches_total` row.

**Structured logs** — a dead-letter logs at `error` with the `function`, `attempts`,
and the per-dispatch `idempotency_token`, so an alert traces to the exact dispatch
(and the operator can dedupe a manual replay with the same token). The dead-lettered
`payload` is never logged at default level.

The full platform metric set (sources + functions) is listed in one table in
[sources.md](sources.md#observability) and here; a dashboard consuming both keys off
`fraiseql_source_*` and `fraiseql_function_*`.

## Configuration

Functions are enabled via feature flags on `fraiseql-server`:

- `functions` — edge-function HTTP endpoint + the pure `after:mutation` planner.
  The stock binary compiles only this; no runtime, no live host context.
- `functions-runtime` — actually *run* `after:mutation` functions after commit,
  on a live host context (WASM runtime + `host-live`).
- `functions-runtime-deno` — additionally run **TypeScript/JavaScript** functions
  (Deno/V8). Additive to `functions-runtime`; a separate opt-in because V8 adds
  ~30 MB and compile time.

**Which published image.** `ghcr.io/fraiseql/server-platform` is the tag built with
`functions-runtime-deno`; `server` and `server-full` are not built with any function
runtime. Since #1326 that is a refusal rather than a silent drop: a compiled schema
declaring a non-empty `functions` section does not boot on a build that cannot run it,
and the error names the feature and the tag. Before #1326 such a server booted clean,
logged nothing, and every declared function never fired — which is what the published
image did, because it is built `rest,arrow`.

A build without the `v8` prerequisites cannot produce this image at all: the `v8` crate
downloads a prebuilt static archive via `curl`, which the Dockerfile's builder stage now
installs for exactly this reason.

The embedder assembles the `FunctionsSubsystem` and registers the runtime(s) it
built with on the observer, e.g.:

```rust
observer.register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default())?);
```

## Authoring a function (#1325)

A function has two halves, with one owner each
([config-vs-settings.md](./config-vs-settings.md)):

- the **definitions** — what fires, on which trigger, under what authority — are schema, authored
  through an SDK and carried in `schema.json`;
- the **settings** — where the modules live and which dead-letter store backs dispatch — are
  deployment facts, declared in `[functions]` in `fraiseql.toml`.

Neither half can reach into the other. A `FunctionDefinition` has no `module_dir` key
and rejects unknown keys, so a schema cannot set a setting; and `[functions]` has no way
to declare a function. Configuring the table with no function declared is a compile
error: there would be nothing for the settings to apply to.

```python
# schema.py
@fraiseql.function(
    trigger="after:mutation:Order:update",
    timeout_ms=2000,
    when=[{"field": "status", "changed_to": "approved"}],
)
def notify_approved() -> None:
    """Runs functions/notify_approved.ts when an order is approved."""
```

```typescript
// schema.ts
class Functions {
  @FraiseFunction({
    trigger: "after:mutation:Order:update",
    timeoutMs: 2000,
    when: [{ field: "status", changed_to: "approved" }],
  })
  notify_approved() {}
}
```

```toml
# fraiseql.toml
[functions]
module_dir = "functions"                # default; where the .ts / .wasm modules live
dlq_store = "postgres"                  # memory (default) | postgres
```

**The function name is the module file stem.** The server loads
`<module_dir>/<name>.<ext>`, so `notify_approved` must be `functions/notify_approved.ts`.
It is the one name an SDK carries **verbatim** — every other name is camelCased on the
way out, and recasing this one would send the compiler looking for a file the author
never wrote. The cross-SDK conformance suite asserts the exact spelling for that reason.

Ten of the eleven official SDKs author functions; `fraiseql-rust` declares the gap in
`sdks/official/conformance/manifest.json` (it is field-level-RBAC focused and ships no
builder). See `sdks/official/README.md` for the support matrix.

### What the compiler checks (#1325)

A bad declaration fails `fraiseql compile`, not server boot:

| Check | Example refusal |
|---|---|
| trigger grammar | `whenever:something:happens` |
| `changed_to` on a non-`update` trigger | `after:mutation:Order:insert` + `changed_to` |
| `http:` / `after:storage:` | nothing mounts them (#871) |
| `before:mutation:` names a declared mutation | `before:mutation:deleteOrder` with no such mutation |
| `after:mutation:` names a **returned type** | `after:mutation:updateOrder` — it matches the mutation's *return type*, not its name |
| `when` fields exist on that type | `{"field": "statuss"}` on an `Order` with `status` |
| the module is on disk, with an extension the runtime loads | `runtime: "Wasm"` beside a `notify.ts` |
| a query's `function` names a **declared** function | `function = "preview_qoute"` |
| …whose trigger is `request:query` | a query bound to an `after:mutation` handler |
| every `request:query` function is named by a query | a declared function no field points at — it would load, the server would boot, and nothing would ever call it |
| no dispatch setting on a `request:query` function | `run_as`, `when`, `re_runnable`, `retry` — see below |
| nothing that lowers into SQL beside a `function` | `relay`, `count`, `inject_params`, `pagination_order`, `auto_params`, `rest_stream`, `jsonb_column`, `read_routing`, `cache_ttl_seconds`, a `rest` override |
| `function` on a **nested** field | root fields only — see below |

The last one runs only when `module_dir` exists at compile time. The compiler is then
looking at the real project layout and a missing module is a typo it can name; when the
directory is absent it is plainly not looking at the deployment layout — a CI job
compiling before the `.wasm` artifacts are fetched — and refusing would block a
legitimate workflow over a fact it cannot observe. The server still checks at boot. It
is the same trade `--database` makes for column validation.

The trigger-grammar half of that list is **one rule with two call sites**
(`TriggerRegistry::validate_definitions`), not two copies: the server's schema loader
calls it too, because a compiled schema is an input it does not produce and a
hand-written or stale artifact must still fail at boot. It used to keep its own
`VALID_TRIGGER_PREFIXES` list instead, and that list had already fallen two trigger
kinds behind — `after:capture:` (#366) and `after:ingest:` were parsed and dispatched by
the registry and refused by the loader, so a valid schema could not boot.

The query ↔ function pairing is the same shape: `validate_query_bindings`, called by
the compiler and again by the loader.

## Answering a request from a function (#1329)

Every trigger above is a **side effect**: something happened, and a function ran
because of it. None of them can compute and *return* a result without persisting
something first — which rules out a quote preview, a recommendation, an LLM-backed
answer, a BFF-style aggregation.

A `request:query` function can. A root query field declares the function that answers
it, in place of a `sql_source`:

```python
# schema.py
@fraiseql.function(trigger="request:query")
def preview_quote() -> None:
    """Runs from functions/preview_quote.ts."""

@fraiseql.query(function="preview_quote")
def quote_preview(sku: str) -> Quote | None: ...
```

```ts
// functions/preview_quote.ts
export default async function (event) {
  const { sku } = event.data;                 // the field's resolved arguments
  const rows = await Deno.core.ops.fraiseql_query(
    `{ price(sku: "${sku}") { amount } }`, "{}",   // read-only, as the caller
  );
  return { id: sku, total: computeTotal(JSON.parse(rows)) };  // the field's data
}
```

### What the engine keeps

Everything except the value. The function is asked for the field's **data**; the
engine still enforces `requires_role` and `requires_actor` before asking, applies
field-level RBAC — both the static `requires_scope` gate and the per-row dynamic
authorizer (#423) — to what comes back, projects the selection set, stamps
`__typename`, and consults and populates the response cache. That is the whole
argument for resolving inside the engine: a field resolved beside it would have to
re-implement each of those and would be wrong about one within a release.

The dynamic gate is the sharpest of them. A function-backed field returns documents
of the **same declared type** a SQL-backed one would, so a policy-gated field on that
type is gated here too — and an anonymous caller selecting one is refused before the
invocation, which also means an unauthorized request spends no isolate.

It also means a guest cannot invent a field the schema does not declare, or return
one the caller's scopes deny — the projector sees to both.

### Root fields only

There is no nested-field equivalent and there will not be one. A nested resolver runs
once per row, so a function there is an N+1 measured in V8 isolates. The compiler
refuses the nested spelling rather than documenting against it, and the read bridge
refuses a *guest* read of a function-backed field for the same reason — otherwise a
field that read itself would recurse, one isolate per level, until the query timed out.

### What it costs, in milliseconds

**~5–8 ms per invocation**, before the function does any work. Measured on a release
build with a trivial guest; ~2.65 ms of it is deno_core's own bootstrap.

Put beside this repository's own documented read latencies — ~5–15 ms cold, well
under 1 ms cached — that is the same order as a cold read and roughly 5× a cache hit.
It is a defensible price for computation SQL cannot express and a poor one for
anything a view could answer. Three consequences worth acting on:

- **Declare what it reads.** With no `sql_source` there is nothing for the invalidator to infer a
  read set from, so `additional_views` is how a function-backed field says which writes must evict
  it. A field that reads nothing declares nothing and is invalidated by nothing — correct, because
  nothing it returns depends on a row.
- **`cache_ttl_seconds` is refused beside it.** A per-query TTL is applied to the **row** cache,
  keyed by the query's view; this field reads none, so the number would be accepted and never
  applied.
- **Today it is not cached at all in the stock binary**, and that is worth saying plainly. The
  engine's whole-response cache is the only facility that could cover a field with no view, and
  `fraiseql-server` installs none (#1344) — so an invocation happens on every request. The read
  path here consults and populates that cache exactly as the SQL path does, so the field becomes
  cacheable the day it is wired rather than needing this decision re-made then.

### Which transports carry it

`/graphql` and **MCP**, because both go through the executor — MCP builds a GraphQL
document and hands it to `execute_with_security`, so it needed no change at all. That
is the practical payoff of resolving inside the engine rather than beside it.

**REST and gRPC do not carry it**, and they were each skipped for a different reason:

| surface | why it is skipped |
|---|---|
| REST route table | the surface is *derived* — it invents list/detail routes, filters and pagination from the type, and a function-backed field accepts none of them. The table reports the omission rather than mounting routes that would answer every request with "Query has no SQL source" |
| REST resource **embedding** | it resolves its target list query out of `schema.queries` directly, not from the route table, so it is the one REST path that could still have reached one. A type with both kinds embeds from its SQL-backed list query; a type whose only list query is function-backed embeds nothing |
| gRPC dispatch table | it answers a method by reading `vr_<type.sql_source>` directly and never consults a resolver. Registering one would have served the **type's rows in place of the function's answer** — a wrong result that looks like a right one, which is worse than an absent method |
| `EXPLAIN` | it reports a database plan and there is none. The refusal says so, and points at the reads the function makes through the bridge, each of which has its own plan under its own query name |

The gRPC row is the one worth remembering. Every other gap in this list fails loudly;
that one would have failed silently, and only enumerating the query-selection sites
found it — not review of the paths anyone was thinking about.

### Identity, and the reads it makes

The function runs **as its caller**. Its `fraiseql_query` bridge is the read-only,
caller-scoped one from #1328 — the same object, not a second one — so it can never
surface a row the caller could not have read itself, and an anonymous request reads
anonymously.

That last part is load-bearing on an RLS deployment. The engine's anonymous-read
refusal governs *this query's own read*, and a function-backed field issues none: what
the function reads goes through the bridge, where the same refusal applies to *that*
read. So a function that reads fails closed on its own read, and one that only
computes still answers an unauthenticated visitor — which is the case the field is
for.

### Timeouts

An invocation that overruns yields a GraphQL error naming the field, not a dropped
connection. Two knobs, each doing one thing:

| | Who sets it | Default |
|---|---|---|
| the function's `timeout_ms` | the **author**, about their own function | none |
| `FRAISEQL_FUNCTIONS_REQUEST_QUERY_BUDGET_MS` | the **operator**, for functions that declare none | 5 000 ms (`0` disables, logged loudly) |

The declaration wins over the default rather than being capped by it: a cap would
silently overrule a number the author wrote down, and the request is bounded either
way by the executor's own `query_timeout_ms`.

### What may not be declared beside it

A `request:query` function is not a dispatch, so `run_as`, `when`, `re_runnable` and
`retry` are compile errors on one. `run_as` is the one that matters: it is an
authority ceiling, and a security setting accepted and never applied reads as a
granted authority.

On the query side, everything that lowers into SQL is refused likewise — `relay`,
`count`, `pagination_order`, `auto_params`, `rest_stream`, `jsonb_column`,
`read_routing`, `cache_ttl_seconds`, a `rest` route override, and `inject_params`.
`inject_params` is the sharp one: it is how a query is scoped to the caller's tenant,
and dropping it does not break a field, it **widens** one.

### The retired name-dispatched route

`POST /functions/v1/{name}` was a library-only route — mounted by
`Server::with_functions`, which the stock binary never called — that dispatched by
function name and ignored the trigger. It was retired with this decision, so there is
one path from a request to a function rather than two that drift. `http:` triggers
stay refused (#871); they can be revisited on evidence of a case a typed root field
cannot serve.

## Crate Dependencies

```
fraiseql-functions
├── fraiseql-error
├── fraiseql-core (optional)
├── fraiseql-db (optional)
├── fraiseql-observers
└── fraiseql-storage (optional)
```

## Testing

```bash
cargo nextest run -p fraiseql-functions --features runtime-wasm,runtime-deno,host-live
cargo test -p fraiseql-server --test platform_e2e_test  # E2E tests
```

> **Use `cargo nextest`, not `cargo test`, for the Deno runtime.** Each test that
> spins up a V8 isolate does so on a fresh thread; the shared-process `cargo test`
> harness can `SIGSEGV` when several isolates are created in one process. Nextest
> runs each test in its own process (fresh V8 platform), which is how CI runs them.

## Authoring: the local invoke harness (`fraiseql functions invoke`)

A function author does not need a running server, a database, or the network to
test a function — `fraiseql functions invoke` runs a compiled function in a **real
V8 isolate** against a fixture payload, with **mocked host ops**, and prints the
result plus every host-op call the guest made. It is the author's inner loop:
fixture → run → observe. Built into the CLI behind the opt-in `functions-invoke`
feature (V8 is ~30 MB, so the stock CLI stays lean).

```bash
# A matching payload runs; --explain shows why the `when` predicate did/didn't fire.
fraiseql functions invoke notifyApproved --payload event.json --explain

# Mock the host ops the function calls (a request matching no mock fails loud).
fraiseql functions invoke syncDeal --payload deal.json \
    --mock-http http.json --mock-query query.json --idempotency-token abc123

# A data-dependent before:mutation rule: the payload IS the mutation's arguments,
# and --mock-query stands in for the read (#1328). The harness prints the decision
# the chain would reach, not just the guest's raw return value.
fraiseql functions invoke creditLimit --payload args.json --mock-query credit.json
#   decision: ABORT `placeOrder` — amount 900 exceeds the remaining credit of 600
```

The module is loaded exactly as the server loads it (from the compiled schema's
`module_dir`). Host ops are answered by a recording mock: `fraiseql_query` /
`fraiseql_http_request` from `--mock-query` / `--mock-http` (a matched entry → its
canned response; a miss against a configured mock **fails loud**, surfacing as a
guest error); other ops return benign defaults so a first run reveals which ops a
function calls before its mocks are written. `--idempotency-token` injects the token
the guest reads via the host op.

**Payload fixtures** are validated against the trigger kind — an `after:mutation` /
`after:capture` fixture is `{ "event_kind": "update", "old": {…}, "new": {…} }` (a
bare object is treated as an insert's `new` image); a **`before:mutation`** fixture
is the mutation's resolved *arguments* object (`{ "input": {…} }`), because the write
has not happened and there are no row images; a **`request:query`** fixture (#1329)
is the query field's resolved arguments object (`{ "sku": "ABC-1" }`), for the same
reason. The `when` predicates (#597) are evaluated *before* any isolate spins, so a
non-matching payload costs nothing.

The `request:query` payload is built by the same routine the server calls
(`request_query_payload`), not by a second copy of the shape. One difference is
unavoidable and is stated rather than hidden: a real invocation carries the name of
the *query field* that named the function, and the harness has no schema to find it
in, so it stands in the function's own name.

For a `before:mutation` function the harness also prints the **decision** —
`ABORT <mutation> — <reason>`, `PROCEED (arguments unchanged)`, or `PROCEED with
rewritten arguments: …` — resolved through the same function the server's chain
decides with, so the harness cannot tell an author one thing while the server does
another. Under `--json` it is a `{"mutation", "decision", "arguments_or_reason",
"rewritten"}` object, so a CI check can assert that a rule refuses a given input.

**Exit codes** are scriptable in CI: `0` = ran; `3` = the `when` predicate did not
match (nothing would fire); `4` = the guest errored; `1` = a config/harness error.

### Typed guest payloads (`functions.d.ts`)

`fraiseql generate-client typescript` emits a `functions.d.ts` alongside the client
whenever the compiled schema declares functions. It gives a function author editor
type-checking for both halves of a function:

- **The host surface** — an ambient `Deno.core.ops.fraiseql_*` declaration
  (`FraiseqlHostOps`), so `fraiseql_query` / `fraiseql_http_request` / … are typed.
- **The event payload** — one interface per function, derived from its trigger. An
  `after:mutation` / `after:capture` function on entity `E` gets
  `{ event_kind, old: E | null, new: E | null }` (with `E` imported from the generated
  `./types`); `cron` gets its schedule context; `after:ingest` gets the inbound-message
  shape. An entity the schema does not define falls back to `unknown` rather than a
  dangling reference. A `request:query` function (#1329) gets a **discriminated union**
  over the root fields that name it — `{ field: "quotePreview"; arguments: { sku: string } }`
  — so a guest answering two fields can tell which one it was invoked for. It is a union
  even when only one field names the function, so the shape a guest destructures does not
  change the day a second one does.

```typescript
import type { NotifyUserEvent } from "./functions";
// `import type` is erased by the runtime's type-stripper; it is authoring-only.
export default async (event: NotifyUserEvent) => {
  if (event.new?.status !== "approved") return;
  await Deno.core.ops.fraiseql_query(/* … typed host op … */);
};
```

> Tracked follow-ups: `cron` / `after:ingest` payload *synthesis* in `invoke`, and a
> `--record` mode that captures real host-op traffic into the mock files for golden replay.

## See Also

- [Storage Architecture](storage.md) -- Object storage backends
- [Architecture Overview](overview.md) -- System-wide architecture
