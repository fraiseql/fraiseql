# Phase 11: Multi-Tenancy

## Objective

Ship a production-grade multi-tenant execution model that allows a single
FraiseQL process to serve isolated GraphQL APIs for N tenants, each with their
own schema and database connection pool.

## Status

[ ] Not Started

## Background

SpecQL's platform model (see `~/code/specql/.phases/20260428-remove-axum/`)
provisions one FraiseQL instance *per deployment*. That is expensive at scale:
each free-tier user costs ~$12.55/month at low scale. Multi-tenancy collapses
N idle instances into one process, reducing per-free-user cost to ~$6–8
(40% reduction). This is the primary cost lever for SpecQL's free tier.

The architecture was reviewed and finalized in `memory/fraiseql_mt_review.md`.
Key decisions:

- `TenantExecutorRegistry` — couples schema + connection per tenant
- Strict tenant validation — 403 on unregistered explicit keys
- PUT-as-upsert management API
- All 5 high/medium review issues addressed

## Success Criteria

- [ ] Single FraiseQL process serves ≥100 tenants in integration test
- [ ] Tenant isolation: query for tenant A cannot access tenant B's data
- [ ] Hot-reload: adding/removing a tenant does not restart the process
- [ ] Management API: `PUT /admin/tenants/{id}`, `DELETE /admin/tenants/{id}`,
      `GET /admin/tenants`
- [ ] `cargo nextest run --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] Benchmarks: multi-tenant RPS within 5% of single-tenant at 10 tenants

---

## TDD Cycles

### Cycle 1: `TenantExecutorRegistry` core type

**Crate**: `fraiseql-core`  
**New file**: `src/runtime/tenant_registry.rs`

**RED**:

- `registry_returns_executor_for_registered_tenant`
- `registry_returns_403_for_unregistered_tenant`
- `registry_is_send_sync` — `assert_send_sync::<TenantExecutorRegistry<_>>()`
- `registry_supports_concurrent_reads` — Tokio join_all with 50 concurrent lookups

**GREEN**:
```rust
pub struct TenantExecutorRegistry<A: DatabaseAdapter> {
    tenants: Arc<DashMap<TenantId, Arc<TenantExecutor<A>>>>,
}

pub struct TenantExecutor<A: DatabaseAdapter> {
    pub schema: Arc<CompiledSchema>,
    pub adapter: A,
    pub config: TenantConfig,
}
```

- `TenantId`: newtype over `String`, validates `[a-z0-9-]{1,63}`
- `get(id) -> Result<Arc<TenantExecutor<A>>, FraiseQLError>` returns
  `FraiseQLError::TenantNotFound` (maps to 403) for unknown tenants
- `register(id, executor)`, `unregister(id)`, `list() -> Vec<TenantId>`

**REFACTOR**: Ensure `TenantId` validation is shared with the management API
handler (Cycle 4) via a common validator.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 2: Tenant-aware request routing

**Crate**: `fraiseql-server`

**RED**:

- `graphql_request_routed_to_correct_tenant` — two tenants with different schemas,
  request with `X-Tenant-Id: tenant-a` resolves tenant-a's type, not tenant-b's
- `graphql_request_without_tenant_id_uses_default` — backward compat: single-tenant
  mode still works without header
- `graphql_request_for_unknown_tenant_returns_403`

**GREEN**:

- Extract `TenantId` from `X-Tenant-Id` header (or subdomain, configurable)
- Thread `TenantExecutorRegistry` through `AppState` alongside the existing
  single-tenant `executor` field
- `Server::new_multi_tenant(registry)` builder path; `Server::new(adapter)`
  remains the single-tenant path (backward compat)
- Handler selects executor: registry lookup if multi-tenant mode, direct
  executor if single-tenant

**REFACTOR**: The single-tenant path must not regress in performance. Use a
`TenantMode` enum (`Single(Arc<Executor>)` | `Multi(Arc<TenantExecutorRegistry>)`)
to avoid an Option branch on every request.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 3: Tenant isolation enforcement (RLS)

**Crate**: `fraiseql-core`

**RED**:

- `tenant_a_query_cannot_read_tenant_b_rows` — integration test with two tenants
  sharing a PostgreSQL database, each with a dedicated schema; verify no cross-
  contamination
- `rls_security_context_carries_tenant_id` — assert `SecurityContext` includes
  `tenant_id` and it is injected into all SQL via `set_config`

**GREEN**:

- Extend `SecurityContext` with `tenant_id: Option<TenantId>`
- `inject_params` injects `set_config('app.tenant_id', ...)` before every query
  when tenant_id is present
- Document the PostgreSQL RLS policy pattern operators should use:
  ```sql
  CREATE POLICY tenant_isolation ON tb_users
    USING (tenant_id = current_setting('app.tenant_id'));
  ```

**REFACTOR**: `tenant_id` in `SecurityContext` is optional to preserve
single-tenant backward compat. Multi-tenant mode makes it required — enforce
at registry lookup time, not at SQL execution time.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 4: Tenant management API

**Crate**: `fraiseql-server`

**RED**:

- `put_tenant_registers_new_tenant` — PUT creates a new tenant
- `put_tenant_updates_existing_tenant` — PUT on existing id replaces schema
- `delete_tenant_removes_from_registry`
- `list_tenants_returns_all_registered_ids`
- `put_tenant_with_invalid_schema_returns_400`
- `put_tenant_with_invalid_id_returns_400` — id with uppercase or spaces

**GREEN**:

- `PUT /admin/tenants/{id}` — body: `{ "schema": {...}, "database_url": "..." }`
  upserts into registry; validates schema before accepting
- `DELETE /admin/tenants/{id}` — gracefully drains in-flight requests before
  removing (or immediately removes with 503 for in-flight — document tradeoff)
- `GET /admin/tenants` — returns `[{ "id": "...", "schema_version": "...", "registered_at": "..." }]`
- All endpoints require admin API key (`X-Admin-Key` header)

**REFACTOR**: Reuse `reload_schema_handler` validation logic for the schema
validation step in `put_tenant`.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 5: Benchmarks + hot-reload stress test

**Crate**: `fraiseql-server` (benches)

**RED**:

- `bench_multi_tenant_10_tenants` — assert ≤5% RPS regression vs single-tenant
- `stress_hot_reload_no_requests_dropped` — concurrent requests while adding/
  removing tenants; assert zero errors during reload

**GREEN**: Fix any performance regressions exposed by benchmarks. Common cause:
unnecessary cloning of `Arc<CompiledSchema>` on every request — use a reader
lock or `Arc::clone` only at registration time.

**REFACTOR**: If `DashMap` contention is measurable, consider sharding the
registry by tenant ID prefix.

**CLEANUP**: Add benchmark results to `docs/benchmarks/multi-tenancy.md`.

---

## Dependencies

- Requires: Phase 10 complete (security baseline stable before adding complexity)
- Blocks:
  - SpecQL `20260428-remove-axum/` Phase 03 (provisioning loop polls FraiseQL
    tenant management API)
- SpecQL coordination: SpecQL provisions tenants via `PUT /admin/tenants/{id}`
  in its provisioning daemon. The schema body format must match SpecQL's
  compiled `schema.json` output exactly (no rename — SpecQL uses `schema.json`).

## Version target

v2.3.0 (feature on `dev`; does not ship to `main` until v2.3.0 release cut)
