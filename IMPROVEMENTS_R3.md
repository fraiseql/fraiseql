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

### F056 — 🟠 Observer entity_type_index reload weakens atomicity vs. prior RwLock
- **Effort:** S
- **Location:** `crates/fraiseql-server/src/observers/runtime.rs:298-304, :675-680` (reload sequence: `self.entity_type_index.clear()` followed by per-key `insert` loop).
- **Finding:** Original `Arc<RwLock<HashMap>>` held a write guard around the entire rebuild so readers observed an atomic transition. The new `DashMap` rebuild does `clear()` then loops `insert(key, ids)` — for the duration of that loop, concurrent CDC-event lookup paths (`crates/fraiseql-server/src/observers/runtime.rs:436,491`) can observe an **empty or partially-populated index**, dispatching no observers for events that should have fired. The field rustdoc (line 130-138) deliberately accepts this on the grounds that "this index drives logging only", but the production read sites at 436/491 use the index to look up `observer_ids` for **action dispatch**, not logging. Verify the read-path is truly best-effort; if any action depends on the lookup succeeding, the reload window is a regression.
- **Suggested approach:** Either (a) keep the documented best-effort posture but assert that the read-path tolerates an empty lookup (add a test that triggers a reload concurrently with an event), or (b) build the new map locally then atomically swap via `ArcSwap<DashMap>` so readers always see a fully-populated snapshot.
- **Risk:** Action dispatch silently no-ops during reload — observable only as missing observer invocations under load + reload concurrency.
- **Confidence:** Medium (read-path side-effects need verification before sizing severity).

### F057 — 🟡 KeyedRateLimiter capacity-cap eviction race
- **Effort:** S
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:321-334`.
- **Finding:** Capacity check uses `!self.records.contains_key(key) && self.records.len() >= self.max_entries` then iterates to find the oldest and `remove(&oldest_key)`. Between the `contains_key`/`len` snapshot and the `remove`, concurrent threads can observe stale capacity → multiple evictions of distinct oldest keys. The field rustdoc (line 289-291) calls this "best-effort", which is fine, but the eviction loop has no upper bound on how far over capacity the map can grow under sustained concurrent insertion. Under heavy attack the limiter could grow well past `max_entries` before any individual thread observes it. Original `Mutex<HashMap>` serialised this so the cap was a hard upper bound.
- **Suggested approach:** Either accept and explicitly document the overshoot bound (e.g., "may temporarily exceed `max_entries` by up to N concurrent inserters") or fall back to a slow-path under a stricter lock once `len() >= max_entries * 1.1`.
- **Risk:** Resource bound becomes soft instead of hard under brute-force attack — degrades the rate-limiter's defensive guarantee under exactly the conditions it exists for.
- **Confidence:** Medium.

---

## Cross-cutting concerns (PASS 3)

### F058 — 🟡 F031 property tests have weak deterministic assertions
- **Effort:** XS
- **Location:** `crates/fraiseql-core/tests/property/property_executor.rs:79-96, :210-222`.
- **Finding:**
  - `prop_parse_query_never_panics` (line 71) uses regex `.{0,400}` — proptest's `.` excludes newlines, so multi-line queries (the realistic case) are never exercised.
  - `prop_parse_query_deterministic` asserts only `operation_type`, `root_field`, `selections.len()` are equal across two parses — a parser that non-deterministically reordered selections, fragment expansions, or aliases would still pass.
  - `prop_match_query_deterministic` asserts only `query_def.name`, `fields`, `operation_name` — but does **not** assert `arguments`, `where_clause` injection, projection plan, or `sql_source` are deterministic. If F005/F024's `extract_arguments` had non-determinism (e.g., HashMap ordering bleeding through), the test would silently pass.
- **Suggested approach:** Replace `.{0,400}` with `(?s).{0,400}` (or `\\PC{0,400}`); expand the deterministic-check tuples to include `arguments`, `where_clause`, and the full `selections` slice.
- **Confidence:** High — these are weak-assertion patterns at well-known sites; the tests aren't wrong, just under-protective.

### F059 — 🟡 `KeyedRateLimiter` Clock blanket impl widens production-vs-test divergence
- **Effort:** XS (docs only)
- **Location:** `crates/fraiseql-auth/src/rate_limiting.rs:43-50`.
- **Finding:** The blanket `impl<F: Fn() -> u64 + Send + Sync> Clock for F` is great for test ergonomics but means **any** closure / `fn` pointer silently satisfies the `Clock` bound in production code, including ones that return constants. There's no compile-time signal that "this is a production limiter using SystemClock" vs. "this is a limiter using an ad-hoc closure as a clock". A misuse in production code (e.g. someone passes `|| 0`) is impossible to grep for. The brief asked whether a `MockClock` struct exists — answer: no, the closure pattern is the mock. This is fine, but the policy is worth surfacing.
- **Suggested approach:** Either rename `SystemClock` constructor sites to make the production-vs-test split self-documenting, or add a `#[must_use]` on `with_clock` so the lint catches dropped limiters. Lowest-cost: document the closure-as-mock policy in the `Clock` trait rustdoc.
- **Confidence:** High.

### F060 — 🟢 PASS 3 — F032 READMEs are accurate but inconsistent on version pinning
- **Effort:** XS
- **Location:** Spot-checked `crates/fraiseql-storage/README.md` (pins `2.3.0`), `crates/fraiseql-functions/README.md` (pins `2.3.0`).
- **Finding:** Both READMEs hard-code `version = "2.3.0"` in their TOML example. Future versions will need a sweep across 13+ README files to keep examples current. Compare e.g. `serde`'s `"1"` or `tokio`'s `"1"` shorthand. Not a release blocker, but a maintenance trap.
- **Suggested approach:** Replace pinned version with `version = "2"` (caret semantics) in usage examples. Single-find-and-replace across 13 READMEs.
- **Confidence:** High.

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

3🟠 5🟡 1🟢 across 5 new findings (F056–F060). Round-2 closures (F049, F050, F042, F028, F031, F032 etc.) all verified real and in place. **No 🔴 found** in the round-3 diff.

### Top 3 must-fix-before-release
1. **F056** — observer `entity_type_index` reload window. Need to confirm whether read-path tolerates empty lookups; if action dispatch depends on the lookup hitting, the reload is now silently dropping observers. Spend an hour reading `runtime.rs:436,491` against a reload-mid-event test fixture.
2. **F058** — strengthen the F031 property tests' deterministic-check assertions. Cheap (one tuple expansion per test) and closes a non-trivial blind spot in regression coverage of the `extract_arguments` refactor.
3. **Migration guide** must explicitly cover items 1–4 above. The Storage→File migration is the highest-risk adopter break, and items 3/4 are silent compile errors that need a sed-friendly recipe.

### Biggest cross-cutting concern
**Documented best-effort lock-free behaviour vs. silent semantic regression** — the F006/F008/F013/F048 DashMap migrations are correct under their stated atomicity rules, but in three places (F056 observer reload, F057 rate-limiter eviction, the documented but unbenched F059 closure-clock pattern) the previous `Mutex`/`RwLock` provided a **stronger** guarantee than the new code admits. Each case is documented in rustdoc with "best-effort", but the adopter who reads the trait signature alone won't see it. A migration-guide section "what 'best-effort' means in 2.3" would surface the contract change.

### Deliverable
- `IMPROVEMENTS_R3.md` (this file).
- 3 new tests added to `crates/fraiseql-error/src/core_error/tests.rs` covering `Auth`/`Webhook`/`Observer` source-downcast pattern. `cargo test -p fraiseql-error` passes (23 lib + 23 integration + 3 doctests).
