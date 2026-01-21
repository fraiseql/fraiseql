# Phase 3: Webhook Runtime

## Objective

Implement the webhook runtime with support for 15+ providers, signature verification, event routing, and idempotency handling.

---

## 3.0 Transaction Boundaries & Testing Seams

### Transaction Semantics

Webhook processing uses the following transaction boundaries:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Signature Verification (No Transaction)                  │
│    - Verify signature before ANY database work              │
│    - Reject invalid signatures immediately                  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Idempotency Check (Read-Only Transaction)                │
│    - Check if event already processed                       │
│    - Return early for duplicates                            │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. Event Processing (Single Transaction - CRITICAL)         │
│    BEGIN TRANSACTION                                        │
│    ├── Insert idempotency record (pending)                  │
│    ├── Execute event handler (DB function)                  │
│    ├── Update idempotency record (success/failed)           │
│    COMMIT / ROLLBACK                                        │
└─────────────────────────────────────────────────────────────┘
```

**Critical**: The idempotency record and event handler MUST be in the same transaction. Otherwise:
- If handler succeeds but idempotency update fails → duplicate processing on retry
- If idempotency records but handler fails → event marked processed but not handled

### Task: Transaction boundary configuration

```rust
// crates/fraiseql-webhooks/src/transaction.rs

use sqlx::{PgPool, Postgres, Transaction};

/// Transaction isolation levels for webhook processing
#[derive(Debug, Clone, Copy, Default)]
pub enum WebhookIsolation {
    /// Read Committed - default, good for most cases
    #[default]
    ReadCommitted,
    /// Repeatable Read - for handlers that read-then-write
    RepeatableRead,
    /// Serializable - for handlers with complex consistency requirements
    Serializable,
}

impl WebhookIsolation {
    pub fn as_sql(&self) -> &'static str {
        match self {
            Self::ReadCommitted => "READ COMMITTED",
            Self::RepeatableRead => "REPEATABLE READ",
            Self::Serializable => "SERIALIZABLE",
        }
    }
}

/// Execute webhook handler within a transaction
pub async fn execute_in_transaction<F, T>(
    pool: &PgPool,
    isolation: WebhookIsolation,
    f: F,
) -> Result<T, WebhookError>
where
    F: for<'c> FnOnce(&'c mut Transaction<'_, Postgres>) -> BoxFuture<'c, Result<T, WebhookError>>,
{
    let mut tx = pool.begin().await
        .map_err(|e| WebhookError::Database(e.to_string()))?;

    // Set isolation level
    sqlx::query(&format!("SET TRANSACTION ISOLATION LEVEL {}", isolation.as_sql()))
        .execute(&mut *tx)
        .await
        .map_err(|e| WebhookError::Database(e.to_string()))?;

    let result = f(&mut tx).await;

    match result {
        Ok(value) => {
            tx.commit().await
                .map_err(|e| WebhookError::Database(e.to_string()))?;
            Ok(value)
        }
        Err(e) => {
            // Explicit rollback (also happens on drop, but be explicit)
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}
```

### Task: Define testing seams for all external dependencies

```rust
// crates/fraiseql-webhooks/src/traits.rs

use async_trait::async_trait;
use serde_json::Value;

/// Signature verification abstraction for testing
#[async_trait]
pub trait SignatureVerifier: Send + Sync {
    fn name(&self) -> &'static str;
    fn signature_header(&self) -> &'static str;
    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        timestamp: Option<&str>,
    ) -> Result<bool, SignatureError>;
    fn extract_timestamp(&self, signature: &str) -> Option<i64> {
        None
    }
}

/// Idempotency store abstraction for testing
#[async_trait]
pub trait IdempotencyStore: Send + Sync {
    async fn check(&self, provider: &str, event_id: &str) -> Result<bool, WebhookError>;
    async fn record(
        &self,
        provider: &str,
        event_id: &str,
        event_type: &str,
        status: &str,
    ) -> Result<uuid::Uuid, WebhookError>;
    async fn update_status(
        &self,
        provider: &str,
        event_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), WebhookError>;
}

/// Secret provider abstraction for testing
#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn get_secret(&self, name: &str) -> Result<String, WebhookError>;
}

/// Event handler abstraction for testing
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle(
        &self,
        function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Value, WebhookError>;
}

/// Clock abstraction for testing timestamp validation
pub trait Clock: Send + Sync {
    fn now(&self) -> i64;
}
```

### Task: Implement mock implementations for testing

```rust
// crates/fraiseql-webhooks/src/testing.rs

#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;

    /// Mock signature verifier that always succeeds or fails based on configuration
    pub struct MockSignatureVerifier {
        pub should_succeed: bool,
        pub calls: Mutex<Vec<MockVerifyCall>>,
    }

    #[derive(Debug, Clone)]
    pub struct MockVerifyCall {
        pub payload: Vec<u8>,
        pub signature: String,
    }

    impl MockSignatureVerifier {
        pub fn succeeding() -> Self {
            Self {
                should_succeed: true,
                calls: Mutex::new(Vec::new()),
            }
        }

        pub fn failing() -> Self {
            Self {
                should_succeed: false,
                calls: Mutex::new(Vec::new()),
            }
        }

        pub fn get_calls(&self) -> Vec<MockVerifyCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl SignatureVerifier for MockSignatureVerifier {
        fn name(&self) -> &'static str { "mock" }
        fn signature_header(&self) -> &'static str { "X-Mock-Signature" }

        fn verify(
            &self,
            payload: &[u8],
            signature: &str,
            _secret: &str,
            _timestamp: Option<&str>,
        ) -> Result<bool, SignatureError> {
            self.calls.lock().unwrap().push(MockVerifyCall {
                payload: payload.to_vec(),
                signature: signature.to_string(),
            });
            Ok(self.should_succeed)
        }
    }

    /// Mock idempotency store with in-memory storage
    pub struct MockIdempotencyStore {
        events: Mutex<HashMap<(String, String), IdempotencyRecord>>,
    }

    #[derive(Debug, Clone)]
    pub struct IdempotencyRecord {
        pub id: uuid::Uuid,
        pub event_type: String,
        pub status: String,
        pub error: Option<String>,
    }

    impl MockIdempotencyStore {
        pub fn new() -> Self {
            Self {
                events: Mutex::new(HashMap::new()),
            }
        }

        /// Pre-populate with existing events for testing duplicates
        pub fn with_existing_events(events: Vec<(&str, &str)>) -> Self {
            let store = Self::new();
            let mut map = store.events.lock().unwrap();
            for (provider, event_id) in events {
                map.insert(
                    (provider.to_string(), event_id.to_string()),
                    IdempotencyRecord {
                        id: uuid::Uuid::new_v4(),
                        event_type: "test".to_string(),
                        status: "success".to_string(),
                        error: None,
                    },
                );
            }
            drop(map);
            store
        }

        pub fn get_record(&self, provider: &str, event_id: &str) -> Option<IdempotencyRecord> {
            self.events.lock().unwrap()
                .get(&(provider.to_string(), event_id.to_string()))
                .cloned()
        }
    }

    #[async_trait]
    impl IdempotencyStore for MockIdempotencyStore {
        async fn check(&self, provider: &str, event_id: &str) -> Result<bool, WebhookError> {
            Ok(self.events.lock().unwrap()
                .contains_key(&(provider.to_string(), event_id.to_string())))
        }

        async fn record(
            &self,
            provider: &str,
            event_id: &str,
            event_type: &str,
            status: &str,
        ) -> Result<uuid::Uuid, WebhookError> {
            let id = uuid::Uuid::new_v4();
            self.events.lock().unwrap().insert(
                (provider.to_string(), event_id.to_string()),
                IdempotencyRecord {
                    id,
                    event_type: event_type.to_string(),
                    status: status.to_string(),
                    error: None,
                },
            );
            Ok(id)
        }

        async fn update_status(
            &self,
            provider: &str,
            event_id: &str,
            status: &str,
            error: Option<&str>,
        ) -> Result<(), WebhookError> {
            if let Some(record) = self.events.lock().unwrap()
                .get_mut(&(provider.to_string(), event_id.to_string()))
            {
                record.status = status.to_string();
                record.error = error.map(|s| s.to_string());
            }
            Ok(())
        }
    }

    /// Mock secret provider with configurable secrets
    pub struct MockSecretProvider {
        secrets: HashMap<String, String>,
    }

    impl MockSecretProvider {
        pub fn new() -> Self {
            Self {
                secrets: HashMap::new(),
            }
        }

        pub fn with_secret(mut self, name: &str, value: &str) -> Self {
            self.secrets.insert(name.to_string(), value.to_string());
            self
        }
    }

    #[async_trait]
    impl SecretProvider for MockSecretProvider {
        async fn get_secret(&self, name: &str) -> Result<String, WebhookError> {
            self.secrets.get(name)
                .cloned()
                .ok_or_else(|| WebhookError::MissingSecret(name.to_string()))
        }
    }

    /// Mock clock for testing timestamp validation
    pub struct MockClock {
        current_time: AtomicU64,
    }

    impl MockClock {
        pub fn new(timestamp: u64) -> Self {
            Self {
                current_time: AtomicU64::new(timestamp),
            }
        }

        pub fn advance(&self, seconds: u64) {
            self.current_time.fetch_add(seconds, Ordering::SeqCst);
        }

        pub fn set(&self, timestamp: u64) {
            self.current_time.store(timestamp, Ordering::SeqCst);
        }
    }

    impl Clock for MockClock {
        fn now(&self) -> i64 {
            self.current_time.load(Ordering::SeqCst) as i64
        }
    }

    /// System clock implementation
    pub struct SystemClock;

    impl Clock for SystemClock {
        fn now(&self) -> i64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
        }
    }
}
```

---

## 3.1 Webhook Configuration

### Task: Define webhook configuration structures

```rust
// crates/fraiseql-webhooks/src/config.rs

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct WebhookConfig {
    /// Provider type (stripe, github, etc.) - inferred from key if not specified
    pub provider: Option<String>,

    /// Endpoint path (default: /webhooks/{name})
    pub path: Option<String>,

    /// Secret environment variable name
    pub secret_env: String,

    /// Signature scheme (for custom providers)
    pub signature_scheme: Option<String>,

    /// Custom signature header (for custom providers)
    pub signature_header: Option<String>,

    /// Timestamp header (for custom providers)
    pub timestamp_header: Option<String>,

    /// Timestamp tolerance in seconds
    #[serde(default = "default_timestamp_tolerance")]
    pub timestamp_tolerance: u64,

    /// Enable idempotency checking
    #[serde(default = "default_idempotent")]
    pub idempotent: bool,

    /// Event mappings
    #[serde(default)]
    pub events: HashMap<String, WebhookEventConfig>,
}

