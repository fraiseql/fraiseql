# FraiseQL Improvement Backlog — Round 3 (Targeted Audit)

Generated: 2026-05-25
Branch: `feat/error-taxonomy-consolidation`
Scope: diff `dev..HEAD` (92 commits, 6,292 insertions / 3,462 deletions across 434 files)
Mode: targeted spot-check of round-1+2 closures + 4 cross-cutting themes (NOT a full re-audit)

## Severity legend

- 🔴 Critical — correctness/security regression, blocks release
- 🟠 High    — functional regression or substantive doc/test gap
- 🟡 Medium  — technical debt, ergonomic friction
- 🟢 Low     — nit, polish, doc gap

## Effort: XS (<30 min) / S (1–2 h) / M (½–1 day) / L (2–5 days) / XL (>1 week)

---

## Verified closed (PASS 1 spot-checks)

Each below was spot-checked at the cited file:line and confirmed real.

- **F001** double-parse — `parse_graphql_document` exposed, `validate_query_doc` accepts AST. `crates/fraiseql-core/src/graphql/complexity.rs`.
- **F002** Arc::unwrap_or_clone — `crates/fraiseql-core/src/runtime/executor/runners/query_regular.rs:106-110`.
- **F003** CompiledPattern — `crates/fraiseql-core/src/runtime/input_validator.rs` (no per-call `Regex::new`).
- **F005/F024** single variables map — `crates/fraiseql-core/src/runtime/matcher.rs:144-256`.
- **F006** KeyedRateLimiter DashMap — `crates/fraiseql-auth/src/rate_limiting.rs:207`, atomic via `Entry` (verified).
- **F007** TrustedDocumentStore DashMap + sync resolve — `crates/fraiseql-server/src/trusted_documents.rs:58`.
- **F008** in-memory bucket maps — `crates/fraiseql-server/src/middleware/rate_limit/in_memory.rs:24-32`, all 4 maps converted.
- **F009** flattened atomics — `crates/fraiseql-server/src/metrics_server.rs:36-54` (bare `AtomicU64`, 28 fields).
- **F010/F012/F045** Debug-redaction + zeroize — confirmed.
- **F013** federation ConnectionManager DashMap — `crates/fraiseql-federation/src/connection_manager.rs:160`.
- **F014/F025/F047** error source-chain propagation — confirmed.
- **F016** doctest fixed — `crates/fraiseql-error/src/core_error.rs:73-100`, 3 real variants.
- **F018** Clock generic + blanket `Fn`-impl — `crates/fraiseql-auth/src/rate_limiting.rs:38-50`; tests at `tests/rate_limiter_time_tests.rs:80-134` exercise both fixed (`u64::MAX`) and advance-by (`AtomicU64`) patterns.
- **F019** parking_lot::Mutex — `crates/fraiseql-core/src/apq/memory_storage.rs:37`.
- **F020** Iterator return — `crates/fraiseql-core/src/runtime/executor/support/pipeline.rs:77`.
- **F021** JoinSet drain — `crates/fraiseql-server/src/server/lifecycle.rs:441-469`; `abort_all` + bounded `join_next` loop under timeout — correctly awaits handles before return.
- **F023/F027** is_no_op extracted, LazyLock<Regex> — confirmed.
- **F028** ViewName end-to-end — `crates/fraiseql-db/src/traits.rs:696,730`, `crates/fraiseql-core/src/cache/result.rs:42,68,194,204`.
- **F031** 9 property tests landed — `crates/fraiseql-core/tests/property/property_executor.rs` (see F058 below for assertion-strength gaps).
- **F032** READMEs — spot-checked `fraiseql-storage` and `fraiseql-functions`: substantive (refers to `StorageBackend`, `FunctionRuntime`, real feature flags), not boilerplate.
- **F033/F034/F035** workspace deps, rustls posture, `cargo ci` — confirmed.
- **F036** `to_sql_param` deleted; `as_sql_param_refs` centralises borrow — confirmed.
- **F040/F041** cache hit/miss tracing, info→debug demotion — confirmed.
- **F042** `ParsedQuery.source: Arc<str>` — `crates/fraiseql-core/src/graphql/types.rs:44`.
- **F043** `ProjectionRequest` — `crates/fraiseql-db/src/traits/adapter_types.rs:193`. NOTE: brief's premise was inverted — struct is **intentionally NOT** `#[non_exhaustive]`. Construction sites use struct literals so missing-field is a hard compile error; design is deliberate (line 188 rustdoc). No regression.
- **F044** `compute_response_cache_key` Result + scratch buffer — confirmed.
- **F048** observer entity_type_index DashMap — confirmed; see F056 below for documented atomicity weakening.
- **F049** Auth/Webhook/Observer `#[source]` — `crates/fraiseql-error/src/core_error.rs:279,287,295`; new round-3 downcast test in `crates/fraiseql-error/src/core_error/tests.rs` passes (`cargo test -p fraiseql-error` clean, 23 lib + 23 integration + 3 doctests).
- **F050** typed FileError migration — spot-checked status mappings:
  - `not_found` → `NotFound` → 404 ✓ (was 404)
  - `permission_denied` → `PermissionDenied` → 403 ✓ (was 403)
  - `io_error` → `IoError` → 500 ✓ (was 500)
  - `size_limit_exceeded` → `SizeLimitExceeded` → 500 ✓ (was 500)
  - `not_implemented` → `NotImplemented` → 500 ✓ (was 500)
  - `unsupported`/`not_supported` → `Unsupported` → 500 ✓ (was 500)
  - `mime_type_not_allowed` → `MimeTypeNotAllowed` → 500 ✓ (was 500)
  - `invalid_key` → `InvalidKey` → 400 (was 500) — deliberate refinement documented at `crates/fraiseql-error/src/file.rs:233-244` and CHANGELOG.
