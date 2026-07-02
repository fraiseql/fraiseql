# Apollo Federation v2 in FraiseQL

FraiseQL implements Apollo Federation v2 as an optional layer (enable with the `federation`
Cargo feature). Unlike a traditional gateway that routes at runtime, FraiseQL compiles
the federation contract into SQL execution plans at build time, eliminating overhead for
deterministic cross-subgraph queries.

## Component Map

```
crates/fraiseql-federation/src/
├── entity_resolver.rs        # @key entity resolution + batching (MAX_ENTITIES_BATCH_SIZE = 1000)
├── http_resolver.rs          # HTTP subgraph client (SSRF-hardened via reqwest::Url::parse)
├── representation.rs         # _Any scalar → EntityRepresentation parsing
├── composition_validator.rs  # @key / @requires / @provides compile-time validation
├── requires_provides_validator.rs
├── dependency_graph.rs       # Cross-entity dependency ordering
├── mutation_executor.rs      # Cross-subgraph mutation dispatch
├── mutation_http_client.rs   # HTTP client for subgraph mutations (SSRF-protected)
├── saga_coordinator.rs       # SagaCoordinator facade (`saga` feature)
├── saga_compensator.rs       # Compensation (rollback) executor (`saga` feature)
├── saga_executor/            # Forward phase: step execution + @requires pre-fetch (`saga` feature)
├── saga_recovery_manager.rs  # On-restart recovery w/ SKIP LOCKED leasing (`saga` feature)
├── saga_store.rs             # PostgreSQL persistence for saga state
└── circuit_breaker (server)  # In fraiseql-server/src/federation/circuit_breaker.rs
```

## Architecture Layers

### Layer 1 — Schema Composition (compile time)

The FraiseQL CLI validates the composed supergraph before producing
`schema.compiled.json`:

1. Parse subgraph schemas and `@key` directives
2. `CompositionValidator` checks for conflicts, missing keys, invalid `@requires`
3. `DependencyGraph` resolves cross-entity reference ordering
4. Compiled schema embeds the `federation` section with entity metadata and
   circuit breaker configuration

```bash
fraiseql compile schema/ -o schema.compiled.json
```

### Layer 2 — Entity Resolution (runtime)

When a federated query contains `_entities`, FraiseQL:

1. Parses `_Any` scalars into typed `EntityRepresentation` objects
2. Groups representations by `__typename`
3. Chooses a resolution strategy per entity type:
   - **Local**: Entity lives in this subgraph's database — resolved via SQL
   - **HTTP**: Entity lives in a remote subgraph — fetched via `HttpEntityResolver`
4. Batches requests (up to `MAX_ENTITIES_BATCH_SIZE = 1000` per batch)
5. Merges results and applies field projection

```text
_entities(representations: [...])
    │
    ▼ EntityResolver
    ├── Local entities → SQL via CachedDatabaseAdapter
    └── Remote entities → HttpEntityResolver → subgraph HTTP endpoint
                              ↑
                         CircuitBreaker (per entity type)
```

### Layer 3 — Cross-Subgraph Mutations (saga pattern)

> **Stable, behind the opt-in `saga` Cargo feature** on `fraiseql-federation`. The full
> round-trip is wired: forward execution over **local SQL** or **remote HTTPS**
> (`SagaCoordinator` / `execute_saga`, with optional mTLS), automatic
> **compensation** in reverse order (local or remote), concurrency-safe on-restart
> **recovery** (`SELECT … FOR UPDATE SKIP LOCKED` leasing), per-step **retry with
> backoff + timeout**, and cross-subgraph **`@requires` pre-fetch**. The public handle
> is `SagaCoordinator`; when the `saga` feature is off, the saga types are not compiled.

Cross-subgraph mutations use a saga orchestrator. Each saga step has a forward action and
a compensation action; on failure, compensation runs in reverse order (N→1) for
best-effort rollback.

See [federation-saga.md](../guides/federation-saga.md) for the developer guide.

### Layer 4 — Circuit Breaker

Each entity type has an independent circuit breaker in `fraiseql-server`.
The breaker protects against cascading failures when a subgraph becomes slow or
unavailable.

See [circuit-breaker.md](../guides/circuit-breaker.md) for configuration and tuning.

## Data Flow: Cross-Subgraph Query