fn default_timestamp_tolerance() -> u64 { 300 }
fn default_idempotent() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct WebhookEventConfig {
    /// Database function to call
    pub function: String,

    /// Field mapping from webhook payload to function parameters
    #[serde(default)]
    pub mapping: HashMap<String, String>,

    /// Condition expression (optional)
    pub condition: Option<String>,
}
```

---

## 3.2 Signature Verification

### Task: Define signature verifier trait

```rust
// crates/fraiseql-webhooks/src/signature/mod.rs

use async_trait::async_trait;

/// Trait for webhook signature verification
#[async_trait]
pub trait SignatureVerifier: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Header name containing the signature
    fn signature_header(&self) -> &'static str;

    /// Verify the signature
    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        timestamp: Option<&str>,
    ) -> Result<bool, SignatureError>;

    /// Optional: Extract timestamp from signature or headers
    fn extract_timestamp(&self, signature: &str) -> Option<i64> {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SignatureError {
    #[error("Invalid signature format")]
    InvalidFormat,

    #[error("Signature mismatch")]
    Mismatch,

    #[error("Timestamp expired")]
    TimestampExpired,

    #[error("Missing timestamp")]
    MissingTimestamp,

    #[error("Crypto error: {0}")]
    Crypto(String),
}
```

### Task: Implement Stripe signature verifier

```rust
// crates/fraiseql-webhooks/src/signature/stripe.rs

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct StripeVerifier;

impl SignatureVerifier for StripeVerifier {
    fn name(&self) -> &'static str { "stripe" }

    fn signature_header(&self) -> &'static str { "Stripe-Signature" }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // Parse Stripe signature format: t=timestamp,v1=signature
        let parts: HashMap<&str, &str> = signature
            .split(',')
            .filter_map(|part| {
                let mut kv = part.splitn(2, '=');
                Some((kv.next()?, kv.next()?))
            })
            .collect();

        let timestamp = parts.get("t")
            .ok_or(SignatureError::InvalidFormat)?;

        let sig_v1 = parts.get("v1")
            .ok_or(SignatureError::InvalidFormat)?;

        // Verify timestamp is recent (5 minutes)
        let ts: i64 = timestamp.parse()
            .map_err(|_| SignatureError::InvalidFormat)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        if (now - ts).abs() > 300 {
            return Err(SignatureError::TimestampExpired);
        }

        // Compute expected signature
        // signed_payload = timestamp + "." + payload
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(signed_payload.as_bytes());

        let expected = hex::encode(mac.finalize().into_bytes());

        // Constant-time comparison
        Ok(constant_time_eq(sig_v1.as_bytes(), expected.as_bytes()))
    }

    fn extract_timestamp(&self, signature: &str) -> Option<i64> {
        signature.split(',')
            .find(|p| p.starts_with("t="))
            .and_then(|p| p.strip_prefix("t="))
            .and_then(|t| t.parse().ok())
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}
```

### Task: Implement GitHub signature verifier

```rust
// crates/fraiseql-webhooks/src/signature/github.rs

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct GitHubVerifier;

impl SignatureVerifier for GitHubVerifier {
    fn name(&self) -> &'static str { "github" }

    fn signature_header(&self) -> &'static str { "X-Hub-Signature-256" }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        // GitHub format: sha256=<hex>
        let sig_hex = signature.strip_prefix("sha256=")
            .ok_or(SignatureError::InvalidFormat)?;

        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(sig_hex.as_bytes(), expected.as_bytes()))
    }
}
```

### Task: Implement Shopify signature verifier

```rust
// crates/fraiseql-webhooks/src/signature/shopify.rs

use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::{Engine as _, engine::general_purpose};

pub struct ShopifyVerifier;

impl SignatureVerifier for ShopifyVerifier {
    fn name(&self) -> &'static str { "shopify" }

    fn signature_header(&self) -> &'static str { "X-Shopify-Hmac-Sha256" }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
```

### Task: Implement generic HMAC verifier

```rust
// crates/fraiseql-webhooks/src/signature/hmac.rs

use hmac::{Hmac, Mac};
use sha2::{Sha256, Sha1};

pub struct HmacSha256Verifier {
    header: String,
}

impl HmacSha256Verifier {
    pub fn new(header: &str) -> Self {
        Self { header: header.to_string() }
    }
}

impl SignatureVerifier for HmacSha256Verifier {
    fn name(&self) -> &'static str { "hmac-sha256" }

    fn signature_header(&self) -> &'static str {
        // This is a bit awkward - would need to return &str lifetime
        // In practice, we'd store this differently
        "X-Signature"
    }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}

