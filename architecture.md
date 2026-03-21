> **Navigation**: Full documentation index at [`docs/README.md`](docs/README.md) —
> Architecture Decision Records at [`docs/adr/`](docs/adr/)
>
> For the detailed architecture reference, see [docs/architecture/overview.md](docs/architecture/overview.md).

```
╔══════════════════════════════════════════════════════════════════════════════════════════╗
║                        FRAISEQL FRAMEWORK — ARCHITECTURE MAP                             ║
║                        (v2.1.0 — guide for quality domain scans)                         ║
╚══════════════════════════════════════════════════════════════════════════════════════════╝

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 0 — LIFECYCLE PIPELINE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  AUTHORING (Python/TypeScript)          COMPILATION (Rust)          RUNTIME (Rust)
  ──────────────────────────────         ──────────────────          ─────────────────────
  @fraiseql.type                         fraiseql-cli                fraiseql-server
  @fraiseql.mutation      ──────────►  ┌─────────────────┐  ──────► Server<DatabaseAdapter>
  @fraiseql.subscription               │  compile        │           └─ loads from JSON
  @fraiseql.query                      │  validate-docs  │           └─ env var overrides
  fraiseql.field()                     │  generate-views │           └─ pure Rust runtime
         │                             │  introspect     │
         ▼                             │  migrate        │
    schema.json              ──────►   │  lint / analyze │  ──────►  schema.compiled.json
    (types + ops)                      │  cost / sbom    │           (types + SQL + config)
                                       │  explain / init │
                                       └─────────────────┘

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 1 — CRATE DEPENDENCY GRAPH
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

                                   ┌─────────────┐
                          ┌────────│ fraiseql    │ (umbrella re-export crate)
                          │        └──────┬──────┘
                          │               │ uses all
                ┌──────────────────────────────────────────────┐
                │                         │                    │
                ▼                         ▼                    ▼
        ┌───────────────┐       ┌──────────────────┐   ┌─────────────┐
        │ fraiseql-cli  │       │ fraiseql-server  │   │fraiseql-wire│
        └───────┬───────┘       └────────┬─────────┘   └───────┬─────┘
                │                        │                     │
                │          ┌─────────────┼───────────────┐     │
                │          │             │               │     │
                │    ┌─────▼──────┐  ┌───▼──────────┐  ┌─▼─────▼─────┐
                │    │fraiseql-   │  │fr aiseql-    │  │ fraiseql-   │
                │    │ auth       │  │ s ecrets     │  │ observers   │
                │    └─────┬──────┘  └──┬───────────┘  └─────┬───────┘
                │          │            │                    │
                └──────────┴────────────┴────────────────────┘
                                        │ all depend on
                              ┌─────────▼────────┐
                              │  fraiseql-core   │
                              └─────────┬────────┘
                                        │ depends on
                              ┌─────────▼────────┐
                              │   fraiseql-db    │
                              └─────────┬────────┘
                                        │ depends on
                              ┌─────────▼────────┐
                              │  fraiseql-error  │
                              └──────────────────┘

  Optional (feature-gated):
    fraiseql-server ──[feature:arrow]──► fraiseql-arrow ──► fraiseql-core
    fraiseql-server ──[feature:webhooks]─► fraiseql-webhooks
    fraiseql-server ──[feature:observers]─► fraiseql-observers

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 2 — fraiseql-error  (base, no deps)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  FraiseQLError (enum)    Result<T>    ErrorContext    ValidationFieldError
  AuthError  ConfigError  FileError  RuntimeError  WebhookError
  Feature: axum-compat  → IntoResponse impl for axum handlers

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 3 — fraiseql-db  (database adapters)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  traits.rs               DatabaseAdapter trait (core contract)
  introspector.rs         DatabaseIntrospector — schema discovery
  where_clause.rs         WHERE clause builder (SQL generation)
  where_sql_generator.rs  SQL text renderer for WHERE trees
  projection_generator.rs SELECT projection SQL
  path_escape.rs          Identifier escaping (anti-injection) per dialect
  identifier.rs           SQL identifier types
  collation.rs / .._config  Collation + locale handling
  wire_pool.rs            Wire streaming pool

  ┌─ postgres/   ─── Postgres adapter  (primary, most features)
  ├─ mysql/      ─── MySQL adapter
  ├─ sqlite/     ─── SQLite adapter (local dev / testing)
  ├─ sqlserver/  ─── SQL Server adapter (tiberius)
  ├─ filters/    ─── Rich filter operators (ExtendedOperator, ExtendedHandler)
  └─ types/      ─── OrderByClause, OrderDirection, SqlProjectionHint

  Features: postgres(default) | mysql | sqlite | sqlserver
            wire-backend | rich-filters | test-postgres | test-mysql | test-sqlserver

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 4 — fraiseql-core  (query engine)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  ┌─ schema/              Schema type system
  │    compiled/          CompiledSchema — loaded from schema.compiled.json
  │    domain_types.rs    FieldName, TableName, SchemaName (canonical newtypes)
  │    field_type.rs      GraphQL ↔ SQL type mapping
  │    graphql_type_defs  Type definitions
  │    introspection/     __schema / __type resolution
  │    scalar_types.rs    Custom scalars
  │    subscription_types, observer_types, security_config
  │
  ├─ compiler/            schema.json → optimized IR + SQL templates
  │    parser.rs          Schema JSON parsing
  │    validator.rs       Schema validation
  │    lowering.rs        AST → IR lowering
  │    ir.rs              Intermediate representation
  │    codegen.rs         SQL template generation
  │    aggregation.rs     Aggregation SQL (with allowlist injection guard)
  │    fact_table/        Fact table compilation
  │    window_functions/  Window function support
  │    compilation_cache  Incremental cache
  │
  ├─ runtime/             Query execution engine
  │    executor/          Executor<A: DatabaseAdapter>
  │      query.rs         SELECT execution + JSONB strategy dispatch
  │      mutation.rs      Mutation execution
  │      tests.rs         Executor unit tests
  │    executor_adapter.rs ExecutorAdapter trait
  │    planner.rs         Query plan builder (JsonbStrategy selection)
  │    jsonb_strategy.rs  Project | Stream strategy
  │    input_validator.rs Input validation
  │    aggregate_*        Aggregation runtime
  │    window_*           Window function runtime
  │    projection.rs      Field projection
  │    mutation_result.rs Mutation response types
  │    relay.rs           Relay cursor pagination
  │    subscription/      Subscription executor
  │    explain.rs         EXPLAIN ANALYZE integration
  │    sql_logger.rs      Query audit logging
  │    query_tracing.rs   OpenTelemetry span creation
  │    tenant_enforcer.rs RLS enforcement
  │    field_filter.rs    Field-level access control
  │    matcher.rs         Pattern matching
  │
  ├─ graphql/             GraphQL query parsing + validation
  │    parser.rs          Query document parser
  │    fragment_resolver  Fragment flattening
  │    complexity.rs      Query complexity calculation
  │    directive_evaluator @require_permission, @deprecated
  │    require_permission_directive
  │    types.rs           GraphQL AST types
  │
  ├─ cache/               Result caching (64-shard LRU)
  │    result.rs          Per-entry TTL cache shards
  │    adapter/           CachedDatabaseAdapter (wraps DatabaseAdapter)
  │      query.rs         Cache-aware SELECT path
  │      mutation.rs      Invalidation-triggering mutation path
  │    cascade_invalidator  Dependency-graph cache invalidation
  │    cascade_metadata   View dependency metadata
  │    fact_table_cache   Fact table version tracking
  │    relay_cache        Cursor cache
  │    query_analyzer.rs  Cache key extraction
  │
  ├─ security/            Field-level and query-level security
  │    security_context.rs SecurityContext (user + scopes)
  │    rls_policy.rs      Row-Level Security policy application
  │    introspection_enforcer  Schema introspection gating
  │    field_filter.rs    Field visibility enforcement
  │    field_masking.rs   PII masking
  │    query_validator.rs Query allowlist / trusted-documents gate
  │    headers.rs         Security response headers
  │    tls_enforcer.rs    TLS requirement enforcement
  │    error_formatter.rs Error sanitization (hides internals)
  │    audit.rs           Security audit hooks
  │    kms/               Key Management Service integration
  │    auth_middleware/   Auth middleware hooks
  │    oidc.rs            OIDC claim mapping
  │    profiles.rs        Security profiles (strict | permissive | custom)
  │    validation_audit.rs Validation event logging
  │
  ├─ apq/                 Automatic Persisted Queries
  │                       (hash → stored query; Redis-backed optional)
  ├─ federation/          Apollo Federation v2 support
  │                       FederationTraceContext, circuit breaker, health
  ├─ tenancy/             Multi-tenancy (TenantContext, RLS integration)
  ├─ design/              Domain design helpers
  ├─ filters/             Filter expression types
  ├─ utils/               Shared utilities
  │                       operators.rs (OPERATOR_REGISTRY)
  │                       vector.rs (pgvector support)
  │                       opaque_id.rs (OpaqueIdValidator)
  └─ validation/          Request/schema validation

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 5 — fraiseql-server  (HTTP server)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  server/                 Server<A: DatabaseAdapter>
    builder.rs            Builder pattern for server construction
    routing.rs            Axum router assembly
    initialization.rs     Schema loading + subsystem startup
    extensions.rs         Extension registry
    lifecycle.rs          Startup / graceful shutdown

  ┌─ HTTP REQUEST PIPELINE ──────────────────────────────────────────────────────────┐
  │                                                                                   │
  │  Request → [TLS] → [Rate limit] → [CORS] → [Trace] → [Auth] → [Content-type]   │
  │          → [Tenant] → [Metrics] → Route handler → Response                       │
  └───────────────────────────────────────────────────────────────────────────────────┘

  routes/                 Endpoint handlers
    graphql/              POST /graphql  (main execution path)
      handler.rs          Query dispatch → Executor
      api/                APQ hash check / store
    auth.rs               /auth/start, /auth/callback, /auth/refresh
    health.rs             GET /health
    introspection.rs      GET /introspection
    metrics.rs            GET /metrics (Prometheus scrape)
    subscriptions.rs      WS /subscriptions (GraphQL over WebSocket)
    playground.rs         GET /playground (GraphiQL)

  middleware/             Axum middleware layers
    auth.rs               Bearer token validation (BearerAuthState)
    oidc_auth.rs          OIDC token validation (AuthUser)
    cors.rs               CORS + security response headers
    content_type.rs       application/json enforcement
    metrics.rs            Per-request metrics recording
    rate_limit/           Token bucket + Redis rate limiting
    tenant.rs             Tenant header extraction (TenantContext)
    trace.rs              OpenTelemetry span creation

  config/                 Runtime configuration (fraiseql.toml)
    mod.rs                RuntimeConfig struct (deserialized from TOML)
    loader.rs             File loading + env var merge
    env.rs                Environment variable overrides
    cors.rs               CorsConfig
    rate_limiting.rs      RateLimitingConfig, BackpressureConfig
    metrics.rs            MetricsConfig, SloConfig, LatencyTargets
    pool_tuning.rs        PoolTuningConfig
    error_sanitization.rs ErrorSanitizer, ErrorSanitizationConfig
    tracing.rs            TracingConfig
    validation.rs         Validation config

  federation/             Apollo Federation gateway integration
    circuit_breaker.rs    Subgraph circuit breaker (dashmap-backed)
    health_checker.rs     Subgraph health polling
    mod.rs                Federation handler + entity resolution

  subscriptions/          GraphQL subscriptions (WebSocket)
    protocol.rs           graphql-ws sub-protocol
    lifecycle.rs          Connection lifecycle (auth expiry enforcement)
    event_bridge.rs       Backend event routing
    webhook_lifecycle.rs  Webhook-triggered subscription events

  logging.rs              Structured logging (StructuredLogEntry, RequestLogger)
  metrics_server.rs       MetricsCollector (atomic counters), PrometheusMetrics
  tracing_utils.rs        W3C traceparent extraction → FederationTraceContext
  extractors.rs           Axum extractors (RequestId, TenantContext, AuthUser, …)
  validation.rs           RequestValidator (depth, complexity, alias limits)
  trusted_documents.rs    Query allowlist enforcement
  api_key.rs              API key authentication
  token_revocation.rs     Token revocation registry
  tls.rs                  TlsSetup (rustls, tokio-rustls)
  error.rs                ServerError enum

  pool/
    auto_tuner.rs         Connection pool auto-sizing (samples load, adjusts max)
  resilience/
    backpressure.rs       Backpressure shed logic
  server_config.rs        ServerConfig (port, host, TLS paths, limits)

  Optional modules (feature-gated):
    [feature:arrow]       arrow/  → Arrow Flight data delivery adapter
    [feature:observers]   observers/ → Observer event routing
    [feature:mcp]         mcp/   → MCP stdio server (FRAISEQL_MCP_STDIO env)
    [feature:auth]        → re-exports fraiseql-auth
    [feature:secrets]     → re-exports fraiseql-secrets encryption + manager
    [feature:webhooks]    → re-exports fraiseql-webhooks
    [feature:redis-*]     → Redis APQ cache / PKCE store / rate limiter

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 6 — fraiseql-auth  (authentication & authorization)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  jwt.rs                  JWT decode + verify (jsonwebtoken)
  jwks.rs                 JWKS endpoint client + key cache
  pkce.rs                 PKCE code verifier / challenge
  state_store.rs          PKCE state store abstraction
  state_encryption.rs     PKCE state AES-GCM encryption (constant-time)
  session.rs              Session management
  session_postgres.rs     Postgres-backed session store
  audit_logger.rs         AuditLogger (canonical audit sink)
  constant_time.rs        Timing-attack-safe comparison (subtle crate)
  error_sanitizer.rs      Auth error message scrubbing
  operation_rbac.rs       Operation-level RBAC enforcement
  middleware.rs           Auth middleware composition
  monitoring.rs           Auth subsystem health metrics
  security_config.rs      Auth security configuration
  security_init.rs        Subsystem wiring at startup
  provider.rs             OAuthProvider trait
  oidc_provider.rs        OIDC discovery + token endpoint
  oidc_server_client.rs   OIDC client (server-side flow)
  rate_limiting.rs        Auth-specific rate limiting (token bucket)

  oauth/                  OAuth 2.0 flow
    client.rs             HTTP client wrapper
    pkce.rs               OAuth PKCE extension
    failover.rs           Provider failover / retry
    provider.rs           Provider selection
    refresh.rs            Token refresh
    audit.rs              OAuth event auditing
    types.rs              OAuth token types

  providers/              Built-in IdP integrations
    auth0, azure_ad, github, google, keycloak, logto, okta, ory

  handlers.rs             Auth HTTP handlers (split into 6 sub-modules):
    do_get / do_put / do_exchange / metadata / actions / send_helpers

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 7 — fraiseql-secrets  (secrets & encryption)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  secrets_manager/        SecretsManager (trait + orchestrator)
    backends/
      vault.rs            HashiCorp Vault (token renewal, KV v2)
      env.rs              Environment variable backend
      file.rs             File-based backend
    types.rs              Secret types, SecretRef
    mod.rs                Backend dispatch + caching

  encryption/             Field-level encryption
    database_adapter.rs   Encrypted DatabaseAdapter wrapper
    audit_logging.rs      Encryption audit trail
    compliance.rs         Compliance metadata
    credential_rotation.rs VersionedFieldEncryption (key rotation)
    rotation_api.rs       Rotation HTTP API
    refresh_trigger.rs    Automatic refresh scheduling
    error_recovery.rs     Encryption error recovery
    middleware.rs         Encryption middleware
    mapper.rs             Field ↔ key mapping
    performance.rs        Encryption performance tracking
    schema.rs             Encrypted schema types
    dashboard.rs          Encryption observability
    query_builder.rs      Encrypted query construction
    transaction.rs        Transactional encryption

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 8 — fraiseql-observers  (reactive business logic)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  traits.rs               Observer trait + ObserverEvent
  executor.rs             ObserverExecutor — dispatch chain
  factory.rs              Observer factory + registry
  event.rs                Structured event types
  condition.rs            Conditional observer triggering
  matcher.rs              Event pattern matching
  cached_executor.rs      Caching observer wrapper
  deduped_executor.rs     Deduplication wrapper
  queued_executor.rs      Queue-backed async observer
  storage.rs              Observer state persistence

  actions.rs / actions_additional.rs   ActionDispatcher trait + builtins
  arrow_bridge.rs         Arrow Flight event bridge
  elasticsearch_sink.rs   Elasticsearch event sink
  testing.rs              MockActionDispatcher + test helpers

  Sub-systems:
    cache/                Observer result cache
    checkpoint/           Checkpointing for at-least-once delivery
    concurrent/           Concurrent observer execution
    config/               Observer configuration
    dedup/                Event deduplication (Redis-backed)
    job_queue/            Persistent job queue
    listener/             Event listener abstraction
    logging/              Observer audit logging
    metrics/              Observer performance metrics
    queue/                Queue implementations
    resilience/           Retry + backoff
    search/               Search event integration
    transport/            NATS transport [feature:nats]
    tracing/              Observer span tracking

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 9 — fraiseql-wire  (streaming JSON query engine)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  json/                   Streaming JSON encoder (zero-copy)
    encode.rs             NDJSON / streaming encoder
    decode.rs             Streaming decoder
    validate.rs           Schema-driven validation
    message.rs            Wire message framing

  auth/                   Wire-level auth
  client/                 Wire client
  connection/             Connection pool + lifecycle
  operators/              Wire operator set
  protocol/               Wire protocol framing
    constants.rs          Protocol constants
  stream/                 Async stream adapters
  util/                   Wire utilities
  metrics/                Wire performance metrics
  error.rs                Wire error types
  lib.rs                  WireDatabaseAdapter + pool

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 10 — fraiseql-arrow  (Arrow Flight high-throughput delivery)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  flight_server/          Arrow Flight gRPC server (tonic)
  convert.rs              RecordBatch ↔ fraiseql types
  db.rs / db_convert.rs   DB result → Arrow
  export.rs               Batch export pipeline
  schema.rs / schema_gen  Arrow schema from CompiledSchema
  ticket.rs               Flight ticket encoding
  metadata.rs             Flight metadata
  subscription.rs         Arrow subscription streaming
  event_schema.rs         Event → Arrow schema
  event_storage.rs        Arrow-backed event store
  exchange_protocol.rs    DoPut / DoExchange handlers
  cache.rs                Arrow result cache
  clickhouse_sink.rs      ClickHouse batch sink
  error.rs                Arrow error types

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 11 — fraiseql-cli  (compiler & dev tooling)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  commands/
    compile.rs            schema.json → schema.compiled.json
    lint.rs               Schema linting
    analyze.rs            Schema complexity analysis
    cost.rs               Query cost estimation
    explain.rs            EXPLAIN ANALYZE wrapper
    generate/             Code generation (SDK, types)
    generate_views.rs     View SQL generation
    introspect_facts.rs   Fact table introspection
    validate.rs           Schema validation
    validate_facts.rs     Fact table validation
    validate_documents.rs Trusted document manifest validation
    migrate.rs            Schema migration
    sbom.rs               Software bill of materials
    dependency_graph.rs   Schema dependency visualization
    extract/              Schema extraction helpers
    federation/           Federation schema merging
    init/                 Project scaffolding
    serve.rs              Dev server (not default)
    run.rs                fraiseql run <compiled>

  schema/                 CLI schema loading
  config/                 CLI configuration (fraiseql.toml parsing)
  introspection.rs        DB introspection pipeline
  output/                 OutputFormatter (progress, section, table, JSON)
  output_schemas.rs       CLI output type schemas (schemars)

  MCP: enabled via FRAISEQL_MCP_STDIO=1 env var (stdio transport)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 12 — CROSS-CUTTING CONCERNS  (scan domains)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  DOMAIN              PRIMARY LOCATIONS                         SCAN FOCUS
  ──────────────────  ────────────────────────────────────────  ──────────────────────────
  SQL Safety          db/path_escape, compiler/aggregation,     injection, escaping,
                      db/where_clause, db/identifier            quoting, dialect coverage

  Auth & AuthZ        fraiseql-auth/, security/, middleware/    PKCE, constant-time,
                      auth.rs, oidc_*, operation_rbac           RBAC, token lifecycle

  Secrets             fraiseql-secrets/encryption/,             rotation, key scope,
                      secrets_manager/backends/                 audit, Vault renewal

  Cache Correctness   cache/result, cache/adapter,              invalidation logic,
                      cache/cascade_*, cache/fact_table_*       RLS isolation, TTL

  Rate Limiting       middleware/rate_limit/, auth/rate_limit,  burst, Redis sync,
                      server/config/rate_limiting               per-tenant isolation

  Federation          server/federation/, core/federation/      circuit breaker,
                      server/routes/graphql/handler             entity resolution

  Subscriptions       server/subscriptions/, runtime/subscription/  auth expiry,
                      routes/subscriptions.rs                   backpressure, cleanup

  Observability       tracing_utils, middleware/trace,          span propagation,
                      metrics_server, config/tracing            OTLP export, sampling

  Error Handling      fraiseql-error, core/security/error_fmt,  sanitization,
                      auth/error_sanitizer                      no-leak policy

  TLS / Transport     server/tls.rs, config/ (tls paths),       cert reload, cipher
                      fraiseql-wire/connection                  suites, mTLS paths

  Observer Safety     observers/executor, observers/actions,    Send+Sync, panic→error,
                      observers/resilience                      at-least-once delivery

  Arrow Flight        fraiseql-arrow/, server/arrow/,           semaphore (50 streams),
                      observers/arrow_bridge                    schema drift, backpressure

  Schema Compilation  compiler/, schema/compiled/,              IR correctness,
                      fraiseql-cli/commands/compile             SQL template safety

  Multi-tenancy       tenancy/, db/adapters (RLS),              tenant isolation,
                      server/middleware/tenant                  cross-contamination

  Testing Coverage    tests/, benches/,                         integration vs unit
                      fraiseql-test-utils/                      ratio, testcontainers

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 13 — FEATURE FLAGS  (Cargo features)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  fraiseql-server:
    default = [auth, secrets, webhooks]
    auth              fraiseql-auth (JWT/OAuth/OIDC)
    secrets           fraiseql-secrets (Vault, encryption)
    webhooks          fraiseql-webhooks
    observers         fraiseql-observers
    observers-nats    observers + NATS transport
    observers-enterprise observers + NATS + enterprise features
    arrow             fraiseql-arrow + tonic (Arrow Flight)
    aws-s3            AWS SDK S3 + config
    mcp               rmcp (MCP stdio server)
    metrics           prometheus metrics exporter
    redis-apq         Redis APQ cache
    redis-pkce        Redis PKCE state store
    redis-rate-limiting Redis token bucket
    tracing-opentelemetry OTLP export
    wire-backend      fraiseql-wire streaming adapter
    testing           Test helpers exposed publicly

  fraiseql-db:
    postgres(default) | mysql | sqlite | sqlserver
    wire-backend | rich-filters
    test-postgres | test-mysql | test-sqlserver

  fraiseql-error:
    axum-compat       IntoResponse impl (avoids hard axum dep in non-server code)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 LAYER 14 — TESTING INFRASTRUCTURE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Unit tests          #[cfg(test)] mod tests in each src file
  Integration tests   crates/*/tests/ directories
  Benchmarks          crates/fraiseql-server/benches/ (criterion)
  Snapshot tests      cargo test --test sql_snapshots (34 tests)

  CI jobs (docker-compose.test.yml):
    redis | nats | observers | tls | vault | server | feature-flags

  testcontainers:     fraiseql-core (watchdog enabled)
                      fraiseql-wire (watchdog enabled)

  fraiseql-test-utils:  Shared test helpers, mock adapters
  observers/testing.rs: MockActionDispatcher
  server/testing/:      Test app builder, health_router, make_test_state

  Key commands:
    cargo check --workspace
    cargo clippy --workspace --all-targets -- -D warnings
    cargo nextest run -p fraiseql-core
    cargo test -p fraiseql-server --lib                 # 572 tests
    cargo test --test sql_snapshots                     # 34 snapshots
    REDIS_URL=... cargo test ... -- redis_rate_limiter --ignored
```
