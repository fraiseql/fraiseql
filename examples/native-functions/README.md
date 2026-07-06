# Native functions (TypeScript)

Custom compute that runs on a database event and does async I/O — authored in
TypeScript, executed **in-process** by the FraiseQL server's Deno runtime. No
Python sidecar, no separate deployment.

These are the workloads migrated off an adjacent Python/FastAPI sidecar onto the
native runtime. Each function is a single `export default`
handler that receives the event payload and reaches the host through
`Deno.core.ops.fraiseql_*`.

Write ordinary TypeScript: the runtime strips type annotations to JavaScript
before execution (a real `deno_ast`/swc transpile — interfaces, `: Type`,
generics, `as`, `enum`), so a `.ts` handler with types runs as-is. Reference
[`fraiseql-host.d.ts`](./fraiseql-host.d.ts) for host-op type-checking and
autocomplete — `deal-scoring.ts` is the annotated reference; the other three use
the type-annotation-free subset for brevity.

## `deal-scoring.ts`

An `after:mutation:Deal:update` scorer + next-action recommender: reads an LLM
API key from the host's secret store (`env_var`), calls the model over
SSRF-allowlisted `http_request`, and writes the resulting score **and recommended
next action** back with a GraphQL `query`. It runs on the **fire-and-forget**
path — scoring is re-runnable — and is idempotent against a human edit
(`score_source === "human"` short-circuits). An unrecognised model action is
coerced to the safe default (`wait`) so a downstream actor never dispatches on a
value it does not understand.

The host surface it uses:

| Op | Purpose |
|----|---------|
| `fraiseql_env_var(name)` | read the LLM API key (allowlisted secret) |
| `fraiseql_http_request(method, url, headers, body?)` | call the LLM (deny-by-default SSRF allowlist via `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`) |
| `fraiseql_query(graphql, variablesJson)` | write the score + next action back |

## `qonto-sync.ts`

An `after:mutation:Invoice:*` function on the **durable** path (registered
`re_runnable = false`, so the dispatcher owns retry/backoff/DLQ — see
`docs/architecture/functions.md` and ADR 0015). It registers the invoice as a
Qonto transfer and records the reference back onto the invoice. Money-path safety
rests on a **deterministic, invoice-derived idempotency key**
(`qonto-invoice-${id}`, never random): every retry of the same invoice reuses it,
so Qonto dedups server-side and the transfer is created at-most-once under
at-least-once dispatch. It fails loud on any non-2xx (never fabricates success)
and short-circuits an already-synced invoice.

| Op | Purpose |
|----|---------|
| `fraiseql_env_var(name)` | read the Qonto API key (allowlisted secret) |
| `fraiseql_http_request(...)` | call Qonto with the idempotency key header |
| `fraiseql_query(graphql, variablesJson)` | record the Qonto reference |

## `follow-up-email.ts`

An `after:mutation:Deal:update` function that acts on the scorer's `send_follow_up`
next action by sending a **per-user** follow-up. The banked constraint is that a
paired outbound email is sent *from the connected user's verified address, never a
shared mailbox*: the `from` is taken only from `auth_context` and a missing
verified address fails loud (no default-sender fall-back). It mirrors the Rust
policy `fraiseql_functions::outbound::resolve_sender_identity`, which a planned
`send_email` host op will enforce structurally. See
`docs/architecture/native-runtime-ergonomics.md`.

| Op | Purpose |
|----|---------|
| `fraiseql_auth_context()` | the connected user's verified sending identity (the `from`) |
| `fraiseql_env_var(name)` | read the mail-provider API key (allowlisted secret) |
| `fraiseql_http_request(...)` | send via the provider (SSRF-allowlisted) |

## `reply-awareness.ts`

An `after:ingest:email` handler for the poll-IMAP email adapter (see
`docs/architecture/inbound-email.md`). Every inbound email is normalized and
classified before this runs, so the function is a thin decision: only a **human**
reply stops the active outreach sequence (via a GraphQL `query`); bounces,
out-of-office replies, challenges, and auto-mail are ignored — that single
`classification === "human"` check is both the reply-awareness gate and the mail-loop
guard. It runs on the **durable** dispatch path (retry + dead-letter), and the
stop mutation is idempotent, so at-least-once inbound delivery is safe.

The host surface it uses:

| Op | Purpose |
|----|---------|
| `fraiseql_query(graphql, variablesJson)` | look up the active sequence and stop it |

## Running

Build the server with the Deno runtime and register it on the function observer:

```bash
cargo run -p fraiseql-server --features functions-runtime-deno
```

```rust
observer.register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default())?);
```

Grant the LLM host in the SSRF allowlist and provide the key:

```bash
export FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS="api.llm.test"
export LLM_API_KEY="sk-…"
```

See `docs/architecture/functions.md` for the full host surface and build features.
