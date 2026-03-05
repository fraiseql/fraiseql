# FraiseQL — Rapport d'Étonnement & Remediation Plan Extension IV

*Written 2026-03-05. Fifth assessor's findings.*
*Extends the four preceding plans without duplicating them.*
*Benchmarks out of scope (handled by velocitybench).*
*All findings confirmed against HEAD (latest commit: `140eea10c`).*
*Working-tree changes reviewed: `async_validators.rs` (K2 resolved — Mock validators*
*renamed to real implementations), `coordinator.rs` (L1/L2 regression context), etc.*

---

## Executive Summary

The four previous assessors covered documentation accuracy, authentication bypass, stub
modules, SQL injection, frozen date, duplicate structs, and observer subsystem design
flaws. This pass focused on three areas untouched by prior reports: **webhook provider
protocol correctness**, **infrastructure configuration gaps**, and **federation discovery
as feature theater**.

| Category | Count | Severity |
|---|---|---|
| Webhook protocol mismatches | 4 | Critical / High |
| Infrastructure config gaps | 2 | Medium |
| Federation discovery stubs | 1 | High |

The most serious finding is the `fraiseql-webhooks` crate: three of the fifteen provider
implementations compute the signature over the wrong input, making them functionally
useless for real webhook traffic. For Twilio, this is a trait design problem — the
`SignatureVerifier` interface lacks a `url` parameter, making a correct Twilio
implementation architecturally impossible without breaking the trait.

---

## Track O — Webhook Provider Protocol Mismatches (Priority: Critical / High)

These are in `crates/fraiseql-webhooks/src/signature/`. All providers are registered in
`ProviderRegistry::new()` and returned to callers without any "preview" or "experimental"
label. A user selecting "twilio" or "sendgrid" from the registry and verifying a real
webhook will receive incorrect results.

---

### O1 — Twilio Verifier Computes Signature Over Wrong Input (Critical)

**File:** `crates/fraiseql-webhooks/src/signature/twilio.rs`

**Problem:**

The Twilio verifier signs the raw request body:

```rust
let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
    .map_err(|e| SignatureError::Crypto(e.to_string()))?;
mac.update(payload);  // ← raw body only
```

