# Batch 3 — Security Regression Tests

## Problem

Campaign 1 fixed four critical bug classes. None of them have dedicated
regression tests. The fixes were applied to existing code, but no test exists
that would fail if the bug were silently reintroduced by a future refactor.

This is the most important batch after TS-1. Regression tests for auth bypasses
and injection bugs are the single most effective prevention for a third campaign.

## Philosophy

Each regression test must:

1. **Name the original bug ID** in a doc comment so future readers understand why
   the test exists.
2. **Be minimal** — test exactly the dangerous case, nothing more.
3. **Fail loudly** — not just assert a status code, but assert the specific
   security invariant (e.g., "RLS WHERE clause was applied").
4. **Live in an integration test file**, not a unit test, so it exercises the
   actual middleware stack.

---

## SR-1 — GET GraphQL endpoint must enforce RLS (regresses E1)

**New file**: `crates/fraiseql-server/tests/auth_regression_test.rs`

```rust
//! Regression tests for Campaign 1 auth bypass bugs.
//!
//! SR-1: E1 — GET /graphql passed security_context: None, bypassing RLS
//!       and field-level auth for all unauthenticated GET queries.

/// GET /graphql without Authorization header must return 401 when
/// the schema has at least one RLS policy enabled.
/// Before the E1 fix, this returned 200 with unfiltered data.
#[tokio::test]
async fn get_graphql_without_auth_returns_401_when_rls_enabled() {
    let harness = TestHarness::with_rls_schema().await;

    let response = harness
        .client()
        .get("/graphql?query={users{id}}")
        .send()
        .await;

    assert_eq!(response.status(), 401,
        "E1 regression: GET /graphql must require auth when RLS is enabled");
}

/// GET /graphql with a valid token must still apply field-level scope checks.
/// Before E1, field-level auth was also bypassed via the None context.
#[tokio::test]
async fn get_graphql_with_valid_token_still_enforces_field_scopes() {
    let harness = TestHarness::with_field_scope_schema().await;
    let token = harness.token_without_scope("sensitive:read");

    let response = harness
        .client()
        .get("/graphql?query={users{id,sensitiveField}}")
        .bearer(token)
        .send()
        .await;

    // Must return 200 but with sensitiveField redacted or a field error,
    // not the actual value.
    let body = response.json::<serde_json::Value>().await;
    assert!(
        body["data"]["users"][0]["sensitiveField"].is_null()
            || body["errors"].as_array().is_some(),
        "E1 regression: field scope must be enforced on GET requests"
    );
}
```

---

## SR-2 — Tenant ID must never appear raw in SQL (regresses AA1)

**New file**: `crates/fraiseql-core/tests/tenancy_sql_injection_test.rs`

```rust
//! SR-2: AA1 — Tenant ID was interpolated into SQL via format!(),
//!       enabling cross-tenant access via SQL injection.

use fraiseql_core::tenancy::TenantContext;
use fraiseql_core::db::WhereClauseGenerator;

/// Tenant IDs containing SQL meta-characters must be fully parameterized
/// and must never appear verbatim in the generated SQL string.
#[test]
fn tenant_id_with_sql_metacharacters_is_parameterized_not_interpolated() {
    let malicious_tenant_ids = [
        "'; DROP TABLE users; --",
        "1 OR 1=1",
        "1; SELECT * FROM secrets",
        "tenant' UNION SELECT password FROM admins --",
        "\x00",
    ];

    for tenant_id in &malicious_tenant_ids {
        let ctx = TenantContext::new(tenant_id.to_string());
        let (sql, params) = ctx
            .apply_to_where_clause("users", &[])
            .expect("where clause generation must not fail");

        // The SQL string must not contain the raw tenant_id.
        assert!(
            !sql.contains(tenant_id),
            "AA1 regression: tenant_id `{tenant_id}` appeared raw in SQL: `{sql}`"
        );

        // The tenant_id must appear as a bind parameter.
        assert!(
            params.iter().any(|p| p.as_str() == Some(tenant_id)),
            "AA1 regression: tenant_id `{tenant_id}` was not bound as a parameter"
        );
    }
}

/// Cross-tenant isolation: a query scoped to tenant A must not return rows
/// from tenant B even when tenant IDs are adversarially crafted.
#[tokio::test]
async fn cross_tenant_query_cannot_escape_rls_boundary() {
    let db = TestDatabase::with_two_tenants("tenant_a", "tenant_b").await;
    db.insert_row("tenant_a", "secret_a").await;
    db.insert_row("tenant_b", "secret_b").await;

    let ctx = TenantContext::new("tenant_a".to_string());
    let rows = db.query_as_tenant(&ctx, "SELECT value FROM items").await;

    assert!(
        rows.iter().all(|r| r.tenant_id == "tenant_a"),
        "AA1 regression: tenant_a query returned rows from another tenant"
    );
    assert!(
        !rows.iter().any(|r| r.value == "secret_b"),
        "AA1 regression: cross-tenant data leak detected"
    );
}
```