pub struct HmacSha1Verifier {
    header: String,
}

impl SignatureVerifier for HmacSha1Verifier {
    fn name(&self) -> &'static str { "hmac-sha1" }

    fn signature_header(&self) -> &'static str { "X-Signature" }

    fn verify(
        &self,
        payload: &[u8],
        signature: &str,
        secret: &str,
        _timestamp: Option<&str>,
    ) -> Result<bool, SignatureError> {
        let mut mac = Hmac::<Sha1>::new_from_slice(secret.as_bytes())
            .map_err(|e| SignatureError::Crypto(e.to_string()))?;
        mac.update(payload);

        let expected = hex::encode(mac.finalize().into_bytes());

        Ok(constant_time_eq(signature.as_bytes(), expected.as_bytes()))
    }
}
```

### Task: Implement provider registry

```rust
// crates/fraiseql-webhooks/src/signature/registry.rs

use std::collections::HashMap;
use std::sync::Arc;

pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn SignatureVerifier>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let mut providers: HashMap<String, Arc<dyn SignatureVerifier>> = HashMap::new();

        // Register built-in providers
        providers.insert("stripe".into(), Arc::new(StripeVerifier));
        providers.insert("github".into(), Arc::new(GitHubVerifier));
        providers.insert("shopify".into(), Arc::new(ShopifyVerifier));
        providers.insert("gitlab".into(), Arc::new(GitLabVerifier));
        providers.insert("slack".into(), Arc::new(SlackVerifier));
        providers.insert("twilio".into(), Arc::new(TwilioVerifier));
        providers.insert("sendgrid".into(), Arc::new(SendGridVerifier));
        providers.insert("postmark".into(), Arc::new(PostmarkVerifier));
        providers.insert("paddle".into(), Arc::new(PaddleVerifier));
        providers.insert("lemonsqueezy".into(), Arc::new(LemonSqueezyVerifier));
        providers.insert("discord".into(), Arc::new(DiscordVerifier));
        providers.insert("paypal".into(), Arc::new(PayPalVerifier));
        providers.insert("hubspot".into(), Arc::new(HubSpotVerifier));
        providers.insert("linear".into(), Arc::new(LinearVerifier));

        // Generic verifiers
        providers.insert("hmac-sha256".into(), Arc::new(HmacSha256Verifier::default()));
        providers.insert("hmac-sha1".into(), Arc::new(HmacSha1Verifier::default()));

        Self { providers }
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn SignatureVerifier>> {
        self.providers.get(name).cloned()
    }

    pub fn register(&mut self, name: &str, verifier: Arc<dyn SignatureVerifier>) {
        self.providers.insert(name.to_string(), verifier);
    }
}
```

---

## 3.3 Idempotency Handling

### Task: Implement idempotency store

```rust
// crates/fraiseql-webhooks/src/idempotency.rs

use sqlx::PgPool;
use uuid::Uuid;

pub struct IdempotencyStore {
    db: PgPool,
}

impl IdempotencyStore {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    /// Check if event has already been processed
    pub async fn check(&self, provider: &str, event_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM _system.webhook_events
                WHERE provider = $1 AND event_id = $2
            ) as "exists!"
            "#,
            provider,
            event_id
        )
        .fetch_one(&self.db)
        .await?;

        Ok(result)
    }

    /// Record processed event
    pub async fn record(
        &self,
        provider: &str,
        event_id: &str,
        event_type: &str,
        status: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();

        sqlx::query!(
            r#"
            INSERT INTO _system.webhook_events (id, provider, event_id, event_type, status, processed_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            "#,
            id,
            provider,
            event_id,
            event_type,
            status
        )
        .execute(&self.db)
        .await?;

        Ok(id)
    }

    /// Update event status
    pub async fn update_status(
        &self,
        provider: &str,
        event_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE _system.webhook_events
            SET status = $3, error_message = $4, updated_at = NOW()
            WHERE provider = $1 AND event_id = $2
            "#,
            provider,
            event_id,
            status,
            error
        )
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

/// Database migration for idempotency table
pub const IDEMPOTENCY_MIGRATION: &str = r#"
CREATE TABLE IF NOT EXISTS _system.webhook_events (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ,

    UNIQUE(provider, event_id)
);

CREATE INDEX IF NOT EXISTS idx_webhook_events_provider_status
ON _system.webhook_events(provider, status);

CREATE INDEX IF NOT EXISTS idx_webhook_events_processed_at
ON _system.webhook_events(processed_at);
"#;
```

---

## 3.4 Event Routing

### Task: Implement JSON path extractor

```rust
// crates/fraiseql-webhooks/src/routing/jsonpath.rs

use serde_json::Value;

/// Extract value from JSON using dot-notation path
/// e.g., "data.object.id" -> payload["data"]["object"]["id"]
pub fn extract_path(value: &Value, path: &str) -> Option<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for part in parts {
        match current {
            Value::Object(map) => {
                current = map.get(part)?;
            }
            Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    current = arr.get(idx)?;
                } else {
                    return None;
                }
            }
            _ => return None,
        }
    }

    Some(current.clone())
}

/// Apply mapping from webhook payload to function parameters
pub fn apply_mapping(
    payload: &Value,
    mapping: &HashMap<String, String>,
) -> Result<HashMap<String, Value>, MappingError> {
    let mut result = HashMap::new();

    for (param_name, json_path) in mapping {
        let value = extract_path(payload, json_path)
            .ok_or_else(|| MappingError::PathNotFound {
                path: json_path.clone(),
            })?;

        result.insert(param_name.clone(), value);
    }

    Ok(result)
}

#[derive(Debug, thiserror::Error)]
pub enum MappingError {
    #[error("Path not found in payload: {path}")]
    PathNotFound { path: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_path() {
        let payload = json!({
            "data": {
                "object": {
                    "id": "pi_123",
                    "amount": 1000
                }
            }
        });

        assert_eq!(
            extract_path(&payload, "data.object.id"),
            Some(json!("pi_123"))
        );

        assert_eq!(
            extract_path(&payload, "data.object.amount"),
            Some(json!(1000))
        );

        assert_eq!(
            extract_path(&payload, "data.missing"),
            None
        );
    }
}
```

### Task: Implement condition evaluator

```rust
// crates/fraiseql-webhooks/src/routing/condition.rs

use serde_json::Value;

/// Simple condition evaluator for webhook event filtering
/// Supports: ==, !=, in, contains, starts_with, ends_with
pub fn evaluate_condition(payload: &Value, condition: &str) -> Result<bool, ConditionError> {
    let condition = condition.trim();

    // Parse condition: "field.path operator value"
    // Examples:
    //   "data.object.status == 'active'"
    //   "data.object.status in ['active', 'canceled']"
    //   "data.object.amount > 1000"

    // Try parsing comparison operators
    for op in ["==", "!=", ">=", "<=", ">", "<"] {
        if let Some((left, right)) = condition.split_once(op) {
            let left = left.trim();
            let right = right.trim();

            let left_value = extract_path(payload, left)
                .ok_or_else(|| ConditionError::PathNotFound { path: left.to_string() })?;

            let right_value = parse_literal(right)?;

            return Ok(compare(&left_value, op, &right_value));
        }
    }

    // Try parsing 'in' operator
    if let Some((left, right)) = condition.split_once(" in ") {
        let left = left.trim();
        let right = right.trim();

        let left_value = extract_path(payload, left)
            .ok_or_else(|| ConditionError::PathNotFound { path: left.to_string() })?;

        let right_values = parse_array_literal(right)?;

        return Ok(right_values.iter().any(|v| v == &left_value));
    }

    Err(ConditionError::InvalidSyntax { condition: condition.to_string() })
}

fn parse_literal(s: &str) -> Result<Value, ConditionError> {
    let s = s.trim();

    // String literal
    if (s.starts_with('\'') && s.ends_with('\'')) ||
       (s.starts_with('"') && s.ends_with('"')) {
        return Ok(Value::String(s[1..s.len()-1].to_string()));
    }

    // Number
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Value::Number(n.into()));
    }

    if let Ok(n) = s.parse::<f64>() {
        return Ok(serde_json::Number::from_f64(n)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }

    // Boolean
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }

    // Null
    if s == "null" {
        return Ok(Value::Null);
    }

    Err(ConditionError::InvalidLiteral { value: s.to_string() })
}

