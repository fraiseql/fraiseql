# Phase 10: Polish & Production Readiness

## Objective

Complete the endpoint runtime implementation with comprehensive testing, documentation, security hardening, performance optimization, and production deployment tooling. This phase ensures the runtime is ready for production workloads.

## Dependencies

- All previous phases (1-9) must be complete
- Core functionality must be working and tested

---

## Part 0: Migration Strategy and Feature Flags

### 0.1 Migration Strategy

**Critical: Zero-downtime migrations require careful planning**

```rust
// src/migration/strategy.rs
use std::collections::HashMap;

/// Migration state machine for zero-downtime deployments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationPhase {
    /// Phase 1: Deploy new code with feature flag off
    Deploy,
    /// Phase 2: Run database migrations (backward compatible)
    Migrate,
    /// Phase 3: Enable feature for canary users
    Canary,
    /// Phase 4: Gradual rollout (10%, 25%, 50%, 100%)
    Rollout { percentage: u8 },
    /// Phase 5: Clean up old code paths
    Cleanup,
    /// Migration complete
    Complete,
}

/// Migration configuration
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    /// Feature flag name
    pub feature_flag: String,
    /// Database migrations to run
    pub db_migrations: Vec<String>,
    /// Canary user IDs for early access
    pub canary_users: Vec<String>,
    /// Rollout schedule (percentage -> duration before next step)
    pub rollout_schedule: Vec<(u8, std::time::Duration)>,
    /// Metrics to monitor during rollout
    pub health_metrics: Vec<String>,
    /// Error rate threshold to pause rollout
    pub error_threshold: f64,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            feature_flag: String::new(),
            db_migrations: Vec::new(),
            canary_users: Vec::new(),
            rollout_schedule: vec![
                (10, std::time::Duration::from_secs(300)),   // 10% for 5 min
                (25, std::time::Duration::from_secs(600)),   // 25% for 10 min
                (50, std::time::Duration::from_secs(1800)),  // 50% for 30 min
                (100, std::time::Duration::from_secs(0)),    // 100% - complete
            ],
            health_metrics: vec![
                "error_rate".to_string(),
                "p99_latency".to_string(),
            ],
            error_threshold: 0.01, // 1% error rate pauses rollout
        }
    }
}

/// Migration orchestrator
pub struct MigrationOrchestrator {
    config: MigrationConfig,
    current_phase: MigrationPhase,
    feature_flags: std::sync::Arc<dyn FeatureFlagProvider>,
    metrics: std::sync::Arc<dyn MetricsProvider>,
}

impl MigrationOrchestrator {
    pub fn new(
        config: MigrationConfig,
        feature_flags: std::sync::Arc<dyn FeatureFlagProvider>,
        metrics: std::sync::Arc<dyn MetricsProvider>,
    ) -> Self {
        Self {
            config,
            current_phase: MigrationPhase::Deploy,
            feature_flags,
            metrics,
        }
    }

    /// Execute the next migration step
    pub async fn advance(&mut self) -> Result<MigrationPhase, MigrationError> {
        match self.current_phase {
            MigrationPhase::Deploy => {
                tracing::info!(
                    feature = %self.config.feature_flag,
                    "Migration: Deploy phase - new code deployed with feature flag off"
                );
                self.current_phase = MigrationPhase::Migrate;
            }
            MigrationPhase::Migrate => {
                tracing::info!("Migration: Migrate phase - running database migrations");
                for migration in &self.config.db_migrations {
                    tracing::info!(migration = %migration, "Running migration");
                    // Database migrations are run externally (sqlx migrate, etc.)
                }
                self.current_phase = MigrationPhase::Canary;
            }
            MigrationPhase::Canary => {
                tracing::info!(
                    users = ?self.config.canary_users,
                    "Migration: Canary phase - enabling for canary users"
                );
                self.feature_flags
                    .set_users(&self.config.feature_flag, &self.config.canary_users)
                    .await?;

                // Wait and check metrics
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;

                if self.check_health().await? {
                    self.current_phase = MigrationPhase::Rollout { percentage: 0 };
                } else {
                    return Err(MigrationError::HealthCheckFailed {
                        phase: "canary".to_string(),
                    });
                }
            }
            MigrationPhase::Rollout { percentage } => {
                // Find next rollout step
                let next_step = self.config.rollout_schedule
                    .iter()
                    .find(|(p, _)| *p > percentage);

                if let Some((next_pct, wait_duration)) = next_step {
                    tracing::info!(
                        percentage = next_pct,
                        "Migration: Rollout phase - increasing to {}%",
                        next_pct
                    );

                    self.feature_flags
                        .set_percentage(&self.config.feature_flag, *next_pct)
                        .await?;

                    // Wait and check health
                    tokio::time::sleep(*wait_duration).await;

                    if !self.check_health().await? {
                        // Rollback to previous percentage
                        self.feature_flags
                            .set_percentage(&self.config.feature_flag, percentage)
                            .await?;
                        return Err(MigrationError::HealthCheckFailed {
                            phase: format!("rollout_{}%", next_pct),
                        });
                    }

                    self.current_phase = MigrationPhase::Rollout { percentage: *next_pct };

                    if *next_pct == 100 {
                        self.current_phase = MigrationPhase::Cleanup;
                    }
                }
            }
            MigrationPhase::Cleanup => {
                tracing::info!("Migration: Cleanup phase - removing old code paths");
                // Old code cleanup is done in a separate PR after migration is verified
                self.current_phase = MigrationPhase::Complete;
            }
            MigrationPhase::Complete => {
                tracing::info!("Migration: Complete");
            }
        }

        Ok(self.current_phase)
    }

    /// Check health metrics during rollout
    async fn check_health(&self) -> Result<bool, MigrationError> {
        let error_rate = self.metrics.get_error_rate().await?;

        if error_rate > self.config.error_threshold {
            tracing::warn!(
                error_rate = error_rate,
                threshold = self.config.error_threshold,
                "Migration health check failed: error rate too high"
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Emergency rollback
    pub async fn rollback(&mut self) -> Result<(), MigrationError> {
        tracing::warn!(
            feature = %self.config.feature_flag,
            phase = ?self.current_phase,
            "Migration: Emergency rollback initiated"
        );

        self.feature_flags
            .set_percentage(&self.config.feature_flag, 0)
            .await?;

        self.feature_flags
            .set_users(&self.config.feature_flag, &[])
            .await?;

        self.current_phase = MigrationPhase::Deploy;
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("Health check failed in phase: {phase}")]
    HealthCheckFailed { phase: String },
    #[error("Feature flag error: {0}")]
    FeatureFlag(String),
    #[error("Metrics error: {0}")]
    Metrics(String),
}

/// Traits for migration dependencies
#[async_trait::async_trait]
pub trait FeatureFlagProvider: Send + Sync {
    async fn set_percentage(&self, flag: &str, percentage: u8) -> Result<(), MigrationError>;
    async fn set_users(&self, flag: &str, users: &[String]) -> Result<(), MigrationError>;
    async fn is_enabled(&self, flag: &str, user_id: Option<&str>) -> Result<bool, MigrationError>;
}

#[async_trait::async_trait]
pub trait MetricsProvider: Send + Sync {
    async fn get_error_rate(&self) -> Result<f64, MigrationError>;
    async fn get_p99_latency(&self) -> Result<f64, MigrationError>;
}
```

