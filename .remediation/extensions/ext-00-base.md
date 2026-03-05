# FraiseQL — Remediation Plan

*Written 2026-03-05. Based on the rapport d'étonnement produced from full codebase discovery.*
*Benchmarks are out of scope (handled by velocitybench).*
*This plan does NOT duplicate the existing quality plan at `/tmp/fraiseql-quality-plan.md`;*
*it addresses issues not covered there, then enumerates which existing phases to execute.*

---

## Overview

The rapport identified two distinct categories of problems:

| Category | Count | Effort |
|---|---|---|
| **Narrative/documentation accuracy** | 6 items | Low–Medium |
| **Code correctness gaps** | 3 items | Medium |
| **Process / measurement** | 2 items | Low |
| **Existing quality plan (open phases)** | 6 phases | Already planned |

The plan is structured as four tracks. Tracks A–C are new work not in the existing quality plan.
Track D coordinates execution of the existing quality plan's open phases.

---

## Track A — Documentation Accuracy (Priority: High, ~2 days)

These are reputation and trust issues. They should be fixed before the next public communication
about the project.

---

### A1 — Remove "10-20x faster" from VALUE_PROPOSITION.md

**File:** `docs/VALUE_PROPOSITION.md`

**Problem:** Lines 12, 25, 258, 277 claim "10-20x faster query execution (measured end-to-end)"
and "10-20x faster (benchmarked)". No benchmark fixture produces this number. The commit
`70af96b43` already replaced similar claims in the performance docs with honest labels —
this file was missed.

**Fix:** Replace with honest qualitative language that can be defended without a benchmark.

```diff
-As a result, applications built with FraiseQL achieve 10-20x faster query execution,
-automatic SQL injection protection, and zero runtime parsing overhead—all while maintaining
-type safety across the entire stack.
+As a result, applications built with FraiseQL eliminate runtime schema interpretation overhead,
+provide automatic SQL injection protection, and maintain type safety across the entire stack.
+Performance characteristics depend on query complexity, database load, and hardware;
+see the benchmarks in the velocitybench repository for measured comparisons.
```

```diff
-| **Performance** | 10-20x faster (benchmarked) | Baseline (interpreted) |
+| **Performance** | Schema interpretation eliminated at compile time | Baseline (interpreted) |
```

```diff
-- 10-20x faster query execution (measured end-to-end)
+- Schema validation and SQL template generation moved to compile time
+- Runtime skips interpreter overhead for known query patterns
+- Measured latency comparisons: see velocitybench repository
```

**Acceptance:** `grep -rn "10-20x" docs/` → empty.

---

### A2 — Correct "zero runtime parsing overhead" in docs/architecture/overview.md

**File:** `docs/architecture/overview.md`, line 86

**Problem:** "Zero runtime parsing overhead" is inaccurate. GraphQL queries submitted at
runtime are still parsed by `graphql-parser`. What the compilation eliminates is *schema
validation* overhead and *SQL planning* overhead — the query text still has to be parsed.

**Fix:** Be precise about what is and isn't eliminated.

```diff
-- Zero runtime parsing overhead
+- Zero runtime *schema validation* overhead (schema is pre-validated at compile time)
+- Zero runtime *SQL planning* overhead for registered query patterns
+- GraphQL query parsing still occurs at runtime (document → AST via graphql-parser)
```

Also fix `docs/sla.md` line 9:
```diff
-FraiseQL is a compiled GraphQL execution engine built on Rust with zero-runtime overhead
-for deterministic query execution.
+FraiseQL is a compiled GraphQL execution engine built on Rust. Schema validation and SQL
+template generation are performed at compile time, eliminating interpreter overhead for
+registered query patterns at runtime.
```

And `docs/adr/0001-three-layer-architecture.md` line 22:
```diff
-- Zero runtime overhead from Python/TS
+- Zero runtime dependency on Python/TS (authoring-only, no FFI at runtime)
```

And `README.md` line 47:
```diff
-1. **Compile-time SQL generation.** Zero runtime overhead for deterministic queries. Your schema
-   is analyzed once at build; queries execute without interpretation.
+1. **Compile-time SQL generation.** Schema validation and SQL templates are generated at build
+   time. Known query patterns execute without re-planning; the GraphQL document is still parsed
+   per-request via the wire protocol.
```