fn parse_array_literal(s: &str) -> Result<Vec<Value>, ConditionError> {
    let s = s.trim();

    if !s.starts_with('[') || !s.ends_with(']') {
        return Err(ConditionError::InvalidArrayLiteral { value: s.to_string() });
    }

    let inner = &s[1..s.len()-1];
    let values: Result<Vec<_>, _> = inner
        .split(',')
        .map(|v| parse_literal(v.trim()))
        .collect();

    values
}

fn compare(left: &Value, op: &str, right: &Value) -> bool {
    match op {
        "==" => left == right,
        "!=" => left != right,
        ">" => compare_ord(left, right) == Some(Ordering::Greater),
        "<" => compare_ord(left, right) == Some(Ordering::Less),
        ">=" => matches!(compare_ord(left, right), Some(Ordering::Greater | Ordering::Equal)),
        "<=" => matches!(compare_ord(left, right), Some(Ordering::Less | Ordering::Equal)),
        _ => false,
    }
}

fn compare_ord(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => {
            let l = l.as_f64()?;
            let r = r.as_f64()?;
            l.partial_cmp(&r)
        }
        (Value::String(l), Value::String(r)) => Some(l.cmp(r)),
        _ => None,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConditionError {
    #[error("Path not found: {path}")]
    PathNotFound { path: String },

    #[error("Invalid condition syntax: {condition}")]
    InvalidSyntax { condition: String },

    #[error("Invalid literal: {value}")]
    InvalidLiteral { value: String },

    #[error("Invalid array literal: {value}")]
    InvalidArrayLiteral { value: String },
}
```

---

## 3.5 Webhook Handler

### Task: Implement main webhook handler

```rust
// crates/fraiseql-webhooks/src/handler.rs

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    body::Bytes,
    Json,
};
use std::sync::Arc;
use serde_json::{json, Value};

use crate::{
    config::WebhookConfig,
    signature::ProviderRegistry,
    idempotency::IdempotencyStore,
    routing::{apply_mapping, evaluate_condition},
};

pub struct WebhookHandler {
    config: WebhookConfig,
    provider_name: String,
    verifier: Arc<dyn SignatureVerifier>,
    idempotency: IdempotencyStore,
    db: PgPool,
}

impl WebhookHandler {
    pub fn new(
        name: &str,
        config: WebhookConfig,
        registry: &ProviderRegistry,
        idempotency: IdempotencyStore,
        db: PgPool,
    ) -> Result<Self, RuntimeError> {
        let provider_name = config.provider.clone().unwrap_or_else(|| name.to_string());

        let verifier = registry.get(&provider_name)
            .ok_or_else(|| RuntimeError::Webhook(WebhookError::ProviderNotConfigured(
                provider_name.clone()
            )))?;

        Ok(Self {
            config,
            provider_name,
            verifier,
            idempotency,
            db,
        })
    }

    pub async fn handle(
        &self,
        headers: &HeaderMap,
        body: Bytes,
    ) -> Result<Json<Value>, RuntimeError> {
        // 1. Get signature from headers
        let signature = headers
            .get(self.verifier.signature_header())
            .and_then(|v| v.to_str().ok())
            .ok_or(RuntimeError::Webhook(WebhookError::MissingSignature))?;

        // 2. Get secret from environment
        let secret = std::env::var(&self.config.secret_env)
            .map_err(|_| RuntimeError::Config(ConfigError::MissingEnvVar {
                name: self.config.secret_env.clone()
            }))?;

        // 3. Verify signature
        let timestamp = headers
            .get(self.config.timestamp_header.as_deref().unwrap_or(""))
            .and_then(|v| v.to_str().ok());

        let valid = self.verifier.verify(&body, signature, &secret, timestamp)
            .map_err(|e| RuntimeError::Webhook(WebhookError::InvalidSignature))?;

        if !valid {
            return Err(RuntimeError::Webhook(WebhookError::InvalidSignature));
        }

        // 4. Parse payload
        let payload: Value = serde_json::from_slice(&body)
            .map_err(|e| RuntimeError::Validation(ValidationError::Field {
                field: "body".to_string(),
                message: format!("Invalid JSON: {}", e),
            }))?;

        // 5. Extract event type and ID
        let (event_type, event_id) = self.extract_event_info(&payload)?;

        // 6. Check idempotency
        if self.config.idempotent {
            if self.idempotency.check(&self.provider_name, &event_id).await? {
                // Already processed - return success
                tracing::info!(
                    provider = %self.provider_name,
                    event_id = %event_id,
                    "Duplicate webhook event, skipping"
                );

                record_webhook_event(&self.provider_name, &event_type, "duplicate");

                return Ok(Json(json!({
                    "status": "ok",
                    "message": "Event already processed"
                })));
            }
        }

        // 7. Record event (pending)
        self.idempotency.record(
            &self.provider_name,
            &event_id,
            &event_type,
            "pending"
        ).await?;

        // 8. Route event to handler
        let result = self.route_event(&event_type, &payload).await;

        // 9. Update event status
        match &result {
            Ok(_) => {
                self.idempotency.update_status(
                    &self.provider_name,
                    &event_id,
                    "success",
                    None
                ).await?;

                record_webhook_event(&self.provider_name, &event_type, "success");
            }
            Err(e) => {
                self.idempotency.update_status(
                    &self.provider_name,
                    &event_id,
                    "failed",
                    Some(&e.to_string())
                ).await?;

                record_webhook_event(&self.provider_name, &event_type, "failed");
            }
        }

        result
    }