---

## SR-3 — Twilio webhook signature (regresses O1)

**New file**: `crates/fraiseql-webhooks/tests/twilio_replay_test.rs`

```rust
//! SR-3: O1 — TwilioVerifier used HMAC-SHA1 of body instead of
//!       HMAC-SHA1 of URL+sorted-params. Forged signatures were accepted.

use fraiseql_webhooks::signature::{TwilioVerifier, WebhookVerifier};

// These are real test vectors from the Twilio docs
// (https://www.twilio.com/docs/usage/webhooks/webhooks-security).
// Do not modify — they encode the exact algorithm requirement.
const TWILIO_AUTH_TOKEN: &str = "12345";
const TWILIO_URL: &str = "https://mycompany.com/myapp.php?foo=1&bar=2";
const TWILIO_PARAMS: &[(&str, &str)] = &[
    ("CallSid", "CA1234567890ABCDE"),
    ("Caller", "+14158675309"),
    ("Digits", "1234"),
    ("From", "+14158675309"),
    ("To", "+18005551212"),
];
// Signature from Twilio docs for the above inputs:
const TWILIO_EXPECTED_SIG: &str = "0/KCTR6DLpKmkAf8muzZqo1nDgQ=";

#[test]
fn known_twilio_signature_verifies_correctly() {
    let verifier = TwilioVerifier::new(TWILIO_AUTH_TOKEN);
    let params: Vec<(String, String)> = TWILIO_PARAMS
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let result = verifier.verify(TWILIO_URL, &params, TWILIO_EXPECTED_SIG);
    assert!(result.is_ok(), "O1 regression: known-good Twilio signature rejected: {result:?}");
}

#[test]
fn forged_twilio_signature_is_rejected() {
    let verifier = TwilioVerifier::new(TWILIO_AUTH_TOKEN);
    let params: Vec<(String, String)> = TWILIO_PARAMS
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    // A signature computed over just the body (old wrong algorithm) would be different.
    // Use any invalid value.
    let result = verifier.verify(TWILIO_URL, &params, "AAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    assert!(result.is_err(), "O1 regression: forged Twilio signature was accepted");
}
```

---

## SR-4 — SendGrid ECDSA P-256 signature (regresses O2)

**New file**: `crates/fraiseql-webhooks/tests/sendgrid_replay_test.rs`

```rust
//! SR-4: O2 — SendGridVerifier used HMAC-SHA256 instead of ECDSA P-256.
//!       Any HMAC-signed payload was being accepted as a valid SendGrid webhook.

use fraiseql_webhooks::signature::{SendGridVerifier, WebhookVerifier};
use p256::ecdsa::{SigningKey, Signature, signature::Signer};
use p256::SecretKey;
use rand_core::OsRng;

/// A valid ECDSA P-256 signature over a known payload must verify.
#[test]
fn valid_ecdsa_p256_signature_verifies() {
    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let payload = b"test payload body";
    let timestamp = "1609459200";

    // Compute the message: timestamp + payload (SendGrid's signing input)
    let mut message = timestamp.as_bytes().to_vec();
    message.extend_from_slice(payload);

    let signature: Signature = signing_key.sign(&message);
    let sig_b64 = base64::encode(signature.to_der().as_bytes());

    let verifier = SendGridVerifier::new(verifying_key);
    let result = verifier.verify_with_timestamp(payload, timestamp, &sig_b64);
    assert!(result.is_ok(), "O2 regression: valid ECDSA signature rejected: {result:?}");
}

/// An HMAC-SHA256 signature (the old wrong algorithm) must NOT verify.
/// Before O2 fix, this would have been accepted.
#[test]
fn hmac_sha256_signature_is_rejected_by_ecdsa_verifier() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let payload = b"test payload body";
    let timestamp = "1609459200";

    // Forge an HMAC-SHA256 signature (wrong algorithm)
    let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
    mac.update(timestamp.as_bytes());
    mac.update(payload);
    let forged_sig = base64::encode(mac.finalize().into_bytes());

    let verifier = SendGridVerifier::new(verifying_key);
    let result = verifier.verify_with_timestamp(payload, timestamp, &forged_sig);
    assert!(result.is_err(), "O2 regression: HMAC-SHA256 forged signature accepted by ECDSA verifier");
}
```

---

## SR-5 — Slack timestamp freshness (regresses O4)

**New file**: `crates/fraiseql-webhooks/tests/slack_replay_test.rs`