### 0.2 Feature Flag System

```rust
// src/feature_flags/mod.rs
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Feature flag configuration
#[derive(Debug, Clone)]
pub struct FeatureFlag {
    pub name: String,
    pub description: String,
    /// Whether flag is enabled globally
    pub enabled: bool,
    /// Percentage rollout (0-100)
    pub percentage: u8,
    /// Specific users with access
    pub user_allowlist: HashSet<String>,
    /// Users explicitly denied access
    pub user_denylist: HashSet<String>,
    /// Start time for time-based flags
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    /// End time for time-based flags
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Environment filter (e.g., only in staging)
    pub environments: Vec<String>,
}

impl Default for FeatureFlag {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            enabled: false,
            percentage: 0,
            user_allowlist: HashSet::new(),
            user_denylist: HashSet::new(),
            start_time: None,
            end_time: None,
            environments: vec![],
        }
    }
}

/// Feature flag service
pub struct FeatureFlagService {
    flags: Arc<RwLock<HashMap<String, FeatureFlag>>>,
    environment: String,
    /// Cache for evaluated flags (user_id -> flag_name -> enabled)
    evaluation_cache: Arc<RwLock<HashMap<String, HashMap<String, bool>>>>,
}

impl FeatureFlagService {
    pub fn new(environment: &str) -> Self {
        Self {
            flags: Arc::new(RwLock::new(HashMap::new())),
            environment: environment.to_string(),
            evaluation_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load flags from configuration
    pub async fn load_flags(&self, flags: Vec<FeatureFlag>) {
        let mut store = self.flags.write().await;
        for flag in flags {
            store.insert(flag.name.clone(), flag);
        }
    }

    /// Check if a feature is enabled for a user
    pub async fn is_enabled(&self, flag_name: &str, user_id: Option<&str>) -> bool {
        // Check cache first
        if let Some(uid) = user_id {
            let cache = self.evaluation_cache.read().await;
            if let Some(user_cache) = cache.get(uid) {
                if let Some(&enabled) = user_cache.get(flag_name) {
                    return enabled;
                }
            }
        }

        let enabled = self.evaluate(flag_name, user_id).await;

        // Cache result
        if let Some(uid) = user_id {
            let mut cache = self.evaluation_cache.write().await;
            cache
                .entry(uid.to_string())
                .or_default()
                .insert(flag_name.to_string(), enabled);
        }

        enabled
    }

    /// Evaluate flag without caching
    async fn evaluate(&self, flag_name: &str, user_id: Option<&str>) -> bool {
        let flags = self.flags.read().await;
        let flag = match flags.get(flag_name) {
            Some(f) => f,
            None => {
                tracing::warn!(flag = %flag_name, "Unknown feature flag");
                return false;
            }
        };

        // Check environment
        if !flag.environments.is_empty() && !flag.environments.contains(&self.environment) {
            return false;
        }

        // Check time window
        let now = chrono::Utc::now();
        if let Some(start) = flag.start_time {
            if now < start {
                return false;
            }
        }
        if let Some(end) = flag.end_time {
            if now > end {
                return false;
            }
        }

        // Check user-specific rules
        if let Some(uid) = user_id {
            // Denylist takes precedence
            if flag.user_denylist.contains(uid) {
                return false;
            }

            // Allowlist grants access
            if flag.user_allowlist.contains(uid) {
                return true;
            }
        }

        // Check global enabled
        if flag.enabled {
            return true;
        }

        // Percentage rollout (deterministic based on user_id)
        if flag.percentage > 0 {
            if let Some(uid) = user_id {
                let hash = self.hash_user_flag(uid, flag_name);
                let bucket = (hash % 100) as u8;
                return bucket < flag.percentage;
            }
        }

        false
    }

    /// Deterministic hash for percentage rollout
    fn hash_user_flag(&self, user_id: &str, flag_name: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        flag_name.hash(&mut hasher);
        hasher.finish()
    }

    /// Update flag percentage (for gradual rollout)
    pub async fn set_percentage(&self, flag_name: &str, percentage: u8) {
        let mut flags = self.flags.write().await;
        if let Some(flag) = flags.get_mut(flag_name) {
            flag.percentage = percentage.min(100);
            tracing::info!(
                flag = %flag_name,
                percentage = percentage,
                "Feature flag percentage updated"
            );
        }

        // Clear evaluation cache
        self.evaluation_cache.write().await.clear();
    }

    /// Add users to allowlist
    pub async fn add_users(&self, flag_name: &str, users: &[String]) {
        let mut flags = self.flags.write().await;
        if let Some(flag) = flags.get_mut(flag_name) {
            flag.user_allowlist.extend(users.iter().cloned());
        }
        self.evaluation_cache.write().await.clear();
    }

    /// Clear evaluation cache (call after flag changes)
    pub async fn clear_cache(&self) {
        self.evaluation_cache.write().await.clear();
    }
}

/// Mock feature flag provider for testing
pub struct MockFeatureFlagProvider {
    pub flags: std::sync::Mutex<HashMap<String, (u8, Vec<String>)>>,
}

impl MockFeatureFlagProvider {
    pub fn new() -> Self {
        Self {
            flags: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn with_flag(self, name: &str, percentage: u8) -> Self {
        self.flags
            .lock()
            .unwrap()
            .insert(name.to_string(), (percentage, vec![]));
        self
    }
}

impl Default for MockFeatureFlagProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl crate::migration::strategy::FeatureFlagProvider for MockFeatureFlagProvider {
    async fn set_percentage(
        &self,
        flag: &str,
        percentage: u8,
    ) -> Result<(), crate::migration::strategy::MigrationError> {
        let mut flags = self.flags.lock().unwrap();
        flags.entry(flag.to_string()).or_insert((0, vec![])).0 = percentage;
        Ok(())
    }

    async fn set_users(
        &self,
        flag: &str,
        users: &[String],
    ) -> Result<(), crate::migration::strategy::MigrationError> {
        let mut flags = self.flags.lock().unwrap();
        flags.entry(flag.to_string()).or_insert((0, vec![])).1 = users.to_vec();
        Ok(())
    }

    async fn is_enabled(
        &self,
        flag: &str,
        user_id: Option<&str>,
    ) -> Result<bool, crate::migration::strategy::MigrationError> {
        let flags = self.flags.lock().unwrap();
        if let Some((pct, users)) = flags.get(flag) {
            if let Some(uid) = user_id {
                if users.contains(&uid.to_string()) {
                    return Ok(true);
                }
            }
            Ok(*pct > 0)
        } else {
            Ok(false)
        }
    }
}
```

### 0.3 Database Migration Best Practices

