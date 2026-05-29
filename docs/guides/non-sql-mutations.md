# Non-SQL Mutations

FraiseQL compiles mutations to PostgreSQL function calls. Anything that fits
PL/pgSQL is in scope; anything that doesn't — HTTP calls, ML inference, payment
APIs, long-running batch jobs — needs a different shape. This guide shows you
which shape, in about 30 seconds.

## Decision table

| Your mutation does... | Use |
|---|---|
| INSERT / UPDATE / DELETE rows (any join complexity) | **SQL function** |
| Reads/writes via a PostgreSQL extension (pgvector, PostGIS, ...) | **SQL function** |
| Calls a third-party HTTP API as part of the operation | **Federation subgraph** |
| Submits a job to a queue and returns a handle | **Federation subgraph** |
| Triggers an LLM / ML model call | **Federation subgraph** |
| Is a one-shot batch import, not a user-facing API call | **Not a mutation** — use `psql` / a cron job / the observer system |

If a row says **SQL function**, write a normal FraiseQL mutation. If it says
**Federation subgraph**, read on. If it says **Not a mutation**, the work is
operational, not part of your GraphQL API at all.

## Why the split exists

FraiseQL's value proposition is **compile-time determinism**: every query and
mutation is statically known, the planner sees all dispatch, and the SQL is
generated and validated before the server boots. An in-process callback to an
arbitrary HTTP endpoint breaks that — the compiler can't know what the callback
does, the planner can't optimise around it, the type system can't validate it.
This is exactly why
[ADR-0010](../adr/0010-async-mutation-handlers.md) (a proposed async-mutation
runtime *inside* FraiseQL) was **rejected**.

The federation pattern preserves the property. The subgraph **is** a separate
service with its own GraphQL schema — which its own framework can statically
validate — and a federation router composes the two schemas at the gateway
level. From the client's view it's one API. From the server's view, each piece
stays in its own lane: FraiseQL owns the SQL; the subgraph owns the async work.

## Worked example: federation subgraph

See [`examples/async-jobs-subgraph/`](../../examples/async-jobs-subgraph/) for
runnable code (`make dev` + `make demo-local` for the subgraph alone, `make run`
+ `make demo` for the full router-composed stack). The narrative:

1. Client issues `mutation { enqueueJob(input: "foo") { id status } }` against
   the federation endpoint.
2. The router routes the mutation to the async-jobs subgraph.
3. The subgraph creates a `JobHandle`, returns it immediately, and kicks off the
   async work in the background.
4. Client polls `query { jobStatus(id: "...") { status result } }` until status
   is `SUCCEEDED` or `FAILED`.

The subgraph can be any language with a GraphQL server library (`async-graphql`
for Rust, Strawberry for Python, Apollo Server for JS, gqlgen for Go, ...).
FraiseQL never calls into it directly — the router composes their schemas.

## Three concrete scenarios

### AI scoring (ML inference)

**Pattern:** federation subgraph.

Model invocation is an external call (OpenAI, Anthropic, a local Triton
server, ...) and is not deterministic in the SQL sense. Wrap it in a subgraph
that accepts the input and returns either a synchronous result (small, fast
models) or a job handle to poll (large models with latency). FraiseQL stores
whatever you decide to persist about the outcome.

### Payment processing

**Pattern:** federation subgraph.

Payment APIs (Stripe, Adyen) are external HTTP calls with their own retries,
idempotency keys, and webhook lifecycle. The subgraph owns that integration;
FraiseQL owns the SQL state that records what happened (the `payment` row, its
status transitions) via ordinary SQL mutations the subgraph's webhook handler
triggers.

### Batch CSV import (one-shot)

**Pattern:** not a mutation. Use the observer system or a standalone script.

Nobody's GraphQL client calls `mutation { importCsv(file: ...) }`. The work is
operational, not API-facing. The observer system can react to a
file-uploaded event; a cron job or `psql -f load.sql` handles the rest. Keeping
it out of the GraphQL surface keeps the schema honest.

## Related

- [ADR-0010: Async Mutation Handlers](../adr/0010-async-mutation-handlers.md) —
  why FraiseQL doesn't ship runtime async-mutation infrastructure (Rejected).
- [`examples/async-jobs-subgraph/`](../../examples/async-jobs-subgraph/) —
  runnable end-to-end example of this pattern.
- [Cross-Subgraph Mutations: Saga Pattern](./federation-saga.md) — coordinating
  a mutation that must write to more than one subgraph.
- [Federation Circuit Breaker](./circuit-breaker.md) — protecting federation
  fan-out when a subgraph is slow or unavailable.