**Acceptance:** `grep -rn "zero runtime overhead\|zero-runtime overhead" docs/ README.md` → empty.

---

### A3 — Clarify "compiled SQL" terminology everywhere it appears

**Problem:** "Compiled SQL" implies something closer to prepared statements or query plan
caching that persists across connections. What actually happens is:
1. At build time: GraphQL schema → validated SQL *templates* with typed placeholders
2. At runtime: templates loaded, parameters bound, queries dispatched

This is genuinely valuable — it's not nothing — but "compiled SQL" sets a wrong mental model.

**Recommendation:** Standardize on "pre-generated SQL templates" or "compile-time SQL template
generation" throughout all documentation.

**Files to update:**
- `README.md` (lines 11, 47, 57, 60)
- `ROADMAP.md` (line 6: "deterministic SQL generation" is fine; "compiled SQL" below it is not)
- `docs/sla.md` (line 78: "compiled SQL" → "pre-generated SQL templates")
- `docs/operations/compiled-schema-lifecycle.md` (line 13: already says "Pre-compiled SQL
  templates" — acceptable, keep)
- `docs/VALUE_PROPOSITION.md` (already handled in A1)

**Acceptance:** All remaining uses of "compiled SQL" either:
(a) are in the context of "compile-time SQL template generation" (accurate), or
(b) are in code comments explaining the actual template mechanism.

---

### A4 — Document fraiseql-wire platform constraint prominently

**Files:** `crates/fraiseql-wire/README.md` (create if absent), `docs/architecture/overview.md`,
main `README.md`

**Problem:** `fraiseql-wire` emits a `compile_error!` on non-Unix at build time, but this
constraint is not visible until a developer attempts to build on Windows or macOS. The streaming
backend is simply silently unavailable when excluded from builds.

**Fix:**

1. Add a `## Platform Support` section to the wire crate's documentation (or top-level module
   docs in `src/lib.rs`):

```rust
//! ## Platform Support
//!
//! `fraiseql-wire` requires a Unix-like operating system (Linux or macOS).
//! It uses Unix domain sockets and `SO_REUSEPORT` for low-latency streaming.
//!
//! **Windows is not supported.** Windows deployments should use the standard
//! HTTP/WebSocket transport in `fraiseql-server` instead, which works on all platforms.
//! The wire streaming backend is an optional performance optimization, not a requirement.
```

2. Add a note in `README.md` under the feature matrix:

```markdown
> **Note on fraiseql-wire:** The PostgreSQL streaming backend (`fraiseql-wire`) is
> Linux/macOS only. Windows deployments use the standard HTTP transport automatically.
```

3. Add a row to the platform compatibility table (if one exists, or create it in
   `docs/architecture/overview.md`):

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| Standard HTTP/GraphQL server | ✅ | ✅ | ✅ |
| fraiseql-wire streaming backend | ✅ | ✅ | ❌ |
| All database adapters | ✅ | ✅ | ✅ |

**Acceptance:** A developer reading README.md before building can predict the Windows limitation
without a compiler error.

---

### A5 — Document SQLite limitations explicitly

**Problem:** SQLite appears in the database support matrix without a "development only" label.
`SqliteAdapter::execute_function_call` returns `Err(FraiseQLError::Unsupported)`. A user
choosing SQLite for a lightweight self-hosted deployment will discover this at runtime.

**Fix:** In every location where databases are listed, add an explicit SQLite note.

`docs/architecture/overview.md` — database support table:

```diff
 | SQLite | ✅ | ❌ | ❌ | ❌ |
+> SQLite is supported for local development and testing only. Mutations are not supported.
+> Do not use SQLite in production deployments.
```

`README.md` — wherever the four databases are mentioned:

```diff
-PostgreSQL (primary), MySQL, SQLite, SQL Server
+PostgreSQL (primary), MySQL, SQL Server — production ready
+SQLite — development and testing only (queries only; no mutations)
```

Add to `fraiseql-core/src/db/sqlite/` (adapter module docs):

```rust
//! ## SQLite Limitations
//!
//! SQLite support is intended for **local development and testing only**.
//!
//! - Mutations (`execute_function_call`) are not supported and return
//!   [`FraiseQLError::Unsupported`].
//! - Relay cursor pagination is not implemented.
//! - Use PostgreSQL for all production deployments.
```