    fn extract_event_info(&self, payload: &Value) -> Result<(String, String), RuntimeError> {
        // Provider-specific event extraction
        let (event_type, event_id) = match self.provider_name.as_str() {
            "stripe" => {
                let event_type = payload["type"].as_str()
                    .ok_or_else(|| WebhookError::UnknownEvent("missing type".into()))?
                    .to_string();
                let event_id = payload["id"].as_str()
                    .ok_or_else(|| WebhookError::UnknownEvent("missing id".into()))?
                    .to_string();
                (event_type, event_id)
            }
            "github" => {
                let event_type = payload["action"].as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let event_id = payload["delivery"].as_str()
                    .or_else(|| payload["hook_id"].as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                (event_type, event_id)
            }
            "shopify" => {
                let event_id = payload["id"].as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                // Event type comes from X-Shopify-Topic header - would need to pass that in
                ("webhook".to_string(), event_id)
            }
            _ => {
                // Generic extraction
                let event_type = payload["type"].as_str()
                    .or_else(|| payload["event"].as_str())
                    .or_else(|| payload["event_type"].as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let event_id = payload["id"].as_str()
                    .or_else(|| payload["event_id"].as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                (event_type, event_id)
            }
        };

        Ok((event_type, event_id))
    }

    async fn route_event(
        &self,
        event_type: &str,
        payload: &Value,
    ) -> Result<Json<Value>, RuntimeError> {
        // Find matching event configuration
        let event_config = self.config.events.get(event_type)
            .ok_or_else(|| {
                tracing::debug!(
                    provider = %self.provider_name,
                    event_type = %event_type,
                    "No handler configured for event type"
                );
                // Not an error - just no handler configured
                // Return success so webhook provider doesn't retry
                RuntimeError::Webhook(WebhookError::UnknownEvent(event_type.to_string()))
            })?;

        // Check condition if present
        if let Some(condition) = &event_config.condition {
            let matches = evaluate_condition(payload, condition)
                .map_err(|e| RuntimeError::Validation(ValidationError::Field {
                    field: "condition".to_string(),
                    message: e.to_string(),
                }))?;

            if !matches {
                tracing::debug!(
                    provider = %self.provider_name,
                    event_type = %event_type,
                    condition = %condition,
                    "Event does not match condition, skipping"
                );

                return Ok(Json(json!({
                    "status": "ok",
                    "message": "Event skipped (condition not met)"
                })));
            }
        }

        // Apply mapping
        let params = apply_mapping(payload, &event_config.mapping)
            .map_err(|e| RuntimeError::Validation(ValidationError::Field {
                field: "mapping".to_string(),
                message: e.to_string(),
            }))?;

        // Call database function
        let params_json = serde_json::to_value(&params)?;

        let result = sqlx::query_scalar!(
            "SELECT * FROM app.$1($2::jsonb)",
            event_config.function,
            params_json
        )
        .fetch_one(&self.db)
        .await
        .map_err(|e| RuntimeError::Database(e))?;

        Ok(Json(json!({
            "status": "ok",
            "result": result
        })))
    }
}
```

### Task: Implement Axum handler wrapper

```rust
// crates/fraiseql-webhooks/src/axum.rs

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    body::Bytes,
    response::IntoResponse,
    Json,
};
use std::sync::Arc;

use crate::handler::WebhookHandler;

/// Axum handler for webhook endpoints
pub async fn webhook_endpoint(
    Path(webhook_name): Path<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let handler = match state.webhook_handlers.get(&webhook_name) {
        Some(h) => h,
        None => {
            return Err(RuntimeError::Webhook(WebhookError::ProviderNotConfigured(webhook_name)));
        }
    };

    handler.handle(&headers, body).await
}
```

---

## 3.6 Database Migration

### Task: Create webhook system tables

```sql
-- migrations/001_webhook_system_tables.sql

-- Schema for system tables
CREATE SCHEMA IF NOT EXISTS _system;

-- Webhook events table (idempotency)
CREATE TABLE IF NOT EXISTS _system.webhook_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    payload JSONB,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ,

    UNIQUE(provider, event_id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_webhook_events_provider_status
ON _system.webhook_events(provider, status);

CREATE INDEX IF NOT EXISTS idx_webhook_events_processed_at
ON _system.webhook_events(processed_at);

-- Cleanup old events (run periodically)
-- DELETE FROM _system.webhook_events WHERE processed_at < NOW() - INTERVAL '30 days';
```

---

## Acceptance Criteria

- [ ] Stripe signature verification passes with test vectors
- [ ] GitHub signature verification passes
- [ ] Shopify signature verification passes
- [ ] Generic HMAC-SHA256 verification works
- [ ] Idempotency prevents duplicate event processing
- [ ] Event routing matches correct handlers
- [ ] JSON path extraction works for nested objects
- [ ] Condition evaluation works for simple expressions
- [ ] Database functions are called with correct parameters
- [ ] Metrics are recorded for all webhook events

---

## Additional Providers to Implement

```
Phase 3a (Core):
- stripe ✅
- github ✅
- shopify ✅
- hmac-sha256 ✅
- hmac-sha1 ✅

Phase 3b (Popular):
- gitlab
- slack
- twilio
- sendgrid
- postmark
- paddle
- lemonsqueezy

Phase 3c (Extended):
- discord (Ed25519)
- paypal
- hubspot
- linear
- jira
- woocommerce
- bigcommerce
- mailgun
- mailchimp
- convertkit
```

---

## Files to Create

```
crates/fraiseql-webhooks/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs
│   ├── handler.rs
│   ├── idempotency.rs
│   ├── signature/
│   │   ├── mod.rs
│   │   ├── registry.rs
│   │   ├── stripe.rs
│   │   ├── github.rs
│   │   ├── shopify.rs
│   │   ├── gitlab.rs
│   │   ├── slack.rs
│   │   ├── twilio.rs
│   │   ├── discord.rs
│   │   └── hmac.rs
│   ├── routing/
│   │   ├── mod.rs
│   │   ├── jsonpath.rs
│   │   └── condition.rs
│   └── axum.rs
└── tests/
    ├── signature_test.rs
    ├── routing_test.rs
    └── fixtures/
        ├── stripe_payment_intent.json
        └── github_push.json
```

---

---

## 3.7 Comprehensive Error Handling

### Task: Define webhook-specific errors with error codes

```rust
// crates/fraiseql-webhooks/src/error.rs

use thiserror::Error;

/// Webhook error codes for consistent error responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookErrorCode {
    /// WH001: Missing signature header
    MissingSignature,
    /// WH002: Invalid signature format
    InvalidSignatureFormat,
    /// WH003: Signature verification failed
    SignatureVerificationFailed,
    /// WH004: Timestamp expired (replay attack protection)
    TimestampExpired,
    /// WH005: Missing timestamp header
    MissingTimestamp,
    /// WH006: Webhook secret not configured
    MissingSecret,
    /// WH007: Unknown webhook provider
    UnknownProvider,
    /// WH008: Event type not configured
    UnknownEventType,
    /// WH009: Payload parsing failed
    InvalidPayload,
    /// WH010: Event handler execution failed
    HandlerFailed,
    /// WH011: Database error during webhook processing
    DatabaseError,
    /// WH012: Condition evaluation failed
    ConditionError,
    /// WH013: Parameter mapping failed
    MappingError,
}

impl WebhookErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingSignature => "WH001",
            Self::InvalidSignatureFormat => "WH002",
            Self::SignatureVerificationFailed => "WH003",
            Self::TimestampExpired => "WH004",
            Self::MissingTimestamp => "WH005",
            Self::MissingSecret => "WH006",
            Self::UnknownProvider => "WH007",
            Self::UnknownEventType => "WH008",
            Self::InvalidPayload => "WH009",
            Self::HandlerFailed => "WH010",
            Self::DatabaseError => "WH011",
            Self::ConditionError => "WH012",
            Self::MappingError => "WH013",
        }
    }

    pub fn http_status(&self) -> StatusCode {
        match self {
            Self::MissingSignature
            | Self::InvalidSignatureFormat
            | Self::SignatureVerificationFailed
            | Self::TimestampExpired
            | Self::MissingTimestamp => StatusCode::UNAUTHORIZED,

            Self::MissingSecret
            | Self::UnknownProvider => StatusCode::INTERNAL_SERVER_ERROR,

            Self::UnknownEventType => StatusCode::OK, // Return 200 so provider doesn't retry

            Self::InvalidPayload
            | Self::ConditionError
            | Self::MappingError => StatusCode::BAD_REQUEST,

            Self::HandlerFailed
            | Self::DatabaseError => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::MissingSignature | Self::InvalidSignatureFormat | Self::SignatureVerificationFailed
                => "https://docs.fraiseql.dev/webhooks/signature-verification",
            Self::TimestampExpired | Self::MissingTimestamp
                => "https://docs.fraiseql.dev/webhooks/replay-protection",
            Self::MissingSecret
                => "https://docs.fraiseql.dev/webhooks/configuration#secrets",
            Self::UnknownProvider
                => "https://docs.fraiseql.dev/webhooks/providers",
            Self::UnknownEventType
                => "https://docs.fraiseql.dev/webhooks/event-routing",
            Self::InvalidPayload | Self::MappingError | Self::ConditionError
                => "https://docs.fraiseql.dev/webhooks/payload-processing",
            Self::HandlerFailed | Self::DatabaseError
                => "https://docs.fraiseql.dev/webhooks/troubleshooting",
        }
    }

    /// Whether to log this error at warn level vs info level
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::MissingSignature
            | Self::InvalidSignatureFormat
            | Self::SignatureVerificationFailed
            | Self::TimestampExpired
            | Self::MissingTimestamp
            | Self::InvalidPayload
            | Self::UnknownEventType
        )
    }
}

