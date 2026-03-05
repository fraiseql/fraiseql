# FraiseQL — Master Remediation Plan 2

> Issues identified by independent post-v2.0.0 "rapport d'étonnement".
> Ordered by risk, then dependency. Batches can be parallelized unless
> a **Requires** note is present.

Severity: 🔴 Critical · 🟠 High · 🟡 Medium · 🔵 Low
Status:   ✅ Done · ❌ Blocked · 🔄 In progress · (blank) Pending

---

## BATCH 1 — Thread safety: `std::thread::sleep` in async test contexts

**Risk**: Tokio thread pool starvation during test runs; test suite becomes
non-deterministically slow; can mask real deadlocks.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| TS-1  | 🔴  | ✅     | `std::thread::sleep(1100ms)` inside `#[tokio::test]` blocks in `pkce.rs` — blocks one tokio worker for 1.1 s per call | `crates/fraiseql-auth/src/pkce.rs:485,579` |
| TS-2  | 🟡  | ✅     | `std::thread::sleep(10ms)` in sync tests that measure `OperationTimer` — safe but slow; replace with `Instant` delta assertion | `crates/fraiseql-auth/src/monitoring.rs:235` |
| TS-3  | 🟡  | ✅     | `std::thread::sleep(10ms)` in sync `PerformanceTimer` test — same issue | `crates/fraiseql-server/src/performance.rs:512` |
| TS-4  | 🟡  | ✅     | `std::thread::sleep(100µs)` in `TimingGuard` test — negligible but sets wrong precedent | `crates/fraiseql-server/src/metrics_server.rs:652` |
| TS-5  | 🟡  | ✅     | `std::thread::sleep(2s)` in `rate_limiter_time_tests.rs` — sync tests, not thread-pool-blocking, but adds 4 s to every CI run; replace with clock injection (see Batch 2) | `crates/fraiseql-auth/tests/rate_limiter_time_tests.rs:39,64` |

See `batches/batch-1-thread-safety.md` for fix patterns.

---

## BATCH 2 — Clock injection

**Risk**: Components calling `SystemTime::now()` directly cannot be unit-tested
without real-time delays; window/TTL logic has no deterministic test coverage;
CI time grows as test suite grows.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| CK-1  | 🟠  | ✅     | `rate_limiting.rs` calls `SystemTime::now()` inline — no way to test window rollover without sleeping | `crates/fraiseql-core/src/validation/rate_limiting.rs:105` |
| CK-2  | 🟠  | ✅     | `cache/result.rs` `current_time_secs()` calls `SystemTime::now()` inline — TTL expiry not unit-testable | `crates/fraiseql-core/src/cache/result.rs:593` |
| CK-3  | 🟠  | ✅     | `rls_policy.rs` uses `SystemTime::now()` for cache expiry in three sites — cache correctness not deterministically testable | `crates/fraiseql-core/src/security/rls_policy.rs:308,321,339` |
| CK-4  | 🟡  | ✅     | `pkce.rs` store uses wall clock for TTL — once TS-1 fixed, tests still need `tokio::time::pause()` + `advance()` to avoid any sleep | `crates/fraiseql-auth/src/pkce.rs` (store impl) |
| CK-5  | 🟡  | ✅     | `kms/base.rs` `current_timestamp_secs()` calls `SystemTime::now()` — key rotation scheduling not unit-testable | `crates/fraiseql-core/src/security/kms/base.rs:19` |

See `batches/batch-2-clock-injection.md` for the `Clock` trait design and rollout plan.

---

## BATCH 3 — Security regression tests

