# FraiseQL Remediation Plan — Extension 13
## fraiseql-secrets: Vault Client Correctness, Lease Renewal Bugs, Encryption Test Coverage

**Date**: 2026-03-05
**Scope**: `crates/fraiseql-secrets/` exclusively
**Previous plans covered**: Extensions 1–12 — this plan is strictly additive.

---

## Context

`fraiseql-secrets` is a 10,758-LOC crate providing field-level encryption and multi-backend
secrets management (Vault, env, file). A targeted read of its source reveals four concrete
correctness issues and a test coverage gap of material size.

---

## Track S1 — VaultBackend: `reqwest::Client` created per call (no connection pooling)

### Evidence

`crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs`

| Line | Call site | What is created |
|------|-----------|-----------------|
| 234 | `VaultBackend::with_approle` | fresh `reqwest::Client` |
| 324 | `VaultBackend::fetch_secret` (called on every secret lookup) | fresh `reqwest::Client` |
| 457 | `VaultBackend::rotate_transit_key` | fresh `reqwest::Client` |
| 502 | `VaultBackend::encrypt_transit` | fresh `reqwest::Client` |

`reqwest::Client` is a connection-pool manager.  The crate's own docs state:
> "The Client holds a connection pool internally, so it is advised that you create one and reuse it."

Creating a new client inside `fetch_secret` means that **every call to `get_secret`,
`get_secret_with_expiry`, `rotate_secret`, `encrypt_transit`, or `rotate_transit_key`
opens a new TCP connection and performs a full TLS handshake to Vault**.

For field-level encryption, each row of a query result that contains encrypted columns
calls `fetch_secret` to obtain the decryption key.  A table with 100 rows and 3 encrypted
columns issues 300 × (TCP + TLS) round-trips to Vault.

### Fix

Move `client: reqwest::Client` into the `VaultBackend` struct, built once at construction
time with appropriate options (TLS verification, timeout).  Share it (clone is cheap — it
clones the inner `Arc`) across all method calls.

```rust
// vault.rs struct declaration
pub struct VaultBackend {
    addr:      String,
    token:     Zeroizing<String>,
    tls_verify: bool,
    namespace: Option<String>,
    client:    reqwest::Client,           // ← add
    cache:     Arc<RwLock<SecretCache>>,
}

// Constructor
pub fn new<S: Into<String>>(addr: S, token: S) -> Self {
    let addr = addr.into();
    let token = token.into();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))  // see Track S2
        .build()
        .expect("reqwest client construction is infallible with valid config");
    VaultBackend { addr, token: Zeroizing::new(token), tls_verify: true,
                   namespace: None, client, cache: ... }
}
```

Remove local `let client = reqwest::Client::builder()...` from `fetch_secret`,
`rotate_transit_key`, `encrypt_transit`, and `with_approle` (use `self.client` instead).

**Severity**: HIGH — latency and file-descriptor exhaustion under load
**Files**: `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs` (all 4 call sites)

---

## Track S2 — VaultBackend: no HTTP timeout on Vault requests

### Evidence

None of the four `reqwest::Client::builder()` chains in `vault.rs` include a `.timeout()`
call.  The `reqwest` default is **no timeout**.  If Vault is slow or unreachable, every
call to `get_secret` or `fetch_secret` blocks its async task indefinitely.

### Impact

FraiseQL-server initialises `fraiseql-secrets` in the hot path for authenticated requests
(field-level decryption).  A hung Vault service therefore blocks the server's Tokio thread
pool.  There is no circuit breaker on the secrets path.

### Fix

Apply the timeout at construction time (Track S1 fix subsumes this):

```rust
reqwest::Client::builder()
    .danger_accept_invalid_certs(!tls_verify)
    .timeout(Duration::from_secs(10))  // configurable; default 10 s
    .connect_timeout(Duration::from_secs(3))
    .build()?
```

Expose `vault_request_timeout_secs` and `vault_connect_timeout_secs` in `VaultConfig`
(already parsed from `fraiseql.toml`).

**Severity**: HIGH — can cause server-wide stall
**Files**: `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs`

---

## Track S3 — `rotate_secret` silently returns stale cached value

### Evidence

`crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs`, lines 185–192:

```rust
async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
    validate_vault_secret_name(name)?;
    // Rotate by requesting new credentials (old lease is implicitly superseded)
    let (new_secret, _) = self.get_secret_with_expiry(name).await?;  // ← hits cache
    Ok(new_secret)
}
```

`get_secret_with_expiry` checks the in-memory `SecretCache` first (line 160):

```rust
if let Some((cached_value, cached_expiry)) = cache.get_with_expiry(name).await {
    return Ok((cached_value, cached_expiry));   // ← cache hit → Vault is NOT called
}
```

The cache TTL is `0.8 × lease_duration` (constant at line 37).  During the first 80% of
the lease lifetime, `rotate_secret` returns the **cached, unchanged** value without
contacting Vault.  The credential is not rotated.

This is particularly dangerous for the intended use-case: if an application suspects that
a credential has been compromised and calls `rotate_secret` explicitly, it receives the
same (compromised) value back.

### Fix

`rotate_secret` must **invalidate the cache entry** before fetching a fresh credential:

```rust
async fn rotate_secret(&self, name: &str) -> Result<String, SecretsError> {
    validate_vault_secret_name(name)?;
    // Evict cached entry to force a real Vault call
    self.cache.read().await.invalidate(name).await;
    let (new_secret, _) = self.get_secret_with_expiry(name).await?;
    Ok(new_secret)
}
```

