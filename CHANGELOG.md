# Changelog

All notable changes to FraiseQL are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Mutation response v2 parser with `schema_version` dispatch** (`ad60c4789`).
  Mutation functions can now return a v2 envelope with `schema_version: 2`,
  enabling richer response metadata including cascade JSONB. The parser
  auto-detects v1 vs v2 based on the `schema_version` column in the result row.

- **Cascade JSONB surfaced in mutation response envelope** (`57c6b5536`).
  When a mutation function returns a v2 response with a `cascade` JSONB column,
  the cascade data is forwarded through the response envelope to clients,
  enabling downstream invalidation and event propagation.

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

- **Cross-SDK parity CI** (`118bf496d`, `2660603bd`). Phase B generators and
  CI jobs added for Java, Ruby, Dart, C#, F#, Rust, PHP, and Elixir SDKs.

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
  + metadata queries). New defaults: `min=2, max=5, acquire_timeout=10s`. Configure
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
