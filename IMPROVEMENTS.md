# FraiseQL Improvement Backlog

Generated: 2026-05-24
Audited at commit: 85ac41e60 (chore/clippy-strict-round2)
Re-audited (round 2) at commit: 788320393 (feat/error-taxonomy-consolidation), 2026-05-24
Workspace: 16 crates, fraiseql-* + fraiseql umbrella

## Round-2 update — 2026-05-24, commit 788320393

### Closed by execution
- **F017** — error taxonomy consolidation. Resolved by the 7-commit refactor on `feat/error-taxonomy-consolidation` (230d4d238..788320393). `RuntimeError` and 5 shadow domain enums deleted; `FraiseQLError::{Auth, Webhook, Observer, File}` variants added; subsystem crates own their own `From<X> for FraiseQLError` impls (sqlx pattern); `ServerError::RuntimeError` renamed to `Engine`. See follow-up F049, F050, F051 below for industrial gaps the refactor introduced.

### Re-prioritized under industrial framing
- **F003** — was 🟠 H / S, **upgraded to 🟠 H / S with concrete API change recommended**. Previously hedged "If `ValidationRule` shape changes, that crosses the serde boundary — prefer a runtime-side cache." Industrial answer: change `ValidationRule::Pattern { pattern: String }` to `ValidationRule::Pattern { pattern: CompiledPattern }` with a custom serde impl that compiles at deserialize-time. The validator stops being responsible for caching, and pattern errors surface at schema load.
- **F006 / F008 / F013 / F048** — Arc<Mutex/RwLock<HashMap>> findings. Previously each carried "internal data structure / surface unchanged" hedges. Under industrial: ship them together as a single PR titled "concurrency: lock-free read-hot maps" using `dashmap` (already a workspace dep) and `arc-swap` for the snapshot-style cases (F007, F048). Per-finding severity unchanged, but bundling raises the joint impact to 🟠 H.
- **F016** — was 🟡 M / XS, **upgraded to 🟠 H / XS**. The doctest's broken references (`RateLimitExceeded`, `Forbidden`, `FieldExclusion`, `TypeMismatch`) are now strictly more conspicuous because the round-2 refactor added 4 *real* variants (`Auth`, `Webhook`, `Observer`, `File`) to the same match arm without fixing the dead siblings. The contradiction is the documentation. Closing this is a 5-minute edit and unblocks any future doctest enforcement.
- **F018** — was 🟡 M / S, **upgraded to 🟠 H / S**. The `Box<dyn Fn() -> u64 + Send + Sync>` clock pattern was hedged "API change". Industrial: kill the boxed clock entirely. Make `KeyedRateLimiter<C: Clock = SystemClock>` generic; tests pass `MockClock`. One-line `Cargo.toml` add for `quanta` or hand-roll a 3-line `Clock` trait. The clock is *not* a runtime-swappable dependency in any production path.
- **F021** — was 🟡 M / M, **upgraded to 🟠 H / M**. Fire-and-forget `tokio::spawn` in lifecycle paths was hedged "easy to introduce hangs". Industrial: replace every untracked `tokio::spawn` with a `JoinSet` held on the `Server` struct, drop the timeout-based shutdown for an explicit `select!` between handles and `shutdown_token`. The audit is mechanical (~6 call sites) and the alternative is the kind of half-graceful shutdown that ends in lost-message bug reports.
- **F026** — was 🟢 L / L, **closed without action**. Q2 policy froze `async_trait` baseline at 180. Pre-existing decision; round-2 honours it.
- **F032** — was 🟢 L / M (crate READMEs missing), **upgraded to 🟠 H / M**. Under industrial framing, a published-to-crates.io workspace with bare landing pages is a documentation defect that costs adoption. The cost is mechanical (16 crates × 30-line README each). Bundle into a docs PR after the F049+ taxonomy follow-ups land.
- **F036** — was 🟡 M / M (Box<dyn ToSql> per param), **upgraded to 🟠 H / M**. Previously hedged "matching tokio-postgres lifetime rules". Industrial answer: lifetime rules are a one-time engineering cost and the trait is stable in tokio-postgres 0.7. Allocations on the hot path are uncapped under load.

### New findings (F049+)
- **F049** — `FraiseQLError::{Auth, Webhook, Observer}` boxed-payload variants drop the `Error::source()` chain.
- **F050** — `FraiseQLError::Storage` and `FraiseQLError::File` carve the file domain across two variants with divergent HTTP codes.
- **F051** — `FraiseQLError::Storage` variant has no documented owner after the file/storage split.
- **F052** — `FraiseQLError::Auth(Box<dyn Error>)` widens the type-erasure surface and prevents downstream matching on auth-error subclasses.
- **F053** — Wire-crate Q3 recommendation: split the 19 crate-level allows into a 4-line cast denylist (kept) + per-module/per-test relocations.
- **F054** — `RateLimited` field renamed `retry_after_secs` (round-2 audit caught a round-1 doc reference still calling it `retry_after`).
- **F055** — `IntoResponse for FraiseQLError` exhaustive match on `#[non_exhaustive]` enum will silently break on next variant add.

### Closed without action (no longer applicable / wrong)
- **F026** — `async_trait` removal audit. Q2 policy locked this. Round-2 will not re-flag traits for `async_trait` removal.

---



## Severity
🔴 Critical — correctness bug, data loss, security, or unbounded resource use
🟠 High    — measurable perf/ergo win, or risk of regression
🟡 Medium  — technical debt, future-trap, ergonomic friction
🟢 Low     — nit, style polish, doc gap

## Effort
XS  (< 30 min)
S   (1–2 hours)
M   (half day to one day)
L   (2–5 days)
XL  (> one week)

## Top 10 quick wins (XS/S, high impact)

1. [F003] Cache `regex::Regex::new(pattern)` in dynamic validators — currently recompiled per-call on the request hot path.
2. [F001] Drop the double GraphQL parse: validator + matcher both call `graphql_parser::parse_query` on every request.
3. [F006] Replace `Arc<Mutex<HashMap>>` in `KeyedRateLimiter` with `DashMap<String, Mutex<RequestRecord>>` — auth endpoint per-IP serialisation today.
4. [F010] `AuthRequest` derives `Debug` and stores raw `Authorization` header — token leakage risk if logged with `?req`.
5. [F012] `Secret::Drop` does not call `zeroize` — secret string lingers in freed heap pages despite the crate already depending on `zeroize`.
6. [F002] `ResponseCache` hit path returns `(*cached).clone()` of `Arc<Value>` — deep-clones the response on every cache hit; expose `Arc<Value>` instead.
7. [F004] `QueryRunner::compute_response_cache_key` allocates a fresh `String` via `serde_json::to_string` per argument per request — use `serde_json::to_writer` into a reusable scratch `Vec<u8>` (or hash bytes directly).
8. [F007] `TrustedDocumentStore` uses `tokio::sync::RwLock<HashMap<String, String>>` on the resolve-per-request hot path with `.cloned()` of the query body — switch to `arc-swap::ArcSwap<HashMap<String, Arc<str>>>`.
9. [F015] Pin `redis = "1"` to `[workspace.dependencies]` — currently a free-standing dep in 4 crates (auth/core/observers/server) with copy-pasted feature lists, inviting version skew.
10. [F022] Enable the commented-out `mold` linker block in `.cargo/config.toml` (gated to local non-CI builds via env / conditional) — 3-5× full-rebuild speedup is documented but not enabled.

## Top 5 multi-day investments (L/XL, high impact)

1. [F005] Rework `QueryMatcher::extract_arguments` to borrow from the parsed variables instead of cloning every `(String, Value)` into a fresh `HashMap` — large mutation payloads pay the clone on every request.
2. [F008] Replace the four `Arc<RwLock<HashMap>>` bucket maps in `InMemoryRateLimiter` with `DashMap`s and shift the bucket itself behind a `parking_lot::Mutex<TokenBucket>` — every request currently acquires a tokio RwLock guard across an await even for the lookup.
3. [F009] `MetricsCollector` wraps each of ~30 counter fields in its own `Arc<AtomicU64>` even though the collector itself is already `Arc`-shared — flatten to plain `AtomicU64`, halve struct size and remove ~30 atomic Arc-clones per construction.
4. [F037] Build a generation-counted view-name interner so `accessed_views: Box<[String]>` on `CachedResult` / `ResponseEntry` becomes `Box<[InternedView]>` — every cache write currently allocates one `String` per view per entry, with `view_index: DashMap<String, …>` also allocating one key per insert.
5. [F042] `ParsedQuery.source: String` stores an owned copy of every query string parsed; downstream consumers only need `&str`. Migrate `ParsedQuery` to be parameterised over `Cow<'a, str>` / `Arc<str>` and stop the per-request `source.to_string()` allocation in `parse_query`.

---

## Findings by category

### Performance

