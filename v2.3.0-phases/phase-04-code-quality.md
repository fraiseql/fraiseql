# Phase 04: Code Quality & Debt Reduction

## Objective
Eliminate policy violations and panic-prone code, enforcing project standards and improving maintainability.

## Success Criteria
- [ ] All #[allow(...)] without // Reason: comments fixed (~29 violations, not 120+)
- [ ] Unwrap/expect in genuine production paths replaced with error handling (requires triage)
- [x] ~~Studio route placeholders wired~~ — no placeholder routes found (see Cycle 3)
- [ ] Infrastructure-gated tests properly env-gated (unverified — see Cycle 4)

> **PLAN REVIEW (2026-05-02):** Two items corrected: the `#[allow]` count was overstated
> by ~4×, and the studio placeholder cycle targets an issue that does not exist in the
> current codebase. Details per cycle below.

## TDD Cycles

### Cycle 1: #[allow] Justifications — COUNT CORRECTED

> **REVIEW NOTE (2026-05-02) — VALID, COUNT OVERSTATED:** The original plan states
> "120+ #[allow(...)]" violations. Codebase inspection found ~29 violations in
> fraiseql-server/src (of 101 total `#[allow(` occurrences). The A+ audit campaign
> (P04, 2026-03-17) cleaned this to zero, but new code added afterward reintroduced ~29.
> Most common missing reasons: `cast_possible_truncation` (12), `cast_sign_loss` (6),
> `cast_precision_loss` (2). These are concentrated in
> `crates/fraiseql-server/src/routes/graphql/handler.rs` and `server/builder.rs`.
>
> Update success criterion: target is ~29 violations, not 120+.

- **RED**: Run `grep -rn '#\[allow(' crates/ | grep -v '// Reason:' | grep -v 'test'` and count violations
- **GREEN**: Add `// Reason:` comments to all violations (expected: ~29 in fraiseql-server, verify other crates)
- **REFACTOR**: Standardize justification format (match existing A+ audit style)
- **CLEANUP**: Confirm zero violations remain

### Cycle 2: Replace Unwrap/Expect — REQUIRES TRIAGE FIRST

> **REVIEW NOTE (2026-05-02) — VALID but scope unclear:** Raw grep finds 1,030+
> `.unwrap()` / `.expect(` calls in the workspace. The vast majority are:
> (a) in test code (`#[cfg(test)]`, `tests/`, `_test.rs`), or
> (b) acceptable numeric casts in metrics/time handling.
>
> A small number of genuinely concerning production panics exist but were not fully
> enumerated. **The RED step must triage first**, not attempt wholesale replacement.
> Recommended triage: exclude `#[cfg(test)]` files and grep for `.unwrap()` in
> `src/` paths that are not clearly infallible (e.g., not `.unwrap_or`, not on
> `Mutex::lock()` where poisoning is intentional).

- **RED**: Triage: list `.unwrap()` / `.expect(` in non-test production code; classify each as (safe/infallible), (acceptable risk), or (must fix); output a prioritized list
- **GREEN**: Fix "must fix" items only — replace with `?`, `unwrap_or_else`, or proper error propagation
- **REFACTOR**: Add error types if the fix exposes a missing error variant
- **CLEANUP**: Re-run triage grep to confirm "must fix" list is empty

### ~~Cycle 3: Wire Studio Placeholders~~ — REMOVED (no placeholders found)

> **REVIEW NOTE (2026-05-02) — NOT FOUND:** Searched all files under
> `crates/fraiseql-server/src/routes/studio/`. No `unimplemented!()` or `todo!()`
> macros found in route handlers. The only placeholder-like code found was an internal
> implementation note in `metrics_summary.rs` ("zero-value summary used as placeholder
> until real collectors are wired") — this is not a route stub but a documented
> v1 limitation of the metrics aggregation logic.
>
> **No work required for this cycle.** The metrics_summary issue should be tracked
> separately if the team decides to wire real collectors.

### Cycle 4: Test Infrastructure Gating — UNVERIFIED

> **REVIEW NOTE (2026-05-02) — UNVERIFIED:** The plan claims infrastructure-dependent
> tests are not properly env-gated. This was not verified during the codebase review.
> Project memory notes that some tests use `FRAISEQL_PLATFORM_E2E=1` gating (Phase 8,
> Cycle 7). It is unclear how consistently this pattern is applied across the workspace.
> The RED step below should establish the actual count before committing to this cycle.

- **RED**: Count `#[ignore]` tests in the workspace; check each for env-var gating; produce a list of tests that have neither a `// Reason:` comment on `#[ignore]` nor an env-var skip guard
- **GREEN**: Add env-var gating to tests that require Redis, NATS, Vault, TLS, or database infra
- **REFACTOR**: Standardize on a single env-var pattern (e.g., `FRAISEQL_INTEGRATION=1`)
- **CLEANUP**: `cargo nextest run` without infra env vars passes with no unexpected failures

## Dependencies
- Requires: Phase 03 complete
- Blocks: Phase 05 (finalize)

## Status
[x] Complete — all 4 cycles done (2026-05-02)

### Results
- **Cycle 1** (#[allow] justifications): 29 violations fixed — all had split-line reasons;
  merged to same-line format (`#[allow(...)] // Reason: ...`) across 16 files
- **Cycle 2** (Unwrap triage): 0 must-fix items; all `.unwrap()` in production src/ files
  are inside inline `#[cfg(test)]` blocks; startup `.expect()` calls are intentional
  fail-fast with clear messages
- ~~Cycle 3~~ (Studio placeholders): removed — no stubs found
- **Cycle 4** (Test gating): 0 violations; all `#[ignore]` tests use `#[ignore = "..."]`
  inline reason string format with infrastructure requirements documented
