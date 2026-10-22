# Apollo Federation v2 in FraiseQL

FraiseQL implements Apollo Federation v2 subgraph support as an optional layer (enable
with the `federation` Cargo feature on `fraiseql-server`, `fraiseql-core` or `fraiseql-cli`).
FraiseQL is a subgraph, not a gateway: an Apollo-compatible router composes the supergraph
and calls this server's `_service` and `_entities` fields. Compilation validates each
entity's `@key` and embeds the federation metadata in `schema.compiled.json`; the
`_entities` lookup SQL is built per request.

Cross-subgraph sagas live in a separate crate, `fraiseql-saga`, which no other crate in
the workspace depends on (see [Layer 3](#layer-3--cross-subgraph-mutations-fraiseql-saga)).

## Component Map

```text
crates/fraiseql-federation/src/
├── entity_resolver.rs             # _entities batch resolution from the local database (MAX_ENTITIES_BATCH_SIZE = 1000)
├── representation.rs              # _Any scalar → EntityRepresentation parsing (same 1000 cap)
├── database_resolver.rs           # Per-type lookup against the entity's backing relation
├── query_builder.rs, sql_utils.rs # Lookup SQL construction
├── selection_parser.rs            # Field selection for the lookup
├── service_sdl.rs                 # _service { sdl } rendering
├── http_resolver.rs               # HttpEntityResolver: _entities over HTTPS (SSRF-hardened); used by fraiseql-saga
├── composition_validator.rs       # CompositionValidator: library API (not called by the compiler)
├── requires_provides_validator.rs # @requires / @provides validation
├── dependency_graph.rs            # Cross-entity dependency ordering
├── connection_manager.rs          # Gateway-mode connections (`unstable` feature)
├── direct_db_resolver.rs          # Direct-DB federation (`unstable`; stub)
├── observability.rs               # Subgraph latency histogram, entity resolution counters
└── health.rs, logging.rs, tracing.rs, tls.rs, types.rs, metadata_helpers.rs, query_plan_cache.rs

crates/fraiseql-saga/src/
├── saga_coordinator.rs            # SagaCoordinator facade
├── saga_executor/                 # Forward phase: step execution + @requires pre-fetch
├── saga_compensator.rs            # Compensation (rollback) executor
├── saga_recovery_manager.rs       # On-restart recovery with SKIP LOCKED leasing
├── saga_store.rs                  # PostgreSQL persistence for saga state
├── mutation_executor.rs           # Local step dispatch through the core mutation path
└── mutation_http_client.rs        # HTTPS client for remote steps (SSRF-protected)

crates/fraiseql-server/src/federation/
├── circuit_breaker.rs             # Per-entity-type circuit breaker
└── health_checker.rs
```

## Architecture Layers

### Layer 1 — Schema Validation (compile time)

`fraiseql compile` reads the `[federation]` section of `fraiseql.toml` and:

1. Checks each entity's `key_fields` against the type it names: every field must exist,
   spelled as published, and an `embedded` type cannot be an entity
   (`crates/fraiseql-core/src/schema/compiled/federation_keys.rs`, #1395)
2. Checks `federation.circuit_breaker.per_database` entries name a declared entity and
   carry non-zero thresholds
3. Embeds the `federation` section (entities, keys, circuit breaker configuration) in
   `schema.compiled.json`

Loading a compiled schema runs the same key check, so a hand-edited artifact with a bad
key cannot reach a server.

```bash
fraiseql compile fraiseql.toml -o schema.compiled.json
fraiseql federation check schema.compiled.json --against other.compiled.json
```

### Layer 2 — Entity Resolution (runtime)

When a request selects `_entities`, the executor
(`crates/fraiseql-core/src/runtime/executor/support/federation.rs`):

1. Parses `_Any` scalars into typed `EntityRepresentation` objects
2. Refuses the batch if it holds more than `MAX_ENTITIES_BATCH_SIZE = 1000` representations
3. Applies the type's authorization and row filters
4. Groups representations by `__typename`
5. Resolves each group from this subgraph's database, through the executor's adapter
6. Returns results in input order and projects each entity against the requested selection

Every entity is resolved locally. Fields marked `@override(from: …)` are owned by this
subgraph and resolved here too; routing across subgraphs is the router's job.

```text
_entities(representations: [...])
    │
    ▼ CircuitBreaker check (per entity type, fraiseql-server)
    │
    ▼ batch_load_entities_enforced (fraiseql-federation)
    └── SQL lookup per __typename → this subgraph's database
```

### Layer 3 — Cross-Subgraph Mutations (fraiseql-saga)

Saga orchestration is the `fraiseql-saga` crate. It is a library: `fraiseql-server`,
`fraiseql-core` and the umbrella `fraiseql` crate do not depend on it, and there is no
`saga` Cargo feature. Adding the crate is the opt-in.

It provides forward execution over **local SQL** or **remote HTTPS**
(`SagaCoordinator` / `execute_saga`, with optional mTLS), **compensation** in reverse
order, on-restart **recovery** (`SELECT … FOR UPDATE SKIP LOCKED` leasing), per-step
**retry with backoff and timeout**, and cross-subgraph **`@requires` pre-fetch** through
`HttpEntityResolver`. A local step runs through
`Executor::execute_mutation_with_security`, so it passes the same authorization and
validation gates as any other mutation (#1354).

See [federation-saga.md](../guides/federation-saga.md) for the developer guide.

### Layer 4 — Circuit Breaker

Each entity type has an independent circuit breaker in `fraiseql-server`, configured from
the compiled `federation.circuit_breaker` section. While a type's breaker is open, an
`_entities` request naming that type is refused with a `circuit_breaker_open` error
carrying a retry hint.

See [circuit-breaker.md](../guides/circuit-breaker.md) for configuration and tuning.

## Data Flow: `_entities` Request

```text
Router → POST /graphql  { _entities(representations: [...]) { ... } }
    │
    ▼ GraphQL handler
    │  Parse + validate query
    │
    ├── Circuit breaker open for a named type → refused (circuit_breaker_open, retry hint)
    │
    ▼ Executor: execute_federation_query
    │
    └── Lookup SQL per __typename, built for this request → PostgreSQL
```

## Data Flow: Cross-Subgraph Mutation (Saga)

> Available to an embedder that uses `fraiseql-saga` directly. The `fraiseql-server`
> binary does not run sagas.

```text
Embedder code → SagaCoordinator.execute_saga()
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

- **Scoping an entity no query returns**: `_entities` reaches types that no root query
  returns (an owner-split `extend type … @key`, and every non-embedded type for
  SDK-synthesized sources). Such a type has no operation to carry `inject_params`, so it
  declares tenant/owner scoping on **itself** (#1142) — see
  [intermediate-schema.md](intermediate-schema.md#scoping-an-entity-no-query-returns-1142).
  Both `_entities` consumers read it behind the backing query: the gate that refuses an
  anonymous caller for a scoped type, and the per-row predicate builder. A type and its
  query declaring one column from two different sources is refused at load. `requires_role`
  has been read from the type since #1030.
- **SSRF protection**: `fraiseql-federation/src/http_resolver.rs` and
  `fraiseql-saga/src/mutation_http_client.rs` use `reqwest::Url::parse()` + private-IP
  rejection. IPv6 brackets are stripped before
  `IpAddr::parse()` to prevent bypass via `[::1]` notation.
- **Batch size cap**: `MAX_ENTITIES_BATCH_SIZE = 1000` in `representation.rs` and
  `entity_resolver.rs` refuses oversized `_entities` batches.
- **State isolation** (`fraiseql-saga`): saga state persists to the
  `tb_federation_sagas`, `tb_federation_saga_steps` and `tb_federation_saga_recovery`
  tables before each step, enabling on-restart recovery.
  `SagaRecoveryManager::run_iteration` / `start_background_loop` re-drive crash-interrupted sagas,
  claiming stuck sagas under a lease via `SELECT … FOR UPDATE SKIP LOCKED` so concurrent
  recovery workers never double-drive one.

## Observability

Prometheus metrics the server's `/metrics` endpoint adds when built with `federation`
(`crates/fraiseql-server/src/routes/metrics.rs`, `crates/fraiseql-federation/src/observability.rs`):

| Metric | Description |
|--------|-------------|
| `fraiseql_federation_circuit_breaker_state{entity}` | 0=closed, 1=open, 2=half-open |
| `fraiseql_federation_subgraph_latency_seconds{subgraph}` | Subgraph request latency histogram (`_bucket`, `_sum`, `_count`) |
| `fraiseql_federation_entity_resolution_total{outcome}` | Entity resolutions, `outcome="success"` or `"failure"` |

`fraiseql-saga` emits no metrics.

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

### Giving a gateway the subgraph's SDL

`_service { sdl }` is authorized like any other read: on a secured subgraph it is served
to the principals the subgraph serves anything to, and refused to an anonymous or unknown
caller. There is no setting that makes it public, by design (ADR-0016 §6: a permissive
case is named and opted into, and the subgraph's SDL lists every type, field and entity key).

**Composing offline** (a CI composition gate, `rover supergraph compose`) needs no running
subgraph and no principal: print exactly what `_service { sdl }` serves, from the artifact.

```bash
fraiseql federation sdl schema.compiled.json > users.graphql
```

**A gateway fetching SDL at runtime** authenticates as a
[service account](../adr/0018-service-account-identities.md): a named identity whose
secret lives in the environment, presented on `x-api-key`, with a ceiling that grants no
more than reading the schema needs. It is declared in the project `fraiseql.toml` read
beside `schema.json`:

```toml
# fraiseql.toml
[fraiseql.security.service_accounts.gateway]
secret_env = "FRAISEQL_SA_GATEWAY_SECRET"
roles      = []
scopes     = []
```

```bash
curl -s https://users.example.com/graphql \
  -H "x-api-key: $FRAISEQL_SA_GATEWAY_SECRET" -H 'content-type: application/json' \
  -d '{"query":"{ _service { sdl } }"}'
```

The gateway is then a named principal in the audit trail (`service_account:gateway`)
rather than an anonymous one. Where the subgraph requires an enriched identity before it
serves anything (ADR-0016), the account needs one, like any other principal.

## Related Docs

- [Federation Saga Guide](../guides/federation-saga.md)
- [Circuit Breaker Tuning](../guides/circuit-breaker.md)
- [Runbook: Circuit Breaker Tripped](../runbooks/14-federation-circuit-breaker.md)
- [ADR: Federation crate extraction](../adr/)