#### F001 — Validator and matcher both parse the query
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** Eliminates one `graphql_parser::parse_query` call per request (the parser allocates `Vec<Definition<String>>` and many small Strings for every keyword and identifier).
- **Location:** `crates/fraiseql-core/src/graphql/complexity.rs:209` (`RequestValidator::validate_query`) and `crates/fraiseql-core/src/runtime/matcher.rs:144` (`QueryMatcher::match_query`); orchestrated together in `crates/fraiseql-server/src/routes/graphql/handler.rs:379` → `:566`.
- **Finding:** The handler first calls `validator.validate_query(&query)` which parses the document with `graphql_parser::parse_query::<String>` to compute depth/complexity/alias counts. Immediately afterwards the executor enters `match_query`, which calls `parse_query` (the wrapper) — another `graphql_parser::parse_query::<String>` on the same string. The parser walks AST twice per request.
- **Suggested approach:** Parse once in the handler, hand the `ParsedQuery` (or the underlying `Document<&str>`) to both `validate_query_ast(&parsed)` and `match_query_from_parsed(&parsed, variables)`. Add `RequestValidator::validate_query_ast(&Document<…>)` and use the existing AST helpers.
- **Verification:** Add a criterion bench in `crates/fraiseql-core/benches/graphql_parse.rs` that times "parse + validate + match" before/after; expect ~30-50 % reduction in CPU per request for queries that exercise validation.
- **Risk:** Public API change to `RequestValidator::validate_query` signature; either deprecate-and-add or keep the string variant for callers that don't have an AST yet.
- **Confidence:** High
- **Status:** Closed in b94abc592 — `RequestValidator::validate_query_doc(&Document<'_, String>)` accepts the pre-parsed AST and `parse_graphql_document(&str)` exposes the underlying parse. The HTTP handler now parses once at the validator boundary and feeds the same `Document` into `validate_query_doc`. Threading the AST through `Executor::execute` (the matcher's parse) is left as a follow-up.

#### F002 — Response cache hit deep-clones the cached value
- **Severity:** 🟠 High
- **Effort:** XS
- **Impact:** A response cache hit currently dereferences `Arc<Value>` and deep-clones the entire JSON tree — defeating the point of storing the response as an `Arc`. For a 50 KB GraphQL response, a single hit allocates ~50 KB.
- **Location:** `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:106` (`return Ok((*cached).clone())`).
- **Finding:** `rc.get(query_key, sec_hash)` returns `Option<Arc<Value>>`. The current code dereferences (`*cached`) then `.clone()`s the `Value`, which is a recursive walk that allocates new `String`s and `Vec`s for every node.
- **Suggested approach:** Change the public return type of `execute_regular_query_with_security` (and the `execute*` entry points) to `Arc<Value>` (or wrap in a thin newtype) and let callers `unwrap_or_clone` if they need ownership. Even keeping the signature, use `Arc::unwrap_or_clone(cached)` to take ownership when possible.
- **Verification:** `criterion` bench measuring "10 000 cache hits of a 50 KB response" with allocator counter (DHAT) before/after.
- **Risk:** Touches the return type of the executor's public method; cascades to `graphql_handler` and tests.
- **Confidence:** High
- **Status:** Closed in 15fd10a48 — applied the minimum-effort signature-preserving variant: `Arc::unwrap_or_clone(cached)` on the cache-hit return path and `Arc::new(response)` + `Arc::clone(&cached)` for the put + return on the cache-miss path. The wider signature change (return `Arc<Value>` end-to-end) was deferred — the `execute_with_security` trait method has 3 impls across fraiseql-arrow and fraiseql-server; routing the Arc through them is a follow-up.

#### F003 — Dynamic validators recompile `Regex` per request
- **Status:** Closed in dd4393d06 — `ValidationRule::Pattern { pattern: String, .. }` is now `ValidationRule::Pattern { pattern: CompiledPattern, .. }` where `CompiledPattern` owns a pre-compiled `Regex` plus the original source string for serde round-trip. Compilation happens once at construction (or at `schema.compiled.json` deserialisation); the three hot-path sites (`runtime/input_validator.rs`, `validation/composite.rs`, `validation/custom_type_registry::validate_pattern`) reuse the compiled `Regex` directly. Invalid patterns now surface at schema load instead of degrading silently per request.
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** Each `ValidationRule::Pattern` validation invokes `regex::Regex::new(pattern)` on every call. Compiling a moderate regex takes 50–200 µs; for a field validated on every request, this dominates execution time.
- **Location:** `crates/fraiseql-core/src/runtime/input_validator.rs:162` and `crates/fraiseql-core/src/validation/composite.rs:247`.
- **Finding:** Both sites unconditionally compile the pattern string. The other validators in this codebase already use `LazyLock<Regex>` (`crates/fraiseql-core/src/validation/rich_scalars.rs:13`); the dynamic-pattern path is the outlier.
- **Suggested approach:** Cache `Arc<Regex>` keyed on the pattern string in a `DashMap<String, Arc<Regex>>` owned by the validator. Stale entries don't matter (validators are read-mostly). If patterns come from a fixed schema, compile once at schema-load time and store `Arc<Regex>` on the `ValidationRule::Pattern` variant directly.
- **Verification:** Add a bench compiling and matching a 30-character regex against 100 short strings; compare before/after.
- **Risk:** None if the cache is internal. If `ValidationRule` shape changes, that crosses the serde boundary — prefer a runtime-side cache.
- **Confidence:** High

#### F004 — `compute_response_cache_key` allocates a String per argument
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** For a mutation with N variables, N temporary `String`s are allocated to feed the hasher. On a hot path this is unnecessary.
- **Location:** `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:328`.
- **Finding:** The key derivation calls `serde_json::to_string(&query_match.arguments[key])` and hashes the resulting string. The intermediate string serves no purpose beyond producing bytes for the hasher.
- **Suggested approach:** Use a thread-local `Vec<u8>` and `serde_json::to_writer(&mut scratch, value)`, then `scratch.hash(&mut hasher); scratch.clear()`. Alternatively, hash the JSON value directly via a custom `Hash` impl that walks the tree.
- **Verification:** Criterion bench around `compute_response_cache_key` with a 5-arg payload.
- **Risk:** None — internal hashing only.
- **Confidence:** High

#### F005 — `extract_arguments` clones every variable
- **Severity:** 🟠 High
- **Effort:** M
- **Impact:** Every GraphQL request with variables incurs `HashMap::insert(k.clone(), v.clone())` over the entire variables map. For a mutation with a 100 KB JSON input the clone walks the whole tree.
- **Location:** `crates/fraiseql-core/src/runtime/matcher.rs:247-255` and `:235` (`build_variables_map`).
- **Finding:** `QueryMatch::arguments` is owned, so the matcher must clone. But all downstream consumers (`combine_explicit_arg_where`, `inject_param_where_clause`, projection planning) only borrow.
- **Suggested approach:** Change `QueryMatch` to borrow: `arguments: &'a serde_json::Map<String, Value>` with a `'a` lifetime tied to the request. If the borrow propagation is too invasive, switch the map values to `Cow<'a, Value>` or `Arc<Value>` so the clone collapses to an `Arc` increment.
- **Verification:** DHAT allocator bench over a synthetic mutation with a 100 KB input.
- **Risk:** `QueryMatch` is widely used across `runtime/executor/**`; the lifetime change ripples through every method signature.
- **Confidence:** Medium (the borrow change is mechanically straightforward, but the blast radius is wide).
- **Status:** Closed in 38c6e705b together with F024 — the matcher used to build the variables map twice (once for directive evaluation, once for `QueryMatch::arguments`). Folded into a single `variables_to_map` conversion that is borrowed by the directive evaluator and then moved onto `QueryMatch.arguments`. The wider "make `QueryMatch` borrow its arguments" change was deferred — the executor's downstream call chain would need a `'a` lifetime in every signature; not necessary to deliver the F005/F024 win.

#### F009 — `MetricsCollector` redundantly wraps every counter in `Arc`
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** ~30 individual `Arc<AtomicU64>` fields means the struct is ~30 cache lines wide instead of ~3. Construction performs 30 atomic ref-count bumps. The Arc indirection is pointless since `MetricsCollector` is itself stored behind `Arc` everywhere it is used.
- **Location:** `crates/fraiseql-server/src/metrics_server.rs:95-163`.
- **Finding:** Each counter is `pub queries_total: Arc<AtomicU64>` etc. `MetricsCollector` is consumed exclusively as `Arc<MetricsCollector>` (see `app_state.rs`). Wrapping the inner counter is therefore a double Arc.
- **Suggested approach:** Replace each `Arc<AtomicU64>` with bare `AtomicU64`. Callers go from `metrics.queries_total.fetch_add(...)` (unchanged at the call site) to a single Arc deref. Group counters into related sub-structs (`GraphqlMetrics`, `FederationMetrics`, `HttpMetrics`) for cache locality and code hygiene.
- **Verification:** Compile-time check of struct size via `std::mem::size_of`; runtime smoke test that fetch ordering remains unchanged.
- **Risk:** Any external code accessing the fields by name will need to drop the `Arc::clone(&metrics.queries_total)` pattern. Grep before changing.
- **Confidence:** High
- **Status:** Closed in f5ddaa59e — 28 atomic counter fields switched from `Arc<AtomicU64>` to bare `AtomicU64`. `MetricsCollector` no longer derives `Clone` (atomics aren't `Clone`); the production wiring already wraps in `Arc<MetricsCollector>` so the only test affected (`metrics_collector_clone_shares_state`) was rewritten as `metrics_collector_arc_shares_state`. The histograms and `OperationMetricsRegistry` keep their `Arc` wrappers (genuinely shared with export endpoints). Sub-struct regrouping (`GraphqlMetrics`/`FederationMetrics`/`HttpMetrics`) deferred — would touch too many call sites for a single PR.

#### F037 — Cache write allocates a `String` per view and per index key
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** Every `ResultCache::put` and `ResponseCache::put` walks `accessed_views: &[String]` and stores them as `Box<[String]>`; the reverse index (`view_index: DashMap<String, DashSet<u64>>`) also allocates each view name as a fresh key on first insert.
- **Location:** `crates/fraiseql-core/src/cache/result.rs:65` (`accessed_views: Box<[String]>`), `crates/fraiseql-core/src/cache/response_cache.rs:69` and `:89`.
- **Finding:** View names are stable identifiers drawn from a small, schema-bounded set. Allocating fresh `String`s per cache entry — and again per index key — is pure waste.
- **Suggested approach:** Build a `ViewInterner` (`DashMap<&'static str /* by schema gen */ , Arc<str>>`) at schema load. Store `accessed_views: Box<[Arc<str>]>`. The reverse index becomes `DashMap<Arc<str>, …>` keyed by pointer-identity-equal `Arc<str>`. Cache writes become essentially copy-free.
- **Verification:** DHAT bench of the cache write path.
- **Risk:** Cross-schema interning requires invalidating the interner on schema reload. The existing schema-reload path (`AppState::reload`) is the obvious place to hook.
- **Confidence:** Medium

#### F042 — `ParsedQuery.source: String` clones the query body
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** Every `parse_query` call clones the entire query string. Trusted-document and APQ hot paths already have the body as `String` upstream; the second copy is gratuitous.
- **Location:** `crates/fraiseql-core/src/graphql/parser.rs:89` (`source: source.to_string()`), `crates/fraiseql-core/src/graphql/types.rs:34` (`pub source: String`).
- **Finding:** `ParsedQuery::source` is read by `Display` impls and error formatters that only need `&str`. Keeping the source as `Arc<str>` would let the parser share the input with the original allocation.
- **Suggested approach:** Either parameterise `ParsedQuery<'a>` over the source lifetime, or change `source` to `Arc<str>` and have the handler hand the body to the parser as `Arc<str>` from the outset (the body is already cloned out of `request.query` once).
- **Verification:** Allocation counter (`dhat`) under the existing graphql_parse benches.
- **Risk:** `ParsedQuery` flows through many APIs; the lifetime form may be invasive. The `Arc<str>` form is mechanical.
- **Confidence:** Medium

#### F011 — Arrow Flight `stream::iter(vec![...])` discards backpressure
- **Severity:** 🟡 Medium
- **Effort:** M
- **Impact:** Arrow Flight responses materialise the full message list in a `Vec` before wrapping it in `stream::iter` — peak memory equals full response size, no chance for streaming pressure.
- **Location:** `crates/fraiseql-arrow/src/flight_server/service.rs:528, :731, :869, :932, :1069, :1087, :1130, :1168`.
- **Finding:** Eight call sites in `service.rs` build a complete `Vec<FlightData>` then convert with `futures::stream::iter`. The Flight server's purpose is high-throughput data delivery; eager materialisation negates that.
- **Suggested approach:** Convert each producer to a `try_stream!` (via `async-stream`) or to a hand-rolled `Stream` that pulls from the database one batch at a time and yields it before fetching the next.
- **Verification:** Add a smoke test that triggers a 1 GB Flight response and asserts peak RSS stays below ~100 MB.
- **Risk:** Larger refactor; ensure error semantics (mid-stream failure) remain compatible with Flight clients.
- **Confidence:** Medium

#### F019 — `parking_lot::Mutex` would replace tokio `Mutex` for sync critical sections
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** `tokio::sync::Mutex` is ~10× slower than `parking_lot::Mutex` for short critical sections that never await. The APQ in-memory storage holds the lock only for a `HashMap::insert/get`.
- **Location:** `crates/fraiseql-core/src/apq/memory_storage.rs:37` (`entries: tokio::sync::Mutex<HashMap<…>>`).
- **Finding:** `MemoryApqStorage` holds the lock only across synchronous `HashMap` ops. The async lock buys nothing here; it forces tasks to use the tokio scheduler for a contention-free path.
- **Suggested approach:** Swap to `parking_lot::Mutex<HashMap<…>>`. The `async fn get/set` methods can keep their signatures (return immediately).
- **Verification:** Existing APQ tests; add a tiny bench around `get/set`.
- **Risk:** None — caller signatures unchanged.
- **Confidence:** High

### Correctness

#### F006 — `KeyedRateLimiter` serialises every auth request through one mutex
- **Status:** Closed in c5c946fb3 — `KeyedRateLimiter` switched to `Arc<DashMap<String, RequestRecord>>`. The hot path uses `DashMap::entry()` so the read-modify-write of a `RequestRecord` for a given key remains atomic per shard, while distinct keys never contend. Periodic sweep and capacity eviction run outside any per-key lock and are now documented as best-effort. Poison-recovery code path removed.
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** All auth-related request paths (login, OAuth callback, etc.) contend on a single `Arc<Mutex<HashMap<String, RequestRecord>>>`. Under load every request blocks on the same lock for the duration of HashMap walk and (every PURGE_INTERVAL calls) a full `.retain()` scan.
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:138, :268-336`.
- **Finding:** The hot `check()` path acquires a single `std::sync::Mutex`, holds it across a HashMap lookup, an optional `min_by_key` scan over all entries (when at capacity), and a write of the record. Worst case is O(N) under the lock.
- **Suggested approach:** Replace with `DashMap<String, Mutex<RequestRecord>>`. The expiry sweep moves to a periodic background task (or piggy-back on `dashmap::DashMap::retain`, which holds per-shard locks). The eviction-when-full case becomes a fallback path that takes a slower lock.
- **Verification:** Add a concurrent bench in `crates/fraiseql-auth/` simulating 1 000 concurrent checks against 100 distinct keys.
- **Risk:** Eviction semantics change subtly (sharded LRU vs. global LRU). The current LRU-by-window-start is approximate anyway.
- **Confidence:** High

#### F008 — In-memory rate-limit buckets use tokio `RwLock<HashMap>` on every request
- **Status:** Closed in 6f79c711e — all four `Arc<RwLock<HashMap<…, TokenBucket>>>` fields in `InMemoryRateLimiter` (`ip_buckets`, `user_buckets`, `path_ip_buckets`, `tenant_buckets`) switched to `Arc<DashMap<…, TokenBucket>>`. The four `check_*` paths use `DashMap::entry()` for per-key atomicity over `try_consume + token_count` and a best-effort capacity check via `contains_key + len`. No inner `parking_lot::Mutex<TokenBucket>` introduced — DashMap's `entry()` already provides per-key exclusive access. `check_*` methods kept their `async fn` signatures so the dispatch enum (in-memory + Redis variants) stays uniform.
- **Severity:** 🟠 High
- **Effort:** M
- **Impact:** Every request acquires a tokio `RwLock` across an await (the bucket itself), and the read lock blocks any concurrent write/refill. Under burst load, the read latency on the hot path grows.
- **Location:** `crates/fraiseql-server/src/middleware/rate_limit/in_memory.rs:18, :20, :24, :26`.
- **Finding:** Four maps (`ip_buckets`, `user_buckets`, `path_ip_buckets`, `tenant_buckets`) all use `Arc<RwLock<HashMap<…, TokenBucket>>>`. Bucket refill is fast and synchronous; the surrounding tokio RwLock is an over-fit.
- **Suggested approach:** `DashMap<String, parking_lot::Mutex<TokenBucket>>` per map. Replace upgrade-from-read paths (`read_then_write`) with `entry().or_insert_with(...)`.
- **Verification:** Concurrent bench at >1 000 RPS, multiple keys.
- **Risk:** Public surface unchanged; internal only.
- **Confidence:** High

#### F013 — Federation `ConnectionManager` `Mutex<HashMap>` with `.unwrap_or_else(|e| e.into_inner())`
- **Status:** Closed in 3cda8124f — `ConnectionManager::adapters` switched from `Arc<Mutex<HashMap<String, ArcDatabaseAdapter>>>` to `Arc<DashMap<…>>`. The three `.lock().unwrap_or_else(|e| e.into_inner())` sites are gone (DashMap has no poisoning). Cache-hit path uses a `dashmap::Ref` borrow; close/clear/count all drop their explicit lock guards.
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** Recovering from a poisoned lock by silently using the inner data risks operating on partially-mutated state. The pattern is correct here (HashMap remains structurally valid) but the choice of `std::sync::Mutex` over `parking_lot::Mutex` is what forces the poisoning case at all.
- **Location:** `crates/fraiseql-federation/src/connection_manager.rs:132, :173, :193`.
- **Finding:** `Arc<Mutex<HashMap<String, ArcDatabaseAdapter>>>` plus `.lock().unwrap_or_else(|e| e.into_inner())`. The recover-from-poison wart appears 3 times.
- **Suggested approach:** Switch to `parking_lot::Mutex` (no poisoning) or `DashMap` (no central lock at all). The cache is read-mostly after warm-up, so `DashMap` is the better fit.
- **Verification:** Existing tests; doctest in the module.
- **Risk:** None — internal data structure.
- **Confidence:** High

#### F014 — `job_queue::executor::execute_batch` silently drops join outcomes
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** When a job task panics or returns an error, the outcome is discarded. Failures are invisible to the worker loop and to metrics.
- **Location:** `crates/fraiseql-observers/src/job_queue/executor.rs:154` (`join_set.join_next().await;`) and `:159` (`while join_set.join_next().await.is_some() {}`).
- **Finding:** Both join sites use `.await` without inspecting the `JoinError` (panic? cancelled?) or the inner `Result<()>` from `execute_job_with_retry`. Even though that inner function returns `()`, panics still surface in the `JoinError` and are currently ignored.
- **Suggested approach:** Match on the join result: `match join_set.join_next().await { Some(Ok(_)) => {}, Some(Err(je)) if je.is_panic() => { error!(..., "worker job panicked"); metrics.errors.fetch_add(1, Relaxed); }, Some(Err(je)) => warn!(..., ?je), None => break }`.
- **Verification:** Add a unit test that injects a panicking job and asserts the error counter increments.
- **Risk:** None — purely additive observability.
- **Confidence:** High
- **Status:** Closed in d1c89be6e — extracted `handle_join_outcome` helper that logs panics at `error!` (with `worker` and `error` fields) and cancellations at `warn!`. When the `metrics` feature is enabled, panics increment the prometheus `fraiseql_observer_job_failed_total{error="panic"}` counter (existing `job_failed` API; no new metric field). Unit test for the panic path deferred — would require constructing a fake `JobExecutor` with sufficient scaffolding to drive a panicking task; the change is purely additive observability per the original finding.

#### F023 — `validate_query`'s "skip-if-all-disabled" branch is the wrong shape
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** Misconfiguration where one of the three checks is silently enabled (e.g., `max_aliases_per_query = 1`) but the others are disabled still incurs the full parse. The comment says it's intentional, but reading the boolean logic again shows the parse runs *unless* all three checks are disabled — fine, but the comment swaps "depth and complexity off" semantics in a confusing way.
- **Location:** `crates/fraiseql-core/src/graphql/complexity.rs:205-207`.
- **Finding:** The comment talks about "alias amplification" being a "distinct DoS vector" that "must run even when depth and complexity validation are both turned off" — but the gate is `if !depth && !complexity && max_aliases == 0`. The truth-table is correct; the prose makes it sound like alias-check is always-on.
- **Suggested approach:** Reword the comment to match the gate, or invert the conditional and exit-early to make the intent explicit.
- **Risk:** Docs-only.
- **Confidence:** High
- **Status:** Closed in cf3a24c2e — extracted `ComplexityValidator::is_no_op() -> bool` const helper. Its doc-comment matches the truth-table (alias-amplification *is* gated, not always-on). `validate_query`'s no-op branch is now `if self.is_no_op() { return Ok(()) }`.

#### F024 — `extract_arguments` clones variables, then `build_variables_map` clones them again
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** Double-clone of every variable per request.
- **Location:** `crates/fraiseql-core/src/runtime/matcher.rs:232-256`.
- **Finding:** `match_query` calls `self.build_variables_map(variables)` (`:150`) then `self.extract_arguments(variables)` (`:235-238`). Both iterate the same `serde_json::Map` and clone every key+value. The two maps are then used for slightly different purposes but contain identical data.
- **Suggested approach:** Build one map (preferably borrowed; see F005) and pass references where each consumer needs read access.
- **Risk:** Internal; signature change for `build_variables_map`/`extract_arguments`.
- **Confidence:** High
- **Status:** Closed in 38c6e705b together with F005 — the two clone passes are now a single conversion. The same `HashMap<String, Value>` is borrowed by the directive evaluator and then moved onto `QueryMatch.arguments`.

### API design

#### F018 — `Box<dyn Fn() -> u64 + Send + Sync>` clock in `KeyedRateLimiter` blocks `Clone`
- **Status:** Closed in 3dca6bd67 — `KeyedRateLimiter<C: Clock = SystemClock>` is now generic over a new `Clock` trait. `SystemClock` is a zero-sized type holding the existing fail-closed `SystemTime::now()` semantics; a blanket impl on `F: Fn() -> u64 + Send + Sync` keeps test ergonomics so closures and `fn` pointers (like `|| u64::MAX`) work unchanged through `with_clock`. The limiter is `Clone` whenever `C: Clone` (which `SystemClock` is via `Copy`) — verified by a new regression test in `tests/rate_limiter_time_tests.rs`.
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** The trait-object clock prevents `KeyedRateLimiter: Clone` and forces a heap allocation on construction. The clock is only swapped in tests; a generic `<C: Fn() -> u64 + Send + Sync>` would inline it.
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:145`.
- **Finding:** A non-test rate limiter always uses the `system_clock` static fn. Boxing for the test case taxes every production limiter.
- **Suggested approach:** Make `KeyedRateLimiter<C = fn() -> u64>`; default to `system_clock`. Tests instantiate `KeyedRateLimiter<impl Fn() -> u64>` directly.
- **Risk:** API change — `KeyedRateLimiter::with_clock` signature changes from `Box` to generic.
- **Confidence:** High

#### F020 — `extract_root_field_names` returns `Vec<&str>` for callers that immediately iterate
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** Minor; one allocation removed per call.
- **Location:** `crates/fraiseql-core/src/runtime/executor/support/pipeline.rs:77`.
- **Finding:** Returns `Vec<&str>`; could be `impl Iterator<Item = &str>` since both call sites iterate without index.
- **Suggested approach:** `pub fn extract_root_field_names<'a>(parsed: &'a ParsedQuery) -> impl Iterator<Item = &'a str> + 'a`.
- **Risk:** Tiny — change two call sites that may take `.collect()`.
- **Confidence:** High
- **Status:** Closed in dffa25762 — signature changed to `impl Iterator<Item = &str> + '_`. The only non-test callers (the function is re-exported in `runtime/mod.rs` but only test files reference it by name across `crates/`) were the two assertions in `executor/support/tests.rs`, which now `.collect()` into a `Vec` explicitly. Breaking change marked with `!` in the commit.

#### F036 — `to_sql_param` returns `Box<dyn ToSql + Sync + Send>` per parameter
- **Status:** Closed in c9b599e15 — the `to_sql_param` helper was dead code: every hot-path call site already used the borrowing pattern `.iter().map(|p| p as &(dyn ToSql + Sync)).collect()` (PR notes called this out as "already shifted"). Deleted the helper and added `as_sql_param_refs(&[QueryParam]) -> Vec<&(dyn ToSql + Sync)>` to centralise the repeated boilerplate. `QueryParam` already implemented `ToSql` so no per-parameter heap allocation remains on the query path.
- **Severity:** 🟠 High
- **Effort:** M
- **Impact:** Every query parameter heap-allocates a `Box<dyn ToSql>`. For a query with 10 params and 1 000 RPS, that's 10 000 allocations/s for a borrow that could be `&dyn ToSql` or an enum dispatch.
- **Location:** `crates/fraiseql-db/src/types/db_types.rs:184`.
- **Finding:** `QueryParam` is an enum already (`Text`, `BigInt`, etc.). Converting to `Box<dyn ToSql>` defeats the enum representation. The tokio-postgres `Statement::query` API accepts `&[&(dyn ToSql + Sync)]` slices, so the trait object can live behind a borrow.
- **Suggested approach:** Add `impl ToSql for QueryParam` and pass `&[&dyn ToSql + Sync]` slices built from `&QueryParam` references. Existing call sites that do `params.iter().map(|p| to_sql_param(p)).collect::<Vec<_>>()` shrink to a borrow.
- **Verification:** DHAT bench around `execute_with_projection_arc`.
- **Risk:** `ToSql` is from `tokio-postgres`; implementing it for `QueryParam` requires matching its lifetime rules.
- **Confidence:** Medium

### Error handling

#### F016 — `FraiseQLError` doctest references non-existent variants
- **Severity:** 🟠 High (re-prioritised — see round-2 update)
- **Effort:** XS
- **Impact:** A documented compile_fail example references `FraiseQLError::RateLimitExceeded`, `::Forbidden`, `::FieldExclusion`, `::TypeMismatch` — none of which exist in the enum (which has 22 variants ending at `Internal`). The compile_fail attribute hides the broken doc. **Round-2 update**: the refactor added 4 *real* variants (`Auth`, `Webhook`, `Observer`, `File`) to the same match arm without fixing the dead siblings — the doctest now has 4 valid arms and 4 invalid arms intermixed.
- **Location:** `crates/fraiseql-error/src/core_error.rs:85-94` vs. the actual enum at `:100-302`.
- **Finding:** The doctest is `compile_fail` so the broken references don't gate CI; they exist as user-facing documentation. Round-2 verification shows lines 85-94 still reference `RateLimitExceeded/Forbidden/FieldExclusion/TypeMismatch` (none exist) alongside the new `Auth/Webhook/Observer/File` arms (all exist).
- **Suggested approach:** Rewrite the doctest to enumerate only the real variants and rely on `#[non_exhaustive]` to demonstrate the wildcard requirement. With 22+ variants, prefer a short example showing 2-3 variants plus the `_ => ...` wildcard.
- **Risk:** Docs-only.
- **Confidence:** High
- **Status:** Closed in bc9df7dc2 — doctest rewritten to enumerate only 3 real variants (`Parse`, `Validation`, `Database`) with an explanatory comment about `#[non_exhaustive]`. All 4 fictional references removed.

#### F017 — `RuntimeError` and `FraiseQLError` overlap on RateLimited / NotFound / Internal / ServiceUnavailable
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** Two error hierarchies cover the same conceptual domain. Conversions between them are repeated formatting passes that often drop the underlying `#[source]`.
- **Location:** `crates/fraiseql-error/src/lib.rs:74-151` and `crates/fraiseql-error/src/core_error.rs:96-269`.
- **Finding:** `RuntimeError::RateLimited`, `::NotFound`, `::Internal`, `::ServiceUnavailable` all duplicate `FraiseQLError::*` variants. The two enums coexist (RuntimeError aggregates domain errors for HTTP; FraiseQLError for engine internals) but the duplicate variants are awkward and lead to lossy `From`-conversions in handlers.
- **Suggested approach:** Pick one. Either nest `FraiseQLError` as `RuntimeError::Core(FraiseQLError)` and delete the duplicates, or extract a shared `CommonError` enum that both expose via `From`. Document the chosen direction so future variant adds don't drift.
- **Verification:** Search for `FraiseQLError::* => RuntimeError::*` conversions; ensure all lossy ones become forwarding.
- **Risk:** Public API change across runtime crates.
- **Confidence:** Medium
- **Status:** Closed — fixed in 230d4d238..788320393 (7 commits on `feat/error-taxonomy-consolidation`). `RuntimeError` deleted; `FraiseQLError::{Auth, Webhook, Observer, File}` added with subsystem-owned `From` impls. Follow-up issues from the new shape are tracked as F049, F050, F051, F052, F054, F055.

#### F025 — Federation HTTP errors format the source into the message and drop the chain
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** When the federation HTTP resolver fails, the error returned is `FraiseQLError::Internal { message: format!("HTTP resolution failed after {} attempts: {}", attempts, last_error), source: None }`. The original `reqwest::Error` (with HTTP status, URL, redirect history) is lost.
- **Location:** `crates/fraiseql-federation/src/http_resolver.rs:460-467`.
- **Finding:** Last error is a `String` accumulated via `format!`; the actual `reqwest::Error` is discarded earlier (`Err(e) => last_error = Some(format!(...))` at `:447`).
- **Suggested approach:** Keep the last `reqwest::Error` (boxed) and attach via `source: Some(Box::new(last_err))`. Then `tracing` event handlers can walk the chain.
- **Risk:** Tiny — the error message in logs may change shape if downstream parsers exist.
- **Confidence:** High
- **Status:** Closed in 500859a48 — `execute_with_retry` keeps the most recent transport error as `Box<dyn std::error::Error + Send + Sync>` alongside the summary string and threads it into `FraiseQLError::Internal { source: Some(...) }`. Non-success HTTP responses leave `source` cleared (the status is already in the summary; there is no `reqwest::Error` to attach).

### Async patterns

#### F021 — `tokio::spawn` fire-and-forget without `JoinHandle` tracking in lifecycle paths
- **Status:** Closed in 19bfd826c — added `tasks: tokio::task::JoinSet<()>` to `Server<A>`. Threaded a `&mut JoinSet<()>` through `trusted_docs_from_schema`/`spawn_trusted_docs_reload` and extracted a shared `spawn_pkce_cleanup` helper used by both constructor paths. `serve_with_shutdown` adds the SIGUSR1 handler, usage-persistence flush, and Arrow Flight gRPC server spawns onto the same set; after `axum::serve` returns a module-level `drain_lifecycle_tasks` aborts and awaits every task under the configured shutdown timeout. Per-request spawns (subscription event handlers, request middleware) are NOT migrated — they are not lifecycle tasks. New regression tests in `server/tests.rs` exercise the abort+drain path.
- **Severity:** 🟠 High
- **Effort:** M
- **Impact:** Server shutdown cannot await background tasks; if a task is still running during graceful shutdown it is dropped mid-flight (which for tokio tasks means cancellation, but for `mpsc` consumers may mean lost messages).
- **Location:** `crates/fraiseql-server/src/server/lifecycle.rs:83, :127, :284`; `crates/fraiseql-server/src/server/initialization.rs:380`; `crates/fraiseql-server/src/server/extensions.rs:298`; `crates/fraiseql-server/src/server/builder.rs:377`.
- **Finding:** Several spawns inside the server bootstrap have no associated `JoinHandle` tracked in the server struct.
- **Suggested approach:** Collect handles into a `Vec<JoinHandle<()>>` on the `Server` struct (or use `tokio::task::JoinSet`); awaited (with timeout) during graceful shutdown.
- **Verification:** Add an integration test that triggers shutdown mid-job and asserts the task has a chance to clean up (e.g., flushes its mpsc).
- **Risk:** Shutdown logic — easy to introduce hangs; bound every join by a timeout.
- **Confidence:** Medium

#### F026 — `async_trait` baseline at 180 is high; several traits could now use return-position `impl Trait`
- **Status:** Closed without action — Q2 policy froze the `async_trait` baseline at 180; pre-existing decision is "wait for RTN-in-`dyn` (RFC 3425) stabilisation". Do not re-flag.
- **Severity:** 🟢 Low
- **Effort:** L
- **Impact:** Each `#[async_trait]` boxes returned futures (`Pin<Box<dyn Future + Send>>`), one allocation per call. With stable RPITIT (1.75+) and `trait-variant` (already a dep of fraiseql-functions), some single-impl traits could drop the macro.
- **Location:** Baseline tracked in `Makefile:286` (180). Audit candidates: every trait without `dyn` callers.
- **Finding:** The codebase has not been swept for traits used only via generics (no `Box<dyn Trait>` or `Arc<dyn Trait>` callers). Those can switch to native `async fn` in trait with `impl Future + Send` returns.
- **Suggested approach:** Run `grep -r "Box<dyn .*\bTrait\b\|Arc<dyn .*\bTrait\b" crates/` and pair each `#[async_trait]` against actual dyn-usage. Drop the macro from traits with no dyn callers.
- **Risk:** Some traits look generic-only but pop into dyn usage via test mocks; check tests too.
- **Confidence:** Medium

### Memory / concurrency

#### F007 — `TrustedDocumentStore` resolves on the request hot path under tokio RwLock
- **Severity:** 🟠 High
- **Effort:** S
- **Impact:** Every request with a document ID acquires `tokio::sync::RwLock` read and then clones the entire query body (`docs.get(hash).cloned()`). Hot-reload is the only writer.
- **Location:** `crates/fraiseql-server/src/trusted_documents.rs:53` (`Arc<RwLock<HashMap<String, String>>>`), `:150-168` (`resolve`).
- **Finding:** Read-mostly map with rare writes is the textbook `arc-swap` use case. The query body cloning is also gratuitous — storing `Arc<str>` values would let the resolver return an `Arc<str>` clone.
- **Suggested approach:** `documents: Arc<ArcSwap<HashMap<String, Arc<str>>>>`; `replace_documents` swaps. `resolve` returns `Arc<str>`. Lock-free on the read path.
- **Verification:** Concurrent bench with 1 000 simultaneous resolves of distinct documents while another task replaces the map.
- **Risk:** Public return type changes from `String` to `Arc<str>` — caller surface in `handler.rs:325` (`request.query = Some(resolved)`) needs to accept `Arc<str>` or `into_owned()`.
- **Confidence:** High
- **Status:** Closed in 4b3e542b3 — `documents` switched to `Arc<DashMap<String, String>>`. The resolve critical section is purely synchronous, so `resolve`, `document_count`, and `replace_documents` dropped their `async fn` signatures entirely (no `.await` suspend point on the hot path). The `Arc<str>` zero-copy optimisation deferred — not necessary to unblock the lock-free goal and would have rippled into `handler.rs` value assignment. 9 unit tests converted from `#[tokio::test] async fn` to `#[test] fn`; production callers in `handler.rs:318` and `initialization.rs:415` dropped their `.await`.

#### F010 — `AuthRequest` derives `Debug` and stores raw `Authorization` header
- **Severity:** 🔴 Critical
- **Effort:** XS
- **Impact:** A `tracing::debug!(?req, ...)` anywhere on the auth path would log the bearer token to structured logs.
- **Location:** `crates/fraiseql-core/src/security/auth_middleware/types.rs:88` (`#[derive(Debug, Clone)]`), `:91` (`pub authorization_header: Option<String>`).
- **Finding:** The struct is `Debug + Clone` and the field is `pub`. No custom `Debug` impl redacts the header. Grep does not currently show a `debug!(?req)` log, but the foot-gun is loaded.
- **Suggested approach:** Implement `Debug` manually: print `Some("Bearer ***")` / `None` based on whether the header is present. Alternatively wrap in `Secret<String>` from `fraiseql-secrets`.
- **Verification:** Add a unit test asserting that `format!("{:?}", AuthRequest{ authorization_header: Some("Bearer abc".into()) })` does not contain "abc".
- **Risk:** None — purely additive safety.
- **Confidence:** High
- **Status:** Closed in 1dbf83119 — `derive(Debug)` removed, manual impl emits `Some("<redacted>")`/`None`, regression tests added in `auth_middleware/tests.rs` (`test_auth_request_debug_redacts_bearer_token`, `test_auth_request_debug_with_no_header_shows_none`).

#### F012 — `Secret::Drop` does not zeroize
- **Severity:** 🟠 High
- **Effort:** XS
- **Impact:** The `Secret` wrapper redacts Debug/Display but the underlying `String` is freed without scrubbing. Memory-dump or use-after-free reads can recover the plaintext.
- **Location:** `crates/fraiseql-secrets/src/secrets_manager/types.rs:60-120` — no `Drop` impl.
- **Finding:** The crate already pulls in `zeroize` (`Cargo.toml:35`) and uses `Zeroizing` for buffers in `vault/cache.rs` and `vault/backend.rs`. `Secret` itself doesn't.
- **Suggested approach:** `impl Drop for Secret { fn drop(&mut self) { self.0.zeroize(); } }`. Or switch the inner field to `Zeroizing<String>`. Note: `String::zeroize` only zeroes the heap allocation; capacity may differ from length, but that's still better than today.
- **Verification:** A test that calls `expose()`, drops the Secret, and reads the freed page would be unreliable; cover via inspection of the Drop impl in unit tests (it ran without panicking).
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in eda6db593 — `Drop` impl added using safe `mem::take + into_bytes + Zeroize` pattern (preserves `#![forbid(unsafe_code)]`). `into_exposed` adapted to use `mem::take` to coexist with the Drop. 4 regression tests added covering normal, empty, post-clone, and into_exposed paths.

#### F027 — `OnceLock<Regex>` wrapped in `fn ` instead of `static LazyLock<Regex>`
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** Minor consistency — the rest of the codebase uses `LazyLock<Regex>` (see `validation/rich_scalars.rs:13`). One outlier uses the older `fn uuid_regex() -> &'static Regex { static REGEX: OnceLock<Regex>; ... }` shape.
- **Location:** `crates/fraiseql-core/src/cache/uuid_extractor.rs:56-62`.
- **Suggested approach:** Replace with `static UUID_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(...).expect("UUID regex is valid"));` and reference `&UUID_REGEX` at call sites.
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in ccd25ee97 — replaced the `fn uuid_regex()` + `OnceLock<Regex>` shape with `static UUID_REGEX: LazyLock<Regex>`. Single call site (`UUIDExtractor::is_valid_uuid`) updated.

### Type system / generics

#### F028 — Newtype `ViewName(Arc<str>)` would prevent String/&str confusion at view-name boundaries
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** View names flow through ~40 functions as `&str`, `String`, `&String`, `&[String]`, `Vec<String>`, `Box<[String]>` — every combination. Introducing a `ViewName` newtype would catch crossed wires at compile time and pair nicely with F037 (view interner).
- **Location:** Pervasive — `crates/fraiseql-core/src/cache/result.rs:65`, `crates/fraiseql-core/src/runtime/executor/runners/mutation/mod.rs:331`, `crates/fraiseql-db/src/postgres/adapter/mod.rs:632`, …
- **Finding:** The codebase consistently passes view names as raw strings, mixing borrow types. A `ViewName(Arc<str>)` (or `&ViewName`) wrapper would also let `Display` enforce identifier quoting/escaping rules.
- **Suggested approach:** Introduce `ViewName` in `fraiseql-db`; migrate the cache layer first (F037 pairs naturally), then the SQL generator. Avoid a big-bang refactor.
- **Risk:** Wide blast radius; do in phases.
- **Confidence:** Medium

#### F029 — `JsonbValue` re-export blurs which side owns the JSON
- **Severity:** 🟢 Low
- **Effort:** S
- **Impact:** `JsonbValue` is the adapter-row return type but is also used as the projection input. Whether the projector borrows or owns is unclear from the signature `&[JsonbValue]`.
- **Location:** `crates/fraiseql-core/src/runtime/projection.rs:376`.
- **Finding:** Documentation gap rather than a bug.
- **Suggested approach:** Add a module-level doc explaining the ownership contract for `JsonbValue` and whether projection allocates.
- **Risk:** Docs-only.
- **Confidence:** Medium

### Testing

#### F030 — No fuzz target for the JSON validate path used in incoming variables
- **Severity:** 🟡 Medium
- **Effort:** M
- **Impact:** `crates/fraiseql-wire/src/json/validate/` has tests but no `cargo fuzz` target. Variables JSON crosses the security boundary on every request; structured fuzzing finds nesting/encoding edge cases that proptest misses.
- **Location:** `crates/fraiseql-wire/fuzz/fuzz_targets/` (only `protocol_decode.rs`, `scram_parse.rs`).
- **Suggested approach:** Add `fuzz_targets/json_validate.rs` driving `wire::json::validate::validate(...)` with arbitrary bytes and structured `Arbitrary` JSON values.
- **Risk:** None — additive.
- **Confidence:** High

#### F031 — Property tests cover schema and cache but not the runtime executor flow
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** End-to-end property tests would catch logic bugs in the WHERE-clause composition (RLS + inject + user-where) that unit tests miss.
- **Location:** `crates/fraiseql-core/tests/property/` (5 files: schema, sql_generation, cache_invalidation, error_handling/sanitization, graphql).
- **Suggested approach:** Add `tests/property/property_executor.rs` that generates compiled schema + query + variables and asserts the executor never panics and never returns RLS-bypassing rows for a stub adapter.
- **Risk:** Property tests can be flaky; budget for shrinking time.
- **Confidence:** Medium

### Documentation

#### F032 — Crate-level READMEs missing on most crates
- **Status:** Re-prioritized — see round-2 update (industrial: published-to-crates.io defect, bundle into docs PR after F049-F052 land).
- **Severity:** 🟠 High
- **Effort:** M
- **Impact:** `crates.io` and `docs.rs` landing pages are bare. Customers landing on `fraiseql-functions` etc. have no overview before clicking into the rustdoc.
- **Location:** `crates/*/README.md` — none of the 16 crates has one.
- **Suggested approach:** Each crate gets a 30-line README from the top of its `lib.rs //!` doc with a one-paragraph description, feature flags table, and a single example.
- **Risk:** None.
- **Confidence:** High

### Dependencies

#### F015 — `redis = "1"` duplicated across 4 crates instead of `[workspace.dependencies]`
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** Future Redis version bumps require touching four Cargo.toml files. Feature-flag drift is the actual risk (auth pulls `connection-manager`, observers may not, etc. — verified each currently lists "aio, tokio-comp, connection-manager" but nothing enforces it).
- **Location:** `crates/fraiseql-auth/Cargo.toml:17`, `crates/fraiseql-core/Cargo.toml:79`, `crates/fraiseql-observers/Cargo.toml:19`, `crates/fraiseql-server/Cargo.toml:82`.
- **Suggested approach:** Add `redis = { version = "1", features = ["aio", "tokio-comp", "connection-manager"] }` to `[workspace.dependencies]`; replace the four declarations with `redis = { workspace = true, optional = true }`.
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in 8278defdc — added workspace declaration, switched all 4 consumers to `redis = { workspace = true, optional = true }`. All 4 used identical features so no per-crate override needed.

#### F033 — `chrono`, `dashmap`, `uuid`, `url`, `axum` declared as raw deps in many crates
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** Same as F015. `chrono` declared 11 times, `dashmap = "6.1"` declared 4 times outside the workspace block. `dashmap = "6.0"` is in `[workspace.dependencies]` but per-crate overrides pin to `"6.1"` — actual version skew exists right now (resolver picks 6.1).
- **Location:** `crates/fraiseql-arrow/Cargo.toml:22, :18`; `crates/fraiseql-auth/Cargo.toml:10`; `crates/fraiseql-server/Cargo.toml:37`; `crates/fraiseql-observers/Cargo.toml:9`.
- **Suggested approach:** Bump workspace `dashmap` to `"6.1"`; switch all crates to `dashmap = { workspace = true }`. Repeat for `chrono`, `uuid`, `url`.
- **Risk:** None — semver-compatible.
- **Confidence:** High
- **Status:** Closed in a0e37c15d — bumped workspace `dashmap` 6.0 → 6.1, added `url = "2"` to workspace, switched all per-crate decls of `chrono`, `dashmap`, `uuid`, `url` to `workspace = true`. `axum` was already using `workspace = true` everywhere (the finding was outdated on that one). No per-crate `features = [...]` overrides were needed: workspace `uuid` declares `["v4", "serde"]` which is a superset of every per-crate usage; `chrono` workspace declares `["serde"]` which matches every per-crate usage; `dashmap`/`url` declare no extra features.

#### F034 — `fraiseql-functions` `reqwest` declaration drops `rustls-tls` workspace settings
- **Severity:** 🟠 High
- **Effort:** XS
- **Impact:** Workspace `reqwest` is `default-features = false, features = ["json", "rustls-tls"]`. The functions crate declares `reqwest = { version = "0.12", optional = true }` — falls back to default features which pull in `native-tls`/OpenSSL on Linux. Defeats the workspace's explicit rustls-only policy.
- **Location:** `crates/fraiseql-functions/Cargo.toml:30`.
- **Suggested approach:** `reqwest = { workspace = true, optional = true }`.
- **Verification:** `cargo tree -p fraiseql-functions --features host-live` should show no `native-tls`/`openssl-sys` after the change.
- **Risk:** If anything in host-live needs a feature the workspace doesn't enable, this surfaces it.
- **Confidence:** High
- **Status:** Closed in 23d4a18ea — switched to `reqwest = { workspace = true, optional = true }`. `cargo tree -p fraiseql-functions --features host-live | grep -E "(native-tls|openssl-sys)"` returns no hits. Lock file diff confirmed native-tls/openssl-sys removed. Also dropped a redundant `features = ["serde"]` override on `chrono` in the same crate.

### Build & tooling

#### F022 — `mold` linker block commented out in `.cargo/config.toml`
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** 3-5× faster local link times. The block is already in place, just commented out.
- **Location:** `.cargo/config.toml:50-56`.
- **Finding:** The author memory notes mold is installed locally (Arch package mentioned in CLAUDE.md). The CI-compat concern is real but solvable via `[target.'cfg(all(target_os = "linux", not(env = "CI")))']` or a developer-opt-in `.cargo/config.local.toml` (and an entry in `.gitignore`).
- **Suggested approach:** Provide `.cargo/config.toml.local.example` that enables mold; document in CONTRIBUTING that local developers should copy it. Alternative: split into `[target.x86_64-unknown-linux-gnu]` with a conditional based on `MOLD_AVAILABLE` env var checked in `build.rs`.
- **Risk:** None if developer-opt-in.
- **Confidence:** High
- **Status:** Closed in 598231ae4 — added `.cargo/config.linker.example.toml` template documenting the opt-in. The block in `.cargo/config.toml` stays commented to preserve CI compatibility (the inline comment block now lists `pacman -S mold` / `apt install mold` and points at the example file). Mold is installed locally but uncommenting in the committed config would break GitHub Actions.

#### F035 — No `cargo ci` alias for the standard lint+test combo
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** Reduces "what do I run before pushing" friction.
- **Location:** `.cargo/config.toml:6-30` (existing aliases).
- **Suggested approach:** `ci = "fmt -- --check && clippy --all-targets --all-features -- -D warnings && nextest run --all-features"` (note: aliases can't chain `&&`; provide a `make ci` target or a script).
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in d04068d34 — added `cargo ci` alias for the strict workspace clippy gate (`clippy --workspace --all-targets --all-features -- -D warnings`) and a `make ci` target chaining clippy + `nextest run --workspace --all-features`. Cargo aliases cannot chain commands; the Makefile carries the full combo.

### Database / SQL codegen

#### F038 — `build_where_select_sql_ordered` rebuilds the SQL string per request
- **Severity:** 🟡 Medium
- **Effort:** L
- **Impact:** Every query call builds `format!("SELECT data FROM {}", ...)` and appends WHERE/ORDER/LIMIT/OFFSET fresh. For a hot view queried 1 000 RPS, that's 1 000 String allocations per second on a path the prepared-statement cache should make trivial.
- **Location:** `crates/fraiseql-db/src/postgres/adapter/mod.rs:624-665`.
- **Finding:** The SQL shape varies by (view, has_where, has_order_by, has_limit, has_offset). For a given (view, has_*) signature, the SQL string is constant modulo parameter placeholders. A small LRU keyed on this signature plus the schema generation would avoid the per-request rebuild.
- **Suggested approach:** Add a `SqlTemplateCache: DashMap<(ViewName, AutoParamShape), Arc<str>>`. WHERE clause SQL still varies with filter shape; cache that on `WhereClause` content hash (the param values get bound separately).
- **Verification:** Bench `execute_with_projection_arc` at high RPS before/after.
- **Risk:** Cache invalidation on schema reload (already a known synchronisation point).
- **Confidence:** Medium

#### F043 — `execute_with_projection_arc` trait method is called from multiple paths but signature is wide
- **Severity:** 🟢 Low
- **Effort:** S
- **Impact:** 7 positional arguments. Easy to mis-order.
- **Location:** `crates/fraiseql-db/src/traits.rs:420`.
- **Suggested approach:** Introduce a `ProjectionExecutionParams<'a>` struct that the adapter accepts. Same call shape but compile-time guarantees on parameter order.
- **Risk:** Trait change — all adapters update.
- **Confidence:** High

### GraphQL / wire protocol

#### F039 — `graphql_parser::parse_query::<String>` allocates strings for every identifier
- **Severity:** 🟡 Medium
- **Effort:** M
- **Impact:** `parse_query::<String>` requests owned strings from the parser. Using `parse_query::<&str>` would let the AST borrow from the query bytes — substantially fewer allocations.
- **Location:** `crates/fraiseql-core/src/graphql/parser.rs:62`, `crates/fraiseql-core/src/graphql/complexity.rs:209`.
- **Finding:** Both parser entry points pick the `String` parameterisation. The cost compounds — every field name, type ref, variable name is its own allocation.
- **Suggested approach:** Migrate to `parse_query::<&'a str>`, with `'a` tied to the request. Likely needs `ParsedQuery<'a>` (pairs with F042).
- **Verification:** DHAT bench of `parse_query` against a 1 KB query with many fields.
- **Risk:** Lifetime propagates through `QueryMatch`, `ResultProjector`, etc.
- **Confidence:** Medium

### Observability

#### F040 — `tracing` spans missing on the cache hit/miss path
- **Severity:** 🟢 Low
- **Effort:** S
- **Impact:** Operators cannot tell from logs whether a slow request was caused by a cache miss or by DB latency.
- **Location:** `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:102-108` (response-cache lookup); `crates/fraiseql-core/src/cache/response_cache.rs:get` (no span).
- **Suggested approach:** Add `#[tracing::instrument(skip_all, fields(cache.hit = ...))]` on the cache lookup, or emit `tracing::debug!` events labelled `cache.event = "hit"|"miss"|"disabled"`.
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in ec9015e26 — `debug!` events emit on the lookup path with structured fields `event` (`hit`/`miss`/`disabled`), `query`, `query_key`, `sec_hash`. Target `fraiseql::cache::response` so operators can isolate them in tracing filters. Miss event fires before the plan/projection work so it timestamps the start of the slow path.

#### F041 — `info!` log per query execution may be excessive
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** Every successful GraphQL request emits an `info!("Executing GraphQL query", ...)` event. At 1 000 RPS with default INFO log level, this is the noisy floor of the logs.
- **Location:** `crates/fraiseql-server/src/routes/graphql/handler.rs:368-373`.
- **Finding:** Most other request frameworks log this at `debug!` and reserve `info!` for warnings/state changes.
- **Suggested approach:** Move to `debug!`; replace with a single `info!` covering startup, shutdown, and schema reload only.
- **Risk:** Operators relying on `info!`-level GraphQL request logs need to bump filter.
- **Confidence:** High
- **Status:** Closed in ef8bc4119 — per-query "Executing GraphQL query" event demoted from `info!` to `debug!`. Unused `info` import dropped. Inline comment documents the reservation policy.

### Security

#### F010, F012 covered above.

#### F044 — `serde_json::to_string(...).unwrap_or_default()` on response-cache key derivation
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** `unwrap_or_default()` substitutes empty string on serialization error, silently producing a different cache key than intended. In theory two distinct argument trees could hash to the same key after one fails to serialize.
- **Location:** `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:341`.
- **Finding:** `serde_json::to_string(&Value)` only fails on a non-string map key (or out-of-memory). The latter is genuine; the silent collapse is wrong even then.
- **Suggested approach:** Hash the JSON bytes directly via `to_writer`, propagating the error or asserting infallibility for `serde_json::Value` -> writer.
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in cf3a202cd (+ clippy follow-up f47445b3d). `compute_response_cache_key` now returns `Result<u64>` and streams each argument via `serde_json::to_writer` into a reused `Vec<u8>` scratch buffer (no intermediate `String`). Failures surface as `FraiseQLError::Validation { path: Some(format!("arguments.{key}")), .. }`. Call site in `execute_regular_query_with_security` threads the error out via `?`.

### FraiseQL-specific (compiler, cache, federation, observers, functions)

#### F045 — Per-trigger `AuthCallbackResponse`/`AuthRefreshResponse` derives `Debug` with token fields
- **Severity:** 🟠 High
- **Effort:** XS
- **Impact:** Same shape as F010 — a stray `?response` log leaks the access/refresh token.
- **Location:** `crates/fraiseql-auth/src/handlers.rs:68-79` (`AuthCallbackResponse`), `:106-115` (`AuthRefreshResponse`), `:117-122` (`AuthLogoutRequest`).
- **Suggested approach:** Manual `Debug` redacting `access_token` / `refresh_token` fields. Or wrap in `Secret<String>`.
- **Risk:** None.
- **Confidence:** High
- **Status:** Partially closed in 47c478768 — `AuthCallbackResponse` and `AuthRefreshResponse` now have manual `Debug` impls redacting token fields, with 3 regression tests in `handlers::debug_redaction_tests`. **AuthLogoutRequest left for follow-up**: the original finding listed it but the Wave-1 scope (per task instructions) was the two response types only; the request type's `refresh_token: Option<String>` has the same shape and should be covered in a follow-up (tracked separately as the same class of fix). See also `AuthRefreshRequest` at `handlers.rs:122-127` which also derives Debug with a refresh_token field.

#### F046 — Federation `ConnectionManager` `Mutex<HashMap>` is uncached if `unstable` is off
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** `get_or_create_connection` is `#[cfg(feature = "unstable")]`. The `adapters` field exists unconditionally; without the feature, the lock is held only by `close_connection` — dead code in stable builds.
- **Location:** `crates/fraiseql-federation/src/connection_manager.rs:132, :192-193`.
- **Suggested approach:** Gate the field with `#[cfg(feature = "unstable")]` too, or delete the close logic in the non-unstable build.
- **Risk:** None.
- **Confidence:** High
- **Status:** Closed in 808b7cf47 — documentation fix. `get_or_create_connection` (the only writer) is genuinely WIP — it currently returns `FraiseQLError::Internal` with an "unstable API" message — so the gate is intentional. Added a "Feature gating" rustdoc section on `ConnectionManager` and a mirrored field comment explaining that the read-only surface (`new`, `close_connection`, `close_all`, `connection_count`) is kept ungated so downstream code can wire the manager into its own types without the `unstable` flag. No code change needed.

#### F047 — Cron scheduler logs swallow the wasm task result
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** A cron task that panics or errors inside the wasm runtime emits nothing visible at the scheduler level.
- **Location:** `crates/fraiseql-functions/src/triggers/cron.rs:422` (`tokio::spawn(async move { ... })`).
- **Finding:** Same pattern as F014; the spawned task's `Result` is dropped.
- **Suggested approach:** Wrap each cron invocation in `instrument_cron_task` that logs on Err / panic at warn level and increments a counter.
- **Risk:** Tiny.
- **Confidence:** High
- **Status:** Closed in 7f99fe498 (+ clippy follow-up f47445b3d). Cron-task error log now adds `error.debug` (full `Debug`) and `error.chain` (concatenated `std::error::Error::source()` walk) fields alongside the existing `error` (top-level `Display`). Added `error_source_chain` helper that joins causes with ` → `, falls back to `<no source>`. (The finding's claim that errors were entirely swallowed was off — the top-level Display *was* logged; the chain was not.)

#### F048 — `entity_type_index: Arc<RwLock<HashMap<(String, String), Vec<i64>>>>` is double-locked
- **Status:** Closed in 1ebae1f61 — `entity_type_index` switched to `Arc<DashMap<(String, String), Vec<i64>>>`. Inner `Vec<i64>` kept as plain `Vec` (no `parking_lot::Mutex`) because call-site audit (`rg "entity_type_index" crates/`) confirmed the only writers republish the whole map via `clear` + per-key `insert` in `start` and `reload_observers`; there is no per-key concurrent mutation. The two background-task reader paths drop the outer `RwLock::read().await` and call `.get(...).map(|r| r.value().clone())` directly.
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** Each observer dispatch acquires the outer RwLock and the inner Vec is rebuilt at every reload.
- **Location:** `crates/fraiseql-server/src/observers/runtime.rs:129`.
- **Suggested approach:** Hot-reload pattern: stuff the index into an `ArcSwap<HashMap<...>>` so the reload is a single atomic pointer swap and reads need no locking.
- **Risk:** Reload semantics — must publish atomically; ensure no torn reads of executor/matcher pair.
- **Confidence:** Medium

---

---

## Round-2 new findings (F049–F055)

### Error handling (post-refactor)

#### F049 — `FraiseQLError::{Auth, Webhook, Observer}` boxed payloads drop `Error::source()` chain
- **Severity:** 🟠 High
- **Effort:** XS
- **Impact:** `tracing`, `anyhow::backtrace()`, `miette` and any other chain-walker calling `err.source()` on a `FraiseQLError::Auth(box)` value returns `None` instead of the underlying `fraiseql_auth::AuthError`. The subsystem-error chain (which may include a `reqwest::Error`, `jsonwebtoken::Error`, etc.) is **invisible** to structured logging — only the top-level `"Auth error: …"` Display string remains. The doc comment on the variant promises "preserves subsystem vocabulary via Display/source chain" — Display works; source does not.
- **Location:** `crates/fraiseql-error/src/core_error.rs:271-280` (the three tuple variants) vs. `:299-301` (the `Internal { #[source] source: Option<Box<...>> }` shape that does it correctly).
- **Finding:** `thiserror 2`'s `#[error("...")]` derive does *not* auto-detect a single tuple field as the source — it requires explicit `#[source]` or `#[from]`. The three new variants have neither attribute. The `Internal` variant in the same file uses `#[source]` on its `source` field and works correctly. The asymmetry is invisible to `cargo test` (no test asserts the source chain) but visible in production log payloads.
- **Suggested approach:** Annotate each variant: change `Auth(Box<dyn std::error::Error + Send + Sync>)` to `Auth(#[source] Box<dyn std::error::Error + Send + Sync>)`. Same for `Webhook` and `Observer`. Add a regression test in `crates/fraiseql-error/tests/http_responses.rs` that asserts `err.source().is_some()` and `err.source().unwrap().to_string() == inner.to_string()` for each variant.
- **Verification:** `let inner = WebhookError::Http("..."); let outer: FraiseQLError = inner.into(); assert_eq!(outer.source().unwrap().to_string(), "..."); ` should pass.
- **Risk:** None — purely additive; no signature change.
- **Confidence:** High
- **Status:** Closed in bc0ed8e25 — `#[source]` added to `Auth`, `Webhook`, `Observer` tuple payloads; 3 regression tests added in `tests/http_responses.rs` (`auth_variant_preserves_source_chain`, `webhook_variant_preserves_source_chain`, `observer_variant_preserves_source_chain`). The Auth variant also gained the downcast_ref recovery-pattern rustdoc that closes F052.

#### F050 — `FraiseQLError::Storage` and `FraiseQLError::File` carve the file domain across two variants with divergent HTTP codes
- **Severity:** 🟠 High
- **Effort:** S — revised to **M** after audit found 118 sites, not 60.
- **Impact:** Storage operations in the `fraiseql-storage` crate produce `FraiseQLError::Storage` (HTTP 500, "storage_error"). Storage operations in `fraiseql-server/src/storage/*` produce `FileError::Storage` → `FraiseQLError::File` (HTTP 400, "file_error"). Identical-named-but-distinct enums (`FraiseQLError::Storage` vs `FileError::Storage`) and identical-conceptual-domain split across two `FraiseQLError` variants. A frontend cannot distinguish "user uploaded a file that failed validation" from "the backend tried to download a managed file and the bucket was unreachable" without backend-internal knowledge of which crate owns the call.
- **Location:** 
  - `FraiseQLError::Storage` definition: `crates/fraiseql-error/src/core_error.rs:234-241`
  - HTTP code mapping: `crates/fraiseql-error/src/core_error.rs:465 (500)` vs `:455 (400 for File)`
  - HTTP shape: `crates/fraiseql-error/src/http.rs:132-134` ("storage_error") vs `:120 file_error_response`
  - 60+ `FraiseQLError::Storage` construction sites in `crates/fraiseql-storage/src/{backend,service,routes,metadata}/**.rs` — **actual count: 115** (re-audit).
  - 30+ `FileError::Storage` construction sites in `crates/fraiseql-server/src/storage/**.rs` — **actual: 0 in `fraiseql-server`, the storage code lives in `fraiseql-storage`**. (Round-2 found 1 site in `fraiseql-functions/src/host/live/storage.rs` and 1 in `fraiseql-error/src/http.rs`.)
- **Finding:** Round-1 F017's `From<FileError> for FraiseQLError` lossy conversion was correctly identified as a problem; the round-2 refactor deleted that conversion (`refactor(error)!: ffd3124e9`) and replaced it with the cleaner `File(#[from] FileError)` variant. But the refactor left `FraiseQLError::Storage` in place, untouched. It is now a vestigial parallel to `File`: both encode "the file-storage subsystem failed", but with different HTTP codes (500 vs 400) and different error categories ("storage_error" vs "file_error"). The split is invisible to API consumers and undocumented in the variant rustdoc. **Round-2 verification also found** the `code: Option<String>` field carries stable routing strings (`"not_found"`, `"permission_denied"`, etc.) used by `storage_error_response` for HTTP routing — migrating to `FileError` requires preserving this semantically (either by extending `FileError` or by giving up the typed routing).
- **Suggested approach:** Two options under industrial framing:
  - **(A) Collapse:** Migrate `fraiseql-storage`'s 60+ call sites to `FileError::Storage { message }.into()` (which becomes `FraiseQLError::File`). Delete `FraiseQLError::Storage`. The HTTP code unifies at 400 (the storage-layer caller is responsible for retry / circuit breaking, not the user). Breaking change accepted under policy.
  - **(B) Document the split:** Add explicit rustdoc to both variants explaining when each is used (e.g. "Storage: backend infrastructure failures, returns 500 because the user cannot fix them; File: user-input failures, returns 400 because the user can change the upload"). Add a doc-test enforcing the convention. Lower-cost but leaves the dual surface.
  - Industrial recommendation: **(A)**. The dual surface is the source of bugs, not the source of value.
- **Verification:** After (A), `grep -rn "FraiseQLError::Storage" crates/ --include="*.rs"` returns zero hits.
- **Risk:** 60+ call sites need updating; mechanical.
- **Confidence:** High
- **Status:** Closed in 4c86d2e0d..cedf7d927 (7 commits on `feat/error-taxonomy-consolidation`, Wave 4). `FraiseQLError::Storage` deleted; 118 call sites migrated to `FraiseQLError::File(FileError::*)` via eight new typed backend variants (`PermissionDenied`, `IoError`, `InvalidKey`, `NotImplemented`, `Unsupported`, `SizeLimitExceeded`, `MimeTypeNotAllowed`, `Backend`) plus the pre-existing `NotFound`. `storage_error_response` now pattern-matches on typed variants (404 for `NotFound`, 403 for `PermissionDenied`, 500 elsewhere) instead of `code: Option<String>` strings. Source chains (reqwest, AWS SDK, sqlx, std::io) preserved via `source: Some(Box::new(e))` on every previously-stringified site. Only deliberate behavior change: `FraiseQLError::File(FileError::NotFound)` returns 404 globally (was 400 outside storage routes) — see CHANGELOG.

#### F051 — `FraiseQLError::Storage` variant has no documented owner after the file/storage split
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** The round-2 refactor added explicit rustdoc to `FraiseQLError::{Auth, Webhook, Observer, File}` explaining ownership (subsystem crate vs. fraiseql-error). `FraiseQLError::Storage` (lines 234-241) has the unchanged minimal doc `/// Storage operation error.` with no statement of (a) which crate constructs it, (b) what relationship it has with the new `File` variant, (c) when callers should use one vs. the other. After F050 lands, this entire variant goes away; if F050 is deferred or rejected, this doc gap blocks any new contributor from picking the right variant.
- **Location:** `crates/fraiseql-error/src/core_error.rs:234-241`.
- **Suggested approach:** Either resolve via F050 (variant deleted), or add ownership rustdoc matching the style of `Auth`/`Webhook`/`Observer`/`File`. Sample:
  ```
  /// A backend-storage infrastructure failure (bucket unreachable, presigned-URL
  /// signing failed, GCS auth refresh failed). Distinct from [`Self::File`],
  /// which carries user-facing file-validation errors (size, MIME, virus scan).
  ///
  /// Constructed by `fraiseql-storage` and `fraiseql-functions/host/live/storage.rs`.
  /// Returns HTTP 500 because callers should retry, not the end user.
  ```
- **Risk:** None — docs-only.
- **Confidence:** High
- **Status:** Closed in 686322bd6 via option (B) — variant rustdoc upgraded with full owner block (`fraiseql-storage` and `fraiseql-functions/host/live/storage.rs`), distinction from `File`, the `code` field's stable string discriminators enumerated, and a forward reference to `FOLLOW_UPS.md` F050 for the planned collapse.

#### F052 — `FraiseQLError::Auth(Box<dyn Error>)` widens type-erasure and prevents downstream matching on auth-error subclasses
- **Severity:** 🟡 Medium
- **Effort:** L (alternative shape) / S (documented limitation)
- **Impact:** Downstream code (an axum middleware, a `match` block in `handler.rs`) cannot match on specific auth-error subclasses like `fraiseql_auth::AuthError::TokenExpired` vs. `::Forbidden` vs. `::OidcDiscoveryFailed`. The information is preserved in `Display` but cannot be pattern-matched. The doc comment on the variant explicitly acknowledges the trade-off ("type-erased here") but does not surface that this is *the* cost of the sqlx pattern.
- **Location:** `crates/fraiseql-error/src/core_error.rs:264-280`, `crates/fraiseql-auth/src/error.rs:238-248`.
- **Finding:** The Q1 policy decision considered (and rejected) the alternative `Auth(#[from] fraiseql_auth::AuthError)` shape because it would introduce a reverse dependency `fraiseql-error → fraiseql-auth`. The chosen sqlx pattern (subsystem crate owns the `From`-impl, fraiseql-error holds a `Box<dyn Error>`) preserves the dependency direction but trades it for the type-erasure. Both are legitimate; the trade-off is real and currently undocumented.
- **Suggested approach:** Two paths:
  - **(A) Document the limitation explicitly.** Add a `# Pattern-matching` rustdoc section to each of `Auth`/`Webhook`/`Observer` explaining that downstream code must `match err.source().and_then(|s| s.downcast_ref::<fraiseql_auth::AuthError>())` to access subclass-level matching, and provide a code example. Lowest-effort.
  - **(B) Promote one or more boxed payloads to typed variants.** For `Auth` specifically (the only one with 26 variants that downstream code frequently wants to match on), revisit the dependency-direction question: introduce a tiny `fraiseql-error-core` crate holding only the engine variants, let `fraiseql-error` re-export those plus typed `Auth(fraiseql_auth::AuthError)`, and depend on `fraiseql-auth`. Higher-effort, removes the type erasure entirely.
- **Industrial recommendation:** **(A) now, revisit (B) if a real consumer needs subclass matching.** YAGNI applies — no current code calls `downcast_ref` on the boxed payload, so the type-erasure cost is hypothetical.
- **Verification:** For (A): doctest in the variant. For (B): `cargo semver-checks` passes; subsystem crates type-check.
- **Risk:** (A) docs-only. (B) workspace-wide dependency-graph restructure.
- **Confidence:** Medium
- **Status:** Closed in bc0ed8e25 via option (A) — `# Pattern-matching on the inner error` rustdoc section added to `FraiseQLError::Auth` with a complete `downcast_ref` example; `Webhook` and `Observer` rustdoc points to `Self::Auth` for the same recovery pattern.

### Build & tooling

#### F053 — Wire-crate Q3 recommendation: cast denylist + module/test relocation, retire the count gate
- **Severity:** 🟡 Medium
- **Effort:** S
- **Impact:** The current `fraiseql-wire/src/lib.rs:17-35` has 19 crate-level `#![allow(...)]` directives covering pedantic lints. The Q3 policy left "count-cap vs explicit denylist" open. A count cap of 20 catches "the file grew an unjustified 21st allow" but is silent on whether the *existing* 19 are still justified. An explicit denylist gives the same regression guard with finer granularity.
- **Location:** `crates/fraiseql-wire/src/lib.rs:17-35` (19 allows); Q3 open issue in `POLICY_DECISIONS.md:118`.
- **Finding:** Audit of the 19 allows shows three categories:
  - **Justified protocol-level (8):** `cast_precision_loss`, `cast_possible_truncation`, `cast_sign_loss`, `cast_possible_wrap`, `format_push_string`, `needless_continue`, `iter_with_drain`, `no_effect_underscore_binding`. These encode genuine binary-decoder / wire-protocol patterns. Keep as crate-level.
  - **Style-pref / API-shape (7):** `items_after_statements`, `match_same_arms`, `manual_let_else`, `needless_pass_by_value`, `implicit_hasher`, `doc_link_with_quotes`, `doc_markdown`. Defensible per-module; either move to specific `#[allow]` on the offending functions, or accept as crate-wide style with a rationale comment block. Industrial choice: keep crate-wide but consolidate the rationale into a single 5-line block above the allows.
  - **Test-bleed (4):** `unreadable_literal`, `map_unwrap_or`, `explicit_iter_loop`, `range_plus_one`. These are flagged in test code (assertions, example data, range expressions). Move to `#[cfg(test)] #![allow(...)]` blocks inside the relevant `mod tests` to scope the suppression to the actual locus.
- **Suggested approach:** 
  1. Move the 4 test-bleed allows into `mod tests { #![allow(...)] }` blocks where they fire.
  2. Group the 8 protocol allows under a single comment header (`// === Wire-protocol cast suppressions (binary decoders, statically bounded) ===`) and the 7 style allows under another (`// === Crate-wide style preferences (rationale: …) ===`).
  3. Reduce wire allow-count from 19 to 15 (8 + 7).
  4. Add a `lint-gate-wire` to `Makefile` set at 15 (current+0 slack, since count is post-refactor) **and** an explicit lint denylist in `crates/fraiseql-wire/Cargo.toml` `[lints.clippy]` that re-denies the test-bleed lints workspace-wide (catches regressions where they reappear outside `mod tests`).
- **Verification:** `cargo clippy -p fraiseql-wire --all-targets --all-features -- -D warnings` passes after the relocation; `make lint-gate-wire` enforces the count.
- **Risk:** None — refactor of allow scoping, no semantic change.
- **Confidence:** High

### Error handling (round-1 line shift)

#### F054 — `RateLimited` field renamed `retry_after_secs` (round-1 line refs invalid)
- **Severity:** 🟢 Low
- **Effort:** XS
- **Impact:** Round-1 IMPROVEMENTS.md and any external doc that referenced the field as `retry_after` is now wrong. The shape used to be `RateLimited { retry_after: Option<u64> }` but is now `RateLimited { retry_after_secs: u64 }` (no Option, renamed). The `ServiceUnavailable` variant still uses `retry_after: Option<u64>` — confusing asymmetry across two adjacent variants.
- **Location:** `crates/fraiseql-error/src/core_error.rs:198-203` (`RateLimited`) vs `:253-259` (`ServiceUnavailable`); `:602-609` (`rate_limited_with_retry` helper).
- **Finding:** Field naming asymmetry between two adjacent rate-limit-style variants is a documentation bug-magnet. New contributors will not realise the distinction.
- **Suggested approach:** Either rename `ServiceUnavailable::retry_after` → `retry_after_secs` for consistency (breaking change, OK under policy), or rename both consistently. Add rustdoc on both noting the convention.
- **Risk:** Breaking change.
- **Confidence:** High
- **Status:** Closed (doc-audit portion) via this commit. Verified `RateLimited { retry_after_secs: u64 }` is the canonical shape post-refactor and that no stale `retry_after` reference inside an active finding's discussion mis-states the field. The remaining asymmetry between `RateLimited::retry_after_secs` and `ServiceUnavailable::retry_after` is a real code defect tracked here for Wave 2 to harmonise (low priority — observable but not security-relevant).

#### F055 — `IntoResponse for FraiseQLError` exhaustive match on `#[non_exhaustive]` enum will silently break on next variant add
- **Severity:** 🟡 Medium
- **Effort:** XS
- **Impact:** `FraiseQLError` is `#[non_exhaustive]` (lines 99-100). The `IntoResponse for FraiseQLError` impl in `crates/fraiseql-error/src/http.rs:84-141` matches all 22 variants exhaustively without a `_` wildcard arm. *Within* the same crate this compiles (the `#[non_exhaustive]` attribute only affects downstream crates), but any future variant addition silently passes CI inside `fraiseql-error` itself and only fails downstream. More importantly, the `status_code()` (`:451-468`) and `error_code()` (`:483-491`) match arms have the same shape. Three exhaustive matches that all need to be updated in lockstep for every new variant.
- **Location:** `crates/fraiseql-error/src/http.rs:84-141`, `crates/fraiseql-error/src/core_error.rs:443-498`.
- **Finding:** The `#[non_exhaustive]` annotation was added in the A+ campaign (P01) but the three same-crate match impls negate its protection inside the defining crate. The risk surfaces every time a new variant lands: the dev adds the variant, the same-crate matches still compile, and the discovery happens only when a downstream crate fails. For internal logic (HTTP code, error category) this is OK; for the `IntoResponse` arm it's worse because a forgotten arm could return the wrong HTTP status to clients.
- **Suggested approach:** Either:
  - **(A)** Add `_ => ("internal_error", "An internal error occurred".to_string(), None)` at the bottom of the `into_response` match, with a `// SECURITY: unmatched variant defaults to generic 500` comment. Same treatment for `status_code()` and `error_code()`. Tradeoff: a new variant silently maps to 500 until explicitly added.
  - **(B)** Add a `#[deny(non_exhaustive_omitted_patterns)]` attribute (currently nightly-only lint, but tracked at rust-lang/rust#89554). Industrial-future; not actionable today.
  - **(C)** Add a unit test that constructs every variant via reflection and asserts `into_response` does not panic — impossible in safe Rust without proc-macro reflection. Skip.
- **Industrial recommendation:** **(A) now.** Document the security convention so future variants don't trip the generic fallback by accident.
- **Verification:** Add a `#[test]` that constructs a variant via `FraiseQLError::internal` and asserts the response is 500; add the same for each catch-all-eligible category.
- **Risk:** Risk is *adding* the wildcard — it now hides forgotten variants. Document the convention clearly.
- **Confidence:** High
- **Status:** Closed in 39078b202 via option (A) — catch-all arm added to all three matches (`into_response`, `status_code`, `error_code`), each with `#[allow(clippy::match_same_arms, unreachable_patterns)]` + `// Reason:` explaining the in-crate vs downstream distinction and the security guarantee. Round-1 `Network` doctest reference (lines 78-90 of the old doctest) was removed as part of the F016 fix in bc9df7dc2 — the round-2 audit references to line 78 (`FraiseQLError::Network`) are obsolete after the F016 fix.

---

## Categories — incomplete (sampled, not exhaustively audited)

- **Build & tooling — sampled 4 of ~12 candidate areas.** CI workflow gaps, `cargo doc --no-deps` clean-state, missing `cargo bench --workspace` recipe not covered.
- **GraphQL / wire protocol — sampled 3 of ~10 areas.** APQ hash derivation, subscription multiplexing, SCRAM hot-path not deep-audited.
- **FraiseQL-specific — sampled 4 of ~8 areas.** Federation HTTP resolver pool reuse, observers job queue back-pressure semantics, storage upload streaming not deep-audited.
