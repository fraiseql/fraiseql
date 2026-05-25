# Policy decisions — round-2 audit prerequisites

Updated: 2026-05-24 (Q1 resolution amendment — per-enum compose/absorb verdicts after grep verification). Prior amendment same day re-derived all four questions under the industrial framing.

Decided at commit: 85ac41e60

## Q1 — RuntimeError vs FraiseQLError **(REVISED)**

**Decision:** Delete `RuntimeError` and its sibling domain re-exports (`AuthError`, `WebhookError`, `FileError`, `NotificationError`, `IntegrationError`, `ObserverError`) before 2.4. Make `FraiseQLError` the single error taxonomy. Re-add domain variants to `FraiseQLError` where genuinely needed (`Auth`, `Webhook`, `File`, `Notification`, `Integration`, `Observer`); collapse the four overlapping HTTP-shaped variants (`Internal`, `NotFound`, `RateLimited`, `ServiceUnavailable`) into the engine enum.

**Rationale:**

- Zero production callers: `grep -rn "Result<.*RuntimeError>" crates/` returns four hits, all in `crates/fraiseql-error/` itself (`http.rs:261-275`, `lib.rs:70`). All other matches are in `crates/fraiseql-error/tests/` (test files). No axum handler in `crates/fraiseql-server/src/routes/` returns `Result<_, RuntimeError>`.
- No external Rust consumer exists. The Rust SDK client (`sdks/official/fraiseql-rust/fraiseql-client/src/error.rs:10`) defines its **own** `FraiseQLError` enum — a name collision, not a re-export — and does not depend on the server-side `RuntimeError`. The CLAUDE.md vision pins Rust to the server side; Python/TypeScript/Go/Java are authoring-only and cannot consume Rust error types.
- The prior decision's "stable extension point for downstream" defense assumed a future Rust SDK author would write custom axum handlers and want a hardened HTTP-shaped error type. Under "breaking changes acceptable + industrial," shipping a 150-line published enum for hypothetical consumers is the opposite of industrial; *one canonical error taxonomy* is what serious downstreams want.
- The auth-error doc at `crates/fraiseql-error/src/auth.rs:1-6` ("client-facing surface") is still correct conceptually — engine errors and domain errors are distinct categories — but the *separation mechanism* should be enum variants within one hierarchy, not two parallel enums with overlapping variants.
- `ServerError::RuntimeError` at `crates/fraiseql-server/src/lib.rs:222` wraps `FraiseQLError`, confirming the naming bug. It also confirms the de-facto pattern: `ServerError → FraiseQLError` is the chain that exists in real code.

**Per-enum compose-vs-absorb verdicts (2026-05-24 resolution):**

Directive from user: *Option A — composition via `#[from]` — is the default. Verify per-enum that none are vestigial enough to absorb. "One canonical taxonomy" means one root type, not one enum. Domain modules own their vocabulary. Flatten only when an enum has ≤5 variants used at ≤2 sites.*

Critical evidence found during verification: the 6 `fraiseql-error/src/<name>.rs` enums **are NOT the production vocabulary**. They are shadow re-statements that only `RuntimeError` aggregates and only their own test files import. The real, in-use subsystem error vocabularies live elsewhere:

- `fraiseql_auth::AuthError` at `crates/fraiseql-auth/src/error.rs:17` (26 variants, heavily used inside `fraiseql-auth/src/{rate_limiting,phone_otp,state_store,handlers,oauth,jwt,totp_mfa,…}`).
- `fraiseql_webhooks::WebhookError` at `crates/fraiseql-webhooks/src/lib.rs:82` (real subsystem enum used by webhooks crate; the `fraiseql-error/src/webhook.rs` copy is unused by any non-test consumer).
- `fraiseql_observers::ObserverError` at `crates/fraiseql-observers/src/error.rs:12` (12+ variants with OB001–OB012 codes; the `fraiseql-error/src/observer.rs` copy is unused by any non-test consumer).
- `fraiseql_wire::auth::AuthError` at `crates/fraiseql-wire/src/auth/mod.rs:17` (SCRAM-specific wire variants; orthogonal vocabulary).

The doc-comments on `fraiseql-error/src/auth.rs:1-6` and `src/observer.rs:1-5` explicitly acknowledge this duality ("for internal OIDC/JWT processing errors, see `fraiseql_auth::AuthError`"). The fraiseql-error copies exist only to feed `RuntimeError`'s HTTP-shape responses — a role that disappears with `RuntimeError`.

