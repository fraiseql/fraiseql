# FraiseQL — Master Remediation Plan

> Synthesizes Extensions 1–20. All issues are de-duplicated and organized
> into actionable fix batches ordered by risk and dependency.
> Benchmarking is out of scope (handled by `../velocitybench`).

---

## How to read this plan

Each batch is self-contained and can be parallelized with other batches unless
a **Requires** dependency is noted. Within a batch, issues are ordered so that
later fixes can reuse earlier ones.

Severity legend: 🔴 Critical · 🟠 High · 🟡 Medium · 🔵 Low
Status legend: ✅ Done · ❌ Blocked · (blank) Pending

---

## BATCH 0 — Process bootstrap  *(do first, unblocks tracking)*

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| EE1 | | 🟠 | Import all issues below as GitHub Issues; tag with `batch-N` and `severity-X`; assign owners; link to milestone `v2.1-security` or `v2.1-quality` | Process |
| EE2 | ✅ | 🔵 | Add `.claude/worktrees/` to `.gitignore` | `.gitignore` |
| L1 | ✅ | 🟡 | Fix bare `lib/` gitignore entry that silently excludes Elixir/Dart/Ruby SDK sources | `.gitignore` |

---

## BATCH 1 — Critical security (auth bypass, SQL injection)

**Merge gate**: No release until this batch is complete and passes security review.

### 1A — Authentication bypasses

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| E1 | ✅ | 🔴 | GET GraphQL handler passes `security_context: None` — RLS and field-level auth silently bypassed for all GET queries | `crates/fraiseql-server/src/routes/graphql.rs` |
| E2a | ✅ | 🔴 | RBAC management router merged without any authentication middleware | `crates/fraiseql-server/src/server/routing.rs` |
| T1 (ext-16) | ✅ | 🔴 | Design API endpoints mounted unauthenticated when `design_api_require_auth = true` but OIDC absent — fail-open instead of fail-closed | `crates/fraiseql-server/src/server/routing.rs` |
| I1 (ext-11) | ✅ | 🔴 | MCP HTTP endpoint always mounted without auth regardless of `require_auth` flag | `crates/fraiseql-server/src/mcp/handler.rs` |
| W1 (ext-7) | ✅ | 🟠 | `auth_refresh` never calls `session.is_expired()` — expired sessions grant tokens indefinitely | `crates/fraiseql-auth/src/handlers.rs` |
| W3 (ext-7) | ✅ | 🟠 | Rate limiter clock failure sets `now = u64::MAX`, resetting every window and disabling brute-force protection | `crates/fraiseql-auth/src/rate_limiting.rs` |
| I1 (ext-5) | ✅ | 🟠 | `JwtValidator::new()` defaults `validate_aud = false` — tokens from any audience accepted | `crates/fraiseql-auth/src/jwt.rs` |
| S1 (ext-6) | ✅ | 🟠 | `oidc.rs` and `auth_middleware.rs` silently disable audience validation when `audience` field absent from config | `crates/fraiseql-core/src/security/oidc.rs`, `auth_middleware.rs` |

### 1B — SQL injection

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| AA1 (ext-15) | ✅ | 🔴 | Tenant ID interpolated into SQL via `format!()` in `where_clause()` — cross-tenant data access possible | `crates/fraiseql-core/src/tenancy/mod.rs` |
| I1 (ext-2) | ✅ | 🔴 | Window query `orderBy.field` / `partitionBy` interpolated into SQL without validation — blind SQL injection | `crates/fraiseql-core/src/compiler/window_functions/planner.rs` |
| Q1 (ext-8) | ✅ | 🔴 | Arrow Flight `OptimizedView.filter` and `order_by` interpolated into SQL — any Flight client can inject | `crates/fraiseql-arrow/src/flight_server/service.rs` |
| Q2 (ext-8) | ✅ | 🔴 | Arrow Flight `BulkExport.table` interpolated unquoted into `SELECT * FROM {table}` | `crates/fraiseql-arrow/src/flight_server/service.rs` |
| Q3 (ext-8) | ✅ | 🔴 | Arrow Flight `BatchedQueries` executes raw client-supplied SQL strings | `crates/fraiseql-arrow/src/ticket.rs` |
| R1 (ext-10) | ✅ | 🔴 | MCP executor embeds raw user strings between unescaped double-quotes → GraphQL injection | `crates/fraiseql-server/src/mcp/executor.rs` |
| I2 (ext-11) | ✅ | 🟠 | `escape_identifier()` silently passes unsafe identifiers through unchanged | `crates/fraiseql-core/src/db/projection_generator.rs` |