- **F051** Storage variant docs — moot, variant deleted.
- **F052/F055** downcast pattern docs + non-exhaustive catch-all arm — confirmed.

---

## Regressions introduced (PASS 2 findings)

### F056 — 🟠 Observer entity_type_index reload weakens atomicity vs. prior RwLock — **CLOSED in v2.3.0**

- **Effort:** S
- **Location:** `crates/fraiseql-server/src/observers/runtime.rs:298-304, :675-680` (reload sequence: `self.entity_type_index.clear()` followed by per-key `insert` loop).
- **Finding:** Original `Arc<RwLock<HashMap>>` held a write guard around the entire rebuild so readers observed an atomic transition. The new `DashMap` rebuild does `clear()` then loops `insert(key, ids)` — for the duration of that loop, concurrent CDC-event lookup paths (`crates/fraiseql-server/src/observers/runtime.rs:436,491`) can observe an **empty or partially-populated index**, dispatching no observers for events that should have fired.
- **Resolution (H1).** The field changed from `Arc<DashMap<(String, String), Vec<i64>>>` to `Arc<ArcSwap<HashMap<(String, String), Vec<i64>>>>`. The reload path rebuilds the new `HashMap` off-line and publishes it via a single atomic `ArcSwap::store(Arc::new(new_map))` call, replacing the prior `clear()` + per-key `insert()` loop. Concurrent CDC-event lookups now always observe a fully-populated pre-reload or post-reload generation, never a partial state. Two new integration tests (`entity_type_index_swap_is_snapshot_atomic`, `entity_type_index_swap_visibility_is_prompt` in `crates/fraiseql-server/src/observers/tests.rs`) drive 100 000 concurrent lookups against a flipping writer and assert each result matches exactly one of the two known generations.
- **Status:** Closed in v2.3.0 (H1).

### F057 — 🟡 KeyedRateLimiter capacity-cap eviction race — **CLOSED in v2.3.0**

