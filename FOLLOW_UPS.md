## Open follow-ups

### Q4 workspace `indexing_slicing` rollout — pilot complete, remaining crates queued

**Deferred from:** Wave 9 (commits `e514bbf25` `fraiseql-error`, `4a6c94664`
`fraiseql-wire`, `3c3e16089` `fraiseql-storage`)

**Status:** 3-crate pilot complete per `POLICY_DECISIONS.md` Q4 prerequisite.
`#![deny(clippy::indexing_slicing)]` is now active at the crate root of
`fraiseql-error`, `fraiseql-wire`, and `fraiseql-storage`. Workspace-wide
enable is the next step but is its own multi-wave effort — do NOT enable
`clippy::indexing_slicing = "deny"` in `Cargo.toml` `[workspace.lints.clippy]`
in a single PR.

**Per-crate hit count breakdown** (from `cargo clippy -p <crate> --all-targets
--all-features -- -W clippy::indexing_slicing 2>&1 | grep -c "warning:
(indexing|slicing) may panic"` as of 2026-05-24):

| Crate | Hits (total) | Notes |
|-------|--------------|-------|
| fraiseql-error | 0 ✅ | Wave 9 pilot 1 — `levenshtein_distance` refactor to rolling buffer |
| fraiseql-wire | 0 ✅ | Wave 9 pilot 2 — introduced private `Cursor<'a>` helper for the decoder |
| fraiseql-storage | 0 ✅ | Wave 9 pilot 3 — `serde_json::Value::get()` + slice `.get()..unwrap_or(&[])` |
| fraiseql-webhooks | 8 | Smallest remaining; do this next as a single-PR warmup |
| fraiseql-storage (pre-pilot) | 54 | (closed by Wave 9) |
| fraiseql-federation | 63 | Likely a mix of GraphQL schema indexing and entity batch slicing |
| fraiseql-db | 93 | SQL codegen — indices into column lists, parameter arrays |
| fraiseql-test-utils | 102 | Test fixture helpers — most can be file-level allow |
| fraiseql-auth | 129 | JWT/OIDC token parsing and OAuth state — security-critical, audit each |
| fraiseql-secrets | 144 | Vault/secret backends — audit for SSRF-adjacent indexing on URL parts |
| fraiseql-arrow | 150 | Apache Arrow array indexing — many will be `try_into` for fixed-width |
| fraiseql-functions | 229 | WASM glue + cron scheduler; harder than it looks |
| fraiseql-observers | 248 | Largest of the mid-tier; trigger registry + cron parsing |
| fraiseql-cli | 903 | CLI commands; most are stdout printing of indexed arrays — many file-level allows |
| fraiseql-server | 1322 | HTTP handlers + state — many sites are pre-checked slices |
| fraiseql-core | 1713 | Schema/executor/cache; the heaviest refactor target |

**Total remaining**: ~5,748 hits across 13 crates (the policy doc's 1,015
estimate counted only production code; the figures above include tests,
benches, and examples, which mostly get file-level `#![allow]`).

**Suggested rollout order** (smallest first, lock in mechanics):

1. `fraiseql-webhooks` (8) — half-day
2. `fraiseql-federation` (63) — one wave
3. `fraiseql-db` (93) — one wave (SQL codegen patterns differ from protocol decoders)
4. `fraiseql-test-utils` (102) — mostly file-level allows
5. `fraiseql-auth` (129) — one wave with security focus
6. `fraiseql-secrets` (144) — one wave with SSRF audit
7. `fraiseql-arrow` (150) — `try_into` patterns
8. `fraiseql-functions` (229) — one or two waves
9. `fraiseql-observers` (248) — one or two waves
10. `fraiseql-cli` (903) — two or three waves; mostly file-level allows
11. `fraiseql-server` (1322) — three or four waves; HTTP handler audit
12. `fraiseql-core` (1713) — four or more waves; the heart of the engine

**Pattern catalogue** (from Wave 9 pilot — useful for the rollout):