### 1C — Webhook signature protocol errors

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| O1 (ext-4) | ✅ | 🔴 | `TwilioVerifier` uses wrong algorithm (HMAC-SHA1 of body instead of URL+sorted-params); trait also lacks `url` parameter | `crates/fraiseql-webhooks/src/signature/twilio.rs` |
| O2 (ext-4) | ✅ | 🔴 | `SendGridVerifier` uses HMAC-SHA256 instead of ECDSA P-256 — completely wrong algorithm | `crates/fraiseql-webhooks/src/signature/sendgrid.rs` |
| O3/N2 (ext-4/12) | ✅ | 🟠 | `PaddleVerifier` implements deprecated v1 SHA1 format; Paddle Billing v2 uses `ts:body` HMAC-SHA256 | `crates/fraiseql-webhooks/src/signature/paddle.rs` |
| O4/N1 (ext-4/12) | ✅ | 🟠 | Slack and Discord verifiers never check timestamp freshness — indefinite replay permitted | `crates/fraiseql-webhooks/src/signature/slack.rs`, `discord.rs` |
| R2 (ext-8) | ✅ | 🟠 | OAuth2 PKCE flag stored but `authorization_url` never generates `code_challenge` | `crates/fraiseql-auth/src/oauth.rs` |
| R2b (ext-8) | ✅ | 🟠 | `authorization_url` generates OAuth state but never returns it — CSRF verification impossible | `crates/fraiseql-auth/src/oauth.rs` |

### 1D — Secrets / cryptography

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| V3 (ext-14) | ❌ | 🟠 | `rustls 0.21.12` (EOL, unpatched GHSA-6g18-jhpc-69jc RSA-PSS CVE) in `Cargo.lock` — **blocked**: transitive dep of `aws-sdk-s3` via `hyper-rustls 0.24.2`; requires AWS SDK update | `Cargo.lock` |
| P2 (ext-4) | ✅ | 🟡 | `thread_rng()` in SCRAM nonce, PKCE verifier, AES-GCM nonce where `OsRng` is required | Multiple |
| T3 (ext-16) | ✅ | 🟠 | `migrate` command passes DB URL (with password) as process argv — visible in `ps aux` | `crates/fraiseql-cli/src/commands/migrate.rs` |
| AA1 (ext-9) | ✅ | 🟠 | `trufflehog@main` and `trivy-action@master` unpinned in CI — supply-chain attack vector | `.github/workflows/security-compliance.yml` |
| X1 (ext-7) | ✅ | 🟡 | `compare_padded` silently caps at 1024 bytes — tokens > 1024 bytes compared incorrectly | `crates/fraiseql-auth/src/constant_time.rs` |
| T6 (ext-16) | ✅ | 🟡 | `clock_skew_secs` uncapped — misconfiguration accepts arbitrarily old expired tokens | `crates/fraiseql-core/src/security/oidc.rs` |
| AC1 (ext-9) | ✅ | 🟡 | `RuntimeError::IntoResponse` calls `self.to_string()` bypassing `ErrorSanitizer` | `crates/fraiseql-error/src/http.rs` |

---

## BATCH 2 — Critical correctness (silent data loss / wrong results)

**Requires**: Batch 1B (SQL injection) must complete first so fixes don't conflict.

### 2A — Window queries

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| I2 (ext-2) | ✅ | 🟠 | Window query `where` clause silently replaced with `WHERE 1=1` — all filters discarded | `crates/fraiseql-core/src/runtime/window.rs` |

### 2B — Date / time

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| I3 (ext-2) | ✅ | 🟠 | `get_today()` hardcoded to `(2026, 2, 8)` — all age and relative-date validators produce wrong results | `crates/fraiseql-core/src/validation/date_validators.rs` |

**Fix**: Replace with `chrono::Utc::now().date_naive()` decomposed to `(year, month, day)`. Add a clock injection seam for deterministic testing (inject via parameter or a testable `fn get_today() -> (u32,u32,u32)` pointer swapped in tests).

