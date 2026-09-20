# fraiseql-saga

Distributed saga orchestration for FraiseQL — forward execution, compensation, crash
recovery, and the Postgres store that persists them.

## Where it sits

```text
fraiseql-db ──► fraiseql-federation ──► fraiseql-core ──► fraiseql-saga
                (entity resolution)      (chokepoint)      (orchestration)
```

A saga step is a **client of the mutation chokepoint**, not a second writer. Its local
arm calls `Executor::execute_mutation_with_security`, so `requires_role`,
`requires_actor`, the operation `Authorizer`, argument validation, the
`before:mutation` chain, session-variable binding and the change-log write all run —
the same gates any other write faces.

That is why this crate exists. The orchestrator used to live *below* the engine, inside
`fraiseql-federation`, where the chokepoint was unreachable. It had grown its own
`INSERT`/`UPDATE`/`DELETE` string builder and dispatched it raw, skipping every gate
above ([#1354](https://github.com/fraiseql/fraiseql/issues/1354)). The layering was the
defect; moving the orchestrator above the engine is the fix.

## What it provides

| Component | Notes |
|---|---|
| `SagaExecutor` | Forward execution with retry/timeout policy and cross-subgraph `@requires` pre-fetch. |
| `SagaCompensator` | Rolls completed steps back in reverse order, each on the transport its forward step used. |
| `SagaRecoveryManager` | Re-drives crash-interrupted sagas under a `FOR UPDATE SKIP LOCKED` lease. |
| `SagaCoordinator` | Ties forward execution and compensation into one handle. |
| `PostgresSagaStore` | Persists saga and step state. |
| `HttpMutationClient` | SSRF-protected mutation propagation to a peer subgraph, with optional mutual TLS. |

## Opting in

Depending on the crate *is* the opt-in. Nothing in the server or core feature chain
reaches it, so a deployment that does not orchestrate cross-subgraph transactions never
compiles the saga store or its dependencies.

```toml
[dependencies]
fraiseql-saga = "2.15"
```

`test-utils` exposes test-helper constructors on the coordinator and the store. Never
enable it in production — they bypass security checks.