1. **Rolling buffer**: 2D `Vec<Vec<T>>` matrix → flat `Vec<T>` two-row rolling buffer with `.get(j).copied().unwrap_or(default)`. (Levenshtein in `fraiseql-error`.)
2. **Bounds-checked cursor**: hand-rolled `Cursor<'a>` struct over `&[u8]` with `read_u8`, `read_iNN_be`, `read_slice(n)`, `position_of_null`, `read_until_null`. (Binary decoders in `fraiseql-wire`.)
3. **Fixed-width array from slice**: `slice.get(a..a+N).ok_or(...)?.try_into().expect("slice of length N always converts to [u8; N]")`. Provably-safe `.expect` documented with `// Reason:`. (Wire decoder.)
4. **Slice-pattern destructure**: `match v.as_slice() { [only] => only, _ => Err(...) }` replaces `if v.len() != 1 { Err(...) } let x = &v[0];`. (Wire `json/validate`, `stream/json_stream`.)
5. **serde_json field extraction**: `value["key"]` → `value.get("key").and_then(serde_json::Value::as_str)`. (Storage `gcs.rs`.)
6. **Reserved-slot backfill**: `buf[len_pos..len_pos+4].copy_from_slice(&len)` → `if let Some(slot) = buf.get_mut(len_pos..len_pos+4) { slot.copy_from_slice(&len) }`. (Wire encoder `fill_length` helper.)
7. **Default-fallback slice**: `arr[a..b]` where bounds are checked upstream → `arr.get(a..b).unwrap_or(&[])` with `// Reason:` comment on the invariant. (Storage `local.rs` pagination.)

**`#[allow(clippy::indexing_slicing)]` introduced in Wave 9**: 0 inline
`#[allow]` annotations on production code. All allows are file-level (tests
and examples). Two `.expect("...")` calls in `fraiseql-wire`'s `Cursor` carry
`// Reason:` comments proving the slice→array conversion is statically safe.

**Recommended wave shape for each remaining crate**:

- Audit (`cargo clippy -p <crate> ... -W clippy::indexing_slicing`) → triage
  by file (production vs test/bench/example).
- Test/bench/example files: file-level `#![allow(clippy::indexing_slicing)]`
  - `// Reason:` matching the existing `unwrap_used` allow.
- Production files: apply pattern catalogue items 1–7 above.
- Final step per crate: `#![deny(clippy::indexing_slicing)]` at the crate
  root in `src/lib.rs`, then `cargo clippy -p <crate> --all-targets
  --all-features -- -D warnings` clean and `cargo nextest run -p <crate>
  --lib` green.

**Workspace-wide enable** (`clippy::indexing_slicing = "deny"` in
`Cargo.toml` `[workspace.lints.clippy]`): land **only after all 16 crates
have crate-root `#![deny]`**. At that point the workspace setting is a no-op
but locks in the invariant for any new crate added to the workspace.

---

### F031 expansion — executor DB-bound property coverage

**Deferred from:** Wave 8 (commit fcee0374b)

**Reason deferred:** the 9 property tests in
`crates/fraiseql-core/tests/property/property_executor.rs` cover every
public no-DB executor entry point (`parse_query`, `QueryMatcher::match_query`,
`extract_root_field_names`). The full `Executor::execute` end-to-end
pipeline needs either a testcontainer Postgres bootstrap (too slow for
proptest's case count) or a comprehensive mock `DatabaseAdapter` that
behaves like Postgres under arbitrary WHERE/ORDER/LIMIT/projection.

**Suggested follow-up:** build a deterministic in-memory mock adapter that
implements `DatabaseAdapter` with table-shaped fixtures (rows + RLS policy
fixtures), then add property tests asserting (a) `execute(query, vars)`
never returns rows that violate the RLS policy for the user's
`SecurityContext`, (b) repeated `execute` with the same input + cache
warm yields byte-equal responses, (c) variable type-checking rejects
mistyped inputs without panicking. Multi-day investment; gate on demand
(no in-the-wild bug reports yet).