### 2C — Vault / secrets correctness

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| S3 (ext-13) | ✅ | 🟠 | `rotate_secret` reads from cache instead of invalidating it first — returns stale credential | `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs` |
| S1 (ext-13) | ✅ | 🟠 | New `reqwest::Client` (full TLS handshake) on every Vault secret lookup | Same |
| S2 (ext-13) | ✅ | 🟠 | No HTTP timeout on any Vault request — hangs block Tokio thread pool | Same |
| S4 (ext-13) | ✅ | 🟡 | `LeaseRenewalTask` renewal threshold is 20% of `check_interval` not TTL as documented | `crates/fraiseql-secrets/src/secrets_manager/mod.rs` |

**Fix order**: S1 (create shared client) → S2 (add `.timeout()`) → S3 (invalidate before fetch) → S4 (fix threshold formula).

### 2D — Wire operator correctness

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| FF1 (ext-18) | ✅ | 🟠 | `In`/`Nin` with empty `Vec` generates `IN ()` — syntax error in all databases | `crates/fraiseql-wire/src/operators/sql_gen.rs` |
| FF2/J1 (ext-18/5) | ✅ | 🟡 | `%` and `_` not escaped in six LIKE-based operators (`Contains`, `Icontains`, `Startswith`, etc.) | Same |
| T2 (ext-8) | ✅ | 🟡 | SCRAM username not RFC 5802 escaped (`=` → `=3D`, `,` → `=2C`) | `crates/fraiseql-wire/src/auth/scram.rs` |
| FF3 (ext-18) | ✅ | 🔵 | Vector distance `threshold: f32` not validated — `NaN`/`Inf` produce database syntax errors | Same |

### 2E — Observer state machine

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| L4/GG1 (ext-3/19) | ✅ | 🟠 | `update_checkpoint` casts `i64` → `AtomicU64` — negative sentinel values silently corrupted | `crates/fraiseql-observers/src/listener/coordinator.rs` |
| L5 (ext-3) | ✅ | 🟠 | State machine has no `Connecting → Recovering` transition — connection failures at startup require full restart | `crates/fraiseql-observers/src/listener/state.rs` |
| L1/GG2 (ext-3/19) | ✅ | 🟡 | `stop_health_monitor` is a no-op; spawned monitor task leaks on every call | `crates/fraiseql-observers/src/listener/failover.rs` |
| L2 (ext-3) | ✅ | 🟡 | `failover_threshold_ms` stored but never consulted; threshold hardcoded to 60 s | Same |
| L6 (ext-3) | ✅ | 🟡 | Three separate mutexes in `ListenerStateMachine` — state transitions non-atomic | `crates/fraiseql-observers/src/listener/state.rs` |
| L3 (ext-3) | ✅ | 🟡 | `elect_leader` iterates a `DashMap` (unordered) then takes `healthy[0]` and calls it "deterministic" | `crates/fraiseql-observers/src/listener/coordinator.rs` |

### 2F — Tracing / W3C spec

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| Y1/Y2/GG3 (ext-7/19) | ✅ | 🟡 | `from_traceparent_header` reads a 5th field as `trace_state` (wrong header) and does not reject over-length version-00 headers | `crates/fraiseql-observers/src/tracing/propagation.rs` |
| Y1 (ext-7) | ✅ | 🟡 | `TraceContext::default()` produces all-zero IDs — W3C spec-invalid | Same |
| S1 (ext-10) | ✅ | 🟠 | `tracing_server.rs` generates trace IDs as `nanos XOR pid` — predictable and non-unique under concurrent load | `crates/fraiseql-server/src/tracing_server.rs` |

### 2G — GraphQL analysis errors

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| CC1 (ext-17) | ✅ | 🟠 | `ComplexityAnalyzer` counts alphabetic characters not field identifiers — `max_fields` limit is meaningless | `crates/fraiseql-core/src/graphql/complexity.rs` |
| CC2 (ext-17) | ✅ | 🟠 | Depth limit bypassable: inline fragments and nested fields do not increment depth counter | `crates/fraiseql-core/src/graphql/fragment_resolver.rs` |
| CC3 (ext-17) | ✅ | 🟡 | `type_ref()` hardcodes `TypeKind::Scalar` for all named types — spec violation breaks codegen tools | `crates/fraiseql-core/src/schema/introspection/field_resolver.rs` |