| Enum (fraiseql-error) | File:line | Variants | Production call sites (non-test, non-self) | Verdict |
|-----------------------|-----------|----------|---------------------------------------------|---------|
| `AuthError` | `crates/fraiseql-error/src/auth.rs:9` | 11 | 0 (only `crates/fraiseql/src/lib.rs:75` umbrella re-export + `crates/fraiseql-error/tests/auth_errors.rs:3`) | **Delete outright** — vocabulary lives in `fraiseql_auth::AuthError`. Compose **that** one into `FraiseQLError::Auth(#[from] fraiseql_auth::AuthError)` |
| `WebhookError` | `crates/fraiseql-error/src/webhook.rs:4` | 9 | 0 (only umbrella + own tests) | **Delete outright** — vocabulary lives in `fraiseql_webhooks::WebhookError`. Compose **that** one into `FraiseQLError::Webhook(#[from] fraiseql_webhooks::WebhookError)` |
| `FileError` | `crates/fraiseql-error/src/file.rs:4` | 8 | **9 real callers**: `fraiseql-server/src/storage/{gcs,s3,azure,local,mod,tests}.rs`, `fraiseql-server/src/routes/storage/mod.rs`, `fraiseql-storage/src/backend/{gcs,azure}.rs` | **Compose** — this is the only one of the six with genuine production use. Keep as standalone enum in `fraiseql-error` (or relocate to `fraiseql-storage` if cleaner), compose into `FraiseQLError::File(#[from] FileError)`. 8 variants ≥ 6 threshold AND ≥ 3 call sites AND clear storage-subsystem ownership. |
| `NotificationError` | `crates/fraiseql-error/src/notification.rs:6` | 8 | 0 (only own tests) | **Delete outright** — zero callers, no `fraiseql-notifications` crate exists yet. If/when a notification subsystem is added, that crate defines its own vocabulary. Today this enum is pure dead weight. |
| `IntegrationError` | `crates/fraiseql-error/src/integration.rs:5` | 5 | 0 (only own tests) | **Delete outright** — zero callers, "integration" is too vague a domain boundary, and 5 variants × 0 sites is the textbook vestigial case under the user's threshold (≤5 variants AND ≤2 call sites AND no clear domain owner). |
| `ObserverError` | `crates/fraiseql-error/src/observer.rs:8` | 7 | 0 (only own tests) | **Delete outright** — vocabulary lives in `fraiseql_observers::ObserverError` (richer, has OB-codes). Compose **that** one into `FraiseQLError::Observer(#[from] fraiseql_observers::ObserverError)`. |

Net shape of `FraiseQLError` after merge:

```rust
pub enum FraiseQLError {
    // ... existing core/engine variants (Parse, Validation, Database, …) ...

    // Domain composition — owned by their subsystem crates
    Auth(#[from] fraiseql_auth::AuthError),
    Webhook(#[from] fraiseql_webhooks::WebhookError),
    Observer(#[from] fraiseql_observers::ObserverError),

    // File domain — kept in fraiseql-error (or moved to fraiseql-storage) because no subsystem crate owns it; 9 production call sites justify a standalone enum
    File(#[from] FileError),

    // HTTP-shape variants absorbed from RuntimeError (Q1 root decision)
    Internal { message: String, source: Option<Box<dyn std::error::Error + Send + Sync>> },
    NotFound { resource: String },
    RateLimited { retry_after: Option<u64> },
    ServiceUnavailable { reason: String, retry_after: Option<u64> },
}
```

Three subsystems compose, one (`File`) survives as a fraiseql-error-owned enum because it has real callers but no owning subsystem crate, and three (`NotificationError`, `IntegrationError`, `Auth/Webhook/Observer fraiseql-error copies`) are deleted outright as vestigial.

**Action items for round-2:**

