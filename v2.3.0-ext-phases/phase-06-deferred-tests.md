# Phase 06: Deferred Integration Test Infrastructure

## Objective
Implement the two integration test cycles deferred from Phase 03 that require
non-trivial test harnesses: WebSocket subscription forwarding and `GET /auth/me`
with PKCE cookie flow.

## Success Criteria
- [ ] Subscription forwarder integration test passes against mock subgraph
- [ ] `GET /auth/me` integration test covers valid cookie, missing cookie, and extra-claims paths
- [ ] Both tests run in CI without external infrastructure

## TDD Cycles

### Cycle 1: Subscription Forwarder Integration Test

The federation subscription forwarder (`SubscriptionForwarder` in
`crates/fraiseql-server/src/subscriptions/forwarder.rs`) proxies WebSocket
connections to subgraph servers. No integration test exists.

**What needs to exist first:**
- A `MockSubgraphServer` that listens on a random port, performs the GraphQL-WS
  handshake (`connection_init` / `connection_ack`), and emits pre-configured
  subscription events
- The mock must be self-contained (tokio task, `oneshot` shutdown)

- **RED**: Write failing test `test_forwarder_proxies_subscription_events` — asserts
  that events emitted by the mock subgraph arrive at the client side via the forwarder
- **GREEN**: Implement `MockSubgraphWsServer` in `tests/common/mock_subgraph.rs`;
  wire up the forwarder; confirm events flow
- **REFACTOR**: Extract harness into `fraiseql-test-utils` if reusable elsewhere
- **CLEANUP**: Confirm test runs in CI (no external ports required — uses `0.0.0.0:0`)

### Cycle 2: `GET /auth/me` Integration Test

The `GET /auth/me` endpoint (`crates/fraiseql-server/src/routes/auth/me.rs`) was
implemented in issue #193. It returns the session identity from an HttpOnly-cookie
PKCE flow. No integration test exists.

**What needs to exist first:**
- A way to mint a valid JWT in test context (use `jsonwebtoken` crate with an
  in-memory RSA/HMAC key — no real OIDC provider needed)
- A way to set the `__Host-access_token` cookie on a test request

**Key scenarios to cover:**
1. Valid `__Host-access_token` cookie → 200 with `sub`, `user_id`, `expires_at`
2. Valid `Authorization: Bearer` header (fallback) → same 200
3. Missing/invalid cookie + no header → 401
4. Extra claims present → filtered by `expose_claims` allowlist
5. Cookie name must be `__Host-access_token` exactly (not arbitrary cookie name)

- **RED**: Write failing tests for the 5 scenarios above
- **GREEN**: Implement JWT-minting helper in `tests/common/jwt_helper.rs`; wire
  `MeEndpointConfig` with an `expose_claims` allowlist; run tests
- **REFACTOR**: Share JWT helper with any future auth-related test cycles
- **CLEANUP**: Confirm tests pass with `#[cfg(not(feature = "oidc-integration"))]`
  gate so no real OIDC provider is required

## Dependencies
- Requires: Phase 03 complete (already done)
- Blocks: Phase 10 (finalize)

## Status
[ ] Not Started
