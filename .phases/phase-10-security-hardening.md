# Phase 10: Security Hardening (S30–S58 Remainder)

## Objective

Close all remaining HIGH and MEDIUM security findings from the S30–S58 audit
campaign. Ships as v2.1.x patch releases on `main` — no feature flags, no
version bump on `dev`.

## Status

[ ] Not Started

## Background

S30–S58 was a ~130-card security audit campaign. About half was already done
during Phases 1–9. The remaining items are grouped by crate below.

## Success Criteria

- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] `cargo nextest run --workspace` passes (target: ≥9245)
- [ ] `cargo deny check` clean
- [ ] `git grep -i "todo\|fixme\|hack"` returns nothing new
- [ ] All HIGH items resolved
- [ ] All MEDIUM items resolved or consciously deferred with ADR

---

## TDD Cycles

### Cycle 1: Vault HTTP body-size guards (S30) + Debug redaction (S32)

**Crate**: `fraiseql-secrets`  
**Files**: `src/secrets_manager/backends/vault/backend.rs`

**RED**: Write tests asserting oversized responses are rejected:

- `vault_approle_rejects_oversized_response` — mock server returns 10MB body, assert `Err`
- `vault_fetch_secret_rejects_oversized_response`
- `vault_token_renewal_rejects_oversized_response`
- `vault_transit_rejects_oversized_response`
- `vault_debug_does_not_expose_token` — assert `format!("{backend:?}")` does not contain the token value
- `vault_token_accessor_is_removed` — compile-time: no `pub fn token()`

**GREEN**:

- Add `MAX_VAULT_RESPONSE_BYTES: usize = 1 * 1024 * 1024` (1 MiB) constant
- Apply `.bytes_limit(MAX_VAULT_RESPONSE_BYTES)` (or manual `take`) on all 4
  response-reading sites (AppRole auth, token renewal, Transit encrypt/decrypt,
  fetch_secret)
- Replace `#[derive(Debug)]` on `VaultBackend` with manual impl that writes
  `VaultBackend { url: "...", token: "[REDACTED]" }`
- Remove `pub fn token(&self)` accessor

**REFACTOR**: Extract a helper `read_vault_response(resp, max_bytes)` to avoid
repeating the limit logic at every call site.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 2: SCRAM key-material zeroization (S38)

**Crate**: `fraiseql-wire`  
**Files**: `src/auth/scram.rs`

**RED**:

- `scram_password_is_zeroized_on_drop` — write a test that drops a `ScramClient`
  and uses `zeroize` test helpers to verify the memory was cleared

**GREEN**:

- Add `zeroize` (with `zeroize_derive` feature) to `fraiseql-wire` dependencies
- Change `ScramClient.password: String` → `ScramClient.password: zeroize::Zeroizing<String>`
- Derive `zeroize::ZeroizeOnDrop` on `ScramClient` (or implement manually)
- Audit for other key-material fields in the SCRAM flow (e.g. `client_key`,
  `server_key` if stored) — wrap those too

**REFACTOR**: No structural change needed.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 3: Auth input caps + `reload_schema` path traversal (S33)

**Crates**: `fraiseql-auth`, `fraiseql-server`  
**Files**: `fraiseql-auth/src/handlers.rs`, `fraiseql-server/src/routes/api/admin.rs`

**RED**:

- `auth_callback_rejects_oversized_code` — POST `/auth/callback` with 8193-char `code`, assert 400
- `auth_callback_rejects_oversized_state` — same for `state`
- `auth_refresh_rejects_oversized_token` — POST `/auth/refresh` with 4097-char token, assert 400
- `reload_schema_rejects_path_traversal` — POST `{"schema_path": "../../etc/passwd"}`, assert 400
- `reload_schema_rejects_absolute_outside_base` — path outside allowed base dir, assert 400

**GREEN**:

- `handlers.rs`: Add `const MAX_AUTH_CODE_BYTES: usize = 8_192` and
  `MAX_REFRESH_TOKEN_BYTES: usize = 4_096`; validate lengths in handler before
  any processing; return `400 Bad Request` with `"code_too_long"` error code
- `admin.rs`: Add `validate_schema_path(path: &Path, base_dir: &Path) -> Result<()>`
  that canonicalizes the path and checks it starts with `base_dir`; thread
  `base_dir` through from `ServerConfig`; apply before `fs::read_to_string`

**REFACTOR**: Consider a shared `input_guards.rs` module in `fraiseql-auth` if
more input caps are added in Cycle 6.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 4: Resource bounds (S34 + S37 + S39)

**Crates**: `fraiseql-observers`, `fraiseql-arrow`, `fraiseql-auth`, `fraiseql-core`  
**Files**:

- `fraiseql-observers/src/event_bridge.rs`
- `fraiseql-arrow/src/subscription.rs`
- `fraiseql-auth/src/pkce.rs`
- `fraiseql-core/src/core_error.rs`

