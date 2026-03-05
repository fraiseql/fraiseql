# FraiseQL Remediation Plan — Extension 12

**Assessor note**: This extension covers findings not addressed in extensions 1–11.
Benchmarking is out of scope (`velocitybench`).

---

## Rapport d'étonnement — new findings (2026-03-05)

### What works well

The batch of recent commits (`test(wire)`, `test(observers)`, `test(doctests)`) shows genuine
momentum. Specific improvements that stand out:

- The `failover.rs` state transition fix is no longer a TODO comment — the state machine now
  actually advances from `Recovering → Running` when a checkpoint succeeds.
- The `in_memory.rs` switch from unbounded MPSC to bounded MPSC adds correct backpressure
  semantics and removes a latent slow-consumer footgun.
- The `child_span_id` replacement of a counter-based span ID with UUID-derived randomness
  is directionally correct and removes a W3C compliance violation.
- The `async_validators.rs` rename from `MockEmail*`/`MockPhone*` to `EmailFormatValidator`/
  `PhoneE164Validator` correctly signals that these are real local validators, not mocks.

### What surprised us (the bad and the ugly)

The items below are **not** covered by extensions 1–11.

---

## Track N — Security Correctness (Webhooks)

### N1 — Slack and Discord verifiers: timestamp included in HMAC but freshness never checked

**Files**: `crates/fraiseql-webhooks/src/signature/slack.rs:35–38`,
`crates/fraiseql-webhooks/src/signature/discord.rs:29,46–49`

**Observation**:
Both Slack and Discord include a timestamp in their signed payload:

- **Slack**: Signs `v0:<timestamp>:<body>` with HMAC-SHA256.
- **Discord**: Signs `<timestamp><body>` with Ed25519.

Both verifiers correctly require the timestamp to be present (returning
`SignatureError::MissingTimestamp` if absent). However, neither verifier checks whether the
timestamp is *recent*. An attacker who captures a legitimate webhook request can replay it
indefinitely — the signature will always validate because the same timestamp is signed.

Slack's own documentation mandates: "Reject any request whose timestamp is more than five
minutes from local time." Discord's documentation states the same window. This is explicitly
required to prevent replay attacks.

Compare with `StripeVerifier` in the same codebase:
```rust
// stripe.rs:84–88 — correctly checks freshness
if (now - ts).abs() > self.tolerance as i64 {
    return Err(SignatureError::TimestampExpired);
}
```
Stripe has a clock, a tolerance, and checks it. Slack and Discord do not.

**Impact**: Any Slack or Discord webhook can be replayed. An attacker who intercepts a
`payment.created` or `order.placed` webhook can trigger the corresponding database function
repeatedly. This is a security regression relative to what users reasonably expect from a
"signature verified" status.

**Fix**: Follow the `StripeVerifier` pattern for both:
1. Add a `clock: Arc<dyn Clock>` and `tolerance: u64` to `SlackVerifier` and `DiscordVerifier`.
2. Parse the timestamp string to `i64`, compare against `clock.now()`, return
   `SignatureError::TimestampExpired` if `|now - ts| > tolerance`.