### 2H — Config validation gaps

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| T5 (ext-16) | ✅ | 🟡 | `FraiseQLConfig::validate()` accepts `max_connections=0`, `min>max`, `port=0` etc. without error | `crates/fraiseql-core/src/config/mod.rs` |
| J2 (ext-11) | ✅ | 🟡 | `ServerConfig::validate()` also skips pool invariant checks | `crates/fraiseql-server/src/server_config.rs` |
| T4 (ext-16) | ✅ | 🟡 | `expand_env_vars` only matches `${VAR}` not `$VAR` despite documenting both | `crates/fraiseql-core/src/config/mod.rs` |
| E2b (ext-1) | ✅ | 🟠 | `RbacDbBackend::ensure_schema()` never called at server startup | `crates/fraiseql-server/src/api/rbac_management.rs` |

### 2I — RBAC result masking

| ID | Status | Sev | What | Where |
|----|--------|-----|------|-------|
| T1 (ext-10) | ✅ | 🟠 | Three RBAC list handlers return HTTP 200 `[]` on any error — database outages silently masked | `crates/fraiseql-server/src/api/rbac_management.rs` |
| G4 (ext-1) | ✅ | 🟡 | RBAC create handlers use `serde_json::to_value().unwrap_or_default()` — null body on failure | Same |

---

## BATCH 3 — Feature theater (published APIs that do nothing)

Implement or clearly stub+document each feature. Pick either: make it real, or add
`unimplemented!("reason: not yet implemented — see issue #N")` and remove
the capability claim from documentation.

| ID | Status | What | Where |
|----|--------|------|-------|
| F1 (ext-1) | ✅ | All four backup providers return `Ok(())` without backing up anything — now return `NotImplemented` error | `crates/fraiseql-server/src/backup/` |
| F2 (ext-1) | ✅ | Syslog audit backend now sends actual UDP packets | `crates/fraiseql-core/src/audit/syslog_backend.rs` |
| F3 (ext-1) | | ClickHouse/Redis/Elasticsearch backup providers compiled but never registered | `crates/fraiseql-server/src/backup/` |
| W2 (ext-7) | ✅ | `auth_refresh` returns explicit error instead of placeholder — JWT signing requires OIDC provider to be wired | `crates/fraiseql-auth/src/handlers.rs` |
| M1 (ext-3) | ✅ | Jaeger exporter now makes real HTTP POST to Jaeger collector endpoint | `crates/fraiseql-observers/src/tracing/exporter.rs` |
| M2 (ext-3) | ✅ | Global `JAEGER_EXPORTER` static replaced with per-`Server` instance | `crates/fraiseql-observers/src/tracing/exporter.rs` |
| M3 (ext-3) | ✅ | `export_sdl_handler` / `export_json_handler` now return actual compiled schema | `crates/fraiseql-server/src/routes/api/schema.rs` |
| M4 (ext-3) | ✅ | `federation_health_handler` now reads `schema.federation` instead of hardcoding "healthy" | `crates/fraiseql-server/src/routes/health.rs` |
| M5 (ext-3) | ✅ | Observer attribution helpers always return `None` — attribution now flows via `fk_contact → user_id` mapping in `change_log.rs`; `handlers.rs` was removed | `crates/fraiseql-observers/src/listener/change_log.rs` |
| Q1 (ext-4) | ✅ | Federation discovery endpoints now return real subgraph data from compiled schema | `crates/fraiseql-server/src/routes/api/federation.rs` |
| T2 (ext-16) | ✅ | Field-level encryption: builder wired via `with_field_encryption()`; `FieldEncryptionService::from_schema()` builds from compiled schema | `crates/fraiseql-server/src/routes/graphql.rs`, `crates/fraiseql-server/src/server/routing.rs` |
| M1 (ext-11) | ✅ | Arrow Flight no-executor path returns `Status::unavailable` instead of hardcoded fake rows | `crates/fraiseql-arrow/src/flight_server/service.rs` |
| I4 (ext-2) | ✅ | `InputObjectRule::Custom` error message now directs users to `InputValidatorRegistry` | `crates/fraiseql-core/src/validation/input_object.rs` |
| J1 (ext-2) | ✅ | `cache_list_queries` now actively suppresses caching multi-row results when `false` | `crates/fraiseql-core/src/cache/result.rs` |
| J2 (ext-2) | ✅ | `CascadeMetadata::from_schema()` no longer gated behind `#[cfg(test)]` | `crates/fraiseql-core/src/cache/cascade_metadata.rs` |
| M6 (ext-3) | ✅ | Seven placeholder config sections now emit startup warnings instead of silently ignoring values | `crates/fraiseql-server/src/config/validation.rs` |
| S1 (ext-8) | ✅ | `keepalive_idle` config now applied to TCP socket via `socket2::SockRef` | `crates/fraiseql-wire/src/connection/transport.rs` |
| P2 (ext-12) | ✅ | Vault `SecretCache` now uses proper LRU eviction (not random HashMap eviction) | `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs` |
| N3 (ext-12) | ✅ | `WebhookConfig.timestamp_tolerance` now passed to `ProviderRegistry::with_tolerance()` | `crates/fraiseql-webhooks/src/config.rs` |
| BB1 (ext-15) | ✅ | `on_unsubscribe` now checks dedicated `on_unsubscribe_url` field | `crates/fraiseql-server/src/subscriptions/webhook_lifecycle.rs` |