```markdown
## Database Migration Checklist

### Backward Compatible Migrations

Always ensure migrations are backward compatible with the previous code version:

1. **Adding columns**: Add with `DEFAULT` value or `NULL`able
2. **Removing columns**: Deploy code that doesn't use column first, then remove
3. **Renaming columns**: Add new column → Deploy code using new column → Remove old column
4. **Adding constraints**: Add as `NOT VALID`, then validate separately

### Example: Adding a required column

```sql
-- Migration 1: Add column as nullable
ALTER TABLE users ADD COLUMN email_verified BOOLEAN DEFAULT FALSE;

-- Deploy new code that writes to email_verified

-- Migration 2: Add NOT NULL constraint (after backfill)
ALTER TABLE users ALTER COLUMN email_verified SET NOT NULL;
```

### Example: Renaming a column

```sql
-- Migration 1: Add new column
ALTER TABLE orders ADD COLUMN total_amount DECIMAL(10,2);

-- Deploy code that writes to both old and new columns

-- Migration 2: Backfill data
UPDATE orders SET total_amount = total WHERE total_amount IS NULL;

-- Deploy code that only reads from new column

-- Migration 3: Remove old column (in separate PR after verification)
ALTER TABLE orders DROP COLUMN total;
```

### Migration Timing

- Run migrations during low-traffic periods
- Use `CONCURRENTLY` for index creation in PostgreSQL
- Set statement timeouts to prevent long-running migrations
- Monitor replication lag during large data migrations
```

---

## Part A: Comprehensive Testing

### A.1 Test Infrastructure

```
tests/
├── unit/                      # Unit tests per module
├── integration/               # Integration tests
│   ├── webhooks/
│   ├── files/
│   ├── auth/
│   ├── observers/
│   ├── notifications/
│   └── integrations/
├── e2e/                       # End-to-end tests
│   ├── scenarios/
│   └── fixtures/
├── load/                      # Load/stress tests
│   ├── k6/
│   └── criterion/
├── security/                  # Security tests
│   └── audit/
└── fixtures/                  # Test data
    ├── schemas/
    ├── requests/
    └── responses/
```

### A.2 Unit Test Coverage

```rust
// tests/unit/webhooks/stripe_test.rs
use fraiseql_webhooks::providers::stripe::StripeVerifier;
use fraiseql_webhooks::SignatureVerifier;

#[test]
fn test_stripe_signature_valid() {
    let verifier = StripeVerifier;
    let payload = b"test payload";
    let timestamp = "1234567890";
    let secret = "whsec_test123";

    // Generate valid signature
    let signature = format!("t={},v1={}", timestamp, compute_hmac(payload, secret, timestamp));

    assert!(verifier.verify(payload, &signature, secret, Some(timestamp)).unwrap());
}

#[test]
fn test_stripe_signature_invalid() {
    let verifier = StripeVerifier;
    let payload = b"test payload";
    let signature = "t=123,v1=invalid";
    let secret = "whsec_test123";

    assert!(!verifier.verify(payload, signature, secret, None).unwrap());
}

#[test]
fn test_stripe_signature_expired() {
    let verifier = StripeVerifier;
    let payload = b"test payload";
    let old_timestamp = "1000000000"; // Way in the past
    let secret = "whsec_test123";
    let signature = format!("t={},v1={}", old_timestamp, compute_hmac(payload, secret, old_timestamp));

    let result = verifier.verify(payload, &signature, secret, Some(old_timestamp));
    assert!(result.is_err()); // Should fail due to timestamp tolerance
}
```

### A.3 Integration Test Suite

```rust
// tests/integration/auth/oauth_flow_test.rs
use fraiseql_auth::providers::google::GoogleProvider;
use fraiseql_test_utils::mock_server::MockOAuthServer;

#[tokio::test]
async fn test_google_oauth_complete_flow() {
    let mock = MockOAuthServer::start().await;

    let config = GoogleConfig {
        client_id: "test_client_id".to_string(),
        client_secret: "test_secret".to_string(),
        redirect_uri: "http://localhost/callback".to_string(),
    };

    let provider = GoogleProvider::new(&config).unwrap();

    // Step 1: Generate auth URL
    let state = "random_state";
    let auth_url = provider.authorization_url(state, &config.redirect_uri);
    assert!(auth_url.contains("accounts.google.com"));
    assert!(auth_url.contains(&config.client_id));

    // Step 2: Exchange code for tokens
    mock.expect_token_exchange("test_code", "access_token_123", "refresh_token_456");
    let tokens = provider.exchange_code("test_code", &config.redirect_uri).await.unwrap();
    assert_eq!(tokens.access_token, "access_token_123");

    // Step 3: Get user info
    mock.expect_userinfo("access_token_123", json!({
        "sub": "123456",
        "email": "test@example.com",
        "name": "Test User"
    }));
    let user = provider.user_info(&tokens.access_token).await.unwrap();
    assert_eq!(user.email, Some("test@example.com".to_string()));
}

// tests/integration/webhooks/delivery_test.rs
#[tokio::test]
async fn test_webhook_processing_pipeline() {
    let app = TestApp::spawn().await;

    // Configure a test webhook
    app.config.webhooks.insert("test", WebhookConfig {
        provider: "stripe".to_string(),
        secret_env: "TEST_SECRET".to_string(),
        ..Default::default()
    });

    std::env::set_var("TEST_SECRET", "whsec_test");

    // Send webhook with valid signature
    let payload = json!({
        "type": "payment_intent.succeeded",
        "data": {"object": {"id": "pi_123"}}
    });
    let signature = generate_stripe_signature(&payload, "whsec_test");

    let response = app.client
        .post("/webhooks/test")
        .header("stripe-signature", signature)
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    // Verify event was stored
    let event = app.db.get_webhook_event("stripe", "payment_intent.succeeded").await;
    assert!(event.is_some());
}
```

### A.4 End-to-End Scenarios

```rust
// tests/e2e/scenarios/ecommerce_test.rs
#[tokio::test]
async fn test_order_lifecycle() {
    let app = TestApp::spawn_with_schema("ecommerce").await;

    // 1. Create user via OAuth
    let user = app.oauth_login("google", "test@example.com").await;

    // 2. Upload product image
    let image = app.upload_file("products", "test.jpg", include_bytes!("fixtures/test.jpg")).await;
    assert!(image.url.starts_with("http"));

    // 3. Create order via GraphQL
    let order = app.graphql(r#"
        mutation CreateOrder($input: OrderInput!) {
            createOrder(input: $input) {
                id
                status
                total
            }
        }
    "#, json!({
        "input": {
            "items": [{"productId": "prod_123", "quantity": 2}],
            "shippingAddress": {"street": "123 Main St"}
        }
    })).await;

    assert_eq!(order["createOrder"]["status"], "pending");

    // 4. Simulate Stripe webhook for payment
    app.send_webhook("stripe", json!({
        "type": "payment_intent.succeeded",
        "data": {"object": {"metadata": {"order_id": order["createOrder"]["id"]}}}
    })).await;

    // 5. Verify order status updated
    let updated = app.graphql(r#"
        query GetOrder($id: ID!) {
            order(id: $id) { status }
        }
    "#, json!({"id": order["createOrder"]["id"]})).await;

    assert_eq!(updated["order"]["status"], "paid");

    // 6. Verify notification was sent
    assert!(app.notification_sent("email", "order_confirmation").await);
}
```

