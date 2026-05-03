# Security Remediation — Verification & Hardening

## Context

Branch `fix/security-remediation-2026-05` contains 7 feature commits implementing the
security remediation plan (P0–P3 + defence-in-depth mTLS). The code compiles and clippy
passes, but the implementing agent never ran tests before committing, leading to cascading
breakage that required a full-workspace fix pass. The logic correctness of the
implementations has not been verified beyond "it compiles."

## Current State

- `cargo clippy --workspace --all-targets --exclude fraiseql-storage -- -D warnings` CLEAN
- All non-Docker tests pass (73 test suites)
- 5 Docker-dependent tests (`error_real_cache.rs`) fail due to sandbox networking — these
  are the testcontainer-based integration tests that exercise the cache adapter against a
  real PostgreSQL instance

## Known Risk Areas

1. **Schema integrity hash mismatch** — The CLI and runtime use different serialization
   paths (struct→JSON vs Value→JSON). We aligned them via canonical Value round-trip, but
   this needs verification against real `.compiled.json` files produced by the CLI in CI.

2. **Cache RLS guard** — The `has_rls: bool` field was added to `CachedDatabaseAdapter`
   but its initialization path needs auditing (who sets it? is it always correct?).

3. **Tenant cross-validation** — `TenantKeyResolver::resolve` now takes `strict: bool`
   and `domain_registry: Option<&DomainRegistry>`, but the callers in production paths
   (handler, subscriptions) may not pass the correct strictness mode.

4. **WebSocket tenant isolation** — `handle_client_message` now receives `tenant_id` but
   the filtering/enforcement logic inside needs verification.

5. **Rate limit tenant key** — The `tenant_id` parameter flows through but may not be
   wired into actual key composition in `InMemoryRateLimiter`.

6. **mTLS** — `tls.rs` exists but integration with `HttpEntityResolver` and actual TLS
   handshake correctness is untested (no integration test infrastructure for mTLS).

## Phases

| Phase | File | Goal | Status |
|-------|------|------|--------|
| 1 | [phase-01-schema-integrity.md](phase-01-schema-integrity.md) | Verify hash round-trip end-to-end | [ ] |
| 2 | [phase-02-cache-rls.md](phase-02-cache-rls.md) | Audit RLS guard initialization and key composition | [ ] |
| 3 | [phase-03-tenant-isolation.md](phase-03-tenant-isolation.md) | Verify tenant cross-validation, subscription filtering, rate limiting | [ ] |
| 4 | [phase-04-mtls-integration.md](phase-04-mtls-integration.md) | Integration test for mTLS handshake with reqwest | [ ] |
| 5 | [phase-05-finalize.md](phase-05-finalize.md) | Squash history, merge to dev | [ ] |