**Acceptance:** `grep -rn "SQLite" docs/ README.md` → every mention carries the "dev/test only"
qualifier or links to the limitations section.

---

## Track B — Code Correctness (Priority: Medium, ~3–5 days)

---

### B1 — Raise coverage threshold for security-critical paths

**Problem:** The CI enforces `--fail-under-lines 60` across the entire workspace. For a project
making correctness guarantees about SQL generation, RLS enforcement, and authentication flows,
60% is too low. Security-critical paths need near-complete coverage.

**Approach:** Two-tier coverage strategy.

**Step 1:** Raise the workspace-wide threshold incrementally.

```yaml
# .github/workflows/ci.yml — coverage job
- name: Generate coverage
  run: |
    cargo llvm-cov \
      --all-features \
      --workspace \
      --lcov \
      --output-path lcov.info \
      --fail-under-lines 70 \   # raise from 60 → 70 initially
      -- --test-threads=1
```

Target: 70 immediately, 75 within the next development cycle.

**Step 2:** Add per-crate coverage gates for the security-critical crates.

```yaml
# Add a separate coverage-security job
- name: Generate security-crate coverage
  run: |
    cargo llvm-cov \
      --features postgres \
      -p fraiseql-core \
      -p fraiseql-auth \
      -p fraiseql-secrets \
      --lcov \
      --output-path lcov-security.info \
      --fail-under-lines 80
```

**Step 3:** Identify the specific files under 60% today.

```bash
cargo llvm-cov \
  --all-features \
  --workspace \
  --json \
  --output-path coverage.json \
  -- --test-threads=1

# Find files below 70%
python3 -c "
import json, sys
data = json.load(open('coverage.json'))
for f in data['data'][0]['files']:
    pct = f['summary']['lines']['percent']
    if pct < 70:
        print(f'{pct:.1f}%  {f[\"filename\"]}')
" | sort -n | head -30
```

Write tests for the worst offenders, starting with:
- `fraiseql-core/src/security/rls_policy.rs` (RLS injection paths)
- `fraiseql-core/src/security/query_validator.rs`
- `fraiseql-auth/src/` (auth token validation)
- `fraiseql-secrets/src/` (encryption paths)

**Acceptance:**
- `--fail-under-lines 70` passes in CI
- `fraiseql-core`, `fraiseql-auth`, `fraiseql-secrets` each pass `--fail-under-lines 80`
- No security-critical module below 75% (verified by coverage report review)

---

### B2 — Audit fraiseql-webhooks for production unwraps

**Problem:** The existing quality plan already addressed most unwraps. Verify no production
(non-test) paths remain.

**Files to audit:**
- `crates/fraiseql-webhooks/src/signature/generic.rs`
- `crates/fraiseql-webhooks/src/signature/stripe.rs`
- `crates/fraiseql-webhooks/src/signature/github.rs`
- `crates/fraiseql-webhooks/src/signature/shopify.rs`

**Check:** All `Hmac::new_from_slice(...).unwrap()` occurrences should be inside `#[cfg(test)]`
or `mod tests {}` blocks. If any are in production code paths:

```rust
// WRONG — panics if key length is ever 0
let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();

// CORRECT — return a verification error
let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
    .map_err(|_| WebhookError::InvalidSignatureKey)?;
```

**Check command:**
```bash
# Find unwraps that are NOT inside test blocks
# (manual inspection required — grep can't track block nesting)
for f in crates/fraiseql-webhooks/src/signature/*.rs; do
  echo "=== $f ==="
  # Lines with unwrap() that come before any #[cfg(test)] or mod tests
  awk 'BEGIN{in_test=0}
       /^#\[cfg\(test\)\]|^mod tests \{/{in_test=1}
       !in_test && /\.unwrap\(\)/{print NR": "$0}' "$f"
done
```

**Acceptance:** Zero `.unwrap()` calls in non-test code in `fraiseql-webhooks/src/`.
Every production `Hmac::new_from_slice` failure maps to a typed `WebhookError`.

---

### B3 — Resolve the 69 missing_errors_doc functions in fraiseql-server

**File:** `crates/fraiseql-server/src/lib.rs` — the `// Note: 69 functions missing
missing_errors_doc (Phase 6 deferred)` comment.