### A.5 Load Testing

```javascript
// tests/load/k6/graphql_load.js
import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
    stages: [
        { duration: '30s', target: 20 },   // Ramp up
        { duration: '1m', target: 100 },   // Sustain
        { duration: '30s', target: 200 },  // Peak
        { duration: '30s', target: 0 },    // Ramp down
    ],
    thresholds: {
        http_req_duration: ['p(95)<500'],  // 95th percentile < 500ms
        http_req_failed: ['rate<0.01'],    // Error rate < 1%
    },
};

export default function () {
    const query = `
        query ListProducts($limit: Int!) {
            products(limit: $limit) {
                id
                name
                price
            }
        }
    `;

    const response = http.post('http://localhost:8000/graphql', JSON.stringify({
        query,
        variables: { limit: 10 }
    }), {
        headers: { 'Content-Type': 'application/json' }
    });

    check(response, {
        'status is 200': (r) => r.status === 200,
        'no errors': (r) => !JSON.parse(r.body).errors,
    });

    sleep(0.1);
}
```

```rust
// tests/load/criterion/benchmarks.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_signature_verification(c: &mut Criterion) {
    let verifiers = vec![
        ("stripe", Box::new(StripeVerifier) as Box<dyn SignatureVerifier>),
        ("github", Box::new(GitHubVerifier)),
        ("shopify", Box::new(ShopifyVerifier)),
    ];

    let payload = b"test payload data";
    let secret = "test_secret_key";

    let mut group = c.benchmark_group("signature_verification");

    for (name, verifier) in &verifiers {
        let signature = generate_signature(verifier.as_ref(), payload, secret);

        group.bench_with_input(
            BenchmarkId::new("verify", name),
            &(payload, &signature, secret),
            |b, (p, s, sec)| {
                b.iter(|| verifier.verify(black_box(*p), black_box(s), black_box(sec), None))
            },
        );
    }

    group.finish();
}

fn benchmark_cache_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let cache = rt.block_on(async { RedisCache::new(&config).await.unwrap() });

    let mut group = c.benchmark_group("cache");

    group.bench_function("get_miss", |b| {
        b.to_async(&rt).iter(|| cache.get("nonexistent_key"))
    });

    group.bench_function("set_get", |b| {
        b.to_async(&rt).iter(|| async {
            cache.set("bench_key", b"value", None).await.unwrap();
            cache.get("bench_key").await.unwrap()
        })
    });

    group.finish();
}

criterion_group!(benches, benchmark_signature_verification, benchmark_cache_operations);
criterion_main!(benches);
```

---

## Part B: Security Hardening

### B.1 Security Audit Checklist

```markdown
## Security Audit Checklist

### Authentication
- [ ] JWT tokens use RS256 or ES256 (not HS256 with weak secrets)
- [ ] Refresh tokens are rotated on use
- [ ] Session tokens have appropriate TTLs
- [ ] Password reset tokens expire quickly (15 min max)
- [ ] Rate limiting on login attempts
- [ ] Account lockout after failed attempts

### Authorization
- [ ] All GraphQL operations check permissions
- [ ] Field-level authorization enforced
- [ ] No authorization bypass via nested queries
- [ ] Admin endpoints require admin role

### Input Validation
- [ ] Query depth limiting configured
- [ ] Query complexity limiting configured
- [ ] File upload size limits enforced
- [ ] File type validation (not just extension)
- [ ] SQL injection prevented (parameterized queries)
- [ ] GraphQL injection prevented

### Secrets Management
- [ ] No secrets in code or configs
- [ ] Environment variables for all secrets
- [ ] Secrets not logged
- [ ] API keys rotatable without downtime

### Network Security
- [ ] TLS 1.2+ required
- [ ] HTTPS enforced (HSTS)
- [ ] CORS properly configured
- [ ] CSP headers set
- [ ] Rate limiting on all endpoints

### Data Protection
- [ ] Sensitive data encrypted at rest
- [ ] PII logged with masking only
- [ ] Audit logs for sensitive operations
- [ ] Data retention policies implemented

### Webhook Security
- [ ] All webhooks verify signatures
- [ ] Timestamp tolerance enforced (5 min)
- [ ] Idempotency keys prevent replay
- [ ] Failed verification logged

### File Upload Security
- [ ] Virus scanning (ClamAV integration)
- [ ] Content type validation
- [ ] Filename sanitization
- [ ] Private files require signed URLs
```

### B.2 Security Middleware

```rust
// src/security/middleware.rs
use axum::{
    http::{header, HeaderValue, Request, Response},
    middleware::Next,
};

/// Security headers middleware
pub async fn security_headers<B>(
    request: Request<B>,
    next: Next<B>,
) -> Response<B> {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Prevent clickjacking
    headers.insert(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );

    // Prevent MIME sniffing
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );

    // XSS protection (legacy browsers)
    headers.insert(
        "X-XSS-Protection",
        HeaderValue::from_static("1; mode=block"),
    );

    // Referrer policy
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    // Content Security Policy
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'"
        ),
    );

    // HSTS (only in production)
    if std::env::var("FRAISEQL_ENV").as_deref() == Ok("production") {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }

    response
}

/// Query complexity analyzer
pub struct QueryComplexityAnalyzer {
    max_depth: u32,
    max_complexity: u32,
}

impl QueryComplexityAnalyzer {
    pub fn new(max_depth: u32, max_complexity: u32) -> Self {
        Self { max_depth, max_complexity }
    }

    pub fn analyze(&self, query: &str) -> Result<ComplexityReport, SecurityError> {
        // Parse query and calculate complexity
        let depth = self.calculate_depth(query);
        let complexity = self.calculate_complexity(query);

        if depth > self.max_depth {
            return Err(SecurityError::QueryTooDeep { depth, max: self.max_depth });
        }

        if complexity > self.max_complexity {
            return Err(SecurityError::QueryTooComplex { complexity, max: self.max_complexity });
        }

        Ok(ComplexityReport { depth, complexity })
    }

    fn calculate_depth(&self, _query: &str) -> u32 {
        // Implement depth calculation
        0
    }

    fn calculate_complexity(&self, _query: &str) -> u32 {
        // Implement complexity calculation
        0
    }
}

#[derive(Debug)]
pub struct ComplexityReport {
    pub depth: u32,
    pub complexity: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Query depth {depth} exceeds maximum {max}")]
    QueryTooDeep { depth: u32, max: u32 },

    #[error("Query complexity {complexity} exceeds maximum {max}")]
    QueryTooComplex { complexity: u32, max: u32 },
}
```

### B.3 Secrets Rotation

