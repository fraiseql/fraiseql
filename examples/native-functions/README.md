# Native functions (TypeScript)

Custom compute that runs on a database event and does async I/O — authored in
TypeScript, executed **in-process** by the FraiseQL server's Deno runtime. No
Python sidecar, no separate deployment.

These are the workloads being migrated off an adjacent Python/FastAPI sidecar as
part of the native-runtime-migration. Each function is a single `export default`
handler that receives the event payload and reaches the host through
`Deno.core.ops.fraiseql_*` (typed by the runtime's `FRAISEQL_HOST_TYPES`).

## `deal-scoring.ts`

An `after:mutation:Deal:update` scorer: reads an LLM API key from the host's
secret store (`env_var`), calls the model over SSRF-allowlisted `http_request`,
and writes the resulting score back with a GraphQL `query`. It runs on the
**fire-and-forget** path — scoring is re-runnable — and is idempotent against a
human edit (`score_source === "human"` short-circuits).

The host surface it uses:

| Op | Purpose |
|----|---------|
| `fraiseql_env_var(name)` | read the LLM API key (allowlisted secret) |
| `fraiseql_http_request(method, url, headers, body?)` | call the LLM (deny-by-default SSRF allowlist via `FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`) |
| `fraiseql_query(graphql, variablesJson)` | write the score back |

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