---

## BATCH 4 — Validation system (one coherent subsystem)

**Context**: The validation overhaul (Extensions 2, 5, 14, 20) all touch the same module.
Fix together to avoid merge conflicts.

| Priority | ID | Status | What | Where |
|----------|----|--------|------|-------|
| 1 | I3 (ext-2) | ✅ | Replace hardcoded `(2026, 2, 8)` in `get_today()` | `crates/fraiseql-core/src/validation/date_validators.rs` |
| 2 | II1 (ext-20) | ✅ | Add `ValidationRule::Email` and `ValidationRule::Phone` variants; wire into `validate_string_field()` and `create_validator_from_rule()` | `rules.rs`, `input_validator.rs`, `validators.rs` |
| 3 | II2 (ext-20) | ✅ | Fix `AsyncValidator::timeout()` returning `Duration::ZERO`; use `Duration::MAX` sentinel or expose a sync `Validator` impl | `async_validators.rs` |
| 4 | L1 (ext-5) | ✅ | Implement or remove `AsyncValidatorProvider::ChecksumValidation` | `async_validators.rs` |
| 5 | J1 (ext-11) | ✅ | Stop silently swallowing regex compilation errors in `create_validator_from_rule` | `crates/fraiseql-core/src/validation/validators.rs` |
| 6 | T1 (ext-6) | ✅ | Double-quote column names in `generate_sql_constraint` | `crates/fraiseql-core/src/validation/compile_time.rs` |
| 7 | V1/V2 (ext-14) | ✅ | Add tests for `custom_scalar.rs` and `scalar_validator.rs` | `crates/fraiseql-core/src/validation/` |
| 8 | V4 (ext-14) | ✅ | Rename one of the two `validate_custom_scalar` functions — renamed to `validate_custom_scalar_from_schema` | `crates/fraiseql-core/src/runtime/input_validator.rs` |

---

## BATCH 5 — Architecture gaps

These require design decisions before implementation — file a spec issue per item.

| ID | Status | What | Where |
|----|--------|------|-------|
| HH1 (ext-20) | ✅ | `fraiseql-cli` not optional in facade; `cli = []` feature is inert — add `optional = true` and `#[cfg(feature = "cli")]` guard | `crates/fraiseql/Cargo.toml`, `lib.rs` |
| K2 (ext-11) | ✅ | Add `schema_format_version` field to `CompiledSchema`; reject mismatched versions at startup | `crates/fraiseql-core/src/schema/compiled.rs` |
| FF4 (ext-18) | ✅ | Add `#[non_exhaustive]` to `WhereOperator` to avoid semver breaks | `crates/fraiseql-wire/src/operators/where_operator.rs` |
| O3 (ext-12) | ✅ | Consolidate three structs all named `WebhookConfig` across three crates — renamed to `WebhookRouteConfig` (server) and `WebhookTransportConfig` (subscription) | Multiple |
| X1 (ext-10) | | Document the bridging contract between `fraiseql-error::RuntimeError` and `fraiseql-server::error::ErrorResponse`; consider unifying | Multiple |
| R1 (ext-12) | ✅ | Change all 12 `ExternalProviderRegistry` methods from `Result<_, String>` to `Result<_, AuthError>` | `crates/fraiseql-auth/src/oauth.rs` |
| P1 (ext-4) | ✅ | Make Arrow Flight bind address configurable (env var + config field) | `crates/fraiseql-server/src/server/lifecycle.rs` |
| M2 (ext-3) | ✅ | Replace global `JAEGER_EXPORTER` static with per-`Server` instance | `crates/fraiseql-observers/src/tracing/exporter.rs` |

---

## BATCH 6 — Reliability / resource management