```rust
// src/security/secrets.rs
use std::sync::Arc;
use tokio::sync::RwLock;

/// Secret manager with rotation support
pub struct SecretManager {
    secrets: Arc<RwLock<HashMap<String, SecretValue>>>,
    providers: Vec<Box<dyn SecretProvider>>,
}

#[derive(Clone)]
pub struct SecretValue {
    pub value: String,
    pub version: u32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait::async_trait]
pub trait SecretProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn get(&self, key: &str) -> Result<Option<SecretValue>, SecretError>;
    async fn rotate(&self, key: &str) -> Result<SecretValue, SecretError>;
}

impl SecretManager {
    pub async fn get(&self, key: &str) -> Option<String> {
        let secrets = self.secrets.read().await;
        secrets.get(key).map(|s| s.value.clone())
    }

    /// Rotate a secret across all providers
    pub async fn rotate(&self, key: &str) -> Result<(), SecretError> {
        for provider in &self.providers {
            let new_value = provider.rotate(key).await?;
            let mut secrets = self.secrets.write().await;
            secrets.insert(key.to_string(), new_value);
        }
        Ok(())
    }

    /// Check for expiring secrets and rotate
    pub async fn check_expiring(&self, threshold: chrono::Duration) -> Vec<String> {
        let now = chrono::Utc::now();
        let secrets = self.secrets.read().await;

        secrets
            .iter()
            .filter_map(|(key, value)| {
                value.expires_at.and_then(|exp| {
                    if exp - now < threshold {
                        Some(key.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("Secret not found: {0}")]
    NotFound(String),

    #[error("Provider error: {0}")]
    Provider(String),
}
```

---

## Part C: Performance Optimization

### C.1 Connection Pooling Tuning

```toml
# fraiseql.toml - Production settings

[database]
url_env = "DATABASE_URL"
min_connections = 10
max_connections = 100
connect_timeout = "5s"
idle_timeout = "10m"
max_lifetime = "30m"

[cache.redis]
url_env = "REDIS_URL"
pool_size = 20
connect_timeout = "2s"
command_timeout = "1s"
```

### C.2 Response Caching

```rust
// src/cache/response.rs
use sha2::{Sha256, Digest};

/// Response cache for GraphQL queries
pub struct ResponseCache {
    cache: Arc<dyn CacheProvider>,
    default_ttl: Duration,
    cache_control_parser: CacheControlParser,
}

impl ResponseCache {
    /// Generate cache key for a GraphQL query
    pub fn cache_key(&self, query: &str, variables: &Value, user_id: Option<&str>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query.as_bytes());
        hasher.update(serde_json::to_string(variables).unwrap_or_default().as_bytes());

        // Include user ID if query is user-specific
        if let Some(uid) = user_id {
            hasher.update(uid.as_bytes());
        }

        format!("response:{:x}", hasher.finalize())
    }

    /// Get cached response if available
    pub async fn get(&self, key: &str) -> Option<CachedResponse> {
        self.cache.get_json(key).await.ok().flatten()
    }

    /// Cache a response
    pub async fn set(&self, key: &str, response: &Value, ttl: Option<Duration>) {
        let cached = CachedResponse {
            data: response.clone(),
            cached_at: chrono::Utc::now(),
        };

        let ttl = ttl.unwrap_or(self.default_ttl);
        let _ = self.cache.set_json(key, &cached, Some(ttl)).await;
    }

    /// Parse @cacheControl directive from query
    pub fn get_cache_ttl(&self, query: &str) -> Option<Duration> {
        self.cache_control_parser.parse(query)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CachedResponse {
    pub data: Value,
    pub cached_at: chrono::DateTime<chrono::Utc>,
}
```

### C.3 DataLoader Pattern

```rust
// src/optimization/dataloader.rs
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Generic DataLoader for batching and caching
pub struct DataLoader<K, V> {
    batch_fn: Arc<dyn Fn(Vec<K>) -> futures::future::BoxFuture<'static, HashMap<K, V>> + Send + Sync>,
    cache: Arc<Mutex<HashMap<K, V>>>,
    batch: Arc<Mutex<Vec<K>>>,
    batch_size: usize,
}

impl<K: Clone + Hash + Eq + Send + 'static, V: Clone + Send + 'static> DataLoader<K, V> {
    pub fn new<F, Fut>(batch_fn: F) -> Self
    where
        F: Fn(Vec<K>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = HashMap<K, V>> + Send + 'static,
    {
        Self {
            batch_fn: Arc::new(move |keys| Box::pin(batch_fn(keys))),
            cache: Arc::new(Mutex::new(HashMap::new())),
            batch: Arc::new(Mutex::new(Vec::new())),
            batch_size: 100,
        }
    }

    pub async fn load(&self, key: K) -> Option<V> {
        // Check cache first
        {
            let cache = self.cache.lock().await;
            if let Some(value) = cache.get(&key) {
                return Some(value.clone());
            }
        }

        // Add to batch
        let should_dispatch = {
            let mut batch = self.batch.lock().await;
            batch.push(key.clone());
            batch.len() >= self.batch_size
        };

        if should_dispatch {
            self.dispatch().await;
        } else {
            // Schedule dispatch on next tick
            let loader = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_micros(100)).await;
                loader.dispatch().await;
            });
        }

        // Wait for result
        let cache = self.cache.lock().await;
        cache.get(&key).cloned()
    }

    async fn dispatch(&self) {
        let keys = {
            let mut batch = self.batch.lock().await;
            std::mem::take(&mut *batch)
        };

        if keys.is_empty() {
            return;
        }

        let results = (self.batch_fn)(keys).await;

        let mut cache = self.cache.lock().await;
        for (key, value) in results {
            cache.insert(key, value);
        }
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.lock().await;
        cache.clear();
    }
}

impl<K, V> Clone for DataLoader<K, V> {
    fn clone(&self) -> Self {
        Self {
            batch_fn: self.batch_fn.clone(),
            cache: self.cache.clone(),
            batch: self.batch.clone(),
            batch_size: self.batch_size,
        }
    }
}
```

---

## Part D: Observability

### D.1 Structured Logging

```rust
// src/observability/logging.rs
use tracing::{Level, Subscriber};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    prelude::*,
    EnvFilter,
};

pub fn init_logging() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("FRAISEQL_LOG_FORMAT")
        .unwrap_or_else(|_| "json".to_string());

    match format.as_str() {
        "json" => {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(env_filter)
                .with_span_events(FmtSpan::CLOSE)
                .with_current_span(true)
                .with_target(true)
                .init();
        }
        "pretty" => {
            tracing_subscriber::fmt()
                .pretty()
                .with_env_filter(env_filter)
                .with_span_events(FmtSpan::CLOSE)
                .init();
        }
        _ => {
            tracing_subscriber::fmt()
                .compact()
                .with_env_filter(env_filter)
                .init();
        }
    }
}

/// Request logging span
pub fn request_span(request_id: uuid::Uuid, method: &str, path: &str) -> tracing::Span {
    tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %method,
        path = %path,
        status = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    )
}
```

### D.2 Metrics Export

