# Phase 03: Integration Test Coverage

## Objective
Add comprehensive integration tests for all v2.2.0 features lacking end-to-end coverage, ensuring reliability and preventing regressions.

## Success Criteria
- [ ] Federated subscription passthrough integration test exists
- [ ] Schema metadata endpoint integration test exists
- [ ] Usage aggregation endpoint integration test exists
- [ ] Mutation audit tracing end-to-end test exists
- [ ] Federation plan visualization integration test exists
- [ ] GET /auth/me session-identity endpoint integration test exists
- [ ] Test harness infrastructure (WebSocket, HTTP mocks) implemented

> **PLAN REVIEW (2026-05-02):** Original 5 cycles are valid — no stale items found.
> One cycle added (Cycle 6) for the GET /auth/me endpoint, which was implemented in
> the sprint tracked in project memory (issue #193) but was not included in this plan.
> This is the most solid phase of the v2.3.0 plan.

## TDD Cycles

### Cycle 1: Subscription Forwarder Integration
- **RED**: Write failing integration test for WebSocket subscription forwarding to subgraph
- **GREEN**: Implement test harness with mock subgraph WebSocket server
- **REFACTOR**: Reuse harness for other federation tests
- **CLEANUP**: Verify test passes with real forwarder

### Cycle 2: Metadata Endpoint Integration
- **RED**: Write failing test for GET /api/v1/schema/metadata with auth
- **GREEN**: Add HTTP client integration test with schema loading
- **REFACTOR**: Test both auth-gated and public scenarios
- **CLEANUP**: Document endpoint behavior

> **REVIEW NOTE (2026-05-02):** This cycle is coupled to Phase 01 Cycle 2 (auth gating).
> The "both auth-gated and public scenarios" REFACTOR step should specifically cover the
> case where `introspection_require_auth = false` — confirming the endpoint is either
> explicitly safe or appropriately protected in that configuration.

### Cycle 3: Usage Aggregation Integration
- **RED**: Write failing test for GET /api/v1/admin/usage aggregating counters
- **GREEN**: Implement full pipeline: emit usage → aggregate → query
- **REFACTOR**: Test tenant isolation and persistence
- **CLEANUP**: Verify counters accurate

### Cycle 4: Audit Tracing End-to-End
- **RED**: Write failing test for mutation audit event emission to aggregation
- **GREEN**: Implement audit event pipeline test (emit → aggregate → query)
- **REFACTOR**: Test sanitization and log safety
- **CLEANUP**: Verify events traceable

> **REVIEW NOTE (2026-05-02):** The "sanitization" step in REFACTOR was originally
> motivated by Phase 01 Cycle 3 (audit log injection), which has been removed as a
> non-issue. This cycle is still valid — the audit pipeline itself needs E2E coverage —
> but the sanitization focus should be dropped from the REFACTOR step.

### Cycle 5: Federation Plan Visualization
- **RED**: Write failing test for GET /admin/v1/federation/plan
- **GREEN**: Add integration test with federation schema
- **REFACTOR**: Test plan generation and visualization
- **CLEANUP**: Document admin endpoint

### Cycle 6: GET /auth/me Session Identity — ADDED

> **REVIEW NOTE (2026-05-02) — MISSING FROM ORIGINAL PLAN:** The GET /auth/me endpoint
> was implemented (issue #193, tracked in project memory as 2026-04-12) after the v2.2.0
> finalization sprint but is absent from this plan. It introduced:
> - `OidcConfig.me: Option<MeEndpointConfig>`
> - `AuthenticatedUser.extra_claims: HashMap<String, serde_json::Value>`
> - Cookie fallback path (`__Host-access_token`) in `oidc_auth_middleware`
>
> No integration test for this endpoint appears to exist.

- **RED**: Write failing integration test for GET /auth/me with PKCE HttpOnly-cookie flow
- **GREEN**: Add test covering: valid cookie → 200 with sub/user_id/expires_at; invalid/missing → 401; extra claims filtered by allowlist
- **REFACTOR**: Test both header (`Authorization: Bearer`) and cookie fallback paths
- **CLEANUP**: Verify cookie name `__Host-access_token` enforced (not arbitrary cookie)

## Dependencies
- Requires: Phase 01 (security), Phase 02 (performance) complete
- Blocks: Phase 04 (debt), Phase 05 (finalize)

## Status
[~] In Progress — Cycles 2, 3, 4, 5 complete (2026-05-02); Cycles 1 & 6 deferred

### Implemented (v230_integration_tests.rs)
- **Cycle 2** (Metadata): 3 tests — envelope structure, empty schema, accessible without auth
- **Cycle 3** (Usage): 4 tests — empty aggregator, recorded events, tenant isolation, invalid period 400
- **Cycle 4** (Audit tracing): 3 tests — layer records tracing events, ignores wrong target, full pipeline
- **Cycle 5** (Federation plan): 2 tests — feature-gated (`#[cfg(feature = "federation")]`), returns 200 and 400
- `MutationAuditEvent::new(...)` constructor added (needed for external test crates due to `#[non_exhaustive]`)

### Deferred
- **Cycle 1** (Subscription forwarder): requires mock WebSocket subgraph server infrastructure
- **Cycle 6** (GET /auth/me): requires JWT token issuance in test context; see `auth_regression_test.rs`