3. Default tolerance: 300 seconds (Slack's documented requirement).

---

### N2 — Paddle verifier implements deprecated v1 format; v2 format silently misverifies

**File**: `crates/fraiseql-webhooks/src/signature/paddle.rs`

**Observation**:
`PaddleVerifier` computes HMAC-SHA1 over the raw payload and compares against the header
value (base64-encoded). This matches the **Paddle Classic (v1)** webhook format.

Paddle Billing (v2), launched in 2023 and now the default for all new Paddle accounts,
uses a completely different signature format:
```
Paddle-Signature: ts=1731594silon;h1=<sha256-hmac>
```
The v2 algorithm:
1. Extracts `ts` and `h1` from the header.
2. Concatenates `ts + ":" + body`.
3. HMAC-SHA256 (not SHA1) over the concatenated string.

A Paddle Billing customer who configures the `paddle` provider will receive **`Ok(false)`**
for every valid webhook — not an error, just a silent verification failure — because the
HMAC-SHA1 over the raw body will never match the SHA256 of `"<ts>:<body>"`.

**Impact**: Paddle Billing webhooks silently reject all events (or, if the caller treats
`Ok(false)` as a failed but non-erroring verification, all events are rejected without an
actionable error message). Users debugging this will find no indication that the algorithm is
wrong — the code will report "signature mismatch" for every valid Paddle webhook.

**Fix**: Implement the v2 format. The Paddle Billing header is parseable in the same style as
Stripe:
```rust
let parts: HashMap<&str, &str> = header.split(';')
    .filter_map(|p| { let mut kv = p.splitn(2, '='); Some((kv.next()?, kv.next()?)) })
    .collect();
let ts = parts.get("ts").ok_or(SignatureError::InvalidFormat)?;
let h1 = parts.get("h1").ok_or(SignatureError::InvalidFormat)?;
// signed payload: ts + ":" + body
// algorithm: HMAC-SHA256
```
Optionally support v1 (SHA1) behind a `PaddleVerifier::v1()` constructor for Classic
customers, but make v2 the default.

---

### N3 — `WebhookConfig.timestamp_tolerance` is deserialized but never read

**Files**: `crates/fraiseql-webhooks/src/config.rs:29–30`,
all call sites of `WebhookConfig`

**Observation**:
`WebhookConfig` defines a `timestamp_tolerance: u64` field (default: 300 seconds):
```rust
#[serde(default = "default_timestamp_tolerance")]
pub timestamp_tolerance: u64,
```

The field is correctly deserialized from TOML. However, `grep -rn "timestamp_tolerance"
crates/` shows it is **only referenced in tests** — never in production code. No verifier or
dispatcher reads this value to configure its tolerance.

The user experience: an operator sets `timestamp_tolerance = 60` in `fraiseql.toml`, which
is deserialized silently into the struct, but the per-provider verifiers (Stripe's default is
300 seconds, hardcoded) never consult it.

**Impact**: Operators cannot configure replay-protection windows through the configuration
file. The documented field is dead code. Any operator who sets a tighter window to comply
with a security policy gets no protection.

**Fix**: Thread `timestamp_tolerance` from `WebhookConfig` into the `StripeVerifier::with_tolerance()`
constructor call and into the not-yet-added `SlackVerifier`/`DiscordVerifier` clock/tolerance
configuration. The `ProviderRegistry` should become configurable (accept a tolerance or
config map), or each verifier should be constructed per-webhook-config rather than shared
globally.

---

## Track O — API Design

### O1 — `HmacSha256Verifier`/`HmacSha1Verifier` constructors accept unused configuration

**File**: `crates/fraiseql-webhooks/src/signature/generic.rs:17–29`

**Observation**:
The two generic HMAC verifiers accept `name` and `header` in their constructors:
```rust
pub struct HmacSha256Verifier {
    _name:   String,    // prefixed with _ to silence "unused field" lint
    _header: String,
}
pub fn new(name: &str, header: &str) -> Self { ... }
```

However, the `SignatureVerifier` trait returns `&'static str`:
```rust
fn name(&self) -> &'static str { "hmac-sha256" }  // hardcoded, ignores _name
fn signature_header(&self) -> &'static str { "X-Signature" }  // hardcoded, ignores _header
```

The comments acknowledge this: "This is a limitation — we'd need Box<str> or similar."

The result: a user calling `HmacSha256Verifier::new("my-provider", "X-My-Header")` receives a
verifier that ignores both arguments and always reports `"hmac-sha256"` and `"X-Signature"`.
The constructor is a silent no-op for its two arguments.

**Fix**: The `SignatureVerifier` trait should return `&str` instead of `&'static str`. The
existing implementations (`StripeVerifier`, `GitHubVerifier`, etc.) can continue to return
`&'static str` implicitly (Rust coerces). The generic verifiers can then store and return
their configured names:
```rust
fn name(&self) -> &str { &self.name }
fn signature_header(&self) -> &str { &self.header }
```
This is a minor semver break but corrects a misleading API.

---

### O2 — Webhook mock implementations exported unconditionally in production

**File**: `crates/fraiseql-webhooks/src/lib.rs:37–41`

**Observation**:
```rust
#[cfg(test)]
pub use testing::mocks;
// Also export mocks for integration tests (tests/ directory)
#[cfg(not(test))]
pub use testing::mocks;
```

The conditional and its inverse together unconditionally export `testing::mocks` in all
compilation modes. The comment explains the intent: integration test binaries in the `tests/`
directory are compiled separately from the crate under test, so they don't see `#[cfg(test)]`
items.

However, this means `fraiseql_webhooks::mocks` is part of the public production API:
any downstream binary that depends on `fraiseql-webhooks` can access `MockSignatureVerifier`,
`MockClock`, and `MockSecretProvider` without any test flag. The `testing.rs` module itself
declares `#![allow(clippy::unwrap_used)]` crate-wide.

**Impact**: The mocks become part of the public semver surface. Any dependency on
`fraiseql_webhooks::mocks::MockClock` in production code compiles successfully with no
warning. More practically, the mock module carries `unwrap_used` allowances into any consumer.

**Fix**: Use Cargo's `[features]` mechanism:
```toml
[features]
testing = []
```
Gate the mock exports behind `#[cfg(feature = "testing")]`. Integration tests that need
mocks declare `fraiseql-webhooks = { ..., features = ["testing"] }` in their
`[dev-dependencies]`. Production code cannot accidentally import them.

---

### O3 — Three distinct `WebhookConfig` structs create naming confusion

**Files**:
- `crates/fraiseql-webhooks/src/config.rs` — full config with events, tolerance, idempotency
- `crates/fraiseql-server/src/config/mod.rs:230` — simplified (secret_env, provider, path only)
- `crates/fraiseql-core/src/runtime/subscription/webhook.rs:7` — URL-based outbound webhook

**Observation**:
Three different structs are all named `WebhookConfig`. They live in different crates so they
do not collide at the type level, but they cause significant confusion:

1. The `fraiseql-server` variant lacks all the fields of the `fraiseql-webhooks` variant —
   `events`, `timestamp_tolerance`, `idempotent` are absent. An operator reading the
   `fraiseql-webhooks` documentation would expect to configure these fields, but the server
   never reads them.
2. The `fraiseql-core` variant is an outbound webhook for subscriptions — conceptually
   unrelated to the inbound webhook processing of the other two — yet carries the same name.
3. The `fraiseql-webhooks::WebhookConfig` (the most complete) is deserialized from TOML
   by the server's `ServerConfig::webhooks: HashMap<String, WebhookConfig>`, but this uses
   the *server's own* `WebhookConfig`, not the one from `fraiseql-webhooks`. The full config
   struct is effectively unreachable from the server.

**Fix**: Rename to distinguish the three roles:
- `fraiseql-webhooks::InboundWebhookConfig` (inbound, full-featured)
- `fraiseql-server`: adopt `fraiseql-webhooks::InboundWebhookConfig` directly (rather than its
  own stripped-down struct), so that `events`, `timestamp_tolerance`, and `idempotent` are
  configurable from TOML
- `fraiseql-core::WebhookSubscriptionConfig` (outbound, subscription-based)

---

## Track P — Reliability and Memory

### P1 — `KeyedRateLimiter` HashMap grows without bound; no periodic cleanup

**File**: `crates/fraiseql-auth/src/rate_limiting.rs:83–228`

**Observation**:
`KeyedRateLimiter` stores a `HashMap<String, RequestRecord>` keyed by client identifier
(IP address or user ID). Entries are created on first request (`or_insert_with`) and
updated on each request. There is no mechanism to remove expired entries:

```rust
pub fn check(&self, key: &str) -> Result<()> {
    let mut records = self.records.lock()...;
    let record = records.entry(key.to_string()).or_insert_with(|| RequestRecord {
        count: 0, window_start: now,
    });
    // ... update count/window, never remove old keys
}
```

The only removal is `clear()`, which is documented as "for testing or reset" and
clears the entire map. There is no `cleanup_expired()`, no background task, and no
bound on map size.

In a long-running server exposed to the internet, the `auth_start` limiter (keyed by IP)
will accumulate an entry for every unique source IP that has ever sent a request. With a
5-minute rate-limit window, entries from IPs that last connected in 2024 are held in memory
indefinitely. A DDoS from spoofed IPs (each unique) can exhaust memory.

**Impact**: Unbounded memory growth in a production internet-facing service. The `active_limiters()`
monitoring metric will monotonically increase and never decrease, making it useless as a
health signal.

**Fix**: Add periodic cleanup of expired entries. Two approaches:
1. **On-check eviction**: During each `check()` call, probabilistically (e.g., 1% of calls)
   scan and remove entries whose `window_start + window_secs < now`. Simple, no background
   task.
2. **Background task**: A `tokio::spawn` task that runs every `window_secs` and calls
   `records.retain(|_, r| r.window_start + config.window_secs >= now)`. Cleaner separation.

Either approach should also add a hard cap: if `records.len() > MAX_ENTRIES`, reject the
new entry (return `Err(RateLimited)` defensively).

---

### P2 — Vault `SecretCache` "LRU" eviction is actually random eviction

**File**: `crates/fraiseql-secrets/src/secrets_manager/backends/vault.rs:70–79`

**Observation**:
When the cache reaches capacity, the code removes the "first N entries":
```rust
let keys_to_remove: Vec<_> =
    entries.iter().take(remove_count).map(|(k, _)| k.clone()).collect();
```

The comment says "Simple LRU". However, `HashMap::iter()` returns entries in an unspecified,
non-insertion-order sequence that changes as the map is mutated. This is not LRU (Least
Recently Used) — it is arbitrary eviction. The most-recently-accessed secret is as likely to
be evicted as the least-recently-accessed one.

**Impact**: The cache does not preferentially retain hot secrets. Under the eviction condition,
recently-refreshed credentials can be evicted, causing unnecessary Vault API calls. This
wastes Vault API quota and increases latency. The comment creates false confidence that the
eviction policy is principled.

**Fix**: Either:
- Use `IndexMap` (insertion-ordered) as a simple approximation of LRU (evict oldest
  inserted, not oldest accessed — this is FIFO, but honest).
- Use the `lru` crate for true LRU semantics.
- Rename the comment from "Simple LRU" to "Simple eviction (arbitrary order)" to be honest
  about the actual behavior.

The most pragmatic fix is the comment rename + FIFO via `IndexMap`. True LRU is unlikely
to matter at scale (Vault rate limits are far more constraining than cache eviction policy).

---

## Track Q — Proc Macro Soundness

### Q1 — `#[traced]` macro uses `span.enter()` across `.await` — unsound in async Rust

**File**: `crates/fraiseql-observers-macros/src/lib.rs:55–78`

**Observation**:
The `#[traced]` procedural macro generates the following for async functions:
```rust
fn my_fn(...) -> ... {
    let span = tracing::debug_span!(span_name);
    let _guard = span.enter();    // ← guard created
    let start = std::time::Instant::now();

    let result = async {
        #fn_body                   // ← fn body may contain .await points
    }.await;                       // ← _guard still alive across this .await

    // _guard dropped here
}
```

`_guard` is a `tracing::span::Entered` RAII guard. Holding it across an `.await` point is
explicitly documented as **incorrect** in the `tracing` crate:

> "Do not use `Span::enter` in async code. [...] If the spawned future moves to a different
> thread between poll points, the `Entered` guard — which is `!Send` — would be on the
> original thread while the span is entered on the new thread."

In the tokio work-stealing executor, async tasks may be polled by any thread in the thread
pool. If a task is preempted at an `.await` point inside `#fn_body` and resumed on a
different thread, the tracing span is "entered" on thread A (where `_guard` lives) but the
actual work continues on thread B. This produces incorrect span data (overlapping spans,
wrong parent-child relationships) and can cause subtle panics in tracing subscribers that
assert single-threaded span ownership.

The correct pattern uses `tracing::Instrument::instrument()`:
```rust
let result = async { #fn_body }
    .instrument(span)
    .await;
```

**Note**: The `#[traced]` macro is currently not used anywhere in production code (only
defined). However, it is part of the public API of `fraiseql-observers-macros` and is
explicitly documented with an example for async functions. The next use of `#[traced]` on
an async function will silently introduce the bug.

**Impact**: Incorrect span data, potential panics under tracing subscribers, and `!Send`
guard living across thread boundaries in a multi-threaded executor.

**Fix**: Replace `span.enter()` + `_guard` with `Instrument::instrument()` in the async branch:
```rust
use tracing::Instrument as _;

let result = async { #fn_body }
    .instrument(span)
    .await;
```
The synchronous branch (`if !is_async`) can continue using `span.enter()` (which is correct
for non-async functions).

---

## Track R — Error Hierarchy

### R1 — `oauth.rs` ProviderRegistry returns `Result<_, String>` (12 methods)

**File**: `crates/fraiseql-auth/src/oauth.rs:608–983`

**Observation**:
The `ExternalProviderRegistry` methods all return `Result<_, String>`:
```rust
pub fn register(&self, provider: ExternalAuthProvider) -> Result<(), String> {
    let mut providers = self.providers.lock().map_err(|_| "Lock failed".to_string())?;
    ...
}
pub fn get(&self, name: &str) -> Result<Option<ExternalAuthProvider>, String> { ... }
pub fn disable(&self, name: &str) -> Result<bool, String> { ... }
// ... 9 more methods
```

This is problematic for three reasons:

1. **Type erasure**: The caller receives an opaque `String` error and cannot match on the
   error kind. A lock-poisoning error and a "provider not found" error both arrive as
   `Err(String)`.

2. **Discarded cause**: `map_err(|_| "Lock failed".to_string())` discards the
   `PoisonError<MutexGuard>`, losing the stack trace and the original panic location that
   poisoned the lock.

3. **Inconsistency**: The same crate defines `AuthError` with a full set of variants. The
   registry methods should return `Result<_, AuthError>` like the rest of the crate.

**Fix**: Define a `RegistryError` variant in `AuthError` (or reuse `AuthError::Internal`):
```rust
AuthError::LockPoisoned { message: String }
AuthError::ProviderNotFound { name: String }
```
Change all 12 method signatures to `Result<_, AuthError>`. This is a minor internal
refactor — the registry is not yet exposed through the server's HTTP API.

---

### R2 — SCRAM `calculate_server_signature` panics on empty key

**File**: `crates/fraiseql-wire/src/auth/scram.rs:248–253`

**Observation**:
```rust
fn calculate_server_signature(server_key: &[u8], auth_message: &[u8]) -> Vec<u8> {
    let mut hmac = HmacSha256::new_from_slice(server_key)
        .expect("HMAC key should be valid");  // ← panics if server_key is empty
    ...
}
```

`HmacSha256::new_from_slice` only fails if the key is zero-length (HMAC accepts any
key length, but the underlying digest crate rejects empty keys in some configurations).
The `.expect()` will panic on any code path where `server_key` arrives empty — for example,
if key derivation in `compute_server_key` returned an empty vector due to a bug upstream.

This is in the authentication verification path. A panic here during a SCRAM handshake
will crash the connection handler task. In a tokio-based server, this terminates the
individual task (not the server), but a client that can repeatedly trigger empty-key
conditions can cause a denial of service.

The fix is a single line: change the return type and propagate the error:
```rust
fn calculate_server_signature(
    server_key: &[u8],
    auth_message: &[u8],
) -> Result<Vec<u8>, ScramError> {
    let mut hmac = HmacSha256::new_from_slice(server_key)
        .map_err(|_| ScramError::InvalidKey("server key is empty".to_string()))?;
    hmac.update(auth_message);
    Ok(hmac.finalize().into_bytes().to_vec())
}
```

---

## Priority Order

| ID | Track | Severity | Effort | Description |
|----|-------|----------|--------|-------------|
| N1 | N | **High** | S | Slack/Discord: timestamp used in HMAC but freshness not checked → replay |
| N2 | N | **High** | S | Paddle verifier implements deprecated SHA1 format; v2 silently misverifies |
| Q1 | Q | **High** | S | `#[traced]` macro: `span.enter()` across `.await` is unsound in async |
| R2 | R | **High** | S | SCRAM: `.expect()` on empty HMAC key panics in auth path |
| P1 | P | **Medium** | M | Rate limiter HashMap grows without bound; no expired-entry cleanup |
| N3 | N | **Medium** | S | `WebhookConfig.timestamp_tolerance` is dead code; never read |
| R1 | R | **Medium** | M | 12 OAuth registry methods return `Result<_, String>` instead of `AuthError` |
| O2 | O | **Medium** | S | Webhook mocks unconditionally in public API; should be `feature = "testing"` |
| O1 | O | **Low** | S | `HmacSha256/Sha1Verifier` constructor config is dead (trait forces `&'static str`) |
| O3 | O | **Low** | M | Three `WebhookConfig` structs; naming collision; full config unreachable from server |
| P2 | P | **Low** | S | Vault SecretCache "LRU" comment is wrong; eviction is arbitrary |

Legend: S = hours, M = 1–3 days

---

## Notes on findings from the subagent that did not survive verification

The subagent also flagged `std::thread::spawn` in `run.rs:359` as a "join handle not kept"
issue. On reading the code: the spawn fires once on schema change and breaks out of the loop
immediately, so the thread is self-terminating. The lack of a join handle is acceptable for
a one-shot file watcher notification. This is not flagged here.

The subagent flagged mutex poisoning in `health_checker.rs` (`.expect("buckets mutex
poisoned")`). This pattern was already covered in extensions 1–11 under the general
`.expect()` policy. Not duplicated here.

The `oauth.rs` mutex `map_err(|_| "Lock failed")` discarding the PoisonError is included
as R1 because the problem is the untyped `String` return, not the `.expect()` vs
`map_err` choice per se.