```rust
// src/observability/metrics.rs
use prometheus::{
    Encoder, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec,
    IntCounter, IntCounterVec, Opts, Registry, TextEncoder,
};

lazy_static::lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // Request metrics
    pub static ref HTTP_REQUESTS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("http_requests_total", "Total HTTP requests"),
        &["method", "path", "status"]
    ).unwrap();

    pub static ref HTTP_REQUEST_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("http_request_duration_seconds", "HTTP request duration"),
        &["method", "path"]
    ).unwrap();

    // GraphQL metrics
    pub static ref GRAPHQL_OPERATIONS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("graphql_operations_total", "Total GraphQL operations"),
        &["operation_type", "operation_name"]
    ).unwrap();

    pub static ref GRAPHQL_ERRORS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("graphql_errors_total", "Total GraphQL errors"),
        &["error_code"]
    ).unwrap();

    // Webhook metrics
    pub static ref WEBHOOK_RECEIVED_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("webhook_received_total", "Total webhooks received"),
        &["provider", "event_type"]
    ).unwrap();

    pub static ref WEBHOOK_PROCESSING_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("webhook_processing_duration_seconds", "Webhook processing time"),
        &["provider"]
    ).unwrap();

    // Cache metrics
    pub static ref CACHE_HITS_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("cache_hits_total", "Cache hits"),
        &["cache_name"]
    ).unwrap();

    pub static ref CACHE_MISSES_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("cache_misses_total", "Cache misses"),
        &["cache_name"]
    ).unwrap();

    // Connection pool metrics
    pub static ref DB_POOL_SIZE: GaugeVec = GaugeVec::new(
        Opts::new("db_pool_size", "Database connection pool size"),
        &["state"]  // idle, active
    ).unwrap();
}

pub fn init_metrics() {
    REGISTRY.register(Box::new(HTTP_REQUESTS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(HTTP_REQUEST_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(GRAPHQL_OPERATIONS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(GRAPHQL_ERRORS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(WEBHOOK_RECEIVED_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(WEBHOOK_PROCESSING_DURATION.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_HITS_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(CACHE_MISSES_TOTAL.clone())).unwrap();
    REGISTRY.register(Box::new(DB_POOL_SIZE.clone())).unwrap();
}

pub fn export_metrics() -> String {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
```

### D.3 Health Checks

```rust
// src/observability/health.rs
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct HealthStatus {
    pub status: String,  // "healthy", "degraded", "unhealthy"
    pub version: String,
    pub uptime_seconds: u64,
    pub checks: HashMap<String, CheckResult>,
}

#[derive(Debug, Serialize)]
pub struct CheckResult {
    pub status: String,
    pub message: Option<String>,
    pub latency_ms: Option<u64>,
}

pub struct HealthChecker {
    checks: Vec<Box<dyn HealthCheck>>,
    start_time: std::time::Instant,
    version: String,
}

#[async_trait::async_trait]
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &str;
    async fn check(&self) -> CheckResult;
}

impl HealthChecker {
    pub fn new(version: &str) -> Self {
        Self {
            checks: Vec::new(),
            start_time: std::time::Instant::now(),
            version: version.to_string(),
        }
    }

    pub fn add_check<C: HealthCheck + 'static>(&mut self, check: C) {
        self.checks.push(Box::new(check));
    }

    pub async fn run(&self) -> HealthStatus {
        let mut results = HashMap::new();
        let mut all_healthy = true;
        let mut any_unhealthy = false;

        for check in &self.checks {
            let result = check.check().await;

            if result.status == "unhealthy" {
                any_unhealthy = true;
            }
            if result.status != "healthy" {
                all_healthy = false;
            }

            results.insert(check.name().to_string(), result);
        }

        let status = if all_healthy {
            "healthy"
        } else if any_unhealthy {
            "unhealthy"
        } else {
            "degraded"
        };

        HealthStatus {
            status: status.to_string(),
            version: self.version.clone(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            checks: results,
        }
    }
}

// Built-in checks
pub struct DatabaseCheck {
    pool: sqlx::PgPool,
}

#[async_trait::async_trait]
impl HealthCheck for DatabaseCheck {
    fn name(&self) -> &str {
        "database"
    }

    async fn check(&self) -> CheckResult {
        let start = std::time::Instant::now();

        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => CheckResult {
                status: "healthy".to_string(),
                message: None,
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
            Err(e) => CheckResult {
                status: "unhealthy".to_string(),
                message: Some(e.to_string()),
                latency_ms: Some(start.elapsed().as_millis() as u64),
            },
        }
    }
}
```

---

## Part E: Deployment & Operations

### E.1 Docker Configuration

```dockerfile
# Dockerfile
FROM rust:1.75-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/*/Cargo.toml crates/
RUN mkdir -p crates/fraiseql-runtime/src && echo "fn main(){}" > crates/fraiseql-runtime/src/main.rs
RUN cargo build --release --bin fraiseql-server && rm -rf crates

# Build actual application
COPY . .
RUN cargo build --release --bin fraiseql-server

# Runtime image
FROM alpine:3.19

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/fraiseql-server /usr/local/bin/

# Non-root user
RUN adduser -D -u 1000 fraiseql
USER fraiseql

EXPOSE 8000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8000/health || exit 1

ENTRYPOINT ["fraiseql-server"]
```

```yaml
# docker-compose.yml
version: "3.8"

services:
  fraiseql:
    build: .
    ports:
      - "8000:8000"
    environment:
      - DATABASE_URL=postgres://postgres:postgres@db:5432/fraiseql
      - REDIS_URL=redis://redis:6379
      - FRAISEQL_ENV=production
      - FRAISEQL_LOG_FORMAT=json
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_healthy
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: "2"
          memory: 1G

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_PASSWORD: postgres
    volumes:
      - postgres_data:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  postgres_data:
```

### E.2 Kubernetes Manifests

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql
  labels:
    app: fraiseql
spec:
  replicas: 3
  selector:
    matchLabels:
      app: fraiseql
  template:
    metadata:
      labels:
        app: fraiseql
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "8000"
        prometheus.io/path: "/metrics"
    spec:
      containers:
        - name: fraiseql
          image: fraiseql/server:latest
          ports:
            - containerPort: 8000
          env:
            - name: DATABASE_URL
              valueFrom:
                secretKeyRef:
                  name: fraiseql-secrets
                  key: database-url
            - name: REDIS_URL
              valueFrom:
                secretKeyRef:
                  name: fraiseql-secrets
                  key: redis-url
          resources:
            requests:
              cpu: 500m
              memory: 512Mi
            limits:
              cpu: 2000m
              memory: 2Gi
          livenessProbe:
            httpGet:
              path: /health
              port: 8000
            initialDelaySeconds: 10
            periodSeconds: 30
          readinessProbe:
            httpGet:
              path: /health
              port: 8000
            initialDelaySeconds: 5
            periodSeconds: 10
          securityContext:
            runAsNonRoot: true
            runAsUser: 1000
            readOnlyRootFilesystem: true
---
apiVersion: v1
kind: Service
metadata:
  name: fraiseql
spec:
  selector:
    app: fraiseql
  ports:
    - port: 80
      targetPort: 8000
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: fraiseql
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: fraiseql
  minReplicas: 3
  maxReplicas: 10
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: 70
```

### E.3 Runbook

```markdown
# FraiseQL Operations Runbook

## Common Issues