**Why this matters:** `missing_errors_doc` is a `clippy::pedantic` lint. The server lib.rs
has `#![allow(clippy::missing_errors_doc)]` as a blanket suppression. Public API functions
that return `Result` without documenting their errors make it harder to write correct client
code and produce incomplete `cargo doc` output.

**Approach:** Do not try to fix all 69 at once. Triage by exposure:

**Tier 1 — Public API (doc visible to library users):** Fix these first.
```bash
grep -n "pub fn\|pub async fn" crates/fraiseql-server/src/*.rs |
  grep -v "#\[allow" | head -20
```
For each public function returning `Result<_, FraiseQLError>` or `Result<_, E>`:
```rust
/// # Errors
///
/// Returns [`FraiseQLError::Configuration`] if the server config is invalid.
/// Returns [`FraiseQLError::Database`] if the connection pool cannot be established.
pub async fn start(config: ServerConfig) -> Result<()> {
```

**Tier 2 — Internal/private functions:** Remove the blanket allow, then address lint
violations individually with surgical `#[allow]` where adding docs would be noise:
```rust
#[allow(clippy::missing_errors_doc)] // internal helper, not public API
fn build_middleware_stack(...) -> Result<Router> {
```

**Target:** Remove `#![allow(clippy::missing_errors_doc)]` from `fraiseql-server/src/lib.rs`.
Replace with surgical per-function `#[allow]` only where genuinely appropriate (private
helpers). All public functions in the server crate must have `# Errors` doc sections.

**Acceptance:**
- `#![allow(clippy::missing_errors_doc)]` removed from `fraiseql-server/src/lib.rs`
- `cargo doc --no-deps -p fraiseql-server 2>&1 | grep "missing"` → empty
- Remaining per-function `#[allow(clippy::missing_errors_doc)]` count ≤ 10,
  all on private/internal functions

---

## Track C — Process / Measurement (Priority: Low, ~1 day)

---

### C1 — Replace self-scored quality metric with objective CI-gated criteria

**Problem:** MEMORY.md records "Quality score: 4.50/5 (target was 4.50 — achieved)". A score
that equals its own target is not a measurement. This creates false confidence: it makes the
project look "done" when 6 phases of a quality plan remain open.

**Fix:** Define quality as a set of binary CI gates, not a numeric score. Add a
`docs/quality-gates.md` file that lists the objective, automatable criteria:

