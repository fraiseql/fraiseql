# Phase 01: Critical Security Fixes

## Objective
Eliminate critical and high-impact security vulnerabilities identified in the assessment, ensuring FraiseQL v2.3.0 is secure for production deployment.

## Success Criteria
- [x] ~~SSRF risk in subscription_forwarder.rs mitigated (IPv6/redirect validation)~~ — already done (see Cycle 1)
- [ ] Auth gating added to /api/v1/schema/metadata or confirmed public-safe
- [x] ~~Mutation audit events sanitized against log-format escapes~~ — not a real risk (see Cycle 3)
- [x] ~~Dependency advisory suppressions reviewed and deadlines set~~ — already done (see Cycle 4)

> **PLAN REVIEW (2026-05-02):** Of the original 4 success criteria, only 1 requires new work.
> The other 3 were either already resolved in prior sprints (S15–S19 / A+ audit) or were
> based on a mis-diagnosis. Details per cycle below.

## TDD Cycles

### ~~Cycle 1: Fix SSRF in Subscription Forwarder~~ — REMOVED (already implemented)

> **REVIEW NOTE (2026-05-02) — STALE:** Codebase inspection shows this is already fully
> implemented. Evidence:
> - `crates/fraiseql-federation/src/http_resolver.rs` — IPv6 brackets stripped before
>   `IpAddr::parse()`, RFC 1918 / ULA / link-local / loopback all blocked
> - redirect policy set to `Policy::none()` on the reqwest client
> - DNS rebinding checked via `dns_resolve_and_check()`
> - `validate_subgraph_url` imported and called in `subscription_forwarder.rs` line 112
>
> This was addressed in the S18-H3 sprint (federation SSRF URL parser fix).
> **No work required.**

### Cycle 2: Secure Metadata Endpoint
- **RED**: Write test verifying /api/v1/schema/metadata exposes sensitive metadata without auth
- **GREEN**: Add auth middleware to metadata endpoint or confirm metadata is public-safe
- **REFACTOR**: Consolidate auth middleware usage across API endpoints
- **CLEANUP**: Update documentation for endpoint security

> **REVIEW NOTE (2026-05-02) — VALID:** Confirmed in
> `crates/fraiseql-server/src/server/routing.rs`. The `/api/v1/schema/metadata` route
> is only auth-gated when `introspection_require_auth = true` — it shares the introspection
> flag with no independent auth control. A deployment that sets
> `introspection_require_auth = false` exposes schema metadata publicly.
> The endpoint and introspection auth should be independently configurable.

### ~~Cycle 3: Sanitize Audit Logging~~ — REMOVED (not a real risk)

> **REVIEW NOTE (2026-05-02) — INCORRECT DIAGNOSIS:** The plan assumes mutation names
> are user-controlled input. They are not. Mutation names come exclusively from the
> compiled schema definition (`schema.compiled.json`), not from query input at runtime.
> Inspection of `crates/fraiseql-server/src/usage/layer.rs` and
> `crates/fraiseql-core/src/runtime/executor/mutation.rs` confirms the name is extracted
> from the schema registry, not the raw query string.
>
> There is no injection surface here. **No work required.**

### Cycle 4: Review Dependency Advisories — SCOPE REDUCED

> **REVIEW NOTE (2026-05-02) — PARTIALLY DONE:** `deny.toml` already has all 16 wasmtime
> advisories suppressed with `// Revisit by 2026-08-01` deadlines. The "set deadlines"
> task is complete. What remains is deciding whether any advisory can be closed by
> upgrading wasmtime now.

- **RED**: Check if any of the 16 suppressed wasmtime advisories have a fix available in a newer version
- **GREEN**: Upgrade wasmtime if the upgrade is safe and eliminates advisories; otherwise confirm current suppressions are sufficient
- **REFACTOR**: *(skip — deny.toml format is already clean)*
- **CLEANUP**: Confirm `cargo deny check` passes; log upgrade decision in deny.toml comment

## Dependencies
- Requires: None (can start immediately)
- Blocks: Phase 02 (performance), Phase 03 (tests)

## Status
[x] Complete — Cycle 2 was already implemented in commit 02081b700; Cycle 4 completed 2026-05-02 (wasmtime 18→44, 16 CVEs eliminated)