### High Latency
1. Check database connection pool saturation: `db_pool_size{state="idle"}`
2. Check cache hit rate: `cache_hits_total / (cache_hits_total + cache_misses_total)`
3. Check for slow queries: Enable `FRAISEQL_LOG_SLOW_QUERIES=true`
4. Scale horizontally if CPU > 70%

### Webhook Failures
1. Check provider status page
2. Verify signature secret is correct: `WEBHOOK_SECRET` env var
3. Check for clock drift (timestamp validation fails)
4. Review logs: `grep "webhook" /var/log/fraiseql.log`

### Memory Usage Growing
1. Check for cache eviction: `cache_evictions_total`
2. Review DataLoader cache sizes
3. Check for query complexity (large responses)
4. Consider reducing `max_connections` if idle connections high

### Authentication Failures
1. Verify OAuth provider is reachable
2. Check JWT signing key is configured
3. Verify clock sync (JWT expiration)
4. Check rate limiting not blocking legitimate users

## Scaling Guidelines

| Metric | Threshold | Action |
|--------|-----------|--------|
| CPU > 70% | 5 min sustained | Scale up replicas |
| Memory > 80% | 5 min sustained | Increase memory limit |
| P95 latency > 500ms | 5 min sustained | Investigate slow queries |
| Error rate > 1% | 1 min sustained | Page on-call |
| DB connections > 80% pool | Immediate | Increase pool size |

## Incident Response

### Severity 1: Complete Outage
1. Page on-call immediately
2. Check health endpoints
3. Roll back recent deployments
4. Scale up infrastructure
5. Post-incident review within 24h

### Severity 2: Degraded Performance
1. Alert on-call within 15 min
2. Identify bottleneck
3. Apply temporary fix
4. Schedule proper fix

### Severity 3: Minor Issue
1. Create ticket
2. Fix in next sprint
```

---

## Part F: Unit Tests for Migration and Feature Flags

### F.1 Feature Flag Tests

```rust
// tests/feature_flags_test.rs
use fraiseql_runtime::feature_flags::{FeatureFlag, FeatureFlagService};
use std::collections::HashSet;

#[tokio::test]
async fn test_feature_flag_enabled_globally() {
    let service = FeatureFlagService::new("production");

    service.load_flags(vec![FeatureFlag {
        name: "new_checkout".to_string(),
        enabled: true,
        ..Default::default()
    }]).await;

    assert!(service.is_enabled("new_checkout", None).await);
    assert!(service.is_enabled("new_checkout", Some("user-1")).await);
}

#[tokio::test]
async fn test_feature_flag_percentage_rollout() {
    let service = FeatureFlagService::new("production");

    service.load_flags(vec![FeatureFlag {
        name: "experiment_a".to_string(),
        percentage: 50,
        ..Default::default()
    }]).await;

    // With 50% rollout, some users should get it, some shouldn't
    // The hash is deterministic, so same user always gets same result
    let user1 = service.is_enabled("experiment_a", Some("user-1")).await;
    let user1_again = service.is_enabled("experiment_a", Some("user-1")).await;

    // Same user should get consistent result
    assert_eq!(user1, user1_again);

    // Without user_id, percentage rollout doesn't apply
    assert!(!service.is_enabled("experiment_a", None).await);
}

#[tokio::test]
async fn test_feature_flag_user_allowlist() {
    let service = FeatureFlagService::new("production");

    let mut allowlist = HashSet::new();
    allowlist.insert("vip-user".to_string());

    service.load_flags(vec![FeatureFlag {
        name: "beta_feature".to_string(),
        enabled: false,
        percentage: 0,
        user_allowlist: allowlist,
        ..Default::default()
    }]).await;

    // VIP user should have access even though flag is disabled
    assert!(service.is_enabled("beta_feature", Some("vip-user")).await);

    // Other users should not
    assert!(!service.is_enabled("beta_feature", Some("regular-user")).await);
}

#[tokio::test]
async fn test_feature_flag_user_denylist() {
    let service = FeatureFlagService::new("production");

    let mut denylist = HashSet::new();
    denylist.insert("banned-user".to_string());

    service.load_flags(vec![FeatureFlag {
        name: "premium_feature".to_string(),
        enabled: true, // Enabled for everyone
        user_denylist: denylist,
        ..Default::default()
    }]).await;

    // Banned user should not have access even though flag is enabled
    assert!(!service.is_enabled("premium_feature", Some("banned-user")).await);

    // Other users should have access
    assert!(service.is_enabled("premium_feature", Some("normal-user")).await);
}

#[tokio::test]
async fn test_feature_flag_environment_filter() {
    let staging = FeatureFlagService::new("staging");
    let production = FeatureFlagService::new("production");

    let flag = FeatureFlag {
        name: "staging_only".to_string(),
        enabled: true,
        environments: vec!["staging".to_string()],
        ..Default::default()
    };

    staging.load_flags(vec![flag.clone()]).await;
    production.load_flags(vec![flag]).await;

    // Should be enabled in staging
    assert!(staging.is_enabled("staging_only", None).await);

    // Should be disabled in production
    assert!(!production.is_enabled("staging_only", None).await);
}

#[tokio::test]
async fn test_feature_flag_time_window() {
    let service = FeatureFlagService::new("production");

    // Flag that starts in the future
    service.load_flags(vec![FeatureFlag {
        name: "future_feature".to_string(),
        enabled: true,
        start_time: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        ..Default::default()
    }]).await;

    assert!(!service.is_enabled("future_feature", None).await);

    // Flag that ended in the past
    service.load_flags(vec![FeatureFlag {
        name: "expired_feature".to_string(),
        enabled: true,
        end_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
        ..Default::default()
    }]).await;

    assert!(!service.is_enabled("expired_feature", None).await);
}

#[tokio::test]
async fn test_feature_flag_set_percentage() {
    let service = FeatureFlagService::new("production");

    service.load_flags(vec![FeatureFlag {
        name: "rollout_feature".to_string(),
        percentage: 0,
        ..Default::default()
    }]).await;

    // Initially 0%
    assert!(!service.is_enabled("rollout_feature", Some("user-1")).await);

    // Increase to 100%
    service.set_percentage("rollout_feature", 100).await;

    // Now everyone should have access
    assert!(service.is_enabled("rollout_feature", Some("user-1")).await);
}

#[tokio::test]
async fn test_unknown_flag_returns_false() {
    let service = FeatureFlagService::new("production");

    assert!(!service.is_enabled("nonexistent_flag", None).await);
    assert!(!service.is_enabled("nonexistent_flag", Some("user-1")).await);
}
```

### F.2 Migration Orchestrator Tests

```rust
// tests/migration_test.rs
use fraiseql_runtime::migration::strategy::{
    MigrationConfig, MigrationError, MigrationOrchestrator, MigrationPhase,
};
use fraiseql_runtime::feature_flags::MockFeatureFlagProvider;
use std::sync::Arc;

/// Mock metrics provider for testing
pub struct MockMetricsProvider {
    error_rate: std::sync::Mutex<f64>,
    p99_latency: std::sync::Mutex<f64>,
}

impl MockMetricsProvider {
    pub fn new() -> Self {
        Self {
            error_rate: std::sync::Mutex::new(0.0),
            p99_latency: std::sync::Mutex::new(100.0),
        }
    }