- Delete `crates/fraiseql-error/src/lib.rs:74-151` (`RuntimeError` enum + impls).
- Delete `crates/fraiseql-error/src/{auth,webhook,notification,integration,observer}.rs` (5 of the 6 shadow domain modules). Keep `crates/fraiseql-error/src/file.rs` as-is (production-used).
- Delete `crates/fraiseql-error/tests/{auth_errors,webhook_errors,notification_errors,integration_errors,observer_errors}.rs`. Keep `tests/file_errors.rs`.
- Add `Auth(#[from] fraiseql_auth::AuthError)`, `Webhook(#[from] fraiseql_webhooks::WebhookError)`, `Observer(#[from] fraiseql_observers::ObserverError)`, `File(#[from] FileError)`, plus the four HTTP-shape variants (`Internal`, `NotFound`, `RateLimited`, `ServiceUnavailable`) to `FraiseQLError`. Note: this introduces three reverse dependencies — `fraiseql-error → fraiseql-auth`, `fraiseql-error → fraiseql-webhooks`, `fraiseql-error → fraiseql-observers`. If that inverts the current dependency graph (`fraiseql-error` is currently a leaf), the resolution is to **invert**: either define `FraiseQLError` in a new lower crate (`fraiseql-error-core`) and have subsystems compose upward, OR keep `FraiseQLError` in `fraiseql-error` and let subsystems depend on it but provide their own `From<SubsystemError> for FraiseQLError` impls in their own crates. The cleaner industrial answer is the second pattern (subsystem owns the `From` impl, fraiseql-error stays a leaf). Round-2 maniac to pick and document in the delete-PR.
- Move `IntoHttpResponse` and `ErrorResponse` (`crates/fraiseql-error/src/http.rs:74,261-275`) to wrap `FraiseQLError` directly. Rename / rebuild trait accordingly.
- Rename `ServerError::RuntimeError` → `ServerError::Engine` (`crates/fraiseql-server/src/lib.rs:221-222`). Update `#[from] fraiseql_core::error::FraiseQLError` chain — unchanged in semantics, fixed in name.
- Delete the umbrella re-export at `crates/fraiseql/src/lib.rs:75` (`pub use fraiseql_error::{AuthError, ConfigError, FileError, RuntimeError, WebhookError}`). Replace with `pub use fraiseql_error::{ConfigError, FileError}; pub use fraiseql_core::error::FraiseQLError;` (or equivalent).
- Add `# Breaking changes` block to `CHANGELOG.md` and a 2.4 `DEPRECATIONS.md` entry covering: `RuntimeError` removal, 5-of-6 shadow domain enum removal, file-storage callers now reach `FileError` via `FraiseQLError::File(_)` pattern.
- Round-2 maniac: re-evaluate F017 against the merged taxonomy. The "lossy `From`-conversions" claim is currently false (no conversions exist) — but after the merge, lossy conversions inside `ServerError → FraiseQLError → axum::Response` *will* exist and should be audited.

**Open issues / things the maniac should NOT touch in round-2 until human review:**

- (none for Q1 — per-enum verdicts above resolve the prior open issue; the only remaining design judgement, the dependency-graph direction noted in the action items, is bounded enough for the delete-PR to make and document)

## Q2 — async_trait removal scope **(UNCHANGED — values-direction considered)**

**Decision:** Freeze the baseline at 180 with the existing `lint-async-trait` gate; do NOT actively migrate. Re-evaluate when RTN-in-`dyn` stabilises (RFC 3425) OR `trait-variant` gains a `dyn`-compat story.

**Rationale (re-derived under industrial framing):**

- Spot-check confirms dyn-dispatch is load-bearing, not ceremonial. Workspace has 84 trait definitions, 64 of which are used as `dyn TraitName`, with 352 call sites (`grep -rno "dyn [A-Z][A-Za-z_0-9]*" crates/*/src/`). Top consumers — `SessionStore` (22), `OAuthProvider` (16), `Clock` (15), `DeadLetterQueue` (14), `CustomScalar` (12), `StateStore` (11), `CheckpointStore` (11), `QueryExecutor` (10), `AccountStore` (10) — are textbook pluggability points where downstream operators supply their own implementations.
- Verified the prior decision's 3 sampled traits: `ApqStorage` has 2 production impls (memory + redis at `crates/fraiseql-core/src/apq/{memory_storage,redis_storage}.rs`); `FunctionStore` has 1 production impl; `CacheBackend` has 1 production + 1 test. The trait list is **future-proofed** for pluggability; deleting dyn-dispatch closes that door.
- Under "industrial + breaking changes OK," the breaking change being considered is "force every downstream impl-or onto generic + monomorphisation." That doesn't *modernise* — it *destroys the pluggability architecture* (heterogeneous registries cannot be expressed with generics). The user's lever does not unblock this path; it makes it worse.
- The current per-call allocation (`Pin<Box<dyn Future + Send>>`) is real overhead but dominated by every other allocation on the request path that F001–F005, F007, F037, F042 are targeting. Get those merged first.
- Stable RTN-in-`dyn` (RFC 3425) is the correct *long-term* answer: it lets dyn-dispatch traits drop the async_trait macro without losing object safety. That's the industrial off-ramp — wait for it.