#[derive(Debug, Error)]
pub enum WebhookError {
    #[error("Missing signature header")]
    MissingSignature,

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Timestamp expired (received: {received}, now: {now}, tolerance: {tolerance}s)")]
    TimestampExpired { received: i64, now: i64, tolerance: u64 },

    #[error("Missing timestamp header")]
    MissingTimestamp,

    #[error("Missing webhook secret: {0}")]
    MissingSecret(String),

    #[error("Unknown webhook provider: {0}")]
    UnknownProvider(String),

    #[error("Unknown event type: {0}")]
    UnknownEventType(String),

    #[error("Invalid payload: {0}")]
    InvalidPayload(String),

    #[error("Handler execution failed: {0}")]
    HandlerFailed(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Condition evaluation error: {0}")]
    Condition(String),

    #[error("Mapping error: {0}")]
    Mapping(String),
}

impl WebhookError {
    pub fn error_code(&self) -> WebhookErrorCode {
        match self {
            Self::MissingSignature => WebhookErrorCode::MissingSignature,
            Self::InvalidSignature(_) => WebhookErrorCode::InvalidSignatureFormat,
            Self::SignatureVerificationFailed => WebhookErrorCode::SignatureVerificationFailed,
            Self::TimestampExpired { .. } => WebhookErrorCode::TimestampExpired,
            Self::MissingTimestamp => WebhookErrorCode::MissingTimestamp,
            Self::MissingSecret(_) => WebhookErrorCode::MissingSecret,
            Self::UnknownProvider(_) => WebhookErrorCode::UnknownProvider,
            Self::UnknownEventType(_) => WebhookErrorCode::UnknownEventType,
            Self::InvalidPayload(_) => WebhookErrorCode::InvalidPayload,
            Self::HandlerFailed(_) => WebhookErrorCode::HandlerFailed,
            Self::Database(_) => WebhookErrorCode::DatabaseError,
            Self::Condition(_) => WebhookErrorCode::ConditionError,
            Self::Mapping(_) => WebhookErrorCode::MappingError,
        }
    }

    /// Convert to JSON error response
    pub fn to_response(&self) -> (StatusCode, Json<Value>) {
        let code = self.error_code();

        // Log appropriately
        if code.is_client_error() {
            tracing::info!(
                error_code = %code.as_str(),
                error = %self,
                "Webhook client error"
            );
        } else {
            tracing::warn!(
                error_code = %code.as_str(),
                error = %self,
                "Webhook processing error"
            );
        }

        (
            code.http_status(),
            Json(json!({
                "error": {
                    "code": code.as_str(),
                    "message": self.to_string(),
                    "docs": code.docs_url(),
                }
            }))
        )
    }
}

impl IntoResponse for WebhookError {
    fn into_response(self) -> Response {
        let (status, body) = self.to_response();
        (status, body).into_response()
    }
}
```

---

## 3.8 Contract Testing

### Task: Contract tests for real provider webhooks

Contract tests verify our signature verification matches real provider behavior using recorded payloads.

```rust
// crates/fraiseql-webhooks/tests/contracts/mod.rs

//! Contract tests for webhook signature verification.
//!
//! These tests use real recorded payloads from providers to ensure
//! our implementation matches actual provider behavior.
//!
//! To update test vectors:
//! 1. Use provider's test mode/CLI to generate webhook
//! 2. Record raw body and headers
//! 3. Update fixtures in tests/fixtures/

mod stripe_contracts;
mod github_contracts;
mod shopify_contracts;
```

```rust
// crates/fraiseql-webhooks/tests/contracts/stripe_contracts.rs

use fraiseql_webhooks::signature::StripeVerifier;

/// Test vector from Stripe CLI: `stripe listen --print-json`
const STRIPE_TEST_SECRET: &str = "whsec_test_secret_for_testing_only_do_not_use";

/// Recorded payload from Stripe test mode
const STRIPE_PAYMENT_INTENT_SUCCEEDED: &str = r#"{
    "id": "evt_1MqqbKLkdIwHu7ixGKtL2Y3V",
    "object": "event",
    "api_version": "2022-11-15",
    "created": 1679076299,
    "type": "payment_intent.succeeded",
    "data": {
        "object": {
            "id": "pi_3MqqbJLkdIwHu7ix0xGsM8vA",
            "amount": 1000,
            "currency": "usd",
            "status": "succeeded"
        }
    }
}"#;

/// Generate Stripe signature for testing
fn generate_stripe_signature(payload: &str, secret: &str, timestamp: i64) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let signed_payload = format!("{}.{}", timestamp, payload);
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(signed_payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());

    format!("t={},v1={}", timestamp, sig)
}

#[test]
fn test_stripe_signature_format_parsing() {
    let verifier = StripeVerifier;
    let timestamp = 1679076299i64;
    let signature = generate_stripe_signature(STRIPE_PAYMENT_INTENT_SUCCEEDED, STRIPE_TEST_SECRET, timestamp);

    // Should parse timestamp correctly
    assert_eq!(verifier.extract_timestamp(&signature), Some(timestamp));
}

#[test]
fn test_stripe_valid_signature() {
    use fraiseql_webhooks::testing::mocks::MockClock;

    let verifier = StripeVerifier::with_clock(Arc::new(MockClock::new(1679076299)));
    let timestamp = 1679076299i64;
    let signature = generate_stripe_signature(STRIPE_PAYMENT_INTENT_SUCCEEDED, STRIPE_TEST_SECRET, timestamp);

    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        &signature,
        STRIPE_TEST_SECRET,
        None,
    );

    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_stripe_invalid_signature() {
    use fraiseql_webhooks::testing::mocks::MockClock;

    let verifier = StripeVerifier::with_clock(Arc::new(MockClock::new(1679076299)));
    let signature = "t=1679076299,v1=invalid_signature";

    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        signature,
        STRIPE_TEST_SECRET,
        None,
    );

    assert!(result.is_ok());
    assert!(!result.unwrap()); // Signature doesn't match
}

#[test]
fn test_stripe_expired_timestamp() {
    use fraiseql_webhooks::testing::mocks::MockClock;

    // Clock is 10 minutes ahead of signature timestamp
    let verifier = StripeVerifier::with_clock(Arc::new(MockClock::new(1679076299 + 600)));
    let timestamp = 1679076299i64;
    let signature = generate_stripe_signature(STRIPE_PAYMENT_INTENT_SUCCEEDED, STRIPE_TEST_SECRET, timestamp);

    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        &signature,
        STRIPE_TEST_SECRET,
        None,
    );

    assert!(matches!(result, Err(SignatureError::TimestampExpired)));
}

#[test]
fn test_stripe_malformed_signature() {
    let verifier = StripeVerifier;

    // Missing timestamp
    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        "v1=abc123",
        STRIPE_TEST_SECRET,
        None,
    );
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));

    // Missing signature
    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        "t=1679076299",
        STRIPE_TEST_SECRET,
        None,
    );
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));

    // Completely malformed
    let result = verifier.verify(
        STRIPE_PAYMENT_INTENT_SUCCEEDED.as_bytes(),
        "not a valid signature",
        STRIPE_TEST_SECRET,
        None,
    );
    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}
