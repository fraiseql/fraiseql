# Phase 13: Federation Maturity (v2.2.0 Core)

## Objective

Make FraiseQL a production-grade Apollo Federation 2 subgraph participant:
correct entity resolution, federated query planning, field-level authorization
at federation boundaries, and multi-service distributed tracing.

## Status

[ ] Not Started

## Background

v2.2.0 is named "Federation Maturity" in the roadmap. The `fraiseql-federation`
crate already exists (extracted in WP-2) with circuit breaker, saga executor,
and basic federation support. This phase fills the gaps for full Apollo
Federation 2 spec compliance and production observability.

The known v2.1.x limitation to fix here is the cache mutation routing overhead:
mutations routing through `CachedDatabaseAdapter` add ~15% overhead due to
coarse-grained view-level invalidation. The fix is targeted eviction — covered
in Phase 14 (this phase), which can run in parallel with Cycles 4–5 here.

## Success Criteria

- [ ] FraiseQL passes Apollo Federation 2 integration test suite
- [ ] `@key`, `@external`, `@requires`, `@provides` directives supported in
      compiled schema
- [ ] Entity resolution `_entities` query works for types with `@key`
- [ ] Federated query plan logs show which subgraph resolved each field
- [ ] Distributed trace spans cross subgraph boundaries with correct parent IDs
- [ ] Field-level `@requiresScope` enforced at federation boundary
- [ ] `cargo nextest run --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean

---

## TDD Cycles

### Cycle 1: Apollo Federation 2 spec compliance — directives

**Crate**: `fraiseql-federation`, `fraiseql-cli`  
**Files**: schema compilation + federation directive handling

**RED**:

- `compile_key_directive_emits_entity_resolver` — schema with `@key(fields: "id")`,
  assert compiled output contains an `_entities` resolver entry
- `compile_external_directive_marks_field` — `@external` field not included in
  local SQL SELECT
- `compile_requires_directive_validates_dependencies` — `@requires(fields: "org { id }")`,
  assert compilation fails if `org` is not `@external`
- `compile_provides_directive_records_provided_fields`

**GREEN**:

- Add `FederationDirective` enum to the compiler IR: `Key`, `External`,
  `Requires`, `Provides`
- CLI compiler: parse federation directives from schema source and embed in
  `compiled.json` under `"federation": { "entities": [...], "directives": [...] }`
- Runtime: `fraiseql-federation/src/entity_resolver.rs` reads the compiled
  federation config to build the `_entities` query handler

**REFACTOR**: Federation directives were previously carried as
`CompiledSchema.federation: Option<serde_json::Value>` (opaque JSON). Give them
proper types: `FederationConfig { entities: Vec<EntityConfig>, ... }`.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 2: Entity resolution (`_entities` query)

**Crate**: `fraiseql-federation`

**RED**:

- `entities_query_resolves_user_by_key` — POST `{ _entities(representations: [{ __typename: "User", id: "1" }]) }`,
  assert returns `User { id: "1", name: "Alice" }`
- `entities_query_rejects_unknown_typename` — `__typename: "Ghost"`, assert 400
- `entities_query_enforces_batch_limit` — >1000 representations, assert 400
  (reuses `MAX_ENTITIES_BATCH_SIZE` from S18-H4)
- `entities_query_returns_null_for_missing_key` — key not in DB, assert
  `null` in result (not error — Federation spec requires null for missing entities)

**GREEN**:

- `EntityResolver::resolve(representations: Vec<Representation>) -> Vec<Option<Value>>`
- For each representation: extract `__typename` + key fields, look up SQL
  template from compiled federation config, execute, return result or null
- Wire into GraphQL execution: `_entities` field handled by `EntityResolver`
  before normal query execution

**REFACTOR**: Batch the SQL lookups: group representations by `__typename`,
execute one `SELECT ... WHERE id = ANY($1)` per type rather than N individual
queries.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 3: Subgraph schema generation (`_service` query)

**Crate**: `fraiseql-federation`

**RED**:

- `service_query_returns_sdl` — POST `{ _service { sdl } }`, assert returns
  valid SDL string
- `service_sdl_includes_key_directives` — `@key(fields: "id")` present in SDL
- `service_sdl_is_valid_graphql` — parse returned SDL, assert no parse errors

**GREEN**:

- `_service { sdl }` is a required Apollo Federation endpoint
- Generate SDL from compiled schema + federation config
- Include `extend schema @link(...)` for Apollo Federation 2 link directives
- Register `_service` as a built-in query in the GraphQL executor

**REFACTOR**: SDL generation and schema introspection share logic — extract
`SchemaRenderer::to_sdl(&self, include_federation_directives: bool)`.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 4: Field-level authorization at federation boundaries

**Crate**: `fraiseql-federation`

**RED**:

- `federation_boundary_enforces_require_scope` — a field with `@requiresScope("admin")`
  on a type resolved via `_entities`; request with non-admin JWT, assert 403
- `federation_boundary_propagates_security_context` — the `SecurityContext`
  (JWT claims) from the gateway request reaches the subgraph entity resolver

**GREEN**:

- Federation gateway must forward `Authorization` header or a signed security
  context token to subgraphs
- `EntityResolver::resolve` receives `SecurityContext` and checks `@requiresScope`
  on each resolved field before returning
- Document the expected header forwarding pattern for Apollo Router

**REFACTOR**: `@requiresScope` enforcement is already in the normal query path;
extract the shared `check_field_authorization(field, context)` fn so it works
for both paths.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 5: Multi-service distributed tracing

**Crate**: `fraiseql-federation`, `fraiseql-server`

**RED**:

- `trace_span_crosses_subgraph_boundary` — integration test with two FraiseQL
  instances; assert the child span has the correct `trace_id` from the parent
- `trace_includes_subgraph_name` — span attributes include `fraiseql.subgraph`
- `trace_includes_entity_type` — `_entities` resolver span includes `__typename`

**GREEN**:

- Extract `traceparent` / `tracestate` from incoming request headers (W3C
  Trace Context spec)
- Forward them in outbound federation HTTP calls
- Add spans: `fraiseql.federation.resolve_entities`, one child span per
  `__typename` batch
- Attribute: `fraiseql.subgraph = <service_name>`, `entity.type = <typename>`,
  `entity.count = N`

**REFACTOR**: The OTel instrumentation for federation should be opt-in via the
`opentelemetry` Cargo feature, same as the existing tracing in `fraiseql-server`.

**CLEANUP**: Clippy, fmt, doc. Add `GET /admin/metrics` documentation for new
`fraiseql_federation_*` Prometheus counters/histograms.

---

### Cycle 6: Apollo Federation integration test suite

**Crate**: `fraiseql-federation/tests/`

**RED** (all integration, `#[ignore = "requires docker"]`):

- `apollo_federation_smoke_test` — spin up a FraiseQL subgraph + Apollo Router
  via docker-compose, run a federated query, assert correct response
- `federation_handles_partial_failure` — one subgraph down, assert gateway
  returns partial response with errors array, not a 500
- `federation_circuit_breaker_opens_on_repeated_failure`

**GREEN**: Write the docker-compose fixture (`tests/fixtures/federation-smoke/`)
and the test harness. The circuit breaker logic already exists — wire the test.

**CLEANUP**: Clippy, fmt, doc. Add `make test-federation` Makefile target.

---

## Dependencies

- Requires: Phase 11 complete (multi-tenant executor stable — federation entity
  resolution builds on the same registry pattern)
- Blocks: v2.2.0 release (this is the headline feature)
- SpecQL coordination:
  - SpecQL platform-gaps P12 (observability API) needs `GET /admin/metrics`
    JSON with federation metrics from Cycle 5
  - SpecQL schemas using FraiseQL as a subgraph need no changes — SpecQL
    generates standard `schema.json`, federation directives would be added
    as new TOML keywords in a future SpecQL alignment phase

## Version target

v2.3.0