**Risk**: The four most severe bug classes from Campaign 1 (auth bypass,
SQL injection, webhook algorithm errors, PKCE CSRF) have no dedicated
regression test. A future refactor could silently reintroduce them.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| SR-1  | 🔴  | ✅     | Add test: GET GraphQL endpoint must return 401/403 when `Authorization` is absent and RLS is enabled — directly regresses E1 | `crates/fraiseql-server/tests/auth_regression_test.rs` (new) |
| SR-2  | 🔴  | ✅     | Add test: tenant ID supplied as `; DROP TABLE` (and variants) must never appear in executed SQL — directly regresses AA1 | `crates/fraiseql-core/tests/tenancy_sql_injection_test.rs` (new) |
| SR-3  | 🔴  | ✅     | Add Twilio webhook fixture test: replay a known Twilio `X-Twilio-Signature` request through `TwilioVerifier`; must pass. Forge with wrong body; must fail | `crates/fraiseql-webhooks/tests/twilio_replay_test.rs` (new) |
| SR-4  | 🔴  | ✅     | Add SendGrid webhook fixture test: replay known ECDSA P-256 `X-Twilio-Email-Event-Webhook-Signature` payload through `SendGridVerifier` | `crates/fraiseql-webhooks/tests/sendgrid_replay_test.rs` (new) |
| SR-5  | 🟠  | ✅     | Add Slack replay test: valid `X-Slack-Signature` at age 0 must pass; same signature at age > 300 s must fail with `TimestampExpired` | `crates/fraiseql-webhooks/tests/slack_replay_test.rs` (new) |
| SR-6  | 🟠  | ✅     | Add Discord replay test: same pattern as SR-5 | `crates/fraiseql-webhooks/tests/discord_replay_test.rs` (new) |
| SR-7  | 🟠  | ✅     | Add PKCE CSRF test: `create_state()` returns the state token; `consume_state()` with mismatched token must return `StateNotFound` | `crates/fraiseql-auth/tests/pkce_csrf_regression_test.rs` (new) |
| SR-8  | 🟠  | ✅     | Add test: RBAC router must return 401 without auth header; 403 with wrong token | `crates/fraiseql-server/tests/rbac_auth_regression_test.rs` (new) |

See `batches/batch-3-security-regressions.md` for fixture design and exact assertions.

---

## BATCH 4 — SDK audit and parity

**Risk**: 11 SDK CI workflows were added in a single batch commit. Some may run
build-only checks with no meaningful test coverage, creating a false confidence
signal. Community SDKs have minimal tests.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| SDK-1 | 🟠  | ✅     | Audit each official SDK CI workflow: verify `test` step runs at least one functional test (not just `dotnet build` or `go build`). Document findings in `infrastructure/sdk-parity-matrix.md` | All 11 `.github/workflows/*-sdk.yml` |
| SDK-2 | 🟠  | ✅     | Go SDK: already has 8 `*_test.go` files (golden, completeness, export, etc.) and CI runs `go test -v -race ./...` — no action required | `sdks/official/fraiseql-go/` |
| SDK-3 | 🟡  | ✅     | Community SDKs (clojure, groovy, kotlin, scala, swift, nodejs): added `schema_roundtrip_test` to each — exercises full decorator → JSON export pipeline with golden assertions | `sdks/community/*/tests/` |
| SDK-4 | 🟡  | ✅     | Elixir and Dart archived from `community/` to `sdks/archived/`; official/ versions are authoritative; `sdks/archived/README.md` explains deprecation | `sdks/archived/` |
| SDK-5 | 🔵  | ✅     | Cross-SDK parity CI job added: generates schema from Python and TypeScript SDKs, performs structural comparison (type/query/mutation names and fields) | `.github/workflows/sdk-parity.yml` |

See `batches/batch-4-sdk-audit.md` for per-SDK findings and required test patterns.

---

## BATCH 5 — Crate architecture: fraiseql-core split

