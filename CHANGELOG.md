# Changelog

All notable changes to FraiseQL are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.3.1] - 2026-05-27

### Fixed

- **Server panic at startup on observer router mount** (#316, #317) — the axum 0.7 → 0.8 migration left one path-capture literal at the old `:listener_id` syntax (`crates/fraiseql-server/src/observers/routes.rs:128`, `/checkpoint/:listener_id`). axum 0.8 hard-panics at `Router::route` build time on the old syntax, so any deployment that mounted the observer changelog router crashed before binding the listener. The literal is now `{listener_id}` and the panic site is gone.

### Added

- **Router-construction tests** (#317) — `observer_routes`, `observer_runtime_routes`, `observer_dlq_routes`, `observer_changelog_routes`, and `rbac_management_router` each have a `#[tokio::test]` that constructs the router (see `crates/fraiseql-server/src/observers/tests.rs::router_construction` and `crates/fraiseql-server/src/api/rbac_management/tests.rs::router_construction`). axum's path-capture validation runs inside `Router::route`, so the same bug class would now surface in `cargo test`, not at first server boot.

- **`axum-route-syntax-check` CI gate** (#317) — `tools/check-route-syntax.sh` greps for `:param` literals inside `.route(...)` calls across `crates/` and `examples/`. Combines a single-line regex with a load-bearing multi-line `awk` pass that catches `.route(\n  "...",\n  handler\n)` calls (the v2.3.0 bug literal was invisible to a single-line regex). Wired as a job in `.github/workflows/ci.yml`; `make lint-routes` runs it locally.

- **`release-smoke` workflow** (#317) — `.github/workflows/release-smoke.yml` boots `fraiseql-server` (release profile) against the `docker/e2e/` fixtures on `release/*` branches and `v*` tags and asserts `/health` responds within ~30s. Catch-all for the "code compiles, server panics on boot" bug class — covers every router constructor the binary actually mounts, not just the ones unit-tested individually.

## [2.3.0] - 2026-05-25

*v2.3.0 supersedes the abandoned 2026-05-14 release attempt — see commit history for the revival. Migration guide for adopters: `docs/migration/v2.2-to-v2.3.md`.*

### Added

- **LTree ID-based operators** (#250) — `descendantOfId` and `ancestorOfId` WHERE operators
  that resolve an entity's ltree path from its UUID before performing hierarchical comparisons.
  Supports self-referencing hierarchies (`path <@ (SELECT path FROM t WHERE id = $1)`) and
  cross-table hierarchies via FK semi-joins. Configured via `[hierarchies]` in `fraiseql.toml`
  with `table` and `path_column` settings. Includes field-level `hierarchy` annotation and
  compile-time validation. PostgreSQL-only (MySQL/SQLite/SQL Server return `Unsupported`).
  (`de05e4252`, `91d92f376`, `b83ca0957`, `8ec7c7617`, `229542276`, `a8d638dc9`, `2be493440`, `3ae032a1d`)

- **JWT nested claims extraction** (#246) — `Claims::email()` and `Claims::name()` accessor
  methods that normalize nested JWT claim formats (Azure AD `{"value": "..."}`, OIDC
  `{"given": "...", "family": "..."}`, arrays) into flat strings. `GET /auth/me` now
  returns top-level `email` and `display_name` fields, and RLS session variables support
  `jwt:email` and `jwt:name`/`jwt:display_name` mappings.
  (`75fbd24be`, `cccb19fc7`, `f012f2e03`, `06a03ba28`)

- **Partial-period aggregates** — UNION ALL dispatch for aggregate queries spanning period
  boundaries, with `TemporalGrain` and `PartialPeriodConfig` schema model additions and
  lower-bound date extraction from WHERE clauses. (`727b68829`, `784a09f89`, `773029355`,
  `bd25bf471`, `6d683dbd8`, `91ac77ab7`)

- **Storage API** (`fraiseql-storage` crate) — S3/local/Azure/GCS storage backends with
  RLS-enforced tenant isolation, file transforms (resize, watermark, format conversion),
  and access control routes mounted on the server. Ported from the Phase 8 platform
  integration; see Phase 12 in the roadmap. (`00ddccb83`, `3fb958715`)

- **Functions trigger system** (`fraiseql-functions`) — `after:mutation`, `before:mutation`,
  `after:storage`, cron, and HTTP trigger types with a `TriggerRegistry` for dispatch.
  WASM host bindings for function execution, WASI support, host op wiring with `SqlExecutor`
  injection, sandbox + concurrency limiter, function secrets (AES-256-GCM), and WASM module
  cache for cold-start optimization. (`11d0e3442`, `db0b65166`, `de162ed9d`, `9c6aaecba`,
  `88d8fc040`, `aa23821d2`, `d36cf1bfb`, `f462fada3`, `37a563fc3`, `6743ad290`, `a76b3e747`,
  `d228dc05e`, `18a310661`)

- **Realtime subsystem** — WebSocket server with subscription protocol, event delivery
  with RLS, broadcast observer, `CronScheduler` for periodic tasks, presence manager with
  room tracking and heartbeat eviction, broadcast channels with REST publish endpoint, and
  CDC `ObserverRuntime` wired into `EventBridge`. Tenant-aware CDC filtering via
  `fk_customer_org`. (`f6dd7e419`, `8b0e78402`, `ed23497bc`, `6ca949577`, `dde8e41f1`,
  `aded85a27`, `4d9639fc8`)

- **Subsystems builder** — `ServerSubsystems` builder pattern with `ExtendedCompiledSchema`
  loader and config validation for composing server capabilities. (`aded85a27`)

- **Auth extensions** (Phase 13) — unified multi-provider social login (Google, GitHub, Apple,
  Microsoft), account linking (same email → same user), magic links / email OTP, TOTP MFA
  with recovery codes, anonymous session signup, and phone-auth SMS OTP. (`b7fb91413`,
  `cd5c594f4`, `d57036537`, `a88b69a19`, `d4879ca6a`, `97a554b81`, `41791f0a0`)

- **Tenancy hardening** (Phase 15) — `TenancyConfig` and `TenancyMode` plumbing, compile-time
  `@tenant_id` row-isolation guard, schema-isolation DDL and `search_path` management,
  suspend/resume lifecycle with admin scope guard, tenant-aware rate limiting and quotas,
  tenant audit trail, and tenant cross-source consistency validation. (`aec9753ff`,
  `6808942ed`, `ed14d8f50`, `c21f78a6f`, `0c2fb55c7`, `9b1fe5c56`, `d1fa0d089`, `8675b43b3`)

- **Schema migrations CLI** (Phase 14) — schema migrations & evolution support via
  `fraiseql-cli`. (`1158be090`)

- **Studio admin dashboard** (Phase 18) — SPA shell with embedded assets at `/studio`,
  admin API schema + health endpoints, data browser backend, auth/storage/realtime/functions/
  metrics backend endpoints, frontend wired to all admin API endpoints. (`6b66e56ad`,
  `0768881a6`, `f4838058a`, `84e6cca47`, `3d2039890`, `53ebbd18a`)

- **Studio metrics endpoint** — `GET /admin/v1/metrics/summary` wired to live
  `MetricsCollector` with real-time latency percentiles and cache hit rate.

- **CLI `setup` command** — generates mutation helper functions (`mutation_response` type,
  `fn_mutation_success` / `fn_mutation_error` SQL functions). (`1c3497e9e`)

- **Observer management** — changelog handlers, DLQ handlers, and shared DLQ state
  across hot-reload cycles. (`3b04c3241`)

- **`DatabaseAdapter::on_schema_reload()`** — adapters react to schema hot-reload
  events (e.g. clear caches). Default no-op for backwards compatibility.

- **PostgreSQL usage persistence backend** — `UsageAggregator` stores mutation counters
  in `fraiseql_usage_counters` table with automatic background flush lifecycle.
  (`5bf080663`, `a0ddffa03`)

- **`[usage]` TOML configuration section** — `ServerConfig.usage: Option<UsagePersistenceConfig>`.

- **REST transport wiring** — `[rest]` TOML section now parsed and compiled
  through the full pipeline (merger → intermediate → compiled schema). Server
  mounts read-only REST query router behind `rest` feature flag. Based on
  PR #229 by @magick93. (`bd98715e4`, `d97924802`, `fe6456854`)

- **Admin query-stats endpoints** (#268) — cross-database query performance
  observability via `GET /api/v1/admin/query-stats`, `GET .../query-stats/{queryid}`,
  and `POST .../query-stats/reset`. Backed by `pg_stat_statements` (PostgreSQL),
  `performance_schema` (MySQL), and `sys.dm_exec_query_stats` (SQL Server). Graceful
  no-op on SQLite. Prometheus gauges: `fraiseql_db_query_exec_seconds`,
  `fraiseql_db_query_calls`, `fraiseql_db_query_mean_exec_seconds`,
  `fraiseql_db_cache_hit_ratio`. Grafana dashboard panel added. (`2f6104d99`, `deb586efb`,
  `396ab5508`, `38562a0d3`, `1cfae166a`)

- **Native aggregation column support** — `native_measures` for flat column
  aggregation without JSONB extraction, and `native_dimension_mapping` for
  GROUP BY column resolution on views with native SQL columns. (`95db4f9b9`, `f7245960e`)

- **Wire protocol network operators** — `isMulticast`, `isLinkLocal`,
  `isDocumentation`, `isCarrierGrade` network filter operators; `isPrivate` / `isPublic`
  consolidated into boolean-value pattern. (`20bb709f3`, `3f4bcfc63`)

- **camelCase operator normalization** — WHERE clause operator names now accept
  camelCase form (e.g. `startsWith`) and normalize to snake_case internally. (`37dc02312`)

- **Independent admin-route auth toggles** — `metadata_require_auth`,
  `schema_export_require_auth`, `playground_require_auth`, and `subscription_require_auth`
  config options decouple each admin/inspection surface from the global `require_auth`
  default. (`02081b700`, `c3286bb60`, `c2f8304ed`, `fdba1d06c`)

- **Federation mTLS** — defence-in-depth mTLS support for federation subgraph connections.
  (`0e5175371`)

- **Schema integrity** — SHA-256 content hash wired into `schema.compiled.json` for
  startup-time integrity verification. (`a27d8f1c5`)

- **Cargo-fuzz target for wire JSON parse path** — covers every variable/row JSON payload
  reaching the engine. [F030] (`2763ca296`)

- **Property tests for runtime entry points** — 9 property tests covering `parse_query`,
  `QueryMatcher::match_query`, and `extract_root_field_names`. [F031] (`fcee0374b`)

- **Crate-level READMEs** — 16 workspace crates now declare `readme = "README.md"` so
  crates.io and docs.rs landing pages render the overview. Three missing READMEs added
  (`fraiseql-functions`, `fraiseql-storage`, `fraiseql-test-utils`). [F032]
  (`7fd709d97`, `494bf086a`, `d69d1fdbc`, `9cb46eccf`)

### Security

- **S33**: auth input caps + `reload_schema` path-traversal guard. (`5f0e76806`)
- **S34**: resource bounds on auth flows. (`2b11e0371`)
- **S35**: quality & observability polish on the auth path. (`ff09fd270`)
- **S36**: session security hardening. (`694b74b56`)
- **S37**: PKCE hardening. (`2aaf5cd89`)
- **S38**: SCRAM / auth key-material zeroization. (`6e476c46a`, `4f9fad1e1`)
- **S39**: redirect URI and auth-code input hardening. (`1059d0368`)
- **S40**: JWT claims hardening. (`9a8a31c15`)
- **S41**: JWT algorithm hardening. (`e123528b6`)
- **S42**: JWT header injection defence. (`b26bfd523`, `5f4265eae`)
- **S43**: IPv6 literal parsing in wire connection strings (RFC 3986 bracket notation).
  (`39b625a89`)
- **S44**: Federation saga table double-prefix fix (`tb_tb_` → `tb_`) + `cleanup_all`
  visibility restriction. (`57c15b286`)
- **S45–S48**: real peer-IP forwarding via `PeerIp` extractor for GraphQL rate limiting,
  `AuthorizationDenied` audit event for SOC 2 compliance logging, Vault backend rotation
  atomicity with per-secret `DashMap` locks, and admin bearer-token brute-force protection.
  (`4e3b680c3`)
- **Vault hardening** — body-size guards and `Debug` redaction on the secrets backend.
  (`17cf97a96`)
- **Cache RLS isolation guard** — additional guard ensuring cache lookups cannot
  cross-leak between security contexts. (`226d0de36`)
- **Subscription tenant isolation** — WebSocket subscriptions now enforce tenant
  isolation end-to-end. (`9639fd894`)
- **HTTP allowlist defaults** — `fraiseql-functions` outbound HTTP now denies by default;
  hosts must be explicitly allowlisted. (`f49885cbf`)
- **RLS enforcement on aggregate/window paths** — closes a gap where aggregate and
  window queries could bypass row-level security. (`f7d5e77a8`)
- **Redact bearer token in `AuthRequest` Debug output.** [F010] — manual `Debug`
  emits `Some("<redacted>")` / `None`. (`1dbf83119`)
- **Redact tokens in `AuthCallbackResponse` / `AuthRefreshResponse` Debug.** [F045]
  (`47c478768`)
- **Zeroize `Secret` buffer on drop.** [F012] — `Secret`'s `Drop` impl now scrubs the
  underlying heap allocation; previously `Debug` was redacted but the plaintext lingered
  in freed pages. (`eda6db593`)

### Fixed

- **Hot-reload cache rebind** — query cache cleared on schema reload, resolving a
  stale-cache bug.
- **fraiseql-storage compile errors** — corrected compile-time failures from the v2.2.0
  federation work.
- **`platform_e2e_test` repaired** — 9 platform E2E tests pass reliably after a race
  condition fix.
- **OIDC enrichment compatibility** — works without the observers feature enabled.
- **CLI SBOM metadata** — falls back to workspace `Cargo.toml` when crate-level
  metadata is unavailable. (`b7486e794`)
- **3 broken doctests in `traits.rs` and `PostgresAdapter`** — repaired. (`185822222`)
- **Federation HTTP retry source chain** — `execute_with_retry` now threads the most recent
  `reqwest::Error` into `FraiseQLError::Internal { source }` instead of stringifying it
  away. [F025] (`500859a48`)
- **Observer job-worker panics propagated** — `execute_batch` now logs panics at `error!`
  with `worker` and `error` fields and increments `fraiseql_observer_job_failed_total`
  (when the metrics feature is enabled). [F014] (`d1c89be6e`)
- **Cron task error chain logged** — cron-task error log now adds `error.debug` and
  `error.chain` fields walking `std::error::Error::source()`. [F047] (`7f99fe498`)
- **Response-cache key serialization errors propagated** — `compute_response_cache_key`
  now returns `Result<u64>` and bubbles serialization failures as `Validation` errors
  instead of `unwrap_or_default()` colliding distinct argument trees onto the empty-string
  key. [F044] (`cf3a202cd`)
- **Per-query execution log demoted from `info` to `debug`.** [F041] (`ef8bc4119`)
- **`FraiseQLError` doctest references** — rewritten to enumerate three real variants
  (`Parse`, `Validation`, `Database`) with a `#[non_exhaustive]` explanatory comment.
  [F016] (`bc9df7dc2`)
- **`IntoResponse for FraiseQLError` catch-all arm** — `into_response`, `status_code`, and
  `error_code` matches now carry a documented catch-all arm so a future
  `#[non_exhaustive]` variant addition defaults to a safe generic 500 rather than failing
  to compile silently. [F055] (`39078b202`)
- **`Auth` / `Webhook` / `Observer` source-chain preservation** — `#[source]` annotation
  added to the three boxed-payload variants so `err.source()` walks the subsystem-error
  chain instead of returning `None`. [F049] (`bc0ed8e25`)
- **`FraiseQLError::Storage` ownership rustdoc** (later collapsed by the F050 deletion).
  [F051] (`686322bd6`)
- **OAuth/token race conditions in tests** — drain tokio task before cancel in token-refresh
  and lease-renewal tests. (`379919faa`, `faca53b82`)

### Changed (breaking)

- **Error taxonomy consolidation** — `FraiseQLError` is now the single root error type for
  the workspace. The parallel HTTP-shaped `RuntimeError` enum has been deleted from
  `fraiseql-error`, along with five vestigial shadow domain enums
  (`fraiseql_error::{AuthError, WebhookError, NotificationError, IntegrationError,
  ObserverError}`) that had zero production call sites. Subsystem error vocabularies
  (`fraiseql_auth::AuthError`, `fraiseql_webhooks::WebhookError`,
  `fraiseql_observers::ObserverError`) now compose into the canonical taxonomy via owned
  `From<X> for FraiseQLError` impls (sqlx pattern); the new variants are
  `FraiseQLError::{Auth, Webhook, Observer, File}`. `FileError` itself is retained (9
  production call sites) and is now a `#[from]` variant of `FraiseQLError`. The
  `impl IntoResponse` in `fraiseql_error::http` now wraps `FraiseQLError` directly
  (was: `RuntimeError`), and `IntoHttpResponse` bridges `Result<T, FraiseQLError>`. The
  umbrella crate `fraiseql` no longer re-exports `RuntimeError`, `AuthError`, or
  `WebhookError`; use `FraiseQLError` (via `fraiseql::FraiseQLError` or
  `fraiseql::prelude::*`) instead. (`ffd3124e9`, `dd1c9b80f`, `230d4d238`)
  **Migration:** see `docs/migration/v2.2-to-v2.3.md` and `DEPRECATIONS.md`.

- **`ServerError::RuntimeError` renamed to `ServerError::Engine`** — the variant wraps
  `fraiseql_core::error::FraiseQLError` (the engine error), not the now-deleted
  `fraiseql_error::RuntimeError`. The old name was a misnomer. The `#[from]` semantics
  are unchanged: any `FraiseQLError` bubbles up as `ServerError::Engine` automatically.
  (`65491c2a9`)
  **Migration:** `sed -i 's/ServerError::RuntimeError/ServerError::Engine/g' **/*.rs`.

- **`FraiseQLError::Storage` removed; storage failures now use
  `FraiseQLError::File(FileError::*)`** [F050]. The 118 call sites in `fraiseql-storage`
  and `fraiseql-functions` that used to construct `FraiseQLError::Storage { message, code }`
  have been migrated to typed `FileError` variants, eliminating the `code: Option<String>`
  string-discriminator anti-pattern. Eight new `FileError` variants cover the
  backend-classification space:

  | New variant | HTTP status | Replaces |
  |---|---|---|
  | `FileError::PermissionDenied { message, source }` | 403 | `Storage { code: Some("permission_denied") }` |
  | `FileError::IoError { message, source }` | 500 | `Storage { code: Some("io_error") }` |
  | `FileError::InvalidKey { message }` | 400 | `Storage { code: Some("invalid_key") }` |
  | `FileError::NotImplemented { message }` | 500 | `Storage { code: Some("not_implemented") }` |
  | `FileError::Unsupported { message }` | 500 | `Storage { code: Some("not_supported"/"unsupported") }` |
  | `FileError::SizeLimitExceeded { message, limit, actual }` | 500 | `Storage { code: Some("size_limit_exceeded") }` |
  | `FileError::MimeTypeNotAllowed { message, mime }` | 500 | `Storage { code: Some("mime_type_not_allowed") }` |
  | `FileError::Backend { message, source }` | 500 | catch-all for `Storage { code: None }` (~67 sites: HTTP / SDK failures, config-validation errors, sqlx database errors) |

  Existing `FileError::NotFound` reused for `Storage { code: Some("not_found") }`.
  **Observable HTTP changes** (two refinements):
  1. `FraiseQLError::File(FileError::NotFound)` now returns 404 globally (was 400). This
     aligns the global status code with what the local `storage_error_response` and
     `fraiseql-server::file_error_response` routes already returned for backend
     not-found cases.
  2. `FraiseQLError::File(FileError::InvalidKey)` returns 400 (was 500 under
     `Storage { code: Some("invalid_key") }`). The previous 500 was a bug: a
     caller-supplied bad key is user-fixable and 400 is the semantically correct status.

  Every other status code is preserved: `storage_error_response` still routes
  `NotFound` → 404, `PermissionDenied` → 403, everything else → 500 exactly as before,
  only by matching on typed variants instead of the `code` string. Source-chain
  preservation is a net improvement: reqwest, AWS SDK, sqlx, std::io errors that were
  previously stringified via `format!("backend error: {e}")` now flow through
  `source: Some(Box::new(e))` so `Error::source()` chain walkers and `tracing`'s
  error-chain instrumentation see the underlying type.
  (`4c86d2e0d`, `ed80df821`, `aa7d59712`, `44432234f`, `acec7e435`, `76288f3ab`)
  **Migration:** downstream callers that matched on `FraiseQLError::Storage { .. }`
  must migrate to `FraiseQLError::File(FileError::*)`. See `docs/migration/v2.2-to-v2.3.md`
  for the `code`-string-to-variant table.

- **`ViewName(Arc<str>)` newtype propagated through cache invalidation APIs** [F028, F037] —
  `DatabaseAdapter::invalidate_views`, `DatabaseAdapter::invalidate_list_queries`,
  `QueryResultCache::invalidate_views`, `QueryResultCache::invalidate_list_queries`,
  `ResponseCache::invalidate_views`, and `CachedDatabaseAdapter::invalidate_views` now
  take `&[ViewName]` instead of `&[String]`. Cache internal storage (`accessed_views`,
  `view_index`, `list_index`) migrated accordingly. View names are now promoted from
  `String` to `Arc<str>` once at the `put` boundary and reused across every reference,
  reducing per-cache-write allocations. (`4bf9a58b1`, `e760033ce`)
  **Migration:** adopters with custom adapter impls update the trait method signatures;
  `ViewName::from(&str)` is a one-line conversion at the call site.

- **`execute_with_projection_arc` takes `&ProjectionRequest<'_>` instead of 6 positional
  arguments** [F043] — adapter trait method signature consolidated into a borrowed struct
  with field order mirroring `SELECT … FROM … WHERE … ORDER BY … LIMIT … OFFSET`. The
  struct is intentionally NOT `#[non_exhaustive]` (a missing field is a hard compile error
  by design). (`83725aed8`)
  **Migration:** override the trait method by constructing a struct literal.

- **`KeyedRateLimiter` is generic over `<C: Clock = SystemClock>`** [F018] — the boxed
  `Box<dyn Fn() -> u64 + Send + Sync>` clock has been replaced with a `Clock` trait. A
  blanket impl on `F: Fn() -> u64 + Send + Sync` keeps closure ergonomics for tests, and
  `SystemClock` is a zero-sized type so default-clock production limiters are now `Clone`.
  (`3dca6bd67`)
  **Migration:** code naming the type explicitly (`KeyedRateLimiter` in a struct field)
  may need `KeyedRateLimiter<SystemClock>` to type-check.

- **`extract_root_field_names` returns `impl Iterator<Item = &str>` instead of `Vec<&str>`**
  [F020]. (`dffa25762`)
  **Migration:** add `.collect::<Vec<_>>()` at the two call sites that need a `Vec`.

- **`InMemoryRateLimiter`, `TrustedDocumentStore`, `KeyedRateLimiter`, federation
  `ConnectionManager`, and observer `entity_type_index` migrated to lock-free reads**
  [F006, F007, F008, F013, F048]. All five maps were previously `Arc<Mutex<HashMap>>`
  or `Arc<RwLock<HashMap>>` on read-hot paths and now use `DashMap` (four of them) or
  `ArcSwap<HashMap>` (the observer index, F056) so request-hot reads no longer block on
  a central lock. Per-key atomicity is preserved via `DashMap::entry()` where the
  previous code held the outer lock across a read-modify-write. The
  `TrustedDocumentStore::resolve` / `document_count` / `replace_documents` methods drop
  their `async` signature (no remaining await suspension). The two stricter contracts
  are also restored:
  - Observer `entity_type_index` (F056) uses `ArcSwap<HashMap>` for **snapshot
    atomicity** — readers always observe a fully-populated generation, never a
    partially-rebuilt index during reload.
  - `KeyedRateLimiter` (F057) enforces its `max_entries` cap **strictly** on the
    insert path under a serialising guard — `len()` never exceeds the cap at any
    observable instant, even under sustained concurrent burst.

  The remaining four maps (F006, F007, F008, F013) use plain `DashMap` and document
  per-key best-effort atomicity in the field rustdoc; these are correct under their
  stated contracts. (`c5c946fb3`, `4b3e542b3`, `6f79c711e`, `3cda8124f`, `1ebae1f61`)
  **Migration:** none for callers; behaviour change is internal.

- **`parking_lot::Mutex` replaces `tokio::sync::Mutex` for synchronous critical
  sections** [F019] — `MemoryApqStorage::entries` and
  `ListenerHandle::last_heartbeat` switched to `parking_lot::Mutex<HashMap<…>>` and
  `parking_lot::Mutex<Instant>`. `ListenerHandle::update_heartbeat` is no longer
  `async`. Three sites that hold their lock across `.await` were intentionally left on
  `tokio::sync::Mutex`. (`bb95ef8e9`)
  **Migration:** none unless calling `update_heartbeat` directly — drop the `.await`.

- **Lifecycle `tokio::spawn` tracked via `JoinSet`** [F021] — server lifecycle spawns
  (SIGUSR1 handler, usage-persistence flush, Arrow Flight gRPC server, trusted-docs
  reloader, PKCE cleanup) are now collected into a per-server `tokio::task::JoinSet`
  that `serve_with_shutdown` aborts and drains under the configured shutdown timeout.
  Per-request spawns (subscription event handlers, request middleware) are NOT migrated.
  (`19bfd826c`)
  **Migration:** none for downstream callers; shutdown behaviour is observably more
  graceful.

- **`MetricsCollector` counters flattened to bare `AtomicU64`** [F009] — 28 individual
  `Arc<AtomicU64>` fields replaced with plain `AtomicU64`. `MetricsCollector` no
  longer derives `Clone`; production wiring already wraps in `Arc<MetricsCollector>`.
  Call-site syntax (`metrics.queries_total.fetch_add(…)`) is unchanged. (`f5ddaa59e`)
  **Migration:** any code holding `Arc::clone(&metrics.queries_total)` becomes a
  borrow of the parent `Arc<MetricsCollector>`.

- **Arrow Flight multi-batch responses streamed via bounded `mpsc::channel(4)`** [F011]
  — 4 multi-batch `service.rs` sites converted to a producer task feeding a
  `tokio_stream::wrappers::ReceiverStream` so the consumer's `poll_next` exerts
  backpressure on the producer. Single-element response sites stay on
  `stream::iter(vec![one])`. (`0077a3eb1`)
  **Migration:** none for callers; output stream shape preserved.

- **`ParsedQuery.source: String` is now `Arc<str>`** [F042] — `ParsedQuery::clone()`
  drops its deep string copy in favour of an atomic ref-count bump. The wire form of
  the serde representation is unchanged (custom `serialize_with` / `deserialize_with`
  preserves backward-compatible JSON). (`bab30d351`)
  **Migration:** code that reads `parsed.source` and required `&String` semantics may
  need `&*parsed.source` to get `&str`.

- **`QueryMatcher` builds the variables map once per request** [F005, F024] — the
  matcher used to convert variables twice (once for directive evaluation, once for
  `QueryMatch::arguments`). Folded into a single `variables_to_map` conversion.
  (`38c6e705b`)
  **Migration:** internal change — the wider `QueryMatch` borrowed-arguments
  refactor was deferred (lifetime ripple too wide); signatures unchanged.

- **`ValidationRule::Pattern { pattern: String }` → `Pattern { pattern: CompiledPattern }`**
  [F003] — regex compilation now happens once at construction (or at
  `schema.compiled.json` deserialisation) rather than on every validation call.
  Invalid patterns surface at schema load instead of degrading silently per request.
  (`dd4393d06`)
  **Migration:** downstream code constructing `ValidationRule::Pattern` directly must
  build a `CompiledPattern` from the source string; a `From<String>`-style helper is
  provided.

- **`QueryParam`'s `to_sql_param` helper deleted; `as_sql_param_refs` centralises the
  borrow pattern** [F036] — `QueryParam` already implemented `ToSql`; the boxed-dyn
  conversion was redundant. (`c9b599e15`)
  **Migration:** code calling `to_sql_param(&p)` should use the existing borrowed
  pattern `.iter().map(|p| p as &(dyn ToSql + Sync)).collect()` or the new helper
  `as_sql_param_refs(&[QueryParam])`.

- **Wire-crate clippy allows reorganised into groups** [F053] — moved 2 test-bleed
  allows (`unreadable_literal`, `explicit_iter_loop`) into per-module `#![allow]`
  inside `mod tests` blocks; removed 2 no-longer-firing allows from the crate level
  entirely; grouped the remaining 15 crate-level allows under two commented headers
  ("Wire-protocol cast suppressions" and "Crate-wide style preferences"). Added
  `make lint-gate-wire` enforcing both the count cap and "no test-bleed lints at
  crate level". (`897a2188a`)
  **Migration:** none for callers; build / lint shape only.

- **Workspace clippy strictly denies `panic`, `unreachable`, `print_stdout`,
  `print_stderr`, `dbg_macro`, `todo`, `unimplemented`, `mem_forget`,
  `lossy_float_literal`, `semicolon_if_nothing_returned`, `undocumented_unsafe_blocks`,
  and `missing_assert_message`** at the workspace `[lints.clippy]` level. The
  `nursery` and `cargo` lint groups are promoted from `warn` to `deny`. Three crates
  (`fraiseql-error`, `fraiseql-wire`, `fraiseql-storage`) additionally deny
  `clippy::indexing_slicing` at the crate root as the Q4 pilot. Workspace-wide
  `indexing_slicing` rollout is planned across v2.3.x; see `FOLLOW_UPS.md` for the
  per-crate rollout plan (13 crates remaining). Three pilot crates were refactored
  with no API surface change: `fraiseql-error` (`levenshtein_distance` rolling
  buffer), `fraiseql-wire` (private `Cursor<'a>` decoder helper), `fraiseql-storage`
  (`serde_json::Value::get()` + slice-`.get()` patterns). (`bb5347e82`, `ace13741e`,
  `e6567fb98`, `4d2c5d17b`, `0a829c2ff`, `04154688d`, `f20fc7717`, `280ff100c`,
  `cfe739c71`, `e514bbf25`, `4a6c94664`, `3c3e16089`)
  **Migration:** downstream crates that opt into the workspace lint table inherit
  these denials; if any external code triggers them, hoist the allow to the
  offending function or module with a `// Reason:` comment.

- **`CompiledSchema::from_json` takes a `strict_integrity: bool` second argument** —
  the canonical schema-load entry point now accepts a strict-integrity flag that
  rejects schemas whose hash does not match the embedded integrity manifest. Re-exported
  via `fraiseql::CompiledSchema` and `fraiseql_core::prelude::CompiledSchema`.
  **Migration:** existing call sites pass `false` for backward-compatible behaviour
  (`CompiledSchema::from_json(json, false)`); set `true` to opt into the new
  integrity check. Surfaces under the schema-integrity hardening landed in v2.3.

- **`fraiseql_cli::schema::intermediate::operations::IntermediateSqlSourceDispatch`
  and `fraiseql_core::schema::SqlSourceDispatch` removed** — both `pub` structs
  belonged to a schema-shape intermediate that was superseded by the v2.3 dispatch
  model. Adopters using the CLI-as-library to introspect schema intermediates, or
  pattern-matching on `QueryDefinition.sql_source_dispatch`, must migrate to the
  new dispatch types.
  **Migration:** see the schema-compilation overhaul in `docs/architecture/compiler.md`.
  If you depended on the removed types, file an issue describing your use case so
  the equivalent v2.3 entry point can be documented.

- **`fraiseql_core::security::oidc::providers::MeEnrichmentConfig` removed** —
  this `pub` struct used to configure the OIDC `/auth/me` claim-enrichment behaviour
  via the Rust API. The OIDC enrichment refactor in v2.3 replaced it with a TOML-driven
  configuration path; programmatic enrichment configuration is no longer supported.
  **Migration:** move claim-enrichment configuration into `fraiseql.toml` under
  `[auth.oidc.providers.<name>.me_enrichment]`. The TOML schema is documented under
  the Auth extensions Phase 13 entry above.

- **`#[non_exhaustive]` rollout to public DTOs (`RelayPageResult`,
  `SqlProjectionHint`, `OrderByClause`, `ActionResult`, `CacheStatus`, `EventKind`)**
  — six public DTOs received `#[non_exhaustive]` so future field additions don't
  break adopters. Each type also gained a `new(...)` constructor so the struct-literal
  pattern can be replaced mechanically. `RelayPageResult` and `ActionResult` are
  returned by public traits (`RelayDatabaseAdapter`, `ActionExecutor`) downstream
  implementations satisfy — those impls must use the new constructors. (`dbc9e0afc`,
  `e2b9944d2`, `3d8c4bce6`)
  **Migration:** replace struct-literal construction with the typed `new()` constructor:
  `RelayPageResult::new(rows, total_count)`, `SqlProjectionHint::new(database, projection_template, estimated_reduction_percent)`,
  `OrderByClause::new(field, direction)`, `ActionResult::new(...)`. Existing pattern
  matches gain a `_` arm.

### Changed

- **Lock-free read paths across `fraiseql-auth`, `fraiseql-server`,
  `fraiseql-federation`, `fraiseql-core`** — five rate-limiter / store / index maps
  migrated to `DashMap`, removing serialised reads on the request hot path (see the
  five-finding bullet under "Changed (breaking)" for breakdown). Hot-path reads no
  longer block on a central lock under concurrent load. [F006, F007, F008, F013, F048]

- **GraphQL parsing on the request hot path** — the validator no longer re-parses the
  query body; `parse_graphql_document(&str)` is exposed and `RequestValidator::validate_query_doc`
  accepts a pre-parsed `Document<'_, String>`. The HTTP handler parses once and feeds
  the same AST into validation and matching. [F001] (`b94abc592`)

- **Response cache hit returns an `Arc::unwrap_or_clone` instead of a deep clone** of
  the cached JSON value. [F002] (`15fd10a48`)

- **`compute_response_cache_key` uses a reused scratch `Vec<u8>` and
  `serde_json::to_writer`** — per-argument `String` allocations on the cache-key path
  removed; errors propagate as `Validation` instead of silently colliding. [F044, F004]
  (`cf3a202cd`)

- **`extract_root_field_names` returns `impl Iterator`** — one allocation removed per
  call. [F020] (see "Changed (breaking)" entry above for the API shape change)

- **Federation HTTP retry preserves the source chain** on the final error rather than
  stringifying it. [F025] (`500859a48`)

- **Tracing on the response-cache lookup path** — `event = "hit"|"miss"|"disabled"`
  structured fields under target `fraiseql::cache::response`. [F040] (`ec9015e26`)

- **`OnceLock<Regex>` replaced with `LazyLock<Regex>`** in `cache/uuid_extractor.rs`.
  [F027] (`ccd25ee97`)

- **`compute_response_cache_key` and `validate_query` extracted helpers** — pure
  refactors that do not change behaviour but reduce duplication. [F023] (`cf3a24c2e`)

- **Workspace dependency consolidation** — `redis`, `chrono`, `dashmap`, `uuid`, `url`
  moved to `[workspace.dependencies]`; the four per-crate `redis` declarations and
  multiple per-crate raw declarations replaced with `workspace = true`. `dashmap`
  workspace version bumped from `6.0` to `6.1` to match the version the resolver was
  already picking. `fraiseql-functions` `reqwest` declaration aligned with the
  workspace rustls-tls posture (drops native-tls / openssl-sys from the dependency
  tree). [F015, F033, F034] (`8278defdc`, `a0e37c15d`, `23d4a18ea`)

- **`cargo ci` alias and `make ci` target** — chains the strict workspace clippy gate
  with `nextest run --workspace --all-features`. [F035] (`d04068d34`)

- **`mold` linker opt-in documented** — `.cargo/config.linker.example.toml` template
  added; the in-tree `.cargo/config.toml` stays commented for CI compatibility.
  [F022] (`598231ae4`)

- **Cargo production dependencies** — non-breaking bumps across the workspace.
- **GitHub Actions** — checkout v4→v6, setup-java v4→v5, setup-go v5→v6,
  upload-artifact v6→v7, setup-uv v5→v7 across 35 workflow files.
- **Pre-commit hooks** — markdownlint-cli v0.48.0, actionlint v1.7.12,
  `stages: [push]` → `stages: [pre-push]` for pre-commit v4.
- **`UsageAggregator.backend`** upgraded to `RwLock<Arc<dyn UsageBackend>>` for
  runtime backend swapping.
- **`UNSUPPORTED_OPERATION` API error code** now maps to HTTP 501 (Not Implemented)
  instead of 500.
- **CVE-related dependency bumps** — `rmcp` 0.16→1.4 (CVE-2026-42559), fuzz
  `jsonwebtoken` 9→10 (CVE-2026-25537), `thrift` removed from default Parquet build
  (CVE-2026-43868 feature-gated). (`cd81b00b4`, `1ab380f58`, `dc9c88bbe`)
- **Newtype wrappers for domain identifiers** — additional newtypes introduced and
  prelude unified to chain exports across crates. (`e70162117`, `158a46a0d`)
- **Construction patterns standardised** — public DTOs gain `new()` constructors with
  builder support; `#[non_exhaustive]` added to `CacheStatus` and `EventKind`.
  (`dbc9e0afc`, `e2b9944d2`, `3d8c4bce6`)

### Known Limitations Update

- **Pool Pressure Monitor** — confirmed that neither `deadpool-postgres` nor
  `bb8-postgres` (as of 2026-05) support runtime pool resizing. The
  `PoolPressureMonitor` remains in recommendation-only mode.
- **Q4 workspace `indexing_slicing` rollout is in progress** — three pilot crates
  (`fraiseql-error`, `fraiseql-wire`, `fraiseql-storage`) deny the lint at the crate
  root; the remaining 13 crates are scheduled across v2.3.x point releases. See
  `FOLLOW_UPS.md` for the per-crate hit-count table and rollout order.

### Deferred to v2.4

- **`F031` property tests cover no-DB executor entry points only** — the full
  `Executor::execute` end-to-end pipeline (RLS composition, projection, cache
  warm/cold) needs a mock `DatabaseAdapter` and is deferred. See `FOLLOW_UPS.md`.

## [2.2.0] - 2026-05-02

### Fixed

- **Native column support in aggregation `WHERE`, `GROUP BY`, and `ORDER BY`**.
  Aggregation queries on views with both native SQL columns and a JSONB `data` column
  now correctly reference native columns directly (`"col"`) instead of using JSONB
  extraction (`data->>'col'`). This enables btree index usage and fixes the PostgreSQL
  error `column "v_foo.data" must appear in the GROUP BY clause`
  (fraiseql/fraiseql-python#337). All four database dialects are covered.

### Changed (breaking)

- **Mutation response format consolidated** — the versioned `schema_version`
  dispatch has been removed. `app.mutation_response` is now a single canonical
  format with typed, column-per-concern fields (`succeeded`, `state_changed`,
  `error_class`, `entity`, `cascade`, etc.). The old v1 string-status parser,
  the v2 version-dispatch shim, and the `MutationOutcome::Error.status` string
  field are all gone. `MutationOutcome::Error` carries a typed
  `error_class: MutationErrorClass` directly.

  **Why:** FraiseQL has no external consumers yet — we are the sole users.
  Neither v1 nor cascade were ever used in production. Collapsing to a single
  greenfield format removes ~300 lines of dead-weight parsing and version
  negotiation, giving future users a clean starting point with no migration debt.

### Added

- **Multi-tenancy support** — per-tenant executor isolation with lock-free reads.
  Each tenant gets its own compiled schema and database connection, dispatched via
  `X-Tenant-ID` header, JWT `tenant_id` claim, or Host-header domain registry.
  Management API: `PUT/DELETE /api/v1/admin/tenants/{key}` (upsert/remove),
  `GET /api/v1/admin/tenants` (list), `GET /api/v1/admin/tenants/{key}/health`,
  `PUT/DELETE /api/v1/admin/domains/{domain}`, `GET /api/v1/admin/domains`.
  ArcSwap-based hot-reload: in-flight requests complete on the old executor while
  new requests use the updated schema. Single-tenant mode is unaffected (zero overhead
  when multi-tenancy is not configured). Security: explicit-but-unregistered tenant
  keys return 403 Forbidden, never the default tenant's data.

- **Three-state update semantics for CRUD mutations** (#221, `29a2c4da8`).
  Update mutations now distinguish between absent (field not mentioned),
  explicit null (set to NULL), and value (set to new value) via the GraphQL
  variable-omission convention. CRUD naming configuration added to
  `fraiseql.toml`.

- **`computed=True` field marker for CRUD input exclusion** (#222). Python SDK
  (`e6dab114e`), TypeScript (`0ebc702f2`), Java (`e62cf9b86`), C#, Dart,
  Elixir, F#, PHP, Ruby (`ccb9607a4`) SDKs all support `computed` fields that
  are excluded from generated CRUD input types (e.g. `created_at`,
  `updated_at`).

- **`not_found` error status for mutations** (`d6392732d`). Mutation responses
  can now return a `not_found` status distinct from generic failure, enabling
  clients to distinguish "entity does not exist" from other error conditions.

- **Session variables injected before read queries** (#218, `45be17e34`).
  `set_config()` session variable propagation now applies to read queries, not
  only mutations, so RLS policies on SELECT can reference `current_setting()`.

- **Cross-SDK parity CI** (`118bf496d`, `2660603bd`). Cross-SDK generators and
  CI jobs added for Java, Ruby, Dart, C#, F#, Rust, PHP, and Elixir SDKs.

- **Apollo Federation 2 — full directive set** (`d78611a94`). `service_sdl.rs`
  now emits all 7 field-level directives (`@external`, `@requires`, `@provides`,
  `@shareable`, `@inaccessible`, `@override`, `@extends`) with correct `extend type`
  syntax for `is_extends: true` types. `@link` import list is complete. Python and
  TypeScript SDKs expose `FieldConfig(external=, requires=, provides=, shareable=,
  inaccessible=, override_from=)` with validation matching spec rules.

- **Federation constraint validation** — `fraiseql federation check` validates
  `@key` field existence, `@override(from:)` non-empty subgraph name, `@requires`
  target field existence, and `@provides` consistency. Unknown-subgraph overrides
  are reported as errors when `--against` is supplied.

- **Federated subscription passthrough** — `SubscriptionForwarder` proxies
  subscriptions to the owning subgraph via the `graphql-transport-ws` WebSocket
  protocol. SSRF protection applied on all remote URLs. Remote subscription field
  ownership tracked via `remote_subscription_fields` on `FederationMetadata`.

- **Federation plan visualization** — `GET /admin/v1/federation/plan?query=...`
  returns the cached query plan as JSON, enabling gateway debuggability.

- **Prometheus federation metrics** — `fraiseql_federation_subgraph_latency_seconds`
  histogram and `fraiseql_federation_entity_resolution_total` counter wired in
  `fraiseql-federation/src/observability.rs`.

- **Mutation audit tracing** — the runtime emits a structured
  `tracing::info!(target: "fraiseql::mutation_audit", ...)` event after every
  successful mutation, carrying `tenant_id`, `entity_type`, `operation`, and
  `duration_us`. Consumed by the in-process `MutationAuditLayer`.

- **Usage aggregation store** — `MutationAuditLayer` subscribes to audit events
  and maintains per-tenant, per-period, per-entity-type counters in a lock-free
  `DashMap`. Exposed via `GET /api/v1/admin/usage?tenant_id=…&period=…`.

- **Schema metadata endpoint** — `GET /api/v1/schema/metadata` returns the
  compiled schema's version, entity count, query count, mutation count, and
  field-level security metadata (required scopes, deny policy, deprecated status)
  in a stable JSON envelope.

- **`fraiseql schema metadata` CLI subcommand** — prints or JSON-outputs the
  compiled schema's security metadata; `fraiseql federation check --json` flag
  emits structured JSON errors for CI pipelines.

- **Structured CLI error output** — non-zero-exit CLI errors now emit a JSON
  envelope `{"error": "…", "code": "…", "details": {…}}` when `--json` is passed,
  enabling machine-readable CI integration.

### Fixed

- **`inject_params` now respects `native_columns`** (#219, `bdc00905f`).
  Injected parameters (e.g. tenant isolation via `inject: { tenant_id:
  "jwt:org_id" }`) previously always used JSONB extraction
  (`data->>'col' = $N`). When the column exists as a native column on the
  backing view, the query now emits `col = $N::type` instead, enabling
  B-tree index usage.

- **Python SDK CRUD `sql_source` no longer adds spurious `fn_` prefix**
  (`c07e12875`). Auto-generated `sql_source` from `crud=True` mutations
  dropped the `fn_` prefix that was incorrectly prepended.

### Changed

- **Vendored `graphql-parser` removed** (`a9221463c`, `36615f6e1`). The
  in-tree vendored copy and drift tooling have been removed; the workspace
  now depends on the upstream crates.io release.

- **3 patched CVEs removed from `.trivyignore`** (`d85a3822b`).
  CVE-2025-14104 (util-linux), CVE-2025-6141 (ncurses), and CVE-2024-56433
  (shadow-utils) now have Debian fixes; next image rebuild picks them up.

---

## [2.1.6] - 2026-04-14

### Added

- **Session variables via PostgreSQL `set_config()`** (#97). The executor now
  propagates per-request session variables (`user_id`, `tenant_id`, roles, and
  arbitrary custom attributes from `SecurityContext`) into the PostgreSQL session
  via `set_config(name, value, is_local=true)`, so RLS policies and SQL functions
  can read `current_setting('fraiseql.user_id')` etc. without a separate round-trip.
- **Schema naming-convention support for GraphQL operations** (#216). The
  compiler accepts an explicit naming convention (camelCase / snake_case) for
  generated query, mutation, and subscription operation names, so authoring
  languages with different conventions emit a consistent GraphQL surface.
- **Nested relation filters via automatic FK resolution** (#196). Where-clause
  inputs can now traverse foreign-key relations (e.g. `where: { post: { author:
  { name: { eq: "..." } } } }`) and the compiler resolves the join path from
  FK metadata rather than requiring an explicit subquery. `c2ae22ef5` further
  simplifies the nested path to a multi-segment path.
- **HS256 auth mode exposed for integration testing** (#217). Server
  configuration accepts an HS256 shared-secret auth mode alongside the existing
  OIDC/JWKS path, so test harnesses can mint tokens locally without a mock
  identity provider.

### Changed

- **Removed dead Cargo features**: `cors`, `database`, and `rich-filters`
  features that were defined but no longer wired to any code have been removed
  from the workspace.
- **`fraiseql-server` CLI now uses `clap`** (#213). `fraiseql-server` and
  `fraiseql run` share a `ServerArgs` definition; `clap` is feature-gated in
  `fraiseql-cli` so the `fraiseql run` ergonomics are preserved for embedding.
- **`__typename` detection moved to `ResultProjector`** (#212). Detection is
  consolidated at the projection layer and the executor gains a
  `federation_mode` switch so Apollo Federation subgraphs produce
  `__typename`-annotated payloads without duplicated detection logic.
- **`orderBy` SQL generation rewritten as a shared builder** (#211). A shared
  builder fixes a cache-key bug (previously colliding on same fields with
  different directions) and emits type-aware SQL casts so ordering by
  `NUMERIC`/`TIMESTAMPTZ` columns produces correct comparisons.
- **Mutation error projection unified via `ProjectionMapper`** (#215). The two
  divergent mutation-result and error-union projection paths were consolidated
  onto a single mapper; behaviour is preserved but the code path is now shared.

### Fixed

- **Mutation error-union inline fragments, array fields, and selection
  filtering** (#214). Inline fragments on error unions, array fields inside
  mutation payloads, and nested selection filtering all projected incorrectly
  in specific shapes; all three now round-trip through `ProjectionMapper`.
- **`__typename` filtered from SQL projection; `orderBy` snake_case keys
  accepted** (`d9c415fff`). `__typename` is a GraphQL-layer concern and must
  never appear in the SQL SELECT list; `orderBy` now accepts snake_case keys
  in addition to the camelCase form.
- **Issues #206–#209** (`74c9d8d21`): `orderBy` regression on composite types,
  stray `__typename` in SQL, `--config` CLI flag lookup, and array-field
  projection edge cases.
- **Issues #195–#204** (`6a024c3d4`): projection types for scalars behind
  nullable wrappers, camelCase key preservation through the executor, and
  input-object round-tripping in mutation arguments.
- **SDKs: snake_case → camelCase auto-conversion** (`ca9e76b29`). Python, Ruby,
  and Dart authoring SDKs now auto-convert snake_case field names to the
  camelCase form the compiler expects, matching the behaviour of the
  TypeScript and Go SDKs.
- **SDK manifests aligned to 2.1.6**: Dart, Elixir, Go, Java, PHP, Ruby, C#
  (`FraiseQL` + `FraiseQL.Tool`), F#, and Rust authoring SDK version strings
  bumped to match the workspace release.

### Performance

- **Eliminated `serde_json` string round-trip in executor** (#153). All executor
  methods now return `serde_json::Value` directly instead of serializing to `String` and
  immediately deserializing again on every request. Touched 26 files across
  `fraiseql-core`, `fraiseql-server`, and `fraiseql-arrow`.

- **Parsed-query AST cache on `Executor`** (#153). Repeated identical query strings skip
  the full lexer + recursive-descent parse. A lock-free `moka` cache keyed by xxHash64 of
  the query string returns an `Arc<(QueryType, Option<ParsedQuery>)>` in nanoseconds. Only
  successful parses are cached; errors are never stored. Capacity: 1 024 distinct query
  strings.

- **Executor-level response cache** (#156). An optional second cache tier above the
  adapter-level row cache. On a hit, the entire projection + RBAC + envelope-wrapping
  pipeline is skipped — only an `Arc::clone`. Keyed by `(query_hash,
  security_context_hash)`; the security hash covers `user_id`, roles, `tenant_id`, scopes,
  and custom `attributes`, so users never see each other's cached data. View-based
  invalidation via a `DashMap` reverse index (O(k), no full-cache scan). Opt-in via
  `ResponseCacheConfig`; disabled by default.

- **TCP_NODELAY + gated compression on GraphQL route** (#157). Enables `TCP_NODELAY` to
  eliminate Nagle-algorithm buffering on response frames. Adds a `CompressionLayer` to the
  GraphQL and REST routers, gated on `compression_enabled` (see *Changed* below).

### Changed (breaking default)

- **`compression_enabled` now defaults to `false`** (was `true` earlier in this release
  cycle). FraiseQL is overwhelmingly deployed behind a reverse proxy (Nginx, Caddy, cloud
  load balancer) that already handles compression — often with brotli, shared across
  upstreams, and with static-asset caching. Framework-level gzip duplicated that work and
  silently cost 3× RPS on TEXT-heavy GraphQL responses under concurrency. Single-binary /
  no-proxy deployments can opt back in with `compression_enabled = true` in `fraiseql.toml`.
- **Compression now skips responses under 1 KiB** when enabled. tiny payloads (short
  GraphQL results, health responses) pay no compressor overhead.

---

## [2.1.5] - 2026-04-12

### Added

- **`GET /auth/me` session-identity endpoint** (issue #193). Frontends using the PKCE cookie
  flow had no way to ask "who am I?" because the JWT is stored in an `HttpOnly` cookie
  inaccessible to client-side script. The new endpoint reflects a configurable subset of the
  validated session's JWT claims as JSON. Enable opt-in via `[auth.me]` in the compiled
  schema:

  ```toml
  [auth.me]
  enabled = true
  expose_claims = ["email", "tenant_id", "https://myapp.com/role"]
  ```

  The response always includes `sub`, `user_id` (alias for `sub`), and `expires_at`. Extra
  fields are included only when listed in `expose_claims` **and** present in the token —
  absent claims are silently omitted, never `null`-padded. No enrichment callbacks, no
  external calls: the endpoint reads only from the already-validated JWT.

  `oidc_auth_middleware` now also accepts tokens from the `__Host-access_token` cookie as a
  fallback when no `Authorization: Bearer` header is present, enabling the middleware to
  protect the new endpoint in browser flows.

  `AuthenticatedUser` gains an `extra_claims: HashMap<String, serde_json::Value>` field,
  populated by the OIDC validation path from a new `#[serde(flatten)] extra` field on
  `JwtClaims`. Custom OIDC claims (e.g. `"email"`, namespaced URL-form claims) that
  previously fell off the floor during JWT validation are now preserved end-to-end.

### Fixed

- **Input types not recognised as valid mutation argument types** (issue #190). The CLI
  schema converter and validator built their known-type sets from object types, interfaces,
  and scalars but omitted input types. A mutation argument declared as a custom input type
  (e.g. `CreateUserInput`) was incorrectly rejected as an unknown type reference. Both
  `SchemaConverter` and `SchemaValidator` now include input types in the valid-type set.

- **Server did not auto-select relay pagination when schema has relay queries** (issue #191).
  `Server::new` does not enable the Relay cursor pagination runtime; operators had to
  explicitly call `Server::with_relay_pagination`. The binary entrypoint now inspects the
  compiled schema at startup and selects `with_relay_pagination` automatically when any query
  carries `relay: true`.

### Changed

- **Relay cursor doc-comments clarified**: the `encode_edge_cursor`, `encode_uuid_cursor`,
  and `encode_node_id` functions now document that base64 is encoding, not encryption — a
  client that decodes the cursor will see the raw integer PK, UUID, or `TypeName:uuid`
  string. The Relay spec requires cursors to be treated as opaque by convention only; no
  cryptographic guarantee is provided.

---

## [2.1.4] - 2026-04-11

### Added

- **Recursive JSONB sub-field projection via `jsonb_build_object`**. Composite fields with
  a `sub_fields` list now emit a nested `jsonb_build_object(...)` instead of returning the
  full JSONB blob, eliminating over-fetching for deeply nested types. Recursion is capped at
  4 levels; deeper fields and list fields fall back to the full-blob path.
  `ProjectionField` gains a `composite_with_sub_fields` constructor and
  `sub_fields: Option<Vec<ProjectionField>>`.

- **APQ (Automatic Persisted Queries) mutation end-to-end test**. Covers the full
  store-on-miss → retrieve-on-hit cycle for mutations, guarding the APQ cache path that was
  previously untested in integration. Adds ADR-0010 documenting the async mutation handler
  design decision.

- **JWT replay counters exposed on Prometheus `/metrics` endpoint**.
  `fraiseql_jwt_replay_rejected_total` and `fraiseql_jwt_replay_cache_errors_total` are now
  registered as Prometheus counters, completing the observability story for JWT replay
  prevention (plan 01). A flaky test assertion on shared `AtomicU64` counters is also fixed.

### Fixed

- **Stale list queries after UPDATE/DELETE targeting a non-first row** (correctness bug).
  `QueryResultCache::put_arc` previously indexed only `result[0]` in `entity_index`. For a
  list query returning N rows, entities at positions 1…N-1 were invisible to
  `invalidate_by_entity`, leaving the stale list result in cache after a mutation. All rows
  are now indexed.

- **Unnecessary point-lookup eviction on CREATE** (performance bug). CREATE mutations called
  `invalidate_views()`, which evicted every cache entry for the view — including
  single-entity point-lookup entries for existing entities that are completely unaffected by
  the newly created row. CREATE now calls `invalidate_list_queries()`, which evicts only
  multi-row list entries via a dedicated `list_index`. Expected cache hit-rate improvement
  under mixed read+write workloads: ~60–70 % → ~85–95 %.

### Changed

- **`CachedResult` struct**: `entity_ref: Option<(String, String)>` replaced by
  `entity_refs: Box<[(String, String)]>` (one entry per row) and `is_list_query: bool`.
  The `invalidate_by_entity` fast path now short-circuits when the entity type has no
  indexed entries, making write-heavy workloads with no cached reads a near-zero-cost no-op.

---

## [2.1.3] - 2026-04-08

### Performance

- **`QueryResultCache` replaced with `moka` W-TinyLFU** (issue #185). Cache reads are now
  lock-free — eliminates hot-key serialisation under high concurrency. View-based and
  entity-based invalidation use O(k) reverse `DashMap` indexes instead of an O(n) full-cache
  scan. `lru` crate usage in the cache module removed. `CachedResult::entity_ids` replaced
  with `entity_ref: Option<(String, String)>`; `CachedResult::hit_count` removed.

- **`Arc<CachedResult>` in cache store eliminates per-hit deep clone.** The moka store
  type changed from `Cache<u64, CachedResult>` to `Cache<u64, Arc<CachedResult>>`. On a
  cache hit, only one atomic reference-count increment occurs; previously `moka::Cache::get`
  deep-cloned the full `CachedResult` value — including the `Box<[String]>` view list — on
  every read.

- **Zero-allocation cache key generation.** `generate_view_query_key` and
  `generate_projection_query_key` replace the previous `format!` + `serde_json::json!` +
  `generate_cache_key` chain on every cache lookup. Parameters are hashed directly via
  ahash with no intermediate `String` or `serde_json::Value` allocations — zero heap
  activity on the hot read path.

- **Short-circuit when cache is disabled removes per-request overhead.** When
  `cache_enabled = false`, `execute_where_query` and `execute_with_projection` skip the
  64-shard lock scan, `CascadeInvalidator` mutex acquisition, and `is_enabled()` check,
  reducing the disabled-cache overhead to a single branch.

### Changed

- **`Server::new` and `Server::with_relay_pagination` now always wrap the database adapter in `CachedDatabaseAdapter`** (issue #184). When `cache_enabled = false` the adapter acts as a zero-overhead passthrough; when `cache_enabled = true` full query result caching is active.
- **`CacheStatus::RlsGuardOnly` deprecated** — the variant is no longer accurate now that `CachedDatabaseAdapter` is always wired. Admin config endpoint returns `active` when `cache_enabled = true`.
- **Startup log updated** — when `cache_enabled = true` the server now logs `"Query result cache: active"` with `max_entries`, `ttl_seconds`, and `rls_enforcement`; when disabled it logs `"Query result cache: disabled"`.

### Fixed

- **`pool_min_size` now pre-warms the connection pool at startup** (issue #183).
  Previously the parameter was silently dropped (`_min_size`); deadpool would lazily
  open connections on the first request, causing high mutation latency under concurrent
  cold-start load. This was the root cause of the 5.5× mutation throughput gap observed
  in benchmarks. After `Server::new` returns, `pool_min_size` live connections are ready.

- **`pool_timeout_secs` is now applied as the deadpool wait and create timeout** (issue #183).
  Previously the parameter was stored in `ServerConfig` but never forwarded to the pool,
  meaning connection acquisition could block indefinitely on pool exhaustion. With a timeout
  set, pool exhaustion now returns an actionable error within `pool_timeout_secs` seconds
  instead of blocking the request indefinitely.

- **`acquire_connection_with_retry` no longer retries on `PoolError::Timeout`** (issue #183).
  A timeout means the pool was genuinely exhausted for the full wait period; retrying would
  only multiply the wait by `MAX_CONNECTION_RETRIES`. Only transient backend/create errors
  are retried with exponential backoff.

- **`cache_enabled = true` now logs a clear startup message** (issue #183).
  Previously the flag silently had no observable effect on query execution (the full
  `CachedDatabaseAdapter` wire-up is a separate future PR). The server now logs whether
  the RLS safety guard is active, making the current semantics visible to operators.

- **Observer pool no longer inherits application pool size** (issue #183).
  Previously `build_observer_pool` used `pool_min_size` / `pool_max_size` from the
  top-level config. The observer runtime needs far fewer connections (LISTEN/NOTIFY
  - metadata queries). New defaults: `min=2, max=5, acquire_timeout=10s`. Configure
  independently via `[observers.pool]` in `fraiseql.toml` — see `DEPRECATIONS.md`.

### Added

- **`PoolPrewarmConfig` struct** (`fraiseql_db::postgres::PoolPrewarmConfig`) — replaces
  the positional `(min_size, max_size)` arguments on `PostgresAdapter::with_pool_config`.
  Carries `min_size`, `max_size`, and `timeout_secs` in a single self-documenting struct.

- **`CacheStatus` enum** (`fraiseql_server::routes::api::admin::CacheStatus`) with variants
  `Disabled`, `RlsGuardOnly`, `Active`. The admin `/api/v1/admin/config` endpoint now
  includes a `cache_status` field with the serialized enum value.

- **`ObserverPoolConfig` struct** (`fraiseql_server::server_config::ObserverPoolConfig`) for
  independent tuning of the observer's dedicated PostgreSQL pool via `[observers.pool]` in
  `fraiseql.toml`.

- **`pool_timeout_secs = 0` is now a validation error.** A zero-second timeout would cause
  every connection acquisition to fail immediately; the server now rejects this configuration
  at startup with a clear error message.

## [2.1.0] - 2026-03-30

First public release of FraiseQL v2 — a compiled GraphQL execution engine that
transforms schema definitions into optimized SQL at build time.

### Added

#### Core Engine (`fraiseql-core`)

- GraphQL-to-SQL compilation engine with build-time schema optimization
- Multi-database support: PostgreSQL (primary), MySQL, SQLite, SQL Server
- Relay Cursor Connections spec: keyset pagination on PostgreSQL, MySQL (v2.1),
  SQL Server (forward v2.0, backward v2.1); `totalCount` via fragment spreads
- Automatic Persisted Queries (APQ) with Redis-backed cache and smart invalidation
- 64-shard LRU result cache with per-entry TTL and cascade invalidation
- Row-level security (RLS): native PostgreSQL RLS or SQL WHERE injection on
  MySQL/SQLite/SQL Server — always AND-ed with application WHERE clauses
- Server-side context injection (`inject={"param": "jwt:<claim>"}`) for
  query/mutation parameter binding from JWT claims
- Typed mutation error variants with scalar field population from JSONB metadata
- `auto_params` inference: list queries automatically gain `limit`, `offset`,
  `where`, and `order_by` parameters unless explicitly overridden
- Domain-specific newtypes: `TypeName`, `FieldName`, `SqlSource`, `RoleName`,
  `Scope` replace bare strings with compile-time type safety
- `FraiseQLError::Unsupported` variant (HTTP 501) for operations not supported
  by the current database backend
- `prelude` module for ergonomic single-import access to common types
- Multi-root query pipelining with parallel execution via `try_join_all`
- AST-based `RequestValidator` replacing the character-scan `ComplexityAnalyzer`
  with correct depth, complexity, and alias-count metrics
- `QueryValidator` wired into `Executor::execute()` for DoS protection without
  requiring `fraiseql-server`

#### Server (`fraiseql-server`)

- Generic `Server<DatabaseAdapter>` with type-safe database swapping
- Graceful schema hot-reload via ArcSwap (zero-downtime config changes)
- PKCE OAuth routes (`/auth/start`, `/auth/callback`) with encrypted state tokens
- OIDC/JWKS authentication with provider error sanitization
- Per-user and per-IP rate limiting with proxy-aware IP extraction and accurate
  `Retry-After` headers; path-specific rate rules for auth endpoints
- Redis backends for PKCE state store (`redis-pkce`) and rate limiting
  (`redis-rate-limiting`) for production clustering
- Cookie security hardening: `__Host-` prefix, RFC 6265 quoting, conservative
  `Max-Age` defaults, `redirect_uri` length cap
- RBAC management API with field-level authorization
- `[server]` and `[database]` runtime configuration via `fraiseql.toml` with
  CLI flags > env vars > TOML > defaults precedence
- CSRF `Content-Type` enforcement and request body size limits
- API key authentication and token revocation
- Admin endpoints: `POST /api/v1/admin/explain` for query analysis,
  `/validate` with real parser errors
- Health check endpoint for load balancers
- Pool pressure monitoring with Prometheus metrics and scaling recommendations
- `PoolPressureMonitorConfig` (replaces deprecated `PoolTuningConfig`)
- Consistent boolean parsing for all `FRAISEQL_*` environment variables

#### Database Adapters (`fraiseql-db`)

- PostgreSQL: full feature support including JSONB fact tables, LISTEN/NOTIFY
  subscriptions, native RLS, window functions
- MySQL: SELECT, mutations, Relay pagination (forward/backward), aggregates,
  field-level encryption, federation; `JSON_UNQUOTE`/`JSON_EXTRACT` for cursors
- SQL Server: SELECT, mutations, Relay pagination (forward/backward), aggregates,
  field-level encryption, federation; SQLSTATE error code mapping (23505, 23502,
  23503, 40001, 22001); `UNIQUEIDENTIFIER` cursor support
- SQLite: read-only queries, aggregates (limited), APQ, RLS via SQL WHERE;
  `execute_function_call` returns `Unsupported` with named function
- Rich scalar type filters (6 of 44 planned types implemented)
- `SupportsMutations` trait (replaces `MutationCapable`)

#### Federation (`fraiseql-federation`)

- Extracted crate (26 files, 10,257 lines) for Apollo Federation v2
- Per-entity circuit breaker with configurable failure thresholds, half-open
  recovery, and success windows
- SAGA transaction support
- Entity type resolution and federated query planning
- `MAX_ENTITIES_BATCH_SIZE = 1_000` guard

#### Wire Protocol (`fraiseql-wire`)

- PostgreSQL wire protocol streaming for fraiseql-wire
- `MAX_FIELD_COUNT = 2_048` in `decode_data_row` / `decode_row_description`
- Property-based tests for protocol encoding round-trips
- Hardened decoder against malformed messages

#### Arrow Flight (`fraiseql-arrow`)

- Apache Arrow Flight data plane for high-throughput data export
- `ArrowDatabaseAdapter` and `ArrowEventStorage` traits
- Event storage, export, and subscription support
- Schema refresh with streaming updates

#### Observers (`fraiseql-observers`)

- Event-driven observer system with NATS backend and enterprise HA
- `CheckpointStrategy` enum: `AtLeastOnce` (fast, idempotent consumers) and
  `EffectivelyOnce` (idempotency key deduplication via `ON CONFLICT DO NOTHING`)
- Storage layer with automatic observer triggering
- Cache backend integration

#### Security (`fraiseql-auth`, `fraiseql-secrets`)

- Audit logging with PostgreSQL and syslog backends
- Field-level encryption-at-rest
- Credential rotation automation with monitoring
- HashiCorp Vault integration with multiple secret backends
- Zeroizing wrapper for sensitive key material
- Constant-time comparison via `subtle` crate
- `OsRng` for all cryptographic nonce generation
- SECURITY.md with vulnerability reporting procedures and compliance profiles
  (STANDARD, REGULATED, RESTRICTED)

#### CLI (`fraiseql-cli`)

- Commands: `compile`, `lint`, `analyze`, `cost`, `dependency-graph`, `generate`,
  `generate-views`, `introspect`, `migrate`, `sbom`, `explain`,
  `validate-documents`
- MCP server integration via `FRAISEQL_MCP_STDIO` env var
- Trusted document store with TOML config and CLI validation
- Decoupled from `fraiseql-server` via `run-server` feature flag — build with
  `--no-default-features` for a pure compile-only binary
- "Did you mean?" suggestions for mutation-not-found and fact-table-not-found errors

#### SDKs (11 languages)

- **Python**: `AsyncFraiseQLClient` with retry, typed error hierarchy, LangChain +
  LlamaIndex integrations; full ruff ruleset, `[tool.ty]` config
- **TypeScript** (`@fraiseql/client`): async HTTP client, typed errors, Vercel AI
  SDK / LangChain.js / Mastra integrations; `noUncheckedIndexedAccess`,
  `no-explicit-any: error`, vitest (282 tests)
- **Go**: HTTP client with retry, typed errors, OpenAI / Anthropic tool converters
- **Java**: `FraiseQLClient`, exception hierarchy, Spring AI + LangChain4j stubs
- **C#**: attribute-driven authoring (`[GraphQLType]`, `[GraphQLField]`),
  `SchemaExporter`, `dotnet tool` CLI, Semantic Kernel integration (103 tests)
- **F#**: dual authoring (attributes + computation expression DSL),
  `SchemaExporter`, `dotnet tool` CLI, Semantic Kernel integration (133 tests)
- **PHP**: `FraiseQLClient` with retry, PSR-18 adapter, OpenAI PHP / Prism
  integrations, `SchemaExporter` + CLI binary
- **Elixir**: compile-time macro DSL (`use FraiseQL.Schema`), `mix fraiseql.export`,
  Dialyzer + Credo strict CI (98+ tests)
- **Ruby**: `FraiseQL::Client` (Net::HTTP), ruby-openai + LangChain.rb integrations
- **Dart/Flutter**: `FraiseQLClient` with `authorizationFactory`, Google Gemini /
  Firebase Vertex AI integration
- **Rust** (`fraiseql-client`): `FraiseQLClientBuilder` with async query/mutate/
  subscribe, Candle ML integration
- All 11 SDKs forward `operationName` in requests
- All 11 SDKs ship GitHub Actions CI workflows (`.github/workflows/`)
- Cross-SDK parity test suite: 1,595 tests across 9 SDKs against golden fixtures

#### Observability

- Prometheus metrics: query latency percentiles, connection pool health, error rates
- Structured JSON logging with correlation IDs
- OpenTelemetry distributed tracing integration
- Pre-built 12-panel Grafana 10+ performance dashboard
- Per-operation metrics and real query EXPLAIN

#### Testing & Quality

- 5,326 passing tests; `cargo clippy --workspace --all-targets --all-features
  -- -D warnings` clean; `cargo deny check` clean
- Criterion benchmark suite: GraphQL parse, cache latency, full-pipeline
- Fuzz harnesses: GraphQL parser, wire protocol, SCRAM auth, schema
  deserialization, SQL codegen
- Property-based testing: 101 properties
- k6 load testing: queries, mutations, mixed workload, auth, APQ scenarios
- E2E pipeline test (`make e2e`): Python authoring → CLI compile → server → SDK
- 34 SQL snapshot tests (WHERE operators, CTE, JSON, FTS, aggregate dialects)
- Docker Compose test infrastructure (`docker/docker-compose.test.yml`) with
  6 CI integration jobs (Redis, NATS, TLS, Vault, observers, server)
- `testcontainers` watchdog for container cleanup on SIGTERM/SIGINT
- 12 operational runbooks; SLA/SLO documentation
- `cargo semver-checks` in CI for API compatibility

#### Configuration & Deployment

- `fraiseql.toml` configuration compiled into `schema.compiled.json` with
  environment variable overrides for production
- Docker multi-stage builds (Alpine base, ~15 MB compressed)
- Kubernetes manifests with Helm charts
- `fraiseql` umbrella crate with feature bundles: `full` (all components),
  `minimal` (core only)
- TLS consolidated to rustls; `native-tls` removed from dependency tree

### Changed

- `ComplexityAnalyzer` replaced by AST-based `RequestValidator` — the old
  character-scan miscounted operation names, argument names, and directive names
  as field selectors
- `QueryMetrics` fields changed: `depth`, `complexity`, `alias_count` replace
  the old `depth`, `field_count`, `score` tuple
- `QueryValidatorConfig` gains `max_aliases` field with presets: permissive=100,
  standard=30, strict=10
- `FRAISEQL_INTROSPECTION_REQUIRE_AUTH` uses consistent boolean parsing (`true`,
  `1`, `yes`, `on` only); non-standard truthy values now log a warning
- `fraiseql-auth`, `fraiseql-webhooks`, `fraiseql-secrets` extracted from
  `fraiseql-server` as independent crates
- Redis crate upgraded 0.25 → 0.28
- `lazy_static`/`once_cell` migrated to `std::sync::LazyLock`
- `std::env::set_var` in tests replaced with `temp_env` crate
- `#[non_exhaustive]` on all public enums (except `DatabaseType`)
- All `#[allow(clippy::...)]` carry `// Reason:` justification comments
- Workspace lint config hardened with explicit `missing_errors_doc` enforcement
- `# Errors` doc sections on all fallible public functions across all crates

### Deprecated

- `PoolTuningConfig` (`fraiseql-server`, since v2.0.1) → use
  `PoolPressureMonitorConfig`; removal target: v3.0
- `observers-full` feature flag (`fraiseql-observers`) → list specific
  sub-features (`nats`, `tracing`, `in-memory`, etc.); removal target: v2.2

### Fixed

- `CachedDatabaseAdapter::cache.put()` argument mismatch: three call sites
  passed 4 arguments to a 5-argument signature, silently breaking cache writes
- Entity-aware cache invalidation: UPDATE/DELETE mutations now call
  `invalidate_by_entity` when `entity_id` is present instead of flushing the
  entire view
- Per-user rate limiting was never called — authenticated requests were limited
  by the shared IP bucket; middleware now extracts `sub` claim and routes through
  per-user token bucket
- Proxy-aware IP extraction: `trust_proxy_headers` option reads `X-Real-IP` /
  `X-Forwarded-For` behind reverse proxies
- `Retry-After` accuracy for path-limited responses (e.g. `/auth/start`)
- Cookie charset safety: `Set-Cookie` values now RFC 6265 quoted-string compliant
- Silent `Set-Cookie` omission on parse failure now returns HTTP 500
- Conservative cookie `Max-Age` default (300 s when OIDC omits `expires_in`)
- OIDC provider error strings no longer reflected to clients (mapped to fixed
  allowlist)
- SQL Server relay backward pagination with custom `order_by` now correctly
  flips all sort directions and restores all custom sort columns
- SQL Server relay `totalCount`: missing/empty `COUNT_BIG` result now surfaces
  as `FraiseQLError::Database` instead of silent zero
- SQL Server SQLSTATE codes corrected: 23505 (unique), 23502 (NOT NULL),
  40001 (deadlock) instead of generic 23000
- UUID cursor validation before SQL Server prevents opaque type-conversion errors
- SQLite `execute_function_call` returns `Unsupported` naming the function
- `null` errors array in Python SDK no longer raises `FraiseQLError`
- Mutation `sql_source` falls back to `operation.table` when None
- Connection pool exhaustion in nested queries
- All rustdoc link warnings resolved (zero `cargo doc --no-deps` warnings)

### Security

- `MAX_VARIABLES_COUNT = 1_000` in `RequestValidator`
- PKCE `code_verifier` length guard
- Discord webhook URL validation
- Rate-limit sliding window overflow protection
- Slack URL SSRF check
- `MAX_FIELD_COUNT = 2_048` in wire protocol decoders
- Unix socket path traversal guard (`validate_socket_dir` rejects `..`)
- Federation SSRF URL parser fix (`reqwest::Url::parse` + IPv6 bracket-strip)
- `MAX_ENTITIES_BATCH_SIZE = 1_000` in federation
- `MAX_JWKS_RESPONSE_BYTES = 1 MiB` in OIDC JWKS fetcher
- `MAX_VAULT_SECRET_NAME_BYTES = 1_024` + Vault SSRF URL-parser fix
- `MAX_MANIFEST_BYTES = 10 MiB` in trusted document store
- `MAX_SERIALIZE_DEPTH = 64` in GraphQL parser `serialize_value_inner`
- GET variables string length capped at `max_get_bytes`
- 19 E2E SQL injection prevention tests
- 27 auth bypass and JWT tampering detection tests
- No internal details leaked in error responses (verified by property tests)