Add `SecretCache::invalidate(&self, key: &str)` that removes the entry from the `DashMap`
(or `HashMap<String, CachedSecret>`).

**Severity**: HIGH — silent no-op for explicit credential rotation
**Files**: `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs`, `SecretCache`

---

## Track S4 — `LeaseRenewalTask`: doc/code mismatch for renewal threshold

### Evidence

`crates/fraiseql-secrets/src/secrets_manager/mod.rs`

Struct-level doc (line 135):
> "Proactively renews secrets when they are within 20% of their **original TTL**."

Comment inside `renew_expiring_leases` (line 193):
> "// Refresh if less than 20% of the **check interval** remains"

Implementation (lines 194–197):
```rust
if remaining < chrono::Duration::seconds(
    (self.check_interval.as_secs() as f64 * 0.2) as i64,
)
```

The threshold is `0.2 × check_interval`, **not** `0.2 × original_TTL`.  These are
equivalent only when the operator happens to set `check_interval == lease_duration`.

Concrete misconfiguration scenario:

| `check_interval` | `lease_duration` | threshold | effective window |
|---|---|---|---|
| 60 s | 1 h (3600 s) | 12 s | Last 12 s of 1 h lease — nearly no buffer |
| 3600 s | 10 min (600 s) | 720 s | Already past expiry — renewal never fires |

In the second row the `LeaseRenewalTask` never renews the secret because the threshold
(720 s) always exceeds the remaining TTL before the secret has been fetched.

### Fix

Option A (simpler): compare against the actual expiry timestamp obtained from
`get_secret_with_expiry`, renew when `remaining < 0.2 × lease_duration`.  This requires
storing the full lease duration in `SecretCache` alongside the cached value.

Option B (preserve interface): update the struct-level doc to match the implementation,
and add a validation in `LeaseRenewalTask::new` that warns when `check_interval > lease_duration`.

```rust
// In LeaseRenewalTask::new (at minimum)
if check_interval.as_secs() > 3600 {
    warn!(
        check_interval_secs = check_interval.as_secs(),
        "LeaseRenewalTask check_interval is large; secrets with short TTLs may expire \
         before renewal. Consider setting check_interval ≤ the shortest expected TTL."
    );
}
```

**Severity**: MEDIUM — silent misconfiguration; compromised leases may expire unrenewed
**Files**: `crates/fraiseql-secrets/src/secrets_manager/mod.rs`

---

## Track S5 — `fraiseql-secrets/encryption/` submodules: 14 files, ~7 500 LOC, zero tests

### Evidence

```
crates/fraiseql-secrets/src/encryption/
├── mod.rs                (covered by encryption_rotation_test.rs — 13 tests)
├── audit_logging.rs      (~unknown LOC) — zero tests
├── compliance.rs         (800 LOC)      — zero tests
├── credential_rotation.rs(774 LOC)      — zero tests
├── dashboard.rs          (828 LOC)      — zero tests
├── database_adapter.rs   (~unknown LOC) — zero tests
├── error_recovery.rs     (617 LOC)      — zero tests
├── mapper.rs             (~unknown LOC) — zero tests
├── middleware.rs         (597 LOC)      — zero tests
├── performance.rs        (722 LOC)      — zero tests
├── query_builder.rs      (~unknown LOC) — zero tests
├── refresh_trigger.rs    (727 LOC)      — zero tests
├── rotation_api.rs       (832 LOC)      — zero tests
├── schema.rs             (656 LOC)      — zero tests
└── transaction.rs        (~unknown LOC) — zero tests
```

Confirmed by `grep -c "#\[test\]" crates/fraiseql-secrets/src/encryption/*.rs` which
returns 0 for all 14 submodules.

The only encryption test file is `crates/fraiseql-secrets/tests/encryption_rotation_test.rs`
(13 tests) which covers `VersionedFieldEncryption` in `mod.rs` only.

### Impact

These modules include: audit trail recording (`audit_logging`), compliance framework
(`compliance`), the automatic key refresh cycle (`refresh_trigger`), the database column
mapper (`mapper`), the encryption middleware (`middleware`), and transactional encryption
(`transaction`).  All ship with zero test coverage.

### Fix

Create a test file per submodule (or one combined `encryption_coverage_test.rs`), covering
at minimum:
- Happy path (successful encrypt/decrypt round-trip where applicable)
- Error path (key not found, malformed input)
- Any state machine transitions (e.g., `refresh_trigger`'s scheduling logic)
- Any configuration validation (e.g., compliance framework's HIPAA/PCI-DSS presets)

Example structure:
```
crates/fraiseql-secrets/tests/
├── encryption_rotation_test.rs   ← existing
├── encryption_audit_test.rs      ← new
├── encryption_compliance_test.rs ← new
├── encryption_credential_rotation_test.rs ← new
└── encryption_middleware_test.rs ← new
```

**Severity**: MEDIUM — quality and regression-detection gap
**Files**: all 14 files under `crates/fraiseql-secrets/src/encryption/`

---

## Summary

| Track | Severity | File(s) | Nature |
|-------|----------|---------|--------|
| S1 | HIGH | `backends/vault.rs` (×4 sites) | New TCP+TLS per Vault call |
| S2 | HIGH | `backends/vault.rs` | No HTTP timeout → indefinite hang |
| S3 | HIGH | `backends/vault.rs` | `rotate_secret` returns stale cached value |
| S4 | MEDIUM | `secrets_manager/mod.rs` | Lease renewal threshold docs contradict code |
| S5 | MEDIUM | `encryption/*.rs` (14 files) | Zero tests for ~7 500 LOC |