```markdown
# FraiseQL Quality Gates

These gates are enforced in CI. The project meets its quality bar when all pass.

## Must Pass on Every Commit
- [ ] `cargo fmt --check` — formatting
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — no lint warnings
- [ ] `cargo test --workspace` — all unit tests pass
- [ ] `cargo doc --no-deps --workspace` — documentation builds clean

## Must Pass on Main Branch
- [ ] `cargo llvm-cov --workspace --fail-under-lines 70` — minimum coverage
- [ ] `cargo audit` — no known vulnerabilities
- [ ] All integration test jobs green in CI

## Quality Improvement Indicators (not gates, but tracked)
- doctest `ignore` count: target ≤ 10 (current: see CI badge)
- `missing_errors_doc` suppression count: target 0 in public API
- `#[allow(clippy::...)]` count: reviewed quarterly
```

Remove the numeric score from MEMORY.md and replace with a link to this document.

**Acceptance:** A new contributor can open `docs/quality-gates.md` and understand what
"production quality" means for this project without circular self-assessment.

---

### C2 — Establish a "quality debt" tracking file

**Problem:** The 15-phase quality plan lives in `/tmp/`. That means it disappears on reboot
and has no connection to the git history. Future quality work will be planned the same way —
ad-hoc, outside the repository, disconnected from the code it describes.

**Fix:** Move the quality plan into the repository as a tracked document.

```bash
cp /tmp/fraiseql-quality-plan.md docs/quality-plan.md
git add docs/quality-plan.md
```

Update it to reflect completed phases (1, 5, 6, 7, 8, 9, 11, 12, 13) and link from
the quality gates document.

**Alternative:** If the phases directory (`/.phases/`) convention from CLAUDE.md is preferred,
create `.phases/` in the repo and check it in on feature branches, removing it on merge to main.

**Acceptance:** The quality plan is version-controlled and visible to all contributors.
`ls /tmp/fraiseql-quality-plan.md` is no longer the authoritative location.

---

## Track D — Execute Existing Quality Plan (Open Phases)

The following phases from `/tmp/fraiseql-quality-plan.md` are not yet done.
They are listed here for completeness and priority ordering.

| Phase | Title | Priority | Blocks |
|---|---|---|---|
| **Phase 2** | Ignored Doctest Triage and Fix | High | Phase 14 |
| **Phase 3** | CLI Command Test Coverage | High | nothing |
| **Phase 4** | Observer Transport Integration Tests | Medium | nothing |
| **Phase 10** | SDK Cross-Compliance Test Harness | Low | nothing |
| **Phase 14** | Documentation Examples Accuracy Audit | Medium | Phase 2 |
| **Phase 15** | Final Verification and Eternal Sunshine | Blocker | All others |

**Recommended execution order:** 2 → 3 → 4 → 14 → 10 → 15

Rationale:
- Phase 2 (doctests) should run before Phase 14 (doc accuracy) because fixing doctests
  reveals stale API examples that Phase 14 would otherwise re-fix
- Phase 3 and 4 are independent; run in parallel if possible
- Phase 10 (SDK parity) is the lowest leverage for internal quality; deprioritize
- Phase 15 is the gate; nothing ships until it passes

**Key acceptance for Phase 15 (Eternal Sunshine):**
```bash
# Must all return empty
git grep -i "phase\|todo\|fixme\|hack" -- '*.rs' '*.md'
grep -rn "10-20x\|zero runtime overhead\|zero-runtime overhead" docs/
grep -rn '```ignore' crates/ --include="*.rs" | grep -v "Requires:" | wc -l
# → 0
```

---

## Execution Summary

### Week 1 — Documentation accuracy (Track A)

Day 1–2:
- [ ] A1: Remove "10-20x" claims from VALUE_PROPOSITION.md
- [ ] A2: Fix "zero runtime overhead" in overview.md, sla.md, adr
- [ ] A3: Standardize "compiled SQL" terminology across docs
- [ ] A4: Document wire Unix-only constraint (lib.rs + README)
- [ ] A5: Document SQLite dev-only limitation (adapter + README)

### Week 2 — Code correctness starts (Track B, Phase 2)

Day 3–5:
- [ ] B1 Step 1: Raise coverage threshold to 70% in CI
- [ ] B1 Step 2: Add per-crate 80% gate for security crates
- [ ] B2: Audit webhooks for production unwraps
- [ ] Begin Phase 2 (doctests): Tier A blocks (pure API, no infra)

### Week 3 — Continuing correctness (Track B + D)

- [ ] B3: Fix missing_errors_doc for public API in fraiseql-server (Tier 1)
- [ ] Continue Phase 2 (doctests): Tier B/C/D
- [ ] Phase 3: CLI command test coverage

### Week 4 — Process + remaining phases (Track C + D)

- [ ] C1: Create docs/quality-gates.md; remove numeric score from MEMORY.md
- [ ] C2: Move quality plan into repository
- [ ] Phase 4: Observer transport integration tests
- [ ] Phase 14: Documentation examples accuracy audit (after Phase 2 complete)

### Month 2 — Final gates

- [ ] B1 Step 3: Coverage analysis; write tests for worst-offending modules
- [ ] Phase 10: SDK cross-compliance (can run in parallel with above)
- [ ] Phase 15: Final verification and Eternal Sunshine check
- [ ] Tag v2.0.1 after Phase 15 passes

---

## Definition of Done

The remediation is complete when:

1. `grep -rn "10-20x\|zero runtime overhead\|zero-runtime overhead" docs/ README.md` → empty
2. `grep -rn "compiled SQL" docs/ README.md` → only in accurate contexts
3. Platform matrix (wire, SQLite) is visible in README.md before installation
4. `--fail-under-lines 70` passes in CI workspace-wide
5. `fraiseql-core`, `fraiseql-auth`, `fraiseql-secrets` each pass `--fail-under-lines 80`
6. `#![allow(clippy::missing_errors_doc)]` removed from fraiseql-server
7. Zero `.unwrap()` in non-test production code (fraiseql-webhooks signature)
8. `docs/quality-gates.md` exists and is linked from README.md
9. Quality plan is version-controlled (not in `/tmp/`)
10. All 15 phases of the existing quality plan marked complete
11. `git grep -i "phase\|todo\|fixme\|hack" -- '*.rs'` → empty (Phase 15 gate)
