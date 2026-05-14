# Phase 3: Tenant Isolation Verification

## Objective

Verify that tenant context flows correctly through three paths: cross-validation in
`TenantKeyResolver`, subscription filtering, and rate-limit key composition.

## Success Criteria

- [ ] Conflicting tenant sources (JWT vs header) produce an error in strict mode
- [ ] Single source or agreeing sources pass normally
- [ ] WebSocket subscriptions store and filter by tenant_id
- [ ] Rate limiter composes keys with tenant_id when present
- [ ] Tenant resolution happens once per request (extension-based caching)

## TDD Cycles

### Cycle 1: Cross-validation logic

- **RED**: Test that when JWT says "tenant-a" and header says "tenant-b", `resolve(...,
  strict=true)` returns `Err`.
- **GREEN**: Verify the implementation in `tenant_key.rs` collects all sources and
  compares them.
- **REFACTOR**: Ensure the error message includes all conflicting values for debugging.
- **CLEANUP**: Lint.

### Cycle 2: Subscription tenant filtering

- **RED**: Write integration-style test: two subscriptions from different tenants; publish
  an event for tenant-a; assert only the tenant-a subscription receives it.
- **GREEN**: Verify the `handle_client_message` implementation checks `tenant_id` when
  dispatching events.
- **REFACTOR**: If the filtering is absent, implement it.
- **CLEANUP**: Lint.

### Cycle 3: Rate limit key composition

- **RED**: Test that `check_ip_limit("1.2.3.4", Some("tenant-x"))` and
  `check_ip_limit("1.2.3.4", Some("tenant-y"))` use independent buckets.
- **GREEN**: Verify `InMemoryRateLimiter::check_ip_limit` incorporates `tenant_id` into
  the bucket key (e.g., `format!("{}:{}", tenant_id, ip)`).
- **REFACTOR**: Ensure the format matches what `middleware_fn.rs` passes.
- **CLEANUP**: Lint.

### Cycle 4: Single-resolution per request

- **RED**: Verify that in the handler path, tenant resolution result is stored in request
  extensions and both the rate limiter and GraphQL handler read from the same source.
- **GREEN**: If double-resolution exists, refactor to resolve once in a preceding layer.
- **REFACTOR**: Document the request extension type.
- **CLEANUP**: Lint.

## Dependencies

- Phase 2 (cache isolation depends on tenant_id being correctly resolved)

## Status

[x] Complete