```

```rust
// crates/fraiseql-webhooks/tests/contracts/github_contracts.rs

use fraiseql_webhooks::signature::GitHubVerifier;

const GITHUB_TEST_SECRET: &str = "test_secret_for_github_webhook";

const GITHUB_PUSH_EVENT: &str = r#"{
    "ref": "refs/heads/main",
    "before": "abc123",
    "after": "def456",
    "repository": {
        "id": 12345,
        "name": "test-repo",
        "full_name": "user/test-repo"
    },
    "pusher": {
        "name": "testuser",
        "email": "test@example.com"
    }
}"#;

fn generate_github_signature(payload: &str, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload.as_bytes());
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[test]
fn test_github_valid_signature() {
    let verifier = GitHubVerifier;
    let signature = generate_github_signature(GITHUB_PUSH_EVENT, GITHUB_TEST_SECRET);

    let result = verifier.verify(
        GITHUB_PUSH_EVENT.as_bytes(),
        &signature,
        GITHUB_TEST_SECRET,
        None,
    );

    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn test_github_invalid_signature() {
    let verifier = GitHubVerifier;

    let result = verifier.verify(
        GITHUB_PUSH_EVENT.as_bytes(),
        "sha256=invalid",
        GITHUB_TEST_SECRET,
        None,
    );

    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn test_github_wrong_prefix() {
    let verifier = GitHubVerifier;

    // Using sha1 prefix instead of sha256
    let result = verifier.verify(
        GITHUB_PUSH_EVENT.as_bytes(),
        "sha1=abc123",
        GITHUB_TEST_SECRET,
        None,
    );

    assert!(matches!(result, Err(SignatureError::InvalidFormat)));
}
```

### Task: Integration test with full handler flow

```rust
// crates/fraiseql-webhooks/tests/integration/handler_test.rs

use axum::http::StatusCode;
use axum_test::TestServer;
use fraiseql_webhooks::{
    testing::mocks::*,
    WebhookHandler,
    WebhookConfig,
};
use std::sync::Arc;

async fn setup_test_server() -> (TestServer, Arc<MockIdempotencyStore>) {
    let idempotency = Arc::new(MockIdempotencyStore::new());
    let verifier = Arc::new(MockSignatureVerifier::succeeding());
    let secrets = Arc::new(MockSecretProvider::new().with_secret("STRIPE_SECRET", "whsec_test"));
    let clock = Arc::new(MockClock::new(1679076299));

    let config = WebhookConfig {
        provider: Some("stripe".to_string()),
        secret_env: "STRIPE_SECRET".to_string(),
        idempotent: true,
        events: vec![
            ("payment_intent.succeeded".to_string(), WebhookEventConfig {
                function: "handle_payment_succeeded".to_string(),
                mapping: [
                    ("payment_id".to_string(), "data.object.id".to_string()),
                    ("amount".to_string(), "data.object.amount".to_string()),
                ].into_iter().collect(),
                condition: None,
            }),
        ].into_iter().collect(),
        ..Default::default()
    };

    let handler = WebhookHandler::new_with_deps(
        "stripe",
        config,
        verifier,
        idempotency.clone(),
        secrets,
        clock,
    );

    let app = axum::Router::new()
        .route("/webhooks/:name", axum::routing::post(webhook_endpoint))
        .with_state(Arc::new(handler));

    (TestServer::new(app).unwrap(), idempotency)
}

#[tokio::test]
async fn test_successful_webhook_processing() {
    let (server, idempotency) = setup_test_server().await;

    let payload = r#"{
        "id": "evt_123",
        "type": "payment_intent.succeeded",
        "data": {
            "object": {
                "id": "pi_123",
                "amount": 1000
            }
        }
    }"#;

    let response = server
        .post("/webhooks/stripe")
        .add_header("X-Mock-Signature", "valid")
        .body(payload)
        .await;

    assert_eq!(response.status_code(), StatusCode::OK);

    // Verify idempotency record was created
    let record = idempotency.get_record("stripe", "evt_123");
    assert!(record.is_some());
    assert_eq!(record.unwrap().status, "success");
}

#[tokio::test]
async fn test_duplicate_event_handling() {
    let idempotency = Arc::new(MockIdempotencyStore::with_existing_events(
        vec![("stripe", "evt_123")]
    ));

    let (server, _) = setup_test_server_with_idempotency(idempotency).await;

    let payload = r#"{"id": "evt_123", "type": "payment_intent.succeeded", "data": {}}"#;

    let response = server
        .post("/webhooks/stripe")
        .add_header("X-Mock-Signature", "valid")
        .body(payload)
        .await;

    // Should return 200 OK for duplicates (so provider doesn't retry)
    assert_eq!(response.status_code(), StatusCode::OK);

    let body: serde_json::Value = response.json();
    assert_eq!(body["message"], "Event already processed");
}

#[tokio::test]
async fn test_invalid_signature_rejected() {
    let verifier = Arc::new(MockSignatureVerifier::failing());
    let (server, _) = setup_test_server_with_verifier(verifier).await;

    let response = server
        .post("/webhooks/stripe")
        .add_header("X-Mock-Signature", "invalid")
        .body(r#"{"id": "evt_123"}"#)
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "WH003");
}

#[tokio::test]
async fn test_unknown_event_type_returns_ok() {
    let (server, _) = setup_test_server().await;

    let payload = r#"{
        "id": "evt_456",
        "type": "customer.deleted",
        "data": {}
    }"#;

    let response = server
        .post("/webhooks/stripe")
        .add_header("X-Mock-Signature", "valid")
        .body(payload)
        .await;

    // Unknown events should return 200 so provider doesn't keep retrying
    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_missing_signature_header() {
    let (server, _) = setup_test_server().await;

    let response = server
        .post("/webhooks/stripe")
        .body(r#"{"id": "evt_123"}"#)
        .await;

    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);

    let body: serde_json::Value = response.json();
    assert_eq!(body["error"]["code"], "WH001");
}
```

---

## 3.9 Unit Tests

### Task: Unit tests for signature verification

```rust
// crates/fraiseql-webhooks/src/signature/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::mocks::MockClock;
    use std::sync::Arc;

    mod stripe {
        use super::*;

        fn make_verifier(now: u64) -> StripeVerifier {
            StripeVerifier::with_clock(Arc::new(MockClock::new(now)))
        }

        #[test]
        fn test_valid_v1_signature() {
            let timestamp = 1679076299u64;
            let verifier = make_verifier(timestamp);
            let payload = b"test payload";
            let secret = "whsec_test";

            let signature = format!(
                "t={},v1={}",
                timestamp,
                compute_stripe_signature(payload, secret, timestamp)
            );

            assert!(verifier.verify(payload, &signature, secret, None).unwrap());
        }

        #[test]
        fn test_multiple_signatures_accepted() {
            // Stripe can send multiple signatures for key rotation
            let timestamp = 1679076299u64;
            let verifier = make_verifier(timestamp);
            let payload = b"test payload";
            let secret = "whsec_test";

            let valid_sig = compute_stripe_signature(payload, secret, timestamp);
            let signature = format!("t={},v1=invalid,v1={}", timestamp, valid_sig);

            assert!(verifier.verify(payload, &signature, secret, None).unwrap());
        }

        #[test]
        fn test_timestamp_within_tolerance() {
            let base_time = 1679076299u64;

            // 299 seconds ago - within 300s tolerance
            let verifier = make_verifier(base_time + 299);
            let signature = format!(
                "t={},v1={}",
                base_time,
                compute_stripe_signature(b"test", "secret", base_time)
            );

            assert!(verifier.verify(b"test", &signature, "secret", None).is_ok());
        }

        #[test]
        fn test_timestamp_outside_tolerance() {
            let base_time = 1679076299u64;

            // 301 seconds ago - outside 300s tolerance
            let verifier = make_verifier(base_time + 301);
            let signature = format!(
                "t={},v1={}",
                base_time,
                compute_stripe_signature(b"test", "secret", base_time)
            );

            assert!(matches!(
                verifier.verify(b"test", &signature, "secret", None),
                Err(SignatureError::TimestampExpired)
            ));
        }

        fn compute_stripe_signature(payload: &[u8], secret: &str, timestamp: u64) -> String {
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            let signed = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(signed.as_bytes());
            hex::encode(mac.finalize().into_bytes())
        }
    }

    mod github {
        use super::*;

        #[test]
        fn test_sha256_prefix_required() {
            let verifier = GitHubVerifier;

            // Without prefix
            assert!(matches!(
                verifier.verify(b"test", "abc123", "secret", None),
                Err(SignatureError::InvalidFormat)
            ));

            // With sha1 prefix (old format, we only support sha256)
            assert!(matches!(
                verifier.verify(b"test", "sha1=abc123", "secret", None),
                Err(SignatureError::InvalidFormat)
            ));
        }

        #[test]
        fn test_constant_time_comparison() {
            let verifier = GitHubVerifier;
            let payload = b"test payload";
            let secret = "secret";

            // Compute valid signature
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(payload);
            let valid = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

            // Valid signature
            assert!(verifier.verify(payload, &valid, secret, None).unwrap());

            // Invalid signature (different length shouldn't leak timing info)
            assert!(!verifier.verify(payload, "sha256=short", secret, None).unwrap());
        }
    }

    mod shopify {
        use super::*;

        #[test]
        fn test_base64_signature() {
            let verifier = ShopifyVerifier;
            let payload = b"test payload";
            let secret = "secret";

            use base64::{Engine, engine::general_purpose};
            use hmac::{Hmac, Mac};
            use sha2::Sha256;

            let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(payload);
            let signature = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

            assert!(verifier.verify(payload, &signature, secret, None).unwrap());
        }
    }
}
```

### Task: Unit tests for JSON path extraction and condition evaluation

```rust
// crates/fraiseql-webhooks/src/routing/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    mod jsonpath {
        use super::*;

        #[test]
        fn test_simple_path() {
            let data = json!({"name": "test"});
            assert_eq!(extract_path(&data, "name"), Some(json!("test")));
        }

        #[test]
        fn test_nested_path() {
            let data = json!({
                "data": {
                    "object": {
                        "id": "obj_123"
                    }
                }
            });

            assert_eq!(extract_path(&data, "data.object.id"), Some(json!("obj_123")));
        }

        #[test]
        fn test_array_index() {
            let data = json!({
                "items": ["a", "b", "c"]
            });

            assert_eq!(extract_path(&data, "items.0"), Some(json!("a")));
            assert_eq!(extract_path(&data, "items.2"), Some(json!("c")));
            assert_eq!(extract_path(&data, "items.5"), None);
        }

        #[test]
        fn test_mixed_path() {
            let data = json!({
                "users": [
                    {"name": "Alice"},
                    {"name": "Bob"}
                ]
            });

            assert_eq!(extract_path(&data, "users.0.name"), Some(json!("Alice")));
            assert_eq!(extract_path(&data, "users.1.name"), Some(json!("Bob")));
        }

        #[test]
        fn test_missing_path() {
            let data = json!({"a": {"b": 1}});

            assert_eq!(extract_path(&data, "a.c"), None);
            assert_eq!(extract_path(&data, "x.y.z"), None);
        }

        #[test]
        fn test_null_values() {
            let data = json!({"value": null});
            assert_eq!(extract_path(&data, "value"), Some(json!(null)));
        }
    }

    mod condition {
        use super::*;

        #[test]
        fn test_equality() {
            let data = json!({"status": "active"});

            assert!(evaluate_condition(&data, "status == 'active'").unwrap());
            assert!(!evaluate_condition(&data, "status == 'inactive'").unwrap());
        }

        #[test]
        fn test_inequality() {
            let data = json!({"status": "active"});

            assert!(evaluate_condition(&data, "status != 'inactive'").unwrap());
            assert!(!evaluate_condition(&data, "status != 'active'").unwrap());
        }

        #[test]
        fn test_numeric_comparison() {
            let data = json!({"amount": 1000});

            assert!(evaluate_condition(&data, "amount > 500").unwrap());
            assert!(evaluate_condition(&data, "amount >= 1000").unwrap());
            assert!(evaluate_condition(&data, "amount < 2000").unwrap());
            assert!(evaluate_condition(&data, "amount <= 1000").unwrap());
            assert!(!evaluate_condition(&data, "amount > 1000").unwrap());
        }

        #[test]
        fn test_in_operator() {
            let data = json!({"status": "active"});

            assert!(evaluate_condition(&data, "status in ['active', 'pending']").unwrap());
            assert!(!evaluate_condition(&data, "status in ['canceled', 'failed']").unwrap());
        }

        #[test]
        fn test_nested_field_condition() {
            let data = json!({
                "data": {
                    "object": {
                        "status": "succeeded"
                    }
                }
            });

            assert!(evaluate_condition(&data, "data.object.status == 'succeeded'").unwrap());
        }

        #[test]
        fn test_boolean_literal() {
            let data = json!({"enabled": true});

            assert!(evaluate_condition(&data, "enabled == true").unwrap());
            assert!(!evaluate_condition(&data, "enabled == false").unwrap());
        }

        #[test]
        fn test_null_comparison() {
            let data = json!({"value": null, "other": "set"});

            assert!(evaluate_condition(&data, "value == null").unwrap());
            assert!(!evaluate_condition(&data, "other == null").unwrap());
        }

        #[test]
        fn test_missing_field_error() {
            let data = json!({"a": 1});

            let result = evaluate_condition(&data, "missing == 'value'");
            assert!(matches!(result, Err(ConditionError::PathNotFound { .. })));
        }

        #[test]
        fn test_invalid_syntax_error() {
            let data = json!({"a": 1});

            let result = evaluate_condition(&data, "invalid syntax here");
            assert!(matches!(result, Err(ConditionError::InvalidSyntax { .. })));
        }
    }

    mod mapping {
        use super::*;

        #[test]
        fn test_apply_mapping() {
            let payload = json!({
                "data": {
                    "object": {
                        "id": "pi_123",
                        "amount": 1000,
                        "currency": "usd"
                    }
                }
            });

            let mapping = [
                ("payment_id".to_string(), "data.object.id".to_string()),
                ("amount".to_string(), "data.object.amount".to_string()),
            ].into_iter().collect();

            let result = apply_mapping(&payload, &mapping).unwrap();

            assert_eq!(result.get("payment_id"), Some(&json!("pi_123")));
            assert_eq!(result.get("amount"), Some(&json!(1000)));
        }

        #[test]
        fn test_mapping_missing_path() {
            let payload = json!({"a": 1});

            let mapping = [
                ("x".to_string(), "missing.path".to_string()),
            ].into_iter().collect();

            let result = apply_mapping(&payload, &mapping);
            assert!(matches!(result, Err(MappingError::PathNotFound { .. })));
        }
    }
}
```

---

## DO NOT

- Do not implement all 15+ providers in first iteration - start with core 5
- Do not add retry logic yet (that's in Phase 6 observers)
- Do not implement webhook sending (outbound) - only receiving
- Do not add complex expression parsing - keep conditions simple
- Do not bypass transaction boundaries for "performance" - correctness first