```rust
//! SR-5: O4 — Slack verifier never checked timestamp freshness.
//!       A captured signature could be replayed indefinitely.
//!
//! This test requires a ManualClock (from Batch 2).

use fraiseql_webhooks::signature::SlackVerifier;
use fraiseql_core::utils::clock::ManualClock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SIGNING_SECRET: &str = "test_signing_secret";

fn slack_signature(secret: &str, timestamp: u64, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let base_string = format!("v0:{}:{}", timestamp, std::str::from_utf8(body).unwrap());
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(base_string.as_bytes());
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn fresh_slack_signature_verifies() {
    let clock = ManualClock::new();
    let now_secs = clock.now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let body = b"payload=test";
    let sig = slack_signature(SIGNING_SECRET, now_secs, body);

    let verifier = SlackVerifier::new_with_clock(SIGNING_SECRET, clock.clone());
    assert!(verifier.verify(body, now_secs, &sig).is_ok(),
        "SR-5 regression: valid fresh Slack signature rejected");
}

#[test]
fn stale_slack_signature_is_rejected_as_replay() {
    let clock = ManualClock::new();
    let original_time = clock.now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    let body = b"payload=test";
    let sig = slack_signature(SIGNING_SECRET, original_time, body);

    // Advance clock past the 5-minute replay window.
    clock.advance(Duration::from_secs(301));

    let verifier = SlackVerifier::new_with_clock(SIGNING_SECRET, clock.clone());
    let result = verifier.verify(body, original_time, &sig);
    assert!(result.is_err(), "SR-5 / O4 regression: stale Slack signature accepted (replay not blocked)");
    // Verify the error is specifically a replay error, not a signature error.
    assert!(matches!(result, Err(e) if e.to_string().contains("replay") || e.to_string().contains("stale")),
        "Error should indicate replay attack, got: {result:?}");
}
```

---

## SR-6 — Discord timestamp freshness (regresses O4)

Same pattern as SR-5, adapted for Discord's `X-Signature-Timestamp` header.
File: `crates/fraiseql-webhooks/tests/discord_replay_test.rs`

Discord's algorithm: `Ed25519(timestamp + body)` with freshness check.
Provide two tests: valid fresh signature passes; same signature after 301 s fails.

---

## SR-7 — PKCE CSRF protection (regresses R2b)

**New file**: `crates/fraiseql-auth/tests/pkce_csrf_regression_test.rs`

```rust
//! SR-7: R2b — authorization_url() generated OAuth state but never returned it,
//!       making CSRF verification impossible.

use fraiseql_auth::oauth::OAuthClient;

/// authorization_url must return the state token that will be validated
/// in the callback. Without this, CSRF protection is impossible.
#[tokio::test]
async fn authorization_url_returns_state_token() {
    let client = OAuthClient::test_client();
    let (url, state) = client.authorization_url().await.expect("must generate URL");
    assert!(!state.is_empty(), "R2b regression: authorization_url returned empty state");
    assert!(url.contains(&state), "state must appear in the authorization URL");
}

/// Callback with wrong state must be rejected as CSRF.
#[tokio::test]
async fn callback_with_mismatched_state_returns_csrf_error() {
    let client = OAuthClient::test_client();
    let (_url, _real_state) = client.authorization_url().await.unwrap();

    let result = client
        .exchange_code("auth_code_123", "wrong_state_value")
        .await;

    assert!(result.is_err(), "R2b regression: CSRF check accepted mismatched state");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("csrf") || err.to_string().contains("state"),
        "Error should be CSRF-related, got: {err}"
    );
}
```

---

## SR-8 — RBAC router requires auth (regresses E2a)

**New file**: `crates/fraiseql-server/tests/rbac_auth_regression_test.rs`

```rust
//! SR-8: E2a — RBAC management router was merged without authentication middleware.

#[tokio::test]
async fn rbac_endpoints_return_401_without_auth_header() {
    let harness = TestHarness::default().await;

    for path in &["/rbac/roles", "/rbac/users", "/rbac/policies"] {
        let response = harness.client().get(path).send().await;
        assert_eq!(
            response.status(), 401,
            "E2a regression: RBAC endpoint {path} returned {} without auth",
            response.status()
        );
    }
}

#[tokio::test]
async fn rbac_endpoints_return_403_without_rbac_admin_scope() {
    let harness = TestHarness::default().await;
    let token = harness.token_with_scopes(&["read:users"]); // not rbac:admin

    for path in &["/rbac/roles", "/rbac/users", "/rbac/policies"] {
        let response = harness.client().get(path).bearer(&token).send().await;
        assert_eq!(
            response.status(), 403,
            "E2a regression: RBAC endpoint {path} accepted token without rbac:admin scope"
        );
    }
}
```

---

## Verification Checklist

- [ ] All 8 regression test files compile with no warnings
- [ ] All tests pass against current codebase (they must — the bugs are fixed)
- [ ] Manually verify SR-1 by temporarily reverting the E1 fix:
      `git stash && cargo test sr_1` should fail; `git stash pop` should restore green
- [ ] Add to CI under a `regression-security` test group that runs on every PR
      touching `crates/fraiseql-server/src/routes/`, `fraiseql-auth/`, `fraiseql-webhooks/`,
      or `fraiseql-core/src/tenancy/`