**Risk**: `fraiseql-core` at 107,712 lines (actual, 2026-03-05 audit) across a
single crate makes incremental compilation ineffective. Any change to `db/`
forces recompilation of `cache/`, `graphql/`, etc.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| CA-1  | 🟡  | ❌     | Extract `crates/fraiseql-core/src/db/` into `crates/fraiseql-db/` — **BLOCKED**: `db/traits.rs` imports `compiler::aggregation::OrderByClause` and `schema::SqlProjectionHint` (used in 30+ files); these must be relocated first to avoid circular dependency | New crate |
| CA-2  | 🟡  | ❌     | Extract `crates/fraiseql-core/src/runtime/executor/` into `crates/fraiseql-executor/` — **BLOCKED**: circular dependency between `cache/adapter/` and `runtime/executor/` requires trait abstraction before split | New crate |
| CA-3  | 🔵  | ❌     | Update `fraiseql-core/Cargo.toml` — deferred pending CA-1/CA-2 | `crates/fraiseql-core/Cargo.toml` |
| CA-4  | 🔵  | ✅     | Added `[workspace.metadata.crate-size-budget]` to `Cargo.toml` and `tools/check-crate-sizes.sh`; all 12 crates pass; fraiseql-core budgeted at 150K (split threshold) | `tools/check-crate-sizes.sh`, `Cargo.toml` |

See `batches/batch-5-crate-split.md` for dependency graph analysis and migration steps.

---

## BATCH 6 — Deprecation enforcement

**Risk**: `observers-full` feature alias is marked deprecated with a target
removal version (2.3.0) but emits no compile-time signal; users will be
surprised at removal.

| ID    | Sev | Status | What | Where |
|-------|-----|--------|------|-------|
| DA-1  | 🟡  |        | Emit `cargo::warning=` from `fraiseql-server`'s `build.rs` when `observers-full` feature is active, explaining the migration path to `observers-enterprise` | `crates/fraiseql-server/build.rs` |
| DA-2  | 🔵  |        | Add `CHANGELOG` note and `docs/migrations/observers-full-removal.md` with exact version timeline and sed one-liner for dependents | `docs/migrations/` (new) |

See `batches/batch-6-deprecation.md`.

---

## BATCH 7 — Blocked / tracked externally

| ID    | Sev | Status | What | Blocker |
|-------|-----|--------|------|---------|
| V3    | 🟠  | ❌     | `rustls 0.21.12` EOL CVE GHSA-6g18-jhpc-69jc (RSA-PSS) — transitive via `aws-sdk-s3` → `hyper-rustls 0.24.2` | AWS SDK must update to `hyper-rustls ≥ 0.25`; track at https://github.com/awslabs/aws-sdk-rust/issues |

**Action required**: Set a calendar reminder for 2026-06-05 (90 days). If AWS
SDK has not resolved by then, evaluate replacing `aws-sdk-s3` with `object_store`
(which uses `rustls 0.23`). See `batches/batch-7-blocked.md`.

---

## Infrastructure (non-issue, process/tooling)

These items do not fix specific bugs but prevent the next campaign from being
necessary.

| ID    | Priority | What | Document |
|-------|----------|------|----------|
| INF-1 | 🟠 High  | Pre-release security checklist — mandatory sign-off before any `v*.*.0` tag | `infrastructure/pre-release-security-checklist.md` |
| INF-2 | 🟠 High  | Security review gate — PRs touching auth/webhooks/tenancy require review checklist | `infrastructure/security-review-gate.md` |
| INF-3 | 🟠 High  | Test quality standards — policy banning `std::thread::sleep` in `#[tokio::test]`, requiring clock injection for time-dependent logic | `infrastructure/test-quality-standards.md` |
| INF-4 | 🟡 Med   | SDK parity matrix — living document tracking which features each SDK tests | `infrastructure/sdk-parity-matrix.md` |
| INF-5 | 🟡 Med   | Crate size policy — size budgets, enforcement script, review process for splits | `infrastructure/crate-size-policy.md` |
| INF-6 | 🟡 Med   | Threat modeling process — one-page template required for every new feature PR | `infrastructure/threat-modeling-process.md` |

---

## Summary by severity

| Severity | Count | Items |
|----------|-------|-------|
| 🔴 Critical | 7 | TS-1, SR-1, SR-2, SR-3, SR-4 |
| 🟠 High    | 12 | CK-1..5, SR-5..8, SDK-1..2, V3 |
| 🟡 Medium  | 12 | TS-2..5, CA-1..4, DA-1, SDK-3..4, INF-1..3 |
| 🔵 Low      | 5  | SDK-5, CA-4, DA-2, INF-4..6 |
| **Total**  | **36** | |
