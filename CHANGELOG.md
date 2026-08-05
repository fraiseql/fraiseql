# Changelog

All notable changes to FraiseQL are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **NUMERIC values beyond 28 significant digits, `NaN` and `Infinity` no longer
  decode to null (#980).** The PostgreSQL adapter's row decoder read
  `NUMERIC`/`DECIMAL` columns through `rust_decimal::Decimal`, whose 96-bit
  mantissa caps at 28-29 significant digits and which has no `NaN` or
  `Infinity` — anything it could not represent failed the type probe and fell
  through the decode ladder to `Null`, silently. NUMERIC columns are now decoded
  from PostgreSQL's binary wire format directly to the exact text the server
  itself prints, at full precision; `NaN`, `Infinity` and `-Infinity` render as
  those JSON strings. The decoder is verified by a differential test comparing
  its output against `SELECT value::text` from a live PostgreSQL over a
  1,500-value corpus.

  This removes `rust_decimal` from the workspace — it had exactly one use site —
  and with it `rkyv` and `borsh` leave `Cargo.lock` entirely (rust_decimal was
  their sole dependent), retiring the RUSTSEC-2026-0235 advisory exception from
  `deny.toml` and `.cargo/audit.toml`.

- **A 27-byte GraphQL query no longer panics the parser (#976).**
  `graphql-parser` computes a block string's common indent in *bytes* but strips
  it with the Unicode-aware `str::trim_start`, then slices the line at that byte
  offset — so a block string indented with U+00A0 sliced mid-codepoint and
  panicked. The document is well-formed by the GraphQL spec, the input is
  unauthenticated, and with no `CatchPanicLayer` in the stack the panic unwound
  the connection task: the client got a dropped connection rather than an error,
  and no error metric moved.

  `parse_graphql_document` is now the single parse seam, and it rejects
  indentation the parser cannot handle before the parser sees it. Six call sites
  went through the raw parser when this was found — including
  `graphql/parser.rs`, which was invisible to the obvious grep because it
  imported the module and called `query::parse_query`. All six are routed, the
  seam additionally wraps the parser in `catch_unwind` so an unknown parser
  panic costs one query rather than the connection, and
  `tools/check-graphql-parse-sites.sh` (wired into `make preflight`) fails the
  build if a seventh appears. Found by the scheduled fuzz campaign, which had
  been reporting it weekly since 2026-06-21.

  Upgrading does not fix this: `graphql-parser` 0.4.1 is the newest release and
  the maintained `graphql-parser-hive-fork` carries the identical bug.

  **Behaviour change:** a query whose *indentation* uses non-ASCII whitespace is
  now rejected with a message naming the reason. Non-ASCII whitespace in string
  content — including block-string content — is unaffected, which is the only
  place the GraphQL spec allows it to carry meaning.

### Changed

- **The weekly fuzz campaign reports what it finds (#441).** A crash now opens
  (or comments on) an issue labelled `fuzz-crash` instead of only reddening a
  scheduled job — seven consecutive weekly failures on a real security defect
  went unread because a red scheduled job is not a signal anyone receives. Build
  failures and crash finds are now separate steps, so a bad nightly cannot
  masquerade as a finding, and the nightly toolchain is pinned rather than
  floating (an internal compiler error on 2026-07-26 failed two targets in
  exactly that way). Seed corpora carry the reproducers for fixed crashes, so a
  regression is caught by a fixture in git rather than by a 90-day cache
  surviving. The campaign remains schedule- and dispatch-only and cannot gate a
  merge.

  All 25 fuzz targets across the 8 crates were build-verified as part of this,
  which is how two of them turned out to be broken: `fraiseql-db`'s
  `where_from_json` and `where_generator` still referenced `MySqlDialect`,
  `SqliteDialect` and `SqlServerDialect`, removed by the PostgreSQL-only
  de-scope (#374), and `WhereClause::from_graphql_json` had gained a second
  argument. `where_from_json` is in the scheduled matrix, so this would have
  reddened the campaign the moment the de-scope merged — no CI leg builds
  `fuzz/`, because each is a separate cargo workspace. Both are fixed and now
  exercise the typed-field path as well as the untyped one; `where_generator`
  additionally asserts the emitted SQL keeps its quotes and parentheses
  balanced. `docs/fuzzing.md` carries a one-command build-verify loop.

- **Five properties from real defects are now checked continuously (#441).** Each
  is derived from a defect this remediation program actually fixed, and each was
  verified by pointing it at the pre-fix code and watching it find the original
  bug — a target that cannot do that asserts nothing while reporting green.

  | Property | Form | Defect |
  |---|---|---|
  | An inline argument never silently vanishes across the `value_json` write→read round trip | `value_json_seam` fuzz target | #719 |
  | No accepted identifier can alter the structure of the SQL it lands in | `identifier_validation` fuzz target | #794, #795, #833 |
  | Generated WHERE SQL keeps quotes and parentheses balanced | `where_generator` fuzz target | #833 |
  | A query parameter never bleeds into the host, user or database name | `fraiseql-wire` proptest | #817 |
  | No caller-supplied argument value can add a root field to a built MCP document | `fraiseql-server` proptest | #808 |

  The last two are proptests rather than fuzz targets because the code they guard
  is behind a private module, and widening it to `pub` purely for test reach would
  enlarge the supported API surface. They run in every CI test leg rather than
  weekly, so for those two the in-crate form is the stronger check.

  `pre-commit` no longer rewrites `fuzz/seed_corpus/`: `end-of-file-fixer`
  appended newlines to five #976 reproducers, and a seed corpus is test data
  where every byte is part of the input.

### Added

- **Outbound CDC is mounted by the server (#382).** `[cdc_outbound]` with one
  or more `[[cdc_outbound.sinks]]` now makes the server drain
  `core.tb_entity_change_log` to a broker on its own task set, behind the new
  `cdc-outbound` feature. The drain engine — durable per-sink delivery state,
  anti-join enqueue with a commit-lag sweep, claim-then-publish under a lease
  with head-of-line ordering, backoff and dead-lettering — shipped in v2.12.0
  and is used unchanged; what was missing is that **nothing in the shipped
  server ever constructed a `DrainWorker`**, so outbound CDC was reachable only
  by writing your own binary.

  Boot is fail-loud: a configured section with no database pool, an unreachable
  broker, delivery-state DDL that will not apply, a duplicate sink name, or a
  `kind` that is unknown or not yet implemented (`kafka`, `kinesis`, `pulsar`)
  all refuse to start. A server that boots without its drain looks healthy
  while every downstream consumer silently starves. Docs:
  `docs/features/cdc-outbound.md`.

- **Per-bucket access policies (#371).** `[[storage.<name>.policies]]` attaches
  a list of permit rules that *replaces* the bucket's coarse `access` mode:
  `methods` (`read`/`write`/`overwrite`/`delete`/`list`) × `principal`
  (`owner`/`authenticated`/`anonymous`/`role:<name>`) × an optional
  `key_prefix`. This expresses the shapes key-prefix routing could not — "the
  audit group may read under `reports/`, but only the creator may delete".

  Three properties are structural rather than documented. **Denial is the
  fallthrough**: there is no `effect = "deny"` whose precedence could be wrong,
  and `permits` returns true only from inside a matched rule, so an empty
  policy denies everything including to an object's own owner. **`write` is
  create-only** — replacing an existing object needs an explicit `overwrite`
  grant, because the natural rule "authenticated callers may write" would
  otherwise re-open the H9/B4 overwrite IDOR through the policy door (this was
  caught by the end-to-end test, not by review). **An unparseable policy
  refuses to boot** — an unknown method or principal, an empty `methods` list,
  or a misspelled field is a startup error, never a rule that silently denies.
  `list` also becomes a distinct permission (no longer implied by write
  access), with row filtering still applied on top.

- **Image renders are served, and hostile images are bounded (#370, closing
  #901).** `GET /storage/v1/render/{bucket}/{*key}?w=&h=&format=&quality=&preset=`
  mounts behind the server's new `storage-transforms` feature and reads through
  exactly the gates the download route uses (metadata, `can_read`, the
  missing/not-yours collapse). `format` is `webp`/`jpeg`/`png`/`avif`; with none
  given, the client's `Accept` header picks the encoding. Named presets now come
  from configuration — `[storage.<name>] transform_presets = [{ name = "thumb",
  width = 200, format = "webp" }]` — which previously could not be set at all
  (`BucketConfig::transform_presets` was hard-coded `None`); declaring them in a
  binary built without the feature is a **startup error**, not a silently absent
  endpoint. Before this, the whole `transforms` feature had no HTTP surface and
  `ImageTransformer` had no non-test caller.

  The transformer itself was unbounded: it decoded whatever a caller supplied
  and resized to whatever was requested, so a decompression bomb (a small file
  whose header declares an enormous image) or an absurd `?w=` allocated
  hundreds of megabytes per request. Source and requested dimensions are now
  capped at 12 000 px per side, checked from the header *before* decoding, with
  matching hard decoder limits behind them; bombs, malformed bytes, non-image
  objects and oversized requests all return a named `400`.

- **Resumable uploads — Tus 1.0.0 core + S3 multipart (#369).** New endpoints
  `POST /storage/v1/uploads/{bucket}/{*key}` (create, `Upload-Length` +
  optional `Upload-Metadata` filetype), `PATCH`/`HEAD`/`DELETE
  /storage/v1/uploads/{id}` (append at the proven offset / resume probe /
  cancel). Interrupted uploads resume from the durable offset; sessions are
  rows in the new `_fraiseql_storage_uploads` table, so they survive a server
  restart. Every path funnels through the SAME machinery as single-shot
  uploads: creation passes the H9/B4 overwrite gate and reserves the metadata
  row exactly like a presigned upload (#866), completion is one routine that
  finalises backend staging and confirms that row, and a foreign session is
  indistinguishable from a missing one (`404`; anonymous `401`; #876 — an
  interrupted upload cannot be resumed, probed, or cancelled by a different
  owner). Backends: local (staging under the reserved — and now
  `validate_key`-fenced — `.fraiseql-uploads/` namespace, rename on
  completion) and S3/MinIO (real multipart: chunks become parts, sub-5-MiB
  non-final chunks are refused up front as `400`); GCS/Azure refuse loudly
  (`NotImplemented`). Concurrency: one in-flight session per key (`409`),
  appends pinned to the proven offset (`409` on races), size caps enforced at
  creation and cumulatively, expired sessions answer `410` and are reaped
  (staging discarded, created reservations released; per-bucket
  `upload_ttl_secs`, default 24 h). Verified end to end over real MinIO + a
  real metadata table in the `server-storage` leg.

- **Durable long-running operations (#391).** New `[async_operations]` section
  mounts `POST/GET/DELETE /operations/v1/…` — submit returns an `op_id`
  immediately, background workers execute the stored GraphQL document through
  the SAME `execute_with_security` pipeline as `/graphql` (RLS, cost gates,
  change-log outbox — never a second execution path), and status reads the
  stored row. Designed against P19's six saga-recovery failure modes, each
  pinned in `async_operations_e2e_pg`: terminal states are never reclaimable;
  claiming is staleness-gated (workers heartbeat, so a live execution is never
  stolen); completions are claim-token-guarded (a superseded worker's late
  result cannot clobber the retry's); `Idempotency-Key` submission replays the
  same `op_id`; the persisted tenant key dispatches execution through the
  shared tenant seam; and a cancel that did not cancel is never reported as
  one (queued → cancelled outright, running → explicit `cancel_requested`).
  The operation allowlist is required and fail-closed, cost is charged at
  submission, status/cancel are submitter-scoped (404, no existence oracle),
  an errored GraphQL envelope records as `failed`, and an expired security
  snapshot refuses to execute. A configured section without a database pool or
  a creatable `_system.async_operations` refuses to boot. Docs:
  `docs/features/async-operations.md`.

- **MCP as a first-class transport (#376).** Three gaps closed on the existing
  (P09-hardened) MCP surface. **Auth parity**: MCP now accepts the same two
  Bearer modes as `/graphql` — OIDC (`[auth]`) *or* local HS256
  (`[auth_hs256]`); previously only OIDC validated, so an HS256 deployment
  could never authenticate an MCP call and `require_auth = true` refused to
  mount the endpoint (`FraiseQLMcpService::with_oidc_validator` is replaced by
  `with_token_validator(McpTokenValidator)`). **Behaviour hints**: every
  advertised tool carries MCP `ToolAnnotations` — queries `readOnlyHint: true`,
  mutations explicitly `destructiveHint: true` / non-idempotent, so agent
  clients confirm before invoking writes. **Audit tagging**: an MCP-originated
  mutation's change-log row is stamped `extra_metadata.transport = "mcp"`
  (forge-safe: the tag rides a framework-reserved security-context attribute
  set by the transport itself), making agent writes one query to find. New
  `mcp_transport_stamp_e2e_pg` suite drives an HS256-authenticated tool call
  through the real executor into the outbox. Resources / Prompts / session
  continuity are tracked in #967. Docs: `docs/mcp.md` gained Authentication,
  Behaviour hints, and Audit trail sections.

- **Session-state subsystem (#389).** New `[session_state]` section: durable
  per-thread conversation memory for agents and multi-turn applications —
  key/value entries scoped to `(session, thread)` with per-entry TTL (expired
  entries are invisible to reads immediately and reclaimed by a background
  sweep), a 64 KiB per-value cap, and an optional `Summarizer` hook that
  atomically collapses a thread into a single reserved `_summary` entry past a
  configurable threshold (a failing summarizer leaves the thread intact).
  Backends: `memory` (volatile, dev — warns at boot) and `postgres`
  (`_system.session_state`, created at boot like `_system.sessions`). A
  configured `postgres` backend without a pool, or whose table cannot be
  initialised, **refuses to boot** — never a silent in-memory downgrade. The
  section is strict (`deny_unknown_fields`). Library API:
  `fraiseql_auth::session_state` + `Server::session_state()`; MCP session
  continuity binds to it in #376. Docs: `docs/features/session-state.md`.

- **Actor-model hardening (#390).** The change-log's `actor_type` domain is now
  enforced by the database itself: migration 08 installs
  `chk_entity_change_log_actor_type` (`NOT VALID`, so a populated legacy table
  migrates safely; new writes are checked), and a CLI lockstep test pins the
  constraint's token list to `ActorType::ALL` so adding an enum variant without
  extending the constraint is a red test. `fraiseql doctor --against-db` gained
  an actor-attribution check: out-of-contract `actor_type` values are a
  **Fail** (rogue writer), a missing constraint or `NULL`-actor rows are a
  **Warn**. A new end-to-end suite (`actor_attribution_e2e_pg`) drives real
  HS256 tokens through the production mount on both HTTP write transports
  (`/graphql` + REST) and asserts the recorded rows: `human_user` /
  `service_account` (scope) / `ai_agent` + `acting_for` (RFC 8693 `act`)
  derivation, that forged `fraiseql.*`/`actor_type` claims cannot influence the
  classification, and that unauthenticated writes are refused rather than
  recorded unattributed. Operator docs: `docs/features/audit-logging.md`.
  Deferred consumption features (RBAC actor predicates, per-actor budgets) are
  tracked in #966.

### Fixed

- **CRITICAL — the second Deno invocation in a process crashed the server
  (#969).** The V8 platform was never explicitly initialized, so the first
  guest invocation lazily installed deno_core's PKU-protected default platform
  on its own short-lived thread. Memory-protection-key rights over V8's JIT
  pages are inherited only by that thread's descendants, so every later
  invocation thread faulted on the first JIT page it touched — SIGSEGV, taking
  the whole process down. Invisible until now: #796 meant scheduled functions
  never fired twice per process, nextest's process-per-test model never
  creates a second isolate, and the resulting `cargo test` crash had been
  misattributed to the Dagger exec sandbox. The runtime now initializes the
  **unprotected** default platform once, up front (rusty_v8's documented
  embedding for hosts that cannot guarantee thread ancestry — standard W^X
  still applies). The full deno suite now passes in one shared process,
  sequential and parallel, for the first time.

- **CRITICAL — guest host-ops poisoned shared connection pools (#970).** Host
  ops (`fraiseql_query`, cursor get/advance, HTTP, storage, email) executed
  their futures on the invocation's throwaway Tokio runtime. Any connection
  *created* during an op — a fresh sqlx pool connection, a reqwest keep-alive
  socket — registered with that runtime's I/O reactor, then outlived it inside
  the shared pool; the next user awaited a wakeup from a dead reactor and hung
  until timeout. Observed as every second scheduled source firing failing with
  "event loop exceeded time limit" and as `pool timed out while waiting for an
  open connection` on the server's main pool. All host I/O is now pinned to
  the owner runtime via `RuntimePinnedHost`: `run_guest` captures the
  dispatching runtime's handle and spawns every op there, so sockets always
  live on the reactor that owns the pools. A host dispatched outside any Tokio
  runtime is refused loudly.

- **Three never-run observer test binaries wired into CI (#390 flight
  finding).** `entity_change_log_contract`, `changelog_views`, and
  `capture_trigger` are `#[ignore]`d suites that no CI leg ran with
  `--ignored` since they were written. All three now run in the observers
  integration leg. Wiring them surfaced an order-dependence defect:
  `capture_trigger` inherited whatever `core.tb_entity_change_log` an earlier
  suite left behind, and on a table with `object_data NOT NULL` (three in-tree
  fixtures create that shape) the capture trigger's legitimate NULL after-image
  on DELETE violated the constraint. The suite now owns its slate.

- **Studio: Schema, Observers, and Logs tabs (#373).** The embedded Studio SPA
  gained a **Schema** browser (types with per-field type/nullability and the
  backing view, queries with their relay/list shape, mutations with their
  backing function — from `/admin/v1/schema`), an **Observers** tab (delivery
  health, DLQ viewer with per-item retry and retry-all — the dispatch-at-most-
  once semantics of retry are pinned server-side), and a **Logs** tab
  (observer dispatch history). Both observer-backed tabs degrade gracefully
  when the `observers` feature is off. A new test pins every shell tab to a
  renderer wired to its real admin endpoint via `include_str!` on the SPA
  source. The raw-SQL editor tab from the original issue is deliberately NOT
  shipped: it requires a new arbitrary-SQL admin endpoint, which is a gated
  decision tracked in #962.

- **Python SDK AI-framework integrations (#388).** `fraiseql.integrations`
  gained `openai` (OpenAI tool/function definitions + a call dispatcher that
  refuses hallucinated names before any server round-trip), `mcp` (raw MCP
  tool descriptors + an in-process `tools/call` dispatcher, no MCP SDK
  dependency), and `rag` (`as_source` wraps any list query — e.g. over a
  `v_embedded_documents` view — as a framework-agnostic retrieval source).
  All adapters, including the existing LangChain/LlamaIndex ones, now share
  one normalised operation model built from introspection, so tool names,
  argument types, and generated documents are identical across frameworks —
  and LangChain tool documents gained true argument types (a `$limit: Int`
  argument was previously declared blanket `String`). `include`/`exclude`
  exposure controls are enforced both at advertisement and at the dispatch
  boundary. Documented in the SDK's `docs/ai-integrations.md`.

- **Typed Python clients + compile-the-output CI gates (#372).**
  `fraiseql generate-client python` emits a fully typed, standard-library-only
  Python (≥ 3.12) client from `schema.compiled.json`: `TypedDict`s for every
  object/interface/input type, `Literal` enum aliases, PEP 695 union aliases,
  relay `Connection[T]`, per-operation functions embedding their GraphQL
  documents, an `is_error_result` runtime guard, and a `urllib`-based
  `FraiseqlClient` — every file stamped with the canonical schema hash. The
  document/selection builders moved into a language-independent core shared
  with the TypeScript generator, pinned by a cross-language test asserting the
  generated GraphQL documents are byte-identical. And the acceptance direction
  that had never executed now runs in CI: the `generated-clients` job in
  `sdk-conformance.yml` compiles the canonical conformance fixture, generates
  both clients, and type-checks them (`tsc --strict`, `ty check`) plus
  consumer-usage projects exercising every operation — including the
  previously `#[ignore]`d-everywhere `client_ts_consumer` gate. Go/Rust
  generators are tracked in #961.

- **The schema↔database drift linter can fail, and covers dropped inputs
  (#384).** `compile --database` findings now carry a severity: a `sql_source`
  that resolves to no relation or function, a missing/mis-typed JSONB or relay
  cursor column, a type-inconvertible native-column argument, or a **required**
  field whose JSONB key is absent from every sampled row fail the compile with
  a non-zero exit and no artifact written; nullable-field key absences and
  performance advisories stay warnings. `--allow-drift` (requires `--database`)
  restores the advisory behaviour, loudly. The mutation→function contract
  check gained the silently-dropped-input scan (#384 category 2): a declared
  input field on the single-JSONB payload path whose key the `plpgsql`/`sql`
  function body never references — while the body demonstrably extracts other
  keys — is reported (warn-grade; whole-payload consumers such as
  `jsonb_populate_record` are recognised). `doctor --against-db` now also runs
  the same L1/L2/L3 view-composition linter and reports each finding as a
  structured check, so `--json` gives CI and editors a machine-readable drift
  report. Fixing the L3 pass exposed that it had never sampled a row: the
  Postgres introspector inherited a trait default that fabricated an empty
  sample set, so the JSONB-key check silently passed on every schema since it
  shipped. `get_sample_json_rows` is now a required trait method with a real
  (schema-qualification-aware) PostgreSQL implementation, and the executed
  `compile_drift_fail_pg` suite proves every direction — including that the
  linter fails — against a live database.

- **pgvector similarity search is real (#386).** The vector type vocabulary
  (`FieldType::Vector`, `VectorConfig`) existed with no producer and no
  executable query path; both now exist end to end. Authoring: TOML
  (`vector = { dimensions = N, index_type = "hnsw", distance_metric = "cosine" }`)
  and the authoring IR (`vector_config`) carry the config into the compiled
  schema — required on `Vector` fields, refused on any other type. Query DSL:
  `docs(nearest: {vector: $q, k: 10, metric: "l2"})` lowers to the
  index-eligible `ORDER BY "embedding" <op> '[…]'::vector LIMIT k` against a
  native `vector(N)` view column, with request-time dimension validation, the
  field's declared metric as default, and full composition with `where`/RLS;
  the four float-vector WHERE operators (`cosine_distance`, `l2_distance`,
  `l1_distance`, `inner_product`) become executable threshold predicates
  (`{vector, threshold}` operand). `--emit-ddl` emits dimensioned `vector(N)`
  columns (a bare `vector` column cannot be indexed), the declared HNSW/IVFFlat
  index, and `CREATE EXTENSION IF NOT EXISTS vector`. The test rigs run
  `pgvector/pgvector:pg16` (local compose and the CI mirror), and the first
  executed vector suite asserts row identity and order per metric — every prior
  vector test was a `sql.contains("<=>")` string assertion.

- **GraphQL over SSE with root-field `@stream` (#387).** Opt-in
  (`enable_graphql_sse`, default off): a GraphQL request carrying
  `Accept: text/event-stream` is answered as Server-Sent Events — any operation
  as one `next` result plus `complete`, and a query whose single root list field
  carries `@stream(initialCount: N)` incrementally: an initial payload with `N`
  rows, then `graphql_sse_stream_batch_size`-row batches
  (`{"incremental":[{"items":…,"path":…}],"hasNext":…}`), each **re-executed
  through the full pipeline** — depth/complexity gates, authorization, RLS
  session variables, result cache — by re-issuing the document with paginated
  variables, so there is no second execution path to drift. The SSE branch lives
  inside the authenticated `/graphql` route (401 before any stream opens);
  long-lived deliveries re-check principal expiry before every batch and
  terminate with an `UNAUTHENTICATED` event; deliveries survive
  `request_timeout_secs` and are exempt from response compression. Ineligible
  shapes are refused loudly before any event: non-list or relay queries, nested
  `@stream`, multi-root operations, mutations, and documents declaring
  `$limit`/`$offset` variables. Outside SSE, `@stream`/`@defer` are now *known*
  advisory no-ops (parse, include, no warning — even under strict directive
  mode), per the incremental-delivery proposal's server-may-ignore semantics.
  See `docs/operations/graphql-sse-streaming.md`.

- **Read replica support (#407).** `read_replica_urls` (plus optional
  `read_replica_pin_after_write_ms`, default 5000) route compiled GraphQL queries —
  and every other structurally read-only adapter path: field projections,
  aggregates, relay pagination, `EXPLAIN` — round-robin across PostgreSQL replicas,
  while mutations and every mixed-use surface (raw SQL, DDL, stats, health, auth
  stores, observers, CDC) stay on the primary. The partition is static: a surface
  that *can* write is never replica-routed. Consistency: every mutation arms a
  shared watermark and reads route to the primary for the pin window afterwards,
  so replication lag cannot serve a client its own stale write (proven by an
  integration test whose stand-in replica never receives the write). Safety rails:
  each replica pool is built from the same configuration as the primary — same
  TLS, same sizing, and the same per-tenant `search_path` in the startup packet
  (#809 generalised to every pool) — an unreachable replica refuses boot, a
  replica that is not actually in recovery is loudly flagged, a runtime replica
  failure falls back to the primary, an inert pin-without-replicas config and a
  wire-backend build with replicas configured are both refused. See
  `docs/operations/read-replicas.md`.

- **`[[analytics.queries]]` is real (#624).** The `[analytics]` section — inert since
  its first commit and rejected since #612 — now lowers each entry at compile time
  into an ordinary compiled query: a list-returning, view-backed `QueryDefinition`
  whose `sql_source` goes through the standard compile-time SQL-identifier
  validation and whose SELECT list is the declared `return_type`'s fields, so no
  client-supplied identifier can reach `FROM` or the SELECT list (the P01
  constraint, satisfied structurally). `AnalyticsQuery` gains the required
  `return_type` field. Compile errors: unknown `return_type`, a name colliding with
  an existing query, a name ending in the executor-reserved `_aggregate`/`_window`
  suffixes (such a query would be unreachable), `enabled = false` with queries, and
  `enabled = true` without queries. A `[[caching.rules]]` entry can target an
  analytics query (analytics lowering runs first). Found and filed alongside: the
  SDK-side `aggregate_queries` seam section is carried and silently dropped (#956).

- **`[[caching.rules]]` is real (#623).** The `[caching]` section — rejected as inert
  since #612 — now lowers each rule at compile time onto the two compiled fields the
  result cache already consumes: the named query's `cache_ttl_seconds` (the per-view
  TTL map, opt-in) and each `invalidation_triggers` mutation's `invalidates_views`
  (mutation-driven eviction). `fraiseql.toml` can therefore author per-query caching
  and cross-entity invalidation edges without the SDK. Every silently-inert shape is
  a compile error: an unknown query or trigger mutation, a TTL already authored via
  the SDK (no last-write-wins between sources), `enabled = false` with rules,
  `enabled = true` without rules, and any `backend`/`redis_url` (there is no
  Redis-backed result cache; the `backend` default is now the honest `"memory"`).
  The server warns at boot when TTLs are declared but `cache_enabled` is off, and
  the multi-tenant cache refusal now names the real `cache_enabled` key instead of a
  nonexistent `[cache]` section. `docs/modules/cache.md` corrected: TTL 0 means
  mutation-invalidated-only (not "never cache"), expiry is moka-managed (no
  read-time check), and there is no `POST /cache/invalidate` endpoint.

- **Operation cost budgets are now a full surface (#379).** What already shipped in
  June (the request-time cost estimator, `[fraiseql.cost_weights]`, the per-tenant
  per-request `cost_budget`, `[security] persisted_queries_only`) gains the missing
  half:
  - `[security.cost_budget] per_request_max` — a schema-wide per-operation cost
    ceiling enforced **inside the executor**, so it binds on every transport that
    executes a GraphQL document (`/graphql` POST/GET/QUERY, MCP, the functions
    bridge, direct embedders), not only the HTTP handler.
  - Declared `[validation]` depth/complexity limits are likewise derived into the
    executor's GATE-1 at construction (embedder-installed validators still win, and
    the server merges its runtime `[validation]` override per field), closing the
    gap where the functions bridge — and any future transport — executed documents
    no declared bound applied to. GATE-1 is now variables-aware: `first: $n` is
    scored at its resolved value instead of the fail-closed ceiling (#869).
  - Per-tenant **rolling per-minute budgets**: `cost_budget_per_minute` on the
    tenant-quota admin API, with `[security.cost_budget]
    per_tenant_per_minute_default` seeding tenants that set none.
  - Distinct error codes: a per-request rejection is `OPERATION_COST_EXCEEDED`
    (200 + `errors[]`; retrying cannot succeed), an exhausted window is
    `COST_BUDGET_EXHAUSTED` (429 + `Retry-After`). Both were previously
    indistinguishable from `RATE_LIMIT_EXCEEDED` with a misleading 1-second retry
    hint.
  - Cost observability: every `/graphql` request logs `cost`, `tenant`, and
    `operation` on the `fraiseql::cost_audit` tracing target and feeds
    `fraiseql_graphql_queries_cost_total` at `/metrics`, so budgets can be sized
    from observed traffic before enforcement.
  - `fraiseql validate-documents --max-cost N [--schema schema.compiled.json]`
    scores each persisted document (worst-case: unresolvable pagination variables
    cost the ceiling) and fails validation for documents over the cap.
  - `[security] persisted_queries_only` is now pinned per HTTP method: ad-hoc
    documents are refused on POST, GET, and QUERY alike.

### Breaking

- **`compile --database` fails on error-severity drift (#384).** Previously
  every schema↔database drift finding was advisory (`warn!` + exit 0, artifact
  written). A schema whose declarations name database objects that do not
  exist — or cannot serve the declared shape — no longer compiles; pass
  `--allow-drift` for the old behaviour. `DatabaseIntrospector::
  get_sample_json_rows` lost its silently-empty default implementation and is
  now required.

- **The vector WHERE operand shape changed (#386).** `cosine_distance: [0.1, …]`
  (a bare array) generated SQL PostgreSQL always refused — a non-boolean
  float8 expression over a mis-parenthesised cast with a jsonb-bound operand —
  so no working query used it. The operand is now
  `{vector: [Float!], threshold: Float}` with distance-≤ (or, for
  `inner_product`, raw-inner-product-≥) semantics. `hamming_distance` and
  `jaccard_distance` are refused loudly: pgvector defines them over binary
  (`bit`) vectors, which the float `Vector` type cannot declare. The
  `SqlDialect::vector_distance_sql`/`jaccard_distance_sql` trait methods
  (unreachable outside that broken path) are removed.

- **`PoolPrewarmConfig` gains the mandatory `read_replicas` field (#407).** Every
  pool construction site must now state its replica topology (`None` for a
  single-primary pool), the same compile-time-visible decision the `tls` field
  imposes: replica pools are built from the very same config, so tenant isolation
  and transport security cannot silently differ between the primary and a replica.

- **Cost rejections changed shape (#379).** A per-tenant `cost_budget` rejection was
  HTTP 429 `RATE_LIMIT_EXCEEDED` with `retry_after_secs: 1`; it is now
  `OPERATION_COST_EXCEEDED` in a 200 GraphQL error response, because retrying an
  over-budget operation can never succeed. `FraiseQLError` gains the `CostExceeded`
  variant carrying `cost`, `limit`, and an optional retry hint.

- **`RuntimeConfig.max_query_depth` and `max_query_complexity` are deleted (#379).**
  Both were declared, defaulted, debug-printed — and read by nothing. The one
  enforcement surface is `query_validation` (embedder-installed, or derived from the
  compiled `[validation]` limits at executor construction). An embedder that set the
  dead fields and expected enforcement never had it; set `query_validation` instead.

- **The compiled `auth` object is nested (#368, #367).** `CompiledSchema.auth` was
  the flat PKCE quadruple; it is now a container with `pkce`, `social` and `local`
  groups, so the `[auth]` block can carry the social-provider registry and the
  first-party auth methods alongside the PKCE client. A schema compiled before this
  change carries the flat shape and no longer deserializes — recompile it. (There are
  no compiled schemas in the wild; the field shipped in #621.)

- **`fraiseql_auth::social` is deleted (#368).** `SocialLoginState`,
  `SocialProviderRegistry` and `social_authorize` were a second, thinner social
  surface: a redirect-only `GET /auth/v1/authorize` with no callback, no account
  linking and therefore no trust gate. The mounted flow is `multi_provider`, which has
  all three. `Server::with_social_login` now takes
  `Arc<MultiProviderAuthState>`; library embedders on the old type should build the
  `multi_provider` state instead, or configure `[auth.social]` and let the server
  build it.

- **`GitHubOAuth::new` is synchronous and fallible (#368).** It was `async` because it
  performed OIDC discovery — against an endpoint GitHub does not serve. It now returns
  `Result<Self>` without any network call; `GitHubOAuth::with_endpoints` takes explicit
  base URLs for GitHub Enterprise Server.

- **`github` is trusted for email-verified account linking by default (#368).** With
  the `/user/emails` second hop implemented, `TrustedEmailProviders::builtin_default`
  is now `{google, apple, github}`. Deployments that want the previous posture should
  call `.distrust("github")`.

- **The rich-filter surface (`<RichType>WhereInput`) is gone (#869).** The compiler
  emitted 48 per-type WhereInput input types advertising 35 operator names
  (`domainEq`, `tldIn`, `withinRange`, …) that the runtime WHERE parser could never
  serve: 32 of them failed with `Unknown WHERE operator`, and two (`depthEq`,
  `overlaps`) silently bound to unrelated ltree/inet operators. The emission, the
  embedded `lookup_data` blob, the CLI SQL-template tables, and the runtime's
  unreachable `ExtendedOperator` machinery (`fraiseql_db::filters`,
  `WhereOperator::Extended`, `SqlDialect::generate_extended_sql`,
  `fraiseql_core::filters`) are all deleted. Rich scalar *names* remain valid
  authoring types; filtering uses the standard operator set. A compiler↔runtime
  contract test now refuses any compiled input type advertising an operator
  `WhereOperator::from_str` cannot parse.

- **The string-SQL tenancy helpers are gone (#736).**
  `fraiseql_core::tenancy::{where_clause, where_clause_postgresql,
  where_clause_parameterized}` (methods and free functions) interpolated or
  templated `tenant_id` SQL that no production path used, behind a doc claim
  ("validated at context creation") that was false — `TenantContext::new` validates
  nothing, and `where_clause()` panicked on IDs outside `[A-Za-z0-9._-]`.
  `TenantContext` now carries identity/metadata only; tenant filtering is done by
  the runtime security machinery (`inject_params`, `rls_policy`, per-tenant pools).

- **An RLS-protected deployment now fails closed on every anonymous query path
  (#784).** With a `RuntimeConfig::rls_policy` configured, the anonymous regular
  path served *unfiltered* rows (it never consulted the policy) and the REST
  direct-read and count paths fell through to unfiltered on a missing security
  context, while the relay and node paths refused. All five paths now refuse
  identically ("Query not found"), and `Prefer: count=exact` can no longer
  disagree with the body it describes.

- **`fraiseql run` refuses malformed `FRAISEQL_*` env values instead of silently
  flipping them to `false` (#874).** `ServerArgs::from_env` routed every boolean
  through a hand parser that mapped clap-valid `y`/`t`/`on` — and any typo, e.g.
  `FRAISEQL_SUBSCRIPTION_REQUIRE_AUTH=ture` or a trailing space — to an explicit
  `false` override, silently disabling the guard the operator was enabling. Both
  binaries now share clap's boolish parser; a set-but-unrecognised boolean or an
  unparseable numeric/address value is a startup error naming the variable.

- **Arrow Flight defaults to loopback (#874).** `flight_bind_addr` defaulted to
  the `0.0.0.0:50051` wildcard while the HTTP surface defaulted to loopback, and
  the `FRAISEQL_FLIGHT_BIND_ADDR` override lived in a serde default — so it lost
  to any config-file value, and a malformed value silently fell back to the
  wildcard. Default is now `127.0.0.1:50051`; the env var / `--flight-bind-addr`
  follow the standard CLI > env > file > default precedence and refuse startup on
  a malformed value.

- **`Server::new`/`from_executor` run `ServerConfig::validate()` (#874).** The
  documented library embedding (`ServerConfig::from_file` + `Server::new`) skipped
  every production safety gate — a leftover `playground_enabled = true`, a zero
  pool timeout, or `[auth]` + `[auth_hs256]` both configured booted happily as a
  library while the binary refused. Every construction path now faces the same
  gates; library embedders with configs the binary would reject will now be
  refused too.

- **`FRAISEQL_REQUIRE_REDIS` now verifies all three shared-auth-state subsystems
  (#874).** The gate inspected only the PKCE store, so the operator's "all shared
  state is distributed" assertion held while revoked tokens stayed accepted on
  other replicas and per-IP limits ran at N× the configured rate. It now refuses
  when the PKCE store, the rate limiter, or the token revocation store is
  per-process (a disabled subsystem is not a violation; Postgres-backed
  revocation counts as shared).

- **The non-kafka `KafkaAdapter` stub fails loud (#784).** The compiled-out stub
  reported `Ok` from `deliver()` (dropping every subscription event) and
  `health_check() == true`. It now errors on delivery and reports unhealthy,
  matching the other compiled-out runtime stubs.

- **The dead `ServerSubsystems` bundle was deleted (#874).**
  `ServerSubsystemsBuilder`, `validate_subsystems_config` and the
  `ServerSubsystems`/`StorageSubsystem` container had no production constructor —
  their "call once during server startup" advisories never reached an operator.
  The live pieces (`FunctionsSubsystem`, `BeforeMutationHooks`, the functions
  loader) are unchanged.

- **`ServerConfig` (the `fraiseql-server --config` file) now refuses unknown keys
  (#839).** The architecture docs shipped a production example whose keys sat in
  `[server]`/`[database]` grouping tables `ServerConfig` does not have; serde silently
  discarded every documented key, so the server booted on `127.0.0.1:8000` with default
  pool sizing while the operator believed they had configured `0.0.0.0:4000` and
  `pool_max_size = 20`. An unknown top-level key is now a parse error naming the key,
  and a section whose build feature is compiled out (e.g. `[observers]` without the
  `observers` feature) gets an error naming the missing feature instead of the former
  warn-and-drop. **Migration:** the config keys are top-level (`bind_addr`,
  `schema_path`, `database_url`, `pool_min_size`, …) — remove any grouping tables and
  any key the error message names.

- **The dead `fraiseql_server::config::RuntimeConfig` layer was deleted (#839).** The
  docs described the binary as "loading `RuntimeConfig` and translating it to
  `ServerConfig`"; in reality the type — with its own `[server]`/`[database]` shape,
  `url_env` indirection, loader and 433-line `ConfigValidator` — was constructed by
  nothing but its own tests and a fuzz target. Removed along with its sub-configs
  (`HttpServerConfig`, `DatabaseConfig`, `LifecycleConfig`, `CorsConfig`,
  `MetricsConfig`, `TracingConfig`, `RateLimitingConfig`, …), the `config::env`
  helpers, and the never-fed `AppState` config slot whose emptiness made
  `GET /api/v1/admin/config` always report `cache_enabled = false`; that endpoint now
  reports the real adapter-cache state and no longer promises port/host/workers fields
  it could never fill. `fraiseql_server::config` retains only the live types
  (`UsagePersistenceConfig`, `WebhookRouteConfig`, error sanitization, pool tuning).

### Added

- **The HTTP `QUERY` method (RFC 10008) on the GraphQL endpoint (#508).** Opt-in via
  `enable_http_query` (default `false`); `GET` and `POST` behaviour is unchanged either
  way. `QUERY` is "GET with a request body" — safe, idempotent and cacheable — so routing
  deterministic GraphQL reads over it stops telling caches, proxies and retry layers
  "unsafe, do not cache, do not retry". Acceptance is **queries-only**: a `mutation` or
  `subscription` is refused with `405`, because a method an intermediary may replay must
  never carry a state-changing operation. The gate parses with the same parser the
  executor uses, so it cannot disagree with what would actually run. CORS advertises
  `QUERY` only when the server accepts it, so the header never promises a route that
  answers 405. axum 0.8 has no `MethodFilter::QUERY` yet, so the method is mounted as a
  `MethodRouter` fallback — two clearly-marked places (`HTTP_QUERY_METHOD` and the
  fallback wiring) swap to the typed filter when upstream ships it.

- **Social login is reachable from the shipped binary (#368).** The account-linking
  trust gate and the provider modules were library-only: `Server::with_social_login`
  had zero callers, nothing auto-registered providers, and `[auth.social]` could not
  even be typed (`[auth]` is `deny_unknown_fields`). A compiled `[auth.social.google]`
  / `[auth.social.github]` block now builds the trust-gated `multi_provider` flow at
  boot and mounts `GET /auth/v1/{providers,authorize,callback}`, backed by
  Postgres-backed sessions and account linking. Configured-but-unusable shapes refuse
  to boot naming the offending key: no `[auth_hs256]`, an unset `client_secret_env`,
  an SSRF-blocked endpoint override, or no database pool. `/auth/v1/authorize` and
  `/auth/v1/callback` are governed by the same per-IP `auth_start` / `auth_callback`
  path buckets that guard `/auth/start` (#788) — both rate-limit backends now derive
  their rules from one shared builder so they cannot drift. Apple, Discord and
  Facebook are split out to #943 and #944.

- **The GitHub provider talks to GitHub (#368).** It wrapped `OidcProvider`, so
  construction performed OIDC discovery against `github.com` — which serves no
  discovery document (404), meaning it could never have constructed against real
  GitHub. It is now a plain OAuth2 client against the fixed well-known endpoints
  (overridable for GitHub Enterprise Server, SSRF-guarded), requesting
  `read:user user:email`, sending `Accept: application/json` at the token endpoint,
  and tolerating the absent `expires_in`. The `/user/emails` second hop resolves the
  **primary verified** address, so a private-email GitHub account can participate in
  email-keyed account linking; any failure of that hop falls back to
  `email_verified = false`. GitHub therefore joins `google` and `apple` in the default
  `TrustedEmailProviders` set — the documented reason for its exclusion was exactly
  this missing hop.

- **`[auth.local]` — first-party auth methods are reachable (#367).** Email+password,
  email OTP / magic link, TOTP MFA and anonymous sessions all existed in
  `fraiseql-auth` with no way to reach them: `with_mfa` / `with_anon_signup` had zero
  callers, the MFA/social/anon route groups were registered against fields hard-coded
  to `None`, OTP had no server route at all, and the password-reset flow had no
  concrete `ResetEmailSender` outside its own test double. A compiled `[auth.local]`
  block now mounts each enabled method — `/auth/v1/password/{signup,login,reset,
  reset/confirm}`, `/auth/v1/{otp,verify}`, `/auth/v1/mfa/*`, `/auth/v1/signup` — and
  a method that cannot work refuses to boot rather than dead-ending: no
  `[auth_hs256]`, no pool, a missing or send-less `email_from` mailbox, or a build
  without the `inbound-email` feature (which carries the SMTP transport) each name
  the offending key.

- **Postgres-backed MFA and OTP stores (#367).** `PgMfaStore` and `PgOtpStore` make
  `[auth.local] mfa`/`otp` safe to serve. The in-memory stores are per-process, which
  for MFA means a deploy silently destroys every user's second factor, and for OTP
  means N replicas multiply both the send budget and the 3-attempt verify cap by N —
  a six-digit code becomes brute-forceable. TOTP secrets are stored recoverable (they
  are shared secrets), recovery codes are bcrypt-hashed and deleted as consumed,
  challenge tokens and OTP codes are stored as SHA-256 hashes so a database read
  cannot replay a live one, and the per-user failure budget lives in the enrollment
  row so it survives a restart. Both budgets are charged in SQL, so a concurrent
  flood cannot lose a failure to a read-modify-write race.

- **A concrete `ResetEmailSender` / `EmailDelivery` (#367).** `MailboxEmailSender`
  relays OTP codes and reset links through the same `[mailbox.<name>.smtp]` transport
  the `send_email` host op uses, so a deployment configures outbound mail once.
  `reset_url_template` / `magic_link_template` are validated at compile time to
  contain their `{token}` / `{code}` placeholder — a template without one builds the
  same dead link for every user.

- **OTP identities are real accounts (#367).** `otp_verify` minted
  `user_id = "otp:<email>"` without touching the account store, so the same person's
  OTP, social and password sign-ins produced as many separate identities as sign-in
  methods. Completing the OTP flow proves control of the mailbox, so the identity now
  resolves through `AccountStore::link_or_create_user` with `email_verified = true`
  and converges with every other verified-email sign-in for that address.

- **`FRAISEQL_SHUTDOWN_TIMEOUT_SECS` / `--shutdown-timeout-secs` (#838).** The
  `shutdown_timeout_secs` config field's rustdoc had promised this override since it
  shipped; the variable now exists — the only occurrence of its name in the workspace
  used to be that comment.

- **Docs-truth CI gates (#838, #839).** `tools/check-docs-env-vars.sh` fails when any
  `FRAISEQL_*` variable named in `docs/`, `README.md` or an example README has no reader
  in the workspace; `tools/check-docs-version.sh` fails when a doc's "vX.Y.Z released"
  status line disagrees with `Cargo.toml`; and `doc_config_examples_test` deserializes
  every `# server.toml`-marked TOML block in the operator docs into the real
  `ServerConfig`. All three run in CI (shell gates + the test leg).

### Fixed

- **`max_query_complexity` is enforced for variable-valued pagination arguments
  (#869).** `first`/`limit`/`take`/`last` supplied as GraphQL variables — the shape
  every Relay/Apollo client uses — scored the neutral multiplier 1, so
  `users(first: $n) { orders(first: $n) { … } }` with `n = 100` scored 4 instead of
  ~10,000 and sailed past the DoS gate the literal form tripped. Variables are now
  resolved during validation and cost estimation; an unresolvable variable scores
  the clamp ceiling (fail closed). The per-tenant cost budget uses the same fix.

- **`read:*` scope wildcards stop at the delimiter (#784).** `RoleDefinition::has_scope`
  matched by bare string prefix, so a role granted `read:*` also passed
  `readwrite:…` checks; `read:User.*`-style grants now also require the `.`/`:`
  boundary the docs describe.

- **`FactTableVersionStrategy::TimeBased { ttl_seconds: 0 }` no longer panics
  (#784).** The version-key bucket divided by the TTL, so a zero TTL — constructible
  via the public `time_based(0)` API or serde config — crashed the process on the
  first cached aggregation query. Zero now means Disabled.

- **Valid subscriptions with `{` in variable defaults or directive arguments are
  accepted (#786).** The subscription-name scanner took the first `{` after the
  literal `subscription` as the selection-set brace; it now uses the real GraphQL
  parser, and multi-root subscription documents are rejected explicitly instead of
  silently serving only the first field.

- **graphql-transport-ws conformance around `connection_init` (#786).** Before the
  ack, an undecodable message closes `4400` and any non-init message closes `4401`
  (both were silently discarded, leaving clients to the generic init timeout);
  legacy `connection_terminate` performs a graceful close instead of being ignored
  with the connection and its subscriptions left alive; malformed subscribe
  payloads close `4400` instead of `1002`.

- **The documented `[rate_limiting]` example parses (#874).** `RateLimitConfig`
  gained the container-level `#[serde(default)]` its own rustdoc example assumed,
  so a partial block no longer dies on `missing field cleanup_interval_secs` — a
  key no documentation mentions. A test pins the exact documented block.

- **`GATE-1` query validation is one function (#736).** `execute_with_scopes` and
  `execute_dispatch` each carried their own copy of the validator block; they now
  share one, so the two entry points cannot drift.


- **Operator runbooks and the config docs now prescribe only knobs that exist
  (#838).** The runbooks told on-call engineers to export ~24 `FRAISEQL_*` variables
  (`FRAISEQL_QUERY_CACHE_SIZE`, `FRAISEQL_DB_POOL_MAX`, `FRAISEQL_RATE_LIMIT_WINDOW_SECS`,
  …) that zero lines of code read — a mitigation that appears applied and does nothing,
  during a live incident. Every runbook step now names the real knob (config-file key +
  restart, or a variable the server actually reads), the documented rate-limit
  precedence matches the implemented one (server `[rate_limiting]` < compiled
  `[security.rate_limiting]` < CLI/env, guards on the result), ports/endpoints/image
  references were corrected (`8815` → `8000`, `/admin/…` → `/api/v1/admin/…`,
  `fraiseql:latest` → pinned `ghcr.io/fraiseql/server`), and illustrative alert-rule
  blocks are marked as such. The false `FRAISEQL_AUTH_*_MAX_REQUESTS` rustdoc claims in
  `fraiseql-auth` were corrected to the real configuration path.

### Removed

- **Committed development archaeology (#735).** `v2.3.0-ext-phases/` (phase files from
  eleven releases ago) and the stray `target-user/` cargo dir are gone (with a
  `.gitignore` entry so a stray `--target-dir` cannot silently return); the frozen
  `IMPROVEMENTS.md` / `IMPROVEMENTS_R3.md` audit ledgers moved to `docs/history/` (code
  comments still cite their finding IDs); the `spikes/` #687(c) RFC conclusion was
  archived onto issue #687 before removal.

### Breaking

- **FraiseQL is PostgreSQL-only: the MySQL, SQLite and SQL Server backends were removed
  (P22, #374 #721 #799 #829 #830 #831 #832 #833 #834 #870).** Three audit passes found
  the non-PostgreSQL paths had never been executed against a real database, and the
  defects were not marginal: every field-projected query failed on MySQL and SQLite (a
  PostgreSQL-only `jsonb_build_object` projection was spliced into their SQL, #799);
  MySQL boolean equality never matched `true` while `neq: true` matched everything
  (#831); MySQL numeric comparison rounded to an integer, so `19.99` and `20.4` compared
  equal (#830); boolean `ORDER BY` collapsed every sort key to 0 (#829); cursor-paginated
  sorts were silently dropped (#832); a client-controlled `where` field name could break
  out of a MySQL string literal (#833); and a multi-argument SQLite `DELETE` applied only
  the first filter, widening the delete (#834). Supporting them properly means three more
  per-dialect integration matrices in CI forever, against a design that is
  PostgreSQL-shaped throughout (Trinity views, JSONB `data` columns, RLS tenancy,
  `LISTEN/NOTIFY` subscriptions, WAL-based CDC).

  **Removed:** the `mysql`, `sqlite`, `sqlserver`, `mssql`, `test-mysql`,
  `test-sqlserver`, `multi-db` and `all-db` Cargo features on every crate; `MySqlAdapter`
  / `SqliteAdapter` / `SqlServerAdapter` and their introspectors; `MySqlDialect` /
  `SqliteDialect` / `SqlServerDialect`; `MySqlProjectionGenerator` /
  `SqliteProjectionGenerator`; the `quote_mysql_identifier` / `quote_sqlite_identifier` /
  `quote_sqlserver_identifier` and `escape_mysql_json_path` / `escape_sqlite_json_path` /
  `escape_sqlserver_json_path` helpers; the observers' MySQL and MSSQL NATS bridges; and
  the `MySQL`, `SQLite` and `SQLServer` variants of `DatabaseType`, which now has one
  variant. `DialectCapabilityGuard` and its `Feature` matrix are gone too — three audit
  passes confirmed the guard was never called from any production path.

  **Migration:** move to PostgreSQL 14+. A `mysql://`, `sqlite://` or `sqlserver://`
  database URL is now refused at startup by both `fraiseql-server` and `fraiseql run`,
  with an error naming the removal — it is never silently downgraded. A
  `[collation.database_overrides.mysql|sqlite|sqlserver]` config table now fails to parse
  (`deny_unknown_fields`) rather than being silently ignored. Because the removed
  backends returned wrong results on filters, sorts and projections rather than working,
  treat data from such a deployment as suspect rather than as a baseline to reproduce.
  See `docs/database-compatibility.md`.

- **`where` field names are validated at the parse boundary (#833).** A `where` key
  outside the GraphQL identifier pattern `[_A-Za-z][_0-9A-Za-z]*` — a quote, a backslash,
  a leading digit — is now rejected with a `Validation` error instead of being
  interpolated into SQL. This is the same rule `orderBy` already enforced, and it is kept
  after the de-scope because it protects PostgreSQL too. A client sending such a key
  previously reached SQL generation; it now gets an error.

- **CDC drain redesign (P20, #797 #814 #815).** `core.tb_cdc_sink_state` gains a
  `lease_expires_at` column and an `in_flight` status (idempotent `ADD COLUMN IF NOT
  EXISTS` migration; re-run `outbox_sink_state_migration_sql`). The enqueue cursor is now
  an anti-join bounded by a commit-lag window (default 15 min,
  `DrainWorker::with_commit_lag_window`) with a periodic full recovery sweep
  (`with_sweep_every`, first tick always sweeps) — a row whose transaction commits out of
  sequence order is no longer permanently dropped. Publishing is claim-then-publish under
  a lease (`with_lease`, default 10 min) with **no database transaction held across broker
  calls**, and a transiently failing row now **blocks its successors** (head-of-line
  blocking; a dead-lettered row releases them) instead of being overtaken —
  `DrainStats.retried` therefore counts at most the head row per tick, and `DrainStats`
  gains `late_recovered`.
- **`fraiseql-wire` connection strings parse their query component strictly (#817).**
  `?sslmode=…`, `?application_name=…` and `?connect_timeout=…` are honoured (`sslmode` is
  *enforced*: a plaintext connect refuses `require`/`verify-*`, a TLS connect refuses
  `disable`, and the opportunistic `prefer`/`allow` modes are refused outright); any other
  parameter is a loud `WireError::Config` instead of being folded into the database name.
  `ConnectionInfo.user`/`database` are now `Option<String>` (explicit-vs-defaulted is
  distinguishable; `user_or_default()`/`database_or_default()` apply the OS-user
  convention), and `Connection::streaming_query` takes the entity name as a parameter
  instead of re-deriving it from the SQL text.
- **`fraiseql-wire` `connect_with_config`/`connect_with_config_and_tls` implement their
  documented merge (#877).** The connection string's explicit user, password, database,
  `application_name` and `connect_timeout` now override the passed `ConnectionConfig`
  (they were previously parsed and silently discarded, so the startup packet carried the
  config's credentials and no password).
- **`fraiseql-wire` `TlsConfig` drops `verify_hostname` and
  `danger_accept_invalid_hostnames` (#877).** Both flags were stored and reported but
  never reached the rustls verifier — hostname verification is always on. The
  debug-build-only `danger_accept_invalid_certs` remains the self-signed-development
  escape hatch (it disables the whole verification, hostname included).
- **`fraiseql-wire` `OrderByClause` renders JSONB fields with text extraction (`->>`)
  (#877).** The previous `->` navigation yielded `jsonb`, so any collated JSONB order
  clause failed at the server with `collations are not supported by type jsonb` (42P22).
- **`fraiseql-wire` SASL mechanism-list decoding hard-errors past the cap (#729)** like
  every other decode cap, instead of silently truncating the list.
- **`fraiseql_arrow::execute_batched_queries` rejects heterogeneous result schemas
  (#717).** A Flight stream carries one schema header; a batch whose queries infer
  different schemas now returns `InvalidArgument` naming both shapes instead of emitting
  an undecodable stream.

- **`fraiseql init` refuses `--database mysql|sqlite|sqlserver|mssql` (#823 follow-through
  of the PostgreSQL-only decision).** The scaffolder still generated projects for the
  removed engines — projects the runtime refuses to boot. It now errors with the removal
  notice instead of scaffolding; `postgres` is the only accepted value.

- **`fraiseql generate-views --validate` now requires a database.** It executes the
  generated DDL against `DATABASE_URL` inside a rolled-back transaction and fails when
  PostgreSQL rejects any statement (#821). The previous flag checked only the view-name
  prefix, so it could never fail — it reported files with syntax errors as "valid". Runs
  without `DATABASE_URL` now exit non-zero with an explanation instead of claiming
  validity.

### Fixed

- **The first thirty minutes work: `fraiseql init` produces a project that runs (#823,
  #822, #569).** The scaffolded views exposed plain columns with no `data` JSONB column,
  so every query against a fresh project failed with `column "data" does not exist`; the
  printed next step (`fraiseql compile fraiseql.toml`) could not succeed on the project
  init had just generated; the CRUD functions returned bare `UUID`/`BOOLEAN` values the
  mutation executor cannot decode; and the Python authoring skeleton was a `SyntaxError`
  that imported nothing and never exported. The scaffold now follows the runtime's
  contracts end to end: Trinity views with snake_case JSONB storage keys, camelCase
  GraphQL field names, nine declared mutations backed by v2.2 `mutation_response`
  functions built with the `fraiseql setup` helpers, and printed next steps
  (`fraiseql setup` → `psql -f …` → `fraiseql compile schema.json` → `fraiseql query`)
  that are executed verbatim by a new live-PostgreSQL e2e suite
  (`init_first_run_pg.rs`), including a committed mutation with its change-log outbox
  row.

- **`fraiseql doctor` resolves hostnames (#819).** The `DATABASE_URL`/`REDIS_URL`
  reachability probe parsed `host:port` with `str::parse::<SocketAddr>` — which performs
  no DNS — and silently dialed the always-refused sentinel `0.0.0.0:0` for every
  hostname-based URL, reporting healthy databases as "connection refused" and exiting 1.
  The probe now resolves through the system resolver, tries every resolved address, and
  reports "host does not resolve" as its own failure mode distinct from "connection
  refused".

- **`fraiseql generate-views` emits the source relation instead of a literal `{}`
  (#821).** Both composition views were unconverted `format!` placeholders — PostgreSQL
  syntax errors on every invocation, with no off switch. They now read the generated
  view; `tv_` targets drop with `DROP MATERIALIZED VIEW` (the plain `DROP VIEW` made
  re-runs fail); the `_recent` helper filters on the column the view actually exposes;
  and the monitoring function is `STABLE`, not `IMMUTABLE`.

- **Fact-table introspection picks the dimensions column by role, not position (#825).**
  With several JSONB columns the last one in ordinal order silently won — guaranteed
  wrong for the documented calendar layout, whose `*_info` columns follow the real
  dimensions column, so `validate-facts` hard-errored on correct schemas and
  `introspect facts` printed calendar metadata for developers to paste. Calendar columns
  are now excluded, a conventional name (`data`/`dimensions`) wins among several
  candidates, and a genuinely ambiguous layout is reported instead of silently picked.
  Non-indexed numeric `*_id` columns now surface as unindexed filters instead of
  vanishing from the metadata.

- **Mutation prepare failures on a fresh stack are actionable (#569).** A missing
  `core.tb_entity_change_log` now says to run `fraiseql setup`; a mutation function that
  does not return the v2.2 `mutation_response` row (e.g. `RETURNS SETOF v_*`) now gets an
  error naming the contract and the `fraiseql.mutation_ok`/`mutation_err` builders,
  instead of the bare `column r.entity_type does not exist`.

- **The documented onboarding path works and is CI-enforced (#734).** The quickstart used
  a `fraiseql.config()` API that does not exist, the wrong server flag (`--schema` for
  `--schema-path`), the wrong port (3000 vs the actual 8000 default), and never created
  the `data`-column views the runtime reads; `examples/basic` declared `v_users`/`v_posts`
  while its SQL created `v_user`/`v_post`, its `schema.py` imported from a nonexistent
  path, and its example queries used Int IDs and a phantom nested field. All rewritten
  against the real SDK API and verified end to end; the phantom `config()` section is
  gone from the Python SDK reference; `docs/architecture/overview.md` and
  `docs/operations/compiled-schema-lifecycle.md` version/flag/port drift corrected. The
  durable gate is a new `quickstart` integration leg (`tools/quickstart-smoke.sh`) that
  extracts the quickstart's fenced code blocks and executes them **verbatim** against real
  PostgreSQL — authoring with the Python SDK, compiling, booting `fraiseql-server`, and
  asserting the documented query response — plus a scaffold-skeleton regeneration check.

- **CDC sinks: outbox rows that commit out of sequence order are never lost (#797)**, a
  transient per-message failure no longer reorders the stream (#815), and a slow broker no
  longer pins a Postgres transaction (and the vacuum horizon) across up to 256
  round-trips (#814). Proven by three new drain integration tests against real Postgres
  (overlapping commits, head-of-line blocking, `pg_stat_activity` transaction probing).
- **Arrow `build_insert_query` emits valid PostgreSQL for every supported type (#715):**
  timestamps render as ISO-8601 literals with `::timestamptz` casts (the old two-argument
  numeric `to_timestamp` does not exist in PostgreSQL, and its `%` arithmetic produced
  negative operands for pre-epoch values); `NaN`/`Infinity`/`-Infinity` render as
  quoted-and-cast literals. A new integration suite (`insert_sql_pg`, wired into the
  Postgres integration leg) **executes** the generated SQL against real PostgreSQL for
  every Arrow type, including all four timestamp precisions, pre-epoch values, float
  specials and NULLs.
- **ClickHouse sink flush timer fires on schedule under a steady stream (#718):** the
  flush deadline anchors at the first buffered row instead of resetting on every received
  message (which left latency unbounded until the size threshold tripped).
- **Elasticsearch sink surfaces an HTTP-client build failure (#718)** instead of silently
  substituting a default client without the configured request timeout.
- **`fraiseql-wire` edge findings (#729):** `soft_limit_warn_threshold` now warns (log +
  `fraiseql_memory_soft_limit_warned_total` metric, once per stream) instead of being
  dropped; `pause()` on a completed/failed stream no longer corrupts the state snapshot;
  the memory limit is documented as the items×2KB heuristic it is; SCRAM HMAC failures
  map to `KeyDerivation` (not `Utf8Error`) and derived key material (salted password,
  client key) is zeroized.
- **`fraiseql-wire` metrics entity label can no longer be minted from row data (#877):**
  the label comes from the caller-known entity (validated as an identifier) instead of a
  heuristic scan of the rendered SQL that could land inside a user-supplied literal and
  create unbounded label cardinality. Adaptive-chunking `adaptive_min_size` /
  `adaptive_max_size` now apply independently instead of requiring both.

### Deprecated

- **`fraiseql_wire::operators::generate_where_operator_sql` (#877).** It emits `$N`
  placeholders that the crate's simple-query protocol can never bind — no encoder for
  Parse/Bind exists and `QueryBuilder` has no method accepting the parameter map, so the
  advertised usage failed at the server with `there is no parameter $1`. Deprecated (and
  the module docs corrected) until the crate either implements the extended query
  protocol or renders operator values as safely quoted literals; use
  `QueryBuilder::where_sql` with an inline predicate.

- **Saga store and recovery API (P19, #744 #745 #766 #767 #785).**
  `PostgresSagaStore::claim_stuck_sagas` takes a `stuck_after_secs` staleness threshold and
  `find_pending_sagas` an `older_than_secs` age gate; `RecoveryConfig` gains
  `stuck_threshold` (default 5 min) and `max_recovery_attempts` (default 5); `SagaStep`
  gains `remote: bool` (set at creation from the coordinator's registry) and
  `compensation_error: Option<String>` (the recorded outcome of the last rollback
  attempt). `update_saga_step_state` now **validates transitions atomically** — illegal
  writes (e.g. `Completed → Executing`, anything out of `Compensated`) return
  `InvalidStateTransition` — and `save_saga_step`'s upsert no longer rewrites `state`
  (state changes must go through the guarded method). `SagaRecoveryManager::with_routing`
  (new `RecoveryRouting`) carries the subgraph registry/HTTP client/entity resolver so
  recovery can re-drive remote steps on their real transport.
- **`HttpMutationClient::execute_mutation` takes an `idempotency_key: Option<&str>`**
  parameter, sent as the `Idempotency-Key` header on every attempt (#747). Saga steps pass
  their persisted step id; compensations a derived `<step-id>:compensate` key.
- **Federation mutation literal building is dialect-aware (#728).**
  `value_to_sql_literal` and `build_insert_query`/`build_update_query`/`build_delete_query`
  take a `DatabaseType`; MySQL (whose backslash-escaping mode is connection-dependent and
  unobservable here) is refused loud instead of mis-escaped.
- **Federation `_entities` wrappers error on resolution failure (#764).**
  `batch_load_entities`, `batch_load_entities_with_tracing` and
  `batch_load_entities_enforced` now return `Err` when any typename batch failed, instead
  of returning `Ok` with all-`None` entities and discarding the errors.
- **Placeholder federation APIs removed or made loud (#785).**
  `FederationResolver::get_or_determine_strategy`, its `strategy_cache` field and the
  `types::ResolutionStrategy` enum are **removed** (the strategy was a hardcoded
  `http://localhost:4000` / nonexistent `<Type>_federation_view`).
  `FederationMutationExecutor::execute_extended_mutation` now always returns an error
  pointing at the real remote-dispatch path (`HttpMutationClient` / saga steps) instead of
  fabricating a success response that no subgraph ever saw.
- **`SagaCoordinator::cancel_saga` no longer writes `Cancelled` over un-compensated work
  (#746).** When the rollback is incomplete the saga is left `Failed` (as the compensator
  recorded), the result reports `compensated: false` and names the un-rolled-back steps.
- **`fraiseql federation check --against` semantics (#820).** `@override(from:)` references
  are validated against the supergraph's declared roster (`federation.subgraphs`) — not
  harvested from its `override_from` annotations — and reported as *unchecked* when no
  roster exists; the blanket "Composition check passed" claim is gone.

### Fixed

- **Saga crash recovery no longer re-executes committed work (#744).** Forward replay
  skips steps already `Completed` (their persisted result stands in); a `Compensated`
  step in a forward drive fails loud. The store's new transition guard makes the
  double-execution write (`Completed → Executing`) unrepresentable on every code path,
  including a second concurrent driver.
- **The recovery loop no longer claims actively-executing sagas (#745).** "Stuck" now
  means *stale*: a live forward drive heartbeats the saga row on every step transition,
  and only sagas untouched past `RecoveryConfig::stuck_threshold` are claimable — so a
  saga mid-flight is never concurrently re-driven by a recovery tick. The same age gate
  covers pending-saga pickup (a saga in its creator's `create_saga` → `execute_saga`
  window is not stolen).
- **Saga mutation dispatch is deduplicable (#747).** Every remote dispatch carries a
  stable `Idempotency-Key` (the persisted step id) across retries, timeouts and
  crash-recovery replays, and the FraiseQL server now honours it on the GraphQL mutation
  path: a repeat with the same body replays the stored response (one logical effect), a
  repeat with a different body is HTTP 409, queries ignore the header. Documented for
  non-FraiseQL subgraph authors in the saga guide.
- **Recovery never replays a remote-subgraph step against the local database (#766).**
  A step bound for a registered remote peer is persisted as `remote`; forward execution
  fails it loud when no transport is configured, and a recovery worker without routing
  **parks the saga for manual recovery** (state untouched, lease pushed to infinity)
  instead of silently executing another service's mutation locally. With
  `SagaRecoveryManager::with_routing`, remote steps re-drive over HTTPS correctly.
- **`compensated: true` is only reported for a rollback that fully happened (#746).**
  Both coordinator call sites now read the `CompensationResult` they used to discard;
  partial rollback surfaces the failed step numbers in the result error.
- **`get_compensation_status` reads recorded state (#767).** Rollback outcomes are
  persisted per step (`Compensated` transition on success, a recorded
  `compensation_error` on failure), the magic-key sniffing of forward payloads is gone,
  `failed_steps` is real, `PartiallyCompensated` is reachable, step numbers are 1-indexed
  like every other API, and a saga with no compensation evidence reports `None` rather
  than a fabricated verdict. Mid-flight compensation reports the new
  `CompensationStatus::InProgress`.
- **Federation `_entities` database errors surface as errors (#764).** A failed batch is a
  GraphQL error response, never `data: [null, …]` — a `null` entity now always means
  "not found", and a router can distinguish a database outage from missing data.
- **Dotted `@requires` paths build valid `_entities` selections (#765).**
  `dimensions.weight` now renders `dimensions { weight }` (an object field with a
  subselection) instead of a bare composite-field leaf that every spec-compliant subgraph
  rejects — documented dotted-path support works against Apollo-class peers.
- **The composition validator enforces its documented rules (#728).**
  `ExternalFieldMultipleOwners` is actually raised; two subgraphs *primarily* defining the
  same type with different `@key`s conflict; an extension keyed on **any** of the
  primary's declared keys is accepted (not just the first); and type-level `@shareable`
  counts in field-sharing consistency.
- **`fraiseql federation check --against` genuinely compares (#820).** `@key` agreement,
  field sharing (the `INVALID_FIELD_SHARING` class that shipped as #698) and `@override`
  roster references are checked and can fail; the success message states exactly what ran
  and defers final authority to the gateway composer. `composable: true` is never
  fabricated for a comparison that did not run.
- **Recovery attempts are genuinely counted and capped (#785).** Each recovery record
  carries a real incrementing attempt count (previously hardcoded 0 forever), and a saga
  past `max_recovery_attempts` is parked for manual recovery instead of being retried
  forever while its recovery rows grow without bound.

- **A live subscription's authorization now holds for the life of the stream (#771).** The
  principal was validated once at the WebSocket upgrade and then trusted forever: an
  expired or revoked JWT kept its RLS-scoped subscriptions and kept receiving row data
  until the client itself disconnected (the in-code A44 TODO). The connection now
  re-checks token expiry and — when a revocation store is configured — revocation on a
  configurable interval (`subscription_auth_recheck_secs`, default 30), consults the
  revocation store (never the IdP) on that hot path, and additionally refuses every event
  delivery on an expired token. A failed check closes the socket with **4401
  Unauthorized**. Pinned over real WebSockets for the expired-at-delivery,
  idle-expired-stream, and revoked-mid-stream (`revoke-all`) paths.
- **Subscription event loss is no longer silent (#772).** Two seams dropped events without
  telling anyone: the observer runtime forwarded CDC events into the `EventBridge` with a
  non-blocking `try_send` (a full channel dropped the event for **every** subscriber, warn
  only), and a connection whose broadcast receiver lagged skipped events with a warn and
  kept streaming. The bridge forward is now a **bounded, awaited send** — a full channel
  applies backpressure to the durable change-log loop (whose checkpoint only advances
  after the batch completes) instead of dropping. Broadcast lag now terminates every
  operation on the affected connection with an explicit **`EVENTS_LAGGED`** error frame —
  a documented resync signal (re-subscribe, then re-query) — so a client can always
  distinguish "nothing happened" from "events were dropped".
- **CDC snapshot rows are no longer delivered as phantom `created` events (#773).** The
  `EventBridge` defaulted every unknown operation string to `Create`, so Debezium `'r'`
  (snapshot/read) change-log rows — surfaced as `CUSTOM` — were broadcast to
  `*Created`-topic subscribers as newly created entities; a snapshot of 10,000 existing
  rows became 10,000 spurious creation notifications. The bridge event's operation is now
  the closed `SubscriptionOperation` enum decided at the forward site by an exhaustive
  match over the (now closed) observer `EventKind`: real changes map 1:1, `Custom` is
  filtered, and an unrecognised `modification_type` in the change log is **rejected and
  logged** (the row is skipped, loudly) instead of silently defaulted to a no-op.
- **Row-visibility policy hot-reloads now reach already-connected subscriptions (#611).**
  A `subscription_policy` added or tightened by a schema hot-reload only applied to new
  subscriptions; existing connections kept their subscribe-time boundary until restart — a
  fail-open window. Every successful executor swap now bumps a reload signal; each live
  connection re-derives its active operations against the current policies with the same
  fail-closed derivation used at subscribe time: still-authorized subscriptions are
  re-scoped **in place** (effective from the next event), and subscriptions the new policy
  refuses are terminated with a `SUBSCRIPTION_REFUSED` error frame.

### Added

- **Graceful subscription drain on shutdown (#571).** When graceful shutdown begins, every
  active subscription receives a per-operation `Complete` frame and the socket closes with
  **1001 (Going Away)**, so clients see a clean end-of-stream during a rolling deploy
  instead of a transport-level abort indistinguishable from a network fault.
- **`subscription_auth_recheck_secs`** server config key (default 30): how often a live
  subscription re-checks its principal's expiry/revocation (#771). `0` disables the
  periodic check; per-delivery expiry enforcement remains.

### Breaking

- **`fraiseql-server`'s bridge `EntityEvent.operation` is now `SubscriptionOperation`**
  (was a free-form `String`), and `fraiseql-observers`' `EventKind` is a **closed enum**
  (no longer `#[non_exhaustive]`), so the subscription forward mapping is an exhaustive
  match and an unmapped variant is a compile error instead of a silent fall-through
  (#773).
- **Unknown `modification_type` verbs in `tb_entity_change_log` are rejected.**
  `INSERT`/`UPDATE`/`DELETE` and the explicit no-op verbs `CUSTOM`/`NOOP`/`READ` remain
  valid; anything else now errors at conversion (the row is skipped and logged, the
  checkpoint still advances) instead of being silently treated as a no-op (#773).

- **A restart no longer replays the entire change log (#805).** The observer runtime wrote
  a checkpoint after every batch but nothing ever read it back — and the row was keyed on
  the entity type of whatever row happened to be last in the batch, so there was no global
  cursor to read. Every process start (deploy, OOM, node drain) re-read
  `core.tb_entity_change_log` from row 0 and re-fired every webhook, email and Slack
  message ever recorded, with severity growing with deployment age. The runtime now
  restores the cursor at startup under a stable listener identity (`listener_id`, default
  `"change_log"`), ensures the checkpoint table exists (the shipped idempotent migration),
  and persists through `PostgresCheckpointStore` after each dispatched batch. Delivery is
  explicitly **at-least-once with a one-batch replay window**; payloads carry the
  change-log row UUID as the dedup key. Pinned by a genuine restart test (second runtime,
  same pool, zero re-dispatch).
- **The job-queue worker actually executes jobs (#844).** `timeout_job_execution` was a
  placeholder returning `Ok(())`: every dequeued observer action was logged as completed,
  counted in `job_executed`, and acknowledged — which `DEL`s the only copy of the payload —
  without any dispatch ever happening. The worker now dispatches the action against the
  event carried on the job, bounded by `job_timeout_secs`; a timeout is a transient failure
  retried per policy, terminal failures land in the DLQ with the payload intact, and a job
  is only removed after a confirmed terminal outcome. Also fixed on the way: the error path
  called `mark_failed` twice per failure (double-counting attempts), and `fail()` re-checked
  `can_retry()` on the already-incremented counter, dead-lettering jobs one attempt early
  with a stored state (`pending`) contradicting the status hash (`dead_lettered`).
- **`field_changed*` conditions error loudly when change tracking is unavailable (#845).**
  On the default change-log path (`changelog_pre_image = false`) UPDATE rows carry no
  pre-image, so `field_changed` / `field_changed_to` / `field_changed_from` silently
  evaluated false — a documented condition family that could not fire in the default
  configuration, indistinguishable from "correctly configured, not matching". Evaluating
  them against an UPDATE without a pre-image is now an error naming the missing
  `changelog_pre_image` prerequisite; a recorded pre-image with an empty diff is a clean
  `false` (the two cases are no longer conflated). The docs (`condition` module, crate
  docs, webhooks.md) now state the prerequisite, and the crate docs' example of a
  non-existent `status_changed_to` function is corrected.
- **Condition `==`/`!=` compare numbers numerically (#843).** serde_json equality is
  representation-strict, so `total != 100` was true for a PostgreSQL `numeric(10,2)` value
  of `100.00` — firing observers on rows they should skip — while `>=`/`<=` on the same
  operands coerced and agreed the values were equal. Equality now routes through the same
  numeric-aware comparison as the ordered operators (exact `i64`/`u64` first, so values
  above 2^53 stay exact), shared with `field_changed_to`/`field_changed_from`, which had
  the identical root cause.
- **`database` and `log` observer actions dispatch for real (#632).** The admin API's 400
  for those action types (the #612 stopgap) is lifted: `database` calls the configured
  PostgreSQL function with a `{"event": ..., "params": ...}` jsonb envelope (function name
  restricted to a strict SQL identifier, re-validated at dispatch), and `log` emits one
  structured tracing event at the configured level with a rendered message template. Both
  fail loud when their backend is absent.
- **Observer metrics reach the server's `/metrics` (#634).** The observer subsystem records
  into the `prometheus` crate's default registry while the server scrape is rendered from
  the `metrics-exporter-prometheus` ecosystem — two registries that never met, so
  `fraiseql_observer_*` series were absent from every scrape. The server (feature
  `observers-metrics`, included in `observers-enterprise`) now appends the observer
  registry's rendering to the scrape output.
- **The observer E2E suite runs, and can pass (#928).** None of its 8 tests constructed a
  runtime — nothing polled the change log, so every test waited for webhooks that could not
  be sent — and no CI leg ran the file. Several also registered observers for `"Order"`
  while inserting `"Order_{test_id}"` rows, asserted a log status (`"failed"`) the writer
  never emits, and counted webhook deliveries with a mock that only recorded successes.
  Each test now drives a real `ObserverRuntime`; the suite is wired into the Dagger
  observers integration leg, and the #844 job-queue tests into the redis leg.
- **`MultiListenerCoordinator` docs no longer claim cross-process HA (#872).** The module
  advertised "shared checkpoint store, leader election, failover coordination" while every
  structure is process-local — three replicas each elect *themselves* leader and all poll
  concurrently. The docs now state the process-local reality and point HA users at the
  advisory `CheckpointLease` plus the durable checkpoint cursor.

- **Every `cron:` function fires on every matching window, not once ever (#796,
  CRITICAL).** `CronExecutionState::should_execute` returned `last_exec >= window_start` —
  the exact negation of its own comment — and `find_schedule_window` stepped back one minute
  before searching, returning the *previous* window (or, for any schedule sparser than
  hourly, giving up after a 60-minute scan and returning the tick instant itself). Under
  real wall-clock timestamps every daily and weekly schedule fired exactly once and then
  never again, and sub-hourly schedules degenerated to a per-tick coin flip that wedged
  permanently after the first miss — silently, with nothing logged. The window is now the
  minute *containing* the tick and the guard is `last_executed < window_start`; the fix is
  pinned by a ported 20 000-tick simulation asserting exactly one fire per matching window
  under sub-second jitter. Every scheduling loop (functions cron, server cron, scheduled
  sources — including #573 scheduled ingress, which this bug had capped at one run per
  process) now logs a window-suppressed tick at `warn` instead of silently continuing.
- **`_fraiseql_cron_state` is read back at boot (#796).** The table was documented as the
  cross-restart "already fired this window" guard, but `PgCronState` had `record_fire` and
  no loader — nothing ever read it. Each cron poller now resumes its fire-window state from
  the durable record; a state read failure refuses boot instead of silently double-firing.
- **Cron day-of-week fields use POSIX numbering (#841).** Matching used chrono's
  `number_from_sunday()` (Sun=1…Sat=7) against POSIX fields (Sun=0…Sat=6), so `0 9 * * 1`
  fired on **Sundays**, `1-5` meant Sun–Thu, and `0` (Sunday) could never match at all.
  Weekday tokens now match their POSIX days, `7` is accepted as the alternate Sunday, and a
  calendar-pinned test covers every token.
- **A dispatched function sees a real identity (#803).** The live host's `SecurityContext`
  was a hard-coded `anonymous` placeholder (documented "for testing") on every production
  path, so `fraiseql_auth_context()` fabricated an empty identity and `send_email` could
  never resolve a sender — the entire wiring was dead on arrival, dead-lettering every
  send. The host now carries the triggering caller's authenticated context on the
  after:mutation request path (GraphQL and REST), and the function's own `run_as` identity
  on background paths (cron, sources, after:ingest, after:capture); the `fraiseql_query`
  bridge stays under the `run_as` ceiling. A host with no wired identity fails
  `auth_context()` loudly instead of fabricating one, and the send-status/suppression
  tenant stamp now carries the caller's tenant instead of collapsing to NULL.
- **`fraiseql_env_var` can actually return a value (#840).** The env-var allowlist had no
  producer — no TOML key, no env var, no builder — so deny-by-default degenerated into
  deny-always while docs described granting secrets, and a blocked read was
  indistinguishable from an unset variable. The allowlist is now populated from
  `FRAISEQL_FUNCTIONS_ALLOWED_ENV_VARS` (after:mutation/cron) and `[sources]
  allowed_env_vars` / `FRAISEQL_SOURCES_ALLOWED_ENV_VARS` (sources).
- **The Deno CPU watchdog stays armed across the event loop (#804).** It was disarmed
  immediately after `execute_script` returned — before the event loop ran — so a guest that
  spun *after* an `await` (a poll loop without a sleep) pinned an executor thread and its
  V8 isolate at 100 % CPU forever; the event-loop `tokio::time::timeout` future was never
  polled again and could not fire. Script evaluation and the event loop now share one
  watchdog deadline, and a spin after a real async host op is terminated at `max_duration`.

### Breaking

- **Runtime observers have exactly one source of truth (#631).** Compiled handler
  declarations are not a runtime concept: the compiled `ObserversConfig` no longer has a
  `handlers` field (and is `deny_unknown_fields`, so a schema smuggling one fails to load),
  `[[observers.handlers]]` keeps failing the TOML compile as permanent policy, and an
  SDK-authored `observers_config.handlers` array — which previously slipped through the
  seam and landed in the compiled schema as decoration — now fails the compile with a
  message naming `tb_observer` / `POST /api/observers`. The unused `EventHandler` type is
  removed from `fraiseql-core`.
- **`job_queue::Job` carries the full triggering `EntityEvent`** (field `event` replaces
  `event_id`): a bare event id gave the worker nothing to dispatch with (#844).
  `Job::new`/`Job::with_config` signatures changed accordingly; jobs serialized by
  pre-#844 builds do not deserialize (they were never executed anyway).
- **Go SDK: `Enum` takes ordered members (#929).** `Enum(name, values map[string]string)`
  iterated a Go map, so the exported member order was randomized per run — two builds of
  one schema produced different artifacts and the SDK conformance gate was a coin flip —
  and the map's values were silently dropped (only keys were ever exported). The
  signature is now `Enum(name string, members ...string)`, and every `GetSchema` category
  is exported in sorted-name order so the whole export is reproducible.
- **Quoted condition literals are strings (#843).** The DSL lexer previously discarded
  quoting, so `code == '100'` compared a string field against the *number* 100 and was
  silently false forever. A quoted literal now always compares as a string and never
  equals a number; `total == 100` (unquoted) compares numerically.
- **`fraiseql-server`'s `observers` feature now requires `fraiseql-observers/checkpoint`**
  — the durable cursor is not optional (#805) — and a new `observers-metrics` feature
  (included in `observers-enterprise`) compiles the metrics bridge (#634).
- **An unrecognized `after:mutation`/`after:capture` operation token fails the load
  (#842).** `after:mutation:User:created` (or `:INSERT`, or any typo) used to silently
  widen the trigger to *all* event kinds — a welcome-email function also fired on every
  delete. Only `insert`/`update`/`delete` narrow; the documented `*` wildcard and the
  token-less form still mean "all kinds"; anything else aborts startup with an error
  naming the function and the valid tokens.
- **`http:` triggers are rejected at registry load (#871).** They were accepted, stored in
  a matcher no server code consumes, and never served — a declared `http:` function
  silently did nothing while `POST /functions/v1/{name}` ignored the trigger entirely.
  Until a mounted route surface exists, a declared `http:` trigger aborts startup with the
  same loud error `after:storage` gets. The `TriggerRegistry` `http_routes` field and its
  accessors are removed.
- **`env_var` refuses non-allowlisted names loudly (#840).** A blocked name is now an
  authorization error (a thrown exception in Deno guests; `result` in the WASM WIT, whose
  `get-env-var` signature changed to `result<option<string>, string>`); `Ok(None)`/`null`
  is reserved for an allowlisted but unset variable.
- **`fraiseql_sql_query` is documented as not implemented (#871).** The guest typings and
  architecture docs advertised a working raw-SQL op; it has never had an execution
  backend (statements were classified, never executed, then failed loud). The typings,
  the host module doc's "RLS-backed raw SQL" claim, and the docs now say so.
- `LiveHostContext.security_context` is no longer a public field; wire an identity with
  `with_security_context(...)`. The dead `host::factory` module (a stub with no
  production caller) is removed. `build_cron_pollers` is now async and fallible;
  `spawn_after_mutation` takes the triggering caller's `SecurityContext`.

### Security

- **A project-wide `[inject_defaults]` tenant predicate reaches the operations it configures
  (#847).** Python, Go, Java, PHP, C#, Elixir and F# each ship a `ConfigLoader` that parses
  `[inject_defaults]` from `fraiseql.toml` and emits it as a top-level key. No compiler code
  path had ever read it — `grep -rn inject_defaults crates/` returned nothing — and
  `IntermediateSchema` accepted unknown keys, so an operator who wrote
  `[inject_defaults] tenant_id = "jwt:tenant_id"` to stamp every operation with the caller's
  tenant got `✓ Schema compiled successfully` and **not one compiled operation carrying a
  tenant predicate**. Seven separate implementations of a feature that had never done
  anything.

  The merge runs in the converter, *after* `[fraiseql.tenancy]` validation rather than before
  it: tenancy auto-injects the annotated field when an operation's `inject_params` is empty
  and **fails the compile** when it is non-empty but lacks that field, so applying defaults
  first would have made a single unrelated default (`read_scope`, the example in the Python
  SDK's own docstring) break every tenancy-annotated query. Precedence, most specific first:
  the operation's own `inject_params`, then tenancy auto-injection, then
  `[inject_defaults].queries`/`.mutations`, then `.base`.

- **Custom-scalar validation rules survive the TOML compile workflow (#755).** `merge_values`
  rebuilt the merged schema from scratch with only `version`/`types`/`queries`/`mutations`, so
  a `custom_scalars` block — and its `validation_rules` — was discarded on every
  `--types`, `--schema-dir`, `--type-files`, domain-discovery and includes compile. An `Email`
  scalar with a pattern rule compiled to nothing and invalid values flowed straight to SQL.
  Seven other authorable categories were dropped with it: enums, input types, interfaces,
  unions, subscriptions, observers and ingress sources.

- **A Python-declared custom scalar reaches the compiled schema at all (#922).** The Python
  SDK emitted `customScalars` as an object keyed by name with a `validate: true` flag; the
  compiler reads `custom_scalars` as an array of `IntermediateScalar`. Three independent
  mismatches — key name, container type, element shape — so no Python custom scalar had
  **ever** been compiled, on any path, and no scalar validation ever ran. The `validate` flag
  is not re-emitted: the compiler's `ValidationRule` is declarative
  (`Pattern`/`Length`/`Range`/`Enum`) and a Python `validate()` method cannot lower into one,
  so the flag asserted runtime enforcement no compiled artifact could deliver.


- **A REST read projects the fields it was asked for, and the field-authorization gate
  fires on that path (#886).** `QueryMatch::from_operation` — the only `QueryMatch`
  constructor the REST transport uses — built a *flat* list of leaf `FieldSelection`s
  instead of one root selection carrying `nested_fields`. Every consumer reads the
  requested field set as `selections.first().nested_fields`, so all of them saw an empty
  slice: the planner projected nothing (`{"data":[{},{},{}]}` for any request, with or
  without `?select=`), and `deny_if_gated_field_selected` was handed nothing to inspect on
  a path whose own comment calls it "leak-proof".

  The two halves masked each other — no gated value leaked only because no value was
  served at all — so repairing projection alone would have converted "REST returns
  nothing" into a live field-authorization bypass. They are fixed together, gates first.

  A third defect sat behind them: `RestFieldSpec::All` expanded to an **empty vector**, so
  "all fields" and "no fields" were the same value. Teaching the projector that empty means
  "project everything" would have left the gate inert, because the gate reads that same
  list; `All` now expands to the type's declared fields.

  `execute_query_direct` additionally ran **no** field RBAC at all — `requires_scope` was
  unenforced across the whole REST read surface. The choice between the authenticated and
  anonymous classifier is a property of whether a principal exists, not of the transport,
  so both GraphQL runners and the REST runner now route through one
  `classify_fields_for_read`.

- **An embedded collection cannot be widened by a client filter (#863).**
  `embed_into_single` seeded the sub-query `WHERE` map with the parent join predicate and
  then merged the client's `?rel.field[op]=value` filter over it with
  `serde_json::Map::insert`, which *replaces*. A filter naming the join column destroyed
  the parent scoping, so one parent's record came back advertising another parent's
  children as its own. The conventional `referenced_key` for `ManyToOne`/`OneToOne` is
  `id`, so `?author.id[gt]=0` was enough. The two predicates are now composed with `_and`,
  which makes the collision structurally impossible rather than defended against.

- **An `Idempotency-Key` is valid only within its tenant, method and resource (#915).**
  The client-supplied header value was used verbatim as the store key, so it collided
  across everything sharing a process: the same key and body on `POST /users` and
  `POST /orders` replayed each other's stored response, and two tenants retrying an
  identical request under a natural key such as `order-42` received each other's results.
  The store API now takes a `ScopedIdempotencyKey` whose only constructor is
  `IdempotencyScope::key`, so keying on an unscoped string is a compile error rather than
  an omission a reviewer has to notice.

- **The Arrow Flight result cache is scoped to the requesting principal (#716).** It was
  keyed on the SQL text alone, while the same file's documentation told operators to scope
  rows "by the underlying `va_*` view itself (e.g. a view that filters on a session/tenant
  setting)". The two are incompatible by construction — with a session-scoped view, one
  tenant's rows are cached under the SQL string and the next tenant issuing the identical
  SQL is served them.

  The documented mitigation was never implementable on this path in the first place:
  `ArrowDatabaseAdapter::execute_raw_query` takes a SQL string and nothing else, so a view
  filtering on `current_setting('app.tenant_id')` has nothing to read. Entries are now
  addressed by `(principal, SQL)`, using the same `hash_security_context` the executor's
  response cache uses, so one principal never observes another's read; and the doc says
  what the path actually does — no RLS, no session scoping, only expose a `va_*` view over
  Flight whose full contents every Flight-authenticated principal may read.

  The isolation is asserted end-to-end through `do_get`, on the principal the live path
  derives from the session token.

- **A `where:` argument that cannot be serialized is refused, not dropped (#719).** The
  GraphQL parser stored inline argument values as JSON built with
  `format!("\"{}\"", s.replace('"', "\\\""))` — escaping the double quote and nothing
  else. A string literal containing a backslash (a Windows path), a newline or a control
  character therefore produced **invalid JSON**, and the reader discarded it with
  `.ok()?`. Dropping a filter does not narrow a result set; it widens it, so a
  serialization bug on a `where:` argument returned rows the filter existed to exclude.
  Serialization now goes through `serde_json`, and a stored value that cannot be read
  fails the query.

  The same seam carried variable references **in band** as the string `"$name"`, so a
  literal `"$100"` was indistinguishable from a reference to a variable called `100` and
  resolved to `null`. A variable is now the tagged object `{"$var": "name"}`; GraphQL
  names match `[_A-Za-z][_0-9A-Za-z]*`, so a client cannot forge that key. `make
  lint-value-json` (Dagger `shell-gates`, CI `value-json-seam-check`) refuses a new
  hand-rolled escaper, a new `$`-prefix check, and a new direct parse of `value_json`.

  Two further consumers of the seam are fixed with it: the field authorizer built its
  policy input by substituting the *raw text* for any argument it failed to parse, so a
  policy that matches on an argument decided on something other than what the client
  sent; and the query classifier unquoted `value_json` by hand, mangling any type name
  containing an escape.

- **The MCP tool allowlist is enforced where the call executes (#808).** `[mcp]
  `include`/`exclude`/`read_only` filtered the *advertised* tool list and nothing else:
  `executor::call_tool` never consulted the config, so naming a withheld operation
  directly in `tools/call` ran it in full. An `exclude`d admin query, or any mutation
  under `read_only = true`, was one guessed name away from an AI agent. Advertisement and
  execution now resolve against **one** list of exposed operations, so a tool that is not
  advertised is not reachable. A withheld name and a nonexistent name get the identical
  `Unknown tool` answer — an error that distinguished them would be an existence oracle
  for exactly the operations the allowlist hides.

- **MCP tool arguments no longer reach the GraphQL document as text (#808).**
  `build_graphql_query` validated top-level argument names "to prevent injection via
  malformed argument names" and then rendered the *value* with `graphql_value`, whose
  object arm interpolated nested keys raw. A caller could close the argument list and
  append root fields of their own — reaching operations the allowlist withheld, with field
  selections the tool's projection would never emit, in unbounded number, all of which the
  runtime's multi-root fan-out then executed in parallel. Values now travel as GraphQL
  variables (`query ($filter: JSON) { users(filter: $filter) { … } }`), so the only
  caller-controlled input that reaches the document is an argument *name*, which must
  match one the resolved operation declares. `graphql_value` and its escaper are deleted
  rather than hardened: the primitive is gone. A bounded corpus of injection payloads —
  flat, nested, inside arrays, escape-laden, non-identifier keys — asserts the built
  document always parses to exactly one root field.

- **MCP tool calls go through per-tenant dispatch and the suspended-tenant gate (#858).**
  `FraiseQLMcpService` captured the default executor at session construction and called it
  directly, never resolving a tenant key or consulting `TenantExecutorRegistry`. An
  authenticated caller read the boot database rather than their own tenant's — silently,
  because the control-plane database has the same relations — and a tenant suspended via
  `POST /api/v1/admin/tenants/{key}/suspend` kept reading over MCP while `/graphql`
  correctly answered 503. The service now holds the server's `AppState` and dispatches
  through the same seam the `/graphql` handler uses, so an unregistered key is refused
  instead of falling back to the default executor, a suspended tenant is refused, and the
  tenant's concurrency and per-second quotas apply. That seam is now one function
  (`routes::graphql::tenant_dispatch`) rather than a block written out in one handler, so
  a control added to it is a control on both transports.

  A validated token also becomes a `SecurityContext` through the same builder on both
  transports: MCP called `SecurityContext::from_user` directly, which leaves `tenant_id`
  unset and `attributes` empty, so an MCP caller's `org_id` never became a tenant and
  every `SessionVariableSource::Jwt` mapping resolved to nothing.

- **MCP execution errors are sanitized (#875).** `mcp/executor.rs` returned
  `e.to_string()`, so with `error_sanitization` enabled a `FraiseQLError::Database` handed
  an AI agent the driver message and SQLSTATE verbatim — internal schema and view names
  included — while every other transport returned a sanitized message. The configured
  `ErrorSanitizer` now reaches the MCP path with the rest of `AppState`.

- **Two keys can no longer name one stored object (#813).** `validate_key` rejected only
  `..` and a leading separator, so `docs/./secret.txt`, `docs//secret.txt` and
  `docs/secret.txt` were three distinct metadata rows over one file on the local backend.
  Ownership is enforced against the metadata string and the filesystem collapses the
  spelling, so an attacker's write to an alias found no existing row, took
  `can_write_object`'s *create* branch — which any authenticated user passes — and
  destroyed another user's object while that user's row still named them as owner and
  reported the old size and etag.

  Keys are now **rejected rather than canonicalised**: normalising `a/./b` to `a/b` would
  merge two keys the client believes are distinct, which is the same collision arriving
  through the front door. A key must be a non-empty `/`-separated relative path whose
  every segment is non-empty, is not `.` or `..`, and has no surrounding whitespace or
  trailing `.`; backslashes, control bytes and percent-escapes that decode to path syntax
  are refused. The rule is one function, enforced at the route boundary on the raw key —
  previously it ran only deep inside the backends, on the already-composed
  `"{bucket}/{key}"`, so `GET`/`DELETE` answered from their metadata probe and `presign`
  never consulted it at all. A bounded-exhaustive test asserts the property the bug
  violated: **key → path is injective**.

  The local backend additionally resolves each path before any I/O and refuses one that
  leaves the storage root or whose final component is a symlink. Key validation is
  lexical and cannot see a symlink planted inside the root; both a symlinked directory
  and a symlinked leaf previously redirected reads and writes outside it.

- **A presigned upload now owns the object it creates (#866).** `presign_handler` signed an
  S3 `PUT` URL and recorded nothing. The bytes went straight to the object store, so the
  object had no owner and no metadata: it was permanently `404` through
  `GET /storage/v1/object/...`, invisible to `list`, and — because `can_write_object`
  reads a missing row as *create* — any authenticated user could overwrite it or claim it.
  The H9/B4 overwrite-IDOR guard was void for precisely the door its own doc comment
  names, and every existing regression test for that guard began with a server-side
  `PUT`, the one path that did create metadata.

  Signing now claims the object first, recording the caller as owner and marking the row
  `pending`; a lost race refuses with `409` rather than signing against a stale
  authorization decision, and a signing failure releases a claim it created. The first
  successful read settles the row against the object that actually landed. `list` reports
  the `pending` flag rather than hiding the claim.

- **A `DELETE` releases an ownership claim whose upload never happened.** Reservations
  record ownership before the bytes exist, so `DELETE` has a case the upload path never
  produced: a metadata row with nothing behind it. The backend's `NotFound` is tolerated
  there — the caller is already authorised and the outcome it asks for is "gone" — so an
  abandoned claim cannot squat a key against its own owner. Every other backend error
  still refuses, so a genuine failure cannot orphan the bytes by dropping their metadata.

- **Object bytes with no metadata row are not served.** An orphan — a rolled-back
  reservation, a manual copy into the bucket, a leftover from a deleted row — has no
  ownership record, so there is nothing for the access rules to evaluate. It is refused,
  not treated as public.

- **The storage read paths are no longer an existence oracle (#876).** `get_handler`
  answered `404` for a missing object and `403` for one the caller may not read, so an
  unauthenticated attacker could enumerate a private bucket's keys — often themselves
  sensitive — with no credentials. `presign(download)` had the same split while its own
  inline comment claimed the opposite. Both now answer identically in the two cases, as
  `put_handler` already did.

### Breaking

- **Every official SDK is now held to a cross-SDK conformance suite, and eleven of them
  changed to pass it (#733, #849, #850, #851, #852, #853, #854, #855).** The canonical schema
  is authored through each SDK's *public API*, compiled by the real `fraiseql compile`, and
  the compiled result compared against a shared expectation
  (`sdks/official/conformance/`). Nothing before this ran the compiler, and six of the
  eleven pre-existing "parity generators" hand-wrote their JSON without calling the SDK at
  all — which is why a green parity gate coexisted with a Ruby README documenting an exporter
  that did not exist and a Dart package with no export path.

  Author-visible changes:

  - **TypeScript**: `@Query`, `@Mutation` and `@Subscription` now **throw**, naming
    `registerQuery`/`registerMutation`/`registerSubscription`. They registered placeholders —
    a return type of the literal string `"Query"`, zero arguments — because TypeScript erases
    the types they would need, and `reflect-metadata` does not recover them either. `@Type`
    remains a marker (the federation decorators build on it), but *exporting* a type whose
    fields never arrived is refused. `registerTypeFields` can now complete a `@Type`
    registration, which its own docstring documented and the duplicate guard forbade.
  - **Java**: `SchemaFormatter` emits arrays of objects, not maps keyed by name; `return_type`
    plus `returns_list` rather than a camelCase `returnType` carrying `"[User]"`; arguments as
    `{name, type, nullable}` objects; `javaClass`, `baseType` and `isList` are gone. Argument
    types are GraphQL type expressions, so a trailing `!` means non-null. `QueryBuilder` and
    `MutationBuilder` gain `nullable()` and `requiresRole()`.
  - **PHP**: `MutationBuilder::toIntermediateArray()` emits `invalidates_views` (not
    `invalidates`), adds `invalidates_fact_tables`, and writes `inject_params` (not `inject`)
    in the nested `{source, claim}` form. `returnsList()`, `nullable()` and `requiresRole()`
    are new. `StaticAPI::enum()` is new.
  - **Go**: all four top-level slices carry `omitempty`, so an unpopulated section is omitted
    rather than marshalled to `null`. `FieldInfo` gains `Description`; `RegisterInputType` is
    new. `Config` is no longer serialized and `SqlSourceDispatch` *refuses* at `Register()`,
    because `sql_source_dispatch` has no consumer anywhere in the compiler (#926). The
    analytics surface matches `IntermediateFactTable`: `Measure(name, sqlType, nullable)`
    replaces `Measure(name, aggregations...)`, dimensions carry a JSONB path, and
    `FactTableDefinition` drops `name`/`dimension_paths` for `table_name`/`dimensions`/
    `denormalized_filters`. Observer actions serialize flat rather than under `config`, and
    an observer with no `Retry()` gets `DefaultRetryConfig()`.
  - **C#**: `IntermediateType` carries `relay` and `is_error`; a type marked
    `IsInput = true` is routed into `input_types` instead of being emitted as an output type.
    `Inject`, `RequiresRole`, `InvalidatesViews`, `InvalidatesFactTables` and `RegisterEnum`
    are new.
  - **F#**: `computed` is no longer serialized (#927). `QueryBuilder`/`MutationBuilder` gain
    `inject`, `requiresRole`, `invalidatesViews`, `invalidatesFactTables`;
    `SchemaRegistry.registerEnum` is new. `QueryDefinition`, `MutationDefinition` and
    `IntermediateSchema` gained fields, so record literals need updating.
  - **Elixir**: `requires_scopes` is folded to a singleton `requires_scope` and refused beyond
    one — the array is a key the compiler does not read. `fraiseql_type` no longer requires
    `sql_source` on an `is_input: true` type, and refuses one that sets it: the macro demanded
    a key the compiler forbids, so input objects were unauthorable. `fraiseql_enum` is new,
    and queries/mutations accept `inject_params`, `requires_role`, `invalidates_views` and
    `invalidates_fact_tables`.
  - **Ruby**: `lib/fraiseql.rb` exists, so `require "fraiseql"` resolves — the README's first
    line raised `LoadError`. `FraiseQL::Schema` is implemented: the `schema.type` /
    `schema.query` / `schema.export_json` API the README has always documented.
    `to_fraiseql_schema` emits the required `nullable`, uses snake_case field names to match
    its CRUD sibling, and no longer emits `deprecated`, which `IntermediateField` has no
    member for.
  - **Rust**: `export_to_json` produces `{"version", "types": [...]}` via `serde_json`
    instead of a name-keyed map built with `format!` — a `"` in any name, scope or description
    previously produced text that was not parseable JSON. Keys are snake_case
    (`requires_scope`, not `requiresScope`). `register_type_with_source` is new. The crate now
    depends on `serde`/`serde_json`.
  - **Dart**: `FraiseQLSchema` and `FieldType` are implemented and `crud_generator` is
    exported — the package shipped annotations nothing read and no way to produce a schema.
  - **Python**: `computed` is no longer serialized (#927).

- **A custom scalar declaring `validation_rules` is refused (#922).**
  `CompiledSchema.custom_scalars` is `#[serde(skip)]`: the converter registers the scalar into
  an in-memory registry that is dropped when the compiled schema is written, and nothing in
  `fraiseql-server` reads scalar rules back. A declared `pattern`, `length` or `range` was
  therefore never enforced, from any SDK, while the compile reported success. Carrying the
  rules further without a runtime consumer would relocate the drop rather than fix it — the
  disposition `#779` got for observers. The scalar *declaration* still works, and is what
  makes the name known to the compiler; enforce the constraint in the database (a `CHECK`
  constraint or a `DOMAIN`) or in the mutation's SQL function.

- **The mutation `operation` verb is matched case-insensitively.** `parse_mutation_operation`
  accepted only uppercase, while `docs/authoring.md`, `docs/architecture/intermediate-schema.md`,
  the Python SDK's parity generator, the PHP `MutationBuilder` docblock and the Java
  `OperationBuilder` all use lowercase — every one of them produced
  `Error: Unknown mutation operation: insert`. The verb set stays closed: an unrecognized word
  is still a hard error rather than a silent fallback to `CUSTOM`, and the diagnostic echoes
  what the author wrote rather than the uppercased form.

- **`IntermediateSchema` and the nested intermediate structs reject unknown fields.** Every
  field on the authoring→compile boundary carries `#[serde(default)]`, because an SDK
  legitimately omits most of them. Without `deny_unknown_fields` that combination means any
  key the compiler does not read binds to an empty default and the compile reports success —
  the mechanism behind #755, #756, #779, #847, #848 and, earlier, #806/#807. A `schema.json`
  carrying a key the compiler does not read now **fails to compile**, naming the key.

  Spellings seen in the wild, and what to use instead: `return_array` → `returns_list`;
  `args` with `required` → `arguments` with `nullable`; `customScalars` → `custom_scalars`;
  `inject` → `inject_params`.

- **A schema declaring top-level `observers` fails to compile (#779).** The block was
  validated by ~220 lines of `SchemaValidator` — a typo in any observer field failed the
  build, which told authors emphatically that it was honoured — and then discarded by
  `observers: Vec::new()` under a comment claiming the opposite. No webhook, Slack message or
  email ever fired for any declared event. The runtime loads observers exclusively from the
  `tb_observer` table and the admin API and reads nothing from the compiled schema, so
  carrying them would only have moved the silent drop one layer down. The compile now fails
  and names the mechanism that works.

- **A `[includes]` pattern that matches no files fails the compile (#723).** Previously the
  glob resolved to nothing and compilation continued from TOML-only definitions, producing a
  schema silently missing everything the include was meant to contribute. An empty *list* of
  patterns is still fine — nothing is configured. The same applies to a configured
  `[domain_discovery]` whose root is missing or whose files fail to parse: the schema-source
  fallback now asks "is this configured?" before attempting it, so a failure inside a
  configured source propagates instead of being swallowed by `if let Ok(schema) = …`.

- **`fraiseql validate` exits 2 on a validation failure**, matching the contract
  `--help-json` publishes and what `lint` and `federation check` already did. It exited 1,
  so CI could not distinguish an invalid schema from a broken toolchain (#868).

- **`--show-output-schema compile` is removed.** `compile::run` prints plain lines and never
  constructs a `CommandResult`, so `fraiseql compile --json` emits no `{status, command,
  data}` object for the advertised schema to describe (#868).

- **`fraiseql explain` no longer emits a `sql` field.** Its value was a hard-coded
  `SELECT data FROM v_table LIMIT 1000;` — a relation appearing nowhere else in the codebase
  — published under the label "Compiled SQL representation". The command takes no `--schema`
  argument, so it could not have produced real SQL in principle (#868).

- **A type marked `is_input: true` compiles into `input_types`, not `types` (#848).** Four
  SDKs advertise the flag and emit it; the compiler had no field to receive it, so such a
  type became an *object* type and any mutation argument referencing it produced a schema
  violating GraphQL §3.10. Output-only attributes on an `is_input` type (`sql_source`,
  `relay`, `requires_role`, `is_error`, `implements`, `subscribable_tables`) are now refused
  rather than ignored.

- **The Python SDK emits `custom_scalars` as an array** rather than `customScalars` as an
  object, and no longer emits a `validate` flag (#922).


- **The REST write surface is mounted (#865).** `POST`/`PUT`/`PATCH`/`DELETE` on derived
  resources, and the collection-level bulk routes, are now served by any deployment whose
  adapter implements `SupportsMutations` (PostgreSQL, MySQL, SQL Server). `rest_router`
  had had **no production caller at all** — a regression of the closed #227 — while the
  served `OpenAPI` document went on advertising every write path, so a client following the
  published contract received `405` on all of them. Read-only adapters (`SqliteAdapter`,
  `FraiseWireAdapter`) are unaffected: they cannot satisfy the bound, so the type system
  rather than a runtime check keeps writes off them.

  The mount goes through the one existing REST mount site, so the write half passes through
  the same `Server::attach_auth` call as the read half — `route_layer` does not survive
  `Router::merge` (#812), and a separately-merged write router would have been
  unauthenticated.

- **`rest_router` and `rest_query_router` take a `RestMountConfig`** instead of two
  positional `bool`s. Every call site read `rest_router(&state, false, false)`, where
  nothing distinguished "compression off" from "no auth attached"; the struct also carries
  the new export configuration.

- **The served `OpenAPI` document is derived from the mounted router (#918, #865).** It is
  now filtered through `MountedRoutes` — the same set the router drives its registration
  from — so it describes exactly the operations the server answers. A read-only mount no
  longer advertises the write API, and an item-level `PATCH /items/{id}/rename` no longer
  suppresses the collection-level bulk `PATCH` while the document promises it. The `links`
  member is removed from the collection-GET response schema: `build_query_response` emits
  `data` + `meta` and never populated it.

- **`[export]` is read from `fraiseql.toml`, and `export_formats` defaults to all three
  formats (#917).** `ExportConfig` had no deserialization site anywhere — all three
  production consumers called `::default()`, one under a comment conceding that
  "TOML-driven `ExportConfig` loading is a later phase" — so a configured CSV delimiter,
  BOM setting, row cap, temp directory, concurrency limit and format allow-list each
  reached nothing. The default changes from the empty vector to all three formats:
  empty is documented as "disables all exports", so wiring the kill-switch up without
  changing the default would have turned every export off in every deployment that had not
  written the key. An *explicit* empty list still disables everything, and a disabled
  format is refused with `406`.

- **`GET /{resource}/stream` returns `501` instead of a heartbeat-only `200` (#873).**
  `RestState::event_transport` is `None` at every construction — the struct is private and
  has no setter — so the endpoint emitted `event: ping` forever and no entity event, while
  the served document described it as carrying `insert`/`update`/`delete`. A dashboard saw
  a healthy connection, so its reconnect and error handling never fired and it displayed
  stale data indefinitely; enabling the `observers` feature turned an honest `501` into a
  silent no-op. Wiring a real transport is #428.

- **`?limit=` on a streaming REST export now caps the export total, and an export without
  it returns every row (#811).** The NDJSON, CSV and XLSX batch loops advanced pagination
  by writing `limit`/`offset` into a clone of `variables`, which `execute_query_direct`
  reads only for authorization — it takes limit/offset from `query_match.arguments`. Every
  batch therefore re-issued the identical first-page query, producing one of two failures
  depending on whether the page filled: a 10,000-row export silently returned
  `default_page_size` rows with HTTP 200 and no error line, or, when `rows.len()` equalled
  the batch size, the loop never terminated and re-emitted the same page indefinitely
  while pinning a database connection.

  Previously `GET /rest/v1/x` with `Accept: application/x-ndjson` returned 100 rows and
  stopped, believing it had exported everything; it now streams the whole result set in
  `ndjson_batch_size` pages. `?limit=N` bounds the total. All three formats share one
  pagination driver — they were three independent copies of the same mistake.

- **`Prefer: tx=rollback` is refused on bulk operations rather than silently committing
  (#914).** It was parsed and its only effect was to echo `tx=rollback` in the
  `Preference-Applied` response header — RFC 7240's assertion that the server honoured the
  preference — while the mutation committed. A dry-run bulk `DELETE` destroyed data and
  answered that it had rolled back. Honouring it needs a per-request execution mode
  threaded through `Executor::execute`, whose `RuntimeConfig` is shared across requests, so
  the honest answer today is an explicit 400. Both `Preference-Applied` echo sites are
  removed: a preference can no longer be reported as applied when it was not.

- **`IdempotencyStore::check`/`store` take a `ScopedIdempotencyKey` (#915).** See the
  security entry above.

- **`/health` reports `observers.events_processed`, not `observers.pending_events`
  (#875).** The field carried `RuntimeHealth::events_processed` — a monotonic lifetime
  counter of events already handled — under a name and a doc comment that promised
  "approximate number of events pending in the internal queue". An operator alerting on
  `pending_events > 100` got an alert that fired permanently after the 100th *successful*
  event and never cleared, while a genuine backlog stayed invisible. The observer runtime
  is checkpoint-driven and `RuntimeHealth` carries no backlog source, so the field is
  renamed to what it actually reports rather than a depth being fabricated for it.

- **`FraiseQLMcpService::new` takes an `AppState`, not a schema and executor (#858), and
  `mcp::executor::call_tool` takes an `McpCallContext`.** Both are consequences of the
  MCP transport reaching the same tenant registry and error sanitizer as `/graphql`.
  `require_auth` is no longer a separate parameter — it is read from the `[mcp]` config
  that is now passed in, so the two cannot disagree.

- **The second storage stack is gone (#813, #866).** `fraiseql_server::storage` (a
  duplicate `StorageBackend` trait with its own local/S3/GCS/Azure implementations) and
  `fraiseql_server::routes::storage` (a `/storage/v1/object/{*key}` router) have been
  removed, along with `ServerBuilder::with_storage`. It was a parallel object API with no
  metadata, no per-object ownership and no RLS — its download handler served any file in
  the backing store to any holder of a single shared token — and it carried its own copy
  of both defects fixed above: a byte-identical weak `validate_key`, and the same Azure
  key-encoding bug. No binary mounted it and no configuration key reached it.

  Use `ServerBuilder::with_storage_state` and a `[storage.<name>]` section; that backend
  now also serves as the inbound-email attachment sink, which previously hung off the
  removed builder method and was therefore unreachable from the shipped server.

- **`allowed_mime_types = []` now allows nothing**, as documented, instead of being read
  by the upload handler as "no restriction".

- **The object-metadata table gains a `pending` column.** The DDL is idempotent and
  applies on startup.

- **`WhereClause` gains a `Typed` variant, and `WhereClause::from_graphql_json` takes the
  declared field types (#798).** The cast a filter needs is a property of the *field*, so
  parsing a user filter without the compiled schema's types is what produced SQL that
  errored on every date and silently under-matched on numbers. The types are required
  rather than optional, and they travel as a node of the clause rather than as an argument
  on the adapter seams the clause passes through — `ProjectionRequest`, the relay cursor
  path, the wire adapter, federation, the cache key — because each of those would
  otherwise be a place to drop them. Embedders with no schema pass
  `SharedFieldTypes::default()` and get the previous value-shape inference.

- **`OrderByFieldType` is renamed `ScalarFieldType`**, and the type → SQL-cast mapping
  moves onto `SqlDialect::cast_type_name`. ORDER BY and WHERE previously carried separate
  tables, so a sort and a filter on the same field could disagree about its type. The
  per-dialect `cast_to_numeric` / `cast_to_boolean` / `cast_param_numeric` methods are
  replaced by `cast_expr_as` / `cast_param_as`.

  Two renderings change as a result: MySQL and SQL Server now cast `Numeric` to
  `DECIMAL(38,12)` (previously `DECIMAL` and `FLOAT` in WHERE), and SQLite emits no cast
  for date/time types (`CAST(… AS TEXT)` was a no-op over an already-textual extraction).

- **Thirteen operator names are no longer advertised (#828).** `has_key`, `has_any_keys`,
  `has_all_keys`, `array_eq`, `array_neq`, `notInSubnet`, `contains_date`, `adjacent`,
  `strictly_left`, `strictly_right`, `not_left`, `not_right` and `distance_within` were in
  `OPERATOR_REGISTRY` — so REST's `?filter=` accepted them and its error messages
  recommended them — with no `WhereOperator` variant behind any of them. Every request
  that used one was accepted by the transport and then rejected by the executor. The
  registry is now generated from the executor's own table, so it can only advertise what
  runs.

- **`WhereOperator` gains an `IsNotNull` variant**, and both null-check operators now
  require a boolean operand instead of reading a non-boolean as "assume IS NULL".

- **A malformed `validation_rules` block fails compilation (#720).** `serde_json::from_value(…).unwrap_or_default()`
  turned a typo'd rule into an empty rule set, so a scalar declared with validation
  shipped with none.

- **`DatabaseAdapter::invalidate_list_queries`, `CachedDatabaseAdapter::invalidate_list_queries`
  and `QueryResultCache::invalidate_list_queries` are removed**, along with the
  `list_index` reverse index and `CachedResult::is_list_query`. List-versus-point-lookup
  classification was derived from result cardinality and was the root of #742; there is no
  sound replacement at that layer, so the distinction is gone rather than repaired. Callers
  use `invalidate_views`, which is what the mutation path now does for every operation
  kind. Expect more evictions per mutation: a point lookup for an unrelated entity is now
  dropped and re-read, where before it was kept on a premise that was never checked.

- **`CachedDatabaseAdapter::with_ttl_overrides_from_schema` is renamed
  `with_cache_metadata_from_schema`.** It is the single seam between the compiled schema
  and the row cache, and it now reads `additional_views` as well as `cache_ttl_seconds`;
  the old name described half its job. `rebuilt_for_schema` (hot reload) delegates to the
  same reader, so a per-query cache annotation cannot work at boot and stop working after
  a schema reload.

- **`QueryCache::get`/`put` in `fraiseql-arrow` take a `CacheScope` first argument.**
  Required rather than optional so no call site can store an entry another principal could
  read back (#716).

### Fixed

- **The intermediate-schema specification no longer documents keys the compiler rejects.**
  `docs/architecture/intermediate-schema.md` documented `inject` where the compiler reads
  `inject_params`, and `invalidates` where it reads `invalidates_views` — which is exactly what
  the PHP SDK emitted, so the document was upstream of #852. Its "minimal valid example" did
  not compile. The worked examples are now the conformance fixtures verbatim, compiled on
  every CI run, and the root/type/field/operation tables list the members that actually exist.

- **PHP field scopes and descriptions reach the compiled schema.** `TypeConverter` parsed and
  validated `#[GraphQLField(scope: ...)]` onto `TypeInfo` and `SchemaExporter` read
  `FieldDefinition::$scope` to emit `requires_scope`, but `SchemaRegistry::extractFieldDefinition`
  never passed the value between them — the property was always null and the exporter's scope
  branch was unreachable, so the #807 fix could not have taken effect. A field the author gated
  still compiled ungated. Field descriptions were a second, independent drop in the same map.

- **The DDL table namer is the acronym-aware one (#738).** `commands::compile` carried a second
  `to_snake_case` that inserted a separator before every uppercase character, so `HTTPServer`
  became `h_t_t_p_server` in emitted DDL while JSONB key derivation produced `http_server` —
  DDL for a table the runtime never looks in. Both now call
  `fraiseql_core::utils::to_snake_case`.

- **`load_from_paths` rejects duplicate names across files (#738).** The directory loader
  detected a name claimed twice in any authorable section; the explicit-file loader — the
  `--schema-file a.json --schema-file b.json` path, and the one the TOML merger calls — did
  not, so two definitions were concatenated in silence and argument order decided which one
  the compiler used. Both loaders now share one detector.

- **A description containing `*/` can no longer inject code into a generated TypeScript client
  (#738).** `push_doc` interpolated author-supplied text into `/** … */` unescaped, and every
  description in the generated client — type, field, enum, input, union, interface, query,
  mutation — passes through it. A field documented `"… */ export const OWNED = 1; /*"` emitted
  TypeScript that closed the comment and declared what followed.

- **Vacuous CLI tests replaced (#738).** `test_validate_schema_success` and
  `test_validate_schema_unknown_type` built ~100-line `CompiledSchema` literals and then
  asserted only that the literal contained what had just been put into it, with comments
  admitting the validation they were named after was never reached; they now drive
  `SchemaValidator` and assert the report. `test_cost_provides_score` asserted inside
  `if let Some(data)`, so it passed when the command returned no data at all.

- **Shipped SDK examples compile, and are gated (#850, #925).** Three committed
  `ecommerce_schema.json` artifacts were stale generated output that no longer matched their
  generators and could not be compiled; they are deleted. Two Python examples imported
  `fraiseql.observers` and `fraiseql.fact_table`, neither of which exists, and are removed.
  The Go analytics and observer examples now compile. The observer examples no longer claim
  "observers will execute automatically on database changes" — the compiler refuses declared
  observers and the runtime loads them from `tb_observer` and the admin API.
  `sdks/official/conformance/check_examples.sh` runs every covered example and compiles what
  it emits; coverage is opt-out, with each exclusion carrying a reason and an issue number.

- **`fraiseql init` scaffolds a project that compiles to what it declares (#921).** The
  scaffolded `schema.json` used `return_array` and `args`/`required`, which the compiler does
  not read, so **all five** queries compiled with `returns_list: false` and `arguments: []`.
  `posts`, `authors` and `tags` were served as single objects rather than lists, and
  `post(id:)`/`author(id:)` took no arguments at all — there was no way to fetch one by id.
  The documented first-run flow (`fraiseql init && fraiseql compile`) produced a schema in
  which not one query behaved as declared.

- **TOML-declared operation arguments reach the compiled schema (#756).** The merger emitted
  `"args"` with a `"required"` flag; `IntermediateQuery`/`IntermediateMutation` deserialize
  `"arguments"` with `"nullable"`. Both sides carried `#[serde(default)]`, so the mismatch
  bound to an empty vector: a query declaring `args = [{ name = "id", required = true }]`
  compiled with no arguments, its `id` filter was never bound, and it returned rows the
  author had excluded. Arguments are now serialized *through* `IntermediateArgument`, so the
  wire keys come from the field definitions and there is no second spelling to keep in sync.
  The same change carries the argument `description` that the compiled `ArgumentDefinition`
  has always had and nothing could reach it with.

- **`fraiseql compile` no longer panics while composing an error message (#724).**
  `suggest_similar_type` sliced `&typo[0..1]` and `&name[0..1]` — *byte* ranges — so an empty
  base type (`"return_type": ""`) or a type name beginning with a multi-byte character aborted
  the process mid-diagnostic. Ranking now uses a real edit-distance match over characters, so
  a typo of `User` gets `User` rather than `Universe` and `Umbrella`.

  Three further diagnostic gaps in the same issue: duplicate-type errors reported the count of
  unique names seen rather than the offending element's index; a misspelled field type was
  auto-registered as a custom scalar, which also legalized the typo as a query return type;
  and the converter's validation tier bailed on the first error with no suggestion while the
  validator collected all of them with suggestions, so identical mistakes got materially
  different diagnostics depending on which tier caught them.

- **`[grpc]` in `fraiseql.toml` reaches `CompiledSchema.grpc_config` (#780).** The type
  documented itself as "compiled from `[grpc]`", but nothing in the CLI parsed such a section
  and `TomlSchema` is `deny_unknown_fields`, so following that documentation failed with
  "unknown field `grpc`". Removing the section compiled — and the server then silently never
  mounted gRPC. There was no supported way to enable a shipped, end-to-end-tested transport.
  `enabled = true` without a `descriptor_path` now fails the compile rather than producing a
  server that cannot mount what it was told to serve.

- **A source's declared `cursor` override is the key the runtime advances (#868).**
  `SourceDefinition::cursor_name()` existed, the schema validator enforced uniqueness on it
  ("a shared cursor name would let two sources clobber each other's watermark"), the
  converter compiled it and `fraiseql sources` printed it — while `build_source_pollers`
  passed `source.name` to the cursor store. An operator renaming a source from `orders` to
  `orders_v2` with `cursor = "orders"` to preserve the watermark advanced a brand-new row and
  re-ingested the entire history on the first tick.

- **`fraiseql doctor` no longer renders a skipped security check as a pass (#868).**
  `check_tls` and `check_rls_cache_coherence` returned `DoctorCheck::pass(…, "check skipped")`
  when the config failed to parse as a schema TOML — which is the *normal* case for the
  documented compiled-schema + runtime-config flow, since `TomlSchema` is
  `deny_unknown_fields` and knows no `[metrics]`/`[tracing]` tables. Both render as `[✓]` and
  count toward "All checks passed", so a missing TLS certificate that would abort server boot
  was reported green. Both checks now read the config as generic TOML — so they work for both
  config shapes and usually *do* run — and a check that could not run is a warning.

- **`fraiseql functions invoke` no longer panics on a multi-byte host-op query (#868).**
  `summarize` sliced at byte index 80, so an accented identifier or a literal like
  `city: { eq: "Zürich" }` aborted the process *after* the isolate had run, losing the guest's
  result and the host-op log.

- **`--show-output-schema` describes what the commands actually emit (#868).** Every declared
  schema disagreed with its producing struct, `cost` most visibly: it declared `depth`,
  `field_count` and `score` as *required*, and `CostResponse` emits neither `field_count` nor
  `score` — so a consumer validating a successful response against the advertised schema
  rejected every one. A test now serializes a representative response per command and checks
  it against the declared schema in both directions.

- **The validator's scalar table and the converter's agree.** `BUILTIN_SCALAR_NAMES` listed
  six names including `"JSON"`; `parse_field_type` matches twelve including `"Json"` — the
  spelling every SDK emits. So a field typed `Json` was not a known scalar to schema
  validation, and one typed `JSON` compiled to `FieldType::Object("JSON")`, a reference to a
  type that does not exist. Both were masked by the implicit custom-scalar registration
  removed above; a drift test now fails the build if the two tables diverge.


- **Every REST mutation returned an empty entity (#919).**
  `Executor::execute_mutation_with_security` — the entry point every REST mutation takes —
  hard-coded its synthetic selection set to `{ status entity_id message }`, the
  `app.mutation_response` envelope column names rather than the fields of the mutation's
  declared return type. A `POST` created the row and answered `201` with
  `{"data":{"createItem":{}}}`, so a client could not learn the id of what it had just
  created. This is the write-path twin of #886 and stayed invisible for the same reason:
  the surface had no production caller, and the REST tests asserted status codes and
  `affected_rows` rather than response content. The selection set is now the return type's
  declared fields, falling back to the envelope names only when the type is unknown.

- **Per-operation `rest` annotations survive compilation (#846).** Every SDK emits
  `"rest": {"path", "method"}` on queries and mutations, and the server's route derivation
  reads `rest_path`/`rest_method` as the path override — but `IntermediateQuery` and
  `IntermediateMutation` declared no such field, and the intermediate schema has no
  `deny_unknown_fields`, so serde discarded the block and both converter sites wrote `None`
  unconditionally. An author who set `rest_path` got a clean compile and a 404; worse,
  `detect_conflicts` answers a route collision with "Use `rest_path` override to resolve",
  advice that could not work. The annotation is now validated loudly at compile time: an
  unsupported verb, a path without a leading `/`, a path carrying a query string, and an
  unknown key inside the `rest` block each fail the build rather than degrading silently.

- **`Prefer: handling=lenient` is honoured (#873).** It was parsed, merged across repeated
  headers, and advertised in the served document with the example summary "Ignore unknown
  parameters", while having no reader outside `prefer.rs`. Unknown query parameters now
  are ignored when it is set — and only unknown ones: a malformed value or an unknown
  bracket operator on a known field still fails. It is echoed in `Preference-Applied` only
  when actually applied.

- **`ETag` / `If-None-Match` → `304`, and `Location` on `201` (#873).**
  `RestConfig::etag` defaults to `true` and the served document promises a `304` on GET,
  but `RestResponseFormatter` — which implements all of it — had no production caller, so
  no `ETag` was ever emitted and an operator setting `etag = false` observed no change
  because the feature had never been on.

- **A collection `PATCH`/`DELETE` mutates every matched row and reports what it did
  (#913).** `execute_bulk_by_filter` ran the filter query, **discarded the matched rows**,
  called the mutation exactly once with the request body and no row identity, and then
  reported `affected_rows` as the number of rows the *filter* matched — a fabricated count
  for work it had not done. Both `_id_field` and `_max_affected` were unused parameters, so
  no cap was enforced anywhere. Replaced by `execute_bulk_by_ids`, with row selection and
  the cap in the REST handler where the filter guard and the HTTP status for "too many
  rows" belong. The row identity overwrites any same-named body key, so a bulk request
  cannot redirect a per-row mutation.

- **The bulk "at least one filter" guard checks what reaches SQL (#862).**
  `has_filter_params` answered a syntactic question about the query string while
  `build_filter_query_match` forwarded only `params.where_clause`, so `?filter={}`,
  `?search=x` and any dotted key satisfied the guard and produced no `WHERE` clause — and
  no `limit` argument either, making the selection an unbounded scan of the whole view.
  The syntactic pre-check is deleted rather than repaired: two functions answering the same
  question differently was the defect. The guard now runs after extraction, `search` and
  embedding filters are refused explicitly instead of dropped, and the selection query
  always carries a `LIMIT`. A list query that does not accept a `where` argument is refused
  outright, since its filter would otherwise be dropped and the first `max_affected` rows
  of the whole view mutated under a filter the caller believes applied.

- **`Prefer: max-affected` can lower the configured bulk cap but never raise it (#916).**
  It used `unwrap_or(config.max_bulk_affected)`, so a client-supplied value replaced the
  operator's limit outright.

- **Collection-level bulk routes are registered, not merely advertised (#918).** The
  router recorded the *collection* path in its "already registered" set whenever it
  registered any `PATCH`/`DELETE` for a resource, including item-level ones, so a single
  `PATCH /items/{id}/rename` suppressed the collection route — while the served OpenAPI
  advertised it regardless. Every affected path answered 405 against the server's own
  published contract.

- **Nested embeddings execute to the depth the validator accepted (#864).**
  `execute_embeddings` collected only `SelectEntry::Field` from a spec's sub-select, so
  nested `Embedded` entries were parsed, depth-validated against `max_embedding_depth`
  (default 3, documented as `?select=posts(comments)`), and then silently discarded. The
  response carried no `comments` key at all, and a client could not distinguish "no
  comments" from "the server dropped my selection". Validator and executor now agree by
  construction.

- **The REST idempotency body hash does not depend on key order (#911).** `hash_body`
  hashed the rendered JSON, and since `serde_json/preserve_order` became an unconditional
  workspace feature `Value` preserves insertion order in every build. Two renderings of the
  same request hashed differently, and the layer treats a differing hash as a different
  request under the same key — so a client whose encoder reorders keys between attempts
  (Go's `encoding/json` sorts map keys; Python retry wrappers commonly use
  `sort_keys=True`) received `409 Conflict` on the retry the key exists to make safe. The
  body is normalized with `fraiseql_core::apq::normalize_json_value` before hashing rather
  than growing a second recursive sorter. Array order remains significant.

- **A re-cached entry stays reachable by every invalidation path (#740).** `put_arc`
  registers a key in the reverse indexes *before* `store.insert`, deliberately, so an
  `invalidate_views` racing the insert cannot miss it. The consequence is that moka fires
  the eviction listener for the entry the insert displaced — while its replacement is
  already live under the same key — and the listener pruned by key alone, deleting the
  registrations the *live* entry depended on. Two concurrent misses of one hot query were
  enough. The detached entry could never be evicted by a mutation again: served until TTL
  expiry, or for the process lifetime on a view annotated `cache_ttl_seconds = 0`, which
  is documented as "no TTL — mutation-invalidated only".

  Each entry now carries a process-unique epoch and registers/deregisters itself under
  `(cache_key, epoch)`, so registration and removal are symmetric per *entry instance*.
  The listener needs no knowledge of moka's `RemovalCause` taxonomy, which means a moka
  bump cannot reintroduce this by reporting a different cause. `ResponseCache` had the
  identical listener and gets the identical fix.

- **Every successful mutation evicts every cached entry for the views it touched
  (#741, #742, #763).** Post-mutation invalidation tried to be precise, and neither signal
  it used proved the entries it kept were unaffected by the write:

  - `is_list_query` was `result.len() > 1`, so a filtered list matching nothing, and one
    matching a single row, were not lists and were never evicted on CREATE (#742) —
    precisely the results a CREATE is most likely to change.
  - CREATE and UPDATE were told apart by whether the payload carried `entity_id` rather
    than by the declared operation, so a create function that stamps the new row's id took
    the entity-aware branch and evicted nothing: no entry cached before the row existed
    can contain its id (#741). PostgreSQL functions stamp it naturally, and the SQLite
    `DirectSql` insert path forwarded it unconditionally.
  - Entity-aware eviction on UPDATE reached only entries whose rows already contained the
    mutated id, so an update that moves a row *into* a cached filtered list left that list
    stale (#763) — the strategy comment claimed "precise, no false positives" without
    addressing the false negatives.

  The decision is now one pure function of `(mutation, outcome, schema)` that resolves
  views from all four declarations that can name one — `invalidates_views`, the return
  type, the entity type a payload wraps, the `entity_type` stamped on `mutation_response`
  — plus cascade side-effects, and sweeps them. `returns_list` was considered as the
  schema-derived replacement for the row count and rejected: a query returning a single
  object (`currentUser`, `latestPost`) is still affected by a CREATE, so "not a list" does
  not mean "not affected" and no flag in the compiled schema does.

- **A query's declared `additional_views` reach the cache (#761).** `additional_views` is
  authored, validated by the CLI as safe SQL identifiers, and documented in `key.rs` as
  "required for correct invalidation when a query reads from multiple views" — and its one
  consumer, `extract_accessed_views`, had no runtime caller. A query on `v_report`
  declaring `additional_views = ["v_user"]` was registered under `v_report` only, so a
  `User` mutation never touched it. Both caches now register an entry under every view its
  query reads.

- **The response-cache key covers the whole operation (#760).** It hashed
  `QueryMatch::fields` — the *top-level* selection names, which for `{ users { id } }` is
  `["users"]`. Nothing below the root reached the hash, so `{ users { id } }` and
  `{ users { id name email } }` shared one entry and whichever ran first decided the shape
  the other client received. Nested field arguments (`posts(limit: 3)` vs
  `posts(limit: 50)`) were excluded for the same reason, and so were aliases, because
  `fields` holds the field *name*: `{ people: users { id } }` replayed the envelope keyed
  under `users` and answered nothing at all under `people`.

  Key derivation moved to `cache::key::generate_response_cache_key`, which hashes the full
  resolved selection tree — every field's name, alias, arguments and directives,
  recursively — plus the operation name and canonically-hashed variables, so a new
  dimension added in the one place that owns cache keys reaches this cache too.

- **`@skip`/`@include` on a named fragment spread did nothing (#826).** The parser preserves
  a spread's directives, and both production paths then ran `FragmentResolver` *before* the
  directive evaluator — expanding `...HeavyFields @skip(if: $lite)` into its fields and
  discarding the `@skip` on the way, so the fields came back regardless. `@skip` and
  `@include` are spec-valid on `FRAGMENT_SPREAD`, and conditional fragments are how every
  real client turns a heavy subtree on and off.

  Reordering the two passes is not available: the executor memoises classification against
  the query string alone, so evaluating a `$variable` condition during expansion would let
  one request's variables decide the next request's field set. Expansion now **carries the
  spread's directives onto every field it contributes** instead, so the evaluator downstream
  still sees them. The conditions compose rather than replace: `...F @include(if: true)`
  cannot resurrect a field that `@skip(if: true)` inside `F` withheld. The inline-fragment
  lift in the `node` classifier had the same bug in miniature and gets the same treatment.

- **`node(id:)` dropped named fragment spreads, and an empty selection returned the whole
  row (#827).** The Relay `node` branch lifted `nested_fields` out of every child whose name
  starts with `"..."` — right for an inline fragment, wrong for a named spread, whose
  pseudo-field always has an *empty* `nested_fields`. `query($id: ID!) { node(id: $id) { id
  ...Container_data } }` is the shape Relay Modern issues for every lookup, and it projected
  `id` alone. When the selection collapsed to nothing the projection hint became `None` and
  the adapter returned the untouched `data` JSONB — every column in the view, for a client
  that had asked for none of them.

  The `node` path now resolves spreads through the same routine as the query matcher and
  evaluates `@skip`/`@include` in its runner, which it never did at all. "Nothing requested"
  now projects nothing rather than everything, matching the regular query path; three
  separate blob fallbacks are gone, including one inside `generate_typed_projection_sql`
  itself and two `unwrap_or_else(|_| "data")` arms that served every column when projection
  generation failed.

- **A multi-root mutation executed only its first root (#759).** `classify_query_with_parse`
  built the mutation from `parsed.selections.first()`, so `mutation { createUser(…) { id }
  createAuditLog(…) { id } }` ran the first write, silently discarded the second, and
  answered with a success envelope naming only the first — the client's only clue was a
  missing key. Every root now executes serially in document order, per the spec. A root that
  fails contributes `null` at its key plus an entry in `errors` and the remaining roots still
  run, so a client that issued three writes learns which of them landed.

- **A multi-root query using a fragment was a hard error, and its directives were ignored.**
  The multi-root fan-out re-serializes each root into its own query string, and that string
  carried neither the document's fragment definitions — `{ users { ...F } posts { id } }`
  failed the whole request with `Fragment not found: F` — nor any `@skip`/`@include`, at the
  root or nested, which were simply dropped. The selection set is now resolved before the
  fan-out, and the serializer emits directives rather than silently discarding part of what
  it was given.

- **Root-field aliases were dropped on queries.** `{ a: users { id } }` answered under
  `users`, because the response envelope was keyed by the *compiled query definition's* name
  rather than the document's response key; two aliased selections of one query collapsed into
  a single key. `QueryMatch::response_key()` is now the one answer, used by the regular,
  relay and mutation envelopes alike.

- **GraphQL responses came back with their fields alphabetised.** The spec requires a
  response's fields to appear in the order the query asked for them. Without
  `serde_json/preserve_order` a `serde_json::Map` is a `BTreeMap`, so they were sorted — and
  whether the feature was on depended on *feature unification*: `deno_core` enables it, so a
  `-full` build and a default build disagreed about the meaning of every `serde_json::Value`
  in the workspace. It is now declared in the workspace manifest and inherited by every
  crate. Field-level RBAC separately moved masked fields to the end of the object by building
  the projection as `allowed` then `extend(masked)`; masked fields now keep their requested
  position and are nulled in place.

- **The observer idempotency token depended on JSON key order in any build pulling
  `deno_core` (#900).** `derive_idempotency_token` hashed `payload.to_string()` and relied on
  `serde_json::Value` sorting object keys — true only without `preserve_order`. In the
  shipped `-full` binary a payload re-serialised by a transport hashed differently, the
  ledger found no prior entry, and the observer ran a second time: a duplicate email, a
  duplicate charge, with nothing logged. The payload is now key-sorted explicitly and
  recursively before hashing; array order is left alone, because it is content.

- **The schema content-hash test verified something the product does not do (#899).** Both
  real paths — the CLI writer and the `from_json` verifier — already canonicalized before
  hashing, and were correct. Only `schema_integrity_verification` hashed the
  *uncanonicalized* value, agreeing with the verifier by accident of `Map` being sorted in
  the build it ran in, and failing under `--workspace --all-features`. There is now one
  `content_hash_of` used by the writer, the verifier, `CompiledSchema::content_hash` and the
  test, plus properties asserting the digest ignores key order and respects array order.

- **A property test asserted a stale operator vocabulary.** `prop_operator_rejects_unknown`
  excluded known operators from a list maintained inside the test, which went stale when #828
  unified the vocabularies and added `ne`. The generator draws `[a-z]{1,10}`, so it failed
  only when it happened to produce that string — passing CI on the commit that broke it. The
  exclusion is now read from `WHERE_OPERATORS`, and a companion property asserts every
  advertised name parses.

- **Every date, timestamp, UUID and string range filter was a hard SQL error (#798).**
  `gt`/`gte`/`lt`/`lte` cast both sides to `::numeric` regardless of the field's declared
  type, so `events(where: { createdAt: { gte: "2024-01-01" } })` aborted the statement with
  `invalid input syntax for type numeric`. `PostgresWhereGenerator` is a type alias of the
  generic generator, so this was the main path every PostgreSQL deployment uses — date-range
  filtering, the most common GraphQL filter there is, did not work. Casts are now chosen
  from the declared field type for every comparison operator at once, which also makes
  `createdAt: { gte: … }` compare *instants* rather than text: a stored
  `2024-01-01T10:00:00+02:00` now correctly sorts before an `09:00:00Z` bound.

  It survived three years because every `gte` test in the repository used a numeric
  literal. The replacement is an operator × declared-type matrix executed against real
  PostgreSQL (`crates/fraiseql-db/tests/where_operator_type_matrix.rs`), asserting the
  returned rows rather than the generated string.

- **`in: [19.9]` returned no rows where `eq: 19.9` matched one (#800).** The `In`/`Nin` arm
  applied none of the casts its sibling operators applied, comparing the raw `text`
  extraction against text parameters — so a stored `NUMERIC(p,s)` value of `19.90` did not
  equal the string `'19.9'`. Worse in the other direction: because `Nin` wraps the same
  predicate in `NOT (…)`, an under-matching `IN` became an *over*-matching `NOT IN` that
  returned the row the client had asked to exclude. Both operators now share the single
  cast decision, and the matrix asserts `eq: X` and `in: [X]` select the same rows for
  every type, with `nin: [X]` their exact complement.

- **`= ANY ARRAY[…]` is not valid PostgreSQL (#835).** The wire-adapter generator emitted
  `data->>'status' = ANY ARRAY['active', 'pending']`; the grammar requires the array
  operand to be parenthesised, so every `in`/`nin` filter on a `wire-backend` build was a
  syntax error. The repository's own test asserted the invalid string verbatim. It now
  emits `= ANY (ARRAY[…])` / `<> ALL (ARRAY[…])`, casts the JSONB extraction before
  comparing it to a numeric or boolean literal (`data->>'qty' = 5` was
  `operator does not exist: text = integer`), and renders an empty list as the constant it
  means instead of an untypeable `ARRAY[]`.

- **REST's `ne` and `is_null` bracket operators always returned 400 (#828).** Two operator
  vocabularies had drifted: `OPERATOR_REGISTRY`, which REST validates against, advertised
  79 names; `WhereOperator::from_str`, which the executor parses with, understood 52. The
  27-name gap meant `?status[ne]=archived` and `?deletedAt[is_null]=true` passed validation
  and then failed in the WHERE parser — and the resulting error message recommended two
  dozen more names that behaved identically. Both are now generated from one table, with a
  test asserting agreement in both directions.

  A null check's operand is also coerced to a boolean rather than to the field's declared
  type, which on a `DateTime` field yielded a string that `as_bool().unwrap_or(true)` read
  as "assume IS NULL" — inverting `is_null=false`.

- **`contains`/`startswith`/`endswith` over-matched on SQLite and SQL Server (#722).**
  `escape_like_literal` neutralises `%`, `_` and `\` with a backslash, which only works if
  the dialect treats `\` as the escape character. PostgreSQL and MySQL do; SQLite and SQL
  Server have no default, so `contains: "100%"` matched any value beginning `100`. Both
  now emit an explicit `ESCAPE '\'`.

- **A validation error was formatted into a string and re-parsed (#720).**
  `validate_input` rendered a structured `ValidationFieldError` into a message and then
  recovered it with `find('(')` / `find(')')`; a field path containing a parenthesis — or
  any change to the message format — silently discarded the violation and let validation
  **pass**. The structured error is now passed through as itself.

- **`Length` counted bytes while every one of its messages said "characters" (#720).**
  Four evaluators of the rule — the standalone validator, the composite evaluator, the
  custom-scalar registry and the runtime input validator — each used `str::len()`, so
  `min: 3` accepted `"é!"` (two characters) and `max: 5` rejected `"éàü"` (three). All four
  now route through one `check_length`, driven by the same multi-byte corpus in a test that
  calls each of them.

- **Every MCP tool call failed under `naming_convention = "camelCase"` (#857).** Tools
  were advertised under `schema.display_name(&q.name)` while `call_tool` looked the
  operation up by the raw compiled name, so a schema authored `list_users` advertised
  `listUsers` and then answered `Unknown operation: listUsers` — with camelCase being the
  compiler default since #456, the entire MCP surface was unusable while `tools/list`
  reported it as available. Advertisement and execution now share one identifier by
  construction. A test parameterised over every naming convention lists the tools, calls
  each by its advertised name, and asserts the read reached its view.

- **MCP tools advertise the arguments they actually accept.** The advertised input schema
  was built from `QueryDefinition::arguments`, which excludes the auto-wired
  `where`/`orderBy`/`limit`/`offset` parameters, so a query with `auto_params` enabled
  advertised none of them. Both the advertisement and the executor's argument validation
  now read `graphql_arguments()`.

- **The MCP test binaries run in CI.** `mcp_integration_test` and `mcp_e2e_test` ran in
  `feature-flags.yml`'s `feature-integration-tests` job, which has been dispatch-only
  since the Dagger migration on 2026-05-31; the Dagger `test` leg runs
  `cargo test -p fraiseql-server --lib`, which does not reach `tests/*`. No leg executed
  them, which is how #857 — total breakage under the default configuration — went
  unnoticed. They are now named explicitly in the `test` leg, alongside the new
  `mcp_transport_safety_test`, and the two-tenant `mcp_tenant_dispatch_e2e_pg` suite runs
  in the `integration: server` leg against a real database.

- **Azure Blob rejected ordinary filenames (#876).** The object key was interpolated raw
  into both the blob URL and the `SharedKey` string-to-sign, so the URL parser rewrote one
  of them and not the other: `#` began a fragment and silently truncated the request path,
  `?` began a query, and `%41` was decoded to `A`. Every key containing one of those
  characters failed with a `403 AuthenticationFailed` surfaced as a `500`, pointing at
  credentials rather than at the key. Both are now percent-encoded per path segment, and
  an Azurite round-trip covers `#`, `?`, `%` and spaces.

- **The bucket MIME allow-list ignores `Content-Type` parameters (#876).** `text/plain`
  rejected the browser-standard `text/plain;charset=UTF-8`. Matching is now on the media
  type alone and case-insensitive. The two enforcement points that had drifted — the
  upload handler honoured `image/*` wildcards but misread an empty list, `BucketService`
  read the empty list correctly but ignored wildcards — are now one method on
  `BucketConfig`.

- **`docs/architecture/storage.md` describes the system that exists (#867).** It
  documented three endpoints that 404, a TOML example that fails to deserialize at boot,
  "per-tenant isolation" from an evaluator with no tenant concept, and EXIF stripping that
  no code performs. Rewritten against the router, the config type and the access rules,
  with the GCS/Azure presign and list gaps marked and the transform gap recorded as #901.

- **The storage metadata suite ran against a hand-copied schema**, not the shipped
  migration, so a column added to the DDL could not redden it. It now executes
  `storage_migration_sql()`.

### Security

- **Every way of constructing the runtime now produces the same configured runtime
  (#750, #754, #783).** `Server::new`, `with_relay_pagination`, `with_flight_service` and
  the hot-reload rebuild were four construction paths, and each one that was not
  `Server::new` had drifted. `with_flight_service` did not call the shared
  `from_executor` at all — it hand-built the `Server` struct literal, which is why its
  OIDC block was `#[cfg(feature = "auth")]`-gated where the others were not: a
  `--no-default-features --features cli,arrow` build discarded a configured `[auth]`
  block and served every request as an anonymous principal, with the same signature and
  no warning.

  Concretely dropped, and now carried on every path: the compiled `[subscriptions]`
  block, whose `on_connect`/`on_subscribe` webhooks are *documented fail-closed
  authorization gates* — losing them left a `NoopLifecycle` that accepts every
  WebSocket connection, and `main.rs` selects `with_relay_pagination` for any schema
  with a single relay query and `with_flight_service` for every Arrow build, so the
  guards were off in both shipped binaries; the per-connection subscription limit
  (unbounded fan-out); `[pool_tuning]`; and the OIDC validator that `main.rs`'s Flight
  service needs — the Flight handshake is fail-closed on a missing validator, so the
  entire Arrow Flight surface answered `Authentication not configured` no matter how
  `[auth]` was set, pointing at `FLIGHT_OIDC_*` environment variables that do not exist
  anywhere in the workspace.

  The prologue is now one `schema_subsystems()`, the assembly one `from_executor()`, and
  the epilogue one `apply_compiled_config()`. A construction-parity matrix builds every
  path from one fully-populated compiled schema and asserts the same properties on each —
  including that the fail-closed `on_connect` hook actually *rejects*, rather than that
  the lifecycle has the right type name.

- **A schema hot-reload no longer silently reverts the runtime's security settings
  (#750, #782).** Both reload entry points — `SIGUSR1` and
  `POST /api/v1/admin/reload-schema` — rebuilt the executor with
  `RuntimeConfig::default()`, so a reload advertised as zero-downtime turned off mutation
  audit logging in a compliance deployment, reset the #421 page-size ceiling from a
  configured 100 back to 1000, re-enabled the change-log outbox write against a table the
  operator had deliberately not installed, and dropped relay dispatch so every relay
  query failed validation until the process restarted. It also ran none of the boot-time
  safety gates, so a reload could move a running server into a state boot refuses: a
  field marked for at-rest encryption (whose write path stores plaintext), or a
  multi-tenant schema with caching and no RLS.

  Reload now re-derives the schema-owned settings on top of the live config, rebuilds
  through the same constructor that booted the server, and runs both boot gates. Where it
  *cannot* apply a change it refuses and says which section needs a restart, rather than
  reporting success and serving the previous configuration.

- **A configured-but-unavailable Redis token-revocation store refuses to boot in
  production.** Both routes to the fallback — the `redis-rate-limiting` feature not
  compiled in, and the Redis connection failing — logged a warning and downgraded to a
  per-process in-memory store. That is not a degraded revocation service but an absent
  one: a token revoked on one replica stays valid on every other replica for its full
  lifetime while the admin API reports success. Development still boots on the fallback.

- **Client-supplied data no longer reaches the log on a GET parse failure (#730).** A
  malformed `variables` query parameter was logged in full at `warn!` — up to
  `max_get_query_bytes` (100 KiB by default) of client-controlled text, which is where a
  bearer token or PII ends up, in every log sink the deployment ships to. The parse error
  and the byte length are logged instead.


- **Database TLS is now a property of the connection rather than a log line
  (#801, #824).** `build_pool` called `create_pool(.., NoTls)` unconditionally. Both
  configuration surfaces that claimed to control database TLS — the server's
  `[database_tls] postgres_ssl_mode` and the CLI's `[database] ssl_mode` — were parsed
  and whitelist-validated, so a typo failed loudly and convinced the operator the
  setting was live; then they were discarded. `serve()` printed
  `postgres_ssl_mode=verify-full … Database connection TLS configuration applied` over a
  pool that had been built in cleartext several hundred lines earlier. An operator had
  documented, validated, log-confirmed evidence that their database traffic, including
  the connection password, was encrypted, and it was not.

  The pool now takes a required `PostgresTlsConfig` (a rustls connector via
  `tokio-postgres-rustls`), so every site that opens a pool has to state its transport
  security and a new one cannot compile without deciding. `require` refuses a server
  that cannot encrypt; `verify-full` refuses a certificate it cannot chain to a trusted
  root and accepts it once `ca_bundle_path` supplies the CA. Two suites assert this
  against real PostgreSQL — one without TLS, one with — and read `pg_stat_ssl` back out
  of the server rather than inferring encryption from the connection having succeeded.

  Per-tenant pools inherit the server's setting through `make_executor_factory`;
  `TenantPoolConfig.tls` is `#[serde(skip)]` like `search_path`, so a tenant-registration
  request body cannot downgrade its own tenant to cleartext.

  The false "applied" log line is gone; the pool reports its own mode from the site that
  builds it, and `require` (which encrypts without authenticating the peer) warns at boot
  and names `verify-full`.

- **The `#618` proxy-trust boot guard can no longer be reached around (#837, #778).**
  It ran only inside the compiled-schema branch, so the same configuration expressed as
  `[rate_limiting]` in `fraiseql.toml` reached `RateLimiter::new` with no gate:
  `trust_proxy_headers = true` with an empty `trusted_proxy_cidrs` booted happily in
  production and honoured `X-Real-IP` from every peer, defeating per-IP rate limiting.
  Separately, the compiled section was deserialized with `.ok()`, so one wrong-typed
  field made it vanish — turning rate limiting off *and* skipping the guards behind the
  parse — while the operator had a successful compile as evidence it was in effect.

  Both constructors now resolve the limiter through one function that runs the guards on
  whatever configuration actually takes effect, and a malformed section refuses to boot.

  A third bypass was found while fixing these and is closed too: `trusted_proxy_cidrs`
  entries that failed to parse were dropped with a warning *after* the guard inspected
  the string list, so `["10.0.0.0/8typo"]` passed the non-empty check and produced an
  empty trust list — the trust-everyone posture the guard exists to refuse. An
  unparseable entry is now a boot error naming it.

- **`[fraiseql.security]` sub-sections reached their consumers (#893).**
  `SecurityConfig::to_json` emitted `auditLogging` / `errorSanitization` /
  `rateLimiting` / `stateEncryption` while the server read snake_case and the other
  compile path (`schema/merger.rs`) emitted snake_case — so which keys survived depended
  on which producer ran, and `[fraiseql.security.rate_limiting]` from a project TOML
  reached nothing at all: no limiter was mounted and the proxy-trust guard behind it
  never ran. All producers and consumers now use one spelling, and the rate-limit
  section is emitted in the flat shape the consumer deserializes.

  `RateLimitingSecurityConfig` gained a real `Default`: it derived one, so a producer
  omitting `requests_per_second` yielded a limiter with a budget of zero that denies
  every request. A limiter enabled with a zero budget is now refused at boot.

- **`fraiseql analyze` no longer fabricates a security attestation (#818).** It read the
  schema, discarded it, and emitted a fixed list stating that rate limiting was
  "configured and active" and audit logging "enabled for compliance", with
  `health_score` pinned at 100 for every possible input — an empty `{}` and a schema
  explicitly disabling both controls produced byte-identical output. The report is now
  computed from the schema, in the `{category, severity, message, suggestion}` shape the
  published `--show-output-schema analyze` contract already described but the command
  never emitted.

### Fixed

- **Usage counters survive a failed startup load, and multiple replicas sum (#861).**
  `PostgresBackend::flush` wrote `SET count = EXCLUDED.count` — an absolute overwrite —
  while a failed startup load was a `warn!`-and-continue that still armed the periodic
  flush. A transient database fault in the window between the schema DDL and the
  restoring `SELECT` therefore destroyed the month's accumulated per-tenant counters at
  the next tick: a row holding 41 300 became whatever the fresh process had counted since
  boot, permanently, with no error surfaced and billing reporting off it. The same
  absolute write made the shipped three-replica manifests last-writer-wins rather than
  summed, silently discarding most of every interval.

  The flush is now additive — renamed `UsageBackend::flush` → `flush_deltas` so no
  implementation could keep the old semantics by accident — carrying the increment since
  the last *confirmed* write, with the per-key watermark advanced only after the backend
  acknowledges. A failed startup load disarms persistence for the process: the aggregator
  refuses to flush and the server reverts to the in-memory backend rather than writing to
  a store it could not read.

- **Seven server edge cases (#731).** The GET size ceiling returns the `413 Payload Too
  Large` its published `# Errors` contract documents, instead of 400. A server-side
  execution timeout maps to `504 Gateway Timeout`; 408 means the *client* took too long to
  send its request, which tells a caller to retry the wrong thing. REST path parameters
  are coerced by the argument type the schema declares rather than by what they look
  like — the string ID `"0123"` used to become the integer `123` and `"true"` a boolean,
  so the row a client addressed and the row the server updated could differ. The
  database-URL guard rejects a scheme-less string: `"postgres"` has no `://`, so
  `split("://").next()` returned the whole string and it was accepted as a valid
  PostgreSQL URL, surfacing later as exactly the opaque driver error the guard exists to
  replace. The trusted-documents manifest poller builds its HTTP client once instead of
  per tick, and enforces its 10 MiB cap while streaming rather than after buffering the
  whole body. The admin brute-force limiter evicts expired per-IP records, which were only
  ever removed on a *successful* authentication — an unbounded map any unauthenticated
  caller could grow. The manifest SSRF item was already closed by the guard unification
  earlier in this release.


- **`[admission_control]` enforces admission (#860).** The controller was constructed,
  inserted into the request extension map — which stores a value and gates nothing — and
  announced in the boot log as "enabled", while `ServerConfig::admission_control`'s
  documentation promised that over-limit requests "receive 503 Service Unavailable
  immediately". Every `try_acquire` caller in the workspace was a test. It is now a real
  middleware holding a permit for the duration of the request. `try_acquire` also
  incremented `queue_depth` on the reject path and never decremented it, so once wired it
  would have ratcheted to permanent rejection after `max_queue_depth` cumulative misses;
  a non-blocking miss enters no queue and is no longer counted.

- **`fraiseql run` sub-second `connect_timeout_ms` no longer breaks every connection
  (#824).** `pool_timeout_secs: connect_timeout_ms / 1000` truncated `1..=999` to `0`,
  which deadpool applies to its `create` slot as `Duration::ZERO`; a TCP connect plus the
  PostgreSQL handshake cannot finish on the first poll, so every attempt failed with a
  pool timeout against a healthy server, naming a timeout the operator never configured.

- **Six `ServerConfig` leaves had real consumers but no manifest entry (#883)**, leaving
  `config_coverage_manifest_test` red on `dev` for long enough that four remediation
  phases recorded it as a pre-existing failure. Registered, so the gate protects new keys
  again.

### Breaking

- `ErrorCode::Timeout` now maps to **504 Gateway Timeout** (was 408 Request Timeout), and
  the GET size ceilings return the new `ErrorCode::PayloadTooLarge` → **413** (was 400 via
  `RequestError`). Clients branching on those statuses need updating.
- `UsageBackend::flush` is renamed **`flush_deltas`** and its contract inverted: the map
  now carries increments to be **added**, not absolute totals to be written. Any external
  implementation must be updated — the rename is deliberate so it cannot compile
  unchanged. `UsageAggregator::flush_to_backend` also now *errors* when the backend's
  startup load failed.
- A schema hot-reload **refuses** a schema whose boot-frozen configuration differs from
  the running one — `[security]`, `[validation]`, `[subscriptions]`, `[mcp]`, `[rest]`,
  `[grpc]`, federation, observers, sources, `[fraiseql.naming]`, `[debug]`, fact tables
  and per-query `cache_ttl_seconds`. These are read once by subsystems that are immutable
  afterwards, so the previous behaviour was to report success and keep serving the old
  configuration. Reloads that change only types, queries, mutations or session variables
  are unaffected; the rest now need a restart, and the refusal says which section.
- `AppState::with_reload_config` takes a third argument, the executor rebuilder recorded
  by the booting constructor. An `AppState` assembled directly (without a `Server`)
  refuses to reload rather than guessing how to rebuild.


- `[database_tls]`: `redis_ssl`, `clickhouse_https` and `elasticsearch_https` are
  **removed**. They only ever rewrote a URL scheme, in a helper with no production
  caller. A config still setting one is refused with a message naming the replacement
  (put `rediss://` / `https://` in the URL, which is what the client library reads) —
  refused rather than dropped, because an unknown key in that struct is discarded
  silently.
- `postgres_ssl_mode` / `[database] ssl_mode`: libpq's `allow` and `verify-ca` are
  **refused** rather than approximated. `allow` has no expression in the driver, and
  `verify-ca` would need a bespoke verifier whose only purpose is to check less than the
  default. Each error names the mode to use instead.
- `postgres_ssl_mode` and `[database] ssl_mode` are now **unset by default** rather than
  `"prefer"`. Unset means "whatever `?sslmode=` in the connection URL says"; a concrete
  default would override an operator's explicit `?sslmode=require` with a value they
  never wrote.
- `[security.constant_time]` is **refused**. Constant-time comparison is applied
  unconditionally, so the toggles switched nothing — and one key inside was misspelled
  `applytoCsrfTokens`, which nothing noticed because nothing read it.
- `[security.rate_limiting] failed_login_max_attempts` / `failed_login_lockout_secs`
  defaults change from 5 / 3600 to 10 / 900, matching the runtime's. The old values read
  as deliberately tuned, and now that this section actually reaches the runtime, a tuned
  value refuses to boot in production (#356).
- `fraiseql analyze` output shape changed from `categories` (a map of constant strings)
  to `recommendations` — the shape its published machine contract already documented.

### Security

- **The authorization and administration surfaces now do what they report doing
  (#748, #749, #768, #769, #757, #677).** An operator could revoke a session, set a
  secret, invite a user, read an audit trail and grant a field-level role — and none of
  those operations happened, while all of them reported success.

  - **#748 — the RBAC schema DDL was a PostgreSQL parse error, so setting `admin_token`
    bricked boot.** `ensure_schema` put `UNIQUE(name, COALESCE(tenant_id, …))` inside a
    `CREATE TABLE`; a table-level `UNIQUE` constraint accepts column names, never
    expressions. Boot runs that DDL unconditionally whenever `admin_token` is set, so the
    shipped `-full` binary with a PostgreSQL `database_url` and the documented admin token
    exited at startup — and the RBAC management API had therefore never executed a single
    statement against a real database. Per-tenant name uniqueness is now a unique index
    over the expression (the `COALESCE` is load-bearing: a plain `UNIQUE (name, tenant_id)`
    would let two identically-named global roles coexist, because NULLs compare distinct).

    Its four test files were ~90 `#[test]` functions with **empty bodies**, which is why
    `cargo test` was green throughout; they are deleted rather than extended.

  - **#749 — five Studio admin write endpoints returned fabricated success.**
    `POST /admin/v1/users/{id}/revoke` answered `{"success": true, "message": "All sessions
    revoked"}` without touching any revocation store, so during an account-compromise
    response the attacker's tokens kept validating. Function-secret set/delete, row mutate
    and user invite did the same, and six read endpoints answered a hard-coded empty
    collection — which asserts "there are none" rather than "this is not wired". Revocation
    is now performed for real (the `TokenRevocationManager` is reachable from `AppState`);
    everything else answers `501` naming the missing subsystem.

  - **#768 — `GET /api/audit/permissions` was a façade.** It ignored its documented
    parameters and returned a hard-coded `[]` while no handler recorded anything, so a
    compliance reviewer was told no permission changes had occurred regardless of activity.
    Every mutating store method now writes an audit row **inside its own transaction**, so
    a recorded event cannot outlive a rolled-back change and a committed change cannot go
    unrecorded, and the endpoint reads them back with its filters applied.

  - **#769 — tenant scoping was inert and role listing truncated silently.** Every handler
    passed `None` for tenant and `list_roles` hard-coded `limit 100, offset 0` with no
    query parameters, so the 101st role was unreachable with no indication and
    `RoleDto.tenant_id` was always null. `create_role` mapped *every* failure — a malformed
    permission string and an unreachable database alike — to `409 role_duplicate`.

  - **#757 — `[fraiseql.security]` role grants evaporated between compiler and runtime.**
    The compiler emitted `roleDefinitions`/`defaultRole`/`tenantClaim`; the runtime
    declares `role_definitions`/`default_role`/`tenant_claim` with a `#[serde(flatten)]`
    catch-all, so the keys landed in the untyped map and the typed fields kept their
    defaults. Since `role_has_scope` is the only input to `can_access_scope`, **field-level
    RBAC was deny-all on every project-TOML compile**: a member of a role granted a scope
    was refused the field the role existed to unlock. `tenant_claim` disagreed in the other
    direction — compile-time `@tenant_id` validation read the camelCase key while the
    runtime read the snake_case one.

  - **#677 — type-level `requires_role` was documented as an access gate and enforced
    nowhere.** It had two non-test readers, neither an execution gate; all five real gates
    read the *operation*'s role and nothing seeded it from the returned type. The
    repository's own golden fixture shows the hole: type `Order` and query `orders` both
    carry `"admin"`, and `orderSummary` — which also returns `Order` — carries none. A
    type's role is now lowered onto the operations returning it when the compiled schema
    loads, so the existing gates enforce it with no sixth check to keep in step.

### Breaking

- **RBAC list endpoints return a page envelope, not a bare array** (#769).
  `GET /api/roles`, `/api/permissions` and `/api/user-roles` now answer
  `{"items": [...], "total": N, "limit": N, "offset": N, "has_more": bool}` and accept
  `limit` (default 100, max 1000), `offset` and — where the resource is tenant-scoped —
  `tenant_id`. Unknown query parameters are refused rather than ignored, so a mistyped
  `tenant_id` cannot silently widen a read. `GET /api/user-roles` now **requires**
  `user_id`; omitting it used to answer `200 []`, indistinguishable from "this user holds
  no roles". The RBAC API could never have been used before this release — its tables
  could not be created (#748) — so there are no existing consumers.

- **`POST /api/roles` and `POST /api/user-roles` refuse unknown body fields** (#769), and
  accept an explicit `tenant_id`. A misspelled `tenantId` used to be silently dropped,
  creating a *global* role while the caller believed it was tenant-scoped.

- **Studio admin endpoints that perform no operation answer `501`** (#749) instead of
  `{"success": true}` or an empty collection: `/admin/v1/users`, `/admin/v1/users/invite`,
  `/admin/v1/data/{entity}/query`, `/admin/v1/data/{entity}/mutate`,
  `/admin/v1/storage/buckets`, `/admin/v1/storage/objects`, `/admin/v1/functions`,
  `/admin/v1/functions/{name}/logs` and the function-secret routes. The response carries
  `{"error": "not_implemented", "feature": "...", "message": "..."}`.

- **`GET /admin/v1/health/detailed` and `/admin/v1/metrics/summary` report `null` for
  figures they cannot measure** (#749), where they previously reported `0`. A zero pool
  size reads as an exhausted pool and a zero hit rate as a cache that never hits.
  `uptime_secs` was `SystemTime::now() - UNIX_EPOCH` — the current Unix timestamp — so a
  four-second-old server claimed ~1.8 billion seconds of uptime; it is now time since
  boot. `errors.rate_5m`/`rate_1h`/`rate_24h` were three copies of the lifetime ratio
  under three window names; the lifetime value moved to `errors.lifetime` and the windows
  report `null` until windowed counters exist.

- **`[fraiseql.security]` compiles `role_definitions`, `default_role` and
  `tenant_claim` under those names** (#757), replacing `roleDefinitions`, `defaultRole`
  and `tenantClaim`. Recompile; no runtime consumer ever read the old spellings.

- **A schema whose type-level `requires_role` cannot be enforced is refused at load**
  (#677). Two shapes: an operation whose own role disagrees with its return type's (both
  are required, and a compiled operation carries only one role), and a gated type
  reachable as a field of a type that is not gated the same way (operations returning the
  container carry no role, so the gated type travels out ungated). Subscriptions carry no
  role gate at all, so a subscription returning a gated type is refused.

### Security

- **Tenant isolation is now an enforced property, not a documented intention (#809, #859,
  #758, #762).** Four mechanisms were supposed to keep tenants apart. Three did not run
  and the fourth passed vacuously.

  - **#809 — schema-per-tenant isolation applied to one connection out of N.** Tenant
    registration issued a single session-scoped `SET search_path TO tenant_x, public`
    through `execute_raw_query`, which borrows one connection from the pool and returns
    it. `RecyclingMethod::Fast` performs no `DISCARD ALL` and no `post_create` hook
    existed, so every other connection the pool opened kept the server default and
    resolved unqualified relations against `public` — silently wrong rows where `public`
    shadowed the relation, an intermittent `relation … does not exist` where it did not,
    and **zero** correct connections after any backend restart. `TenancyMode::Schema`'s own
    doc claimed the path was set "on connection acquisition".

    The search path is now a property of the pool, lowered into the PostgreSQL startup
    `options` parameter (`SearchPath` in `fraiseql-db`), so the server applies it while
    establishing *every* connection, including replacements — and `RESET`/`DISCARD ALL`
    restore it rather than clearing it. Registration then verifies the isolation actually
    took by reading `pg_settings.reset_val` (the *established* value, which a session `SET`
    cannot fake) and refuses to register a tenant whose connections would serve `public`.

  - **#859 — `DELETE /api/v1/admin/tenants/{key}` reported "removed" over surviving data.**
    The handler dropped the registry entry and recorded a `Deleted` audit event;
    `destroy_tenant_schema`, whose doc comment claimed the handler called it, had no callers
    anywhere in the workspace. Because provisioning is `CREATE SCHEMA IF NOT EXISTS`,
    registering a new tenant under a recycled key adopted the previous tenant's schema.
    Deletion now reports `schema_retained` by default and takes `?purge=true` to drop the
    schema — resolved through the tenant's own adapter *before* deregistration, so a failed
    drop leaves the tenant registered rather than answering success over live data.
    Registration additionally warns when it adopts a schema that already holds relations.

  - **#758 — `security.multi_tenant` had no producer.** `is_multi_tenant()` read
    `security.multi_tenant`; both TOML security structs are `deny_unknown_fields` and no
    SDK emitted the key, so the flag was false for every schema any supported workflow
    could produce — and the two gates that depend on it (the subscription tenant
    fail-closed gate, the cache+RLS boot gate) were permanently inert, while the boot
    gate's own error text told operators to set it. Both TOML formats now accept
    `multi_tenant`, and a non-`none` `[tenancy] mode` implies it.

  - **#762 — the RLS gate checked a GUC that is on by default.** `validate_rls_active`
    read `current_setting('row_security')`, which governs whether *existing* policies apply
    and says nothing about whether any policy exists — so `RlsEnforcement::Error`, the
    documented "safest" setting, approved caching on a database with no RLS at all. It now
    inspects each of the schema's source relations: a table must have `relrowsecurity` and
    at least one `pg_policy` row; a view must be `security_invoker`, because a default view
    runs with its owner's privileges and bypasses the caller's policies entirely. A missing
    relation fails the check.

  - **The boot gate now exists once and runs everywhere.** The cache+RLS check was inlined
    in `Server::new` and `with_relay_pagination` and absent from `with_flight_service`;
    the drift was invisible only because the flag it read could never be true. It is now
    `tenant_isolation_declaration_check`, called by each constructor, and a multi-tenant
    schema that declares RLS has the declaration verified against the live catalog at boot.

- **Closed the unauthenticated REST read surface (#812, #739, #810).** The REST transport
  served every row of every tenant to any caller, by three independent routes, none of
  which the existing REST suite could see.

  - **#812 — REST was mounted with no authentication at all.** Every auth middleware in
    the crate is attached with `route_layer` on a specific sub-router, and axum's
    `route_layer` does not survive `Router::merge`. `mount_extensions` merged the REST
    router without one, so `security_context` was `None` on every `/rest/v1/**` request —
    disabling RLS-policy evaluation and the `SET LOCAL app.tenant_id` session stamp — even
    when the caller presented a valid bearer token. The router's own module doc asserted
    the opposite. `/graphql` and REST now acquire authentication from one shared
    `Server::attach_auth`, so the two cannot drift again.

  - **#739 — the resolved tenant filter was thrown away.** `execute_query_direct` — the
    runner behind the whole REST read surface, including CSV/XLSX/NDJSON export and
    resource embedding — resolved each `inject_param` into `let _value = …` and never
    composed it, under a comment claiming the params were applied via WHERE clauses. Every
    sibling path (`count_rows`, `execute_regular_query_with_security`, the relay runners)
    composed them correctly. The visible symptom shipped for releases: a
    `Prefer: count=exact` response whose `X-Total-Count` was tenant-filtered while its body
    was not. Inject params with no security context now fail closed, matching `count_rows`.

  - **#810 — `require_auth` was honoured by one route in six.** Only the SSE handler read
    it; GET/POST/PUT/PATCH/DELETE never did, while the served OpenAPI advertised
    `BearerAuth` and a documented 401 on every operation — so an operator who set the flag,
    read back the machine-readable contract, and shipped, shipped an open endpoint. The
    check now runs during security-context *extraction*, which a handler cannot skip, and
    the OpenAPI document derives its security advertisement from what is actually enforced.
    Three operation builders (the `openapi.json` meta entry, the SSE endpoint, and the two
    bulk operations) hand-built their JSON and never consulted the security helper; all four
    now route through one `apply_security`.

- **The compiler no longer discards author-declared security controls (#806, #807).** Two
  authorization controls were lost at the SDK → compiler seam, both by key drift, both
  under `✓ Schema compiled successfully`.

  - **#806 — server-side parameter injection.** TypeScript, Go and Java emit
    `inject_params`; the compiler read `inject`. With `#[serde(default)]` and no
    `deny_unknown_fields` the map became an empty default, so every query and mutation
    those SDKs produced compiled with **no tenant predicate**. The root cause was two names
    for one concept — `inject` in the intermediate format, `inject_params` in the compiled
    schema — so an SDK author reading a compiled artifact to learn the name got it wrong.
    The names are now the same.

  - **#807 — field-level scopes.** Go, C# and F# emit `scope`/`scopes`, the Rust authoring
    SDK emits `requiresScope`, Java and Elixir emit the plural `requires_scopes`; the
    compiler reads `requires_scope`. The compiled field carried `requires_scope: None`,
    which the runtime field filter treats as public and always accessible — on a PII column
    the author gated and the SDK validated the grammar of. PHP dropped the scope one layer
    earlier, in its exporter, with the same outcome.

  Both are now refused at compile time rather than aliased: an alias would keep six
  spellings working and leave the seventh SDK free to invent a seventh. The guard runs on
  the raw JSON at **both** deserialization sites — the JSON workflow and the TOML /
  multi-file merger — because guarding only one would have left every `--schema-dir` user
  with the original silent drop.

### Breaking

- **`[security.rls]` is the RLS declaration; `security.policies` no longer implies it.**
  `has_rls_configured()` counted `security.additional["policies"]` — *authorization*
  policies, a section #612 made a hard compile error — so it answered `false` for every
  producible schema. Declare `[security.rls] enabled = true` (or
  `[fraiseql.security.rls]`) to state that database RLS isolates the deployment. With
  `multi_tenant` also set, the server verifies the claim against the live catalog at boot
  and refuses to start when it is not true.

- **`[security] multi_tenant` and `[session_variables]` are declarable in TOML.**
  `multi_tenant` was rejected as an unknown field by both TOML security structs.
  `[session_variables]` had no TOML producer at all, though the compiled field documented
  itself as "compiled from the `[session_variables]` TOML section" — the only way to
  declare the mechanism RLS policies read was to hand-author `schema.json`.

- **A session-variable mapping is one flat table.** `SessionVariableMapping` now flattens
  its source, so a mapping is `{name, source, claim}` in JSON and

  ```toml
  [[session_variables.variables]]
  name = "app.tenant_id"
  source = "jwt"
  claim = "tenant_id"
  ```

  in TOML — against the same type the runtime consumes, with no CLI-side mirror struct to
  drift. No SDK emitted `session_variables`, so nothing in the wild produced the old
  nested shape.

- **`CachedDatabaseAdapter::validate_rls_active` and `enforce_rls` take the compiled
  schema.** They need the relation list to check anything; the previous signatures could
  only read a GUC (#762).

- **`PoolPrewarmConfig` carries a `search_path`.** Every pool construction site must now
  state whether its connections are schema-isolated. `PostgresAdapter::new` and
  `with_pool_size` are unchanged.

- **`DELETE /api/v1/admin/tenants/{key}` reports what it did.** `status` is now
  `removed_schema_retained` or `removed_and_purged` rather than `removed`, with
  `schema_retained` / `schema_dropped` naming the schema (#859).

- **`max_storage_bytes` is renamed `max_storage_bytes_advisory`** (#633). Nothing meters
  per-tenant storage, so nothing was ever rejected on the basis of this value; a field
  called `max_storage_bytes` reads as a boundary that does not exist. The registration
  body is now `deny_unknown_fields`, so the old key is a 400 rather than a silently
  ignored setting. `TenantExecutorRegistry::is_quota_exceeded` / `set_quota_exceeded` are
  removed — a public quota API with no producer on either side reads as an enforced limit
  to anyone who greps for one. Metering remains tracked at #633.

- **`examples/saas` declares queries only.** Its eight mutations named no input type and
  no backing SQL function; the compiler accepted them and none could ever execute. See
  `examples/mutation-patterns` for the mutation story.

- **The intermediate-schema injection key is `inject_params`, not `inject`** (#806). The
  value may be either `"jwt:<claim>"` or `{"source": "jwt", "claim": "<claim>"}`. A schema
  using `inject` is now **refused** with a message naming the replacement, rather than
  compiling to a query with no injected filter. The Python decorator's `inject=` argument
  is unchanged; only the emitted JSON key moved.

- **Field scopes must be declared as `requires_scope`** (#807). `scope`, `scopes`,
  `requiresScope`, `requiresScopes` and `requires_scopes` are refused with a message naming
  the replacement. The Go, C#, F#, Rust, PHP and Java SDKs now emit the canonical key.

- **Multiple required scopes on one field are unsupported and now say so.** The compiled
  schema and the runtime field filter represent exactly one `requires_scope`; a multi-scope
  declaration compiled to a field with *no* scope. The SDKs refuse it at authoring time. A
  singleton list is normalised to a single scope.

- **`require_auth = true` now applies to every REST route, including
  `{base}/openapi.json`** (#810). A surface closed to anonymous callers no longer hands
  those callers a full description of its resources, fields and filters.

- **`rest_query_router` and `rest_router` take an `auth_layer_attached` argument** (#810),
  and `generate_openapi` takes it too, so the served document reflects the deployment's
  actual authentication rather than a static template.

- **Unified every outbound-address guard and every production check onto one
  implementation (#802, #836, #816, #725, #882).** The workspace carried **eight**
  hand-rolled SSRF address predicates and **two** production detectors. Each was
  individually reasonable; collectively they disagreed, and the gaps between them were
  exploitable.

  - **#802 — `IPv4`-mapped `IPv6` bypassed the serverless-function HTTP guard.** Its
    `IPv6` arm tested `is_loopback`/`is_unique_local`/`is_unicast_link_local`, none of
    which fire for `::ffff:169.254.169.254`, so a guest function could reach cloud
    instance metadata over a dual-stack socket — via a bracketed literal, or via an
    allowlisted hostname with an attacker-controlled AAAA record, which is precisely the
    rebinding attack the surrounding code claimed to close. Five of the eight predicates
    shared this gap; it is the same defect as #776 in a different crate.

  - **#836 — the SSRF bypass was honoured in production.** `ServerConfig::is_production_mode()`
    treated an unset `FRAISEQL_ENV` as production, and every server safety gate is keyed
    off it. `observers::insecure_guard::is_production_environment()` read the same variable
    and treated unset as **not** production. On any non-Kubernetes deployment — Docker
    Compose, systemd, a VM, ECS — the server therefore believed it was in production while
    the observer subsystem honoured `FRAISEQL_OBSERVERS_ALLOW_INSECURE`, disabling the
    scheme allow-list, the private-address blocklist and the rebinding defence on a webhook
    URL that comes from a mutable `tb_observer` row.

  - **#882 — two escape hatches had no production check at all.**
    `FRAISEQL_VAULT_ALLOW_INSECURE` and `FRAISEQL_OIDC_ALLOW_INSECURE` disabled their SSRF
    guards on the environment variable alone, under every environment including an explicit
    `FRAISEQL_ENV=production` and inside a Kubernetes pod. All four of the product's escape
    hatches now share one policy: honoured only when development is positively declared.

  - **#816 — the CDC NATS plaintext guard was inverted.** It refused plaintext `nats://`
    only for loopback hosts — the one case that is safe — and accepted every remote
    plaintext endpoint, publishing full row after-images in the clear. It also skipped
    every non-`nats://` URL including the scheme-less form that `async-nats` rewrites to
    plaintext, split the host with `split(['/', ':'])` so `nats://user:pw@host` yielded
    `"user"`, and compared the host without lower-casing it. It had no unit tests.

  - A **ninth** hand-rolled guard, on the manifest hot-reload URL, was found by the new
    gate rather than by review. Its doc comment claimed it used "the same pattern as the
    federation and Vault SSRF guards"; it had drifted from both.

  The shared guard additionally blocks ranges no previous copy covered: the NAT64
  well-known prefix `64:ff9b::/96` (a live route to the metadata service wherever a NAT64
  gateway exists), NAT64 local-use `64:ff9b:1::/48`, `IPv4`-compatible `::a.b.c.d`,
  multicast, site-local `fec0::/10`, discard-only `100::/64`, IETF protocol assignments
  `192.0.0.0/24` (Oracle Cloud metadata), the RFC 5737 documentation ranges, RFC 2544
  benchmarking, and the `2001:db8::/32` and `2001:2::/48` `IPv6` equivalents.

  `make lint-guard-parity` now fails the build on a new hand-rolled address predicate, a
  new `is_production`-shaped helper, or an escape hatch read without a posture check. It
  runs in the Dagger `preflight` leg and as the `guard-parity-check` CI job.

### Breaking

- **New crate `fraiseql-guard`.** Holds the workspace's single outbound-address guard
  (`fraiseql_guard::net`) and its single production detector
  (`fraiseql_guard::deployment`). It is a Tier-1 leaf with no dependencies beyond `std`,
  published before every crate that depends on it.

- **`fraiseql_auth::constant_time::ConstantTimeOps::compare_padded` and
  `compare_jwt_constant` are removed (#725).** They truncated both inputs to `fixed_len`
  before comparing, so `compare_jwt_constant` reported **equality** for any two tokens
  sharing their first 512 bytes — the shape of two JWTs with identical header and payload
  and different signatures, since the signature sits at the end and real tokens exceed
  512 bytes. `"abc"` and `"abc\0"` also compared equal. Nothing on a production path
  called either; the one real comparison uses `ConstantTimeOps::compare`, which is correct
  for values of any length. Callers wanting length hiding should compare digests rather
  than values. `compare`, `compare_str` and `compare_len_safe` are unchanged.

- **Documentation, benchmarking and reserved ranges are now refused by every outbound
  guard.** A URL targeting `192.0.2.0/24`, `198.51.100.0/24`, `203.0.113.0/24`,
  `198.18.0.0/15`, `240.0.0.0/4`, `224.0.0.0/4` or their `IPv6` equivalents is rejected
  where some guards previously allowed it. These are not globally routable; the practical
  impact is on test fixtures that used a documentation address as a stand-in for a public
  one. Conversely, a *mapped public* address such as `::ffff:8.8.8.8` is now allowed
  rather than blanket-refused: mapped and NAT64 addresses are canonicalised and judged as
  the `IPv4` address the stack would route to.

- **`FRAISEQL_NATS_ALLOW_PLAINTEXT` now requires a declared development environment**, in
  both `fraiseql-observers` and `fraiseql-cdc-sinks`, and no longer accepts a remote
  plaintext endpoint at all. The opt-in permits loopback — its purpose is a local dev
  broker — but does not disable the address guard for other hosts.

- **`fraiseql_federation::http_resolver::is_ssrf_blocked_ip` is now a re-export** of
  `fraiseql_guard::net::is_blocked_ip`. The signature is unchanged; the accepted set is
  strictly smaller.

- **CRITICAL: closed two unauthenticated SQL-injection holes on the analytics execution
  path (#794, #795).** Both were reachable by any client able to POST a GraphQL query, on
  any deployment whose compiled schema declares at least one fact table, and both were
  verified against live PostgreSQL 16 exfiltrating `pg_authid` contents.

  - **#794 — window aliases and dimension paths were interpolated raw.** Four sinks on the
    live `*_window` path wrote request-supplied strings straight into the SELECT list: the
    dimension select arm and the `PARTITION BY` arm both built `format!("{}->>'{}'", …)`
    with no charset check, and `alias` was cloned through untouched for measure, dimension,
    filter and window-function selections before being emitted as `<expr> AS <alias>`.
    Because `WindowProjector::project` copies every returned column into the response, an
    injected column was handed back to the caller.

    Every alias and dimension path is now rejected unless it matches
    `[_A-Za-z][_0-9A-Za-z]*`, through a single entry point that all sinks share — the
    defect existed because one arm carried a check its four siblings did not. The
    `WindowAllowlist` is additionally consulted wherever the schema enumerates dimension
    paths. It existed and was documented as the defence for this path, but was only ever
    called by `WindowFunctionPlanner`, which nothing in the shipped binary invokes; the
    live planner is `WindowPlanner`, which never built one.

  - **#795 — the `table` request key selected the FROM target.** The relation is already
    determined by the GraphQL root field (`sales_window` → `tf_sales`), but a second,
    unchecked channel could name any relation or substitute an entire subquery. Worse, the
    RLS policy was looked up by that same attacker-controlled name, so naming a table with
    no configured policy yielded `None` and composed **no tenant WHERE clause at all**.

    Both the aggregate and window planners now reject a `table` that does not match the
    resolved fact table, every FROM sink emits the resolved name, and the RLS policy is
    evaluated against the resolved name — which matters independently, because RLS is
    evaluated before the planner runs.

  Regression coverage runs against real PostgreSQL in the Dagger `integration: server`
  suite (`analytics_injection_e2e_pg`), driving the real HTTP handler and asserting both
  that each payload is refused and that no catalog data reaches the response.

### Breaking

- **`fraiseql run`, `fraiseql validate facts`, and `fraiseql introspect facts` no longer
  collide on `-d` (#650).** The global `--debug` short (`-d`) and each subcommand's
  `--database` short (also `-d`) claimed the same letter, so debug builds of the CLI
  panicked at startup (clap `debug_asserts`: "Short option names must be unique … '-d' is
  in use by both 'database' and 'debug'") and release builds advertised an ambiguous `-d`.
  `--database` is now long-only on all three subcommands — the global `-d` (debug) is
  consistent across every subcommand — and a `Cli::command().debug_assert()` test guards
  against reintroduction. Use `--database <url>` (the long form always worked).

### Fixed

- **The shipped multi-tenant examples now demonstrate isolation they actually have
  (#628).** `examples/multitenant` and `examples/saas` described a tenant-isolation
  mechanism and shipped none of it. Both now carry the whole path: JWT claim →
  `[[session_variables.variables]]` → `set_config` → RLS policy, plus
  `sql/01_schema.sql` with `FORCE ROW LEVEL SECURITY`, `security_invoker` views, and an
  unprivileged application role — because PostgreSQL skips every policy for a superuser or
  `BYPASSRLS` role, which is the most common way a correctly-policied database leaks.
  `example_multitenant_rls_e2e_pg` applies that SQL to a real database, compiles both
  examples through the production compile path, and asserts two tenants never cross and an
  unauthenticated caller sees nothing.

- **Three shipped examples had never been compiled by CI.** `integration_domain_discovery`
  looked its examples up on paths relative to the crate directory rather than the
  repository root, so every case took its `if !path.exists() { return; }` branch and passed
  without doing anything. All three were in fact failing to compile (`pool_size` is not a
  `[database]` key), the multitenant and saas domain files declared queries with
  `return_array` — a key the intermediate schema does not read, so every list query
  compiled as a single-object query — and none declared a `sql_source`. The lookup is
  fixed, a missing example is now a failure rather than a skip, and the examples compile.
  `examples/ecommerce`, removed in `0ef37210c`, is no longer referenced.

- **`cache_rls_isolation_test` had never run either.** It was gated on
  `TEST_DATABASE_URL`, which the `integration: postgres` leg does not set, which is how
  `test_validate_rls_active_fails_without_rls` could ship accepting either outcome ("we
  assert the return type is correct either way"). It now uses the harness's
  `DATABASE_URL`, its fixture view is `security_invoker` (it was not, in the file whose
  subject is tenant isolation), and its isolation assertions connect as an unprivileged
  role and drive real session variables rather than comparing two different WHERE clauses.

## [2.14.1] - 2026-07-24

### Added

- Release: a fully-static `x86_64-unknown-linux-musl` lean binary artifact
  (`fraiseql-x86_64-unknown-linux-musl.tar.gz`) for Alpine / distroless / scratch
  containers.
- Auth: OIDC `[auth]` now supports **issuer-less identity providers** — IdPs whose
  signed access tokens omit the `iss` claim (e.g. self-hosted Hanko 2.x). The
  `issuer` field is now optional, symmetric with `audience`: when it is unset,
  `iss` is not validated and the JWKS endpoint must be pinned via `jwks_uri`
  (discovery is impossible without an issuer). Signature (against the pinned JWKS)
  and the mandatory `audience` check still gate every token. See
  [docs/auth/issuer-less-jwt.md](docs/auth/issuer-less-jwt.md). Previously the
  server refused to start without an `issuer` (`missing field \`issuer\``). The
  `fraiseql compile` CLI's `[auth]` schema now mirrors the **full** `OidcConfig`
  JWT-validation surface (`jwks_uri`, optional `issuer`, `additional_audiences`,
  `allowed_algorithms`, `jwks_cache_ttl_secs`, `clock_skew_secs`, `required`,
  `scope_claim`, `require_jti`, `[auth.me]`), so any `[auth]` block the server
  accepts — including issuer-less — validates under `fraiseql compile`/lint. A
  drift-guard test keeps the CLI and runtime schemas in lockstep.

### Changed

- Auth: when an OIDC `issuer` **is** configured, the `iss` claim is now **required
  and matched** — a token that omits `iss` is rejected. Previously `iss` was only
  checked when present (jsonwebtoken validates the issuer only if the claim
  exists), so an `iss`-less token could slip past a configured issuer. A provider
  that omits `iss` should now leave `issuer` unset and pin `jwks_uri` (see above).

### Fixed

- Release: the Linux `-gnu` binaries are now built with `cargo-zigbuild` against a
  **glibc 2.28 floor**, so they load on Debian 12 (glibc 2.36) and other older
  distributions. Previously the umbrella binary was built on `ubuntu-latest`
  (glibc 2.39) and pulled weak `pidfd_getpid`/`pidfd_spawnp@GLIBC_2.39` symbols that
  made the dynamic loader refuse to start on any glibc < 2.39. A release-time
  `objdump` gate now fails the build if any Linux binary would exceed `GLIBC_2.34`.

## [2.14.0] - 2026-07-22

### Breaking

- **The dormant `/realtime/v1` WebSocket subsystem has been removed in full (#605).** It was a
  second, parallel "entity after-images over WebSocket" mechanism (~3,600 LOC) that duplicated
  the live `/ws` GraphQL-subscription path, with no production `TokenValidator`, no production
  `RlsEvaluator`, and no binary that ever assembled it — security-sensitive dead surface whose
  main risk was a future permissive assembly reintroducing the row-visibility gap #596 closed.
  Everything realtime is gone:
    - the `/realtime/v1` WebSocket endpoint family and the public `fraiseql_server::realtime`
      module;
    - the `POST /realtime/v1/broadcast` channel-broadcast endpoint plus room presence
      (`BroadcastManager`/`PresenceManager`), with their crate API
      `fraiseql_server::subscriptions::{broadcast::*, presence::*}` and the
      `Server::with_broadcast` / `Server::with_presence` builder methods;
    - the four `GET /admin/v1/realtime/{stats,broadcast,presence,cdc}` studio-monitor endpoints
      and the Studio dashboard's "Realtime" panel (placeholder handlers wired to no live state —
      no CDC replication-lag metric ever backed the `cdc` tile);
    - the `RealtimeSubsystem` / `ServerSubsystems.with_realtime` / `Server::with_realtime`
      assembly and the mount glue;
    - the compiled-schema `"realtime"` section (now loaded with a `warn!` and ignored — the load
      never fails; fraiseql-cli never emitted it) and the `[realtime]` `fraiseql.toml` section
      (now silently ignored, where it previously refused to boot with "not yet implemented").

    **The live `/ws` GraphQL-subscription path is unaffected and is the single supported
    real-time mechanism** — it delivers entity change streams, hardened with #596 row-visibility
    and #611 hot-reload, so the entity stream needs no successor. **Broadcast and presence were
    removed *without replacement*** — they were never wired into any production delivery path, so
    `/ws` does not supersede them; if channels-style features (presence, ephemeral pub/sub) are
    ever wanted they are to be designed fresh as `/ws`-native GraphQL subscription fields on the
    hardened auth/policy/protocol machinery, not by reviving this code (which had no working
    delivery or auth to reuse). A revival must rebuild a real `TokenValidator` over
    `OidcValidator`, a production `RlsEvaluator`, and #539 identity plumbing; the PR series
    #664 / #667 / #668 / #669 / #670 / #671 / #672 is the reference for what that would entail.
    Any code calling `with_broadcast`/`with_presence`/`with_realtime` will no longer compile;
    drop the call.
- **In production, `[security.rate_limiting] trust_proxy_headers = true` with an empty or
  omitted `trusted_proxy_cidrs` now refuses to boot (#618).** The 2.13 deprecation warning
  (#609) promised exactly this. Trusting `X-Forwarded-For` from every direct peer lets any
  client spoof its IP and bypass per-IP rate limiting (and poison IP-derived logging).
  Restrict trust with `trusted_proxy_cidrs = ["10.0.0.0/8"]` (your load-balancer/proxy
  ranges), or opt into trust-all **explicitly** with `trusted_proxy_cidrs = ["0.0.0.0/0"]`
  (unchanged, never warns). Development (`FRAISEQL_ENV=development`) downgrades the refusal
  to a warning, matching the `failed_login_*` production/development split.

### Added

- **`@fraiseql.type(embedded=True)` declares a type an embedded value object (#687,
  follow-up to #653).** An embedded value object has no independent identity and is always
  nested under a parent entity (e.g. a `Money` amount on an `Order`). Such a type declares
  **no** `sql_source` — the SDK suppresses the synthesized `v_{name}` — and is exempt from
  cascade classification: the compiler never enforces the `CascadeNode` `id: ID!` contract on
  it nor auto-implements the interface. This fixes the case where a value object embedded under
  a `cascade` mutation could not compile at all: its synthesized source made it a cascade
  entity that could never satisfy `id: ID!`. Purely additive — `embedded` defaults false, a
  type must opt in, and nothing currently-compiling changes. The SDK rejects the two
  contradictions the combination implies: `embedded=True` with an explicit `sql_source` (a
  value object has no backing view), and `embedded=True` with `cascade=True` (a value object
  cannot originate a cascade).

### Changed

- **The cascade `id: ID!` error now names *why* a type was classified an entity, *how* a
  mutation reaches it, and the actionable fix for each case (#653, #659, #687).** When a
  `cascade = true` mutation drags in a type that lacks `id: ID!`, the error previously stated
  only the failed assertion — and its two suggested fixes ("add `id`" / "remove `cascade`")
  were both wrong for an embedded value object the SDK gave a synthesized `sql_source`. It now
  reports the classification signal and names both exits — `classified as a cascade entity
  because it declares sql_source = "v_money"; if it is an embedded value object, mark it
  embedded=True (it will declare no source and be exempt); if it is an entity, add id: ID!` —
  alongside the reference path (`createOrder → Order.total → Money`). Purely a
  diagnostic change — which schemas compile is unchanged.
- **`compile --database` now warns when a `@fraiseql.type` declares a `sql_source` view that
  is absent from the connected database (#653).** These phantom sources — usually an
  embedded value object the SDK synthesized a `v_<name>` source for — were invisible to
  every existing check (type sources are not on the shared existence-probe work-list that
  the hard `FRAISEQL_VALIDATE_SQL_SOURCES` gate uses). The warning surfaces them at their
  origin instead of later as an unrelated cascade `id` error. Warn-grade and non-breaking:
  it never fails the compile, and the opt-in hard gate is unchanged (it still does not probe
  type sources, to avoid drowning in synthesized-source noise until they stop being
  synthesized).

### Fixed

- **Federation subgraphs with `cascade = true` mutations now compose into a supergraph (#698).**
  A cascade mutation makes the compiler synthesize five envelope value types — `UpdatedEntity`,
  `DeletedEntity`, `CascadeMetadata`, `QueryInvalidation`, `CascadeUpdates` — that are
  structurally identical in every cascade-enabled subgraph and carry no independent identity.
  They were emitted non-`@shareable`, so composing two such subgraphs into a supergraph failed
  Federation-v2 validation with one `INVALID_FIELD_SHARING` per field (21 in total). The
  synthesizer now marks exactly the envelope types it generates `@shareable` — via
  `federation.shareable_types`, the same mechanism the authored `MutationError` uses — when a
  federation block is present; a user type that already owns one of those names and the
  per-mutation `<Name>Payload` types are left alone. Per-subgraph compile and runtime are
  unchanged (the failure only ever surfaced at supergraph composition); the fix is schema-only.

- **`[changelog] expose = true` and `cascade = true` mutations now compile together (#665).**
  Since 2.12.0 the two features were mutually exclusive. The cascade pass classifies every
  view-backed, non-error type as a cascade entity and enforces the `CascadeNode` `id: ID!`
  contract on it — which swept in the framework's own `TransportCheckpoint` projection, keyed by
  `transport_name` with no `id` by design, so exposing the change-log and opting any mutation
  into cascade failed to compile on a type the user never wrote and could not fix. Framework-
  synthesized read-only projections (`EntityChangeLog`, `TransportCheckpoint`) are now marked
  `internal` and excluded from cascade classification: they are the change-capture *mechanism*,
  never cascade-deliverable entities, so they carry neither the `id: ID!` enforcement nor the
  `implements CascadeNode` auto-annotation. The runtime cascade path also rejects a payload entry
  naming an `internal` type, so the exclusion holds end-to-end. Adds one additive, optional key
  (`internal`) to the compiled-schema format — cli↔server are version-locked, so no cross-version
  concern, and single-feature schemas compile byte-identically. (This is the framework-generated
  instance of the cascade-classification problem whose diagnostic half shipped with #653; the SDK
  provenance half remains with #653.)
- **CSV and XLSX exports now emit a deterministic, alphabetically-sorted column order when no
  `?select=` is given.** The fallback column order was taken from `serde_json::Map` iteration,
  which is alphabetical only for the default `BTreeMap` build; a dependency that enables
  serde_json's `preserve_order` feature (e.g. the `functions-runtime-deno` runtime, or any
  `--all-features` build) flips the map to insertion order, silently changing export column
  order for the same data. `determine_columns` now sorts the fallback keys explicitly, so the
  header is alphabetical regardless of feature resolution. Default-build output is byte-for-byte
  unchanged; `?select=` still drives explicit column order.
- **`fraiseql setup` now installs the `core.tb_entity_change_log` change-log contract, so the
  first mutation on a freshly authored stack no longer fails at prepare time (#569).** Every
  default (`changelog = true`) mutation writes this table via its transactional-outbox CTE;
  previously it shipped only as an observers migration that no documented authoring step
  applied, so a stack built the documented way (`export_schema` → `compile` → DDL →
  `fraiseql-server`) failed on its first mutation with `relation "core.tb_entity_change_log"
  does not exist`. `setup` now applies the contract's idempotent DDL (vendored byte-for-byte
  from observers migration 08, guarded by a drift test), and `fraiseql doctor --against-db`
  points at `fraiseql setup` when the table is missing. Row-Level Security (migration 12)
  remains a separate operator step, and `RETURNS SETOF v_*` mutation functions remain
  unsupported — both documented in `docs/architecture/change-log-contract.md`.
- **Subscription row-visibility policies now hot-reload for new subscriptions (#611).** A
  `subscription_policy` added or tightened via a schema hot-reload previously took effect only
  on server **restart** — a fail-open window where a newly-tightened entity kept delivering
  all rows to new subscribers until then. The `/ws` handler now reads policies from the live
  (reload-aware) schema per new subscription, so a reloaded change applies on the next
  subscribe. Fail-closed is preserved: a policy whose owner identity is unresolvable refuses
  the subscription, never deliver-all. Already-connected subscriptions keep their
  subscribe-time boundary until they reconnect (the reload warning now says so); mid-stream
  re-derivation of live subscriptions is a deferred follow-up.

### Security

- **`mutation(requires_role=…)` is now enforced — the authored role was previously discarded
  at compile time, shipping the mutation ungated (#676).** A mutation declared with
  `@fraiseql.mutation(requires_role="admin")` (a capability documented in
  `docs/guides/operation-authorization.md`) was accepted by the SDK, silently dropped during
  compilation, and served callable by **any** principal — with no error at author time, no
  warning at compile time, and nothing in the compiled artifact recording that a gate had been
  requested. The runtime gate itself was always correct and tested; it simply never received
  its input. Two links were broken: `IntermediateMutation` declared no `requires_role` field
  (so serde discarded the key from `schema.json`), and the converter hardcoded `None`. Both are
  fixed, mirroring the query path, which was unaffected throughout.

  **Audit your mutations.** Any deployment relying on `requires_role` for mutation
  authorization was unprotected by that control on every release since it was documented;
  compensating controls (field authorizers, operation authorizers, RLS, gateway rules) were
  not affected. Recompiling with this release makes the declared roles take effect — which
  may **newly reject** callers that previously succeeded, so verify role assignments before
  rolling out. Type-level `requires_role` remains unenforced and is tracked separately in
  \#677.
||||||| parent of 28a1cc50b (fix(cli): drop `-d` short from --database (collides with global --debug) (#650))

## [2.13.1] - 2026-07-18

### Changed

- **The `-full` release tarballs now ship the `fraiseql-server` binary alongside the
  umbrella `fraiseql` binary (#643).** Previously every tarball — lean and `-full` —
  contained only the umbrella binary, whose `fraiseql run` subcommand is a development
  quick-launcher that reads only `[server]` and `[database]`. A `-full` download therefore
  could not run a real `server.toml` deployment (`[auth]` / `[federation]` / `[observers]`
  / `[enrichment]` / `[tenancy]` / `[storage]` were silently ignored), and the production
  entrypoint — `fraiseql-server --config` — was reachable only by rebuilding from source.
  The `-full` archive now carries **both** binaries, built from one revision (the #507
  same-revision contract made physical): use `fraiseql-server --config server.toml` for
  deployments and `fraiseql run` for local iteration. Windows `-full` zips include
  `fraiseql-server.exe`. The lean default artifact is unchanged; `aarch64-unknown-linux-gnu`
  remains lean-only (V8 does not cross-compile there — a documented gap in
  `docs/releases.md`).

### Fixed

- **`fraiseql run` no longer silently ignores unsupported config sections (#643).** Handed
  a config that declares platform sections it does not consume (`[auth]`, `[federation]`,
  `[observers]`, `[enrichment]`, `[tenancy]`, `[storage]`, `[security]`), it now logs a
  warning naming each ignored section and pointing at `fraiseql-server --config`, rather
  than dropping them without a word. Pointing the dev launcher at a full config stays
  legitimate (it is a warning, not a boot refusal) — but the drop is no longer silent.

- **Nested list-of-object query output is now recased to camelCase and projected to the
  selection set (#489).** A nested `[Object]` field projected from a JSONB `data` view
  was returned as the stored blob verbatim — `snake_case` element keys, plus keys the
  query never selected — while the camelCase-selected fields came back null. This was the
  third recasing path after #456 (mutation input) and #486 (query arguments): the SQL
  projector leaves list fields as the raw sub-blob, so the recasing + selection-set
  projection is now applied in Rust on both the query path (`project_nested_lists`, wired
  into the query runner) and the mutation/entity path (`project_entity`), at any depth up
  to the projection cap. Scalar fields and already-SQL-projected single objects are
  untouched.

## [2.13.0] - 2026-07-17

### Security

- **Private-bucket downloads are no longer advertised as shared-cacheable (#608).**
  Every storage download was served `Cache-Control: public, max-age=3600` regardless
  of the bucket's access mode. For a `Private` bucket this defeated the per-request
  RLS check (`can_read`) that ran immediately before it: any shared cache on the path
  (CDN, reverse proxy, corporate forward proxy) was told the response was public and
  could store it and serve the private object to unauthenticated third parties for up
  to an hour, and revocation did not take effect until the entry expired. Downloads
  now branch on access mode — a `Private` bucket serves `Cache-Control: private,
  no-store` (per-row `can_read` cannot be represented by a URL-keyed shared cache, so
  the object must not be stored at all); a `PublicRead` bucket is unchanged (`public,
  max-age=3600`). Only shared-cache-fronted deployments were exposed; direct-to-server
  deployments were unaffected.

- **Subscription row-level visibility on the live `/ws` path — fail-closed (#596).**
  Previously any principal authorized to subscribe to an entity over `/ws`
  (`graphql-transport-ws` / legacy `graphql-ws`) received **every** row's after-images:
  the subscribe path passed empty RLS conditions, so the push path enforced no row
  boundary the pull path (GraphQL queries) already did. An entity may now declare a
  `subscription_policy` (`owner_path` / `identity_field` / `bypass_roles`) in the
  compiled schema; at subscribe time the server derives a **server-owned** owner
  condition from the connection's server-resolved enriched identity (#539, the
  forge-proof `fraiseql.enriched.*` namespace — a client-supplied claim cannot widen
  it) and enforces it on every delivered event. **Fail-closed:** a policy-declaring
  subscription whose identity is unresolvable (no enrichment, denial, resolver outage,
  or anonymous) is **refused at subscribe time**, never delivered unfiltered; a
  `bypass_roles` role keeps full visibility; an entity with no policy is unchanged. The
  legacy `graphql-ws` subprotocol routes through the same enforcement and cannot bypass
  it. The single policy→condition derivation lives in `fraiseql-core` so the pull and
  push paths consume the same semantics. See
  `docs/architecture/enriched-identity-rls.md` ("The push path: subscription row
  visibility").

- **Realtime `/realtime/v1` entity-stream seam is fail-closed for policy entities
  (#596).** This second push subsystem carries entity after-images too but is **not
  assembled by any production binary** (#605). It now consumes the *same* `fraiseql-core`
  policy derivation as the `/ws` path, and — the property that matters on a dormant
  seam — its delivery is fail-closed for a policy-declaring entity: a subscription that
  reaches delivery without a resolved owner enforcement (`Bypass`/`Scoped`) is **dropped**,
  and a subscription whose identity is unresolvable is **refused** at subscribe time. So
  whoever eventually productionizes the subsystem cannot bring it up deliver-all by
  accident. Issue #605 tracks the productionize-or-remove decision.

- **Rate-limiting proxy-trust mitigation is reachable on the compiled path (#609).**
  `trust_proxy_headers = true` was settable in `[security.rate_limiting]`, but its safety
  valve `trusted_proxy_cidrs` was not — the CLI schema lacked the field and
  `deny_unknown_fields` rejected it, so the only reachable posture trusted
  `X-Forwarded-For` from **every** proxy IP. Any client could then spoof its address to
  bypass per-IP rate limiting or poison IP-derived logging — the mitigation the docs
  recommend could not be applied. The field is now accepted on the compiled path,
  **validated as CIDR notation at compile time** (a malformed range fails `fraiseql
  compile`, not server boot), and carried through to the runtime the server already
  honours (`extract_real_ip`). The permissive posture is now **explicit**:
  `trusted_proxy_cidrs = ["0.0.0.0/0"]` says "trust every proxy" on purpose.
  **Deprecation:** `trust_proxy_headers = true` with an empty or omitted CIDR list still
  boots but now warns that it will **refuse to boot in 2.14** (#618); set
  `trusted_proxy_cidrs` to your proxy ranges, or `["0.0.0.0/0"]` to keep trusting every
  proxy explicitly.

### Changed

- **`fraiseql_core::runtime::extract_rls_conditions` is now fail-closed (`Result`).**
  It previously silently dropped any clause shape it could not represent as a flat
  `field = value` equality ("delivers more events, never fewer") — a deliver-all hole
  when the conditions gate row visibility (#596). It now returns
  `Result<Vec<(String, Value)>, String>` and **errors** on a non-`Eq` operator, `Or`,
  `Not`, or native-column shape, so a caller deriving visibility conditions refuses the
  subscription rather than widening it. (Public API change; no in-tree callers relied
  on the fail-open behavior.)

- **Ambiguous-credential requests are rejected (401) when service accounts are enabled
  (ADR-0018).** In a deployment with `[security.service_accounts]` configured, a single
  request carrying **both** a valid JWT **and** an `x-api-key` secret is rejected as
  ambiguous rather than silently resolving to the JWT principal. Deployments without
  service accounts are unaffected (existing API-key behavior is unchanged).

### Added

- **Service-account identities — named, auditable, ceiling-bound external principals
  (ADR-0018, #602).** A `[security.service_accounts.<name>]` block grants an external
  daemon a first-class identity: an **env-indirected** static secret + a `run_as` ceiling
  (`roles` / `scopes` / `tenant`). It reuses — and supersedes — the scopes-only static
  API key: same header, same SHA-256 + constant-time compare, but the secret lives only
  in the env var named by `secret_env` (the config holds only the *name*, never an inline
  hash) and the principal carries a full ceiling minted through
  `SecurityContext::service_account` (`ActorType::ServiceAccount`,
  `user_id = service_account:<name>` in audit rows — no new actor type, no new column).
  - **Multi-entry-point:** the authenticator runs on the GraphQL handler, the `/ws`
    subscription upgrade (so a service principal can hold a **policy-scoped subscription**,
    #596), and REST — the same seam everywhere, no auth-middleware change.
  - **Fail-closed:** an unknown account / bad secret is a 401 **indistinguishable** from
    each other (no account-existence oracle); an account with no ceiling authenticates but
    has no authority (RLS / field-authz deny its writes); a secret whose env var is unset
    is skipped (unusable, never anonymous).
  - **`static_enriched`** (opt-in, per account) server-injects `fraiseql.enriched.*` fields
    for a daemon with no actor row — the only sanctioned deviation from uniform enrichment
    (ADR-0016 decision 6), server-injected and never token-asserted.
  - **Credential presentation** is the `x-api-key` header, not `Authorization: Bearer`
    (which the JWT middleware consumes first) — an amendment to ADR-0018 decision 2.
  See `docs/adr/0018-service-account-identities.md`.

- **`fraiseql functions invoke` — a local V8 test harness for function authors.**
  Runs a compiled function in a **real V8 isolate** against a fixture payload, with
  **mocked host ops**, printing the guest's result and every host-op call it made —
  the author's inner loop, no server/database/network required. The module loads
  exactly as the server loads it (from the compiled schema's `module_dir`).
  `--mock-http` / `--mock-query` supply canned responses (a request matching no
  configured mock fails loud); `--idempotency-token` injects the per-dispatch token;
  `--explain` shows why the `when` predicates (#597) did or did not match (evaluated
  before any isolate spins). Exit codes are CI-scriptable: `0` ran, `3` predicate
  no-match, `4` guest error, `1` config error. Built behind the opt-in
  `functions-invoke` CLI feature (V8 is ~30 MB, so the stock CLI stays lean). See
  `docs/architecture/functions.md`. Tracked follow-ups: `cron`/`after:ingest`
  payload synthesis in `invoke`, and a `--record` mode.

- **Typed guest payloads — `functions.d.ts` from `fraiseql generate-client`.** When the
  compiled schema declares functions, the TypeScript client generator now emits a
  `functions.d.ts` giving function authors editor type-checking for the host surface
  (`Deno.core.ops.fraiseql_*` via an ambient `FraiseqlHostOps`) and a typed event
  payload per function, derived from its trigger: `after:mutation`/`after:capture` on
  entity `E` → `{ event_kind, old: E|null, new: E|null }` (with `E` imported from the
  generated `./types`), `cron` → schedule context, `after:ingest` → the inbound-message
  shape (an undefined entity falls back to `unknown`). See `docs/architecture/functions.md`.

- **Function dispatch metrics + durable dead-letter queue (#598).** Function-trigger
  dispatch is now observable on `/metrics` and its failure record can survive a
  restart.
  - **Metrics** (Prometheus facade, `metrics` feature — the sibling of the source
    metrics): `fraiseql_function_dispatches_total{function, trigger_kind, result}`
    (`trigger_kind` ∈ after:mutation / after:ingest / after:capture / cron; `result` ∈
    ok / error / dead_lettered), `fraiseql_function_run_duration_seconds{function}`,
    `fraiseql_function_predicate_skips_total{function}` (a #597 `when` that evaluated
    false — the zero-cost-skip made visible), `fraiseql_function_dlq_size`, and
    `fraiseql_function_dlq_evictions_total`. `before:mutation` (sync), `http` (edge),
    and `after:storage` (no dispatch path yet) are metered elsewhere or not applicable
    — documented, not silently omitted.
  - **Durable DLQ:** `[functions] dlq_store = "memory" | "postgres"` (env override
    `FRAISEQL_FUNCTIONS_DLQ_STORE`). `"postgres"` persists dead-lettered dispatches to
    `_fraiseql_function_dlq` so they survive a restart and stay listable/replayable;
    `"memory"` (the default) is unchanged. The durable store honors the same
    `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE` drop-newest cap.
  - The per-dispatch `idempotency_token` is now in the dead-letter error log line so
    an alert traces to the exact dispatch (and a manual replay dedupes on it).
  - A dedicated `DispatchSource::AfterCapture` now tags capture-driven dispatches, so
    they are separable from `after:mutation` in the DLQ and on `/metrics`.
  See `docs/architecture/functions.md` (Observability).

- **`[mcp] read_only = true` — fail-closed MCP tool exposure.** When set, no mutation
  is ever exposed as an MCP tool regardless of `include`/`exclude`, so a mutation added
  to the schema later is not silently exposed to AI callers (the regression `exclude`
  alone cannot prevent). `read_only` wins over `include` (fail-closed precedence; a
  load-time warning is logged if `include` names a mutation under `read_only`). The docs
  now recommend `read_only = true` unless you deliberately expose writes. See
  `docs/mcp.md`.

- **Functions fire on externally-captured writes — `after:capture` (#366).** A
  third-party daemon (or `psql`) INSERTing into a `@subscribable` table can now drive
  a function, not just observers/subscriptions — event-driven reconciliation instead
  of cron-polling. A new `after:capture:<Entity>[:<operation>]` trigger dispatches from
  the change-log reader on the same durable/`re_runnable`/DLQ machinery and phase-02
  `run_as` host as `after:mutation` (so the function can `fraiseql_query` back). **Loop
  safety:** dispatch keys on the captured-row discriminator
  `extra_metadata.cdc_source = "fallback_trigger"` — a FraiseQL executor/bridge write
  carries no marker, so a capture-dispatched function writing back never re-enters the
  capture path. Phase-04 `when` predicates evaluate identically on capture payloads.
  See `docs/architecture/external-write-capture.md`.

- **Declarative `when` predicates on after:mutation triggers (#597).** A function can
  now declare *when* it fires — `{ "field": "status", "changed_to": "approved" }` or
  `{ "field": "kind", "eq": "standard" }` — evaluated by the dispatcher on the row
  images **before** any runtime spins; a false predicate produces no dispatch record
  at all. The condition, previously invisible guard code inside the guest, is now
  auditable from the schema. Deliberately small: `eq` (state) + `changed_to`
  (UPDATE-only transition), a conjunction list, exactly one operator per predicate,
  unknown keys and `changed_to` on a non-`update` trigger are load errors — a dispatch
  filter, not a rules engine. Note: the after:mutation route path has no pre-image, so
  `changed_to` there gates on the after-value; full transition detection needs the
  after:capture path (`pre_image=True`). See `docs/architecture/functions.md`.

- **`cron:` functions fire from a running server, single-firing across replicas
  (#595).** Scheduled functions previously never fired from a stock server — the
  `CronScheduler` existed but nothing wired it at startup, and it had no leader
  election despite the docs claiming it. The server now builds one leased `CronPoller`
  per cron function at startup (a cron function is "a scheduled source without a
  cursor"): it ticks on the schedule, single-fires across replicas via the sources'
  PostgreSQL advisory lease (`LeaseGuardedRunner`, keyed `cron:<function>`), runs on
  the phase-02 I/O host (so `fraiseql_query` works under the function's `run_as`
  ceiling), and records each firing to `_fraiseql_cron_state`. Missed-tick policy is
  **skip** (a server down over a scheduled instant does not replay on boot). Requires a
  DB pool (the lease + state table). Metrics land in a later minor. See
  `docs/architecture/functions.md`.

- **`after:mutation` functions can write back — the `fraiseql_query` bridge under a
  `run_as` ceiling (#594).** An event-dispatched function's `fraiseql_query` host op
  now executes against the engine, closing the trigger → side-effect → **record** loop
  (previously only scheduled sources had the bridge; every other path failed with
  "query executor not configured"). Authority is the same fail-closed model sources
  use: an optional `run_as` (`{ roles, scopes, tenant? }`) ceiling on the function
  definition, carried at runtime by a `system_job:<function-name>` identity. A
  function with **no `run_as`** runs the bridge anonymously — RLS and field-authz deny
  its writes until an operator grants a ceiling. Function-authored writes are audited
  as `ActorType::SystemJob`, attributable to the issuing function. The
  `SourceQueryExecutor` was extracted into a shared `RunAsQueryExecutor`
  (`crate::query_bridge`) so sources and functions run through one authority +
  hot-reload seam. **Deliberate asymmetry:** a bridge write does not itself fire
  `after:mutation` (dispatch is route-layer only; there is no recursion to guard). See
  `docs/architecture/functions.md`. (`after:ingest` bridge wiring is a tracked
  follow-up.)

- **Feature-complete `-full` release binaries.** The published release now includes a
  second artifact per native target, `fraiseql-full-<target>`, carrying the stable
  opt-in platform features that were previously reachable only from a source build:
  Deno/TypeScript functions (`functions-runtime-deno`), scheduled `sources`, `mcp`,
  `inbound` + `inbound-email`, `observers`, and Prometheus `metrics`. It also enables
  `run-server`, so the `-full` binary is a self-contained server (`fraiseql run`), not
  a CLI with dead server code. Adopters of the platform features download this binary
  instead of rebuilding cli+server at the same revision themselves — the
  same-revision `jsonb_column` contract (#507) makes mixing a stock cli with a custom
  server dangerous, so a matched pair matters. **The lean default artifact
  (`fraiseql-<target>`, `cli,server,postgres`) is unchanged** — use the cli from the
  same release. See `docs/releases.md` for the artifact matrix and the
  native-target-only caveat (V8 does not cross-compile to `aarch64-unknown-linux-gnu`,
  which ships lean; use the Docker image or a source build for ARM-Linux platform
  features). The umbrella `fraiseql` crate gains pass-through features
  (`functions-runtime`, `functions-runtime-deno`, `sources`, `mcp`, `inbound`,
  `inbound-email`, `metrics`, `run-server`) mirroring the `observers` precedent, plus
  a `release-full` bundle.

### Breaking

- **Config sections that validated then did nothing are now rejected at compile
  (#612).** Several `fraiseql.toml` sections the compiler accepted but no runtime
  consumed now fail loudly at load — the fix-forward "honest-loud over silently-wrong"
  stance (the v2.7.0 field-encryption precedent) — with a message naming the section
  and either the real alternative or the tracking issue. Previously each was accepted
  and silently ignored, so an operator believed it took effect. A schema using any of
  these now errors; remove the section (or migrate as noted). The rejection runs on
  **every** compile path, including `--types` (`merge_files`), which skips the rest of
  `validate()`:
  - **`[security.rules]` / `[security.policies]` / `[security.field_auth]` (security).**
    Declared authorization the runtime never enforced — `RuntimeConfig::from_compiled_schema`
    pins the operation- and field-authorizers to `None`, so any access boundary these
    blocks implied did not exist. Every deployment carrying them was operating on a
    false belief; the break *is* the fix. Remove them and enforce authorization at the
    database layer (RLS policies keyed on the session variables FraiseQL sets from the
    request identity) until a compiled-schema declarative-authorization engine ships
    (**#626**).
  - **`[caching]`** — never lowered into the compiled schema; no runtime honored it (**#623**).
  - **`[analytics]`** — fully inert (**#624**).
  - **`[observability]`** — inert on the compiled path; configure metrics under
    `[metrics]` and tracing under `[tracing]` in `fraiseql.toml` instead (**#625**).
  - **`[security.api_keys] storage`** — only `"env"` is implemented; `"postgres"`
    authenticated nothing. Set `storage = "env"` (postgres-backed store: **#627**).

- **The `multitenant` and `saas` examples stopped compiling until corrected (#612).**
  Both declared `[[security.rules]]`, and the `multitenant` README + config claimed those
  rules enforced tenant isolation — a false security claim (the rules were never
  enforced). The unenforced blocks were removed and the docs corrected to point at
  database-layer RLS plus the session variables FraiseQL sets from the identity
  (`resolve_session_variables`, `crates/fraiseql-core/src/runtime/executor/support/security.rs`);
  a worked end-to-end isolation example is tracked in **#628**.

- **Admin-API and observer config that "succeeded then did nothing" now fails loud
  (#612, part 2).** The same honest-loud pass applied to the admin/observer surface:
  - **`[[observers.handlers]]` in `fraiseql.toml` is rejected at compile.** Compiled
    handlers were never loaded as runtime observers — those come only from the
    `tb_observer` table / the admin observer API — so a declared handler silently
    never fired. Define observers in `tb_observer` (or `POST /api/observers`) and
    remove the block. Loading compiled handlers at boot is tracked in **#631**.
  - **Creating an observer with an action `type` of `"database"` or `"log"` now
    returns `400`, not `201`.** The runtime has no dispatcher for those types, so the
    observer was created and then silently warn-and-skipped at load. The admin API now
    rejects them at create/update, naming the supported types (`webhook`, `email`,
    `slack`); real database/log dispatchers are tracked in **#632**.
  - **The admin observer retry field is `backoff_strategy`, not `backoff`.** The
    runtime reads `retry_config.backoff_strategy`, so the old DTO field name `backoff`
    was silently dropped and every observer defaulted to exponential backoff. The field
    is renamed and both `RetryConfig` structs gained `deny_unknown_fields`, so the dead
    `backoff` key (or a typo) now fails loud. **Migration:** any `tb_observer.retry_config`
    JSONB written before this release that carries a `backoff` key must be updated to
    `backoff_strategy`, or that observer will fail to reload.

### Fixed

- **Webhook observers honor their configured HTTP method (#612 item 12).** The admin
  API accepted a `method` on webhook actions but the runtime always issued a `POST`.
  The method is now threaded through dispatch (`PUT`/`PATCH`/… work; default stays
  `POST`); an unparseable method fails loud rather than silently posting.

- **Config drift-prevention gate (#612 item M).** A checked-in coverage test walks
  every leaf of `TomlSchema::default()` (CLI) and `ServerConfig::default()` (server)
  and asserts each maps to a named consumer in a reviewable manifest — a new config
  key that no runtime consumes now fails CI at PR time rather than surfacing in a docs
  pass. This is the durable half of #612: the mechanism whose absence let the whole
  class accumulate. Paired with the CLI↔server round-trip pins (#6, #9, 5b) that a
  leaf-walk cannot reach.

- **Honest docs for two accepted-but-unenforced knobs (#612 items 13/14).** Tenant
  `max_storage_bytes` is now documented as advisory-only (stored, never enforced — no
  metering path exists; enforcement tracked in **#633**), and the observer Prometheus
  metrics registry documents that it is not scraped by the server's `/metrics`
  (a two-ecosystem split; bridging tracked in **#634**). No behavior change — the
  surfaces no longer imply a guarantee that isn't there.

- **A single `[auth]` block now validates on both the CLI compiler and the server (#612).**
  The CLI's `[auth]` schema required the PKCE OAuth-client fields (`discovery_url`,
  `client_id`, `client_secret_env`, `server_redirect_uri`) with `deny_unknown_fields`,
  while the server's OIDC config — read from the **same** `fraiseql.toml` — expects
  `issuer`. So a JWT-validation block (`[auth] issuer = "…"`) failed `fraiseql compile`,
  and no single `[auth]` could satisfy both tools. `[auth]` now accepts a **JWT-validation
  group** (`issuer` + optional `audience`) and a **PKCE OAuth-client group** (the four
  client fields), each validated as a coherent unit (client group is all-four-or-none;
  `audience` requires `issuer`; an empty `[auth]` is a load error). The PKCE server-login
  flow is **not yet functional on the compiled path** — the compiled schema carries no
  `auth`/`auth_endpoints` blob for the server's `OidcServerClient`, and the CLI never
  emitted one — so a *complete* client group is now **rejected at compile time with a
  pointer to #621** instead of being silently accepted (the item-4 precedent: declared-but-
  unenforced auth fails loud). The previously-false operator instructions that told
  operators to configure those four fields (`builder.rs` startup error, the Auth0 and SAML
  integration guides) are corrected to say so. JWT validation is unaffected.

- **RLS session variables now reach the Relay `node(id:)` and partial-period aggregate
  read paths (#610).** Both paths ran their read without resolving the schema's configured
  `session_variables`, so a PostgreSQL RLS policy reading `current_setting()` did not
  constrain them — a cross-tenant read on any Relay `node(id:)` lookup and any aggregate
  taking the partial-period (`UNION ALL`) branch, both reachable from an ordinary GraphQL
  request. They now resolve session variables and use the connection-affine
  `*_with_session` adapter methods, exactly as regular queries, Relay pages, standard
  aggregates, and mutations already did. (These were the two surviving read-path follow-ups
  from #329, which was closed after its mutation-path fix shipped; they are fixed here under
  #610, not by reopening #329.)

- **`security.token_revocation.revoke_all_ttl_secs` now reaches the server (#612).**
  The server reads this key (default 86400s) to bound how long a `revoke-all` epoch
  suppresses tokens, and the docs instructed setting it — but the CLI TOML schema lacked
  the field, so `deny_unknown_fields` rejected any config that set it and it could never
  take effect. Added to the CLI `[security.token_revocation]` schema; it now serializes
  into the compiled schema the server already reads.

- **Removed a dead rate-limiting config reader that silently fed hardcoded defaults
  (#612).** `fraiseql-auth`'s `SecurityConfigFromSchema` parsed a nested-camelCase
  `rateLimiting.authStart.maxRequests` shape the compiler never emits (it emits flat
  snake_case `security.rate_limiting`), so that reader always fell back to hardcoded
  defaults; its output only fed startup logging/validation before being dropped and
  never drove runtime limits (those come from the server middleware's live
  `RateLimitingSecurityConfig`, which reads the flat shape correctly). The dead
  rate-limiting portion of the reader was removed and a merger→reader round-trip test
  now pins the flat shape so the two ends cannot drift silently again.

## [2.12.0] - 2026-07-15

### Added

- **Scheduled ingress `Source`s — the dual of `Observer` (#573).** A `Source` pulls
  from an external system on a schedule and drives the results into the database via
  mutations, resuming from a durable cursor with at-least-once delivery — the ingress
  counterpart to an observer's egress. The primitive ships end to end:
  - **Coordination.** A durable opaque cursor store (`_fraiseql_source_cursor`, one
    row per source) advanced by a monotonic **compare-and-swap** so a stale writer can
    never regress the watermark; deny-by-default RLS + `REVOKE … FROM PUBLIC`, the
    cursor value stored as opaque JSONB (written only via parameterized binds, never
    assembled into SQL text). A single-firing **advisory-lease runner** (lock key = a
    stable `SHA-256` of the source name) so a source scheduled on N replicas fires on
    exactly one.
  - **Two execution models, one envelope.** *Model A* — a native Rust `PullSource`
    (`poll() → batch`): the framework spine-dedups each message and advances the
    cursor **in the same transaction** as the ingest writes (atomic, no reprocess
    window). *Model B* — a handler-driven **Deno connector** (`ctx.cursor` → fetch →
    `ctx.query` mutate → `ctx.advance`); the envelope supplies single-firing, the
    cursor, and observability. The poll-IMAP email adapter is reimplemented as the
    reference Model A source (see Breaking).
  - **Least-privilege identity, fail-closed.** A source's mutations run under an
    explicit authority *ceiling* — `run_as` (`{ roles, scopes, tenant? }`) on the
    compiled definition — carried at runtime by a new
    `SecurityContext::system_job(...)` (the first use of the `ActorType::SystemJob`
    principal, recorded for audit, never an authorization input). A source with no
    `run_as` runs with no authority: RLS and field authorization deny its writes until
    an operator grants a ceiling. `run_as.tenant` scopes a single-tenant or global
    source; a multi-tenant source leaves it unset and re-scopes each write to a
    per-message tenant, and a source already pinned to a tenant cannot forge writes for
    another. Outbound fetches inherit the deny-by-default SSRF allowlist (no bypass).
  - **Authoring.** `@Source({ schedule, cursor, runAs })` (TypeScript) and
    `@fraiseql.source(...)` (Python) compile to a `sources` array in the schema; the
    compiler validates the cron expression, unique source/cursor names, and `run_as`.
  - **Server.** An opt-in `sources` cargo feature spawns one poller per enabled source
    on the functions-subsystem lifecycle (`[sources]` TOML + `FRAISEQL_SOURCES_*` env
    overrides, env > TOML > default), drains gracefully on shutdown, and warns loudly
    if `[sources]` is configured while the feature is off.
  - **Observability.** Prometheus metrics `fraiseql_source_fires_total{source,result}`,
    `fraiseql_source_skips_not_leader_total{source}`, and
    `fraiseql_source_run_duration_seconds{source}` (under the `metrics` feature);
    structured fire/skip/error logs carrying the source and a per-firing idempotency
    token (HMAC-signed when a server secret is configured; payload logging opt-in via
    `[sources] log_payloads`); and a read-only `fraiseql sources` CLI that lists each
    source's schedule, `run_as` ceiling, and — with a database URL — its cursor value,
    CAS version, and staleness.

  Opt-in and STABLE-tracked (may evolve). See `docs/architecture/sources.md`.

### Breaking

- **Poll-IMAP email cursor storage removed; email now uses the generic source
  cursor (#573).** The bespoke `_fraiseql_inbound_email_cursor` table and its
  `PostgresEmailCursorStore` are deleted with no migration or backfill (there are no
  IMAP users). The email adapter is reimplemented as the reference native
  `PullSource` (`ImapSource`) driven by the generic source envelope, and its
  per-mailbox UID watermark now lives — as opaque JSONB — in the shared
  `_fraiseql_source_cursor` table (created by the same `init_cursor_store`). This
  also **fixes a multi-replica double-poll**: each mailbox is now polled under a
  single-firing advisory lease, so several server replicas poll each mailbox exactly
  once between them instead of all polling it. Cursor advance is now transactional
  with the spine emit (all-or-nothing per poll batch) rather than per-message.
  `fraiseql_functions::migrations::inbound_email_cursor_migration_sql` is removed.

### Changed

- **Entity-identity contract: `id: UUID` is canonicalized to `id: ID` (ADR-0017).**
  FraiseQL now treats a global `id: ID!` as a first-class invariant consumed
  uniformly by cascade, Relay `Node`, and federation `@key(fields: "id")` — rather
  than each subsystem re-deriving identity from heterogeneous id types. The compiler
  rewrites the Trinity external id (`id: UUID`) to `id: ID` on every output object
  type. This is **wire-transparent** — a UUID and an `ID` serialize to the same JSON
  string — so clients are unaffected; only introspection/SDL now reports `id: ID`
  where it previously reported `id: UUID`. Non-identity ids (a serial `id: Int`, or
  no `id`) are left as-is and must expose `id: ID` (a UUID surrogate) to use
  cascade/Relay. See ADR-0017 and `docs/architecture/mutation-response.md`.
- **All authoring SDKs emit `id: ID` for identity fields (honest at source).** Every
  FraiseQL SDK that emits `schema.json` now canonicalizes a field named `id` typed as
  a wire-transparent string (`string`/`str`/`UUID`) to GraphQL `ID`, enforcing the
  documented convention each SDK already stated. Previously a Trinity surrogate
  authored as `id: string` leaked `id: String`, which the compiler's identity contract
  then rejected; the emitted schema is now conformant at the source, so the compiler's
  `UUID → ID` canonicalization is a backstop rather than the primary mechanism.
  Covered: **Python, TypeScript, Go, C#, F#, Java, PHP** (fixed + tests green),
  **Elixir** (fixed + tests, gated by `elixir-sdk.yml` CI), and **Scala** (fixed +
  tests, gated by a new `scala-sdk.yml` CI workflow — the community SDK previously had
  none). A numeric `id: Int` is left unchanged. Client/runtime SDKs (Dart, Ruby, Rust,
  Kotlin, Swift, Node.js, Clojure, Groovy) are unaffected — they consume the API, they
  don't emit schemas.

### Fixed

- **`fraiseql compile` now surfaces the full error cause chain.** Converter
  failures printed only the top-level context (e.g. `Failed to convert schema to
  compiled format`) with the underlying reason swallowed at every log level and in
  `--json` — the cause was reachable only via the undocumented `--debug` flag, and
  never in JSON. The CLI now renders the whole `anyhow` source chain in both modes:
  `  caused by: …` lines in human output and a `"causes": [...]` array in `--json`
  (additive; `message`/`code` unchanged). Applies to every command, not just
  `compile`.
- **Cascade/Relay type synthesis no longer emits a schema that fails its own
  validator.** Auto-implementing `CascadeNode` (cascade) or `Node` (`relay = true`)
  on an entity whose `id` was `UUID`/`Int` or absent produced IR that the
  compiled-schema validator then rejected with a swallowed "missing field 'id'"
  bail — so a schema that compiled under 2.10 could fail under 2.11 (which began
  honoring the previously-dropped SDK `cascade` flag) with the real reason hidden.
  Resolved by the entity-identity contract (see *Changed*): the Trinity `id: UUID`
  now canonicalizes to `id: ID` and satisfies the interface automatically, and any
  residual non-identity id (`Int`, absent) fails fast — *before* the `implements`
  push — with one aggregated, actionable error naming every offending type and the
  remedy, instead of a swallowed bail.
- **The compiled-schema validator recognizes interfaces as valid return types.** A
  query or mutation returning an interface type (narrowed via inline fragments) is
  now accepted instead of silently failing the type-reference check; every
  validator rejection also logs a `warn!`, not just the query-return case.
- **Built-in change-log fields now follow the SDK's camelCase convention by default
  (#500).** The Python SDK recases every field, operation, and argument it emits to
  camelCase, but it did not record that convention in the exported schema. The
  compiler's change-log injection (the #149 audit log) renders its identifiers via the
  schema's `naming_convention`, which defaulted to `Preserve` — so an exposed
  `EntityChangeLog` / `TransportCheckpoint` came out in snake_case
  (`pk_entity_change_log`, `object_type`, `created_at`), the only snake_case corner of
  an otherwise camelCase API. The camelCase change-log support added in #498 was
  therefore inert unless the caller hand-injected `naming_convention` into the schema
  JSON. `get_schema_dict()` (and the registry's `get_schema()`) now emit
  `naming_convention: "camelCase"`, so a change-log exposed from an SDK-authored schema
  compiles to camelCase fields with no manual intervention. The SQL contract is
  unchanged — the runtime still recovers the snake_case JSONB keys via `to_snake_case`.
- **`auto_params` queries now expose their `where`/`orderBy`/`limit`/`offset` as real
  GraphQL arguments.** A list query configured with `auto_params` carried those
  parameters only as compile-time flags — the runtime read them straight from the
  argument map, but they were never materialised as field arguments. Every consumer
  that renders from the argument list therefore emitted the query without them:
  `generate-client typescript` produced a bare, argument-less query and function, and
  the federation `_service { sdl }` (and `/schema` endpoint) advertised the field with
  no arguments — so a generated client or a federated gateway could fetch the first
  page of a collection but could not paginate or filter it (the built-in change-log
  being the clearest case: its own description documents `where`/`orderBy`/`limit`).
  `QueryDefinition::graphql_arguments()` now synthesizes those arguments from the
  `auto_params` flags (`where`/`orderBy` typed as the `JSON` scalar, `limit`/`offset`
  as `Int`), and the SDL renderer, the TypeScript client generator, and GraphQL
  introspection all render through it — so the three stay consistent and a generated
  client can paginate and filter. An explicit argument of the same name still wins
  (no duplicates), and Relay connection queries are unchanged (their `first`/`after`/
  `last`/`before` surface is owned by the Relay path).

## [2.11.0] - 2026-07-06

### Added

- **Typed, enforced GraphQL cascade (cascade hardening).** The `cascade`
  feature — a mutation returning every entity it affected, per the graphql-cascade
  spec — is rebuilt from a verbatim JSONB passthrough into a first-class, enforced
  surface. Opt a mutation in with `cascade=True`
  (`@fraiseql.type(crud=True, cascade=True)` / `@fraiseql.mutation(cascade=True)`);
  the flag is now honored end-to-end (it was silently dropped by the compiler). The
  mutation then returns a typed payload wrapper `<Name>Payload { entity, cascade,
  updatedFields }` whose `cascade: CascadeUpdates` carries `updated`
  (`[UpdatedEntity!]!`), `deleted` (`[DeletedEntity!]!`), `metadata`
  (`CascadeMetadata!`), and `invalidations` (`[QueryInvalidation!]!`) — a
  `CascadeNode` interface is auto-implemented on every queryable entity so cascade
  entities are selectable via inline fragments. At runtime every cascade entity is
  projected to camelCase and run through the field-level authorizer (#423) exactly
  like a queried entity; cascade is selection-gated (never injected unrequested);
  the response is bounded by `RuntimeConfig.cascade_limits` (max affected entities →
  truncated with `metadata.truncated`, max response size → rejected); and cascade
  entities drive cache invalidation on their schema-resolved views. Shipped SQL
  builders (`fraiseql.build_cascade` / `cascade_entity` / `deleted_entity` /
  `cascade_invalidation`, installed by `fraiseql setup`) are the paved path; a live
  2-tenant conformance test pins that cascade row-visibility follows RLS. See
  `docs/architecture/mutation-response.md`.
- **`fraiseql doctor --against-db` audits `sql_source` views for `security_invoker`.**
  Warns when a view backing an entity type lacks `security_invoker` while the
  database uses RLS — that view runs as its owner and silently bypasses the caller's
  RLS (a cross-tenant leak on ordinary reads and in cascades).

- **Enriched-identity RLS (#539).** One request-scoped `sub → DB → identity`
  resolver: the application maps a token's subject to its own internal identity
  (`actor_id`/`actor_role` for reads, verified from-address for sends) in its own
  database, resolved once per request, cached, and **fail-closed**. Configure a
  top-level `[identity.enrichment]` (and `[identity.sender]`) — top-level so it
  applies under HS256 and OIDC alike. Resolved fields merge under the reserved,
  forge-proof `fraiseql.enriched.*` namespace (the extractor strips incoming
  `fraiseql.` claims), read with no fallback by the new
  `SessionVariableSource::Enrichment` / `InjectedParamSource::Enrichment` sources,
  so RLS/views/inject-params scope on a DB-derived identity rather than a
  client-asserted claim. An unknown/ambiguous subject, a NULL mapped field, or a
  missing bound param is denied (403) *before* any data query runs — never a
  silent skip or an empty-string GUC; a transient DB error is a 503, never an
  unscoped read. The precise denial reason is logged server-side (WARN) while the
  outward body stays generic (no actor-table existence oracle). The cache keys on
  the bound-`$param` tuple (multi-issuer apps bind `$iss`) with a bounded positive
  TTL (default 60s) so a revocation propagates within that window — or immediately
  via the admin `POST /api/identity/flush[-all]` (behind the admin bearer token).
  Verified
  sender-identity resolves on the same primitive through an object-safe
  `SenderIdentityResolver` seam (`LoginEmailSender` is the degenerate default);
  the `send_email` op that consumes it lands with the native-runtime hardening
  train. Supersedes #242. See `docs/architecture/enriched-identity-rls.md` and
  ADR-0016.
- **`send_email` host op with a host-owned `from` (native-runtime hardening).**
  Functions can now send email through a first-class `send_email` host op (WASM +
  Deno): the guest supplies only `to`/`subject`/body, and the host injects the
  `from` from the resolved sender identity (the #539 seam — `LoginEmailSender` by
  default, a DB-backed resolver where the sending mailbox differs). A guest cannot
  send from another address (a `from` in the request is dropped at the type level).
  The transport is **per-connected-account SMTP** (`[mailbox.<name>.smtp]`,
  STARTTLS, secrets read server-side by mailbox, the account selected by the
  verified sending address — never a shared mailbox), with a **send-warming daily
  cap** that ramps 10/day → 200/day → unlimited. Failures classify onto durable
  dispatch: a permanent refusal (denied identity, bad recipient, SMTP 5xx,
  over-cap) is a 4xx → dead-letter; a transient one (SMTP timeout/greylist,
  identity store momentarily down) is a 5xx → retry. The DB-backed `SendCounter`
  (over the app's mailbox table) is the remaining warming piece. See
  `docs/architecture/native-runtime-ergonomics.md`.
- **Host-provided per-dispatch idempotency token (native-runtime hardening).** A
  function can read a per-dispatch idempotency token —
  `Deno.core.ops.fraiseql_idempotency_token()` (WASM: `get-idempotency-token`),
  `string | null` — and pass it straight to a downstream money/mail idempotency
  header. The durable dispatcher derives it **once** from the dispatch's stable
  identity (source + function + trigger + payload data — never wall-clock/random)
  and injects it into every retry attempt, so it is stable across retries and
  across a resume, and distinct per logical operation; an at-least-once dispatch
  therefore stays at-most-once. The token is 32 lowercase hex characters, URL-safe
  and short enough for a VERP email local part (reused as the send correlation id
  by the delivery-feedback work). `examples/native-functions/qonto-sync.ts` now
  prefers it and falls back to its invoice-derived key only on a non-dispatched
  invocation; the dead-letter record carries the token for operator inspection.
- **Delivery feedback loop — bounces, challenges, replies, suppression
  (native-runtime hardening).** `send_email` is now a *tracked* delivery rather than
  a one-shot: SMTP `2xx` means accepted, not delivered, and the real outcome arrives
  inbound. Each send sets a per-send VERP Return-Path
  (`MAIL FROM: bounces+<send-id>@<domain>`, header `From` unchanged) keyed by the
  **HMAC** idempotency token (unforgeable, so a forged bounce cannot poison delivery
  status); the poll-IMAP adapter correlates an inbound bounce/challenge/reply back to
  the send via the recipient plus-tag (with a `References`/`Message-ID` fallback) and
  transitions a send-status lifecycle (`Sent → Bounced | ChallengePending | Replied`;
  **no lying `Delivered`** — "no news ≈ delivered"). A **suppression list** is checked
  before every send (a suppressed recipient is a permanent refusal); it stores a
  **keyed hash of the address, never the raw address**, so a GDPR erasure elsewhere
  keeps the do-not-contact match. Hard bounces suppress immediately (permanent);
  repeated unanswered **challenges** suppress after N (`[send]
  challenge_suppress_after`, default 2, per-recipient, event-based) and are
  **surfaced, never auto-solved** for cold outreach (reputation + GDPR); a genuine
  reply lifts a challenge suppression at once. A durable retry of an already-sent
  dispatch is **skipped** (exactly-once). Transient SMTP failures carry a
  mail-appropriate backoff floor (greylisting clears in minutes, not the policy's
  seconds). An opt-in startup **Return-Path probe** (`[send] verp_probe_on_start`)
  verifies the provider preserves plus-addressing before VERP is trusted. All stores
  are tenant-scoped (RLS); the operator appends/queries suppressions via
  `POST /api/email/suppress` / `POST /api/email/suppression` (admin bearer,
  server-side hashing, address in the body — never a query string, so it is not
  captured by access logs); IMAP processing is read-and-move only (never expunges).
  Configured via `[server] hmac_secret_env` (VERP-gated: absent → plain token, no
  correlation), `[mailbox.<name>.smtp.return_path]`, and `[send]`.
- **After:mutation functions now run in the stock server binary.** The server loads
  each declared function's module from the compiled schema's `module_dir`
  (`<module_dir>/<name>.<ext>` — `.wasm` for WASM, `.js`/`.ts` for Deno), registers
  the compiled-in runtimes, and mounts the before-mutation dispatch hooks at serve
  time, so `after:mutation` functions (and the `send_email` op) fire on the
  I/O-capable live host. Previously the runtime + dispatch machinery existed and was
  unit-tested but was never wired into the binary. A declared function whose module
  file is missing or unreadable, or whose runtime is not compiled in, **fails server
  startup** (a declared function that can never run is a misconfiguration, surfaced
  loudly rather than silently never firing).
- **Permanent-error tagging for durable functions.** A function can now signal that
  a failure is permanent so durable dispatch dead-letters it on the first attempt
  instead of exhausting retries: a guest throws
  `Object.assign(new Error(msg), { fraiseqlPermanent: true })` (or a message carrying
  the `[fraiseql:permanent]` marker), which the runtime maps to a 4xx `FraiseQLError`.
  Host ops auto-tag — any op returning a 4xx (client) error is permanent by default,
  so e.g. a `send_email` refusal (denied identity, rejected recipient, over-cap)
  dead-letters immediately while a transient one still retries. Untagged errors are
  unchanged (transient). Works on both the Deno and WASM runtimes.
- **Beta workload migrated to the native runtime.** The adjacent Python/FastAPI
  sidecar's compute is now native TypeScript,
  proving the host surface against a real workload. Four `examples/native-functions`
  are the migrated workload, each driven end-to-end through the Deno runtime
  against a recording host: `deal-scoring.ts` (LLM scoring **+ next-action** on the
  fire-and-forget re-runnable path); `qonto-sync.ts` (money path on the **durable**
  dispatcher, with a deterministic invoice-derived idempotency key so at-least-once
  dispatch never double-charges, and fail-loud on any non-2xx); `follow-up-email.ts`
  (**per-user** send — the `from` comes only from the connected user's verified
  address in `auth_context`, never a shared mailbox, and a missing address fails
  loud); and `reply-awareness.ts` (`after:ingest:email`) **proven end-to-end
  against a fixture mailbox** — real `.eml` fixtures run through the real
  normalization + classification + dispatch-payload builder, where only a *human*
  reply stops the sequence and out-of-office / bounce / auto-generated mail is
  ignored (the live end-to-end proof of the inbound-email path). The per-user send rule is a pure,
  fail-loud policy `fraiseql_functions::outbound::resolve_sender_identity`, and the
  live host's `auth_context` now surfaces the connected user's verified `email` /
  `display_name`. A first-class `send_email` host op (host-owned `from`) over a
  concrete SMTP/provider transport, and real TypeScript type-stripping, are
  documented follow-ups for a planned hardening train — see
  `docs/architecture/native-runtime-ergonomics.md`.
- **Poll-IMAP email adapter + normalization.**
  The first *pull* inbound source, riding the inbound-source primitive. Behind the
  opt-in `inbound-email` feature, each configured `[imap.<name>]` mailbox runs a
  background poll worker (no IMAP-IDLE — *stateless with a cursor*) that fetches
  messages above a per-mailbox `UIDVALIDITY`/`UID` watermark
  (`_fraiseql_inbound_email_cursor`), normalizes their MIME, emits them onto the
  same durable spine as the webhook adapter, and fires `after:ingest:email`
  functions. Transport is IMAPS over rustls (no OpenSSL); `BODY.PEEK[]` means
  polling never marks mail `\Seen`. The high-value **normalization layer** is pure
  and lives in `fraiseql-functions` (always compiled, unit-tested): MIME headers,
  text/HTML bodies, attachments (streamed to `[storage]`, with the raw message
  retained for replay), threading (`Message-ID`/`In-Reply-To`/`References` →
  `thread_key`), dedup by `Message-ID`, and a `Classification`
  (human / out-of-office / bounce / challenge / auto-generated) that reply-awareness
  keys on — with loop protection via `Auto-Submitted` / `Precedence` / list
  headers. The cursor advances only past committed messages, so a transient
  failure or a `UIDVALIDITY` reset re-fetches and the spine's `Message-ID` dedup
  makes it idempotent (at-least-once). Sending stays per-user. Attachment
  size/type limits, virus scanning, and `StorageState` wiring remain follow-ups.
- **Inbound ingestion as a source (continues #431).** The symmetric mirror of the
  outbound observer→signed-webhook path: an
  external message becomes a normalized `InboundMessage` on a durable spine that
  `after:ingest[:<source>]` functions consume. A `Source` trait models both push
  (ack-based, e.g. a provider webhook) and pull (cursor-based, e.g. poll-IMAP)
  adapters; the shared normalization above transport (idempotency/thread keys,
  bodies, attachments, routing) lives once in `InboundMessage`. The
  `fraiseql-webhooks` receiver — previously verified-but-unmounted — is now
  mounted as the first push adapter behind the opt-in `inbound` feature:
  `POST /webhooks/{provider}` verifies the signature via the existing pipeline,
  normalizes the delivery, and persists it onto the spine
  (`_fraiseql_inbound_message`, deduplicated by `(source, idempotency_key)`)
  *inside the receiver transaction*, so persistence is atomic with the
  idempotency claim and `after:ingest` dispatch is at-least-once. Normalized
  messages fire `after:ingest[:<source>]` functions on the same I/O-capable host
  context as `after:mutation`, reusing the durable dispatch path (retry +
  dead-letter, tagged `DispatchSource::AfterIngest`). A declared routing rule
  (`resolve_routing` — dedicated address + plus-tag, e.g.
  `support+ticket-42@…` → `Ticket`/`42`) maps a message to an entity; a resolver
  function is available for free since `after:ingest` handlers receive the whole
  message. Poll-IMAP email is the first pull adapter, in a later phase.

- **`after:mutation` function dispatch is now durable (retry + dead-letter).**
  Previously fire-and-forget — a transient failure silently dropped the
  invocation — dispatch is now durable by default: a transient failure (5xx,
  timeout, execution error; a `4xx` client error is treated as permanent) is
  retried with backoff, and once retries are exhausted the invocation is pushed
  to a dead-letter queue where it is inspectable (`function_dlq_count` on the
  observer delivery-health endpoint) and replayable, so money- and send-path
  work is never silently lost. A function can opt out into fire-and-forget with
  `re_runnable = true` for re-runnable/idempotent work (e.g. LLM scoring). The
  retry policy round-trips per-function from the compiled schema
  (`FunctionDefinition.retry`), with `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS`,
  `FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS`,
  `FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS`, and `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`
  environment overrides. Reuses the observer subsystem's retry/backoff and
  dead-letter-queue machinery (shared `DispatchPolicy`, extended
  `DeadLetterQueue` trait) rather than a parallel implementation; running
  after:mutation functions (`functions-runtime`) therefore now compiles the
  `observers` subsystem. See ADR 0015 for the durable-by-default rationale.

- **TypeScript/JavaScript functions reach the full I/O-capable host surface.**
  `FunctionObserver::invoke_with_context`
  now dispatches by the module's runtime (WASM **or** Deno) instead of a hardwired
  WASM lookup, and the Deno runtime gained `invoke_with_context` plus the async
  host ops (`fraiseql_query`, `fraiseql_sql_query`, `fraiseql_http_request`,
  `fraiseql_storage_get`/`_put`, `fraiseql_auth_context`, `fraiseql_env_var`), so a
  TS `after:mutation` function can make SSRF-allowlisted outbound HTTP calls, run
  GraphQL queries, read storage and secrets, and write results back — at parity
  with WASM. Both backends share one `DynHostContext` bridge (hoisted to
  `fraiseql_functions::host::dyn_context`), so the SSRF/validation policy is defined
  once. A host op invoked without a live host context (the sync `invoke` path) fails
  loud rather than returning empty data. New opt-in server feature
  `functions-runtime-deno` builds the runtime in; the embedder registers it on the
  observer. (The isolate executes JavaScript today — TypeScript type-stripping
  transpilation is a tracked follow-up.)

- **Saga steps can pre-fetch cross-subgraph `@requires` fields under
  `saga` (#429).** A saga step may declare `RequiredField` specs
  (`SagaCoordinatorStep::with_required_fields`): before the step's mutation runs,
  each field is fetched from its owning subgraph's `_entities` endpoint (via the
  resolver configured with `SagaCoordinator::with_entity_resolver`) and merged
  into the step's mutation variables — so a step whose input depends on data owned
  by another subgraph (e.g. a step that `@requires product.price` from the catalog
  subgraph) runs correctly in a distributed saga. Sagas are runtime-constructed, so
  the application supplies these specs directly; they are persisted in a new nullable
  `required_fields` JSONB column on `tb_federation_saga_steps` (added idempotently by
  `migrate_schema`). A step that `@requires` a field from an unregistered subgraph is
  rejected at `create_saga` (fail-loud-at-setup); a field that cannot be resolved at
  execution fails the step **before** its mutation runs — a real `Failed` step that
  triggers compensation, never a mutation dispatched with missing inputs (audit H32).
  Requires the `saga` Cargo feature (opt-in, but stable and semver-covered).

- **Saga remote dispatch supports mutual TLS under `saga` (#429).**
  `SagaCoordinator::with_http_client_mtls` (backed by
  `HttpMutationClient::new_with_mtls`) configures the remote-dispatch HTTP client to
  present a client certificate and trust a configured root CA (from `MtlsConfig` /
  `MtlsMaterial`), so saga steps dispatched to a peer subgraph are mutually
  authenticated. mTLS is opt-in — `enabled: false` yields an ordinary one-way-TLS
  client — and fails loud at setup if enabled with missing or malformed certificate
  material, so the client is never silently downgraded. Requires the `saga` Cargo feature (opt-in, but stable and semver-covered).

- **Saga steps can now retry with backoff and time out under `saga`
  (#429).** `SagaExecutor` / `SagaCoordinator` gained a `RetryPolicy`
  (`with_retry_policy`): a transient step failure is retried up to `max_retries`
  times with exponential backoff — and, when `step_timeout_ms` is set, each attempt
  is bounded by a per-step timeout — before the saga gives up and its
  `CompensationStrategy` decides whether to roll back. So a flaky mutation no longer
  needlessly compensates a whole saga. The default `RetryPolicy::none()` preserves
  the original one-attempt behaviour. A retried or timed-out attempt is always a real
  failed attempt (a captured error, never a fabricated success — audit H32). Requires
  the `saga` Cargo feature (opt-in, but stable and semver-covered).

- **Saga crash recovery is now concurrency-safe under `saga` (#429).**
  `SagaRecoveryManager` now claims stuck (`Executing`) sagas via a single atomic
  `UPDATE … WHERE pk_ IN (SELECT … FOR UPDATE SKIP LOCKED)` statement, leasing each
  to the recovering worker, so two recovery workers (or a worker racing a live
  coordinator) claim **disjoint** sets and never double-drive the same saga. A
  claimed saga's lease outlives one iteration; a crashed worker's claims lapse and
  are automatically reclaimable. `tb_federation_sagas` gained nullable
  `recovery_worker_id` / `recovery_lease_expires_at` columns (added idempotently by
  `migrate_schema`; rows that predate them are always claimable). Requires the
  `saga` Cargo feature (opt-in, but stable and semver-covered).
- **Saga compensation can now roll back remote steps over HTTPS under
  `saga` (#429).** A step that executed against a registered peer subgraph
  is now compensated on that same transport: `SagaCompensator::compensate_saga`
  (and `compensate_step`) take the coordinator's subgraph registry + HTTP client
  and dispatch each completed step's inverse mutation over HTTPS via
  `HttpMutationClient` when its `subgraph` names a registered peer, otherwise against
  the local SQL adapter — so a saga that mixed local and remote forward steps rolls
  each one back on its own transport. `SagaCoordinator`'s automatic-compensation
  and `cancel_saga` paths thread the registry through. The "never fabricate a rollback"
  contract still holds (audit H33): a remote inverse that errors leaves the step
  un-compensated and the saga `PartiallyCompensated`, never a fabricated `Compensated`.
  Forward execution and compensation now share one `resolve_remote` routing helper.
  Requires the `saga` Cargo feature (opt-in, but stable and semver-covered).
- **Saga steps now persist their full mutation name for remote dispatch (#429).**
  `tb_federation_saga_steps` gained a nullable `mutation_name` column (added
  idempotently by `migrate_schema`) carrying the exact GraphQL operation name
  (e.g. `createOrder`) alongside the coarse mutation *kind*.
  `SagaCoordinator::create_saga` persists it, and both local and remote step
  dispatch now send the full name instead of the reconstructed verb (`create`), so
  a remote subgraph receives the operation it actually defines. Rows that predate
  the column (`mutation_name` `NULL`) fall back to the verb, so existing sagas are
  unaffected. This is the store-schema groundwork for remote saga compensation.
  Requires the `saga` Cargo feature (opt-in, but stable and semver-covered).
- **Saga steps can now be dispatched to remote subgraphs over HTTPS under
  `saga` (#429).** `SagaCoordinator` gained a subgraph registry:
  `with_http_client(config)` configures an SSRF-protected `HttpMutationClient`, and
  `with_subgraph(name, url)` registers a peer (validating the URL at registration —
  fail-loud-at-setup). During `execute_saga`, a step whose `subgraph` field names a
  registered peer is dispatched over HTTPS via `HttpMutationClient::execute_mutation`
  instead of the local SQL adapter; steps with no matching peer (or when no HTTP client
  is configured) fall through to the local path, so a single saga can mix local and
  remote steps. The dispatch honours the "never fabricate" contract — a remote mutation
  error (HTTP failure or GraphQL error) becomes a real failed step, never a fabricated
  success. Remote *compensation* still runs via the local executor (a remote rollback
  path is future work). Requires the `saga` Cargo feature; the API may change
  without semver guarantees.
- **Saga coordinator facade is now wired under `saga` (#429).** A new
  `SagaCoordinator` ties forward execution and compensation into a single handle over
  a `PostgresSagaStore`: `create_saga` validates and persists a saga (`Pending`) with each
  step's compensation metadata; `execute_saga` runs the forward phase and, on a step failure
  under the default `Automatic` strategy, rolls back the completed steps via
  `SagaCompensator::compensate_saga` (returning `Failed` + `compensated: true`), while
  the `Manual` strategy leaves the saga `Failed` for an operator; `cancel_saga` refuses a
  terminal saga, compensates any completed steps, then marks the saga `Cancelled` (a new
  terminal `SagaState`); and `get_saga_status` / `get_saga_result` / `list_in_flight_sagas`
  report real persisted state. Requires the `saga` Cargo feature (opt-in, but stable and
  semver-covered).
- **Saga crash recovery is now wired under `saga` (#429).** A
  `SagaRecoveryManager` background loop re-drives sagas that a crash or restart left
  in-flight: each tick finds stuck (`Executing`) and pending (never-started) sagas, records
  a recovery attempt, and replays each one's forward execution through
  `SagaExecutor::execute_saga` until it reaches a terminal `Completed`/`Failed`
  state, then cleans up stale terminal sagas. Recovery is resilient — a single saga's
  failing replay is logged and counted, never aborting the iteration — and idempotent to
  start: `start_background_loop` compare-and-swaps a running flag and rejects a
  second concurrent loop. Recovery replay is local-only (re-driving a crash-interrupted
  remote step is deferred). Requires the `saga` Cargo feature (opt-in, but stable and
  semver-covered).
- **Saga compensation (rollback) is now wired under `saga` (#429).** When a
  distributed saga fails partway, `SagaCompensator::compensate_saga` rolls back the
  already-completed steps in strict reverse execution order by executing each step's
  registered compensation (inverse) mutation through the local SQL adapter, then persists
  a real `Compensated` step state — never a fabricated rollback (audit H33). Compensation
  is best-effort: a step whose inverse fails, or that has no compensation registered,
  leaves the saga `Failed` and is reported `PartiallyCompensated` rather than marking the
  saga `Compensated` having undone only part of its work. The saga step schema gained
  nullable `compensation_mutation` / `compensation_variables` columns (added idempotently
  by `migrate_schema`; rows that predate them read back as `None`). Requires the `saga`
  Cargo feature (opt-in, but stable and semver-covered).
- **Short-lived runtime executor: `fraiseql query` + `doctor --runtime` (#501).** A
  scriptable way to exercise a compiled schema against a database without standing up
  the long-lived server, closing the gap between "static checks pass" and "the server
  resolves it". `fraiseql query --schema schema.compiled.json --db "$DATABASE_URL"
  '{ orders(limit: 1) { id } }'` boots the engine in-process (PostgreSQL; no HTTP
  layer, no `run-server` feature), runs one operation, prints the GraphQL JSON to
  stdout, and exits non-zero on a resolution error — a one-shot "does this resolve?"
  for CI and the inner loop. `--variables '{...}'` passes GraphQL variables.
  `fraiseql doctor --runtime --against-db "$DATABASE_URL"` probes every root query
  field with a minimal selection (and dry-runs no-argument mutations), reporting each
  as resolving or failing; operations that require arguments are skipped with a warning.
  Mutations run by `query` COMMIT unless `--dry-run` is given, which executes the
  mutation inside a transaction that is **rolled back** — the function binds and runs
  (constraints, triggers, and the `mutation_response` shape are all validated) but no
  writes persist. Dry-run is PostgreSQL-only; other adapters return a clear
  "not supported" error rather than silently committing.

### Breaking

- **Cascade is now typed, selection-gated, and enforced (cascade hardening).**
  Cascade was previously injected verbatim into *every* mutation response, unrequested
  and undeclared. Now: (1) only mutations with `cascade=True` expose it, and they
  return a payload wrapper — `createUser { id }` becomes
  `createUser { entity { id } cascade { … } }`; (2) cascade is present only when the
  client selects it; (3) cascade entities are projected to camelCase (a cascade entity
  that used to arrive with snake_case keys like `author_id` now arrives as `authorId`);
  (4) a non-cascade mutation no longer surfaces a `cascade` blob even if its function
  returns one; (5) the cascade JSON the DB function returns must use the spec-nested
  shape (`updated: [{__typename, id, operation, entity}]`, `deleted: [{__typename, id,
  deletedAt}]`) — use the shipped `fraiseql.build_cascade` builders. No production
  consumers existed, so this affects no shipped deployment.

### Changed

- **Inbound email config renamed: `[imap.<name>]` → `[mailbox.<name>.imap]` (breaking).**
  A connected mail account now has one section, `[mailbox.<name>]`, carrying both its
  poll-IMAP *receive* half (`[mailbox.<name>.imap]`) and its SMTP *send* half
  (`[mailbox.<name>.smtp]`, consumed by the new `send_email` host op). Either half is
  optional. **Migration:** move each `[imap.foo]` section to `[mailbox.foo.imap]` and each
  `[[imap.foo.routing]]` to `[[mailbox.foo.imap.routing]]`. Only affects the opt-in
  `inbound-email` feature. See `docs/architecture/inbound-email.md`.

- **The distributed saga subsystem is now stable (#429).** The `unstable-saga`
  Cargo feature has been renamed to **`saga`** and its API is now covered by semver
  (it will not change without a major version bump). It remains an opt-in feature so
  that builds which do not orchestrate cross-subgraph transactions are not forced to
  compile the Postgres saga store and its dependencies — when the feature is off, the
  saga types are not compiled at all. This closes the #429 saga journey: forward
  execution (v2.9.0), the gated round-trip — compensation, recovery, coordinator,
  remote dispatch (v2.11.0) — and the hardening train (full remote mutation names,
  remote HTTPS compensation, concurrency-safe `SKIP LOCKED` recovery, mTLS,
  retry/backoff + per-step timeout, and `@requires` cross-subgraph pre-fetch). The
  public handle is `SagaCoordinator` (`new` / `create_saga` / `execute_saga` /
  `cancel_saga` / `get_saga_status` / `get_saga_result` / `list_in_flight_sagas`).
  **Migration:** replace `features = ["unstable-saga"]` with `features = ["saga"]`.

- **The native functions runtime is now stable (native-runtime hardening).** The
  `functions-runtime`, `functions-runtime-deno`, `inbound`, and `inbound-email`
  features graduate from opt-in-and-maturing to **stable and semver-covered** (their
  APIs will not change without a major version bump), mirroring the saga promotion.
  They keep their names and stay **opt-in** so the default binary stays lean — V8
  (~30 MB) is only compiled with `functions-runtime-deno` — and the stock server
  binary mounts the runtime and the after:mutation dispatch hooks at serve time when
  the feature is enabled. This closes the native-runtime hardening train: real
  TypeScript type-stripping, the `send_email` host op, a verified sending address,
  a host-provided idempotency token, the delivery-feedback loop, and permanent-error
  tagging are all delivered. **Scope note:** the delivery-feedback surfaces (per-send
  VERP Return-Path correlation, the suppression-store schema, the
  `POST /api/email/suppress[ion]` admin API, and the send-status lifecycle) landed
  immediately before this release and have not yet met a real bounce, a provider's
  actual plus-addressing, or a live challenge-response; they are stable-*tracked* but
  may evolve through v2.12 as beta feedback lands. See
  `docs/architecture/native-runtime-ergonomics.md`.

### Removed

- **The legacy fail-loud saga placeholders are removed (#429).** The former
  always-compiled `SagaCoordinator` stub and the `SagaExecutor::{execute_step,
  execute_saga, get_execution_state}`, `SagaCompensator::{compensate_step,
  compensate_saga}`, and `SagaRecoveryManager::{run_iteration, start_background_loop}`
  stub methods only ever returned `SagaStoreError::NotImplemented`. They are deleted;
  the real store-backed implementations now carry those clean names (behind the `saga`
  feature) — `SagaCoordinator` is the wired coordinator, and the executor / compensator
  / recovery-manager methods dispatch real mutations. Nothing functional depended on
  the placeholders (they never succeeded), so no working code changes.

### Fixed

- **Native-column `WHERE` filters no longer 500 on a camelCase argument (#540).**
  A filter argument on a native (real-column) field emitted the camelCase argument
  name verbatim as the SQL column — `comments(postId: …)` became `WHERE postId = …`,
  a 500 on the non-existent column. Both native-column emission paths now recase the
  argument to the `snake_case` column (`postId` → `post_id`), matching the JSONB
  filter path.
- **Federation `HttpMutationClient` now infers `Float!` for fractional variables
  (#429).** `build_variable_definitions` typed every JSON number `Int!`, so a mutation
  variable carrying a fractional value (e.g. a price `9.99`) produced `$price: Int!` and
  the remote subgraph rejected the variable. Fractional numbers are now typed `Float!`;
  whole numbers stay `Int!`.
- **Introspection policy is now enforced on the GraphQL execution path.** The
  `IntrospectionEnforcer` existed but was never wired in, so a `disabled`/role-gated
  introspection policy had no effect over GraphQL (#453). It is now applied in
  `execute_graphql_request` from a single `IntrospectionPolicy::from_config` source of
  truth; a rejected introspection query returns HTTP 200 with a GraphQL error in
  `errors[]` (new `ErrorCode::IntrospectionDisabled`), never a 5xx. The detector was also
  switched from substring matching to AST inspection so legitimate queries — in
  particular the `__typename` meta-field — are never misclassified as introspection and
  blocked (#454). (#453, #454)
- **Federation `_entities` no longer drops the first field of each entity on minified
  gateway queries.** Federation gateways (Hive Router, Apollo Router) routinely send
  subgraph `_entities` queries minified — no spaces around the type condition or the
  selection braces (`...on User{name email}`). The hand-rolled selection scanner did
  not flush the pending token when a `{` opened the inline-fragment body, so the type
  condition fused onto the first field (`User` + `name` → `Username`), and it failed to
  skip the type name after the minifier-merged `...on` token. The first requested field
  of every federated entity was projected under a mangled response key — a non-existent
  column — and silently returned as `null`. The scanner now flushes on `{` and skips the
  type condition for both `... on Type` and `...on Type`; pretty-printed queries are
  unaffected. The `_entities` projection logic added in #504/#507 was already correct —
  only the parser feeding it was wrong. (#512)
- **Federation `_entities` resolves owner-split `extends` entities from a type-level
  `sql_source`.** An `extend type … @key` entity resolved in a subgraph that does not
  own it exposes no root query, so the `_entities` resolver had no backing query to
  source its relation from and fell back to the non-existent `lower(typename)`
  relation — silently resolving the entity to `null`. When the authoring SDK emits a
  type-level `sql_source` on such an entity, the compiler now carries it through to
  `types[].sql_source` (instead of always leaving it empty — chosen over the federation
  config block because `types[]` survives the TOML federation merge), and the resolver's
  backing-source builder (`CompiledSchema::entity_sources`) falls back to it when no
  query supplies the relation. The fallback honours the same jsonb/flat convention as
  the query path — a non-empty `jsonb_column` projects `<col>->'<field>'`, an empty one
  reads bare columns — and the compiler defaults an extends entity's `jsonb_column` to
  the standard `data` view shape (so flat-column extends entities are authored with an
  explicit empty `jsonb_column`). Owned, query-backed entities are unaffected: a
  query-sourced relation always wins. Completes the runtime/compiler half of the #504
  fix; the authoring SDKs must emit the type-level `sql_source` for owner-split
  `extends` entities. (#507)
- **Federation `_entities` now resolves view-backed, jsonb-`data` entities.** The
  runtime `_entities` resolver built its `FROM` relation from the lowercased GraphQL
  type name and selected bare columns, but FraiseQL entities are view-backed and
  expose their fields inside a `data` jsonb column — so it queried a relation that
  does not exist and could not read jsonb fields. The query errored and the gateway
  silently turned the field into `null`, making cross-subgraph entity joins
  non-functional for standard entities. The resolver now sources each entity's
  backing relation **and** jsonb column from the compiled schema's backing query
  (keyed by `return_type`; the compiler leaves `types[].sql_source` empty), reads
  from that relation (schema-qualified names quoted segment-by-segment), and
  projects each requested field as `data->'<snake(field)>'` with camelCase→snake
  recasing and type fidelity — mirroring the normal query path. The `@key` lookup
  matches `data->>'<snake(key)>'` for jsonb views, and falls back to a text-cast
  flat-column comparison (`id::text IN ($1)`) for flat-column views, so `uuid` /
  integer / text keys all match. (#504)

  Owner-split `extends` entities (resolved in a subgraph that does not own the type,
  and so have no backing query) are now resolved from a type-level `sql_source` the
  compiler carries through to the entity's `TypeDefinition` — see the #507 entry above.
- **HTTP GraphQL now populates `SecurityContext.roles` from the JWT `role`/`roles`
  claim, so `requires_role` operations are reachable over HTTP.** `SecurityContext::from_user`
  hard-coded `roles: vec![]` (`// Will be populated from JWT claims`) and nothing ever
  filled it, so every `requires_role` / `read_role` / `write_role`-gated operation was
  unreachable over the HTTP path regardless of the bearer token — the enumeration-safe
  `Query '<name>' not found in schema` response made the cause non-obvious. `from_user`
  (the common chokepoint for every auth transport — OIDC, HS256, gRPC, MCP — alongside
  the existing actor classification) now derives `roles` from the signature-verified
  `role` (scalar), `roles` (array) and `fraiseql_roles` (array) claims, sorted and
  de-duplicated, matching the claim names already honoured by the observer-admin RBAC
  engine. The claims continue to be forwarded into `attributes` for RLS / session-var
  injection — the two surfaces are independent. (#503)
- **Federation SDL: scalar names are consistent and `*WhereInput` types are valid.**
  Two residual `_service` SDL gaps that the type-closure fix exposed: (1) the `scalar`
  declaration walk canonicalised names (`DateTime`) while fields rendered them verbatim
  (`datetime`), so the reference dangled (`Unknown type datetime`); declarations are now
  collected from the exact rendered field names (covering `LTree`, `FloatRange`, lowercase
  date/time scalars). (2) the generated rich-filter `*WhereInput` types declared `eq`
  twice — the standard operator set was hardcoded and the type's operator set (which also
  contains `eq`) was appended — producing an invalid input type (`Field eq already
  exists`); the generated fields are now de-duplicated by name. A FraiseQL subgraph's SDL
  now composes through a gateway with zero consumer-side workarounds.
- **Federation `_service` SDL is now type-complete.** Building on the root-operations
  fix, the generated SDL rendered only object types and the root `Query`/`Mutation` —
  it omitted input objects, enums, non-built-in scalar declarations (`DateTime`,
  `JSON`, `Decimal`, rich/custom scalars), and synthesized mutation result unions. A
  gateway composing a subgraph whose operations referenced those types failed with
  `Unknown type CreateQuoteInput` (and the like). `raw_schema()` now renders the full
  type closure — `scalar`/`enum`/`interface`/`input`/`union` declarations alongside the
  object types and root operations — so a FraiseQL subgraph composes without
  consumer-side scalar stubs. The federation `@link` directive definition was also
  corrected (`for: String` → `for: link__Purpose`, with the supporting `link__Purpose`
  enum and `link__Import` scalar), which composers were rejecting.
- **Federation `_service` SDL now advertises root operations.** A subgraph's
  `_service { sdl }` (and the `/schema` SDL endpoint) is generated from
  `CompiledSchema::raw_schema()`, which only rendered object types — FraiseQL keeps
  root operations in `queries`/`mutations`, not as `Query`/`Mutation` object types, so
  the emitted SDL exposed **no root fields**. A gateway composing that SDL (Apollo
  Router, Hive Router — both read `_service.sdl`) failed with `NO_QUERIES`, making
  FraiseQL subgraphs uncomposable despite advertising their entities and `@key`s. The
  generated SDL now renders the root `type Query`/`type Mutation` from `queries`/
  `mutations` with correct argument and list/nullable signatures, so subgraphs compose
  into a working supergraph. (Router-independent; the fix is upstream of the gateway.)
- **`fraiseql compile` no longer drops the `federation` block.** Schemas authored
  with federation enabled (e.g. the Python SDK's `export_schema(federation=...)`)
  emit the configuration under the top-level `federation` key, but the compiler only
  bound `federation_config` — so the legacy JSON workflow silently produced a
  non-federated `schema.compiled.json` (`jq 'has("federation")'` ⇒ `false`), and the
  server it loaded never advertised itself as a subgraph (`_service` / SDL absent).
  `IntermediateSchema.federation_config` now also binds the `federation` key (serde
  alias), so the block carries through to `CompiledSchema.federation` and the
  compiled subgraph serves a proper Apollo Federation v2 `_service { sdl }`. The TOML
  (`[federation]`) workflow already carried through the merger and is unchanged. As a
  guardrail against a future silent regression, the legacy JSON path now fails the
  compile if a non-empty input `federation` block does not bind into the schema.
- **TOML `[federation]` now carries the subgraph identity.** The `[federation]`
  section gained `service_name`, `version` (Apollo spec string, e.g. `"v2"`), and
  `schema_url`; the legacy integer `apollo_version` is still accepted (`2` ⇒ `"v2"`,
  and `version` wins when both are set). The merger now lowers the TOML config into the
  compiled shape explicitly, so `service_name`/`version` and per-entity circuit-breaker
  overrides (`[[federation.circuit_breaker.per_database]]` ⇒ runtime `per_entity`) reach
  the runtime instead of being silently dropped on a field-name mismatch.

### Security

- **Field-level authorization (#423) now resolves inline fragments.** The gated-field
  detector matched selection field names literally, so a policy-gated field selected
  through an inline fragment — `{ users { ... on User { ssn } } }` — was invisible to the
  gate check: the field authorizer was skipped for it while the projector still resolved
  the fragment. A gated field wrapped in a fragment could therefore evade the per-row
  authorizer. Present since the field authorizer shipped (v2.5.0). The detector
  (`selection_set_selects_gated_field` / `collect_top_level_gated_fields` /
  `selection_set_has_nested_gated_field`) now resolves `... on T` fragments via
  `effective_selections` before matching; the fix is over-enforcement-safe (it can only
  add authorization checks, never remove them). Regression-tested at the detector level
  and end-to-end through the cascade authz suite. This surfaced while wiring per-entity
  authorization for cascade entities (which are interface-typed and always fragment-selected).

- **RLS deployments must make `sql_source` views `security_invoker` (deployment-dependent
  hardening).** A *default* PostgreSQL view runs with the view owner's privileges and
  bypasses the querying role's Row-Level Security, so any RLS deployment whose view owner
  differs from the querying role silently leaks cross-tenant rows on ordinary reads (and in
  mutation cascades, which read the same views). This is a property of PostgreSQL views, not
  a code regression, but it is now called out: create each `sql_source` view
  `WITH (security_invoker = true)` (PG 15+), the view-authoring docs document it, and
  `fraiseql doctor --against-db` warns when a view lacks it while RLS is in use.

## [2.10.0] - 2026-06-26

### Added

- **Opt-in fail-fast `sql_source` validation (#487).** A declared-but-unbacked
  `sql_source` (a query view or mutation function that doesn't exist) used to surface
  as an opaque per-request 500 while the server booted "healthy" — a bug an agent only
  discovers by hitting it. Two new gates close that gap, both fed by the shared
  `fraiseql_core::schema::sql_source_probes` work-list so they agree on "backed" by
  construction:
  - **CLI** — `validate --against-db` now runs an existence pass after the #397
    mutation-contract check, printing a precise list of unbacked sources and **exiting
    non-zero** when any are found (a CI/pre-push gate). Resolution mirrors the runtime
    (qualified relations via `to_regclass` verbatim, functions via `pg_proc`).
  - **Server** — an opt-in boot check (`validate_sql_sources` config key,
    `--validate-sql-sources` flag, or `FRAISEQL_VALIDATE_SQL_SOURCES` env var; env/flag
    win over the config key). **Default OFF** — boot is unchanged unless enabled.
    Postgres-only; when on, an unbacked source fails boot with the precise list instead
    of a later per-request 500. Once `validate --against-db` is wired into CI this
    subsumes the existence half of the bespoke `check_management_drift.py` gate.
- **`after:mutation` function triggers now dispatch after commit (#460).** An
  `after:mutation:<entity>:<op>` function trigger was parsed, registered, and surfaced in
  the `TriggerRegistry`, but the server had zero call sites for
  `find_after_mutation_triggers` — a registered after-mutation function silently never ran,
  while `before:mutation` already ran inline on the request path. The server now plans and
  dispatches matching triggers after a mutation commits (new always-compiled
  `routes::after_mutation` module with a pure, unit-tested `plan_after_mutation_dispatch`),
  bringing `after:mutation` to parity with `before:mutation`. Invocation is gated on the
  `runtime-wasm` feature via a new `FunctionObserver::invoke_with_context`, which runs the
  function on a full I/O-capable host context rather than the sync-only snapshot.
- **Per-subscription webhook `signing_secret` literal (#467).** The webhook action could
  previously sign only with `signing_secret_env` — the *name* of a process environment
  variable — so a DB-backed / admin-API-managed observer store (`tb_observer`) could not
  carry a distinct HMAC secret per subscription, collapsing multi-tenant webhook signing to
  one shared process secret. An optional `signing_secret` literal now sits alongside
  `signing_secret_env` (mirroring the existing `url` / `url_env` pattern and rounding
  through the `actions` JSONB), so each dynamically-managed subscription can sign with its
  own key while the static/config case keeps using the env-var form.
- **`doctor --against-db` now reports mutation→function contract drift (#384).** The "does
  every declared mutation have a callable backing function with a `mutation_response`-shaped
  return?" check previously lived only in `validate --against-db`; a declared mutation with
  no (or a mismatched) backing function surfaced only as a runtime "failed to prepare",
  never at check time. `doctor` now runs the same static check (reusing the
  `validate_mutation_contract` engine), so the single live-DB pass reports change-log / RLS
  / grant / capture drift, PL/pgSQL body resolution, **and** mutation→function contract
  drift.

### Fixed

- **`doctor --against-db` no longer emits two spurious reds on the Python-SDK flow (#488).**
  (a) Connectivity reported "DATABASE_URL not set" even when `--against-db` connected fine,
  because `run_with_db_checks` dropped `against_db` before the connectivity checks. The
  effective URL (`--db-url` > `--against-db` > env) is now threaded through, so an
  `--against-db`-only run reports the URL as set/reachable. (b) "TOML syntax valid" failed
  because `--config` (a *runtime* config) was always parsed as a *schema-definition* TOML;
  it is now syntax-only-parsed when `--schema` resolves to a compiled JSON, and only
  schema-parsed when `--schema` is itself a `.toml` source. Malformed TOML and unreachable
  databases still fail — the spurious reds are gone without masking real problems.
- **`compile --database` no longer emits false "`sql_source … does not exist`" (#485).**
  Two false-positives made the existence gate untrustworthy: (1) **every mutation** was
  probed as a *relation* (`list_relations`, `search_path`-scoped) when a mutation's
  `sql_source` is a *function* — so a perfectly-backed mutation reported `MissingRelation`;
  and (2) a **schema-qualified view in an off-`search_path` schema** (or a mixed-case one)
  was probed against the same `search_path`-scoped relation map and case-folded, so a view
  the runtime serves was flagged missing. Mutations are now probed via `pg_proc`
  (new `MissingFunction` diagnostic, distinct from `MissingRelation`); schema-qualified
  relations are resolved **verbatim** with `to_regclass(quote_postgres_identifier(src))` —
  exactly how the runtime resolves identifiers — so case-sensitive / off-path relations
  resolve correctly while genuinely-absent ones are still reported. The L3 JSON-key check
  now uses the canonical acronym/digit-aware caser (`dns1Id` → `dns_1_id`, not `dns1_id`),
  so acronym/digit fields no longer false-flag `MissingJsonColumn`. A new
  `fraiseql_core::schema::sql_source_probes` defines "what counts as a backed source" once,
  shared by the CLI gate and (forthcoming) the server boot check so they cannot drift.
- **`doctor`/`validate --against-db` no longer false-fail single-JSONB mutations (#484).**
  The #384 mutation→function contract check derived the function's *expected* arity by
  flattening the `input` type's fields whenever the operation was not `Update` — so every
  `Insert`/`Delete`/`Custom` mutation on the single-JSONB convention (`fn(p_input jsonb)`,
  authored with `input_style = jsonb`) reported a spurious
  `expected N argument(s) but the function takes [1]`, making the gate unusable as a
  green-means-green CI check. `expected_call` now mirrors the runtime's single-JSONB
  predicate exactly (`mutation/mod.rs:499-500`): a structured single `input` arg is
  expected as one `jsonb` payload when the op is `Update`, **or** `input_style == jsonb`,
  **or** the input type is absent from the schema — and the arg-1-is-`jsonb` assertion now
  applies to all of those. Genuinely-wrong functions (wrong arity, non-`jsonb` payload)
  are still caught. The fix lands on both `doctor --against-db` and `validate --against-db`
  from one change (shared `mutation_contract` module).
- **Multi-word camelCase query filters no longer silently return `[]` (#486).** On the
  query path, an explicit filter argument (`orders(organizationId: "x")`) built its
  JSONB predicate from the raw camelCase name — `data->>'organizationId'` — which never
  matches the stored `organization_id` key, so the filter silently dropped to an empty
  result instead of erroring. The JSONB key is now `snake_case`d with the same
  acronym-aware caser the WHERE-input and mutation-input paths use
  (`crate::utils::to_snake_case`), so `organizationId` → `organization_id`,
  `dns1Id` → `dns_1_id`. This closes the class across every arg-shaped filter surface:
  explicit args + inject params (`query_params`), aggregate `where` and `groupBy`
  fallback dimensions, window `where`, and the EXPLAIN diagnostics
  (`build_where_from_variables` / display SQL). For `groupBy`, only the JSONB extraction
  *path* is recased — the result *alias* keeps the camel surface name so the GraphQL
  response key is unchanged (the #418/#410 rule). The query-side analog of the merged
  #456 mutation-input fix; a six-surface parity test fences the class against future
  filter surfaces that forget to recase.
- **Observer execution log now populates its request/response audit columns (#468).**
  `tb_observer_log` declares `action_index`, `action_type`, `response_status_code`,
  `response_payload`, and `request_payload`, but the runtime log writer left them
  `NULL` — it recorded only status/duration — so a webhook delivery-log / audit UI
  built on the schema could not show response codes or bodies. The per-action result
  (HTTP status, action type/index, outcome) is now threaded from the
  `fraiseql-observers` executor back to the writer via a new
  `ExecutionSummary.action_details` / `ActionExecutionDetail` contract and written to
  the log. `action_type`, `action_index`, `response_status_code`, and the
  `response_payload` outcome summary are non-sensitive and always populated;
  `request_payload` (the triggering event data, which can carry PII) is gated behind
  the new `[observers.runtime].log_payloads` flag (default off) and truncated to a
  marker past 64 KiB. Part of the #471 "declared but unwired" cluster.
- **PostgreSQL `ENUM` columns no longer decode to null in `row_to_map` (#472).** An
  enum-typed column (notably `app.mutation_response.error_class`, the
  `app.mutation_error_class` enum) fell through the `row_to_map` decode ladder to
  `Null` because `String`'s `FromSql` rejects a custom enum OID. On a *failed*
  mutation this dropped `error_class` to absent, and the parser then rejected the
  whole row with `succeeded=false requires error_class` — so the typed error arm was
  never reached and the failure surfaced as an opaque validation error. `row_to_map`
  now decodes any enum to its text label (a small `FromSql` wrapper keyed on
  `Kind::Enum`), generalising to every enum-typed column. Same class of latent
  null-drop as the earlier `SMALLINT`/`http_status` and `TEXT[]`/`updated_fields`
  (#228) fixes.
- **Failed mutations now resolve onto the declared error type, not the success arm
  (#465).** When a `mutation_response` reported failure (`succeeded = false`), the
  GraphQL projection resolved the result type *only* via `find_union(return_type)`.
  If that found no `is_error` member — e.g. the mutation returns the bare success
  entity with sibling error types declared (the shipped `04-error-type` pattern), or
  any schema where the result union is not the mutation's compiled return type — the
  error arm fell through to emitting `__typename = <success/return type>` and
  projected no error fields. A client selecting `... on MutationError { … }` then
  received nothing on that arm and `__typename` named the entity. The error arm now
  mirrors the success arm: it resolves the concrete error type from the response's
  `entity_type` column (the declared error type the function stamps, e.g.
  `DuplicateEmailError`) when it names an `is_error` type, falling back to the return
  union's `is_error` member otherwise. This both fixes the mis-routing and, for unions
  with several error members, projects the *specific* error the function reported
  rather than the first one in the union. Success-path projection is unchanged.
- **A SQL-`NULL` `updated_fields` no longer breaks mutation parsing (#473).** A failed
  mutation's function commonly leaves `mutation_response.updated_fields` (a `TEXT[]`)
  unset, which `row_to_map` renders as JSON `null`. The parser's `#[serde(default)]`
  only fills an *absent* key, so an explicit null reached `Vec<String>`'s deserializer
  and failed with `invalid type: null, expected a sequence` — turning the failed
  mutation into an opaque parse error before the typed error arm was reached. A null
  `updated_fields` column is now read as the empty list, matching the absent-key
  behaviour. No function-side `updated_fields := ARRAY[]::text[]` boilerplate required.
- **Root `{ __typename }` resolves to the operation type name (#450).** A top-level
  `{ __typename }` — the canonical zero-cost GraphQL health probe — was rejected with
  `Query '__typename' not found in schema` because the query classifier only
  special-cased `__schema`/`__type`. Per the GraphQL spec, `__typename` is a
  meta-field available at every selection set; at the root it resolves to the
  operation's root type name (`Query`/`Mutation`/`Subscription`). It now resolves with
  no database round-trip — including when aliased (`{ ping: __typename }`), mixed with
  real fields (`{ __typename users { id } }`, either order), repeated
  (`{ a: __typename b: __typename }`), on a `mutation`, and under `@skip`/`@include`.
  (Fragment-wrapped root `__typename` is not yet special-cased.)
- **PostgreSQL statement-prepare failures surface the real diagnostic (#451).** A
  failed prepare previously surfaced the opaque `Failed to prepare statement: db error`,
  dropping the PostgreSQL diagnostic (e.g. `function "foo" does not exist`,
  SQLSTATE 42883) that names the offending object — making server↔database contract
  drift undiagnosable from logs. The diagnostic is now extracted into the message (the
  SQL state was already populated); client-facing error sanitization is unchanged.
- **Mutation input keys are now recased to snake_case across every transport (#456).** A
  camelCase `input` payload reached the SQL function verbatim on several mutation paths, so
  a function reading `p_input->>'snake_field'` silently saw `NULL` instead of erroring. Root
  cause: the compiler emits an input-type argument as `FieldType::Object(name)` — never
  `FieldType::Input`, which the compile pipeline never constructs — so the runtime's
  single-JSONB / flatten detection, which keyed only on `FieldType::Input`, fell through to
  the verbatim-forward path. The single-JSONB path now recognizes a single `input` arg of
  `FieldType::Object(<registered input type>)` and recases its keys with the engine's
  acronym-aware `to_snake_case` (`s3Key`→`s3_key`, `dns1Id`→`dns_1_id`), matching the read
  path so writes round-trip exactly as reads. The gRPC mutation path
  (`execute_grpc_mutation`), which bypassed `recase_input_payload`, now applies the same
  recursive recasing, and the SQLite DirectSql `direct_columns` list is recased too.
  TomlSchema's `naming_convention` now defaults to `CamelCase` (matching the JSON-schema
  compile path; `preserve` still opts out explicitly), and a new compile-time warning flags
  a `preserve` schema whose single-JSONB mutation takes a camelCase-looking declared input
  type.
- **Observer admin CRUD writes now refresh the in-process matcher (#466).** The
  `/api/observers` admin API persisted create/update/delete/enable/disable writes to
  `tb_observer` but never refreshed the in-process `EventMatcher`, so an edit was a silent
  no-op until a separate `runtime/reload`, a listener restart, or a periodic poll. The
  success arm of each write now invokes the existing atomic `ObserverRuntime::reload_observers()`
  (`ArcSwap`) through a new optional runtime handle on `ObserverState`. The same change also
  repairs a `reload_observers()` failure on `actions` JSONB that stored `"headers": null`
  (the server API struct serialized `None` headers as explicit `null`, which the runtime
  struct's deserializer rejected with `invalid type: null, expected a map`); the runtime
  reader now maps an explicit `null` to an empty map, fixing existing rows too.
- **Startup now warns when `[observers]` is configured but the feature is not built (#469).**
  When `fraiseql-server` is built without the optional `observers` feature, a populated
  `[observers]` config section was silently parsed-and-discarded by serde — the runtime
  never started and `/api/observers` returned 404 with no signal as to why (the
  `ServerConfig.observers` field is `#[cfg(feature)]`-gated, so the deserialized config
  cannot reveal the discrepancy). The server now re-reads the raw TOML at startup and emits
  a clear, actionable warning when a top-level `[observers]` table is present on a binary
  built without the feature.

## [2.9.0] - 2026-06-22

### Security

- **Demo / example / test deployment artifacts hardened (#436).** Follow-up to the
  Phase 13 production deploy sweep, covering the demo/example/test residue the
  production gate (`tools/check-deploy-security.sh`) does not guard. Docker port
  publishing bypasses host firewalls, so every backing-service port in the demo and
  test compose stacks (`docker/docker-compose.{prod,prod-examples,demo,examples,test}.yml`,
  `docker/tls-postgres/`, `examples/ecommerce_api/`, `examples/async-jobs-subgraph/`,
  `examples/observability/`) is now bound to `127.0.0.1` — still locally usable for
  demos and CI, no longer reachable from the network. Weak literal passwords became
  overridable env vars with a documented demo default
  (`${POSTGRES_PASSWORD:-…}`) so the stacks still start out of the box while honouring
  an override. The two misnamed `docker/docker-compose.prod*.yml` files now carry a
  header clarifying they are local demo stacks, not production templates. Dockerfile
  hardening: `tutorial/Dockerfile` and the two `examples/async-jobs-subgraph/*`
  images now drop to a non-root `USER`; the federation example images pin
  `FROM rust:latest` → `rust:1.92`; the `fraiseql-wire` CI test fixture gained a
  comment marking its deliberately-open settings as test-only. Loki/Grafana/nginx
  monitoring artifacts gained comments documenting their no-auth / TLS-terminated-
  upstream assumptions. No production artifact changed; the deploy-security gate
  still passes.
- **Example apps no longer ship insecure patterns users copy to prod (#438).**
  Sibling of #436, covering the example *application* code (not the deploy
  artifacts). Four findings fixed: (1) `examples/multitenant/fraiseql.toml` had no
  `[security]` block, so `listTenants`/`listResources` returned every tenant's rows
  to any anonymous caller — added `default_policy = "authenticated"` plus per-tenant
  row-scoping rules (mirroring `examples/saas/`) and a "Tenant isolation" note in the
  README. (2) `examples/ecommerce_api/.../customer_functions.sql` `register_customer()`
  stored the plaintext password into `password_hash`; it now bcrypt-hashes via
  `crypt(p_password, gen_salt('bf'))`, and the init migration creates the `pgcrypto`
  extension. (3) `examples/async-jobs-subgraph/router/router.yaml` dropped
  `allow_any_origin: true`, which silently overrode the `origins:` allow-list above
  it. (4) `examples/analytics_dashboard/` and `examples/cascade-create-post/` were
  re-pinned off the legacy v1 `fraiseql[fastapi]==1.8.1` to the v2 line
  (`fraiseql==2.8.0`, no v2 `fastapi` extra) and gained a `requirements.lock`.
- **Row-Level Security on the change-spine change-log — BREAKING (#437 F6 / #443).**
  `core.tb_entity_change_log` holds the full before/after payload for every tenant,
  and until now any database role with `SELECT` on the table or its views
  (`core.v_entity_change_log`, `core.v_entity_change_log_debezium`) could read all
  of them — the contract called `tenant_id` an "RLS partition stamp" but RLS was
  never enabled. Migration `12_enable_change_log_rls.sql` turns it on: the table is
  now **deny-by-default** (a role that is neither owner nor `BYPASSRLS`, and has not
  set the `fraiseql.tenant_id` GUC, reads zero rows), with a forward-looking
  per-tenant SELECT policy and a permissive INSERT policy (the executor outbox + the
  now-`SECURITY DEFINER` capture function stamp the tenant). The two views are
  flipped to `security_invoker = true` (PostgreSQL 15+) so they enforce the
  base-table RLS instead of bypassing it as the view owner; on PostgreSQL < 15 the
  migration warns and the views must be access-restricted to trusted roles. The
  capture function `core.fn_entity_change_log_capture()` is now `SECURITY DEFINER`
  with a pinned `search_path = pg_catalog, core`, so external-write capture keeps
  working under RLS. The migration also `REVOKE ALL … FROM PUBLIC` on the table and
  both views (least-privilege baseline — the change-log is never world-readable, so
  RLS is genuine defence-in-depth rather than the sole control). A new
  `fraiseql doctor --against-db` check warns when RLS is enabled on the change-log
  but the connecting role is neither the table owner nor `BYPASSRLS` — catching the
  silent-empty-pipeline footgun before it bites in production. **Operator action
  (BREAKING):** the change-log consumers (poller, the 3 NATS bridges, the server
  changelog HTTP handlers, the mutation executor outbox) all run on the server's
  database role — that role must be the table owner or carry `BYPASSRLS`, otherwise
  the CDC pipeline and the admin change-log query silently return empty. FraiseQL
  does not set `fraiseql.tenant_id` on its read paths today, so the practical effect
  is deny-by-default; per-tenant GUC filtering is forward-looking. MySQL / SQL Server
  change-log isolation is a tracked follow-up.
- **Per-tenant GraphQL operation cost budgets (#379).** `max_query_depth` and the
  complexity limit stop naive recursion, but not an expensive within-depth query. A new
  per-tenant `cost_budget` (on `TenantQuota`, settable via the tenant admin API) rejects a
  request whose estimated cost exceeds the tenant's budget at the same chokepoint as the
  rate/concurrency quotas (HTTP 429). The cost reuses the existing complexity score
  (`estimate_query_cost`); a root operation listed in `[fraiseql.cost_weights]` counts as
  its manual `@cost` weight instead of its walked subtree, letting operators pin the cost
  of a known-expensive query. Off by default (no budget configured ⇒ unlimited).
- **`[security] persisted_queries_only` shorthand (#379).** A single top-level flag that
  forces the trusted-document store into `strict` mode — reject any operation that is not a
  persisted/trusted document — regardless of the declared `[security.trusted_documents].mode`.
  Equivalent to setting `mode = "strict"`, but expressed as one operator-facing toggle. It
  only takes effect when a trusted-documents manifest is configured (there must be persisted
  operations to allow-list); the server logs a warning if the flag is set without an enabled
  manifest so it never fails silently. Off by default.
- **The production Docker Compose no longer exposes backing services to the network
  (H46).** `docker-compose.prod.yml` published PostgreSQL (`5432`), Redis (`6379`), and
  Prometheus (`9090`) on `0.0.0.0` — and because Docker's port publishing inserts its own
  `iptables` rules ahead of the host firewall, those services were reachable from the
  internet regardless of any `ufw`/firewall policy. Redis additionally ran with no
  password (`protected-mode` is off once a port is published), so anyone who could reach
  it had full command access. The backing-service ports are now bound to `127.0.0.1`
  (containers still reach each other by service name over the bridge network, and host
  loopback access remains for local admin/migration tooling), Redis now requires
  `--requirepass ${REDIS_PASSWORD}`, and `${DB_PASSWORD}`/`${REDIS_PASSWORD}` use a
  fail-loud `:?` guard so an unset secret aborts startup instead of silently creating a
  passwordless database. The root dev `docker-compose.yml` backing services were likewise
  rebound to loopback. A static gate (`tools/check-deploy-security.sh`, wired into
  ShellGates/CI) prevents regressions. **Operator action:** if you relied on reaching
  Postgres/Redis/Prometheus from another host via the published port, front them with an
  SSH tunnel or reverse proxy, and set `DB_PASSWORD` and `REDIS_PASSWORD` in your
  environment.
- **The NATS observer transport refuses plaintext `nats://` by default
  (L-nats-plaintext).** Change-log events bridged to NATS previously crossed the wire in
  the clear over `nats://` with no TLS enforcement. `validate_nats_url` now requires
  `tls://`; plaintext `nats://` is accepted only when `FRAISEQL_NATS_ALLOW_PLAINTEXT` is
  set to `1`/`true` **and** no production marker is present (`KUBERNETES_SERVICE_HOST`,
  `FRAISEQL_ENV=production`, `FRAISEQL_PROFILE=production`) — the same refused-in-production
  policy as the SSRF bypass, but a separate flag so allowing plaintext NATS does not also
  disable the outbound SSRF guards. **Behavior change:** a deployment configured with a
  `nats://` URL must switch to `tls://` (or set the opt-in outside production).
- **Presigned storage URLs are now clamped to a maximum validity (L-presigned-expiry).**
  The `GET /storage/v1/object/sign/*key` endpoint accepted an unbounded `expiry_secs`, so a
  client could mint a credential-free URL valid for years. The requested expiry is now
  clamped to a configurable ceiling (default 7 days, `StorageRouteState::with_max_presign_expiry_secs`).
- **Hardened the shipped Kubernetes and Helm deployment manifests.** `k8s/service.yaml`
  carried a `Secret` with the literal database password `password`; it is now a
  non-functional placeholder with guidance to inject the real value out-of-band. The raw
  manifests, the Helm chart, and the "hardened" manifest pinned images to `:latest` (now
  `2.8.0`) and ran with a writable root filesystem (`readOnlyRootFilesystem: false`, now
  `true` — including the Helm values and the PodSecurityPolicy). Lower-severity
  demo/example/test deployment artifacts are tracked in #436.
- **Allow-list-backed `redirect_uri` flow for multi-provider auth (#427).**
  `GET /auth/v1/authorize` previously accepted a `redirect_uri` and then discarded it
  (returning tokens as JSON), with a server-side redirect deliberately deferred as an
  open-redirect risk. `MultiProviderAuthState::with_redirect_uri_allowlist` now enables a
  safe redirect: `authorize` rejects any `redirect_uri` not on the allow-list (400) and
  binds the validated URI to the CSRF state; `callback` redirects the browser to it with
  the tokens in the URL fragment. Matching is exact scheme + host + port + path-boundary
  prefix, so `https://app.example.com` does not match `https://app.example.com.evil.com`.
  With no allow-list configured the legacy JSON response is preserved and the URI is never
  used as a redirect target (no open-redirect surface). Closes the deferred audit finding
  L-redirect-uri.
- **SQL-layer hardening for shipped/template SQL (#437).** The two prod-init
  `SECURITY DEFINER` functions now pin `SET search_path = pg_catalog`
  (search-path-hijack hardening); the application role's grants are narrowed from
  `ALL PRIVILEGES` to `SELECT, INSERT, UPDATE, DELETE` (+ `USAGE, SELECT` on sequences);
  the `fraiseql` helper schema grants EXECUTE per function instead of the snapshot
  `ON ALL FUNCTIONS … TO PUBLIC`; and the init-script role passwords are documented as
  insecure placeholder defaults that must be overridden. (Change-spine view RLS — F6 — is
  deferred to a dedicated effort.)

### Removed

- **Unwired "enterprise" field-encryption modules removed from `fraiseql-secrets`
  (BREAKING).** The `encryption::{compliance, dashboard, error_recovery, mapper,
  performance, query_builder, refresh_trigger, rotation_api, schema, transaction}`
  modules (~7,300 LOC) had zero production consumers — reachable only from their own
  tests, never from the server binary. They are no longer part of the public API. The
  encryption primitives (`FieldEncryption`, `VersionedFieldEncryption`) and the three
  wired modules (`middleware`, `database_adapter`, `credential_rotation`) are retained.
  This matches the field-encryption stance documented in v2.7.0 (the write path is
  inert; the server refuses to boot on encryption-marked fields).
- **`fraiseql_core::validation::CustomScalarRegistry` removed (BREAKING).** It was a
  public API wired to nothing. The `CustomScalar` trait it managed is unaffected.
- **Dead CLI command handlers removed.** The unreachable `generate-proto`/`openapi`
  handlers, the `gateway` command module (no `Commands` variant), and the orphaned
  `codegen` tree they depended on are gone (~3,600 LOC), along with the now-unused
  `prost`/`prost-types`/`thiserror` dependencies. No wired subcommand changes.
- **`fraiseql-db` empty `grpc` feature removed** (it gated nothing; real gRPC lives
  behind `fraiseql-server`'s `grpc` feature), and the `fraiseql-core` passthrough with it.
- **`ArrowFlightError::Flight` variant removed** — defined but never constructed. The
  enum is `#[non_exhaustive]`, so downstream matches (which already need a wildcard arm)
  are unaffected.

### Changed

- **Documentation accuracy sweep (#439).** `docs/` and the tutorial were swept against
  current behavior, every claim verified against source. Capabilities that are inert or were
  removed are now documented as such — field-level at-rest encryption (not implemented; the
  server refuses to boot on encryption-marked fields), the removed secrets compliance audit
  logger, the `sms`/`push`/`search` observer stubs, and distributed sagas (experimental,
  forward-only behind `unstable-saga`). Partial features are framed honestly (inbound webhook
  pipeline is library-only; CDC ships via the standalone `fraiseql-cdc-sinks` crate, not an
  umbrella `cdc` feature). TLS docs reflect the reverse-proxy termination stance (the server
  refuses to boot on `[tls]`), and stale version/flag strings were corrected. No behavior
  change.
- **`fraiseql-db` adapter integration tests read DB URLs from the test harness, and the
  `PostgresAdapter` `NoTls` stance is documented (#445).** The feature-gated
  `postgres`/`mysql`/`sqlserver` adapter + introspector integration tests no longer hardcode
  `localhost:5433`/`:3307`/`:1434`; they now source their URL from the new
  `fraiseql_test_support::{database_url, mysql_url, sqlserver_url}` env-URL helpers, so they
  resolve under Dagger service bindings (local == CI). The `PostgresAdapter` connection pool
  uses `NoTls` by design (transport security is terminated at a proxy or trusted
  loopback/private link, mirroring the server-TLS stance); this is now documented at the call
  site rather than left implicit. Residual cleanup split out of #442.
- **The `lint --verbose` CLI flag was removed** — it was parsed and discarded. The
  global `--verbose` flag is unaffected.
- `fraiseql-arrow` type-conversion errors now surface as `ArrowFlightError::Conversion`
  instead of the mislabeled `InvalidTicket`. `ClickHouseSink::run` now terminates (with a
  final flush) when its channel closes instead of spinning forever, and the health-check
  `version` reports `CARGO_PKG_VERSION` instead of a hardcoded `2.0.0-a1`.
- `fraiseql federation check --json` no longer double-prints its result under the global
  `--json` flag; `fraiseql run` now warns loudly when a sibling `fraiseql.toml` fails to
  parse instead of silently falling back to defaults.

### Added

- **SQLite Insert/Delete mutations via direct SQL.** SQLite gains write support: the
  mutation runner now dispatches on the adapter's `MutationStrategy`, so a `DirectSql`
  adapter (SQLite) executes `INSERT … RETURNING *` / `DELETE … RETURNING *` generated from
  the mutation contract instead of calling a stored function. Insert and Delete mutations
  work end-to-end through the server, making SQLite usable for local development and testing
  of mutating schemas. Update (three-state JSONB) and stored-procedure (`fn_*`) mutations
  remain PostgreSQL / MySQL / SQL Server only and are rejected at startup
  (`url_guard::guard_sqlite_mutations`) with a clear diagnostic. The stored-function
  (`FunctionCall`) path is byte-identical.
- **Real Redis cache-invalidation observer transport (#428).** The observer `cache`
  action's `invalidate` operation — previously a fail-loud stub — now has a real transport:
  a `RedisCacheInvalidator` (behind the `caching` feature) deletes matching keys on the
  application Redis keyspace. Untrusted event values are glob-escaped before substitution
  into the key pattern, then dispatched either as a direct `UNLINK` (no glob) or a bounded
  `SCAN … MATCH` + `UNLINK` (never `KEYS` / `DEL`). A missing backend fails loud rather than
  silently no-opping, and a Redis outage surfaces as a retryable error so a stale cache is
  never treated as success. `sms` / `push` / `search` actions and cache `refresh` remain
  unimplemented (fail loud).
- **Inbound webhook receiver pipeline (#431).** New `fraiseql-webhooks` library core for
  receiving third-party callbacks: secret resolution → signature verification (no DB work on
  a bad signature) → an **atomic idempotency claim inside the handler transaction**
  (`INSERT … ON CONFLICT DO NOTHING RETURNING`), so a duplicate delivery is discarded and a
  handler error rolls back both the effects and the claim for a clean retry. Ships a
  `PostgresIdempotencyStore` (deny-by-default RLS) and a static secret provider. The HTTP
  route and server mount are tracked follow-ups — this slice is the library pipeline only.
- **Outbound change-data-capture to NATS `JetStream` (#382, first slice).** A new,
  additive, off-by-default `fraiseql-cdc-sinks` crate drains the framework-owned
  `core.tb_entity_change_log` outbox — the rows the mutation executor *and* the #366
  external-write capture trigger already write in-transaction — to an external broker,
  closing the #366 → #382 CDC pair. A `DrainWorker` enqueues each new, matching outbox
  row into a per-sink delivery-state table (`core.tb_cdc_sink_state`) keyed by a durable
  `MAX(seq)` cursor (restart-safe, no separate cursor table), then publishes due rows in
  `seq` order under `FOR UPDATE SKIP LOCKED`. Delivery is **at-least-once** — a broker
  outage accumulates backlog and retries with capped exponential backoff rather than
  losing events, and a permanent failure (e.g. an un-renderable subject) is
  dead-lettered; consumers dedup on `(object_type, seq)`, carried as the NATS
  `Nats-Msg-Id` header (which also engages `JetStream`'s server-side dedup window).
  Per-tenant/per-table subject templating (`fraiseql.{tenant_id}.{table}`) sanitises
  every interpolated segment against the NATS subject charset, failing closed on any
  `.`/`*`/`>`/whitespace that could escape into another tenant's namespace. The NATS
  sink rides the pure-Rust `async-nats` client behind the `cdc-nats-jetstream` feature;
  the drain worker and all encoding/sanitisation logic compile with no broker feature.
  Server auto-mount from `[cdc.outbound]` TOML, additional brokers (Kafka / NATS-core /
  Kinesis / Pulsar), and Avro/Protobuf encodings remain on the #382 umbrella.
- **Real distributed saga *forward* execution behind the `unstable-saga` feature (#429).**
  Follow-up to the audit H32 honesty fix that left the saga forward phase failing loud.
  The wiring is *additive*: the existing `SagaExecutor::{execute_step, execute_saga,
  get_execution_state}` keep their signatures and fail-loud `NotImplemented` behaviour in
  every build (they are the published placeholder contract and the #429 acceptance spec).
  With `unstable-saga`, new methods carry the real local-mutation path:
  `execute_step` dispatches a step's real mutation via
  `FederationMutationExecutor::execute_local_mutation` (the persisted `MutationType`
  renders to a `create`/`update`/`delete` verb, so no schema change) and reports the
  outcome without fabricating success; `execute_saga` marks the saga `Executing`,
  drives steps in `order`, persists each step's real result and `Completed`/`Failed`
  state, **stops at the first failed step**, and marks the saga `Completed` or `Failed`;
  `execution_state` derives progress from persisted step state. A failed mutation persists
  a real `Failed` transition, never a fabricated `Completed`. Compensation, recovery, and
  the coordinator facade remain unwired in both builds. The wired dispatch/honest-failure
  is proven against an in-memory SQLite adapter (no service); the multi-step orchestration
  is proven against a real PostgreSQL saga store in the integration leg. The API is
  unproven in production and may change without semver guarantees.
- **Native SAML 2.0 SP-initiated SSO + ACS (#381).** Opt-in `auth-saml` build feature adds
  an in-process SAML Service Provider: `GET /auth/saml/login` (signed-relay-state
  `AuthnRequest`) and `POST /auth/saml/acs`. Assertion verification (via `samael`/xmlsec1)
  is fail-closed across the full attack surface — XML signature verified against the IdP
  cert under a SHA-256+ algorithm allow-list, document *reduced to the signed bytes* before
  parsing (XML Signature Wrapping defense), `DOCTYPE`/entity declarations rejected (XXE),
  audience / `Recipient` / `Destination` / `NotBefore` / `NotOnOrAfter` / `InResponseTo`
  enforced, and single-use assertion-ID replay protection. A verified assertion resolves to
  a local user via the existing #411 account store keyed on `("saml:<idp>", NameID)`. Email
  auto-linking is **off by default** and, when opted in per IdP (`trust_asserted_email`),
  honored only when the merge is provably bounded to a single tenant — a tenant-bound IdP
  fails closed rather than risk a cross-tenant nOAuth merge, and SAML is never added to the
  global trusted-provider set. The `samael` XML/crypto C stack (libxml2 + xmlsec1) stays
  behind the non-default feature, so the default build is unchanged; CI runs a dedicated
  `integration: saml` Dagger suite. Multi-IdP discovery, per-tenant SAML config storage, and
  SCIM provisioning remain on the #381 umbrella. See `docs/auth/saml-sso.md`.
- **Password reset for local accounts (#367).** `LocalPasswordAuthenticator` gains
  `start_password_reset` / `confirm_password_reset` — a single-use, one-hour,
  non-enumerable reset on top of #412's Argon2 credentials. This is the reset slice
  deferred from the #367 bundle until #412 and the #349 email path shipped (both now done).
  Tokens use a **selector + verifier** scheme: the opaque token is `selector.verifier`, but
  the store (`core.tb_password_reset_token`, FK-linked to `core.tb_user`, same
  deny-by-default RLS as #411/#412) keeps only the selector and `sha256(verifier)`, so a
  database read cannot forge a usable token; redemption looks the row up by selector and
  compares the verifier hash in **constant time**. `start_password_reset` always returns
  `Ok(())` and dispatches the link in a spawned task, so an unknown or OAuth-only email is
  indistinguishable from a real one. `confirm_password_reset` validates the token, sets the
  new Argon2id hash, marks it used under an atomic single-use guard, invalidates the user's
  other outstanding tokens, and revokes the user's sessions. Email delivery is abstracted
  behind the new `ResetEmailSender` trait, so `fraiseql-auth` carries no SMTP dependency;
  HTTP endpoints and a concrete sender are deferred to the local-auth route wiring, matching
  #412's service-only precedent. Email verification (the remaining #367 sub-flow) and reset
  rate limiting are tracked follow-ups. See `docs/auth/local-password.md`.
- **Local password authentication (#412).** New `LocalPasswordAuthenticator` — email +
  password signup / login / secure storage using **Argon2id** (constant-time verify,
  per-credential random salt), built on the #411 identity store. Credentials live in a
  new `core.tb_password_credential` table FK-linked to `core.tb_user`, mirroring #411's
  schema and deny-by-default RLS (`ENABLE`-not-`FORCE`, GUC `fraiseql.tenant_id`,
  `REVOKE ALL … FROM PUBLIC`); `init()` is idempotent and self-sufficient. Security
  posture is deliberate: signup resolves the user through the existing `AccountStore`
  with provider `"local"` and `provider_id = normalize_email(email)`, links **fail-closed**
  (`email_verified = false`, so an unverified local signup can never seize an existing
  verified-email account — H26), and login is **non-enumerable** (unknown-user and
  wrong-password return the same `InvalidCredentials` with the same Argon2 cost via a
  same-parameter dummy hash; the server audit log keeps the precise reason). A correct
  password on a disabled account returns the distinct `AccountDisabled`; a successful
  login transparently rehashes when the cost policy strengthens. Always compiled, opt-in
  at runtime. Rate-limiting/lockout, non-enumerable signup, and password reset are tracked
  follow-ups (#367/#349). See `docs/auth/local-password.md`.
- **Persistent user / identity store (#411).** New `PostgresAccountStore` — a durable
  PostgreSQL backend for the existing `AccountStore` trait, so account-linking survives a
  process restart (the in-memory store loses it). It is a drop-in: same trait, same
  `"user_<uuid>"` identifier that joins `_system.sessions.user_id`, so `multi_provider` /
  `phone_otp` need no change beyond which `Arc<dyn AccountStore>` they are handed.
  `init()` idempotently creates `core.tb_user` and `core.tb_auth_identity` (the
  `CREATE … IF NOT EXISTS` form is the back-compat path for deployments with no user
  table). Both tables carry a `tenant_id` and Row-Level Security **deny-by-default**
  (mirroring the change-log RLS: `ENABLE`-not-`FORCE`, GUC `fraiseql.tenant_id`,
  `REVOKE ALL … FROM PUBLIC`) — the store is the trusted owner that bypasses, while any
  other role reads zero rows unless scoped to a tenant. Account-linking semantics
  (verified-email cross-provider linking; H26 fail-closed on absent/unverified email)
  match the in-memory store exactly, verified against PostgreSQL. This unblocks the
  Argon2id local-password authenticator (#412), social auto-linking (#368), and SCIM
  provisioning (#381). See `docs/auth/identity-store.md`.
- **Compiler→runtime contract gate.** A new test (`fraiseql-cli`) compiles fixtures with
  the real CLI and asserts the server boot seam (`RuntimeConfig::from_compiled_schema`)
  accepts the output, that an enterprise security toggle survives emit→parse→derive, and
  that core parse drops no compiler-emitted field — closing the class behind two past
  config-drift boot failures.
- **Signature-verification tests** for the Postmark and LemonSqueezy webhook verifiers
  (previously the only two of 13 with zero coverage), and a loud-failure assertion for
  the `fraiseql-test-support` database-URL harness.
- **`release-smoke`** now runs one real GraphQL query through the full pipeline, not just
  the health endpoint.
- **`fraiseql watch` — recompile + zero-downtime live reload (#383).** A new CLI command
  watches a schema source and, on every (debounced) save, recompiles `schema.compiled.json`
  and — when `--reload-url` is given — POSTs it to a running server's
  `POST /api/v1/admin/reload-schema` admin endpoint, which swaps the executor via `ArcSwap`
  (in-flight queries finish on the old schema, no restart). Unlike `run --watch` (which
  restarts an in-process server), `watch` drives a separately running server: `fraiseql
  watch schema.json --reload-url http://localhost:8080 --admin-token $TOKEN`. Compile and
  reload failures are reported but never stop the loop. Omit `--reload-url` to recompile to
  disk only.
- **`fraiseql compile --database` now lints more of the view contract (#384).** Three
  residual checks were added to the compile-time database validator: (1) each mutation's
  `inject_params` and call/response shape are validated against the real `pg_proc`
  signature (PostgreSQL), reusing the `validate --against-db` contract logic; (2) a
  query argument that resolves to a native column whose SQL type cannot drive the
  predicate (e.g. an `Int` argument filtering a `uuid` column) is flagged — conservative,
  so permissive `ID`/`String` filters never warn; and (3) the `MissingJsonKey` warning
  now names the owning GraphQL type so the field is locatable. All findings remain
  advisory warnings (the compile never fails on them).
- **`fraiseql doctor --against-db` gained two change-log hardening checks (#443).**
  Alongside the existing change-log RLS posture check, the live-database pass now
  verifies the rest of the migration-12 / migration-11 hardening: (1) **Change-log
  PUBLIC grants** warns when `PUBLIC` still holds any privilege on
  `core.tb_entity_change_log` or its two views (the `REVOKE ALL … FROM PUBLIC`
  least-privilege baseline is not in force — every tenant's before/after payload is
  world-readable); and (2) **Change-log capture function** warns when
  `core.fn_entity_change_log_capture()` is not `SECURITY DEFINER`, or is DEFINER but
  has no pinned `search_path` (a DEFINER function reachable from a trigger on any
  schema with a mutable `search_path` is a privilege-escalation vector). Both are
  advisory warnings; an absent table or function is an informational pass
  (single-tenant or pre-migration deployments).
- **GraphQL subscription clients now receive the Change-Spine envelope (#425).** Each
  delivered `next` event carries the audit / provenance metadata the Change-Spine
  already records — `actorType` (human / service account / AI agent / system job),
  `actingFor` (the human a delegated agent acted for, #390), `schemaVersion` (the
  producer schema, #377), `tenantId`, `durationMs`, and `seq` — bringing the
  subscription path to the same envelope parity the change-log reader and NATS bridges
  have. The envelope rides in the graphql-transport-ws `extensions.changeSpine` slot of
  the `next` payload (the spec-blessed, client-ignorable channel), so the resolved
  entity `data` is untouched and no schema or SDK-codegen change is required. It is
  always present, carrying only the fields the producer stamped (unset fields are
  omitted); an event with no stamped envelope delivers the plain payload unchanged. The
  metadata round-trips observer `EntityEvent` → `BridgeEntityEvent` → `SubscriptionEvent`
  → client, with tenant filtering and resolved-`data` delivery unaffected.

### Fixed

- **The FraiseQL-Wire database backend now honors `ORDER BY` (#442).** `FraiseWireAdapter`
  silently dropped the `order_by` argument, so relay/keyset pagination over the
  `wire-backend` feature returned database-native order. Both the streaming and the
  in-memory limit/offset paths now push the (validated, dialect-aware) ordering down to the
  wire query builder. Also removes a dead `build_query` method and an unreachable SQL Server
  pagination branch, and de-duplicates the sqlite/sqlserver adapter test files.
- **The `ecommerce_api` example's database now initializes cleanly (#446).** The flat
  `docker-compose` migration tree (`db/migrations` + `db/views` + `db/functions`) had
  several latent SQL errors that left the schema half-built — `migrate` printed
  "Migrations completed!" regardless because `psql` ran without `ON_ERROR_STOP`. Fixed:
  the shared `mutation_response` composite type (returned by every function but never
  defined in the flat tree) is now created in `001_initial_schema.sql`; `add_customer_address()`
  and `submit_review()` reorder their parameters so no required parameter follows a defaulted
  one (PostgreSQL rejects that); the `product_detail` / `customer_wishlists` views compute
  per-product images, variants, price and stock via correlated subqueries instead of
  `json_agg(DISTINCT …)` over multiplying joins (invalid `ORDER BY`/`json`-equality/nested-aggregate
  forms); and `related_products` counts shared tags with an `INTERSECT` instead of the
  non-existent `&` array operator. The `migrate` step now runs each file with
  `ON_ERROR_STOP=1` so a broken migration fails loudly. Verified end-to-end against
  PostgreSQL (full init + mutation calls + view reads). Found while hardening the examples
  for #438.

## [2.8.0] - 2026-06-18

### Security

- **The `sql_query` host read-only guard now inspects CTE bodies (M-cte-classifier).** The
  SQL classifier mapped any `Statement::Query` to read-only without walking its `WITH`
  clause, so a data-modifying CTE — `WITH t AS (DELETE FROM x RETURNING *) SELECT * FROM t`
  (and the `INSERT`/`UPDATE`/`MERGE` equivalents, including nested and derived-subquery
  CTEs) — passed as read-only, bypassing the guard. The classifier now recurses through CTE
  and subquery bodies and rejects data-modifying statements with the (previously dead)
  `RejectionReason::WritableCte`.
- **Deno function resource limits are now enforced by V8, not by string matching
  (M-deno-limits, DoS).** The "limits" were `source.contains("while (true)")` substring
  checks that the configured memory cap never reached V8 — trivially bypassed and prone to
  false positives. They are replaced with real enforcement: a V8 heap limit
  (`CreateParams::heap_limits` + a near-heap-limit callback that terminates execution) and a
  watchdog thread that calls `terminate_execution()` after the configured duration (catching
  tight synchronous loops that never yield to the event loop). The substring heuristics are
  deleted.
- **SSRF guards converged and hardened across the functions, federation, and observers
  crates (M-fn-ssrf, M-fed-mut-ssrf, M-fed-allow-insecure, M-ssrf-blocklist).** Each
  outbound-HTTP path now resolves DNS and blocks private/reserved addresses (closing the
  DNS-rebinding TOCTOU), disables redirects (`Policy::none()` — a `3xx` can no longer bounce
  to an internal target), and fails closed:
  - **functions** `http_validator`: the default domain allowlist was `["*"]` (allow-all) —
    now empty (deny-by-default); the guard now resolves the host and rejects private IPs
    instead of only checking literal-IP hosts; the outbound client disables redirects.
  - **federation** `HttpMutationClient` (the state-changing direction) gained the
    `redirect(Policy::none())` + `https_only(true)` + DNS-rebinding guards its sibling entity
    resolver already applied.
  - **federation** `FRAISEQL_FEDERATION_ALLOW_INSECURE` is **removed**: it logged "HTTPS
    enforcement disabled" while `https_only(true)` was unconditional — a lying no-op with no
    recorded user. `http://` subgraph URLs are now rejected unconditionally.
  - **observers**: the drifted SSRF blocklist duplicated in `executor/dispatch.rs` is deleted
    in favour of the canonical `ssrf::validate_outbound_url` (with `0.0.0.0/8` and
    `localhost.*`-alias coverage merged into the canonical first so nothing is lost) plus a
    dispatch-time `dns_resolve_and_check`.
- **The `sql_query` host read-only guard now inspects CTE bodies (M-cte-classifier).** The
  `provider::PkceChallenge::validate` compared the recomputed challenge with `==`
  (variable-time), a timing-attack vector, while the parallel `oauth::pkce::PkceChallenge`
  used constant-time `ct_eq`. The `provider` path now uses `subtle::ConstantTimeEq`, so all
  PKCE verification paths are constant-time.
- **JWKS fetch pins the connection to the validated IP (M-jwks-toctou, DNS-rebinding SSRF).**
  `dns_resolve_and_check` validated the resolved IPs, but the subsequent reqwest call
  re-resolved the host independently — a TOCTOU window where attacker-controlled DNS could
  flip the host to a private IP after the check (blind internal SSRF). The fetch now resolves
  and validates once, then pins reqwest to the validated addresses (`resolve_to_addrs`) and
  disables redirects (`Policy::none()`) so the connection cannot be re-pointed to an internal
  target. **Behavior change:** a `jwks_uri` that issues an HTTP redirect is no longer followed
  (OIDC `jwks_uri` endpoints are served directly; following redirects on this fetch is an SSRF
  amplifier).
- **Vault `AppRole` login validates the address before sending credentials (H15, SSRF).**
  `with_approle` POSTed the `role_id`/`secret_id` to the configured address and only
  afterwards ran the SSRF address check, so a misconfigured/attacker-influenced address
  (e.g. `169.254.169.254`) received the high-value `secret_id` before the guard fired.
  `validate_vault_addr` now runs as the first statement, matching the token path.

### Added

- **`naming_convention` is now configurable for the JSON-schema compile workflow
  (`[fraiseql.naming] convention`).** Previously only the author-in-TOML workflow could set a
  naming convention; the `fraiseql-cli compile schema.json` + `fraiseql.toml` workflow was
  hardwired to `preserve`, so a backend on that path could never activate the server's
  single-JSONB mutation input-key recasing (gated on `camelCase`) and had to hand-roll a
  `camelCase → snake_case` input shim. `[fraiseql.naming] convention = "preserve" | "camelCase"`
  now flows through the compiler into the compiled schema's `naming_convention`, the same value
  the TOML workflow already populated. With `camelCase`, the engine owns all casing end-to-end
  (`snake_case` columns/functions in the database, `camelCase` operation/field names to clients,
  input keys recased before they reach the SQL functions), letting such backends delete the shim.
- **Configurable casing acronyms (`[fraiseql.naming] acronyms`).** Identifiers shaped as a
  lowercase word plus a digit (`s3`, `ipv4`, `oauth2`) are ambiguous to reverse — `phone1`
  (from `phone_1`) and `s3` are structurally identical — so they now keep their digit attached
  via an acronym registry. A built-in default set covers the common cases (`s3`, `ec2`, `ipv4`,
  `ipv6`, `oauth2`, `sha256`, `md5`, `base64`, …); add your own `<word><digit>` keys with, e.g.,
  `[fraiseql.naming]\nacronyms = ["widget3", "iso9001"]`. Registering an acronym declares its
  JSONB key is the atomic form (`s3`, not `s_3`) — author the field accordingly. The list flows
  from `fraiseql.toml` through the compiler into the compiled schema and is installed at server
  boot; only the reverse (`to_snake_case`) consults it, so the GraphQL surface is unchanged.
- **Opt-in auto-synthesis of mutation result unions (`[fraiseql.mutations] auto_error_union`).**
  When enabled, the compiler synthesizes a shared `MutationError` type and a per-mutation
  `<Mutation>Result` union (`= Entity | MutationError`) for every object-returning mutation,
  rewriting the mutation's return type to that union — so the server's existing success/error
  discrimination over the `app.mutation_response` composite has a union to resolve against
  without declaring `Entity | MutationError` by hand for each mutation. Off by default;
  mutations already returning a union (and scalar/enum returns) are left untouched, and an
  existing type name is never overwritten. The synthesized `MutationError` exposes `status`,
  `message`, `httpStatus`, and `errorClass`, now surfaced from the composite's first-class
  columns on the error arm (previously only `status` was injected). See the "result unions"
  section of `docs/guides/typed-clients.md` for the authoring contract.
- **Changelog tail query for tip checkpointing (H28, server side).** The
  `GET /api/observers/changelog` endpoint accepts `?latest=true`, returning only the single newest
  entry (`ORDER BY pk DESC LIMIT 1`, honouring the `object_type` filter) and echoing its cursor as
  `next_cursor`. This lets a consumer checkpoint at the real tail without replaying history — the
  server-side half of the `from_now` consumer fix (the consumer half lands in a later release).
- **Per-mutation `input_style: flatten | jsonb`, decoupling input-passing from the DML verb.**
  A new opt-in mutation flag controls how the GraphQL `input` argument reaches the SQL function,
  independently of `operation`. The executor takes the single-JSONB-argument path when
  `input_style == jsonb` **or** the operation is `Update` (today's behavior). This lets a backend
  using the single-JSONB wrapper convention (`fn(input_payload jsonb, …) RETURNS app.mutation_response`)
  register the *real* verb (`Insert`/`Delete`/`Custom`) and still receive the whole input as one
  `jsonb` arg — so the Change Spine records the true `modification_type` instead of a blanket
  `UPDATE` (creates and deletes were previously indistinguishable in the audit/CDC stream when a
  backend forced `operation = Update` purely to opt into single-JSONB passing). The forced
  single-JSONB path composes with the #400 acronym-aware input-key recasing. Surfaced as
  `@fraiseql.mutation(input_style="jsonb")` in the Python SDK, `@Mutation({ inputStyle: "jsonb" })`
  in the TypeScript SDK, and `input_style = "jsonb"` on a `[mutations.<name>]` table in the TOML
  schema. Fully opt-in and backward compatible: the default `flatten` is byte-for-byte today's
  behavior, and an absent value adds no compiled-schema bytes (no codegen schema-hash churn).
- **Per-mutation `changelog_pre_image` — opt-in Debezium-style pre-image for the Change Spine.**
  A new opt-in flag makes a mutation also record the changed entity's **before-state** alongside
  the after-state it already writes, into a new nullable `object_data_before JSONB` column on
  `core.tb_entity_change_log`. The pre-image is sourced from an optional `entity_before` on the
  mutation's `app.mutation_response` (the same way the after-image is sourced from `entity`), and
  the in-transaction outbox CTE reads `r.entity_before` **only when the flag is set**. `object_data`
  stays the after-image for *every* consumer — the pre-image is a separate column, never a
  `{before, after}` envelope — so audit-sensitive mutations (price/contract/order edits, financial
  deletes) get an inline `{before, after}` without paying that cost on every change. The
  out-of-band #366 capture trigger is unified on the same shape: it now writes `object_data = NEW`
  (after-image) and, for tables that opt in via `@subscribable(tables=[...], pre_image=True)`,
  `object_data_before = OLD`. A new `core.v_entity_change_log_debezium` view projects the classic
  `{before, after, op, source}` event from the columns (a view, not a stored shape). Surfaced as
  `@fraiseql.mutation(changelog_pre_image=True)` in the Python SDK,
  `@Mutation({ changelogPreImage: true })` in the TypeScript SDK, and `changelog_pre_image = true`
  on a `[mutations.<name>]` table in the TOML schema. Fully opt-in and backward compatible: the
  default is off (after-image only, byte-for-byte today's behavior) and an absent value adds no
  compiled-schema bytes (no codegen schema-hash churn). The nullable column is added to the
  PostgreSQL, MySQL, and SQL Server contracts for parity; only the PostgreSQL outbox CTE and
  capture trigger write it.

### Fixed

- **`SMALLINT`/`int2` columns now decode to JSON numbers instead of `null` (incl.
  `mutation_response.http_status`).** The PostgreSQL `row_to_map` decoder tried `i32`/`i64`
  for integers but had no `i16` branch, so a non-null `int2` value fell through the type
  ladder to `Null` (`FromSql for i32` rejects `int2`). The headline symptom: a failed
  mutation's `MutationError.httpStatus` came back **absent** — the `http_status` `SMALLINT`
  column nulled here, so the parser's `Option<i16>` read `None` and the projection's
  `if let Some(code)` guard skipped the field, while `errorClass` (a `TEXT` column) resolved
  fine through the same path. An `i16` branch is added to `row_to_map`, fixing `httpStatus`
  (404 not_found / 409 conflict / 422 validation / 500 internal) and every other `SMALLINT`
  column generally. No behavior change for any other column type.

- **GraphQL variables nested inside object/list literal arguments are now substituted.**
  A variable used as a value *inside* an object or list literal argument
  (`where: { field: { eq: $v } }`, `createMachine(input: { f: $v })`) was not resolved from the
  request `variables` map — only a whole-argument variable (`where: $where`, `input: $input`) was.
  Nested `$v` placeholders reached WHERE-clause SQL generation and mutation input coercion verbatim,
  so filters silently matched nothing and inline mutation inputs surfaced as a missing required
  argument. The matcher now recurses into object/list members (depth-bounded; an unknown variable
  resolves to `null`, matching GraphQL's treatment of an omitted nullable), and the mutation path
  carries the root field's inline arguments so an inline `input: { ... }` literal with nested vars
  is visible before required-argument validation. Whole-argument behavior is unchanged.

- **Mutation results now surface `updatedFields`, selection-gated (#433).** The executor
  surfaced the `cascade` wire payload without the SQL function embedding it in the entity
  JSONB, but its sibling envelope column `updated_fields` (the GraphQL field names a mutation
  changed) was parsed into the typed `mutation_response` row and then dropped at the success
  boundary, so `mutation { updateOrder(input: $input) { updatedFields … } }` silently returned
  no `updatedFields` key. The success arm now injects `updatedFields` symmetric with `cascade`,
  but **selection-gated** — present only when the client selects it (including inside an inline
  fragment), so a mutation that does not ask for it keeps an exact projected shape. An empty
  list (a noop) surfaces as `[]` when selected.
- **List field and argument types now compile to a list, not a single object (#434).** The CLI
  schema converter's `parse_field_type` matched built-in scalar names and routed everything else
  — including the SDL list string `"[Item!]"` — to `FieldType::Object`, so a list field arrived
  as `Object("[Item!]")`: a single object whose type name does not exist. The runtime then
  projected `parent { items { id } }` as `{"items": {"id": null}}` (one null object) instead of
  `{"items": [{"id": …}]}`. `parse_field_type` now unwraps an SDL list wrapper (`[Inner]` /
  `[Inner!]`, recursing for nested lists like `[[Inner!]!]`) into `FieldType::List`, and strips a
  trailing non-null `!` before matching the base name (outer-field nullability is tracked
  separately). This applies to both type fields and list query arguments (`ids: [ID!]`).
- **Digit-suffixed field names now camelize and resolve correctly (`phone_1` → `phone1`).**
  A field whose `snake_case` name ended in a digit segment (`phone_1`, `address_2`, `line_2`)
  was emitted into the GraphQL schema unchanged (`phone_1`) while every other field camelized,
  and the runtime could not map a collapsed digit field back to its JSONB key. The casing pair
  is now bijective, mirroring FraiseQL v1: the Python SDK and the engine's `to_camel_case`
  collapse the digit boundary (`phone_1` → `phone1`, `dns_1_id` → `dns1Id`), and the canonical
  reverse `to_snake_case` reinserts it (`phone1` → `phone_1`), so a field surfaced as `phone1`
  reads `data->>'phone_1'`. **Behavior change:** the GraphQL surface name of digit-suffixed
  fields changes from `phone_1` to `phone1`; clients querying the old `phone_1` name must switch
  to `phone1` (or add an explicit GraphQL alias). Common acronyms (`s3`, `ipv4`, `oauth2`, …)
  stay whole via the built-in acronym registry — extend it for your own `<word><digit>` keys
  with `[fraiseql.naming] acronyms` (see Added). An unregistered `<word><digit>` name still
  splits, so author the underscore form or add the acronym/an alias.
- **Mutation input recasing now covers nested composites on the Insert/Custom path (#400).**
  Under `naming_convention = "camelCase"`, the Update path already recased a mutation's whole
  `input` payload to the schema's canonical (`snake_case`) field names before it reached the
  SQL function; the Insert/Custom path recased only its top-level keys (which map to columns
  positionally), passing a *nested* composite input field as one JSONB arg with its keys
  verbatim — so a `jsonb_populate_record(NULL::config, $arg)` saw camelCase keys it could not
  read, silently writing NULLs (`affected_count = 0`). Both paths now share one
  `recase_input_field_value` helper that recurses into nested input objects and lists of them,
  so a create with `config: { s3Bucket, maxConnections }` reaches the function as
  `{ s3_bucket, max_connections }`. Recasing is driven by the input type's per-field map (not a
  lossy `camel→snake` regex), so it honours the acronym registry in both directions
  (`dns1Id` → `dns_1_id`, `s3Key` → `s3_key`) and leaves scalar values, enum values, and
  free-form JSON untouched; a `Preserve`-convention schema is unaffected. This completes the
  server-side `naming_convention` input work (#216/#400): a backend reading `snake_case`
  composite columns no longer needs a `jsonb_camel_to_snake(input)` SQL shim, for reads or writes.
- **Federation mutations recase input keys to canonical column names (#400, federation path).**
  The federation mutation builder turned GraphQL input variable keys *directly* into quoted SQL
  column identifiers (`INSERT INTO "users" ("s3Key") …`) and looked the `@key` value up by its
  canonical name, so a camelCase surface (`s3Key`, `dns1Id`) produced an `INSERT`/`UPDATE`
  against a column that does not exist — and the `UPDATE`/`DELETE` key lookup missed entirely
  (`Key field 'dns_1_id' missing`). `FederationMutationExecutor` now recases the input keys to
  their canonical `snake_case` names (via the same acronym-aware `to_snake_case`, scalar-only as
  federation mutations are) before SQL generation, gated by a `recase_input_keys` flag set from
  the schema's `naming_convention == CamelCase` (off for `Preserve`).
- **Mutation input recasing now also covers the single-JSONB-argument path when no Input type
  drives it (#400).** The field-driven recasing above only fires when a *registered* Input type
  supplies the per-field name map. A custom `mutation(input: JSON)` whose SQL function takes
  `(input jsonb, …)` — and an Update whose declared Input type is absent from the compiled
  schema — fell through to the catch-all argument path and reached the function with the whole
  object as one **verbatim camelCase** JSONB blob, so `jsonb_populate_record(NULL::…, input)` /
  `input->>'snake_field'` saw keys it could not read (spurious validation error or
  `affected_count = 0` no-op). The single-JSONB path now recases the object's keys itself: it
  uses the field-driven map when the Input type is known, and otherwise the canonical
  acronym-aware `to_snake_case` directly on the keys — recursing into nested objects and lists,
  leaving scalar values untouched. Because that is the same `to_snake_case` the read path uses,
  write keys round-trip exactly as reads do (`dns1Id` → `dns_1_id`, `s3Key` → `s3_key`,
  `ipv4Cidr` → `ipv4_cidr`, `oauth2Token` → `oauth2_token`). It is gated by
  `naming_convention == CamelCase` and scoped to the single `input`-named argument, so a
  `Preserve` schema, a plain-scalar `input` arg, and free-form JSON arguments on multi-argument
  mutations are all left untouched. This is the last single-JSONB-convention backend's reason to
  keep a hand-rolled `jsonb_camel_to_snake(input)` write shim.
- **Injected params now filter on a real column when the view has one (native-column inference
  gap).** Compile-time native-column inference (`database_validator.rs`) consulted only a query's
  explicit arguments, so an injected param (e.g. a `tenant_id` from a JWT claim) was never added to
  `native_columns`. The runtime then rendered `WHERE data->>'tenant_id' = $1` even when the backing
  view had a real `tenant_id` column — returning 0 rows for inject-scoped list queries whose views
  keep `tenant_id` as a column (not inside `data`). Native-column inference now consults inject-param
  names against the introspected columns too; a match renders `WHERE tenant_id = $1::uuid`.
  Explicit-arg behaviour (including the `NativeColumnFallback` warning) is unchanged, and inject-param
  misses stay silent (a claim may legitimately live in the `data` JSONB). Requires recompiling with
  `--database` so the inference can see the view's columns.
- **SDK publishing is no longer silently frozen (H30, release integrity).** `tools/release.sh`
  bumped the Rust manifests but never the SDK manifests, so the Python `pyproject.toml`/
  `__init__.py` and the npm `package.json`/`package-lock.json` stayed pinned at `2.1.6`
  (the TypeScript `version` constant had drifted further, to `2.0.0-alpha.1` — L-ts-version).
  Each `v*` release then built that stale version; `twine upload --skip-existing` and the
  npm "already published, skipping" branch no-oped, and the validation step installed the
  *old* version — so v2.3.0–v2.6.0 SDK publishes reported success while shipping nothing.
  `release.sh` now bumps all SDK manifests in lockstep with the crates, and the
  `publish-python`/`publish-typescript` jobs gained a fail-loud gate
  (`assert_sdk_version_matches`) that refuses to publish when the manifest version does not
  match the release tag; the validation steps now assert the *new* tag version specifically.
  New unit coverage in `make test-release-tooling` exercises the bump and the gate.
- **Python SDK: `except fraiseql.FraiseQLError` now catches async-client errors (H27).**
  There were two unrelated `FraiseQLError` classes — one in `client.py` (the package-level
  `fraiseql.FraiseQLError`) and one in `errors.py` (the base of `GraphQLError`/`NetworkError`/
  `TimeoutError`/`AuthenticationError` raised by `AsyncFraiseQLClient`). They shared a name
  but no inheritance, so the documented catch-all silently caught nothing the async client
  raised (`issubclass` was `False`). The hierarchy is now consolidated in `errors.py` under a
  single `FraiseQLError` base; `client.py` re-exports it, so both clients' errors are
  catchable as `fraiseql.FraiseQLError` and existing `from fraiseql.client import FraiseQLError`
  imports keep working. The two clients deliberately classify differently (async: HTTP status;
  sync: GraphQL `extensions.code`), now documented on the module. Behaviour change: code that
  relied on the catch-all *not* catching async errors will now catch them.
- **Python SDK: `ChangelogConsumer(startup_mode="from_now")` no longer replays history (H28).**
  `_initialise_cursor` fetched the first page (`after_cursor=0, limit=1`) and checkpointed at
  its `next_cursor` — the *oldest* entry's cursor — so the next poll replayed almost the entire
  changelog with side effects. It now resolves the real tail via the `?latest=true` tail query
  (Phase 09), then pages forward to the true tail (correctness on older servers that ignore
  `?latest`), checkpointing there and processing zero pre-existing rows.
- **SDK correctness cluster.** Several SDK behaviour bugs:
  - **Python `AsyncFraiseQLClient` honours `RetryConfig.retry_on` (M-retry-config).** The
    retry loop's `except` tuple was hardcoded to `(NetworkError, TimeoutError)`, so a custom
    `retry_on` (e.g. `AuthenticationError`) was never caught and the request ran once instead
    of `max_attempts` times. It now catches broadly and lets `RetryConfig.should_retry` decide.
  - **Python `export_schema(include_custom_scalars=False)` now drops the block
    (M-export-schema).** The filter checked the snake_case key `custom_scalars` while the
    registry emits camelCase `customScalars`, so the flag was a no-op. (The neighbouring test
    passed vacuously — it never registered a scalar.)
  - **Python injected clients keep the configured Authorization (L-sdk-injected-client).**
    `AsyncFraiseQLClient` and `ChangelogConsumer` discarded the `authorization` argument when a
    client was injected; they now apply it to the injected client's headers.
  - **TypeScript: malformed `inject` specs are rejected, not silently dropped (M-ts-inject).**
    `normaliseConfig` dropped any spec without a `jwt:<claim>` shape; it now validates the
    param identifier, the `jwt:<claim>` source, and argument-name collisions and throws —
    matching the Python SDK's `_validate_inject`.
- **SDK: the no-op `config()` helper is removed (H29).** Both SDKs shipped a `config()` that
  the docs told users to `return` from a decorated function (`return fraiseql.config(sql_source=...)`)
  — but its result was stored in a holder nothing ever read, so the call did nothing. Removed
  `config()`/`_ConfigHolder` (Python) and `config()`/`getPendingConfig`/`ConfigHolder`
  (TypeScript), with their package exports, and corrected the docstrings/examples to the real
  pattern: pass config as decorator arguments (`@fraiseql.query(sql_source="v_user")`) or via
  `fraiseql.toml`.
- **One cross-SDK parity comparator; empty output fails (M-parity-comparators).** Two
  comparators had drifted: the strict, CI-wired `sdks/official/tests/compare_schemas.py` and a
  lenient copy `tools/compare_parity_schemas.py` that *skipped* any item missing from a
  candidate — so an SDK generator emitting nothing passed vacuously. The lenient copy is
  removed and `make parity-compare` now uses the strict comparator (which hard-fails when
  type/query/mutation name sets differ, including against empty output).
- **`tools/lint.sh` reports failures honestly (L-lint-sh).** The `sql-helpers-sync` check
  called `fail`/`pass` itself and then returned 0, so `run_check` *also* printed ✅ on a real
  divergence; it now returns a status and lets `run_check` report. The `lint-gate-db` count
  used `grep -c … || echo 0`, which emitted a two-line `"0\n0"` on no match and broke the
  numeric comparison; it now uses `|| true`.
- **Wire hygiene cluster (L-wire-*).** A set of low-severity wire-crate correctness fixes:
  - **`Field::JsonbField` extracts text (`->>`) as documented (L-wire-jsonb).** It emitted
    `(data->'field')` (JSONB) while its own doc and the `sql_gen` cast strategy assume text
    extraction — so a string comparison saw a quoted JSON value and the numeric/inet/ltree
    casts had no valid source type. It now emits `(data->>'field')`.
  - **Connection-string credentials are percent-decoded (L-wire-connstr).** The userinfo
    parser split on the *first* `@` and never decoded `%XX` escapes, so a password containing
    `@`, `:`, or `%` was mangled. It now splits on the last `@` and percent-decodes the user
    and password (rejecting malformed `%` escapes).
  - **`connect_timeout` is now applied (L-wire-timeout).** The config field was parsed but
    never used; `connect_with_config`/`connect_with_config_and_tls` now bound the
    transport-connect future with it, surfacing a lapse as `WireError::Connection`.
  - **The SCRAM PBKDF2 result is propagated, not discarded (L-wire-scram).** Both key
    derivations did `let _ = pbkdf2(...)`; a swallowed error would have left an all-zero
    salted password and silently produced a wrong proof. The result is now checked (a new
    full round-trip test verifies the client proof against an independently-derived server
    key).
  - **Adaptive-chunking builder options now take effect (L-wire-builder).** `execute_query`
    hardcoded adaptive chunking off and dropped the builder's
    `adaptive_chunking`/`adaptive_min_size`/`adaptive_max_size`; the options are now threaded
    through and the streaming loop actually observes channel occupancy and retunes the batch
    size. The builder default is now explicitly off (preserving the prior effective
    behaviour — fixed-size chunking is the zero-overhead path).
  - **`StreamStats` row counters are populated (L-wire-stats).** `total_rows_yielded` /
    `total_rows_filtered` were always zero; the stream now counts rows yielded to the consumer
    and rows rejected by a `QueryStream` predicate.
  - **De-duplicated the chunk-flush logic (L-wire-chunk-dup).** Two ~70-line copies with
    drifted error termination (the final-chunk path reported success even after the consumer
    dropped) were factored into one `stream_chunk_rows` helper that fails consistently.
  - **Removed a 29 MB `test_import` ELF binary committed to the repo (L-wire-elf).**
- **Wire `metrics`-facade emissions are now captured by an installed recorder (H45).** The
  workspace carried two incompatible `metrics` facade versions — `fraiseql-wire` emitted via
  `metrics` 0.22 while the server's `metrics-exporter-prometheus` was built against 0.24 — and
  no recorder was installed at all, so the emission and the (absent) recorder bound to
  different process-global statics and every one of wire's ~40 counters/histograms/gauges was
  silently dropped. `fraiseql-wire` is bumped to `metrics` 0.24 (single facade version in the
  lock), the server installs a process-global `PrometheusBuilder` recorder at startup behind
  the `metrics` feature, and the `/metrics` endpoint appends the rendered facade metrics to
  its hand-rolled output. The server's unreferenced direct `metrics` 0.22 dependency (the
  server emits its own metrics via hand-rolled atomics, not the facade) was dropped.
- **Wire stream pause/resume now actually reaches the background reader (H43).**
  `JsonStream` allocated its pause/resume state lazily, on the first `pause()` call — but the
  background reader task had already captured `None` clones of those handles at spawn time,
  so `pause()`/`resume()` never affected it: the reader streamed on regardless, and the
  pause-timeout and paused-occupancy metrics were permanently dead. The state is now
  allocated eagerly in `JsonStream::new`, so the reader shares the same handles the caller
  drives. As a result: `pause()` parks the reader at the next chunk boundary (and records the
  buffered-row count in `paused_occupancy()`); `set_pause_timeout` is honoured live via a
  shared handle (and the auto-resume timeout metric fires); and a drop-while-paused now tears
  the reader down cleanly instead of leaking a task blocked forever (the pause wait also
  selects on cancellation). The dead `pause_signal` (notified but never awaited) was removed.
- **The wire connection no longer hangs on a malformed, unrecognized, or ordinary
  control message (H42).** `receive_message` decoded with `if let Ok(..)`, discarding the
  error kind and treating *every* decode failure as "the frame is incomplete, read more
  bytes" — so a malformed message, an unknown tag, or an unsupported message looped forever,
  buffering toward the size cap. Decode errors are now classified by `io::ErrorKind`: only
  `UnexpectedEof` reads more; `InvalidData`/`Unsupported`/oversized are fatal and surface as
  `WireError::Protocol`. Decode arms were added for the ordinary `EmptyQueryResponse` (`I`,
  the reply to an empty query) and `NotificationResponse` (`A`, `LISTEN`/`NOTIFY`) — which
  were previously mistaken for unknown tags and wedged `simple_query("")` and any session
  that received a `NOTIFY` — and the `COPY` family (`G`/`H`/`W`) now decodes to an explicit
  `Unsupported` error rather than an infinite wait.
- **Federation local mutations read the row back instead of echoing the input (#430,
  M-fed-mut-executor).** `execute_local_mutation` built its response from the input `variables` and
  ran the `INSERT`/`UPDATE`/`DELETE` without inspecting the result, so it returned a fabricated
  "success" even when an `UPDATE`/`DELETE` matched no row (the entity didn't exist), and never
  reflected database-computed columns. The mutation SQL now uses `RETURNING *`; the response is the
  actual post-mutation row (`__typename` plus every returned column), and a 0-row `UPDATE`/`DELETE`
  returns `FraiseQLError::NotFound` (404). **Behavior change:** a federation mutation against a
  non-existent entity now fails loud instead of reporting success. (Un-parks the two
  `mutation_cross_graph` tests that were deferred to this work.)
- **The MSSQL→NATS bridge honours its configured `batch_size` (M-mssql-batch).** The change-log
  fetch query hardcoded `SELECT TOP (100)` and discarded the configured `batch_size` (a `let _ =
  batch_size` swallowed it), so a deployment that tuned the batch size was silently capped at 100
  rows per poll. SQL Server accepts a parameter in `TOP (expression)`, so the row cap is now bound
  (`TOP (@P1)`) from the configured value.
- **Twilio webhook signature verification decodes form bodies correctly (H44).** The
  percent-decoder pushed each decoded byte as its own `char` (Latin-1 per byte), so a UTF-8
  sequence like `%C3%A9` became `Ã©` instead of `é`, and `+` was never decoded to a space — so a
  legitimately-signed webhook whose body contained an accented character or a space failed
  verification. Decoding now accumulates bytes and interprets the result as UTF-8, and `+` decodes
  to a space. The vacuous test helper that re-implemented the in-repo signing algorithm (verifying
  the bug against itself) is deleted; the new tests sign with Twilio's published algorithm
  independently.
- **Webhook replay protection no longer wraps to reject every request (M-webhook-replay-drift).**
  `SlackVerifier`/`SendGridVerifier::with_tolerance` cast the `u64` tolerance with `as i64`, so a
  large configured tolerance wrapped to a *negative* window that rejected every timestamp
  (replay protection inverted into a total outage) — the wrap-safe fix had landed only in the
  Discord and Paddle copies. All five timestamped verifiers (Slack, SendGrid, Discord, Paddle,
  Stripe) now share one `check_timestamp_freshness` seam that stores the tolerance as a `u64` and
  saturates it to `i64::MAX` at comparison time, so the freshness logic can't drift between
  providers again.
- **Webhook errors map to an HTTP status that reflects fault (M-webhook-error-status).** Every
  `WebhookError` variant boxed into `FraiseQLError::Webhook`, which maps to HTTP 400 — so a
  transient database error while handling a webhook returned 400 ("permanent client error, do not
  retry") and the event was lost. The conversion now routes per variant: `Database` → 5xx
  (retryable, the sender re-delivers), `MissingSecret` → 5xx (a server-side misconfiguration),
  and only `InvalidPayload` (a genuinely malformed sender payload) stays 400.

- **Arrow schema inference maps JSON null to `Utf8`, not `DataType::Null` (H37).**
  `schema_gen`'s `infer_type_from_value` mapped a JSON `null` to `DataType::Null`, which the
  Arrow array converters reject — so a result column whose *first* row was `null` poisoned the
  entire batch, while the sibling `metadata.rs` path correctly mapped null to a nullable `Utf8`
  column. Both paths now route through one shared `json_value_to_arrow_type` helper (null →
  `Utf8`), so they cannot drift again; a pre-existing test that asserted the buggy `DataType::Null`
  result was corrected.
- **The S3 storage backend detects a missing object structurally (H40).** `download()` and
  `exists()` decided "not found" by string-matching the `SdkError` Display for `"NoSuchKey"` /
  `"404"` — but that Display is just `"service error"` (the status lives in the typed error), so
  the match never fired: `exists()` returned an error instead of `Ok(false)` and `download()` of a
  missing key surfaced a generic 500 instead of a 404. Both now inspect the typed service error
  (`GetObjectError::is_no_such_key` / `HeadObjectError::is_not_found`), matching the structural
  pattern already used in the server's storage path.

- **The PostgreSQL adapter no longer nulls NUMERIC, UUID, and timestamp columns
  (H35).** `row_to_map` decoded a fixed ladder of types (`i32`/`i64`/`f64`/`String`/`bool`/
  `text[]`/`jsonb`) and fell through everything else to `Null`, so a `SUM(revenue)` aggregate,
  any raw `NUMERIC`/`DECIMAL` column, a `uuid` column (e.g. `mutation_response.entity_id`), and
  `timestamptz`/`timestamp`/`date` columns all silently became JSON `null`. The ladder now
  decodes `NUMERIC`/`DECIMAL` (as a JSON number, via `rust_decimal`), `UUID` (canonical string),
  and chrono timestamps/dates (ISO 8601 text); a column whose type still isn't representable is
  logged with its name and PostgreSQL type instead of nulling silently. A cross-type conformance
  test pins the mapping so the next drift fails a shared test.
- **MySQL database errors now carry a usable SQLSTATE (H36).** The `execute_raw` path parsed
  `db_err.code()` — which already *is* the SQLSTATE string — as a MySQL error *number* and fed it
  to `map_mysql_error_code` (which expects numbers like 1062), so the mapping never matched and
  every raw-query error surfaced with `sql_state: None`; the #413 client-input classifier never
  mapped a MySQL constraint violation to HTTP 400. All SQLSTATE extraction in the adapter is now
  routed through one `mysql_sql_state` seam that reads MySQL's native error number via downcast,
  normalises the well-known integrity/serialization numbers to canonical SQLSTATEs, and falls
  back to MySQL's own SQLSTATE — so a duplicate-key violation now classifies as 400. The drifted
  inline copies (and the duplicate `map_mysql_error_code` in `helpers.rs`) are removed.

- **Federation `_entities` results are now positionally aligned to the input
  representations (H31).** The resolver grouped representations by typename and re-numbered the
  resolved entities with a per-group running counter, so for an interleaved request like
  `[User#1, Product#1, User#2]` the result array came back in group order
  (`[User#1, User#2, Product#1]`). Apollo Router zips the `_entities` result against the input
  array **by index**, so every consumer downstream of an interleaved batch received the wrong
  entity for a representation. Grouping now records each representation's original input index
  and scatters the resolved entities back to those positions, so the result array zips 1:1 with
  the input regardless of typename interleaving.

- **Every server constructor now applies the same schema-derived runtime config and boot
  validation (H16).** `Server::with_relay_pagination` and `Server::with_flight_service` (the
  Arrow Flight path) built the executor with `RuntimeConfig::default()`, so a server created
  via either constructor silently ignored the compiled `audit_logging_enabled` flag, the #421
  `max_page_size` ceiling (and its `FRAISEQL_MAX_PAGE_SIZE` override), and the change-log
  write toggle — and, unlike `Server::new`, never validated the compiled schema's format
  version or ran the at-rest-encryption refusal check (H12). The schema-derived config now
  flows through a single seam, `RuntimeConfig::from_compiled_schema`, that all three
  constructors call; the format-version validation is coupled into it so a constructor cannot
  obtain a config while skipping the check. The relay/Arrow constructors additionally run the
  H12 field-encryption boot refusal. (The #421 `page_size_precedence` helper moved from
  `fraiseql-server` to `fraiseql-core` alongside the seam.)
- **Authenticated multi-root queries no longer silently drop roots (H19).** The authenticated
  executor entry point (`execute_with_security`) had no multi-root branch, so a query like
  `{ users { id } posts { id } }` matched only the first root and silently discarded the rest;
  the anonymous path dispatched all roots in parallel. Both paths now route through one shared
  `execute_dispatch(.., Option<&SecurityContext>)` so the authenticated path also fans multi-root
  queries out in parallel (with the security context applied to every root), runs the GATE-1
  query-structure validator it previously skipped (L-gate1-skip), and consults the parse cache it
  previously bypassed (L-parse-cache). The `fraiseql_multi_root_queries_total` metric now counts
  authenticated multi-root queries too. (Also corrects a stale doc claim that the security context
  was "not yet applied" to aggregations/window/federation — it is, on both paths.)
- **REST error responses now use the correct HTTP status for every error variant
  (M-rest-error-mapper).** The REST `From<FraiseQLError>` mapper handled only a handful of
  variants and sent everything else to `500`, so `Conflict` (should be 409), `Timeout`/`Cancelled`
  (408), `RateLimited` (429), `ServiceUnavailable` (503), and `Unsupported` (501) were all reported
  as `500 Internal Server Error`. REST status is now derived from the canonical
  `FraiseQLError::status_code()` — the single source of truth shared with the GraphQL mapper
  (L-error-map-triplication) — with the one documented divergence being the #413 client-input
  SQLSTATE override (22xxx/23xxx → 400). A property test asserts REST status equals
  `status_code()` for every variant.
- **Observer audit-log write failures are no longer silently swallowed
  (M-observer-log-swallow).** The success- and error-path `INSERT INTO tb_observer_log` writes in
  the observer runtime discarded their result with `let _ = …`, so a failed audit-log write left no
  trace. Both now `warn!` with the observer and event id on failure (non-fatal — the event itself
  is already processed/counted).
- **Removed the dead `PreferHeader::applied_header_value` builder (L-prefer-header).** It built an
  RFC 7240 `Preference-Applied` header value that no production code emitted, and carried a no-op
  `resolution` branch. Emitting `Preference-Applied` is a deliberate REST feature to be added with
  its response-path wiring, not kept as dead code.
- **Error sanitization now defaults to ON in production (H7, behavior change).** A default
  deployment with no explicit `[security.error_sanitization]` config previously ran with
  sanitization disabled, so raw database/SQL error text (schema names, constraint detail, SQL
  fragments) could reach clients in `5xx` responses. The default is now **environment-aware** at the
  server boot seam: when `FRAISEQL_ENV` is not `development`/`dev` (i.e. production), sanitization
  is enabled; in development it stays disabled for verbose-error ergonomics. An explicit compiled
  config still overrides in either direction. The pure `ErrorSanitizationConfig::default()` shared
  with `fraiseql-cli` is unchanged (still `enabled = false`); only the runtime boot default flips.
  **Operators who relied on raw 5xx error text in production must set
  `[security.error_sanitization] enabled = false` explicitly.**
- **CLI gate flags now affect the exit code (H21).** `fraiseql lint --fail-on-critical` and
  `--fail-on-warning` printed a failure result but always exited 0, so they were inert as CI
  gates — a pipeline depending on them passed regardless of the findings. Lint now reports a
  `validation-failed` status when a gate trips and the runner exits **2** (the documented
  `validation_failed` code); operational errors (missing file, bad JSON) still exit 1. The
  lint output schema's failure variant is updated to `validation-failed` to match.
- **`fraiseql federation check` exits non-zero on a composition failure (H22).** A subgraph
  with composition errors (e.g. a federated type missing `@key`) printed the errors but
  exited 0, so federation composition gates in CI never failed. The command now exits 2 when
  the result is `validation-failed`.
- **`fraiseql setup` installs the dollar-quoted helper library correctly (#426).** The
  installer split the embedded SQL on `;` and ran the fragments individually, which shredded
  the `$$…$$` PL/pgSQL function bodies and the trailing `DO`-block self-test — so on a clean
  database it failed on the first body and installed zero helpers, leaving the documented
  install path unusable. It now runs the file as a single `batch_execute` (simple-query
  protocol), which understands dollar-quoting and multi-statement scripts the same way
  `psql -f` does.
- **`fraiseql compile` refuses to write its compiled output over the input file (H23,
  defense-in-depth).** A real write now errors when `--output` resolves to the input path,
  preventing the same source-clobbering class that motivated removing `serve` (below).
- **`PostgresSagaStore` no longer silently coerces corrupt state and ignores missing rows
  (M-saga-store-defaults, M-saga-rowcounts).** Row mappers coerced an unrecognised
  `state`/`mutation_type` string to a default (e.g. `Pending`), which could re-execute
  completed work; they now raise `SagaStoreError::CorruptStoredValue`. Step/saga writes
  ignored the affected-row count, so an update targeting a non-existent saga/step returned
  `Ok`; they now check it and raise `SagaNotFound`/`StepNotFound`.
- **The server refuses to boot when `FRAISEQL_SECRETS_BACKEND` is set on a build without
  the `secrets` feature (M-secrets-backend-stub).** The no-`secrets` build's
  `build_secrets_manager` returned `Ok(None)` unconditionally, so an operator who configured
  a secrets backend silently ran with none — believing secrets were managed when they were
  not. It now fails loud with an explicit error telling the operator to rebuild with
  `--features secrets` or unset the variable.
- **The `sql_query` host function fails loud instead of faking an empty result set
  (M-sql-query-stub).** A read-only-classified `SELECT` returned `Ok(vec![])` ("not yet
  implemented"), making a valid query look like it ran and matched no rows; it now returns
  `FraiseQLError::Unsupported`.
- **Deno and WASM function runtimes now share one failure contract (M-fn-failure-contract).**
  A guest WASM error was wrapped as *successful data* (`Ok(FunctionResult { value:
  {"error": …} })`) while the Deno runtime returned `Err` for the same failure. The WASM
  path now returns `Err(FraiseQLError::Unsupported)` for guest errors, timeouts, and traps,
  matching Deno — a guest failure can no longer be silently consumed as data.
- **Deno function duration is measured across execution, not just channel setup
  (M-deno-duration).** The elapsed time was captured immediately after spawning the executor
  thread, before awaiting the result, so reported durations were meaningless; it is now
  measured after the executor completes.
- **Federation mutation executor rejects unrecognised operation names (M-fed-mut-executor,
  partial).** `determine_mutation_type` defaulted any name without a `create`/`update`/`delete`
  prefix to `UPDATE`, so a typo'd or unsupported mutation silently issued an `UPDATE`; it now
  errors. The remaining read-back correctness (return the mutated row via `RETURNING` instead
  of echoing the input; treat 0-row `UPDATE`/`DELETE` as not-found) is documented and deferred
  to Phase 09 ([#430](https://github.com/fraiseql/fraiseql/issues/430)). The two cross-graph
  integration tests that relied on the old silent default (`verifyUser`,
  `executeTransaction`) are parked with `#[ignore]` pointing at the same issue, so they still
  compile as the acceptance spec for the Phase 09 rework.
- **`InMemoryStateStore` now evicts the oldest entry at capacity instead of returning 500
  (L-state-store-doc).** The struct documented LRU-style eviction, but `store` returned a
  `ConfigError` (500) once the cap was reached — an availability footgun under CSRF-state
  flooding. It now evicts the oldest (smallest-expiry) state to admit the new flow, keeping
  the map bounded while new logins keep working. Clock-read failure still fails closed
  (the store rejects rather than admitting a state whose TTL cannot be validated).
- **Clock failures now fail closed in four auth expiry checks (L-clock-failopen).**
  `Session::is_expired`, `OtpRecord::is_expired`, the multi-provider callback CSRF-state
  check, and `InMemoryStateStore::cleanup_expired` read the clock with
  `unwrap_or_default()`/`unwrap_or(0)`, so a clock failure yielded `now = 0` and treated
  expired sessions/OTP codes/CSRF states as still valid (fail-open) — contradicting the
  crate's fail-closed doctrine. They now treat an unreadable clock as expired/at-capacity.
- **`JwtValidator::validate_hmac` now emits the same audit log as `validate` (L-validate-hmac).**
  The HMAC path logged nothing on decode failure, expiry, temporal-claim rejection, or
  success; it now mirrors the asymmetric path's four audit points.
- **`auth_refresh` no longer records an audit success for a request that always fails
  (L-auth-refresh-500).** Access-token issuance (signing a JWT) is not wired, so refresh
  cannot complete; it logged `SessionTokenValidation` *success* and then returned 500. It
  now logs the refresh as a failure and returns the explicit not-implemented error.
- **Vault `rotate_secret` no longer self-deadlocks (H10).** It held the per-secret
  rotation mutex and then called `get_secret_with_expiry`, which re-acquired the same
  non-reentrant lock — a permanent hang on first invocation that wedged the
  lease-renewal loop. The fetch+cache body is now a lock-free helper both methods call
  while holding the lock exactly once.
- **Vault Transit encrypt/decrypt use padded standard base64 (H14).** Encryption sent
  `STANDARD_NO_PAD` plaintext (real Vault's Go `base64.StdEncoding` rejects unpadded for
  ~2/3 of lengths) and decryption decoded Vault's always-padded response with
  `STANDARD_NO_PAD` (errors on the trailing `=`). Both directions now use padded
  `STANDARD`, so Transit round-trips against a real Vault.

- **Account linking no longer collapses email-less provider identities into one account (H26, account takeover).**
  `link_or_create_user` previously keyed every account on the provider's email and treated a
  missing email as the empty string, so every user whose provider omits an email (a GitHub
  account with a private email is the canonical case) resolved to the **same** `user_id` —
  cross-user account takeover. Account linking is now fail-closed: cross-provider linking
  happens only when the provider supplies a non-empty, **verified** email; otherwise the
  identity is keyed on `(provider, provider_id)`, so distinct identities can never collapse and
  an unverified email can never link into another user's account.

### Changed

- **BREAKING (JSON-compile workflow only): the default naming convention is now `camelCase`,
  not `preserve`.** The `fraiseql-cli compile schema.json` + `fraiseql.toml` workflow now
  compiles to a `camelCase` GraphQL surface by default — `snake_case` columns/functions in the
  database, `camelCase` operation and field names exposed to clients, with mutation input keys
  recased `camelCase → snake_case` before they reach the SQL functions. This matches the
  standard GraphQL convention and the casing most (JS) clients expect. The default applies even
  when no `fraiseql.toml` is present. **Migration:** a backend relying on the old `snake_case`-
  on-the-wire behavior must set `[fraiseql.naming]\nconvention = "preserve"` to keep names exactly
  as authored. The author-in-TOML workflow (`TomlSchema`) is unaffected — it carries its own
  `naming_convention` (still defaulting to `preserve`).
- **The #366 external-write capture trigger now writes the after-image into `object_data`, not a
  `{op, before, after}` envelope (changelog_pre_image unification).** To make `object_data` the
  after-image from *every* producer (executor outbox AND capture trigger), the shipped capture
  trigger function (`core.fn_entity_change_log_capture`) now writes `object_data = to_jsonb(NEW)`
  (NULL for a DELETE) and, only for tables that opt into the pre-image, `object_data_before =
  to_jsonb(OLD)`; the Debezium `op` is the `modification_type` column. The change-log reader's
  `ChangeLogEntry::debezium_operation` / `after_values` / `before_values` were updated to match
  (op derived from `modification_type`, after from `object_data`, before from `object_data_before`).
  **Migration note:** any consumer that read trigger-captured `object_data` as a `{op,before,after}`
  envelope must switch to the column shape (or read the new `core.v_entity_change_log_debezium`
  view, which reconstructs the envelope). Executor-written rows are unaffected — they already wrote
  the after-image into `object_data`.
- **Default builds now link a single rustls crypto provider — ring (M-dual-crypto).** Every
  default build previously compiled *both* `aws-lc-rs` and `ring` into one `rustls 0.23`
  because `fraiseql-server` and `fraiseql-wire` pulled rustls/tokio-rustls with their default
  `aws_lc_rs` provider while the rest of the graph (reqwest, sqlx, lettre, tungstenite) used
  ring. The server's direct `rustls`/`tokio-rustls`/`rustls-pemfile` deps were dead (their
  `ServerConfig` plumbing was removed in v2.7.0) and are now dropped; `fraiseql-wire` pins
  `default-features = false` + `ring`. A new gate, `tools/check-crypto-providers.sh` (wired into
  `make security` and the Dagger security leg), asserts the default `fraiseql-server` build
  links one provider and one rustls major. The opt-in `metrics` and `aws-s3` features still pull
  additional stacks by design (documented in the gate).
- **The two side-by-side WebSocket stacks are collapsed to one (L-ws-stacks).** `tokio-tungstenite`
  is bumped `0.28 → 0.29` to match axum 0.8's transitive version, so `tungstenite`/`tokio-tungstenite`
  no longer compile twice; the corresponding `deny.toml` skip entries are removed.
- **BREAKING (`fraiseql-auth`):** `UserInfo.email` is now `Option<String>` (was `String`) and
  gains an `email_verified: bool` field; an empty/whitespace email claim is normalized to
  `None`. `AccountStore::link_or_create_user` now takes `(email: Option<&str>, email_verified:
  bool, provider, provider_id)` (was `(email: &str, provider, provider_id)`), and
  `AccountRecord.email` is now `Option<String>`. Implementors and direct callers of these
  published-crate APIs must update their signatures; the in-tree OAuth providers and handlers
  are already updated.
- **BREAKING (`fraiseql-auth`):** `AuthMiddleware::new` no longer takes `session_store` or
  `optional` (now `new(validator, public_key)`). Those parameters were stored but never
  consulted — no session-revocation check, no optional-auth handling — so they were removed
  rather than continue to advertise behavior that did not exist (L-authmw-ignores).
- **BREAKING (`fraiseql-cli`): the hidden `serve` command is removed (H23).** It derived its
  output path from the input via an extension swap (`.json` → `.compiled.json`); given an
  input with no `.json` segment (e.g. `serve fraiseql.toml`) the derived output path equalled
  the input, so it overwrote the source file with compiled output. Use `fraiseql run --watch`
  (compiles in-memory, no disk artifact, hot-reloads on change) instead.
- **BREAKING (`fraiseql-webhooks`): the crate docs no longer advertise capabilities it does
  not have, and the dead scaffolding is removed (M-webhooks-advertised).** The crate docs
  claimed built-in **idempotency** and **transaction boundaries** as Security Properties, but
  no inbound receiver pipeline exists — the crate provides signature verification and the
  `SignatureVerifier` trait as building blocks; the caller wires the pipeline. The docs now
  state that honestly (and a Paddle "RSA-SHA256" error was corrected to HMAC-SHA256). The 12
  never-constructed `WebhookError` variants and the unused `WebhookConfig`/`WebhookEventConfig`
  types are **removed** from the published API (the enum is `#[non_exhaustive]`, so exhaustive
  external matches already carry a wildcard arm). The real receiver pipeline is tracked in
  [#431](https://github.com/fraiseql/fraiseql/issues/431).
- **BREAKING (`fraiseql-federation`): distributed saga execution now fails loud instead of
  fabricating success (H32, H33, M-saga-coordinator, M-saga-recovery).** `SagaExecutor`
  (`execute_step`/`execute_saga`/`get_execution_state`), `SagaCompensator`
  (`compensate_saga`/`compensate_step`), `SagaCoordinator` (`create_saga`/`execute_saga`/
  `get_saga_status`/`cancel_saga`/`get_saga_result`/`list_in_flight_sagas`), and
  `SagaRecoveryManager` (`run_iteration`/`start_background_loop`) previously fabricated and
  **persisted** success — building fake result documents, marking sagas `Completed`/
  `Compensated` having done nothing, and (the coordinator) holding `Arc<dyn Any>`
  executor/compensator fields that contained `()`. They now return
  `SagaStoreError::NotImplemented`; nothing is persisted. The coordinator's
  `with_executor`/`with_compensator` builders (which accepted unusable `Arc<dyn Any>` values)
  are **removed**. The `lib.rs` maturity table no longer advertises sagas as production. The
  real implementation is planned and tracked in
  [#429](https://github.com/fraiseql/fraiseql/issues/429); the behavioural acceptance suite is
  retained (parked) as its specification.
- **BREAKING (`fraiseql-federation`): `construct_batch_where_clause` is removed
  (M-batch-where-dup).** It was a drifted, weaker duplicate of the production
  `construct_where_in_clause`: it interpolated key values as string literals and returned an
  empty (WHERE-less, full-table) clause when no conditions matched. It had no production
  caller; use `construct_where_in_clause`, which binds values as parameters and fails closed
  (`1 = 0`) on empty input. Compound-key coverage was ported to the canonical builder.
- **BREAKING (`fraiseql-observers`): the `sms`, `push`, `search`, and `cache` observer action
  types now fail loud instead of fabricating success (H24).** Their dispatch handlers
  delegated to stub actions that returned `success: true` and sent nothing — an observer
  configured with `type = "sms"` reported success on every event while delivering no SMS.
  `ActionConfig::validate()` now rejects these types with `ObserverError::UnsupportedActionType`
  at config-load time (a misconfigured observer refuses to start), and the dispatcher returns
  the same error at execution time. The fabricating stub types (`SmsAction`, `PushAction`,
  `SearchAction`, `CacheAction` and their `*Response` types) are **removed** from the public
  API. The `ActionConfig` enum variants are retained so existing configs still deserialize and
  receive a clear error. Real transports are tracked in
  [#428](https://github.com/fraiseql/fraiseql/issues/428).

## [2.7.0] - 2026-06-13

### Security

- **Complexity validator no longer pins a worker on crafted fragment spreads (H4, DoS).**
  The depth/complexity analyzer re-walked every fragment spread with no memoization, so a
  ~1 KB query with N chained fragments each spread `b` times forced `b^N` recursive walks —
  the audit's 31-fragment / branch-2 construction pins a Tokio worker for ~88 s, and because
  the full metric was computed *before* any limit comparison, the configured depth/complexity
  limits never got a chance to reject it (the validation step itself was the DoS, and the
  opt-in `TimeoutLayer` cannot preempt synchronous CPU-bound work). Each fragment's
  depth/complexity/alias contribution is now resolved exactly once and memoized by name, with
  fragment cycles detected and treated as over-limit (rejected, never recursed into) and an
  over-long spread chain capped as before — making validation linear in document size
  regardless of fragment topology. The same pass also closes a companion alias-amplification
  bypass: the old alias counter scored fragment spreads as 0, so aliases hidden inside a
  fragment spread many times never counted toward `max_aliases`; each spread now contributes
  the fragment's own alias count per occurrence. No configuration change; depth, complexity,
  and alias metrics are unchanged for all non-pathological queries.
- **REST `?select=` parser no longer panics on multi-byte UTF-8 or unbounded nesting
  (H17, H18; `rest` feature).** The parser walked a `Vec<char>` by character position but
  then byte-sliced the original `&str` with those positions, so any multi-byte UTF-8
  character before a slice boundary panicked with "byte index N is not a char boundary" —
  `GET /<resource>?select=%C3%A9` (decodes to `é`) aborted the request task (H17). Separately,
  a local `let mut depth = 1` inside the embedded-resource branch shadowed the recursion-depth
  parameter, so the recursive call always received `1` and the `MAX_PARSE_DEPTH` guard never
  fired; `?select=a(a(a(…)))` recursed without bound and a deep value overflowed the worker
  stack, aborting the **whole process** (SIGSEGV) (H18). The parser now translates character
  positions to byte offsets before slicing (no desync at any site) and propagates the true
  recursion depth so the nesting guard rejects over-deep input. A proptest asserts the parser
  returns a `Result` — never panics — over arbitrary UTF-8.
- **Error/log/audit paths no longer panic when truncating user-controlled text (H20).**
  Six display paths truncated strings with a fixed byte offset (`&s[..N]`), which panics when
  the cut lands inside a multi-byte UTF-8 character. The live ones were the query-timeout
  handler (`format!("{}...", &query[..100])`, duplicated across the anonymous and
  authenticated executors) — an attacker sends a slow query with a multi-byte char at byte
  99–100 so the timeout handler *itself* panics — and the syslog audit-export path, where a
  caller could place a multi-byte char at byte 200 to abort (and so suppress) their own audit
  record. A new `utils::text::truncate_at_char_boundary` / `truncate_for_display` helper
  truncates on character boundaries; the copy-pasted timeout snippet and the SQL-logger,
  query-trace, and error-formatter truncations now route through it. The same sweep fixed two
  stragglers of the class: the API-key `Authorization: ApiKey …` prefix check (`raw_key[..7]`
  on an attacker-controlled header) now compares on bytes, and the SQL logger's 2000-byte cut
  is char-safe. No behavioural change for ASCII input.
- **Panicking PostgreSQL/Arrow/GCS code paths now fail loud (H34, H38, L-gcs-expect).** Three
  remotely- or environment-triggerable panics are converted to errors:
  - **PostgreSQL `data` column (H34).** `execute_raw`, `execute_raw_with_session`, and the relay
    pager extracted the JSONB `data` column with `Row::get`, which panics on SQL NULL or a
    non-JSONB type — a backing view projecting NULL `data` (e.g. via a LEFT JOIN) turned a query
    into a request-path panic. PostgreSQL was the only backend that aborted here; all three sites
    now go through a shared helper that returns `FraiseQLError::Database` (naming the column and a
    bounded slice of the query) for both the NULL and the type-mismatch case.
  - **Arrow Flight `limit = 0` (H38).** A client ticket with `limit = 0` produced `batch_size = 0`
    and `slice::chunks(0)` panics in the authenticated `do_get` handler. `execute_optimized_view`
    now rejects `limit = 0` fail-loud with `InvalidArgument`, the client-derived batch size is
    clamped to `[1, 10_000]`, and every chunk loop is routed through one helper that floors the
    size at 1 — so no call site (present or future) can pass a zero chunk size.
  - **GCS JWT clock (L-gcs-expect).** `create_gcs_jwt` used `.expect()` on
    `SystemTime::duration_since(UNIX_EPOCH)`; it now returns `FraiseQLError::File` instead of
    panicking if the system clock is before the UNIX epoch.
- **`GET /auth/v1/authorize` is now rate-limited per IP (H25, DoS).** `social_authorize` carried
  a `RateLimiters` field it never consulted, and the endpoint matched none of the path-based
  rate rules — so each request inserted a `CSRF` state into the bounded in-memory store, and a
  single IP at ~17 req/s could keep it full, making the store reject all new states (500) and
  denying social login for everyone. The handler now checks the shared `auth_start` limiter on
  the transport-peer IP before touching the store and returns 429 (with `Retry-After`) when
  exceeded.
- **Wire protocol caps single-message size (M-wire-msg-cap, memory-exhaustion DoS).**
  `decode_message` validated only a *lower* length bound, and DataRow column values carry no
  per-column cap — so a malicious/compromised peer (or a non-TLS MITM) could declare a length up
  to ~2 GiB and force the connection read buffer to grow that large before any per-field cap ran.
  A `MAX_MESSAGE_LEN` (256 MiB) bound is now checked right after the length is read (a fatal
  `InvalidData`, ahead of the incomplete-body path), and the connection read loop refuses to
  buffer past that bound (`WireError::Protocol`). The broader malformed-vs-incomplete decode-error
  distinction (H42) lands in the wire-protocol phase.
- **Relay `node(id:)` now enforces row-level authorization (H2, IDOR).** The global
  object lookup `node(id: …)` resolved any type by opaque id while applying none of the
  backing query's `requires_role` / RLS / `inject_params` gates, so a leaked node id
  returned the row with no access control — an authenticated low-privilege user could
  read role-gated types or other tenants' rows, and an anonymous caller could read any
  registered type. The node path now enforces all three gates for the resolved type and
  fails closed: an anonymous lookup of an RLS-/inject-/role-gated type returns "not
  found" (null) instead of the raw row, and an authenticated lookup ANDs the RLS /
  `inject_params` filter onto the id. Relay connection pagination carried the same
  latent fail-open — an RLS-configured deployment silently dropped the RLS filter for
  anonymous callers, leaking every row — now also fails closed. **Behavioral change:**
  in deployments that configure RLS, anonymous `node(id:)` and anonymous relay
  pagination of protected types now return nothing / error rather than leaking rows.
- **Federation `_entities` now fails closed for gated entity types (M-fed-entities-rls).**
  The `_entities` resolver resolved entities by `__typename` while applying none of the
  backing query's `requires_role` / RLS / `inject_params` gates, so an anonymous caller
  under an RLS-configured deployment, or any caller requesting a role-gated type, could
  resolve protected entities by id. The path now denies (403) when: row-level security is
  configured and the request is unauthenticated; a requested type's backing query
  declares `requires_role` the request does not hold; or a requested type is
  `inject_params`-scoped (tenant/owner) and the request is unauthenticated — denials run
  before any SQL. When the request **is** authenticated, `inject_params`-scoped types are now
  row-filtered at the resolver (see the next entry); an app-level `rls_policy` `WhereClause`
  remains under the federation *trusted-gateway* assumption. The existing field-level
  fail-closed guard (deny when the schema declares any policy-gated field) is retained.
  **Behavioral change:** anonymous `_entities` resolution of RLS-/inject-gated types, and any
  `_entities` resolution of role-gated types without the role, now error rather than returning
  the entity.
- **Federation `_entities` now applies per-row tenant/owner scoping to authenticated requests
  (M-fed-entities-rls follow-up, C1b/R1).** Closing the `_entities` per-row gap left by the
  fail-closed C1b gate: for an authenticated caller, the resolver no longer resolves
  `inject_params`-scoped entity types "under the trusted-gateway assumption" (i.e. with no
  per-row filter). The runtime now composes the backing query's `inject_params` (tenant/owner
  scoping) into a columnar predicate — `"tenant_id" = $N` — and ANDs it onto the key `IN`
  lookup, and threads the caller's session variables onto the resolver's connection so
  `current_setting()` DB-native row-level security is enforced (the federation counterpart of
  the #329 connection-affine RLS fix). A direct `_entities` hit with arbitrary ids is therefore
  scoped to the caller's tenant/owner instead of resolving every requested row. The predicate is
  built as a native-column equality (never a JSONB `data->>` path), so it composes onto the
  columnar entity table; an app-level `rls_policy` `WhereClause`, which targets the JSONB view
  shape, is **not** composable onto that table and remains a documented trusted-gateway
  limitation. **Behavioral change:** in a multi-tenant deployment, an authenticated `_entities`
  request now returns only the caller-scoped rows for `inject_params`-scoped types; a foreign
  tenant's id resolves to `null`.
- **Admin-plane endpoints now enforce mandatory auth + admin scope (H5, H6).** The OIDC
  middleware (`oidc_auth_middleware`) defers to the validator's global `required` flag,
  which governs only the anonymous data plane — so any deployment that allowed anonymous
  GraphQL silently un-authed the admin routers too (H5). The observer admin API was also
  authenticated but not authorized: any valid end-user token could read observer
  `actions[].headers` (webhook bearer secrets) and drive DLQ retry-all / delete / observer
  mutation (H6). Two net-new middlewares fix this independently of the global flag:
  `admin_auth_middleware` (valid token **and** `fraiseql:admin` scope) now gates the
  observer admin API and the design-audit API; `required_auth_middleware` (valid token,
  any scope) now gates the introspection, schema-export, and schema-metadata endpoints so
  that "require auth" actually rejects anonymous callers. Endpoints already configured
  with `*_require_auth = false` keep their explicit open-mount behavior. As defense in
  depth (R8), observer read/write responses now redact webhook secret values in
  `actions[].headers` (`[REDACTED]`) so secrets never travel in a response body.
- **Storage object overwrites now require ownership (H9, B4 — overwrite IDOR).** The
  upload path checked only bucket-level write permission (`can_write`, satisfied by any
  authenticated user), never the existing object's owner — so user B could clobber user
  A's object data by writing to its key (`metadata::upsert` preserved A's `owner_id` on
  conflict, but the bytes were overwritten). Both write doors are affected: `PUT
  /storage/v1/object/{bucket}/{key}` (H9) and `POST /storage/v1/presign/{bucket}/{key}`
  with `operation=upload` (B4 — a presigned PUT that overwrites a foreign object). Both
  now load any existing object and gate on a new `can_write_object` check: creating a new
  object still needs only authentication, but overwriting an existing one requires owner
  match or the admin role (mirroring `can_delete`). A non-owner overwrite returns `403`;
  anonymous callers always return `401` (no object-existence oracle). **Behavioral
  change:** uploads that overwrite an object owned by another user now fail instead of
  silently replacing its contents.
- **Arrow Flight `BulkExport` is now fail-closed behind a table allow-list (H39).** The
  Flight `BulkExport` ticket ran `SELECT * FROM "<table>"` for any client-supplied table
  with no allow-list and no per-user RLS filtering (the `SecurityContext` was only logged),
  so an authenticated Flight client could dump any table. `FraiseQLFlightService` now
  carries a `bulk_export_allowed_tables` allow-list (`None` by default = `BulkExport`
  disabled); `execute_bulk_export` returns `permission_denied` unless the requested table
  was explicitly opted in via the new `with_bulk_export_tables(...)` builder. The
  misleading documentation on `execute_optimized_view` (which claimed per-user RLS was
  applied) and `execute_bulk_export` is corrected to state plainly that these raw-SQL
  Flight paths apply **no** per-user RLS filtering and must be gated by configuration / the
  underlying view. **Behavioral change:** Arrow Flight `BulkExport` is disabled until an
  operator allow-lists specific tables.
- **Realtime broadcast endpoint now requires the admin plane (M-broadcast).** `POST
  /realtime/v1/broadcast` — which pushes an arbitrary event to every connected client — was
  mounted with no authentication whenever a broadcast manager was configured. It is now
  gated by `admin_auth_middleware` (valid token **and** `fraiseql:admin` scope), consistent
  with the design-audit API, and **fails closed**: with no OIDC validator configured to
  authenticate the admin plane, the endpoint is not mounted at all. **Behavioral change:**
  broadcasting now requires an admin-scoped token, and deployments without an OIDC validator
  no longer expose the broadcast endpoint.
- **Introspection now hides role-gated mutations (M-introspection-mut).** The introspection
  endpoint filtered role-gated *types* and *queries* out of its response (enumeration-hiding)
  but emitted the *mutations* list unfiltered, leaking the name and return type of every
  `requires_role` mutation to any caller — including anonymous ones. Mutations are now subject
  to the same `requires_role` filter, so a caller never sees a mutation it could not invoke.
- **Storage admin role decollided from the generic `"admin"` (M-storage-scope).** The storage
  RLS evaluator treated any role literally named `"admin"` as a full-access storage admin, and
  the server maps an OIDC token's `scopes` verbatim into a user's storage roles — so any token
  carrying an unrelated `admin` scope (a common scope name) silently gained read/overwrite/delete
  on every object in every bucket. The bypass role is now the explicit, storage-namespaced
  `fraiseql:storage:admin` (exported as `fraiseql_storage::STORAGE_ADMIN_ROLE`), and the static
  `storage_token` admin grant was updated in lockstep. **Behavioral change:** a generic `admin`
  role/scope no longer confers storage admin; grant the explicit `fraiseql:storage:admin` scope
  instead.
- **Legacy / unauthenticated storage mounts now fail closed (M-storage-legacy).** Two storage
  mount paths previously served an unauthenticated API: the legacy backend mount (which has *no*
  RLS evaluator) mounted with no auth layer when `storage_token` was unset — world-readable and
  world-writable — and the hardened RLS mount served an anonymous-only API when neither
  `storage_token` nor an OIDC validator was configured. Both now refuse to mount (logging a
  `SECURITY` error) unless an authentication mechanism is configured. **Behavioral change:** a
  storage deployment with no `storage_token` and no OIDC validator no longer exposes the storage
  routes at all.
- **Relay-enabled executors apply the same introspection filtering as non-relay ones
  (L-relay-inaccessible).** The relay constructor (`new_with_relay`) built its introspection
  responses without the federation `@inaccessible` field filter that the non-relay constructor
  applies, leaving the two paths free to diverge. Both constructors now build introspection
  through a single shared helper so a relay executor can never expose an `@inaccessible` field
  in `__type`/`__schema` that the non-relay path would hide (defense-in-depth).
- **Multi-tenant subscriptions fail closed on the tenant gate (M-tenant-ws-failopen).** The
  `WebSocket` subscription matcher only filtered events when *both* the subscription and the
  event carried a tenant id; a subscriber with no tenant id matched **every** tenant's events,
  and a tenant-scoped subscriber still received untagged events. In multi-tenant deployments
  (`security.multi_tenant = true`) the gate now requires both sides to carry the *same* tenant —
  a missing tenant on either side never matches. Single-tenant deployments keep the permissive
  behavior (tenant ids are typically absent), so they are unaffected. **Behavioral change:** in
  a multi-tenant deployment a subscription that does not resolve a tenant id now receives no
  events, and events without a tenant id are not delivered to tenant-scoped subscribers.
- **Suspended tenants are rejected on the subscription `WebSocket` path (M-tenant-ws-suspended).**
  Tenant suspension (`TenantStatus::Suspended`) returned 503 on the GraphQL data plane but was
  not consulted for subscriptions, so a suspended tenant could still open subscriptions and keep
  receiving events. The subscription path now consults the tenant registry through a new
  `TenantStatusSource`: a new subscription whose resolved tenant is suspended is rejected with a
  `TENANT_SUSPENDED` error, and event delivery to a connection whose tenant becomes suspended
  mid-stream is paused (re-checked per event).
- **Per-tenant concurrency quotas are now enforced (M-quotas).** `TenantQuota.max_concurrent`
  was configurable and a per-tenant concurrency semaphore existed, but the GraphQL request path
  never acquired a permit, so the limit was silently ignored. The handler now acquires a
  concurrency permit (held for the duration of the request) after resolving the tenant executor,
  for explicitly-keyed registered tenants; exceeding the limit returns HTTP 429 Too Many Requests
  (previously a tenant-dispatch `RateLimited` collapsed to 403). Requests with no explicit tenant
  key (the default executor) are unlimited, as before.
- **Per-tenant per-second rate limiting is now enforced (M-quotas, RPS follow-up).**
  `TenantQuota.max_requests_per_sec` was configurable but had no enforcement primitive and was
  silently ignored. Each tenant now carries a fixed one-second-window rate limiter (the audited
  `KeyedRateLimiter` from `fraiseql-auth`), and the GraphQL request path checks it at the same
  chokepoint as the concurrency permit — for explicitly-keyed registered tenants only. Exceeding
  the configured requests-per-second returns HTTP 429 Too Many Requests (reusing the C7
  `RateLimited` → 429 dispatch mapping); the default executor and tenants without a per-second
  quota are unaffected. Enforcement requires the default-on `auth` feature (which provides the
  limiter); a `--no-default-features` build parses `max_requests_per_sec` but logs a warning at
  registration that it is not enforced. The limiter is per-process, so an *N*-replica deployment
  enforces *N* × the configured rate — configure a distributed backend for true global limiting.
- **MySQL stored-procedure mutation path is now parameterized (C1, critical).**
  `CALL` statements on the MySQL backend bound arguments by inline string-escaping
  that doubled single quotes only and left backslashes untouched; under MySQL's
  default SQL mode a GraphQL mutation argument like `\', …; -- ` could break out of
  the string literal and execute injected SQL (the driver negotiates
  `MULTI_STATEMENTS`). Both call paths (`execute_function_call` and the Change-Spine
  outbox variant) now bind arguments as prepared-statement parameters
  (`CALL fn(?, …)`) and the inline escaper is removed. Affects every published
  release with the MySQL backend.
- **Webhook `body_template` values are JSON-escaped (H11).** Observer webhook bodies
  were built by substituting entity-field values into a string template and
  re-parsing the result, so an attacker-controlled string field (a username,
  comment, …) could break out of its JSON string and inject or override keys in the
  HMAC-signed (`X-FraiseQL-Signature-256`) payload. String values are now
  JSON-escaped into their surrounding string context; typed (number/bool) slots and
  plain-text bodies are preserved. The Slack and email paths were already safe.
- **Aggregation, federation, full-text, and relay SQL paths hardened against
  injection (H1, H3, H41, and latent M-/L- sites).**
  - GROUP BY dimension aliases — echoed verbatim from GraphQL variable JSON keys
    into the SELECT list — are validated as `[_A-Za-z][_0-9A-Za-z]*` at parse time,
    independent of the compile-time dimension allowlist (H1).
  - Federation `_entities` resolution binds key-field values as dialect-native
    parameters instead of single-quote-escaping them (unsafe on MySQL), validates
    key/field identifiers, and never selects `@inaccessible` / `@external` fields
    (H3, M-fed-select-list); the federation `escape_sql_string` helper is removed.
  - Full-text search `language` (regconfig) is validated against `[a-z_]+` in
    `WhereOperator::validate()` before it reaches `plainto_tsquery` in the published
    `fraiseql-wire` crate (H41).
  - The SQL Server relay ORDER BY builders validate order-by field names before
    interpolating them into `JSON_VALUE` paths (M-relay-orderby), and the row-view
    DDL codegen skips field names that are not safe identifiers (L-row-views).
- **Removed a dead, dialect-incomplete tenant-filter helper.** The unused
  `TenantEnforcer::enforce_tenant_scope_sql` (string-concatenation tenant filter
  with incomplete escaping) was deleted; the parameterized AST-based
  `enforce_tenant_scope` is the supported path (L-tenant-enforcer).
- **Advisory-gate hardening (Phase 02).** `make audit` / `make security` no longer
  fail on a clean tree: `.cargo/audit.toml` and `deny.toml` ignore lists are now
  kept in lockstep by `tools/check-audit-lockstep.sh` (wired into both targets and
  the Dagger ShellGates). A new `tools/check-deadlines.sh` fails the build once an
  accepted-advisory deadline in `deny.toml` lapses, and the Dagger security leg now
  runs `cargo audit` alongside `cargo deny`. The rustls-webpki advisories
  (RUSTSEC-2026-0098/0099/0104, behind the opt-in `aws-s3` feature) had their
  acceptance deadline extended to 2026-09-01: a spike confirmed no aws-config
  feature selects rustls 0.23 over the legacy rustls-0.21 connector, so the
  migration is tracked as Phase 12 (aws-stack bump).
- **Token revocation is now enforced on every request, and revoke-all actually
  revokes (H8, M-revoke-all).** Revocation was write-only: `POST /auth/revoke[-all]`
  recorded revoked tokens, but the OIDC auth middleware validated the JWT and decoded
  its `jti` **without ever consulting the revocation store**, so a revoked token kept
  working until its natural `exp` — logout, compromise response, and admin force-logout
  were silent no-ops (H8). The middleware now checks the revocation store after token
  validation on every authenticated route (data plane *and* admin plane) and rejects
  with 401. Separately, `revoke-all` was inert across all three backends: `revoke`
  records no `sub`, so the old `revoke_all_for_user` (a `sub`-keyed delete on
  in-memory/Postgres, a phantom-namespace `SCAN` on Redis) always affected 0 rows
  (M-revoke-all). `revoke-all` now records a per-user *epoch* and the request path
  rejects any of that user's tokens whose `iat` is at or before it — catching tokens
  that were never individually revoked (and tokens with no `jti`). New
  `[security.token_revocation] revoke_all_ttl_secs` (default 86400) bounds epoch
  retention; set it above your maximum access-token lifetime. The HS256 auth path is
  unaffected (revocation routes mount only with an OIDC validator).
  **Breaking / behavioral change:** enabling `[security.token_revocation]` now actually
  enforces it — with `require_jti = true` (default) a validated token that lacks a `jti`
  claim is rejected 401 post-validation; set `require_jti = false` to admit jti-less
  tokens (losing per-token revocation, keeping the revoke-all epoch). The
  `POST /auth/revoke-all` response body changed from `{ "revoked_count": N }` to
  `{ "revoked": true }` (the epoch design has no per-token count).
- **REST error responses no longer leak raw database error text (H7).** With error
  sanitization enabled, GraphQL stripped internal detail from `DatabaseError` /
  `InternalServerError` responses, but the REST surface had **zero** sanitization: a
  server fault (undefined function `42883`, `XX000`, a connection error, …) rendered
  `FraiseQLError`'s raw message — schema names, constraint details, SQL fragments —
  verbatim into the `{"error":{"message":…}}` body. (The dedicated sanitization
  middleware meant to cover this was orphaned: never declared in `mod.rs`, never
  layered, and its body-shape matcher did not even recognise the nested REST error
  shape.) REST now applies the **same** sanitization gate as GraphQL at its
  error-rendering site: when `[security.error_sanitization]` is enabled, 5xx bodies
  carry the generic `custom_error_message` (default `"An internal error occurred"`)
  and the raw detail is logged server-side instead. Client-facing 4xx messages —
  validation, auth, not-found, and SQLSTATE 22/23 client-input faults (#413) — are
  intentional and pass through unchanged. The orphaned middleware module was deleted
  (two sanitization layers with divergent body-shape assumptions invite drift).
- **The server now refuses to boot when a field is marked for at-rest encryption,
  instead of silently storing it in plaintext (H12).** Field-level at-rest encryption
  was advertised but never worked end-to-end: the write/mutation path does not encrypt
  (`FieldEncryptionService::encrypt_variables` has no caller), so a field marked
  `encryption` was written to the database in **plaintext** and the read path then
  failed to decrypt it, returning HTTP 500 on every read — and when the `secrets`
  feature was absent the field round-tripped silently in plaintext, so operators
  believed sensitive columns were encrypted at rest when they were not. Rather than
  ship a security control that silently does the opposite of what it claims, the server
  now performs a startup check and **refuses to start** when any compiled-schema field
  declares `encryption`, naming the offending field(s) and how to remove the marker.
  The false "transparently encrypted… decrypted when read back" claims on
  `FieldDefinition.encryption` / `FieldEncryptionConfig` and in the `fraiseql-secrets`
  README were corrected. End-to-end field encryption (write-path call, array/nested
  recursion, `(type, field)` keying, ciphertext versioning, key KDF/zeroize) remains
  unimplemented and is tracked for a future release.
  **Breaking change:** a deployment whose compiled schema marks any field for
  encryption will now fail to start (it was previously 500-ing on every read of that
  field, or silently storing plaintext); remove the `encryption` marker and any
  `[security.field_encryption]` config to boot.
- **Removed dead field-encryption audit logging and its false compliance claims (H13).**
  `fraiseql-secrets` advertised "audit logging — track all secret access for compliance
  (HIPAA/PCI-DSS/GDPR/SOC 2)," but `AuditLogger` was an in-memory `Vec` commented "for
  testing" with no persistence or tracing sink, invoked from nowhere — it audited
  field-encryption operations that, after the H12 fix, cannot occur at all. The dead
  module was deleted and the false at-rest-encryption / audit-logging claims were excised
  from the `fraiseql-secrets` crate docs and README. (This does **not** affect the
  separate, genuinely-wired server/auth audit system configured via
  `[security.audit_logging]`, which continues to record mutations and admin operations.)
- **Security response headers are now sent on every response (M-sec-headers).** The
  `security_headers_middleware` (`X-Content-Type-Options: nosniff`, `X-Frame-Options:
  DENY`, `Strict-Transport-Security`, `Referrer-Policy`, `Content-Security-Policy`,
  `X-XSS-Protection: 0`) existed but was never layered, so none of these headers were
  emitted. It is now applied globally in `apply_middleware`. The headers are set
  *if-absent* so a handler can opt into its own policy — the GraphQL playground sets a
  relaxed CSP for its CDN-loaded IDE assets, which the global strict CSP no longer clobbers.
- **Mutations over HTTP GET are now rejected with 405 (M-get-mutations).** A mutation
  sent via `GET /graphql` was executed with only a log warning, sidestepping the POST-only
  CSRF posture; detection was also an unreliable `mutation` string-prefix match. The GET
  handler now parses the operation and returns **405 Method Not Allowed** for mutations,
  per the GraphQL-over-HTTP spec (queries over GET are unaffected). **Behavioral change:**
  clients that (incorrectly) sent mutations over GET now receive 405 instead of a result.
- **The auth brute-force limiter no longer trusts `X-Forwarded-For` (M-xff-limiter).** When
  `ConnectInfo` was unavailable (some library embeddings), the per-IP failed-auth limiter
  fell back to keying on the attacker-controlled `X-Forwarded-For` header, letting a caller
  rotate it to mint a fresh failure budget per value. The XFF fallback was removed: the
  limiter keys only on the validated transport peer, and when that is absent all callers
  share one bucket (fail-closed, not bypassable). The shipped binary always supplies
  `ConnectInfo`, so its behaviour is unchanged.
- **The server refuses to boot under an enabled server-side `[tls]` config instead of
  serving plaintext while claiming TLS (M-tls-enforce).** FraiseQL does not terminate TLS
  itself — it serves plaintext HTTP and expects a reverse proxy / load balancer / service
  mesh in front. The `[tls]` section was parsed and validated, a rustls `ServerConfig` was
  built from it and then **silently discarded**, the listener kept serving plaintext, and
  startup logged `mtls_required = true` — a server that claimed mutual TLS while doing no
  certificate check at all. The server now **refuses to start** when `[tls].enabled` is set,
  with a message directing operators to terminate TLS at a proxy (or remove `[tls]`). The
  dead server-side TLS plumbing (`TlsEnforcer`, `create_rustls_config`, certificate/key
  loaders) was removed; **database** connection TLS (`[database_tls]`:
  `postgres_ssl_mode`, `redis_ssl`, …) is fully retained.
  **Breaking change:** a deployment that set `[tls]` expecting the server to terminate TLS
  will now fail to start (it never actually terminated TLS — it served plaintext);
  terminate TLS in front of the server and remove `[tls]`.
- **Patched Postgres-protocol denial-of-service advisories.** Bumped `tokio-postgres`
  0.7.17 → 0.7.18 and `postgres-protocol` 0.6.11 → 0.6.12 (semver-compatible) to pick up
  fixes for RUSTSEC-2026-0178 (unbounded SCRAM iteration count → CPU-exhaustion DoS),
  RUSTSEC-2026-0179 (panic decoding a malformed `hstore` value), and RUSTSEC-2026-0180
  (panic on a `DataRow` with fewer fields than columns). Also dropped the now-stale
  `RUSTSEC-2026-0002` (lru) ignore from `deny.toml` / `.cargo/audit.toml`, which no longer
  matches any crate in the tree.

### Added

- **External-write capture for subscriptions (#366).** Uncooperative external
  writes — a raw `INSERT`/`UPDATE`/`DELETE` from psql, a migration, or a
  third-party tool — now reach GraphQL subscribers, without double-emitting for
  writes that already flow through FraiseQL's mutation executor. The executor sets
  a transaction-local marker (`fraiseql.cdc_mediated = 'on'`) at the start of every
  mutation transaction; a shipped, suppressible fallback trigger
  (`core.fn_entity_change_log_capture`) writes a contract-conforming
  `core.tb_entity_change_log` row only when that marker is absent — so an app-path
  write keeps its rich in-transaction outbox row and the trigger no-ops, while an
  external write is captured with a Debezium-style `{op, before, after}` envelope
  and fans out through the existing change-log reader and NATS bridges. The
  triggers are statement-level with transition tables, so a bulk statement captures
  all its rows in a single set-based INSERT (one event per changed row) rather than
  firing per row. Declare which tables feed a type with
  `@fraiseql.type(subscribable_tables=["tb_post"])`; the new
  `fraiseql generate-capture-triggers -s schema.compiled.json | psql "$DATABASE_URL"`
  command emits the self-contained, idempotent install DDL. No new infrastructure:
  plain triggers, no `wal_level=logical`, no replication slots — works on any
  managed PostgreSQL. See `docs/architecture/external-write-capture.md`.
- **Actor model on the Change-Spine envelope (#390).** Every audited operation now
  carries a first-class actor classification — `human_user`, `service_account`,
  `ai_agent`, or `system_job` — derived onto the `SecurityContext` at
  authentication and stamped into the change-log `actor_type` column by the
  in-transaction outbox write. For a delegated agent request (RFC 8693 `act`
  claim), the change-log `acting_for` column records the underlying human's
  public-facing UUID. The tenant lifecycle audit log (`TenantEvent`) gains the same
  `actor_type` / `acting_for_user_id` fields, now populated from the request
  principal at every tenant-admin endpoint (previously the actor was always NULL).
  API-key requests classify as `service_account`. The classification is recorded
  for forensics, not consumed as an authorization input.
- **Change-log reader surfaces the full Change-Spine envelope (#390 follow-up).**
  The observer change-log reader now projects the `actor_type` and `acting_for`
  columns onto `ChangeLogEntry` and the emitted `EntityEvent`, so out-of-session
  consumers (the NATS bridges, CDC fan-out, DLQ handlers) receive the actor
  classification and delegated-human UUID — not just the in-process listener. The
  PostgreSQL, MySQL, and SQL Server NATS bridges are brought to full envelope
  parity in the same pass: they now also carry `tenant_id`, `duration_ms`, and
  `seq` (previously only `user_id` survived the bridge). `EntityEvent`'s envelope
  fields gained `#[serde(default)]` so a consumer can decode an event serialized
  before these fields existed (forward/backward wire tolerance over NATS).
- **Change-log reader surfaces `schema_version` (#377 follow-up).** The observer
  change-log reader now projects the Change-Spine `schema_version` envelope column
  onto `ChangeLogEntry` and the emitted `EntityEvent`, and all three NATS bridges
  (PostgreSQL / MySQL / SQL Server) carry it across the bridge — so out-of-session
  consumers can audit which producer schema version wrote a change (e.g. "which
  schema produced this dead-lettered action"; see
  `docs/operations/zero-downtime-deploys.md`). The listener's row decode was
  converted from a positional tuple to a named `sqlx::FromRow` struct, removing the
  16-column tuple ceiling it had reached and the positional fragility. The field is
  `#[serde(default)]` for NATS wire tolerance.

### Changed

- **BREAKING (change-log contract):** the `acting_for` column is retyped
  `BIGINT → UUID` across the PostgreSQL / MySQL / SQL Server contract DDL to hold
  the delegated human's public-facing UUID (mirroring `tenant_id`). The column
  shipped NULL-by-design in v2.6.0 with no producer, so the migration's guarded
  retype is lossless; re-run migration `08` (and the `09`/`10` variants) to adopt
  it. `doctor --against-db` reports the type drift until a database is re-migrated.

### Fixed

- **gRPC mutations always reported failure.** The gRPC mutation handler read a
  non-existent `status == "success"` column from the `mutation_response` row instead of
  the canonical `succeeded` boolean (see `core::runtime::mutation_result`), so every gRPC
  mutation returned `success = false` regardless of the actual outcome. It now reads
  `succeeded`.
- **REST 204 No Content responses carried a `{}` body.** The REST response renderer wrote
  `{}` for an absent body (`None.unwrap_or(json!({}))`), giving 204 No Content (e.g. a
  `DELETE`) a 2-byte body in violation of the HTTP spec. A `None` body now emits an empty
  body.
- **REST error responses dropped structured `details`.** The REST error renderer wrote
  only `code` + `message`, discarding `RestError.details` — so a 422 validation failure's
  `missing_fields`, and any other structured error detail, never reached the client.
  Errors now render via `RestError::to_json`, preserving `details` (internal-error details
  are still stripped when error sanitization is enabled).

### Documentation

- **Zero-downtime deploy guide (#378).** New `docs/operations/zero-downtime-deploys.md`
  documents rolling, blue-green, and canary deploys behind a load balancer, the
  expand/contract migration discipline, and the in-process primitives FraiseQL already
  provides: in-place atomic schema reload (`SIGUSR1` / `POST /api/v1/admin/reload-schema`),
  the graceful shutdown drain, the `schema_format_version` boot guard, and schema-decoupled
  observer DLQ retry. Establishes that deploy-time version coherence belongs in the deploy/LB
  layer (with [fraisier](https://github.com/fraiseql/fraisier) as the worked example), not in
  per-request dual-schema routing inside the server. Corrects two stale claims in
  `compiled-schema-lifecycle.md` (it asserted "no hot reload" and a non-existent
  `fraiseql_version` major/minor guard).

### Added

- **Change Spine: the mutation executor writes the `core.tb_entity_change_log` outbox row
  in-transaction.** Every successful, state-changing mutation now records exactly one
  change-log row **inside the mutation function's own transaction, on the same connection** —
  a transactional outbox, the first runtime step of the Change Spine. The write is a single
  statement: the function call is wrapped in a `MATERIALIZED` CTE (so a volatile mutation
  function runs exactly once) whose data-modifying CTE INSERTs the row and whose primary query
  returns the function's row unchanged to the caller — no extra connection acquire, atomic with
  the mutation (a crash leaves neither the change nor the log row). The row carries the
  changed-entity columns straight off the `app.mutation_response` row (`object_id`,
  `object_data`, `updated_fields`, `cascade`), the DML verb in `modification_type` (`INSERT` /
  `UPDATE` / `DELETE` / `CUSTOM`, from the mutation's `operation`), `object_type` (the entity
  type, falling back to the GraphQL return type), and a wall-clock `duration_ms` computed on
  the DB clock from the txn-local `fraiseql.started_at` and stamped with
  `extra_metadata.duration_calc_version = 2`. The executor also stamps `tenant_id` (the UUID
  tenant from the request's `SecurityContext` — left NULL for a non-UUID tenant, never aborting
  the mutation) and `commit_time`, while `seq` comes from the table's global sequence default;
  it also stamps the envelope `trace_id` + `trace_context` (#375) and `schema_version` (#377) — see
  the dedicated entries below. `actor_type` / `acting_for` ship as columns but stay NULL pending
  #390. Only an effective change (`succeeded AND state_changed`) is logged — no-ops
  and business-logic failures do not produce a spine event. Implemented for PostgreSQL, MySQL,
  and SQL Server (see the multi-DB outbox-wiring entry below). **Opt-out (default-on):** the write can be
  disabled globally — `[changelog] write_enabled = false` in `fraiseql.toml`, or
  `FRAISEQL_CHANGELOG_ENABLED=false` at runtime — and per endpoint via the compiled-schema
  `MutationDefinition.changelog` flag (serde-defaults to `true`), authored as
  `@fraiseql.mutation(changelog=False)` (Python) or `@Mutation({ changelog: false })`
  (TypeScript). A row is written only when the global switch and the per-mutation flag
  are both on. The contract is documented in `docs/architecture/change-log-contract.md`.

- **Prepared-statement caching on the mutation function-call path — large mutation-throughput
  win.** The PostgreSQL adapter now uses deadpool's per-connection `prepare_cached` for
  `execute_function_call` and its session-affine / change-log variants, so PostgreSQL parses
  and plans each mutation's statement **once per connection** instead of re-parsing it on every
  call. In a 40-worker concurrent benchmark this lifted baseline mutation throughput by roughly
  **+60%** (≈20k→33k RPS on the test box). It is also what makes the in-transaction change-log
  outbox above effectively free: the outbox CTE's ~33% apparent cost was almost entirely
  repeated parse/plan, not the durable write — with caching the outbox penalty collapses to
  within noise on a PK-only table (the residual on the fully-indexed contract table is
  secondary-index maintenance, a write-vs-read tradeoff in the index strategy).

- **Change Spine: multi-DB outbox portability + reader reconcile.** A portable,
  fully-parameterized outbox INSERT builder (`fraiseql_db::changelog::build_changelog_insert_sql`
  over `CHANGELOG_PORTABLE_INSERT_COLUMNS`) emits the contract shape for PostgreSQL / MySQL /
  SQLite / SQL Server, and the contract migration now ships MySQL (`09_*`) and SQL Server
  (`10_*`) DDL variants — so cooperative external producers (and the non-PostgreSQL adapters,
  now wired — see below) write the same shape. The change-log poller's row decoder is reconciled
  to the Trinity column types (`fk_* = BIGINT`, public id = `UUID`, nullable `object_data`); its
  public string-based API is unchanged.

- **Change Spine: live MySQL and SQL Server in-transaction outbox.** The MySQL (sqlx) and SQL
  Server (tiberius) adapters now write the `tb_entity_change_log` outbox row themselves, atomic
  with the mutation — the multi-DB counterpart of PostgreSQL's in-txn CTE. Since neither dialect
  can reference a `CALL`/`EXEC` result set in a following `INSERT … SELECT`, each opens a
  transaction, parses the `mutation_response` row in Rust, and INSERTs the outbox row on the same
  connection before commit (a raised procedure or a failed INSERT rolls back both). `duration_ms`
  / `started_at` are legitimately NULL on these dialects (no request-scoped DB clock); `seq` fires
  from the table default. Wiring against live MySQL 8.3 and SQL Server 2022 surfaced and fixed
  three latent bugs: the MySQL `09_*` DDL gave `id CHAR(36)` no default (the portable INSERT omits
  `id`, like PG/MSSQL); both the `09_*`/`10_*` DDL and the portable INSERT builder emitted the
  reserved word `cascade` unquoted (a syntax error on MySQL and SQL Server) — the builder now
  quotes column identifiers per dialect; and the MySQL `CALL` runs over sqlx's binary protocol
  (the text-protocol `raw_sql` cannot form a `Send` future over `&mut MySqlConnection`), reading
  its result columns by ordinal. SQLite (read-only) and mock adapters keep the no-op default.

- **`fraiseql doctor --against-db` — change-log contract drift check (#380).** Reports drift
  between a live `core.tb_entity_change_log` and the shipped contract: missing columns the
  additive migration will add (warning), app-specific extra columns it leaves untouched
  (warning), and — the one drift it *cannot* reconcile — a pre-existing column with the wrong
  type (failure), e.g. a legacy `object_id text` the contract wants as `uuid` (`ADD COLUMN IF
  NOT EXISTS` no-ops on an existing column and cannot retype it). The expected column set is
  sourced from the single typed contract definition shared with the migration DDL
  (`fraiseql_observers::migrations::ENTITY_CHANGE_LOG_CONTRACT`). Runs alongside the #409
  PL/pgSQL body-resolution pass under the same `--against-db` flag.

- **Authoring-SDK surface for the per-mutation change-log opt-out.** The Change-Spine
  per-mutation flag can now be set from the authoring decorators —
  `@fraiseql.mutation(changelog=False)` in the Python SDK and
  `@Mutation({ changelog: false })` (or the typed `MutationConfig.changelog`) in the
  TypeScript SDK — instead of hand-editing the compiled schema. Both decorators validate
  the value is a boolean and fail fast at authoring time on anything else, and emit the
  `changelog` key only when it is set, so a schema authored without it keeps logging (the
  compiler serde-defaults `MutationDefinition.changelog` to `true`).

- **Change Spine: the change-log poller surfaces the envelope/perf columns on the observer
  event path.** `fraiseql_observers`'s `ChangeLogListener` now projects three more contract
  columns top-level — `tenant_id` (the public-facing UUID partition stamp), `duration_ms`, and
  `seq` (the monotonic Change-Spine sequence) — onto `ChangeLogEntry`, and carries
  `duration_ms` / `seq` through to the `EntityEvent` it emits. NATS subscribers, the deduped
  executor's `TenantScope`, and the search / Arrow sinks now see the perf and ordering metadata,
  not just the GraphQL `data` JSONB. (The `core.v_entity_change_log` read view already exposed
  these for the #149 GraphQL / #392 perf path; this closes the gap on the Rust event path.) All
  three are contract-nullable and decode as `None` for cooperative external producers that do not
  stamp them.
- **`fraiseql perf` — change-log performance observability (#392).** The first Change-Spine
  consumer. A new CLI command group reads the framework-owned change-log
  (`core.v_entity_change_log`) and turns it into operator forensics. `perf regression-scan`
  flags mutations whose p50 latency regressed between a baseline and a recent window, per
  `(object_type, modification_type)` — never aggregating across modification types (a shift in
  the operation mix can otherwise mask a regression as a false improvement) and comparing only
  rows carrying the current `duration_calc_version` (pre-fix `EXTRACT(MILLISECONDS)` rows are
  excluded, not mixed). `perf explore slowest | null-rate | summary` are ad-hoc reads of the
  slowest mutations, `duration_ms` completeness, and per-operation percentiles. The scan exits 0
  even when it finds regressions (a report, not a gate; `--fail-on-regression` opts into exit 1);
  `--json` emits a stable `findings`/`skipped`/`summary` shape and the human report prints
  greppable `WARN` / `SKIP` lines — the seam the `fraisier` orchestrator schedules against.
  PostgreSQL-only.
- **Change Spine: the change-log `trace_id` is now populated from the request trace (#375).**
  The mutation executor stamps the originating request's W3C trace id — parsed from the inbound
  `traceparent` header onto the `SecurityContext` — into the change-log `trace_id` column, on every
  dialect (it is a plain text column, unlike the PostgreSQL-only `duration_ms`). A change-log row now
  links back to its distributed trace, and the #392 `perf explore slowest` / regression findings
  surface it as the investigation handle. `trace_id` is `NULL` for a request with no trace context
  (e.g. an anonymous mutation, which carries no `SecurityContext`) — a best-effort stamp that never
  aborts the mutation, consistent with `tenant_id`. The full W3C `trace_context` JSONB is also now
  populated — see the dedicated entry below; #375 is fully landed.

- **Change Spine: the change-log `schema_version` is now populated from the compiled schema (#377).**
  The mutation executor stamps the compiled schema's content hash
  (`CompiledSchema::content_hash()`) into the change-log `schema_version` column, on every dialect
  (a plain text column, like `trace_id`). Unlike `trace_id` / `tenant_id`, this is **not** a request
  value but a per-deployment constant — the same hash on every row a given deployment writes — so it
  is computed **once** at executor construction and cached on the `ExecutorContext` rather than
  recomputed per mutation. It is the same content hash that already keys the query cache, the
  `/health` schema digest, and hot-reload diffing, so it changes on any schema change. A change-log
  row now records which deployment produced it, the correctness handle that unblocks #378
  (zero-downtime deploys / DLQ replay: reject a row replayed under a different schema rather than
  corrupt data). `schema_version` is `NULL` only for producers with no compiled schema in scope —
  cooperative external producers (ETL) and the non-PostgreSQL no-op path.

- **Change Spine: the change-log `trace_context` JSONB is now populated — #375 fully closed.**
  Beyond the scalar `trace_id`, the mutation executor now stamps the **full W3C trace context** into
  the `trace_context` JSONB column: the parsed `traceparent`
  (`{version, trace_id, parent_id, trace_flags}`, hex lower-cased) plus the `tracestate` header when
  present. A change-log row therefore carries enough to **re-propagate / reconstruct** the
  distributed trace, not merely link to it. The context is parsed feature-independently from the
  request headers onto the `SecurityContext` (alongside `trace_id`) and written on every dialect —
  JSONB on PostgreSQL, JSON on MySQL, `NVARCHAR(MAX)` on SQL Server. It is `NULL` for a request with
  no well-formed `traceparent` (same gate as `trace_id`), never aborting the mutation. With this, the
  only envelope columns still NULL-by-design are `actor_type` / `acting_for` (#390).

### Breaking

- **The observer admin API and design-audit API now require the `fraiseql:admin`
  scope; introspection / schema-export / schema-metadata now require a valid token
  whenever their `*_require_auth` flag is set.** Previously these admin-plane routes
  were authenticated only by the global OIDC middleware, which let anonymous callers
  through whenever the data plane allowed anonymous queries, and the observer API
  performed no scope check at all. Callers of the observer admin API
  (`/api/observers/*`) and design-audit API (`/api/v1/design/*`) must now present a
  JWT carrying the `fraiseql:admin` scope; tokens without it receive `403`. Tooling
  that reads introspection / schema export / metadata must present a valid token (any
  scope) when those endpoints are configured to require auth. Routes left at
  `*_require_auth = false` are unchanged.

- **Broadcast and storage subsystems now refuse to run unauthenticated (Phase 03 C6).**
  Three privileged surfaces that previously mounted (or admitted callers) without
  authentication now fail closed:
  - `POST /realtime/v1/broadcast` requires a `fraiseql:admin`-scoped token, and is not
    mounted at all unless an OIDC validator is configured (M-broadcast).
  - The legacy storage backend (no RLS) is not mounted unless `storage_token` is set, and
    the hardened storage API is not mounted unless `storage_token` or an OIDC validator is
    configured (M-storage-legacy).
  - The storage admin role is now `fraiseql:storage:admin`, not the generic `"admin"`; OIDC
    callers needing storage-admin must carry the explicit scope (M-storage-scope).
  Deployments relying on anonymous broadcast or anonymous/`admin`-scoped storage must add the
  appropriate auth configuration.

- **Multi-tenant subscription delivery and per-tenant concurrency are now strict (Phase 03 C7).**
  In `security.multi_tenant = true` deployments the subscription tenant gate fails closed: a
  subscription that resolves no tenant id receives no events, and untagged events are not
  delivered to tenant-scoped subscribers (M-tenant-ws-failopen). Suspended tenants can no longer
  open subscriptions or receive further events (M-tenant-ws-suspended). A configured
  `TenantQuota.max_concurrent` is now actually enforced on the GraphQL path and returns 429 when
  exceeded (M-quotas) — previously it was ignored. Single-tenant deployments and tenants without a
  concurrency quota are unaffected.

- **The framework now owns the `core.tb_entity_change_log` write — remove app-side
  hand-rolled inserts.** Before, FraiseQL apps populated the change log themselves, typically
  with a per-mutation-function `INSERT INTO core.tb_entity_change_log …`. The mutation
  executor now writes that row itself, in-transaction, for every successful state-changing
  mutation (see Added, above). **On upgrade, delete the hand-rolled inserts from your mutation
  functions** — otherwise each mutation logs the row twice (one app row + one framework row).
  There is no opt-out flag and no `ON CONFLICT` cutover guard: owning the write *is* the
  feature, and the duplicate-write window closes as soon as the app-side insert is removed.
  External *cooperative* producers (ETL / jobs / sister services writing
  contract-conforming rows directly into the table) remain first-class and are unaffected —
  that is a distinct, supported pattern, not the app double-writing its own mutation output.

- **The observer `EntityEvent.tenant_id` is now the UUID `tenant_id`, not `fk_customer_org`;
  `EntityEvent` also gains `duration_ms` / `seq` (wire-format change).** The change-log poller
  previously copied the internal `fk_customer_org` BIGINT (as a decimal string) into
  `EntityEvent.tenant_id`, collapsing the Trinity pair — so tenant isolation that keys off it
  (the NATS subscription tenant filter, the deduped executor's `TenantScope`) matched on an
  integer that never equals the JWT/RLS tenant. The poller now surfaces the contract's
  public-facing `tenant_id` UUID instead, and `None` when it is NULL (no more `fk_customer_org`
  fallback). **If you filter observer events by tenant, switch your configured tenant
  identifiers from the `fk_customer_org` integer to the UUID `tenant_id`.** Separately,
  `EntityEvent` now serializes two new fields — `duration_ms` and `seq` — with no serde
  default, so a consumer deserializing an `EntityEvent` produced by an older build (e.g. a
  message already resident in a durable NATS stream across a rolling upgrade) must be upgraded
  in lockstep; the change-log table is the source of truth and events are re-derivable, so
  drain the stream or accept the brief gap rather than mixing versions.

### Fixed

- **`fraiseql-server` now compiles with `--features rest,arrow` (unbreaks the
  `server-full` image).** The `#[cfg(feature = "arrow")]` server path builds a
  `Server<PostgresAdapter>` (the Arrow Flight constructor keeps the raw adapter), but the
  multi-tenant runtime wiring (#330) built the per-tenant executor factory only for the
  *cached* adapter type, so `with_tenant_executor_factory` failed to type-check (`E0308`)
  on the arrow path. The factory is now built per build with the adapter type that matches
  the server it is installed on — `PostgresAdapter` for the arrow path,
  `CachedDatabaseAdapter<PostgresAdapter>` otherwise. This was the one feature combination
  no CI leg compiled (preflight runs `--all-features`, which enables `wire-backend` and
  takes a different `cfg` branch), so it had been broken since #330 landed and left the
  `fraiseql-server-full` Docker image — the sole artifact that builds `rest + arrow` —
  stale at `2.4.0`; it ships again from the next release. A `server-rest-arrow`
  feature-matrix combo now guards the build, and the pre-existing arrow-path lint/doctest
  debt the combo surfaced has been cleared.

## [2.5.0] - 2026-06-08

### Security

- **Operation-level authorization — pluggable `Authorizer` (#422).** v2 had only a
  *static* per-operation gate (`requires_role`, an enumeration-hiding role compare) and no
  general, pluggable hook to authorize a whole operation against the principal and its
  input. A new decision-returning `Authorizer` trait (the operation-level counterpart of
  the field-level `FieldAuthorizer`, mirroring the `RLSPolicy` plugin) closes that gap:
  the engine *enforces* but delegates the *decision* to an app-supplied trait object
  (in-process rules, a DB query, or an external service). Register one on `RuntimeConfig`
  via `with_authorizer(…)`; it receives `AuthzRequest { principal, operation, name, input }`
  and returns `Allow` / `Deny { reason }`. Semantics: **fail-closed** — any policy error or
  a `Deny` returns HTTP 403 `FORBIDDEN` and the operation never executes (the underlying
  policy error is not surfaced); the decision **AND-composes** with `requires_role` (both
  must allow, and `requires_role` keeps its enumeration-hiding "not found in schema"
  response — it is *not* routed through the authorizer); and the **anonymous** entry path
  is consulted with `principal: None` rather than blanket-denied, so public operations
  remain expressible. **Path coverage (the security-critical part):** every operation entry
  path is gated — authenticated and anonymous GraphQL (incl. multi-root, where a deny on
  any root fails the whole request before dispatch), MCP, **all REST reads** (GET, count,
  streaming, embedding, bulk-by-filter) at the shared read runner, **all mutations** at the
  universal mutation chokepoint (`execute_mutation_impl`, which also covers the
  anonymous-REST write path that bypasses the GraphQL chokepoints), introspection,
  federation `_entities`, and **subscriptions** at subscribe-time (a deny rejects with a
  `FORBIDDEN` GraphQL-WS error). Because the gate runs *before* the response cache, a warm
  cache never replays an allow past a later deny (no cache bypass needed, unlike the
  per-row field authorizer). **API note:** `AuthzRequest.principal` is
  `Option<&SecurityContext>` (a deliberate divergence from the field authorizer's
  non-optional principal) so the anonymous path is a first-class, explicit case. No
  compiled-schema change. Per-event subscription re-evaluation, federation per-entity-type
  granularity, an `RLSPolicy` argument widening, and a declarative/SDK authoring surface are
  tracked follow-ups. See `docs/guides/operation-authorization.md`.

- **Dynamic field-level authorization — pluggable `FieldAuthorizer` (#423).** v2 had
  only *static* field gating (`field(requires_scope=…)`): it can answer "does this
  principal hold scope X?" but not relational/contextual rules that depend on the
  **row** being resolved, the **principal**, and the **field arguments** (e.g. "show
  `User.email` only to the row's owner or an admin"). A new pluggable, decision-returning
  `FieldAuthorizer` trait (the field analogue of an operation-level authorizer, mirroring
  the `RLSPolicy` plugin) closes that gap. Register one on `RuntimeConfig` via
  `with_field_authorizer(…)`; mark a field policy-gated with `authorize: true` in the
  compiled schema (authored as `field(authorize=True)` → `IntermediateField.authorize`).
  For each selected gated field the engine consults the authorizer per row, passing the
  principal, the **full** row (`parent`), and the field arguments. Semantics:
  **fail-closed** — any policy error or a `Deny { on_deny: Reject }` returns HTTP 403
  `FORBIDDEN` and the value is never served; `Deny { on_deny: Mask }` nulls just that
  field on just that row; and the decision **AND-composes** with the static
  `requires_scope` gate (a field is visible only if both allow). Enforced on the
  authenticated query and mutation paths; **every other projection path
  (unauthenticated query, REST direct, Relay list/`node`, federation `_entities`) fails
  closed** when a policy-gated field could be projected — a missed path cannot leak a
  gated field. Per-row enforcement on Relay/federation, an SDK `@authorize_field`
  authoring surface, and nested-field enforcement are tracked follow-ups (top-level
  fields are enforced today; nested gated fields fail closed). **Compiled-schema format
  note:** `FieldDefinition.authorize` / `IntermediateField.authorize` are new fields;
  unlike the project's usual "plain required field, recompile to migrate" stance for
  compiled-schema additions, this one keeps `#[serde(default, skip_serializing_if = …)]`
  (a deliberate divergence) so `authorize: false` is never serialized — existing golden
  fixtures and the fuzz corpus stay byte-stable and no recompile is forced.

- **Outbound observer webhooks can now be HMAC-signed (#345).** Webhook payloads
  were sent unsigned, so receivers had no way to authenticate them — the
  documented receiver-side verification pattern was not implementable
  end-to-end. Setting `signing_secret_env` on a webhook action (the env var
  *name* holding the secret) now signs the payload with HMAC-SHA256 and attaches
  `X-FraiseQL-Signature-256: t=<unix_ts>,v1=<hex>`, byte-compatible with
  `fraiseql-webhooks`'s `StripeVerifier` (the signature is computed over the
  exact bytes transmitted on the wire, not a re-serialization). If
  `signing_secret_env` is set but the env var is absent or empty, dispatch fails
  loud rather than silently sending an unsigned payload. Settable on
  DB-defined observers and via the `/api/observers` admin API; unset leaves
  delivery unsigned (back-compat).

- **PostgreSQL token-revocation backend implemented (#357).** `[security.token_revocation]
  backend = "postgres"` previously fell back to an in-memory store after a single warning —
  revocations were lost on restart and not shared across replicas, silently breaking the
  cross-replica revocation contract operators expected. The binary now provisions a real
  PostgreSQL-backed store (table `fraiseql_revoked_tokens`, idempotent migration) on the
  PostgreSQL runtime path, so revoked `jti`s persist and are shared across replicas. An
  unrecognised `backend` value is now a hard startup error instead of a silent in-memory
  fallback, and a non-PostgreSQL deployment that requests `backend = "postgres"` warns at
  startup that the backend is unavailable.

- **Failed-login lockout config is no longer silently ignored (#356).** The server
  previously dropped `[security.rate_limiting] failed_login_max_attempts` /
  `failed_login_lockout_secs` on deserialization. The off-the-shelf binary performs no
  first-factor login of its own (OIDC/JWT is validated cryptographically and delegated
  to the identity provider; TOTP MFA is a library-only feature the binary does not
  mount), so it cannot enforce a failed-login lockout. The fields are now captured, and
  tuning them away from the defaults refuses startup in production with an actionable
  message (enforce brute-force protection at the identity provider or edge proxy),
  downgraded to a warning under `FRAISEQL_ENV=development`. Untouched default values
  still boot silently. **Breaking:** a production config that set non-default
  `failed_login_*` values now fails to start until they are removed.

- **PKCE refuses to boot without state encryption in production (#360).** When
  `[security.pkce] enabled = true` but `[security.state_encryption]` is missing or
  disabled, the server now refuses to start in production instead of serving
  `/auth/start` while emitting only a warning — the outbound state token would
  otherwise be the raw, unencrypted lookup key, contradicting the documented "state
  encryption is enforced" posture. Set `FRAISEQL_ENV=development` to downgrade the
  refusal to a warning for local development.

- **JWKS rotation no longer leaves revoked keys cached (#361).** When the OIDC
  provider rotates signing keys, FraiseQL now replaces its JWKS cache with the
  provider's current key set on the next refetch — even when the looked-up `kid` is
  absent — so a token signed by a rotated-out key stops validating once the cache
  refreshes, instead of being trusted until the cache TTL expires. `fraiseql-core`
  embedders can close the window immediately on a known key compromise with the new
  `OidcValidator::invalidate_jwks_cache` (flush) and `refresh_jwks` (eager refetch)
  methods; operators of the off-the-shelf binary can trigger the same via the new
  admin-token-gated `POST /admin/v1/auth/refresh-jwks` endpoint (fail-closed: if the
  provider is unreachable the cache is invalidated anyway). The `jwks_cache_ttl_secs`
  documentation now describes it as the maximum stolen-key replay window once a
  rotation has propagated.

- **Top-level page-size ceiling (#421).** A root query's `first`/`last`/`limit`
  argument is now capped at a configurable maximum (default **1000**) before it
  reaches SQL, closing an unbounded-pagination denial-of-service vector — a single
  query could previously request millions of rows, sizing the database scan, the
  materialized JSONB, and the response buffer with no server-side limit. A request
  exceeding the ceiling is rejected with a validation error. Configure it via
  `[validation] max_page_size` in `fraiseql.toml`, the `FRAISEQL_MAX_PAGE_SIZE`
  environment variable (a number, or `0`/`none` to disable), or
  `RuntimeConfig::max_page_size` for direct `fraiseql-core` embedders. Also fixed
  an integer overflow in the relay `page_size + 1` fetch when pagination is
  unbounded.

- **WebSocket subscriptions now enforce tenant dispatch (#331).** The subscription
  upgrade previously resolved the tenant key with `security_context = None`,
  `domain_registry = None`, and `strict = false` hard-coded — silently dropping JWT
  `tenant_id` precedence, ignoring an installed domain registry, and disabling the
  strict cross-source validation the GraphQL handler applies when RLS is configured.
  A client could carry a JWT for tenant `bar` and still tag its subscription as
  tenant `foo` via an `X-Tenant-ID` header. The handler now extracts the
  authenticated `SecurityContext`, propagates the domain registry, and drives strict
  mode from `schema.has_rls_configured()`, rejecting the upgrade (HTTP 400) on a
  conflicting or invalid tenant key — mirroring the GraphQL handler exactly.

- **Storage list-prefix LIKE-injection (#339).** The `prefix` filter on
  `GET /storage/v1/list/{bucket}` is now matched as a literal string. A client-supplied
  `%` or `_` was previously interpolated into the metadata `LIKE` pattern unescaped,
  letting a caller widen the match and enumerate a bucket's keys (e.g. `prefix=%`
  matched every object). The prefix is now escaped and bound with an explicit `ESCAPE`
  clause.

- **Storage stored-XSS hardening (#337).** Object downloads now always carry
  `X-Content-Type-Options: nosniff` and default to `Content-Disposition: attachment`,
  so an uploaded payload with a client-chosen `Content-Type` (e.g. HTML or SVG) can no
  longer be rendered as active content in the storage origin. A bucket may opt into
  in-browser rendering with the new `BucketConfig::serve_inline` flag, but content
  types browsers execute as active content (`text/html`, `image/svg+xml`, …) stay
  attachments even then.

### Added

- **`fraiseql-cli validate --against-db` — static server↔database mutation-contract
  check (#397).** The server invokes each mutation as `SELECT * FROM <sql_source>(…)` and
  decodes the returned row into `MutationResponse`; both halves of that contract — the
  *call binding* and the *response shape* — were only mirrored by hand between the compiled
  schema and the SQL functions, so every drift surfaced as an opaque runtime 500 (the root
  of the #413/#414 family). `validate --against-db <DATABASE_URL> schema.compiled.json` now
  verifies the contract against a live PostgreSQL **without booting a server or invoking any
  mutation**: for each DB-backed mutation it checks that `sql_source` resolves to exactly one
  function (catching *does not exist* and *is not unique*) whose input arity matches what the
  runtime sends (the positional args — flat, flattened input-object fields, or the
  update-path jsonb payload — plus the trailing injected params), that the update payload
  parameter is `jsonb`, that the trailing parameter names match the inject keys, and that the
  function's result row carries `succeeded` + `state_changed` (both `boolean`, required by
  the decoder) with compatible types for the optional `MutationResponse` columns (`error_class`
  accepts `text` or a project enum). Error-severity findings fail the command (exit 1) for CI
  gating; `--json` emits a machine-readable report. The *behavioural* response invariants
  (`succeeded ⇒ error_class IS NULL`, `http_status ∈ 100..=599`, …) are out of scope — they
  are only observable by invoking the mutation, which would have database side effects.

- **`fraiseql-cli doctor --against-db` — PL/pgSQL body-resolution pass (#409).** PostgreSQL
  defers PL/pgSQL body analysis to runtime, so a migration that changes a function's
  signature silently breaks every *internal* caller until that branch executes — invisible to
  `compile` and to the server-facing check in #397. `doctor --against-db <DATABASE_URL>
  --schemas a,b` resolves every call inside each managed function's body against the live
  catalog (via the [`plpgsql_check`](https://github.com/okbob/plpgsql_check) extension) and
  reports unresolved internal calls as failed doctor checks. It degrades gracefully: when
  `plpgsql_check` is not installed (the common case on managed Postgres), the pass is skipped
  with a `Warn` and an install hint rather than failing.

### Breaking

- **Compiled-schema format: input-object fields now carry `nullable` (#414).** Each
  `InputFieldDefinition` in `schema.compiled.json` gains a `nullable` boolean (mirroring the
  output `FieldDefinition.nullable`), so the runtime can distinguish a required (non-null)
  input field from an optional one — previously a compiled input field carried only `name` +
  `field_type` and requiredness was lost. **`fraiseql-cli compile` emits the new field;
  recompile your schema** to pick up required-input-field enforcement (see Fixed, below). The
  field is serde-defaulted to `true` (nullable) on load, so an older compiled artifact still
  deserialises — it simply enforces nothing until recompiled. Nullability is driven by the
  `nullable` flag the SDK emits, **not** by a `!` suffix in the type string: a hand-written
  compiled schema encoding a required field only as `"field_type": "ID!"` (without
  `"nullable": false`) is treated as optional until recompiled via the SDK.

### Fixed

- **Required input fields are now enforced before the database call (#414).** `fraiseql-cli
  compile` dropped per-field nullability for input-object types, so the runtime could not
  tell a required input field from an optional one: a create mutation that **omitted** a
  non-null input field (or passed explicit `null`) flattened a SQL `NULL` straight into the
  function instead of being rejected. The compiler now carries input-field nullability into
  the compiled schema (see Breaking, above), and the mutation executor rejects an
  omitted-or-explicit-null required (non-null, no-default) input field with a GraphQL
  **validation error** (HTTP 200 + `errors[]`) before any DB round-trip — a clear, actionable
  message in place of relying on a downstream constraint failure (post-#413 those surface as
  HTTP 400, but only after the function runs). Enforcement covers the insert/delete/custom
  **flatten** path at the universal mutation chokepoint. As part of the same lookup fix, a
  **latent camelCase Insert bug** is closed: under `NamingConvention::CamelCase` the flatten
  path looked up input values by the canonical (snake_case) name while clients send camelCase
  keys, so values silently became `NULL`; fields are now matched by their GraphQL surface
  name. GraphQL introspection now reports a required input field as `NON_NULL`. **Not**
  covered (tracked follow-ups): update-path three-state inputs (an omitted field still means
  "leave unchanged"), the gRPC mutation path (binds proto fields directly, bypassing the
  chokepoint), query/filter inputs (optional by design), input-object-field **kind** +
  list-element nullability in introspection, and applying an input field's default for an
  absent value.

- **Client-input DB errors now return HTTP 400, not 500 (#413).** When a PL/pgSQL
  mutation raised on **client input** — a malformed value that fails a cast (e.g.
  `"not-a-uuid"` → `uuid`, SQLSTATE `22P02`) or an integrity-constraint violation
  (not-null / unique / foreign-key / check, class `23xxx`) — the server returned
  **HTTP 500 / `DATABASE_ERROR`**, because every `FraiseQLError::Database` was mapped
  to `INTERNAL_SERVER_ERROR` regardless of SQLSTATE. HTTP-aware clients and test
  harnesses treat 5xx as a server fault to retry/alert on, not a 4xx to surface to the
  user. The server now classifies a `Database` error by its SQLSTATE: class **`22`**
  (data exception) → **HTTP 400 / `BAD_USER_INPUT`**, class **`23`** (integrity
  constraint) → **HTTP 400 / `CONSTRAINT_VIOLATION`**; every other class, an absent
  SQLSTATE, and connection-pool errors stay **HTTP 500 / `DATABASE_ERROR`**. The PG
  message is preserved in the structured error. Applied to **both** transports — the
  GraphQL mapper (`from_fraiseql_error`) and the REST/bulk mapper (`RestError::from`),
  which classify via one shared predicate so they cannot drift. **Client-visible
  behaviour change:** these specific cases move from 500 to 400. (Per-subclass
  `23505 unique_violation → 409 Conflict`, surfacing the SQLSTATE in the error
  extensions, and the gRPC `Code::Internal` path are tracked follow-ups.)

- **Observer DLQ CLI fabricated data; now talks to the real server API (#341).** The
  `fraiseql-observers dlq` subcommands (list/show/retry/retry-all/remove/stats)
  returned hard-coded JSON fixtures — synthetic items, invented retry counts and
  stats — so the CLI confidently reported state that did not exist. They now call
  the server's observer admin API over HTTP and render the real response, or fail
  loud: a non-2xx status (e.g. a 404 from `remove` on a missing item) or an
  unreachable server surfaces as an error with a non-zero exit, never a synthetic
  success. New global args `--base-url` (default `http://localhost:8000`) and
  `--admin-token` (sent as `Authorization: Bearer`) target the server. Two new
  server endpoints back the CLI: `DELETE /api/observers/dlq/{id}` (remove) and
  `GET /api/observers/dlq/stats` (aggregate stats). Mock-era filters the server API
  does not support (`--observer`/`--after`/`--by-observer`/`--by-error`/`--dry-run`)
  now emit a warning rather than being silently honored.

- **Observer email action reported success without sending (#349).** `EmailAction`
  was a stub that always returned success, so a dead email integration showed green
  metrics while silently dropping every message. It now sends real email over SMTP
  via `lettre` (rustls, no OpenSSL): configure `[observers.runtime.email]`
  (`host`/`port`/`from`/`tls` = `start_tls`|`tls`|`none`, with credentials supplied
  via the `username_env`/`password_env` environment-variable *names*). SMTP failures
  are classified — permanent (5xx, bad recipient, auth rejected) go straight to the
  DLQ, transient (connection refused, timeout, 4xx greylisting) are retried per
  policy. When SMTP is **not** configured the action fails loud (permanent) instead
  of faking success, so a misconfigured email integration is always surfaced. The
  `[observers.runtime.email]` block is strict (`deny_unknown_fields`): a typo or a
  literal-credential key fails the parse. The failure path (a refused send is a
  loud, classified error) is covered without infra; the happy path is covered
  end-to-end by a MailHog SMTP sink bound into the `integration(observers)` CI
  leg — a test sends through `lettre` and asserts the message arrives.

- **Observer transport selection was silently ignored; NATS ran on PostgreSQL (#350).**
  The off-the-shelf binary never read `[observers.runtime.transport]` /
  `FRAISEQL_OBSERVER_TRANSPORT`, so selecting `transport = "nats"` quietly ran on
  PostgreSQL LISTEN/NOTIFY with a false "running on NATS" posture. The runtime now
  honors the selection: PostgreSQL drives the existing change-log listener, while
  NATS `JetStream` and the in-memory transport run through the library's
  `EventTransport` stream — a non-Postgres selection can never fall through to the
  PG listener. A selection this binary cannot run (NATS without the `observers-nats`
  feature, or no broker URL) refuses to boot in production (downgraded to a warning
  under `FRAISEQL_ENV=development`, which runs on PostgreSQL), and a configured NATS
  transport whose broker is unreachable fails startup rather than silently coming up
  without it. Configure via `[observers.runtime.transport]` (`transport = "postgres"
  | "nats" | "in_memory"`) with `[observers.runtime.transport.nats]` for the broker
  URL and JetStream settings; NATS requires a binary built with `--features
  observers-nats`.

- **DLQ retry could double-fire the action under concurrent requests (#344).**
  `POST /api/observers/dlq/{id}/retry` read the item, released the lock, then
  re-dispatched and removed it — so two concurrent retries (or a per-item retry
  racing `retry-all`) both dispatched the action, turning at-least-once delivery
  into at-least-twice. Retries now go through an atomic claim (single-lock
  remove-and-return): exactly one caller dispatches per claim, the loser gets
  404; `retry-all` drains via the same claim. A failed redispatch re-inserts the
  item (cap-bypassing, so a DLQ that refilled to capacity during the claim
  cannot silently drop the just-failed item) with its `attempts` incremented.

- **Observer DLQ ignored `max_dlq_size`; failed retries silently destroyed (#343).**
  The `fraiseql-server` binary's in-memory dead letter queue grew without bound
  — `max_dlq_size` was a documented setting the binary never honored, a memory
  DoS amplifier under sustained action failures. It now enforces the cap with
  the same policy as the `fraiseql-observers` library (drop-newest + a `warn!`
  with matching fields + an overflow counter), enforced atomically under the
  items mutex. The overflow counter is surfaced as `dlq_dropped` on
  `GET /api/observers/delivery/health`. Configure via
  `[observers.runtime] max_dlq_size` (default `None` = unbounded, for
  back-compat). Separately, `mark_retry_failed` previously deleted the failed
  item outright, destroying the audit trail; it now keeps the item, increments
  its `attempts`, and records the latest error — items leave the DLQ only on
  success or an explicit operator delete.

- **Observer runtime routes mounted at the wrong prefix (#340).** The observer
  runtime-health and reload endpoints were `merge`d at the router root, so
  `/api/observers/runtime/health` and `/api/observers/runtime/reload` returned
  **404** while the handlers were instead reachable at `/runtime/health` /
  `/runtime/reload`, shadowing any user routes there. Both are now `nest`ed under
  `/api/observers` like the other observer routers. **Breaking (path move):**
  clients calling the root `/runtime/*` paths must switch to
  `/api/observers/runtime/*`.

- **Cross-bucket object collisions (#336).** Storage backend operations
  (upload / download / delete / presign) now scope the object key by bucket
  (`{bucket}/{key}`). Two objects with the same key in different buckets previously
  mapped to the same backend object, so one upload could overwrite or shadow another
  and a delete in one bucket could remove a different bucket's bytes. Object metadata
  already keyed on `(bucket, key)`; the backend store now matches.

- **Storage uploads capped below the per-bucket limit (#338).** The storage router now
  applies its own request-body limit, sized to the largest configured `max_object_bytes`
  (or 100 MiB when a bucket is unlimited), overriding the server-wide
  `max_request_body_bytes` (default 1 MiB) and axum's 2 MiB extractor default for storage
  routes only. Previously a bucket's `max_object_bytes` was unreachable and larger uploads
  failed with a generic 413. Very large objects should still use presigned
  direct-to-backend uploads.

- **Storage routes unreachable from the `fraiseql-server` binary (#334).** The
  off-the-shelf binary now wires a `[storage.<name>]` TOML section into a mounted
  `/storage/v1/*` route group (object upload / download / delete, list, presign) at
  startup. Previously `ServerConfig` had no `storage` field, so serde silently dropped the
  section and every storage path returned **404** even though the library API existed. The
  section name is the logical bucket; optional `access` (`"private"` default /
  `"public_read"`), `max_object_bytes`, `allowed_mime_types`, and `serve_inline` set the
  bucket policy. Authentication uses the configured OIDC validator (per-user RLS) and/or a
  `storage_token` bearer treated as a full-access admin; with neither set, only
  `public_read` buckets are reachable (read-only). Object storage via the binary is
  **PostgreSQL-only** (the object-metadata repository requires PostgreSQL), and **v1
  supports a single backend** — configuring more than one `[storage.<name>]` is a startup
  error. `[files.<name>]` sections are parsed but not yet wired (a startup warning is
  logged).

- **Suspended tenant now returns HTTP 503 + `Retry-After` (#332).** The GraphQL
  handler mapped every error from per-tenant executor dispatch to HTTP 403,
  collapsing a suspended tenant (`ServiceUnavailable { retry_after }`) and an
  unknown tenant key (`Authorization`) onto the same status and dropping the
  retry hint. Dispatch errors are now mapped by variant: an unknown key stays
  403 Forbidden, while a suspended tenant returns 503 with a `Retry-After`
  header carrying the registry's retry value (60s), matching the documented
  suspend/resume contract.

- **Multi-tenant runtime now wired into the `fraiseql-server` binary (#330).** The
  per-tenant executor runtime (registry, `X-Tenant-ID` / JWT `tenant_id` / Host
  dispatch, the `/api/v1/admin/tenants/*` lifecycle API, suspend/resume, and the
  explicit-deny 403 for an unregistered tenant key) was implemented only as a
  library API; the off-the-shelf binary never installed it, so the admin tenant
  endpoints returned `404 multi-tenant mode not enabled` and an explicit
  `X-Tenant-ID` was silently served by the default executor. Enable it with
  `[tenancy.runtime] enabled = true` in `fraiseql.toml`: the binary installs the
  registry (seeded with the default executor), an in-memory tenant audit log, the
  domain registry, and — on PostgreSQL — the executor factory so
  `PUT /api/v1/admin/tenants/{key}` provisions a tenant with its own connection
  (and schema, in `tenancy.mode = "schema"`). `PostgresAdapter` now implements
  `FromPoolConfig`. Runtime provisioning is PostgreSQL-only; dispatch to
  pre-registered tenants works on any adapter.

### Changed

- **Breaking (observer config layout, #342):** the server's observer **runtime**
  tuning moved from the flat `[observers]` table to a dedicated
  `[observers.runtime]` sub-table: `poll_interval_ms`, `batch_size`,
  `channel_capacity`, `auto_reload`, `reload_interval_secs`, and the
  `[observers.pool]` table (now `[observers.runtime.pool]`). The same
  `fraiseql.toml` is consumed by both `fraiseql compile` and `fraiseql-server`;
  the compiler owns the `[observers]` top-level keys (`backend`/`handlers`/…) and
  rejected server-tuning keys placed there, so a shared file could never carry
  both. With the relocation, `fraiseql compile` tolerates `[observers.runtime]`
  and the server reads it. Two fail-loud guards replace the previous silent
  swallow: a server-tuning key left at the flat `[observers]` level now fails
  startup with a migration message naming the key and its new home, and an
  unrecognised key under `[observers.runtime]` (e.g. a typo) fails to parse
  instead of being ignored. Move any server-tuning keys under
  `[observers.runtime]` to upgrade.

- **Breaking (runtime behavior, #421):** clients requesting more than 1000 rows in
  a single page now receive a validation error by default. Raise
  `[validation] max_page_size`, set `FRAISEQL_MAX_PAGE_SIZE`, or set it to `0`/`none`
  to restore the previous unbounded behavior.

- **Breaking (storage backend layout, #336):** objects are now stored under
  bucket-prefixed backend keys (`{bucket}/{key}`). Deployments that wrote objects via
  the `fraiseql-storage` library routes before this release must relocate existing backend
  objects under the new prefix. Earlier releases' off-the-shelf `fraiseql-server` binary
  did not mount these routes (#334 wires them in this release), so only deployments that
  used storage through the library API before upgrading are affected.

- **Storage downloads default to `Content-Disposition: attachment` (#337).** Buckets
  that need in-browser rendering must opt in with `BucketConfig::serve_inline = true`.

- **Breaking (tenant-key alphabet, #333):** the `X-Tenant-ID` header validator is
  tightened to `[a-zA-Z0-9_]` with a 56-character cap (derived from PostgreSQL's
  63-character identifier limit minus the `tenant_` schema prefix), matching the
  schema-mode DDL helpers. Hyphenated keys (e.g. `acme-corp`) and keys of 57–128
  characters — previously accepted at dispatch but silently rejected at schema-mode
  provisioning — are now rejected uniformly, including at tenant registration
  (`PUT /api/v1/admin/tenants/{key}`). Deployments using hyphenated tenant keys in
  row-mode must migrate to underscores.

## [2.4.0] - 2026-06-04

### Added

- **Multi-database runtime support for `fraiseql-server` and `fraiseql run`
  (#327).** The server binary and the CLI's `run` command now dispatch on the
  `database_url` scheme at startup and construct the matching adapter:
  `postgresql://` (always available), `mysql://`, `sqlite://`, or
  `sqlserver://`. Non-PostgreSQL adapters are gated behind new Cargo features
  on `fraiseql-server` (`mysql`, `sqlite`, `sqlserver`) and `fraiseql-cli`
  (which cascade-enable them on `fraiseql-server` when `run-server` is also
  on). Build with e.g. `cargo install fraiseql-server --features mysql,sqlite`.
  Pointing the binary at a URL whose scheme matches an adapter that was not
  compiled in fails fast at startup with a clear `--features <name>` rebuild
  hint, instead of producing an opaque driver error from inside `tokio-postgres`.
  Two intentional constraints:
  1. **SQLite is read-only.** `SqliteAdapter` deliberately does not implement
     `SupportsMutations`. Starting the server against a `sqlite://` URL with a
     schema that declares any mutations fails at startup with a diagnostic
     naming the first three offending mutations.
  2. **Observers (LISTEN/NOTIFY) remain PostgreSQL-only.** Arrow Flight, the
     observer-pool initialisation, and relay-pagination auto-detection are
     skipped for the non-PostgreSQL adapter paths and are tracked as separate
     follow-ups. The `arrow` Cargo feature is silently no-op on non-PG paths.
  A new module `fraiseql_server::url_guard` exposes the `DatabaseScheme` enum,
  `parse_database_url`, and `guard_sqlite_mutations` for downstream tooling
  that needs to mirror the dispatch logic.
- **Entity change log over GraphQL — opt-in pull-based event consumption (#149).**
  Set `[changelog] expose = true` (requires `[observers]`) and the compiler injects
  read-only `EntityChangeLog` / `TransportCheckpoint` types, a cursor-paginated
  `entity_change_logs` query, a `transport_checkpoint` point lookup, and an
  idempotent `upsert_transport_checkpoint` mutation — all backed by views the new
  migration `07_create_changelog_views.sql` installs. Sidecar consumers (AI
  scoring, search-index sync, audit dashboards) can now poll the observer
  change-log over the same GraphQL endpoint as the rest of the API — same auth,
  audit logging, and rate limiting — instead of opening a side-channel PostgreSQL
  connection. Cursor pagination uses the standard generic filter machinery
  (`where: { pk_entity_change_log: { gt: $cursor } } orderBy limit`), numeric and
  gap-free. Access is gated by configurable `read_role` / `write_role`; denied
  callers receive `"not found in schema"` (enumeration-prevention). This also adds
  `MutationDefinition.requires_role` with runtime enforcement. See
  `docs/guides/changelog-graphql.md` and `examples/changelog-sidecar/`.
- **`fraiseql generate-client typescript` — typed TypeScript clients from a
  compiled schema (#291).** A new `fraiseql-codegen` crate turns a
  `schema.compiled.json` into a consumer-side client that *calls* a FraiseQL API:
  interfaces for every type, typed query/mutation functions, a relay
  `Connection<T>`, relationship metadata, and a tiny `fetch`-based runtime client
  with zero dependencies. This is distinct from `fraiseql generate <language>`,
  which emits server-side *authoring* code fed back into the compiler. Two
  deliberate, GraphQL-correct design choices set it apart from naive schema-to-TS
  tools: (1) result types are **selection-scoped** — each type contains exactly
  the leaf fields (scalars, enums, `__typename`) the generated default document
  fetches, so the type never claims relationship fields it did not retrieve; and
  (2) mutations are typed as **result unions discriminated by `__typename`** (with
  an `isErrorResult` type guard and a `status` field on `@fraiseql.error` types),
  matching the actual wire contract rather than a synthetic response wrapper.
  Every generated file carries a `schema-hash` header for CI staleness detection.
  The `fraiseql-codegen` crate also exposes the generator programmatically
  (`fraiseql_codegen::client::typescript::generate`) for IDE extensions,
  scaffolders, and build plugins. See `docs/guides/typed-clients.md` and
  `examples/typescript-client/`.

- **FreeBSD (`x86_64-unknown-freebsd`) is now a CI-enforced compile target (#148).**
  A new `freebsd-cross-check` job cross-compiles the workspace (default
  features) and the full `fraiseql-server` feature surface for FreeBSD on
  every PR, using a FreeBSD `base.txz` sysroot + `clang` on the existing Linux
  runners — no FreeBSD VM or extra infrastructure. A dependency audit confirmed
  no Linux-specific source assumptions (the one `/proc/self/limits` read is
  already `#[cfg(target_os = "linux")]`-gated; `notify` selects its kqueue
  backend on BSD). Two optional features are intentionally out of cross-check
  scope because they have no Linux→FreeBSD cross path and must be built natively
  on FreeBSD: the Deno edge-functions runtime (`fraiseql-functions/runtime-deno`
  → `v8`) and the SQL Server backend (`tiberius` → `openssl-sys`). Compile-time
  only — runtime testing on a real FreeBSD host remains deferred pending user
  signal. No engine changes.

### Fixed

- **Azure Blob (`azure-blob`) and Google Cloud Storage (`gcs`) backends now
  honour the configured `endpoint` URL (#326).** Previously the `endpoint`
  field on `StorageConfig` was silently ignored for these two backends, which
  hardcoded `*.blob.core.windows.net` / `storage.googleapis.com` into every
  request — so the Azurite and fake-gcs-server emulators could not be used for
  local development or CI. Both backends now route through the configured
  endpoint (matching the existing S3 behaviour), enabling emulator round-trips.
  Real-cloud Azure/GCS deployments are unaffected: the endpoint defaults to the
  production hostname when not specified. `AzureBackend` and `GcsBackend` gain
  additive `new_with_endpoint` constructors (and `AzureBackend` an additive
  `create_container_if_missing`); the existing `new` constructors are unchanged.

- **Session variables now reach mutation SQL functions and RLS policies (#329).**
  Before this release, `current_setting('app.x', true)` inside a mutation
  function, an RLS-protected view, a relay-paginated list, or an aggregate
  always returned NULL: `PostgresAdapter::set_session_variables` ran
  `SELECT set_config(..., true)` on a pooled connection in its own autocommit
  transaction — transaction-local *and* on a different connection than the
  subsequent operation. Session variables are now applied transaction-locally
  on the **same connection** as the operation. Applications that worked around
  this by passing tenant/user ids as mutation arguments via `inject_params` can
  continue to do so, or now rely on session variables.

- **Update mutations re-case the input payload to the schema's canonical field
  names (#400).** With `naming_convention = "camelCase"`, the Update path forwarded
  the GraphQL input object to the SQL function verbatim, so a `camelCase` surface
  delivered `camelCase` keys (`{ "fullName": ... }`) that a `snake_case` function
  reading `payload->>'full_name'` / `jsonb_populate_record` could not see — silently
  writing NULLs (or failing NOT NULL constraints). The payload is now re-cased to
  the canonical names before it reaches the function, recursing into nested input
  objects and arrays of input objects. The mapping is driven by the input type's
  per-field map (not a lossy regex), so acronyms and intentional names are preserved;
  `Preserve` schemas, unknown input types, and unmatched keys are untouched. The
  Insert/Delete paths were already correct (positional args).

- **Mutation success responses now project nested typed-object fields like queries
  do (#410).** The success arm projected the returned entity with a flat mapping
  keyed by the selection's `camelCase` names, so it could not read the `snake_case`
  entity JSONB and dropped (or failed to recurse into) nested typed-object fields —
  a mutation selecting `{ thing { id billingAddress { postalCode } } }` lost
  `billingAddress` entirely, while the same selection over a query returned it.
  Mutation success **and** error responses now flow through a single canonical
  entity projector that mirrors the query path exactly (`snake_case` source keys,
  `camelCase` surface output, depth-aware recursion into nested objects), so a
  mutation's payload and a query over the same entity return an identical shape.
  This also removes a latent acronym drift between the SQL and Rust projectors,
  which now share one `to_snake_case` definition. As part of the same unification,
  mutation result selections now resolve named fragment spreads and evaluate
  `@skip` / `@include` directives before projection, exactly as the query path
  does — so factoring mutation fields into a fragment (or guarding them with a
  directive) now behaves identically to a query.

- **Mutation error fallback now detects `__typename` inside inline fragments
  (#419).** When a mutation's error outcome has no matching error type declared
  in the return union, the response carries just `__typename` (plus the synthetic
  `status`), and only when the client selects `__typename`. That selection scan
  was top-level only, so a client that nested `__typename` inside an inline
  fragment — `... on SomeError { __typename }` — was silently denied it, even
  though #410 already resolves named fragment spreads and `@skip` / `@include`
  on this same path. The scan now recurses into inline fragments, reusing the
  `selections_contain_field` helper the query projector already uses.

- **Aliased query fields now read from their source JSONB column (#418).** The
  query SQL projector derived a field's JSONB key from its *response* key
  (`to_snake_case` of the alias), so an aliased field like `myName: fullName`
  generated `data->>'my_name'` and read the wrong (nonexistent) column —
  returning null where the un-aliased query worked. `ProjectionField` now
  carries a `source` (the GraphQL field name that drives the JSONB key) distinct
  from `name` (the output/response key): the column is read from `source` and the
  value emitted under `name`. The mutation projector was already correct after
  #410.

### Changed

- Upgraded the RustCrypto hashing stack jointly (#300): `sha1 0.10 → 0.11`,
  `sha2 0.10 → 0.11`, `hmac 0.12 → 0.13`, and `pbkdf2 0.12 → 0.13` (the latter
  forced by the wire SCRAM PRF). These all ride `digest 0.11` / `crypto-common
  0.2` and cannot be mixed with the `digest 0.10` generation, so they move in
  lockstep. Call sites were updated to the new API: `KeyInit` is now imported
  for `Hmac::new_from_slice`, and digest outputs are hex-encoded via
  `hex::encode` (the new `hybrid-array` `Output` no longer implements
  `LowerHex`). No public API changed. The `cargo deny` skip for the transitive
  `sha1 0.10.6` (pinned by `sqlx-mysql`) was re-added.
- `DatabaseAdapter` gains `execute_function_call_with_session`,
  `execute_with_projection_arc_with_session`,
  `execute_where_query_arc_with_session`, and
  `execute_parameterized_aggregate_with_session`; `RelayDatabaseAdapter` gains
  `execute_relay_page_with_session`. All have default implementations that
  delegate to the existing methods, so custom adapter implementors need not
  change anything (#329).

### Security

- **MCP tool calls now enforce row-level security and authentication.** Pre-v2.4.0 the MCP (Model Context Protocol) transport built a GraphQL query from the tool call and ran it through the *unauthenticated* executor path (`Executor::execute`), bypassing every protection the HTTP GraphQL endpoint applies via `execute_with_security`: RLS `WHERE`-clause injection, session-variable binding, and `@inject` JWT resolution. On a multi-tenant deployment with RLS configured, any MCP client therefore received rows across **all** tenants, regardless of the `[mcp] require_auth` flag — which until now only gated whether the HTTP endpoint was *mounted*, never whether an individual tool call carried a validated identity.

  The fix threads an optional `SecurityContext` through `mcp::executor::call_tool` and makes it **fail closed**: when no security context is present and the compiled schema has an RLS policy configured *or* `require_auth = true`, the tool call is refused with an authentication error instead of running unfiltered. When a context is present the call is routed through `execute_with_security`, so RLS, session variables, and `@inject` apply exactly as they do for HTTP GraphQL. Over the HTTP transport the `Authorization: Bearer` token is now extracted from the request and validated against the configured OIDC validator per call (mirroring the gRPC handler). The stdio transport carries no per-request credentials, so under RLS or `require_auth` it is governed by the same fail-closed policy — to use stdio MCP unauthenticated, disable `require_auth` and do not configure RLS (development only).

- **Query-complexity scorer is now overflow-safe (fail-closed).** The AST complexity scorer in `graphql/complexity.rs` computed `1 + nested * multiplier` with unchecked `usize` arithmetic, and the pagination multiplier (client-controlled `first`/`limit`/`take`/`last`, clamped to ≤100) compounds multiplicatively per nesting level. A crafted deeply-nested query with pagination args reaches ≈100^depth, overflowing `usize`: in release builds (no `overflow-checks`) the score *wrapped* to a small value and slipped under `max_query_complexity`; in overflow-checked builds (debug/test, and `cargo fuzz`, whose `complexity.rs` target asserts "must never panic") it *panicked*. The scorer now uses `saturating_add`/`saturating_mul`, so an overflowing query saturates to `usize::MAX` and is always rejected (`QueryTooComplex`), never wraps under the limit nor panics. Severity is low for FraiseQL specifically — its view/table-view execution returns the full denormalised entity as one JSONB read, so GraphQL nesting is projection rather than join fan-out and a "bypassed" deep query is cheap to run — but the wrap/panic is a genuine robustness defect. (A follow-up will add clamping of the *top-level* `first`/`limit` row count, which is the actual cost lever in FraiseQL.)

- **`POST /auth/revoke` and `POST /auth/revoke-all` are now authenticated** (#358, FW-21 class). In v2.3.x and earlier, both routes were mounted with no auth middleware, so any unauthenticated client could revoke any harvested JWT (by `jti`) or wipe every active session for any user (by `sub`). The handlers used `jsonwebtoken::dangerous::insecure_decode` to extract the `jti` from a body-supplied token without any proof-of-possession, so the attack required nothing beyond a network path to the server. Affected anyone running `[security.token_revocation] enabled = true`.

  The fix has three parts:

  1. The revocation router is now mounted behind `oidc_auth_middleware` — unauthenticated requests get `401 Unauthorized` before reaching the handler. If `[security.token_revocation]` is configured without a corresponding `[auth]` OIDC validator, the routes are *not* mounted at all and a startup warning is emitted, rather than mounting them open.

  2. `POST /auth/revoke` no longer trusts a token submitted in the body. It revokes the `jti` of the bearer token used to authenticate the request — surfaced as a new `SessionJti` request extension populated by the auth middleware. The body's `token` field is still accepted on the wire for compatibility but is ignored. This closes the residual attack where an authenticated alice could `insecure_decode` a body token claiming `sub: "alice"` but carrying a victim's `jti`.

  3. `POST /auth/revoke-all` now requires the caller's authenticated `sub` to match `body.sub`, unless the caller holds the `admin` scope. Cross-user revocation requests return `403 Forbidden` with a `caller_sub`/`target_sub` warning logged for incident response.

- **Webhook dispatch INFO logs no longer leak URLs, headers, or rendered bodies** (#346). Pre-v2.4.0 `WebhookAction::execute` emitted four INFO lines on every dispatch — full URL (including any query-string secrets or embedded credentials), full headers debug-formatted (including any `Authorization: Bearer ...` operators put in the observer `headers` map), the raw `body_template`, and the full rendered event body as JSON. Centralised log aggregators ingested and retained the payload for every dispatch, exposing bearer tokens (reuse → same access as the framework) and PII rows (customer email, shipping address, payment refs) for the retention window.

  The fix: URL / headers / body are demoted to DEBUG (URL, redacted headers, body template) and TRACE (rendered body). INFO now carries only delivery metadata — `action_type`, `event_id`, `host` (no path/query), `status_code`, `duration_ms`. Two new helpers ship: `redact_secret_headers` masks any header whose name contains (case-insensitive) `authorization`, `api-key`, `cookie`, `secret`, or `token` — false-positives (over-masking) are accepted, false-negatives (printing a real bearer) are not. `url_host_only` extracts the host via `reqwest::Url::parse` so even DEBUG-level URL logs strip userinfo / path / query / fragment when needed.

- **Storage `POST /storage/v1/presign/{bucket}/{*key}` now consults `StorageRlsEvaluator`** (#335). Pre-v2.4.0 the handler lacked the `Option<Extension<StorageUser>>` parameter present on every other storage handler and called neither `state.rls.can_read` / `can_write` nor `state.metadata.get`. Any anonymous client could `POST /storage/v1/presign/<bucket>/<key>` with `{"operation":"download","expires_in_secs":86400}` and receive a 24-hour-valid presigned GET URL for any object in any bucket — including `BucketAccess::Private` buckets owned by other users. With `"operation":"upload"`, the same anonymous client received a presigned PUT URL that overwrote arbitrary objects, bypassing bucket-level `max_object_bytes` and `allowed_mime_types` (those checks live in `put_handler`, not in the S3 presigned URL).

  The handler now mirrors `put_handler` / `get_handler`: `operation = "download"` loads the metadata row and consults `state.rls.can_read` (missing objects yield `404`, denied access yields `403`); `operation = "upload"` consults `state.rls.can_write(bucket)` (denied access yields `401`). The RLS gate runs *before* any S3 work, so unauthorised callers do not observe whether the object exists. Known limitation documented inline: bucket-level `max_object_bytes` and `allowed_mime_types` are still not enforced via the S3 presigned PUT URL (S3 cannot encode those constraints in a vanilla presigned URL); operators must restrict presigned uploads to trusted users via RLS or route through `PUT /storage/v1/{bucket}/{*key}` instead.

- **`[auth_hs256]` now requires `audience` to be set** (#359). The HS256 shared-secret testing path is the most likely place for two services to share a signing key (test fixtures, internal service meshes, monorepo CI); pre-v2.4.0 it accepted any token whose `aud` matched the unset (`None`) configuration — i.e., any token from any service. A token minted for service A was accepted by service B, exactly the cross-service token-confusion attack the v2.3 S40 OIDC hardening closes for the OIDC path. `Hs256Config::validate` now returns an error when `audience` is `None`, called from `build_hs256_auth` at server startup with a clear actionable message. Mirrors `OidcConfig::validate` exactly.

- **`FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass is refused in production environments** (#347). Pre-v2.4.0 the env var disabled every outbound SSRF guard (scheme allowlist, private-IP blocklist, DNS-rebinding defence) in observer dispatch — `validate_outbound_url`, `dns_resolve_and_check`, `executor::dispatch::validate_url_ssrf` — with a `std::sync::Once` warn-on-first-use that was easy to miss in streaming log aggregators. Combined with #348 (anonymous observer install), this was a one-step path to AWS metadata-service credential exfiltration: install an observer pointing at `http://169.254.169.254/latest/meta-data/iam/security-credentials/<role>`, wait for the next mutation.

  The fix centralises the bypass policy in a new `fraiseql_observers::insecure_guard` module. The check now refuses the bypass when ANY production-marker env var is set:

  - `KUBERNETES_SERVICE_HOST` (automatic in any K8s pod).
  - `FRAISEQL_ENV=production` (case-insensitive, also accepts `prod`).
  - `FRAISEQL_PROFILE=production` (case-insensitive, also accepts `prod`).

  When the bypass is refused in production, a structured `ERROR` is logged once per process and a `WARN` is emitted at every outbound dispatch (so operators see the bypass-attempt at every dispatch, not just once at startup). When the bypass is honored in dev, a `WARN` is emitted on every dispatch — the `std::sync::Once` warn-once is gone.

- **Observer admin API now requires authentication** (#348, FW-21 class). All four observer HTTP routers — `observer_routes` (CRUD), `observer_changelog_routes`, `observer_runtime_routes` (`/runtime/health`, `/runtime/reload`), and `observer_dlq_routes` (`/api/observers/dlq/*`) — were previously mounted with no auth middleware. Handlers used `OptionalSecurityContext` (which returns `None` on anonymous calls) or no auth extractor at all, so any unauthenticated client could:

  - `POST /api/observers` — install an attacker-controlled webhook observer pointing at any URL (combined with #347, a one-step path to AWS metadata-service credential exfiltration).
  - `PATCH /api/observers/{id}` — silently re-route an existing observer to an attacker URL.
  - `DELETE /api/observers/{id}` — wipe an observer.
  - `POST /runtime/reload` — denial-of-service against the observer runtime.
  - `GET /api/observers/{id}` — read bearer-token secrets stored in `actions[].headers`.
  - `POST /api/observers/dlq/retry-all` — replay queued events through whatever URL the (now attacker-controlled) observer points at.

  All four router nests now mount behind `oidc_auth_middleware` via `route_layer`. If the `observers` feature is enabled but `[auth]` is not configured (no OIDC validator available), the HTTP admin API is *not* mounted and a `WARN` is logged at startup. The in-process observer runtime — triggers, dispatch, DLQ retention — is unaffected; only the HTTP control plane is gated. Affected anyone running the `observers` feature.

- **Tenant-scoped reads through `CachedDatabaseAdapter` now bypass the result
  cache when session variables are configured (#329)**, until the cache key is
  extended to include a hash of the applied session variables (tracked as a
  follow-up). Before this release the cache key was likewise not
  session-variable-aware, but the bug masked any actual leak by making session
  variables invisible to RLS policies.

### Breaking changes

- **`POST /auth/revoke` request body changed.** The `token` field is now `Option<String>` and ignored. Clients that previously submitted a body token will continue to receive `200 OK`, but the revocation now targets the *authentication* token, not the body token. Update any flow that depended on revoking an arbitrary harvested token via this endpoint — there is no longer such a primitive.

- **`POST /auth/revoke` and `POST /auth/revoke-all` now require a valid bearer token.** Anonymous calls return `401 Unauthorized`. Update any internal tooling that called these endpoints unauthenticated.

- **`[auth_hs256]` refuses to boot without `audience`.** Deployments using HS256 auth with no `audience` will fail startup with an actionable error message. Set `audience = "..."` in the `[auth_hs256]` section of `fraiseql.toml` to your API identifier. There is no compatibility shim — the cross-service token-confusion attack the fix closes (#359) is not acceptable in a "warn-and-continue" mode.

- **Token revocation requires `[auth]` to be configured.** If `[security.token_revocation] enabled = true` but no OIDC validator is present, the revocation routes are skipped at startup (with a `WARN` log) rather than mounted open. Configure `[auth]` in `fraiseql.toml` to restore the routes.

- **Observer admin HTTP API requires `[auth]` to be configured.** If `[auth]` is absent, `/api/observers/*`, `/runtime/health`, and `/runtime/reload` are not mounted (with a `WARN` log at startup) rather than mounted open. Any internal tooling that called these endpoints unauthenticated must now present a valid bearer token. Reverse-proxy auth (mTLS or a bearer-token gate) is no longer the only line of defence.

- **`DatabaseAdapter::set_session_variables` has been removed (#329).** It applied `set_config(..., true)` on a pooled connection in its own autocommit transaction — transaction-local *and* on a different connection than the subsequent operation — so it never reached the operation (the bug this release fixes), and the executor no longer calls it. Custom `DatabaseAdapter` implementors that overrode it should delete the override; any direct caller should switch to the connection-affine `*_with_session` methods (`execute_function_call_with_session`, `execute_where_query_arc_with_session`, `execute_with_projection_arc_with_session`, `execute_parameterized_aggregate_with_session`, and `RelayDatabaseAdapter::execute_relay_page_with_session`), which apply session variables on the same connection as the operation.

- **Mutation response shape now matches the query contract (#410, #400).** Mutation success and error responses are projected by the same engine as queries, which changes three things for clients that relied on the old behaviour. (1) **`__typename` is returned only when selected** — it is no longer auto-injected into every mutation response; add `__typename` to your selection set if you depend on it (this matches the GraphQL spec and the query path). (2) **Nested typed-object fields are now projected** — previously dropped or returned as a verbatim sub-blob, they are now recased and subset to the selection, so clients that hand-rolled per-mutation key recasing must drop it or they will double-convert. (3) **Nested fields inside both success and error responses are now subset to the selection** rather than returned in full. There is no compatibility flag — fix-forward. The `Executor::execute_mutation` signature also changed its last parameter from `&HashMap<String, Vec<String>>` (flattened per-type field names) to `&[FieldSelection]` (the result selection set); pass `&[]` for no field filtering.

### Documentation

- **Added a FreeBSD deployment guide (#148):** `docs/guides/freebsd-deployment.md`
  walks operators through the Jails + ZFS + Caddy stack — building or
  cross-compiling the binary, a two-Jail (API + network-isolated DB) layout
  with a nullfs-mounted Postgres Unix socket, ZFS-clone multi-tenancy, and a
  per-feature FreeBSD support/limitations table.

- **Documented the federation-subgraph pattern for non-SQL mutations (#170).**
  Operations that can't be expressed as PL/pgSQL (AI/ML, payments, external
  services, long-running jobs) are handled with a federation subgraph rather
  than runtime async handlers in core. ADR-0010 is marked **Rejected** with the
  rationale and alternatives considered; a decision guide
  (`docs/guides/non-sql-mutations.md`) covers when to use SQL vs federation vs
  neither; and a runnable example (`examples/async-jobs-subgraph/`) ships a
  self-contained Rust + `async-graphql` subgraph composed alongside a FraiseQL
  schema. Docs and a new example crate only — no engine changes.

### Known limitations

The docs-overhaul audit on 2026-05-29/30 surfaced the following issues
that are **NOT fixed** in this release and remain open for triage. Pin
your usage accordingly:

**Silent-no-op TOML wiring (config looks honored but isn't):**

- #330 — multi-tenant runtime not wired into the `fraiseql-server` binary
- #334 — `[storage.<name>]` / `[files.<name>]` not auto-wired by the binary
- #340 — observer `/runtime/*` mounted at root instead of `/api/observers/runtime/*`
- #341 — DLQ subcommands return hard-coded mock JSON instead of reading the runtime DLQ
- #342 — `[observers]` TOML schema diverges between `fraiseql-cli` and `fraiseql-server`
- #350 — `FRAISEQL_OBSERVER_TRANSPORT` ignored even with `observers-nats`
- #356 — `failed_login_max_attempts` / `failed_login_lockout_secs` dropped by server runtime
- #357 — `[security.token_revocation] backend = "postgres"` silently downgraded to in-memory
- #360 — PKCE routes mount without `[security.state_encryption]` (warn-and-continue, not refuse)
- #361 — JWKS hot-rotate stolen-key replay window: `detect_key_rotation` only warns

**Functional bugs:**

- #331 — WebSocket subscription endpoint drops JWT `tenant_id`
- #332 — suspended tenant returns 403, not 503 + `Retry-After: 60`
- #333 — tenancy header validator and schema-mode validator disagree on tenant-key shape
- #336 — storage bucket name dropped before backend call — cross-bucket key collisions
- #337 — storage stored XSS surface (uploads with attacker `Content-Type`, no `nosniff`)
- #338 — global 1 MB `DefaultBodyLimit` silently caps every storage upload
- #339 — LIKE-pattern injection in `StorageMetadataRepo::list` prefix arg
- #343 — `InMemoryDlq` is unbounded; documented `max_dlq_size` cap silently ignored
- #344 — DLQ retry handlers race; concurrent retries double-fire the webhook
- #345 — webhook payloads are not signed; receivers cannot detect forged events
- #349 — `ActionConfig::Email` observers report success without sending email
- #270 — additional follow-up tracking (see GitHub for details)

These will be addressed in 2.4.x / 2.5.0; tracking on GitHub.
A follow-up runbook with per-issue fix shapes lives at
`/tmp/fraiseql-deferred-bugs-2026-05-30/runbook.md` (local) for the
next agent to pick up cold.

### Known follow-ups (#329)

- Relay `node(id:)` lookups, partial-period aggregate UNION branches, and gRPC
  mutations do not yet thread a `SecurityContext`/session-variable config, so
  `current_setting()`-backed RLS is not configured on those paths. Each call
  site is annotated in the source.

## [2.3.2] - 2026-05-28

### Fixed

- **`cargo publish` for `fraiseql-server`, `fraiseql-cli`, and `fraiseql` (umbrella)** — `crates/fraiseql-server/build.rs` ran `npm install` and `npm run build` inside `crates/fraiseql-server/studio/`, populating `studio/node_modules/` (~45 MB) and `studio/dist/` during cargo's verify step. Cargo correctly flagged this as `Source directory was modified by build.rs during cargo publish` and refused to publish on the v2.3.1 release run (CI run 26516845920). `build.rs` now stages the Studio package into `$OUT_DIR/studio/` and runs npm/esbuild there, so the source tree is no longer touched. The `[package].exclude` for `studio/{node_modules,dist,.cache,.npm,*.log}` is added as defensive insurance against stale on-disk copies leaking into the `.crate` tarball. **This is the first v2.3.x release where `cargo install fraiseql-server` actually works** — v2.3.0 and v2.3.1 were tagged but never published successfully via automation. (Crates.io's `fraiseql-server@2.3.0` was published manually outside the workflow.)

- **`fraiseql-functions` and `fraiseql-storage` missing from release automation** — both crates are publishable (`publish = true` / unset) and `fraiseql-functions` is a mandatory dep of `fraiseql-server`, but neither appeared in `release.yml`'s publish job. Both are now published alongside the other 13 crates on tag push, in correct topological position (`fraiseql-storage` in Tier 2, `fraiseql-functions` in a new Tier 6.5 between observers and server).

### Changed

- **`release.yml` validate-release dry-run now covers every publishable crate** (15 total) instead of only `fraiseql-error`. The packaging-rules failure that blocked v2.3.0 and v2.3.1 publish for `fraiseql-server` would have been caught here. Timeout bumped to 30 minutes to accommodate the longer loop.

### Migration

Consumers currently pinned to `fraiseql-server = "2.3.1"`, `fraiseql-cli = "2.3.1"`, or `fraiseql = "2.3.1"` will get a `cargo install` failure (those versions are not on crates.io). Upgrade to `2.3.2`. Pins to `fraiseql-server = "2.3.0"` continue to resolve but are missing the #316 axum-0.8 startup-panic fix.

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