- **Effort:** S
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:321-334`.
- **Finding:** Capacity check uses `!self.records.contains_key(key) && self.records.len() >= self.max_entries` then iterates to find the oldest and `remove(&oldest_key)`. Between the `contains_key`/`len` snapshot and the `remove`, concurrent threads can observe stale capacity → multiple evictions of distinct oldest keys. The eviction loop had no upper bound on how far over capacity the map could grow under sustained concurrent insertion. Original `Mutex<HashMap>` serialised this so the cap was a hard upper bound.
- **Resolution (H2).** Added `insert_guard: Arc<parking_lot::Mutex<()>>` and split `check()` into a fast path (update existing keys via lock-free `get_mut`) and a slow path (acquire `insert_guard` for new-key inserts). The slow path performs cap-check, oldest-entry eviction, and the `entry()`-based insert in a single critical section, so `records.len() <= max_entries` holds at every observable instant — even mid-burst, not just after settling. Updates to keys already present never contend on the guard. Two new tests in `crates/fraiseql-auth/tests/error_rate_limiter_memory.rs` cover the sequential overflow case and a concurrent (`max_entries + 100`) burst with a sampler thread that records the high-water mark — strict-cap invariant is asserted mid-flight, not post-hoc.
- **Status:** Closed in v2.3.0 (H2).

---

## Cross-cutting concerns (PASS 3)

### F058 — 🟡 F031 property tests have weak deterministic assertions — **CLOSED in v2.3.0**

- **Effort:** XS
- **Location:** `crates/fraiseql-core/tests/property/property_executor.rs:79-96, :210-222`.
- **Finding:**
  - `prop_parse_query_never_panics` (line 71) uses regex `.{0,400}` — proptest's `.` excludes newlines, so multi-line queries (the realistic case) are never exercised.
  - `prop_parse_query_deterministic` asserted only `operation_type`, `root_field`, `selections.len()` across two parses — a parser that non-deterministically reordered selections or expanded fragments differently would still pass.
  - `prop_match_query_deterministic` asserted only `query_def.name`, `fields`, `operation_name` — silent on `arguments`, `selections`, or `parsed_query`.
- **Resolution (H4).** Replaced `.{0,400}` / `.{0,200}` with `(?s).{0,400}` / `(?s).{0,200}` in the three string-input tests so newline-bearing queries are exercised. For deterministic checks, `assert_eq!` is now applied to the full `QueryDefinition` (already `PartialEq`), `fields`, `operation_name`, and `arguments`; `selections` and `parsed_query` are compared via `serde_json::to_value` (no public-API change required for `FieldSelection` / `ParsedQuery`). The deeper comparison surfaces any non-determinism in `extract_arguments`, fragment expansion order, or alias resolution that the tuple-subset previously missed. All 9 F031 property tests still pass.
- **Status:** Closed in v2.3.0 (H4). Diverged from the plan's `PartialEq` suggestion in favour of `serde_json::to_value` to avoid adding `PartialEq` to public types (`FieldSelection`, `ParsedQuery`) — equivalent depth, zero API surface change.

### F059 — 🟡 `KeyedRateLimiter` Clock blanket impl widens production-vs-test divergence — **CLOSED in v2.3.0**

- **Effort:** XS (docs only)
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:43-50`.
- **Finding:** The blanket `impl<F: Fn() -> u64 + Send + Sync> Clock for F` lets any closure silently satisfy the `Clock` bound in production code. A misuse in production (e.g. someone passes `|| 0`) is impossible to grep for, with no compile-time signal that distinguishes a `SystemClock`-backed limiter from a closure-backed one.
- **Resolution (H3).** Added an `# Implementation note — production vs. test divergence` block to the `Clock` trait rustdoc explicitly framing the blanket impl as a test-only seam, calling out the three closure-misuse failure modes (constant, non-monotonic, `u64::MAX`), recommending the `Arc<AtomicU64>`-backed monotonic pattern used in `crates/fraiseql-auth/src/tests.rs`, and stating that code review should reject `with_clock(|| ...)` outside `#[cfg(test)]`. Migration guide section 7 mirrors the same callout.
- **Status:** Closed in v2.3.0 (H3).

### F060 — 🟢 PASS 3 — F032 READMEs are accurate but inconsistent on version pinning — **CLOSED in v2.3.0**

- **Effort:** XS
- **Location:** `crates/fraiseql-storage/README.md`, `crates/fraiseql-functions/README.md`.
- **Finding:** Both READMEs hard-coded `version = "2.3.0"` in their TOML example, forcing adopters to bump the literal string on every patch release.
- **Resolution (H5).** Replaced `version = "2.3.0"` with `version = "2.3"` in both READMEs. The lower bound stays at 2.3.0 (where the example APIs exist) while the caret semantics allow any 2.3.x patch without README edits. Pre-sed `git grep` confirmed only those two files had matches and both were in `[dependencies]` snippets, not changelog or migration prose.
- **Status:** Closed in v2.3.0 (H5).

---

## Recommendations for migration guide (item 03)

These are the **adopter-facing breaking changes** that downstream patterns will trip over:

1. **`FraiseQLError::Storage` deletion** — `match err { FraiseQLError::Storage { code, message } => ... }` will not compile. Migration:
   ```
   FraiseQLError::File(FileError::NotFound { id }) => ...
   FraiseQLError::File(FileError::PermissionDenied { message, .. }) => ...
   FraiseQLError::File(FileError::Backend { message, source }) => ...
   ```
   Suggested sed for blind migration (cannot do safely without manual review of the `code` discriminator):
   ```
   # NOT a safe sed — code string discriminator must be hand-mapped to the typed FileError variant.
   ```
   The migration guide should list the 9 historical `code` strings (`not_found`, `permission_denied`, `io_error`, `invalid_key`, `not_implemented`, `unsupported`, `not_supported`, `size_limit_exceeded`, `mime_type_not_allowed`) and the canonical typed variant for each.

2. **HTTP status refinement: `invalid_key` 500 → 400.** Adopters relying on the prior 500 for retry-loop signalling will start short-circuiting on 400 (no retry). Call out explicitly in migration guide.

