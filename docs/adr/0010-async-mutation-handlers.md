# ADR-0010: Async Mutation Handlers for Long-Running Operations

- **Status:** Rejected (2026-05-29)
- **Decision:** FraiseQL will **not** add an async-mutation runtime to the
  engine. Non-SQL mutations are handled with a **federation subgraph** instead.
- **See instead:** [`docs/guides/non-sql-mutations.md`](../guides/non-sql-mutations.md)
  and the runnable [`examples/async-jobs-subgraph/`](../../examples/async-jobs-subgraph/).

## Context

FraiseQL mutations compile to SQL function calls (`PL/pgSQL`, `PL/MySQL`, etc.).
If the work cannot be expressed as a database function, there is no mutation
path in FraiseQL itself. That rules out an entire class of operations:

- AI/ML analysis (sentiment, classification, image recognition)
- Batch processing (report generation, bulk exports, transformation)
- External service calls (payments, third-party integrations)
- Async workers (email, SMS, webhooks)

This ADR explored closing that gap inside the engine, and concluded the gap
should be closed *outside* it.

## What was proposed

A pluggable **async mutation handler** system: schema authors would declare
async mutations in `fraiseql.toml` (mutation name, an HTTP/AMQP handler
endpoint, timeout, retries); the compiler would validate them, auto-inject
`JobHandle` / `JobStatus` / `Job` types and a `jobStatus(jobId)` query (mirroring
the Relay type-injection pattern), and rewrite each async mutation's return type
to `JobHandle`. At runtime a trait — `AsyncMutationHandler` — would delegate the
mutation to the configured external service, passing a `SecurityContext`
(never raw credentials), with job state held in a pluggable `JobStore`
(in-memory for the MVP, Redis/database later).

## Decision

**Rejected.** The cost to FraiseQL's core guarantees outweighs the convenience,
and a clean alternative already exists.

## Rationale for rejection

1. **Breaks compile-time determinism.** Runtime calls to arbitrary endpoints
   cannot be validated by the FraiseQL compiler. The guarantees that make the
   SQL engine trustworthy — every query and mutation is statically known, the
   planner sees all dispatch, the generated SQL is validated before the server
   boots — collapse the moment a handler dispatches to an opaque, user-defined
   endpoint the compiler can't reason about.
2. **Introduces server-side mutable state.** A job store is lost on restart and
   not shared across instances unless it is made durable — and making it durable
   means building a queue/worker subsystem, which is exactly what external job
   systems (Redis, SQS, Temporal, Celery, ...) already provide, with operational
   maturity FraiseQL would otherwise have to rebuild and maintain.
3. **Federation already solves this cleanly.** One resolver in a lightweight
   subgraph, composed into the schema by a federation router. No framework
   changes in core, and the subgraph can be written in any language with a
   GraphQL library.
4. **Keeps FraiseQL focused.** FraiseQL is a compile-time SQL engine, not a
   runtime orchestration layer. Saying no here preserves the property that
   differentiates it from Hasura and PostGraphile, and keeps the security story
   simple — no SSRF surface from configurable handler endpoints, no credential
   delegation, no handler-versioning matrix to manage.

## Alternatives considered

| Alternative | Verdict |
|---|---|
| In-engine async handler system (the proposal above) | **Rejected** — reasons above. |
| Arbitrary command execution (`command = "python analyze.py"`) | Rejected — shell-injection / arbitrary-code-execution surface; not statically validatable. |
| Post-mutation webhooks | Partial complement only — fire-and-forget side effects, not part of the response; doesn't model "the mutation *is* the async job". |
| `@defer` / lazy evaluation | Rejected for this use case — `@defer` optimises existing queries; it doesn't model "enqueue a job". |
| Inline `Job` object as the mutation return type | Rejected — schema bloat, opaque `String` result loses type safety, couples every async mutation to one job model. |
| SQL delegation (`SELECT enqueue_job(...)` from PL/pgSQL) | Rejected — SQL lacks first-class async primitives; tying handler lifecycle to the transaction is error-prone across engines. |
| **Federation subgraph** | **Accepted** — see below. |

## Recommended alternative

Build the async work as a **federation subgraph** — a small GraphQL service
(any language, any framework) that:

1. Accepts a mutation (`enqueueJob(input: ...)`), submits the work to your queue
   of choice, and returns a `JobHandle { id, status }` immediately.
2. Exposes a `jobStatus(id: ID!)` query for polling.
3. Federates into the FraiseQL schema via a router, so consumers see one
   unified GraphQL API — SQL-backed types from FraiseQL, async operations from
   the subgraph.

This keeps every compile-time guarantee intact: FraiseQL stays a pure SQL
engine, the subgraph owns the non-deterministic work behind its own statically
validated schema, and the router composes the two.

A runnable end-to-end example lives in
[`examples/async-jobs-subgraph/`](../../examples/async-jobs-subgraph/) (the
async-jobs subgraph is a self-contained Rust + `async-graphql` service you can
`cargo run`). A decision guide — *when to use SQL vs federation vs neither* — is
in [`docs/guides/non-sql-mutations.md`](../guides/non-sql-mutations.md).

## References

- [GraphQL Federation Spec](https://www.apollographql.com/docs/apollo-server/using-federation/introduction/)
- OWASP [SSRF Prevention](https://cheatsheetseries.owasp.org/cheatsheets/Server_Side_Request_Forgery_Prevention_Cheat_Sheet.html)
- FraiseQL architecture: [`docs/architecture/overview.md`](../architecture/overview.md)