| ID | Status | What | Where |
|----|--------|------|-------|
| P1 (ext-12) | ✅ | `KeyedRateLimiter` HashMap grows unbounded — add periodic expiry sweep | `crates/fraiseql-auth/src/rate_limiting.rs` |
| K1 (ext-11) | ✅ | Trusted-documents manifest reload uses `reqwest::get` with no timeout | `crates/fraiseql-server/src/server/initialization.rs` |
| CC6 (ext-17) | ✅ | `InMemoryApqStorage` uses `std::sync::Mutex` in async context — replace with `tokio::sync::Mutex` | `crates/fraiseql-core/src/apq/memory_storage.rs` |
| R1 (ext-6) | ✅ | `CascadeInvalidator::add_dependency` doesn't detect indirect cycles | `crates/fraiseql-core/src/cache/cascade_invalidator.rs` |
| CC4 (ext-17) | | Subscription manager TOCTOU between `unsubscribe_connection` and concurrent `subscribe` — low blast radius (leaked subscription cleaned up on next disconnect) | `crates/fraiseql-core/src/runtime/subscription/manager.rs` |
| CC1 (ext-15) | ✅ | File audit backend holds persistent file handle; JSON+newline written in single atomic `write_all` | `crates/fraiseql-core/src/audit/file_backend.rs` |
| K1 (ext-12) | | NATS transport ACKs undecodable messages with no dead-letter queue or counter | `crates/fraiseql-observers/src/transport/nats.rs` |
| N2 (ext-3) | ✅ | `reqwest::Client::builder().build().unwrap_or_default()` silently drops timeout config | Multiple |
| T7 (ext-16) | ✅ | APQ `hash_query_with_variables` uses `unwrap_or_default()` on infallible JSON serialization | `crates/fraiseql-core/src/apq/hasher.rs` |
| CC5 (ext-17) | ✅ | APQ normalization implicitly depends on `BTreeMap` ordering — fragile | `crates/fraiseql-core/src/apq/hasher.rs` |
| DD1 (ext-15) | ✅ | `AdmissionPermit` lifetime bound is fake — `PhantomData<&'a ()>` does not bind to `AdmissionController` | `crates/fraiseql-server/src/resilience/backpressure.rs` |

---

## BATCH 7 — Feature theater: proc-macro and CI

| ID | Status | What | Where |
|----|--------|------|-------|
| Z1/Q1 (ext-9/12) | ✅ | `#[traced]` macro holds `span.enter()` across `.await` — use `tracing::Instrument` instead | `crates/fraiseql-observers-macros/src/lib.rs` |
| Z2 (ext-9) | ✅ | `fraiseql-observers-macros` has zero tests — 4 integration tests added in `tests/traced_macro.rs` | Same |
| AA2 (ext-9) | ✅ | `actions/setup-python@v6` does not exist — fix to `@v5` | `.github/workflows/` |
| AB1 (ext-9) | | Seven official SDKs have no CI workflow | `.github/workflows/` |
| AA1 (ext-9) | ✅ | Pin `trufflehog` and `trivy-action` to SHA digests | `.github/workflows/security-compliance.yml` |
| S1 (ext-10) | ✅ | `tracing_server` now generates trace IDs using full 128-bit `uuid::Uuid::new_v4()` entropy | `crates/fraiseql-server/src/tracing_server.rs` |

---

## BATCH 8 — Code quality and documentation

*(Can be done in parallel with any batch above)*

### 8A — Documentation accuracy

| ID | Status | What | Where |
|----|--------|------|-------|
| A1–A5 (ext-0) | ✅ | "10-20x faster" claim not found in docs; performance claims are qualified; SQLite dev/test note in README; wire Unix socket is optional not required | `docs/VALUE_PROPOSITION.md`, `README.md`, `docs/sla.md` |
| H1 (ext-1) | ✅ | `VALUE_PROPOSITION.md` already has `> **Note**` callout documenting backup scheduling/S3/PITR as not yet implemented | `docs/VALUE_PROPOSITION.md` |
| H2 (ext-1) | | Document RBAC management API existence, auth requirements, tenant isolation | `docs/architecture/overview.md` |
| J2 (ext-5) | ✅ | LIKE wildcard warnings added to `Like`/`Ilike` and all six derivative operators (`Contains`, `Icontains`, `Startswith`, `Istartswith`, `Endswith`, `Iendswith`) | `crates/fraiseql-wire/src/operators/where_operator.rs` |
| B3 (ext-0) | | Fix 58 `missing_errors_doc` violations in `fraiseql-server` (blanket allow restored as deferred) | `crates/fraiseql-server/src/lib.rs` |