**Action items for round-2:**

- Keep `ASYNC_TRAIT_LIMIT := 180` in `Makefile:286` as the sole enforcement. Do not lower opportunistically.
- Document the only acceptance criterion for a removal PR (in `docs/architecture/overview.md` or a new `docs/architecture/async-trait-policy.md`): trait must have **zero** matches in `grep -rn "dyn TraitName\b" crates/ tests/ sdks/` AND must pass `cargo semver-checks` (already in CI at `.github/workflows/ci.yml:108-123`) AND must include a criterion bench showing measurable wins.
- Reject any round-2 "drive-by" migration PR that doesn't satisfy all three criteria.

**Open issues / things the maniac should NOT touch in round-2 until human review:**

- None. Policy is "leave alone, gated, wait for RTN-in-`dyn`."

## Q3 — Per-crate `#[allow]` budget gates **(REVISED — promotion mechanics added)**

**Decision:** Keep the 3-tier scheme. **Add explicit promotion criteria** so crates graduate tier-3 → tier-2 → tier-1 as they mature. Round-2 may extend allow-count gates per the table below; errors-doc gates stay at the four existing tier-1 targets but the *minimum-floor* values are now reviewed quarterly.

**Tiering (16 crates) — same as prior version, see prior table for LOC/churn data.**

**Promotion criteria (NEW):**

- **tier-3 → tier-2** when a crate exceeds **either** 50 commits in any rolling 3-month window **or** 10,000 LOC. Trigger: `make tier-promote-check` (round-2 maniac to add a Makefile target that prints the next crate due for promotion). At promotion time, set the allow-count gate at `current + 1` slack.
- **tier-2 → tier-1** when a crate exceeds **either** 100 commits in any rolling 3-month window **or** 20,000 LOC **or** introduces a new public extension trait. At promotion time, add an errors-doc floor sized to current coverage.
- **Demotion**: never. A gated crate stays gated. Removing a gate is a separate breaking-change decision.

**Rationale (re-derived under industrial framing):**

- "Uniform tier-1 gates everywhere now" was considered. Rejected: enforcing errors-doc floors on crates with 22 commits and 132 LOC (`fraiseql` umbrella) costs more PR friction than it catches regressions. The industrial answer is *graduated* enforcement, not *uniform* enforcement — match the gate cost to the change rate.
- Without promotion criteria, the 3-tier scheme was static and would silently rot as `fraiseql-functions` (46 commits, 10,892 LOC, tier-2) approaches tier-1 thresholds. Making promotion mechanical fixes this.
- Cost: ~25 lines of Makefile (5 new gates + 1 promotion-check target), modelled after `lint-gate-core` (`Makefile:301-309`) and `lint-gate-db` (`Makefile:315-329`).

**Action items for round-2:**

- Add `lint-gate-cli` (max 15), `lint-gate-auth` (max 3), `lint-gate-observers`, `lint-gate-federation`, `lint-gate-arrow`, `lint-gate-secrets`, `lint-gate-functions`, `lint-gate-wire` to `Makefile`, each at `current allow count + 1` slack. Wire into `make check` (currently `Makefile:409`).
- Add `make tier-promote-check`: scans `git log --since=3.months.ago crates/<X>/ | wc -l` and `find crates/<X>/src -name '*.rs' -exec wc -l {} +` for each crate, flags those exceeding next-tier thresholds.
- Decline maniac PRs that add an errors-doc gate to tier-2/tier-3 crates without first lifting the floor to a meaningful value.

**Open issues / things the maniac should NOT touch in round-2 until human review:**

- `fraiseql-wire` has 19 crate-level allows (mostly cast lints for binary protocol decoders). Capping at 20 is fine, but inverting the gate to a specific *denylist* of lints — instead of a *count* — would be more industrial. Flag for human decision; not blocking round-2.

## Q4 — `indexing_slicing` phased rollout pilot **(REVISED — refactor confirmed as policy)**

**Decision:** **Refactor**, not annotate. Pilot the refactor mechanics in `fraiseql-error` (7 hits, single function), then apply to `fraiseql-wire` (70+ hits, security-critical). Each pilot ends with `clippy::indexing_slicing = "deny"` added to that crate's `[lints.clippy]` block. Once 3 crates are clean, propagate workspace-wide via a tracking issue with per-crate sub-tasks.