The actual Twilio signature algorithm (from Twilio's official documentation) is:

1. Take the full request URL (including query string)
2. If the request is `application/x-www-form-urlencoded`, sort all POST parameters
   alphabetically by key and append each `key=value` pair to the URL (no separator)
3. Compute `HMAC-SHA1(auth_token, constructed_string)`
4. Base64-encode the result

**Impact:** The current implementation will:
- **Reject valid Twilio webhooks** — real Twilio requests carry form-encoded parameters
  (CallSid, From, To, etc.), producing a URL+params string that differs from raw body
- **Accept forged webhooks** — any HMAC-SHA1 over just the raw body bytes matches the
  implementation, regardless of URL or parameter authenticity

**Structural problem:** The `SignatureVerifier` trait does not include a `url` parameter:

```rust
fn verify(
    &self,
    payload: &[u8],
    signature: &str,
    secret: &str,
    timestamp: Option<&str>,   // ← no url parameter
) -> Result<bool, SignatureError>;
```

A correct Twilio implementation requires the full request URL. Without a `url` field in
the trait, correct Twilio verification is architecturally impossible.

**Fix — Option A (recommended): Extend the trait:**

```rust
fn verify(
    &self,
    payload: &[u8],
    signature: &str,
    secret: &str,
    timestamp: Option<&str>,
    url: Option<&str>,          // ← add this
) -> Result<bool, SignatureError>;
```

All existing providers ignore `url`; only Twilio uses it. Default: `None`.

Update `TwilioVerifier::verify` to:

1. Require `url` (return `InvalidFormat` if absent)
2. Parse the form body into key-value pairs
3. Sort alphabetically
4. Build `url + concat(sorted_kv_pairs)`
5. HMAC-SHA1 and Base64-encode

**Fix — Option B (short-term):** Remove `TwilioVerifier` from `ProviderRegistry` and
mark it with a doc comment:

```rust
/// # Warning
/// `TwilioVerifier` implements an incorrect signature algorithm and will
/// reject all valid Twilio webhooks. Do not use in production.
/// Tracking issue: #XXXX
```

**Acceptance:**
- A real Twilio webhook (captured via Twilio CLI) passes verification, OR
- `ProviderRegistry::new()` does not register a "twilio" provider until fixed

---

### O2 — SendGrid Verifier Uses HMAC-SHA256 Instead of ECDSA (Critical)

**File:** `crates/fraiseql-webhooks/src/signature/sendgrid.rs`

**Problem:**

```rust
// What the verifier does:
let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
    .map_err(|e| SignatureError::Crypto(e.to_string()))?;
mac.update(payload);
let expected = hex::encode(mac.finalize().into_bytes());
Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
```

The actual SendGrid Event Webhook signature scheme:

- Header: `X-Twilio-Email-Event-Webhook-Signature` — contains a **base64-encoded ECDSA
  signature** over `(timestamp + body_bytes)` using the **P-256 (secp256r1)** curve
- Header: `X-Twilio-Email-Event-Webhook-Timestamp` — seconds since epoch (for replay
  protection), must be included in the signed message
- Key format: The "secret" is a **public key** provided by SendGrid in the developer
  portal, not a shared HMAC secret

The implementation uses HMAC-SHA256 with the key treated as a symmetric secret. This is
the wrong algorithm, the wrong key type, the wrong encoding, and ignores the timestamp
component entirely.

**Impact:**
- **All real SendGrid webhooks will fail verification** — HMAC-SHA256 of a public key
  string will never match an ECDSA signature
- **The "secret" field semantics are wrong** — the caller should provide the ECDSA public
  key, not an HMAC secret; documentation of this provider misleads users

**Fix:**

Use ECDSA P-256 verification with the `p256` crate (already available in the Rust
cryptography ecosystem):

```rust
use base64::{engine::general_purpose::STANDARD, Engine};
use p256::{
    ecdsa::{Signature, VerifyingKey, signature::Verifier},
    pkcs8::DecodePublicKey,
};

pub struct SendGridVerifier;

impl SignatureVerifier for SendGridVerifier {
    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        public_key_pem: &str,  // PEM-encoded P-256 public key from SendGrid portal
        timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;

        // Signed message: timestamp_bytes + payload
        let mut message = timestamp.as_bytes().to_vec();
        message.extend_from_slice(payload);

        // Decode public key (PEM format from SendGrid)
        let verifying_key = VerifyingKey::from_public_key_pem(public_key_pem)
            .map_err(|e| SignatureError::Crypto(format!("invalid public key: {e}")))?;

        // Decode base64 signature
        let sig_bytes = STANDARD.decode(signature)
            .map_err(|e| SignatureError::Crypto(format!("invalid signature encoding: {e}")))?;

        let sig = Signature::from_slice(&sig_bytes)
            .map_err(|e| SignatureError::Crypto(format!("invalid ECDSA signature: {e}")))?;

        Ok(verifying_key.verify(&message, &sig).is_ok())
    }
}
```

**Acceptance:**
- A real SendGrid Event Webhook payload (captured via ngrok + SendGrid test) passes
  verification, OR
- `ProviderRegistry::new()` does not register a "sendgrid" provider until fixed
- `cargo doc -p fraiseql-webhooks` correctly documents that `secret` is an ECDSA
  public key, not an HMAC secret

---

### O3 — Paddle Verifier Implements Paddle Classic API, Not Paddle Billing (High)

**File:** `crates/fraiseql-webhooks/src/signature/paddle.rs`

**Problem:**

The implementation uses HMAC-SHA1 with Base64 output:

```rust
let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())...
let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());
```

This matches the **Paddle Classic (v1)** webhook signature format, which was deprecated
when Paddle launched Paddle Billing (v2) in 2023.

The current Paddle Billing signature format uses:
- Header: `Paddle-Signature` — contains `ts=<timestamp>;h1=<hex_hmac_sha256>`
- Algorithm: HMAC-SHA256 (not SHA1)
- Signed payload: `<timestamp>:<raw_body>`
- Format: Similar to Stripe's format, with timestamp for replay protection

**Impact:**
- New Paddle customers using Paddle Billing will find verification always fails
- Paddle Classic customers (legacy) will work correctly
- The provider name "paddle" with no version qualification is misleading

**Fix:**

Implement Paddle Billing (v2) as the default, matching their current documentation:

```rust
pub struct PaddleBillingVerifier {
    tolerance: u64,
    clock:     Arc<dyn Clock>,
}

impl SignatureVerifier for PaddleBillingVerifier {
    fn signature_header(&self) -> &'static str { "Paddle-Signature" }

    fn verify(&self, payload: &[u8], signature: &str, secret: &str,
              _timestamp: Option<&str>) -> Result<bool, SignatureError> {
        // Parse: ts=<ts>;h1=<hex>
        let parts: HashMap<&str, &str> = signature.split(';')
            .filter_map(|p| { let mut kv = p.splitn(2, '='); Some((kv.next()?, kv.next()?)) })
            .collect();
        let ts = parts.get("ts").ok_or(SignatureError::InvalidFormat)?;
        let h1 = parts.get("h1").ok_or(SignatureError::InvalidFormat)?;
        // Verify timestamp freshness
        let ts_val: i64 = ts.parse().map_err(|_| SignatureError::InvalidFormat)?;
        if (self.clock.now() - ts_val).abs() > self.tolerance as i64 {
            return Err(SignatureError::TimestampExpired);
        }
        // HMAC-SHA256 over "ts:body"
        let signed = format!("{}:{}", ts, String::from_utf8_lossy(payload));
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());
        Ok(constant_time_eq(h1.as_bytes(), expected.as_bytes()))
    }
}
```

Rename the existing implementation to `PaddleClassicVerifier` and register both:

```rust
providers.insert("paddle".into(), Arc::new(PaddleBillingVerifier::new())); // current
providers.insert("paddle-classic".into(), Arc::new(PaddleClassicVerifier)); // legacy
```

**Acceptance:**
- A real Paddle Billing webhook passes with "paddle" provider
- A real Paddle Classic webhook passes with "paddle-classic" provider
- The registry name "paddle" refers to the current API

---

### O4 — Slack and Discord Providers Include Timestamps in Signed Messages but Perform No Freshness Check (Medium)

**Files:**
- `crates/fraiseql-webhooks/src/signature/slack.rs`
- `crates/fraiseql-webhooks/src/signature/discord.rs`

**Problem:**

Both providers correctly include the timestamp in the signed message (preventing
tampering with the timestamp value). However, neither checks that the timestamp is
*recent* — a valid signature from an old webhook can be replayed indefinitely.

Slack's documentation recommends rejecting requests older than 5 minutes.
Discord's documentation requires signature verification only but does not mandate
freshness, though replay is still a risk in practice.

**Slack verifier** (`SlackVerifier::verify`):
- Requires timestamp from a separate header (`X-Slack-Request-Timestamp`)
- Correctly uses `timestamp` in the signed string: `v0:timestamp:body`
- Does NOT check that `timestamp` falls within an acceptable window

**Discord verifier** (`DiscordVerifier::verify`):
- Requires timestamp from `X-Signature-Timestamp`
- Correctly includes it in the signed message
- Does NOT check freshness

**Impact:**

A valid Slack/Discord webhook captured and replayed hours or days later will pass
signature verification. For event-driven systems (e.g., a Slack slash command that
deploys infrastructure), this is a replay attack surface.

**Fix:**

Add a `Clock` dependency and freshness check (same pattern as `StripeVerifier`):

```rust
pub struct SlackVerifier {
    clock:     Arc<dyn Clock>,
    tolerance: u64, // seconds, default 300
}

impl SignatureVerifier for SlackVerifier {
    fn verify(&self, payload, signature, secret, timestamp) -> Result<bool, SignatureError> {
        let ts = timestamp.ok_or(SignatureError::MissingTimestamp)?;
        let ts_val: i64 = ts.parse().map_err(|_| SignatureError::InvalidFormat)?;

        // Freshness check
        if (self.clock.now() - ts_val).abs() > self.tolerance as i64 {
            return Err(SignatureError::TimestampExpired);
        }

        // Existing HMAC verification...
    }
}
```

**Acceptance:**
- A Slack webhook with a timestamp older than 5 minutes returns `Err(TimestampExpired)`
- Test: `SlackVerifier::with_clock(MockClock::new(now + 600))` → `Err(TimestampExpired)`
- A valid, fresh Slack webhook still passes

---

## Track P — Infrastructure Configuration Gaps (Priority: Medium)

---

### P1 — Arrow Flight Port Hardcoded at 50051, Not Configurable (Medium)

**File:** `crates/fraiseql-server/src/server/lifecycle.rs:86`

**Problem:**

```rust
// Flight server runs on port 50051
let flight_addr = "0.0.0.0:50051".parse().expect("Valid Flight address");
```

The Arrow Flight gRPC server binds on port 50051 unconditionally. There is no:
- `flight_port` field in `ServerConfig`
- `FRAISEQL_FLIGHT_PORT` environment variable
- CLI flag `--flight-port`

**Consequences:**
- Users running two FraiseQL instances on the same host (e.g., blue-green deployment,
  staging and production on the same VM) cannot avoid a port conflict without modifying
  source code and recompiling
- Environments where 50051 is reserved (e.g., occupied by another gRPC service or blocked
  by firewall rules) have no workaround
- The `ServerConfig::bind_addr` configures the HTTP port but has no sibling for gRPC,
  creating an asymmetric configuration story

**Fix:**

Add a `flight_bind_addr` field to `ServerConfig`:

```rust
/// Address for the Arrow Flight gRPC server.
///
/// Only used when the `arrow` feature is enabled.
/// Default: `0.0.0.0:50051`
pub flight_bind_addr: std::net::SocketAddr,
```

Expose via environment variable:

```rust
// In config/env.rs:
if let Ok(addr) = env::var("FRAISEQL_FLIGHT_BIND_ADDR") {
    config.flight_bind_addr = addr.parse().map_err(|_| ConfigError::InvalidValue {
        key: "FRAISEQL_FLIGHT_BIND_ADDR".into(),
        value: addr,
        reason: "must be a valid socket address (e.g. 0.0.0.0:50051)".into(),
    })?;
}
```

Update `lifecycle.rs`:

```rust
let flight_addr = self.config.flight_bind_addr;
info!("Arrow Flight server listening on grpc://{}", flight_addr);
```

**Acceptance:**
- `FRAISEQL_FLIGHT_BIND_ADDR=0.0.0.0:50052 fraiseql-server` binds gRPC on 50052
- `ServerConfig::default().flight_bind_addr` returns `0.0.0.0:50051`
- `cargo test` for the config module verifies env var override

---

### P2 — Cryptographic RNG Inconsistency: `thread_rng` in Security-Sensitive Paths (Low-Medium)

**Files:**
- `crates/fraiseql-wire/src/auth/scram.rs:61` — SCRAM client nonce
- `crates/fraiseql-auth/src/provider.rs:165` — PKCE verifier generation
- `crates/fraiseql-auth/src/session.rs:134` — refresh token generation
- `crates/fraiseql-server/src/secrets/mod.rs:217` — AES-GCM nonce generation

**Problem:**

`rand::thread_rng()` in rand 0.8 uses `ChaCha8Rng` seeded from the OS and **is**
cryptographically secure. This is not a correctness bug.

However:

1. **Internal consistency**: The same codebase uses `rand::rngs::OsRng` for PKCE and
   CSRF generation (`fraiseql-auth/src/pkce.rs`, `handlers.rs`) with explicit comments
   explaining the security choice. Using `thread_rng` in adjacent security-sensitive
   code (`provider.rs` generates the PKCE verifier with `thread_rng` while `pkce.rs`
   generates PKCE keys with `OsRng`) creates confusion about which RNG is intentional.

2. **Security audit failure mode**: An automated security review that scans for `OsRng`
   to identify all cryptographic randomness will miss the `thread_rng` usages and
   produce an incomplete map.

3. **The security tests in `fraiseql-auth` explicitly document**: "OsRng should be used
   for cryptographic randomness, not thread_rng" — and then `provider.rs` uses
   `thread_rng` for the PKCE verifier.

4. **Within `fraiseql-auth/src/provider.rs`**: The function comment says "uses
   `rand::thread_rng()` which is cryptographically secure on all major platforms" — this
   is accurate but contradicts the test that says OsRng should be used.

**Fix:**

Standardize on `OsRng` for all security-sensitive random generation:

```rust
// scram.rs — nonce generation
use rand::{RngCore, rngs::OsRng};
let mut nonce_bytes = [0u8; 24];
OsRng.fill_bytes(&mut nonce_bytes);

// session.rs — refresh token
use rand::{RngCore, rngs::OsRng};
let mut random_bytes = [0u8; 32];
OsRng.fill_bytes(&mut random_bytes);
base64::engine::general_purpose::STANDARD.encode(&random_bytes)

// provider.rs — PKCE verifier character selection
// Use OsRng::fill_bytes and index into CHARSET, not rng.gen_range()

// secrets/mod.rs — AES-GCM nonce
use rand::{RngCore, rngs::OsRng};
OsRng.fill_bytes(&mut nonce);
```

Remove the contradictory comment in `provider.rs` and update the one in `security_tests.rs`
to reflect that `thread_rng` has been replaced.

**Acceptance:**
- `grep -rn "thread_rng" crates/ --include="*.rs"` returns only benchmark or clearly
  non-security code
- `fraiseql-auth/src/security_tests.rs` test `test_csrf_state_is_cryptographically_random`
  no longer contradicts `provider.rs`'s actual implementation

---

## Track Q — Federation Discovery as Feature Theater (Priority: High)

---

### Q1 — Federation Subgraph Discovery and Graph Endpoints Return Hardcoded Stubs (High)

**Files:**
- `crates/fraiseql-server/src/routes/api/federation.rs:72–75` — `subgraphs_handler`
- `crates/fraiseql-server/src/routes/api/federation.rs:97–220` — `graph_handler`

**Problem:**

These two endpoints are registered in the production router and return hardcoded data:

```rust
// subgraphs_handler
// Placeholder: Return empty list
let response = SubgraphsResponse { subgraphs: vec![] };  // ← always empty

// graph_handler — delegates to:
fn generate_json_graph() -> String {
    r#"{"subgraphs": [], "edges": []}"#.to_string()  // ← always empty
}

fn generate_dot_graph() -> String {
    r#"digraph federation {
  // Subgraphs would be added here
  // Example: ..."#.to_string()               // ← comment-only template
}
```

These are distinct from M4 (`federation_health_handler`, covered in Extension III):
- M4 covers the health endpoint (`GET /health/federation`) that always returns 200 healthy
- Q1 covers the discovery endpoints (`GET /api/v1/federation/subgraphs` and
  `GET /api/v1/federation/graph`) that always return empty structures

**Impact:** A federation operator calling `GET /api/v1/federation/subgraphs` to enumerate
active subgraphs for monitoring or traffic routing receives an empty list regardless of
the actual federation configuration. Tools built on this API will silently misconfigure
themselves.

The federation circuit breaker is a real, working implementation in
`crates/fraiseql-server/src/federation/`. The `AppState` already holds a reference to
the circuit breaker via `CompiledSchema::federation`. The discovery API simply never
reads it.

**Fix:**

Connect `subgraphs_handler` to the actual federation state:

```rust
pub async fn subgraphs_handler<A: DatabaseAdapter>(
    State(state): State<AppState<A>>,
) -> Result<Json<ApiResponse<SubgraphsResponse>>, ApiError> {
    let federation_config = state.executor.schema().federation.as_ref();

    let subgraphs = federation_config
        .and_then(|cfg| cfg["subgraphs"].as_array())
        .map(|arr| arr.iter().filter_map(|s| {
            Some(SubgraphInfo {
                name:     s["name"].as_str()?.to_string(),
                url:      s["url"].as_str()?.to_string(),
                entities: s["entities"].as_array()
                              .unwrap_or(&vec![])
                              .iter()
                              .filter_map(|e| e.as_str().map(|s| s.to_string()))
                              .collect(),
                healthy:  true, // TODO: query circuit breaker for status
            })
        }).collect::<Vec<_>>())
        .unwrap_or_default();

    Ok(Json(ApiResponse {
        status: "success".to_string(),
        data:   SubgraphsResponse { subgraphs },
    }))
}
```

**Acceptance:**
- A server started with a federation-enabled schema returns actual subgraph names and URLs
  from `GET /api/v1/federation/subgraphs`
- `generate_json_graph()`, `generate_dot_graph()`, `generate_mermaid_graph()` are replaced
  with implementations that read from the schema's federation metadata
- No comment-only content appears in the generated DOT or Mermaid output

---

## Interaction with Existing Plans

| This plan | Existing plan |
|---|---|
| O1 (Twilio wrong input) | New; unrelated to E1/E2 auth bypass |
| O2 (SendGrid wrong algorithm) | New; more severe than K2 (mock validators) |
| O3 (Paddle v1/v2 confusion) | Complements F1/F2 (incomplete features) |
| O4 (Slack/Discord no freshness) | Complements G5 (silent config swallow) |
| P1 (Flight port hardcoded) | New; no analogue in existing plans |
| P2 (thread_rng inconsistency) | Complements G7 (fraiseql-auth allows) |
| Q1 (federation discovery stubs) | Extends M4 (federation health stub) |

---

## Execution Order

### Immediate (security and functional correctness)

1. **O1** — Fix or remove Twilio verifier (verifier silently accepts forged webhooks)
2. **O2** — Fix or remove SendGrid verifier (verifier rejects all real webhooks)

### Week 1 (alongside Track E from extension 1)

3. **O3** — Implement Paddle Billing verifier; rename existing to `paddle-classic`
4. **O4** — Add freshness checks to Slack and Discord verifiers
5. **Q1** — Connect federation discovery endpoints to actual schema metadata

### Week 2 (alongside Track G from extension 1)

6. **P1** — Add `flight_bind_addr` to `ServerConfig` with env var override
7. **P2** — Replace `thread_rng` with `OsRng` in security-sensitive paths

---

## Definition of Done (Extension IV)

The remediation is complete for this set of findings when, in addition to the preceding
plans' definitions of done:

1. A real Twilio webhook (with form-encoded parameters) passes `TwilioVerifier::verify`,
   OR "twilio" is absent from `ProviderRegistry::new()`
2. A real SendGrid Event Webhook passes `SendGridVerifier::verify` using ECDSA P-256,
   OR "sendgrid" is absent from `ProviderRegistry::new()`
3. `ProviderRegistry::new()` registers "paddle" pointing to the Paddle Billing
   implementation and "paddle-classic" pointing to the HMAC-SHA1 implementation
4. A Slack webhook with a timestamp more than 5 minutes old returns
   `Err(SignatureError::TimestampExpired)` (not `Ok(true)`)
5. `FRAISEQL_FLIGHT_BIND_ADDR=127.0.0.1:9090` causes the Arrow Flight server to bind
   on port 9090 instead of 50051
6. `grep -rn "thread_rng" crates/ --include="*.rs"` returns no results in
   `scram.rs`, `session.rs`, `provider.rs`, or `secrets/mod.rs`
7. `GET /api/v1/federation/subgraphs` returns the actual subgraph list from the
   compiled schema (not an empty array) when federation is configured