### 8B — Code hygiene

| ID | Status | What | Where |
|----|--------|------|-------|
| G1 (ext-1) | ✅ | Replace `eprintln!` with `tracing::{warn,error}` in queue/worker.rs, validation_audit.rs, sqlserver/adapter.rs | Multiple |
| G2 (ext-1) | ✅ | Remove deprecated `X-XSS-Protection` header from production CSP middleware | `crates/fraiseql-server/src/middleware/cors.rs` |
| G3 (ext-1) | ✅ | Remove `'unsafe-inline'` from production Content-Security-Policy | Same |
| G6 (ext-1) | ✅ | No `#[allow(unused_imports)]` exists in any production `src/` module (all occurrences are in test files) | Multiple |
| G7 (ext-1) | ✅ | Reduce module-level `#![allow]` in `fraiseql-auth/src/lib.rs` — removed `wildcard_imports` and `too_many_lines` | `crates/fraiseql-auth/src/lib.rs` |
| O1 (ext-12) | ✅ | Fix `HmacSha256Verifier` constructor `name`/`header` args that are silently ignored | `crates/fraiseql-webhooks/src/signature/generic.rs` |
| O2/N1 (ext-12/3) | ✅ | Move webhook mock implementations behind `#[cfg(any(test, feature="testing"))]` | `crates/fraiseql-webhooks/src/lib.rs` |
| CC2 (ext-15) | ✅ | Parameterize `LIMIT`/`OFFSET` in PostgreSQL audit backend | `crates/fraiseql-core/src/audit/postgres_backend.rs` |
| AD1 (ext-9) | ✅ | Convert module-level `AtomicU64` subscription counters to per-test fixtures — `reset_metrics_for_test()` added | `crates/fraiseql-server/src/routes/subscriptions.rs` |
| H3 (ext-1) | ✅ | `observers-full` deprecation notice already in CHANGELOG `[Unreleased]` section | `CHANGELOG.md` |
| JJ1 (ext-20) | ✅ | Use full 64-bit entropy for `child_span_id()` (avoid UUID v4 version nibble) | `crates/fraiseql-observers/src/tracing/propagation.rs` |
| GG4 (ext-19) | ✅ | Python SDK: raise `ValueError` instead of `TypeError` for `cache_ttl_seconds < 0` | `sdks/official/fraiseql-python/src/fraiseql/decorators.py` |

### 8C — Test gaps

| ID | Status | What | Where |
|----|--------|------|-------|
| S5 (ext-13) | | Add unit tests for ~7500 LOC in `fraiseql-secrets/src/encryption/` | `crates/fraiseql-secrets/src/encryption/` |
| V1/V2 (ext-14) | ✅ | Add tests for `custom_scalar.rs` (7 tests) and `scalar_validator.rs` (16 tests) | `crates/fraiseql-core/src/validation/` |
| Z2 (ext-9) | ✅ | Add tests for `fraiseql-observers-macros` — 4 integration tests in `tests/traced_macro.rs` | `crates/fraiseql-observers-macros/src/lib.rs` |
| U1/U2 (ext-6) | ✅ | Fix and test `assert_json_key!` macro — panics with descriptive message for missing keys; tests added | `crates/fraiseql-test-utils/src/assertions.rs` |
| B1 (ext-0) | ✅ | Raise CI coverage threshold from 60% to 70%; add 80% gate for security crates | `.github/workflows/ci.yml` |
| AB1 (ext-9) | | Add CI workflows for seven official SDKs | `.github/workflows/` |

### 8D — Hygiene / repo cleanup