**RED**:

- `event_bridge_spawn_is_must_use` — compile-time lint test via `#[deny(unused_must_use)]`
- `arrow_subscription_channel_is_bounded` — test that sending > N events blocks
  rather than growing unbounded
- `pkce_store_evicts_oldest_on_capacity` — test that inserting entry N+1 evicts
  entry 1 (or returns error) rather than growing forever
- `levenshtein_on_long_input_is_bounded` — call suggestion with 2000-char field
  names, assert completes in < 1ms (time-bounded test)

**GREEN**:

- `event_bridge.rs`: Add `#[must_use = "spawned tasks must be awaited or explicitly dropped"]`
  to `EventBridge::spawn`
- `subscription.rs`: Replace `unbounded_channel()` with `channel(SUBSCRIPTION_CHANNEL_CAPACITY)`
  where `SUBSCRIPTION_CHANNEL_CAPACITY: usize = 1_024`; handle `SendError` gracefully
- `pkce.rs`: Add `const MAX_PKCE_STORE_ENTRIES: usize = 100_000` and enforce in
  `InMemoryPkceStateStore::insert` by evicting the oldest entry (or returning
  `TooManyPendingAuthorizations` error)
- `core_error.rs`: Cap both inputs to Levenshtein at `MAX_SUGGESTION_INPUT_LEN: usize = 128`
  before computing; field names longer than that get no suggestion

**REFACTOR**: Bounded channel capacity should be configurable via `ServerConfig`
(not hardcoded) if per-deployment tuning is expected.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 5: Webhook SSRF + subscription cap (S52)

**Crates**: `fraiseql-webhooks`, `fraiseql-server`  
**Files**: `fraiseql-webhooks/src/` (outbound delivery, if any), subscription manager

**RED**:

- `webhook_delivery_blocks_private_ips` — attempt delivery to `http://192.168.1.1/evil`,
  assert `Err(SsrfBlocked)`
- `webhook_delivery_blocks_loopback` — `http://localhost/evil`, assert `Err`
- `subscription_manager_enforces_per_connection_cap` — open N+1 subscriptions
  from same connection ID, assert N+1 is rejected

**GREEN**:

- Webhooks: Add `validate_webhook_url(url: &Url) -> Result<()>` using
  `reqwest::Url::parse` + `IpAddr::parse` after stripping IPv6 brackets;
  reject private ranges (10/8, 172.16/12, 192.168/16, 127/8, ::1, link-local)
- Subscription manager: Add `MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 100`
  constant; track per-connection count in `SubscriptionManager`; return
  `SubscriptionLimitExceeded` when exceeded

**REFACTOR**: The SSRF validator is the same pattern used in S18-H3 and S19-I2
(federation, vault). Extract a `ssrf_guard::validate_url(url)` utility in
`fraiseql-error` or a new `fraiseql-http-utils` module.

**CLEANUP**: Clippy, fmt, doc.

---

### Cycle 6: Federation table naming + Redis SCAN (S44 + S36 partial)

**Crates**: `fraiseql-federation`, `fraiseql-auth`  
**Files**:

- `fraiseql-federation/src/saga_store.rs`
- `fraiseql-auth/src/` (Redis session store, if it uses `KEYS`)

**RED**:

- `saga_table_name_has_single_prefix` — assert DDL contains `tb_federation_sagas`
  not `tb_tb_federation_sagas`
- `saga_step_table_fk_is_consistent` — assert FK references updated table name
- `cleanup_all_requires_explicit_enable` — assert calling `cleanup_all()` without
  a `CleanupGuard` or test-only flag returns `Err(NotAllowedInProduction)`
- `redis_session_scan_not_keys` — if Redis session store exists: assert no call
  to `CMD("KEYS")` in the implementation (grep-based or mock-based)

**GREEN**:

- `saga_store.rs`: rename `tb_tb_federation_sagas` → `tb_federation_sagas` and
  `tb_tb_federation_saga_steps` → `tb_federation_saga_steps` (and sequences)
  throughout all SQL strings and references. Write a migration SQL fragment.
- `cleanup_all`: add `#[cfg(test)]` or wrap behind a `CleanupPermit` token that
  can only be constructed in `#[cfg(test)]` contexts
- Redis: if `KEYS` is used, replace with cursor-based `SCAN`

**REFACTOR**: The saga store rename is purely cosmetic — only SQL strings change.

**CLEANUP**: Clippy, fmt, doc. Update any snapshot tests that embed the table name.

---

## Dependencies

- Requires: Phase 9 merged to `dev`
- Blocks: nothing (patch releases)
- SpecQL impact: none directly, but S52 webhook SSRF fix is needed before
  SpecQL platform uses FraiseQL webhooks

## Commit strategy

Each cycle → one PR to `dev` → cherry-pick to `main` as patch release.
No single "security mega-PR" — reviewers need focused diffs.