3. **`RuntimeError` and 5 shadow domain enums deleted.** Pattern matches on `RuntimeError::*`, `error::AuthError`, `error::WebhookError`, `error::NotificationError`, `error::IntegrationError`, `error::ObserverError` (the `fraiseql-error` re-exports — not the subsystem-crate originals) will not compile. Subsystem crates' own `AuthError` / `WebhookError` / `ObserverError` are unchanged and reach `FraiseQLError` via subsystem-owned `From` impls.

4. **`ServerError::RuntimeError` → `ServerError::Engine` rename.** Anyone with `match e { ServerError::RuntimeError(inner) => ... }` breaks. Simple sed:
   ```
   sed -i 's/ServerError::RuntimeError/ServerError::Engine/g' **/*.rs
   ```

5. **`FraiseQLError::Auth/Webhook/Observer` are boxed-payload variants, not typed.** Downstream `match err { FraiseQLError::Auth(AuthError::TokenExpired) => ... }` will not compile. Use the documented `err.source().and_then(|s| s.downcast_ref::<AuthError>())` pattern (rustdoc on `Self::Auth` has the canonical example). The new round-3 test at `crates/fraiseql-error/src/core_error/tests.rs` proves this works.

6. **`DatabaseAdapter::invalidate_views` / `invalidate_list_queries` signature change** (F028 Wave 8) — takes `&[ViewName]` not `&[String]`. Adopters with custom adapter impls must update the trait method signatures. `ViewName::from(&str)` is a one-line conversion at the call site.

7. **`execute_with_projection_arc` signature change** (F043) — now takes `&ProjectionRequest<'_>` (struct) instead of 6 positional args. Adopters override the trait method by constructing a struct literal; missing-field is a hard compile error (deliberately not `#[non_exhaustive]`).

8. **`KeyedRateLimiter` is now generic over `<C: Clock = SystemClock>`** (F018). Code that names the type explicitly (`KeyedRateLimiter` without a parameter) still works at the default; code that names it inside a struct field as `KeyedRateLimiter` may need `KeyedRateLimiter<SystemClock>` to type-check.

9. **`Vec<&str>` → `impl Iterator` for `extract_root_field_names`** (F020). Two call sites typically need `.collect::<Vec<_>>()` added.

10. **Workspace clippy is now strictly denied** on `panic`, `unreachable`, `print_stdout`, `print_stderr`, `dbg_macro`, `todo`, `unimplemented`, `mem_forget`, `lossy_float_literal`, `semicolon_if_nothing_returned`, `undocumented_unsafe_blocks`, `missing_assert_message`. Three crates (`fraiseql-error`, `fraiseql-wire`, `fraiseql-storage`) additionally deny `clippy::indexing_slicing` at the crate root (Q4 pilot).

---

## Headline

**0🔴 / 0🟠 / 0🟡 / 0🟢 open** — all 5 round-3 findings (F056–F060) closed in v2.3.0 via the H1–H5 pre-merge hardening pass. Round-2 closures (F049, F050, F042, F028, F031, F032 etc.) all verified real and in place. **No 🔴 found at any point in the round-3 audit.**

| F-number | Original severity | Closure |
|---|---|---|
| F056 | 🟠 | H1 — `ArcSwap<HashMap>` snapshot swap + 2 integration tests |
| F057 | 🟡 | H2 — `parking_lot::Mutex<()>` insert guard + 2 tests |
| F058 | 🟡 | H4 — `(?s)` regex flag + `serde_json::to_value` full-struct equality |
| F059 | 🟡 | H3 — `Clock` trait rustdoc implementation note + migration-guide callout |
| F060 | 🟢 | H5 — `version = "2.3"` in the two README dep snippets |

### Resolution summary

The pre-merge hardening pass eliminated every contract-weakening that the round-3 audit surfaced. The DashMap migrations stay (lock-free reads on the request hot path), but the two maps whose previous `Mutex`/`RwLock` provided a stronger guarantee now restore that guarantee: the observer `entity_type_index` swaps atomically via `ArcSwap`, and `KeyedRateLimiter` enforces its capacity cap strictly under a serialising insert guard. The migration guide section 9 reflects the restored contracts and no longer carries "atomicity weakened" callouts. The `Clock` trait blanket-impl divergence is now explicit in the rustdoc and migration guide.

### Deliverable

- `IMPROVEMENTS_R3.md` (this file).
- 3 new tests added to `crates/fraiseql-error/src/core_error/tests.rs` covering `Auth`/`Webhook`/`Observer` source-downcast pattern. `cargo test -p fraiseql-error` passes (23 lib + 23 integration + 3 doctests).
- H1–H5 closure work — see per-F-number "Resolution" lines above.