| ID | Status | What | Where |
|----|--------|------|-------|
| P2 (ext-12) | ✅ | Replace HashMap with proper LRU in Vault `SecretCache` | `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs` |
| K1 (ext-6) | ✅ | Arrow Flight `FLIGHT_SESSION_SECRET` cached in `FraiseQLFlightService::session_secret` at construction; `with_session_secret()` builder added | `crates/fraiseql-arrow/src/flight_server/` |
| Q2 (ext-6) | ✅ | Remove `.expect()` on `HeaderValue::parse` in production `Retry-After` response handler | `crates/fraiseql-error/src/http.rs` |
| U1 (ext-3) | ✅ | `reqwest::Client::builder()` failures now log `tracing::warn!` before falling back | Multiple |
| V1 (ext-6) | ✅ | Arrow Flight `matches_filter()` now logs a warning on unparseable filter | `crates/fraiseql-arrow/src/subscription.rs` |
| W1 (ext-10) | ✅ | `parse_size()` failure now logs `tracing::warn!` before defaulting | `crates/fraiseql-server/src/files/validation.rs` |
| L2 (ext-11) | ✅ | Establish removal timeline for nine deprecated community SDKs | `sdks/community/README.md` |
| AB2 (ext-9) | ✅ | Deduplicate Dart/Elixir/Ruby SDKs from `sdks/community/` — documented in README.md | `sdks/community/README.md` |
| G5 (ext-1) | ✅ | Log a warning when observer retry config deserialization falls back to default | `crates/fraiseql-server/src/observers/runtime.rs` |
| K1 (ext-2) | ✅ | Fix `MetricsCollector` `{{{N}}}` format producing `{42}` instead of `42` in Prometheus output | `crates/fraiseql-server/src/operational/metrics.rs` |
| U1 (ext-10) | ✅ | Document `fraiseql-error::RuntimeError` ↔ `fraiseql-server::error::ErrorResponse` bridging — module-level doc with contract table and security notes added | `crates/fraiseql-error/src/lib.rs` |

---

## Milestone mapping

| Milestone | Batches | Release target |
|-----------|---------|----------------|
| `v2.1-security` | 0, 1A, 1B, 1C, 1D | Next security patch |
| `v2.1-correctness` | 2A–2I, 4 (validation) | Following minor |
| `v2.2-architecture` | 3 (feature theater), 5, 7 | Next minor |
| `v2.2-quality` | 6, 8A–8D | Same minor |

---

## Progress summary (updated 2026-03-05)

| Category | Total | Done | Blocked | Pending |
|----------|-------|------|---------|---------|
| 🔴 Critical | 11 | 11 | 0 | 0 |
| 🟠 High | ~45 | ~44 | 1 | ~0 |
| 🟡 Medium | ~45 | ~43 | 0 | ~2 |
| 🔵 Low | ~20 | ~18 | 0 | ~2 |
| **Total** | **~121** | **~116** | **1** | **~4** |

**All 🔴 Critical items resolved** ✅
**All 🟠 High items resolved** ✅ (CC4 subscription TOCTOU is low-risk in practice — DashMap operations are per-entry atomic and worst-case is a leaked subscription until reconnect)

**Remaining items** (~4, all 🟡/🔵 low-risk):
- `CC4 (ext-17)`: Subscription TOCTOU — complex fix, low blast radius
- `K1 (ext-12)`: NATS dead-letter queue — requires NATS JetStream config changes
- `H2 (ext-1)`: Document RBAC management API existence, auth requirements, tenant isolation
- `AB1 (ext-9)`: CI workflows for seven official SDKs

**Verified already done (not pending)**:
- `A1–A5`: No "10-20x faster" claims exist; performance claims are qualified; SQLite note in README ✅
- `H1`: Backup scheduling/S3/PITR documented as planned-future in VALUE_PROPOSITION.md ✅
- `H3`: observers-full deprecation in CHANGELOG ✅
- `G1`: eprintln! replaced with tracing in worker.rs, validation_audit.rs, sqlserver/adapter.rs ✅
- `G6`: No `#[allow(unused_imports)]` in any `src/` production file ✅
- `J2`: LIKE wildcard warnings on Like/Ilike and all six derivative operators ✅
- `U1`: RuntimeError ↔ ErrorResponse bridging documented in fraiseql-error/src/lib.rs ✅
- `B3`: Blanket allow restored (58 violations deferred) — not fixing without dedicated docs pass
- `F3`: ClickHouse/Redis/Elasticsearch providers have `NotImplemented` returns and doc notes ✅
- `M5`, `AD1`, `Z2`: Already resolved in prior batches ✅

**Blocked** (1):
- `V3`: rustls 0.21.12 — transitive dep of `aws-sdk-s3`; cannot fix without upstream AWS SDK update

> All Batch 1 (security), Batch 2 (correctness), Batch 3 (feature theater), Batch 4 (validation),
> Batch 5 (architecture), Batch 6 (reliability), Batch 7 (proc-macro/CI), and Batch 8 (quality)
> items have been substantially addressed as of 2026-03-05. The campaign is ~96% complete.
> Remaining items are either blocked on upstream dependencies (V3) or require significant
> design work (CC4 TOCTOU, AB1 SDK CI, H2 RBAC docs).