    pub fn set_error_rate(&self, rate: f64) {
        *self.error_rate.lock().unwrap() = rate;
    }
}

#[async_trait::async_trait]
impl fraiseql_runtime::migration::strategy::MetricsProvider for MockMetricsProvider {
    async fn get_error_rate(&self) -> Result<f64, MigrationError> {
        Ok(*self.error_rate.lock().unwrap())
    }

    async fn get_p99_latency(&self) -> Result<f64, MigrationError> {
        Ok(*self.p99_latency.lock().unwrap())
    }
}

#[tokio::test]
async fn test_migration_advances_through_phases() {
    let config = MigrationConfig {
        feature_flag: "new_feature".to_string(),
        canary_users: vec!["canary-1".to_string()],
        error_threshold: 0.1,
        rollout_schedule: vec![
            (50, std::time::Duration::from_millis(10)),
            (100, std::time::Duration::from_millis(0)),
        ],
        ..Default::default()
    };

    let flags = Arc::new(MockFeatureFlagProvider::new());
    let metrics = Arc::new(MockMetricsProvider::new());

    let mut orchestrator = MigrationOrchestrator::new(config, flags.clone(), metrics);

    // Deploy -> Migrate
    let phase = orchestrator.advance().await.unwrap();
    assert_eq!(phase, MigrationPhase::Migrate);

    // Migrate -> Canary
    let phase = orchestrator.advance().await.unwrap();
    assert_eq!(phase, MigrationPhase::Canary);

    // Canary -> Rollout (health check passes with 0% error rate)
    let phase = orchestrator.advance().await.unwrap();
    assert!(matches!(phase, MigrationPhase::Rollout { percentage: 0 }));
}

#[tokio::test]
async fn test_migration_stops_on_high_error_rate() {
    let config = MigrationConfig {
        feature_flag: "risky_feature".to_string(),
        canary_users: vec!["canary-1".to_string()],
        error_threshold: 0.01, // 1% threshold
        ..Default::default()
    };

    let flags = Arc::new(MockFeatureFlagProvider::new());
    let metrics = Arc::new(MockMetricsProvider::new());
    metrics.set_error_rate(0.05); // 5% error rate

    let mut orchestrator = MigrationOrchestrator::new(config, flags, metrics);

    // Advance to Migrate
    orchestrator.advance().await.unwrap();

    // Advance to Canary - should fail health check
    let result = orchestrator.advance().await;
    assert!(matches!(
        result,
        Err(MigrationError::HealthCheckFailed { .. })
    ));
}

#[tokio::test]
async fn test_migration_rollback() {
    let config = MigrationConfig {
        feature_flag: "test_feature".to_string(),
        ..Default::default()
    };

    let flags = Arc::new(MockFeatureFlagProvider::new());
    let metrics = Arc::new(MockMetricsProvider::new());

    let mut orchestrator = MigrationOrchestrator::new(config, flags.clone(), metrics);

    // Advance a few phases
    orchestrator.advance().await.unwrap();
    orchestrator.advance().await.unwrap();

    // Rollback
    orchestrator.rollback().await.unwrap();

    // Verify flag was disabled
    let flags_state = flags.flags.lock().unwrap();
    let (percentage, users) = flags_state.get("test_feature").unwrap();
    assert_eq!(*percentage, 0);
    assert!(users.is_empty());
}
```

### F.3 Health Check Tests

```rust
// tests/health_test.rs
use fraiseql_runtime::observability::health::{
    CheckResult, HealthCheck, HealthChecker, HealthStatus,
};

struct AlwaysHealthyCheck;

#[async_trait::async_trait]
impl HealthCheck for AlwaysHealthyCheck {
    fn name(&self) -> &str {
        "always_healthy"
    }

    async fn check(&self) -> CheckResult {
        CheckResult {
            status: "healthy".to_string(),
            message: None,
            latency_ms: Some(1),
        }
    }
}

struct AlwaysUnhealthyCheck {
    reason: String,
}

#[async_trait::async_trait]
impl HealthCheck for AlwaysUnhealthyCheck {
    fn name(&self) -> &str {
        "always_unhealthy"
    }

    async fn check(&self) -> CheckResult {
        CheckResult {
            status: "unhealthy".to_string(),
            message: Some(self.reason.clone()),
            latency_ms: Some(100),
        }
    }
}

#[tokio::test]
async fn test_health_checker_all_healthy() {
    let mut checker = HealthChecker::new("1.0.0");
    checker.add_check(AlwaysHealthyCheck);

    let status = checker.run().await;

    assert_eq!(status.status, "healthy");
    assert!(status.checks.contains_key("always_healthy"));
    assert_eq!(status.checks["always_healthy"].status, "healthy");
}

#[tokio::test]
async fn test_health_checker_unhealthy() {
    let mut checker = HealthChecker::new("1.0.0");
    checker.add_check(AlwaysHealthyCheck);
    checker.add_check(AlwaysUnhealthyCheck {
        reason: "Database connection failed".to_string(),
    });

    let status = checker.run().await;

    assert_eq!(status.status, "unhealthy");
    assert_eq!(status.checks["always_unhealthy"].status, "unhealthy");
    assert_eq!(
        status.checks["always_unhealthy"].message,
        Some("Database connection failed".to_string())
    );
}

#[tokio::test]
async fn test_health_checker_uptime() {
    let checker = HealthChecker::new("1.0.0");

    // Wait a bit
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let status = checker.run().await;

    assert!(status.uptime_seconds >= 0);
}
```

## Verification Commands

```bash
# Run all tests
cargo nextest run --all-features

# Run security audit
cargo audit

# Run benchmarks
cargo bench

# Build release
cargo build --release

# Docker build
docker build -t fraiseql/server:latest .

# Load test
k6 run tests/load/k6/graphql_load.js

# Check coverage
cargo llvm-cov --html

# Test feature flags
cargo nextest run -p fraiseql-runtime --test feature_flags_test

# Test migrations
cargo nextest run -p fraiseql-runtime --test migration_test
```

---

## Final Acceptance Criteria

- [ ] Unit test coverage > 80%
- [ ] Integration tests pass for all features
- [ ] E2E scenarios pass
- [ ] Load test: P95 < 500ms at 1000 RPS
- [ ] Security audit passes (no critical/high issues)
- [ ] Documentation complete (API, deployment, operations)
- [ ] Prometheus metrics exported
- [ ] Health checks working
- [ ] Docker image < 100MB
- [ ] Kubernetes manifests validated
- [ ] Runbook reviewed and approved

---

## Definition of Done

The FraiseQL Endpoint Runtime is production-ready when:

1. **Functionality**: All phases 1-9 implemented and working
2. **Quality**: Test coverage > 80%, no known critical bugs
3. **Performance**: P95 < 500ms, handles 1000+ RPS
4. **Security**: No critical vulnerabilities, audit passed
5. **Observability**: Metrics, logs, traces, health checks
6. **Documentation**: API docs, deployment guide, runbook
7. **Operations**: Docker, Kubernetes, CI/CD configured