```
Client → POST /graphql
    │
    ▼ GraphQL Handler
    │  Parse + validate query
    │
    ▼ FederationExecutor
    │  Detect federated entity references
    │
    ├─ Local fields → SQL (zero overhead, compiled at build time)
    │
    └─ Remote entities → EntityResolver
           │
           ▼ CircuitBreaker.try_request()
           │
           ├── CLOSED → HttpEntityResolver.resolve_batch()
           │       └── POST {subgraph_url}/_entities
           │
           └── OPEN → 503 Service Unavailable + Retry-After header
```

## Data Flow: Cross-Subgraph Mutation (Saga)

> Wired behind the opt-in `saga` feature via `SagaCoordinator`: forward steps run
> over local SQL or remote HTTPS, and on failure the completed steps are compensated in
> reverse order.

```
Client → mutation { createOrder(...) }
    │
    ▼ SagaCoordinator.execute_saga()
    │
    ├── Step 1: inventory-service.reserveInventory  ← forward
    │     └── OK → persist step result to tb_federation_saga_steps
    │
    ├── Step 2: billing-service.chargePayment        ← forward
    │     └── FAIL → trigger compensation phase
    │
    └── Compensation (reverse order):
          Step 1 compensation: inventory-service.releaseInventory
          └── OK → saga state = Compensated
```

## Security Notes

- **SSRF protection**: `http_resolver.rs` and `mutation_http_client.rs` use
  `reqwest::Url::parse()` + private-IP rejection. IPv6 brackets are stripped before
  `IpAddr::parse()` to prevent bypass via `[::1]` notation.
- **Batch size cap**: `MAX_ENTITIES_BATCH_SIZE = 1000` in `representation.rs`
  prevents memory exhaustion from oversized `_entities` queries.
- **State isolation**: saga state persists to the `tb_federation_saga*` tables before each
  step, enabling on-restart recovery. `SagaRecoveryManager::run_iteration` /
  `start_background_loop` (the `saga` feature) re-drive crash-interrupted sagas,
  claiming stuck sagas under a lease via `SELECT … FOR UPDATE SKIP LOCKED` so concurrent
  recovery workers never double-drive one.

## Observability

Prometheus metrics emitted by the federation layer:

| Metric | Description |
|--------|-------------|
| `fraiseql_federation_circuit_breaker_state{entity}` | 0=closed, 1=open, 2=half-open |
| `fraiseql_federation_circuit_breaker_opens_total{entity}` | How often the breaker trips |
| `fraiseql_federation_circuit_breaker_rejections_total{entity}` | Requests rejected while open |
| `fraiseql_saga_steps_total{subgraph, status}` | Saga step outcomes (planned — saga metrics not yet emitted) |
| `fraiseql_saga_duration_seconds` | End-to-end saga duration histogram (planned) |
| `fraiseql_saga_compensations_total` | Compensation phases triggered (planned) |
| `fraiseql_entity_resolution_duration_seconds{entity, strategy}` | Resolution latency |

## Enabling Federation

```toml
# Cargo.toml
[dependencies]
fraiseql-server = { version = "2", features = ["federation"] }
```

```toml
# fraiseql.toml
[federation]
enabled = true
service_name = "users"   # subgraph name (Apollo Studio / subgraph listing)
version = "v2"           # Apollo Federation spec version (defaults to "v2")
# schema_url = "https://users.example.com/graphql"  # optional, for /__subgraph_schema

# Entities this subgraph owns and their key fields.
[[federation.entities]]
name = "User"
key_fields = ["id"]

[federation.circuit_breaker]
enabled = true
failure_threshold = 5
recovery_timeout_secs = 30
success_threshold = 2

# Optional per-entity overrides (entity name must match a federation entity).
[[federation.circuit_breaker.per_database]]
database = "User"
failure_threshold = 10
```

> The legacy integer form `apollo_version = 2` is still accepted (`2` ⇒ `"v2"`);
> prefer the `version` string. When both are set, `version` wins.

## Related Docs

- [Federation Saga Guide](../guides/federation-saga.md)
- [Circuit Breaker Tuning](../guides/circuit-breaker.md)
- [Runbook: Circuit Breaker Tripped](../runbooks/14-federation-circuit-breaker.md)
- [ADR: Federation crate extraction](../adr/)