**Rationale (re-derived under industrial framing):**

- Under "industrial + breaking changes OK," `arr.get(i)?` is unambiguously the right answer over `#[allow] // Reason: …` per-function annotations. Annotation defers the panic-risk surface; refactor removes it. The prior "annotation as cheaper fallback" position was a values hedge that the user has now removed.
- `fraiseql-error` pilot: `crates/fraiseql-error/src/core_error.rs:498-514` is a single `levenshtein_distance` helper with a pre-allocated 2D `Vec<Vec<usize>>` and statically-bounded loop indices. Refactor cost: ~30 min, ~10-line diff, zero API surface change (helper is private). High-confidence dry-run for the mechanic.
- `fraiseql-wire` is the right second target: statically-bounded length-prefix decoder pattern, security-critical (every variable JSON crosses this boundary on every request), and the workspace-rationale block at `Cargo.toml:217-223` already records the case. Spot-check at `protocol/decode/mod.rs:407-425` shows the pattern: switching to `.get(i)?` requires the surrounding `decode_*` fn signature to propagate `Result<…, WireError>`. That's a signature change, but `WireError` already exists; the change is mechanical.
- Breaking-change tolerance unlocks the wire refactor: previously the worry was that downstream consumers of the wire-decode API depended on the panicking signature. They don't (`grep -rn "decode_data_row\|decode_row_description" sdks/`) returns no hits in any SDK — but even if they did, breaking the signature is acceptable now.
- The workspace-wide enable comes only after **3 crates have completed the refactor** (not 1, as before) AND the average effort per crate is recorded (so the remaining 12 crates have a budget estimate).

**Action items for round-2:**

- Phase 0 (pilot mechanics — `fraiseql-error`): refactor `levenshtein_distance` to use `.get(i).and_then(|row| row.get(j))` or restructure with a flat `Vec<usize>` indexed via `row * width + col` (likely cleaner). Add `clippy::indexing_slicing = "deny"` to `crates/fraiseql-error/Cargo.toml` `[lints.clippy]`. Single PR.
- Phase 1 (real pilot — `fraiseql-wire`): refactor `protocol/decode/mod.rs` first (most hits), then `protocol/encode/mod.rs`, then `stream/json_stream/mod.rs`. One PR per file. Each PR records its diff stats (lines changed, sig changes, bench delta if any) in the PR body for the budget estimate.
- Phase 2 (third crate): pick from `fraiseql-secrets` or `fraiseql-storage` (small + low-hit). Cement the pattern.
- Phase 3 (workspace propagation): open a tracking issue listing the remaining 12 crates with sub-task checkboxes. Set workspace-level `clippy::indexing_slicing = "deny"` in `Cargo.toml` `[workspace.lints.clippy]` *only* when the last crate's PR merges. Update the workspace-rationale block at `Cargo.toml:217-223` to record completion.

**Open issues / things the maniac should NOT touch in round-2 until human review:**

- None. Refactor-by-default policy resolves the prior values call.

## Cross-cutting note

The four decisions interact in two places.

**(1) Q1 (delete `RuntimeError`) and Q3 (`fraiseql-error` is tier-3, no gate):** With `RuntimeError` and its sibling enums removed, `fraiseql-error` LOC drops by roughly 40 % (from 1,933 to ~1,150 estimated) and its errors-doc count drops by ~20 entries. Tier-3 still applies — no gate change needed. The size-budget at `Cargo.toml:321` (`fraiseql-error = 5_000`) has plenty of headroom.

**(2) Q2 (freeze async_trait) and Q4 (refactor indexing_slicing):** Both ask "how aggressively do we modernise vs preserve invariants?" Under the new "industrial + breaking changes OK" framing, the asymmetry survives but the reason sharpens: the two "breaking changes" are *different kinds*. Q2's breaking change would **destroy a property** of the architecture (heterogeneous-backend pluggability via `Arc<dyn StorageTrait>` — a property that downstream SDK authors, hypothetical or real, depend on for trait-object registries). Q4's breaking change **gains a property** (no-panic indexing) and the cost is signature changes that propagate `Result` types already in use. Q2's blast radius is *semantic loss*; Q4's blast radius is *signature churn that ends in a safer surface*. Industrial engineering accepts the latter and rejects the former. If the user later wants a single uniform "modernisation tempo" knob, only Q2 changes — and only after RTN-in-`dyn` stabilises. The current asymmetry is **correct, not a hand-wave**.
