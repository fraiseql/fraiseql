# Phase 10: Production Hardening Roadmap

**Status**: Planning → In Progress
**Total Effort**: 3-4 weeks (14-20 implementation days)
**Target**: Production-ready FraiseQL v2 with enterprise features

---

## Overview

Phase 10 transforms FraiseQL v2 from a functionally complete system into a production-grade platform suitable for enterprise deployments. It covers security, reliability, scalability, and operational excellence.

---

## Phase 10.1: Admission Control, Rate Limiting & Backpressure (3 days)

### Objective
Prevent resource exhaustion and ensure fair sharing of system capacity.

### Components

#### 1.1 Token Bucket Rate Limiter
**Files**: `crates/fraiseql-server/src/rate_limit.rs` (NEW, ~200 lines)

```rust
pub struct RateLimiter {
    // Per-client rate limiting
    capacity: u32,                    // Max tokens per window
    refill_rate: u32,                 // Tokens per second
    windows: Arc<Mutex<HashMap<ClientId, TokenBucket>>>,
}

impl RateLimiter {
    pub fn new(capacity: u32, refill_rate: u32) -> Self { /* */ }
    pub async fn acquire(&self, client_id: &str, tokens: u32) -> Result<()> { /* */ }
}
```

**Features**:
- Per-client rate limiting (enforce max QPS)
- Per-action rate limiting (webhook calls, Slack messages, emails)
- Separate limits for read vs write operations
- Sliding window algorithm (more accurate than fixed windows)
- Graceful degradation (queue requests instead of rejecting immediately)

**Configuration**:
```toml
[rate_limiting]
# Global limits
global_max_qps = 10000
global_max_concurrent = 100

# Per-client limits
client_max_qps = 100
client_max_concurrent = 10

# Per-action limits
webhook_max_per_sec = 50
slack_max_per_sec = 10
email_max_per_sec = 5
```

#### 1.2 Admission Control
**Files**: `crates/fraiseql-server/src/admission.rs` (NEW, ~150 lines)

**Features**:
- Queue depth monitoring (reject if queue > threshold)
- Memory usage limits (GC or reject if >80%)
- Database connection pool saturation (queue if at capacity)
- Graceful rejection with "try again later" HTTP 429

```rust
pub struct AdmissionController {
    max_queue_depth: usize,
    max_memory_bytes: usize,
    reject_if_saturated: bool,
}

impl AdmissionController {
    pub async fn check_admission(&self, event: &EntityEvent) -> Result<()> {
        // Check queue depth
        if job_queue.depth() > self.max_queue_depth {
            return Err(RateLimitError::QueueFull.into());
        }
        // Check memory
        if memory_usage() > self.max_memory_bytes {
            return Err(RateLimitError::OutOfMemory.into());
        }
        Ok(())
    }
}
```

#### 1.3 Backpressure Handling
**Files**: Modified `crates/fraiseql-observers/src/queued_executor.rs` (~50 lines)

**Features**:
- Queue rejection policies (fail-fast vs queue):
  - `FailFast`: Reject if queue full (fast but loses events)
  - `QueueAll`: Always queue (buffers memory if consumers slow)
  - `Adaptive`: Switch based on memory pressure
- Exponential backoff for failed queuing attempts
- Metrics for backpressure events

#### 1.4 Configuration
**Files**: Modified `crates/fraiseql-observers/src/config.rs` (~30 lines)

```toml
[rate_limiting]
enabled = true
global_qps = 10000
per_client_qps = 100
token_refill_rate = 100

[backpressure]
queue_rejection_policy = "adaptive"  # or "fail-fast", "queue-all"
max_queue_memory_mb = 500
memory_threshold_percent = 80
```

### Tests
- Unit: Rate limiter token calculation, window sliding
- Integration: Concurrent requests, backpressure triggering
- Load: 10k QPS burst handling

### Verification
- [ ] `cargo clippy` clean
- [ ] `cargo test rate_limit*` passes
- [ ] Load test: 10k QPS for 60 seconds, no rejections
- [ ] Memory test: Backpressure kicks in at >80% memory

---

## Phase 10.2: Deployment Patterns (2 days)

### Objective
Document and automate deployment to Kubernetes, Docker, systemd.

### 2.1 Kubernetes Manifests
**Files**: `k8s/` directory (NEW, ~400 lines)

```yaml
# k8s/fraiseql-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-server
spec:
  replicas: 3  # HA setup
  template:
    spec:
      containers:
      - name: fraiseql-server
        image: fraiseql/fraiseql-server:latest
        env:
        - name: FRAISEQL_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: fraiseql-secrets
              key: database-url
        ports:
        - containerPort: 8000  # GraphQL
        - containerPort: 5432  # Arrow Flight
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
        readinessProbe:
          httpGet:
            path: /ready
            port: 8000
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"

---
# k8s/fraiseql-service.yaml
apiVersion: v1
kind: Service
metadata:
  name: fraiseql-service
spec:
  ports:
  - name: graphql
    port: 8000
    targetPort: 8000
  - name: arrow-flight
    port: 5432
    targetPort: 5432
  selector:
    app: fraiseql-server
  type: LoadBalancer  # Or NodePort for internal

---
# k8s/fraiseql-configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fraiseql-config
data:
  fraiseql.toml: |
    [server]
    bind_address = "0.0.0.0:8000"
    [observer]
    max_workers = 10
    [rate_limiting]
    global_qps = 10000
```

### 2.2 Docker Compose (Development)
**Files**: `docker-compose.yml` (UPDATED, includes all services)

```yaml
version: '3.8'
services:
  fraiseql-server:
    build: .
    ports:
      - "8000:8000"  # GraphQL
      - "5432:5432"  # Arrow Flight
    environment:
      DATABASE_URL: postgres://postgres:password@postgres:5432/fraiseql
      REDIS_URL: redis://redis:6379
      NATS_URL: nats://nats:4222
    depends_on:
      - postgres
      - redis
      - nats
      - clickhouse
      - elasticsearch

  postgres:
    image: postgres:16
    environment:
      POSTGRES_DB: fraiseql
      POSTGRES_PASSWORD: password

  redis:
    image: redis:7-alpine
    command: redis-server --appendonly yes

  nats:
    image: nats:2.10-alpine
    command: -js

  clickhouse:
    image: clickhouse/clickhouse-server:latest
    ports:
      - "8123:8123"

  elasticsearch:
    image: docker.elastic.co/elasticsearch/elasticsearch:8.0.0
    environment:
      xpack.security.enabled: "false"
```

### 2.3 Systemd Service (Single-Node)
**Files**: `systemd/fraiseql.service` (NEW)

```ini
[Unit]
Description=FraiseQL v2 GraphQL Server
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=fraiseql
WorkingDirectory=/opt/fraiseql
ExecStart=/usr/local/bin/fraiseql-server --config /etc/fraiseql/fraiseql.toml
Restart=on-failure
RestartSec=10s

[Install]
WantedBy=multi-user.target
```

### 2.4 Terraform/CloudFormation
**Files**: `infrastructure/` directory (NEW, ~500 lines)

- RDS PostgreSQL (managed, automated backups)
- ElastiCache Redis (cluster mode)
- Elasticsearch managed service
- EKS cluster definition
- Load balancer setup

### Tests
- [ ] Docker image builds and runs
- [ ] K8s manifests are valid (`kubectl apply --dry-run`)
- [ ] Healthchecks respond correctly
- [ ] Service discovery works

### Verification
- [ ] Docker: `docker-compose up` works
- [ ] Kubernetes: 3 replicas healthy
- [ ] Systemd: Service starts and stays running

---

## Phase 10.3: Circuit Breakers, Retry Logic & Graceful Degradation (3 days)

### Objective
Handle transient failures without cascading, degrade gracefully under load.

### 3.1 Circuit Breaker Pattern
**Files**: `crates/fraiseql-observers/src/circuit_breaker.rs` (NEW, ~250 lines)

```rust
pub enum CircuitState {
    Closed,     // Normal operation
    Open,       // Failing, reject requests
    HalfOpen,   // Testing if service recovered
}

pub struct CircuitBreaker {
    state: Arc<Mutex<CircuitState>>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: Fn() -> BoxFuture<'static, Result<T>>,
    {
        match self.state() {
            CircuitState::Closed => {
                match f().await {
                    Ok(result) => Ok(result),
                    Err(e) => {
                        self.record_failure();
                        Err(e)
                    }
                }
            }
            CircuitState::Open => Err(CircuitBreakerOpen.into()),
            CircuitState::HalfOpen => {
                // Try one request to see if service recovered
                f().await
            }
        }
    }
}
```

**Per-action circuit breakers**:
- Webhook action: threshold=5 failures in 60s → open
- Slack action: threshold=3 failures in 60s → open
- Email action: threshold=2 failures in 60s → open
- Database queries: threshold=10 failures → open

### 3.2 Enhanced Retry Logic
**Files**: Modified `crates/fraiseql-observers/src/job_queue/backoff.rs` (~100 lines)

**Enhancements**:
- Jitter to prevent thundering herd
- Per-error-type backoff (timeout vs auth failure)
- Exponential backoff with cap (max 5 minutes)
- Early exit for permanent errors (HTTP 4xx)

```rust
pub fn calculate_backoff(
    attempt: u32,
    strategy: BackoffStrategy,
    error: &ActionError,
) -> Duration {
    // Permanent error (4xx)? Don't retry
    if let ActionError::Http(status) = error {
        if status >= 400 && status < 500 {
            return Duration::ZERO;  // No retry
        }
    }

    // Transient error? Retry with backoff
    match strategy {
        BackoffStrategy::Exponential => {
            let base = Duration::from_millis(100);
            let delay = base.mul_f64(2.0_f64.powi(attempt as i32));
            let jitter = Duration::from_millis(rand::random::<u64>() % 100);
            std::cmp::min(delay + jitter, Duration::from_secs(300))
        }
        // ... Linear, Fixed, etc.
    }
}
```

### 3.3 Graceful Degradation
**Files**: Modified `crates/fraiseql-server/src/handler.rs` (~50 lines)

**Features**:
- If Redis unavailable: Queue in-memory (with persistence to disk)
- If ClickHouse unavailable: Buffer events locally
- If Elasticsearch unavailable: Skip indexing (fallback to direct search)
- If one action type failing: Others still work

```rust
impl QueuedObserverExecutor {
    pub async fn process_event(&self, event: &EntityEvent) -> Result<Summary> {
        // Try primary queue (Redis)
        match self.queue.enqueue(job).await {
            Ok(_) => return Ok(Summary::Queued),
            Err(_) if self.fallback_enabled => {
                // Fallback to in-memory queue with persistence
                self.in_memory_queue.push(job);
                self.persist_to_disk(&job);
                return Ok(Summary::DegradedMode);
            }
            Err(e) => return Err(e),
        }
    }
}
```

### 3.4 Error Handling
**Files**: Modified `crates/fraiseql-core/src/error.rs` (~30 lines)

**New error types**:
```rust
pub enum ObserverError {
    // Transient (retry)
    Timeout { message: String },
    NetworkError { message: String },
    DatabaseBusy { message: String },

    // Permanent (don't retry)
    Unauthorized { message: String },
    NotFound { message: String },
    ValidationError { message: String },

    // Circuit breaker
    CircuitBreakerOpen { service: String },
    ServiceDegraded { service: String },
}

impl ObserverError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Timeout | NetworkError | DatabaseBusy)
    }
}
```

### Tests
- [ ] Circuit breaker transitions correctly (Closed → Open → HalfOpen → Closed)
- [ ] Permanent errors not retried
- [ ] Exponential backoff with jitter works
- [ ] Graceful degradation activates when Redis down
- [ ] In-memory queue persists during Redis outage

### Verification
- [ ] Chaos test: Kill Redis, verify in-memory fallback
- [ ] Load test: Circuit breaker prevents cascading failures
- [ ] Backoff test: Jitter prevents thundering herd

---

## Phase 10.4: Performance Optimization & Profiling (2 days)

### Objective
Achieve target performance benchmarks (15-50x Arrow vs HTTP).

### 4.1 Profiling
**Tools**: Flamegraph, Criterion benchmarks

**What to profile**:
- Event → rule matching latency
- Rule → action dispatch latency
- Arrow conversion latency
- JSON serialization overhead

### 4.2 Optimization Targets
- Connection pooling (PostgreSQL, Redis, ClickHouse)
- Query plan caching
- Rule compilation caching
- Parallel rule evaluation
- Batch processing (multiple events per operation)

### Tests
- [ ] Flamegraph shows no surprises
- [ ] Arrow performance: >15x vs HTTP baseline
- [ ] Event latency: <100ms p95

### Verification
- [ ] Run benchmarks: `cargo bench`
- [ ] Compare: baseline vs optimized
- [ ] Document: bottlenecks and optimization strategies

---

## Phase 10.5: Authentication & Authorization (3 days) 🔴 CRITICAL

### Objective
Implement secure access control for all endpoints.

### 5.1 OAuth2/OIDC
**Files**: `crates/fraiseql-server/src/auth/oauth.rs` (NEW, ~300 lines)

**Integrations**:
- GitHub OAuth (for dev/testing)
- Google OAuth (for consumer)
- Keycloak (for enterprise SAML/LDAP)
- Azure AD (for enterprise)

```rust
pub struct OAuthProvider {
    client_id: String,
    client_secret: String,
    authorize_url: String,
    token_url: String,
    userinfo_url: String,
}

impl OAuthProvider {
    pub async fn get_token(&self, code: &str) -> Result<Token> { /* */ }
    pub async fn get_user(&self, token: &str) -> Result<OAuthUser> { /* */ }
}

// Usage: POST /auth/oauth/github/callback?code=XXX
```

### 5.2 JWT Token Management
**Files**: `crates/fraiseql-server/src/auth/jwt.rs` (NEW, ~200 lines)

**Features**:
- Issue JWT tokens on successful OAuth
- Validate JWT on every request
- Refresh token rotation
- Token expiration (15 min access, 7 day refresh)

```rust
pub struct JwtManager {
    secret: String,
    expiry: Duration,
}

impl JwtManager {
    pub fn issue_token(&self, user_id: &str, org_id: &str) -> Result<Token> { /* */ }
    pub fn validate_token(&self, token: &str) -> Result<Claims> { /* */ }
}

// Middleware: validates JWT on every HTTP request
pub async fn jwt_middleware(req: HttpRequest, next: Next) -> Result<HttpResponse> {
    let token = extract_token(&req)?;
    let claims = jwt_manager.validate_token(&token)?;
    req.extensions_mut().insert(claims);
    Ok(next.call(req).await)
}
```

### 5.3 Role-Based Access Control (RBAC)
**Files**: `crates/fraiseql-server/src/auth/rbac.rs` (NEW, ~200 lines)

**Roles**:
- **Admin**: View/create/edit/delete rules, manage users, access all data
- **Operator**: View/create/edit/delete rules, execute actions, view logs
- **Viewer**: View rules and logs (read-only)

**Resources**:
- Rules (get, list, create, update, delete)
- Actions (get, list, execute)
- Logs (get, list)
- Settings (get, update)

```rust
pub struct RbacPolicy {
    role: Role,
    permissions: HashMap<Resource, Vec<Action>>,
}

// Usage: Check permission before operation
fn require_permission(claims: &Claims, resource: Resource, action: Action) -> Result<()> {
    let policy = RbacPolicy::for_role(claims.role);
    if !policy.has_permission(resource, action) {
        return Err(ForbiddenError.into());
    }
    Ok(())
}
```

### 5.4 API Keys
**Files**: `crates/fraiseql-server/src/auth/api_key.rs` (NEW, ~150 lines)

**Features**:
- Create API keys for service-to-service auth
- Key scoping (specific resources)
- Rotation policy (keys expire after 90 days)
- Rate limiting per key

```rust
pub struct ApiKey {
    id: String,
    secret: String,  // Hashed
    scope: ApiScope,  // What can this key do?
    expires_at: DateTime,
    last_used: DateTime,
}

// Usage: Authorization: Bearer <api_key>
pub async fn api_key_middleware(req: HttpRequest, next: Next) -> Result<HttpResponse> {
    let key = extract_api_key(&req)?;
    let api_key = api_key_store.get(&key)?;
    if api_key.is_expired() {
        return Err(UnauthorizedError.into());
    }
    req.extensions_mut().insert(api_key);
    Ok(next.call(req).await)
}
```

### 5.5 Configuration
**Files**: Modified `crates/fraiseql-server/src/config.rs` (~50 lines)

```toml
[auth]
enabled = true

# OAuth2/OIDC
oauth_provider = "github"  # or "google", "keycloak"
oauth_client_id = "..."
oauth_client_secret = "..."

# JWT
jwt_secret = "..."  # From env or Vault
jwt_expiry_minutes = 15
refresh_token_expiry_days = 7

# RBAC
default_role = "viewer"  # or "operator", "admin"
admin_users = ["admin@example.com"]
```

### Tests
- [ ] OAuth flow works (mock OAuth provider)
- [ ] JWT validation rejects invalid tokens
- [ ] RBAC enforces permissions correctly
- [ ] API key authentication works
- [ ] Token expiration triggers refresh flow

### Verification
- [ ] HTTP 401 on missing token
- [ ] HTTP 403 on insufficient permissions
- [ ] Token refresh flow works
- [ ] API key scope limits access

---

## Phase 10.6: Multi-Tenancy & Data Isolation (3-4 days) 🔴 CRITICAL (if SaaS)

### Objective
Enforce strict data isolation between organizations.

### 6.1 Query-Level Isolation
**Files**: `crates/fraiseql-core/src/tenant.rs` (NEW, ~150 lines)

**Every query includes org_id filter**:

```sql
-- Before
SELECT * FROM observer_rules WHERE user_id = $1;

-- After (with org_id isolation)
SELECT * FROM observer_rules WHERE user_id = $1 AND org_id = $2;
```

**Implementation**:
- Middleware extracts org_id from JWT claims
- All database queries wrapped with org_id filter
- Request context includes org_id (cannot be overridden)

```rust
pub struct TenantContext {
    org_id: Uuid,
    user_id: Uuid,
}

// Middleware extracts from JWT
pub async fn tenant_middleware(req: HttpRequest, next: Next) -> Result<HttpResponse> {
    let claims = req.extensions().get::<Claims>().ok_or(Unauthorized)?;
    let tenant = TenantContext {
        org_id: claims.org_id,
        user_id: claims.user_id,
    };
    req.extensions_mut().insert(tenant);
    Ok(next.call(req).await)
}

// Usage: All queries automatically include org_id
impl ObserverRuleRepository {
    pub async fn get(&self, rule_id: Uuid, tenant: &TenantContext) -> Result<ObserverRule> {
        let rule = sqlx::query_as::<_, ObserverRule>(
            "SELECT * FROM observer_rules WHERE id = $1 AND org_id = $2"
        )
        .bind(rule_id)
        .bind(tenant.org_id)  // Cannot be bypassed
        .fetch_one(&self.pool)
        .await?;
        Ok(rule)
    }
}
```

### 6.2 Storage Isolation
**Files**: `crates/fraiseql-arrow/src/tenant.rs` (NEW, ~100 lines)

**ClickHouse views per organization**:
```sql
-- Main table (all orgs)
CREATE TABLE fraiseql_events (
    event_id UUID,
    org_id UUID,
    timestamp DateTime,
    data String,
    ...
) ENGINE = MergeTree()
ORDER BY (org_id, timestamp);

-- View for Org A (data isolation)
CREATE VIEW fraiseql_events_org_123 AS
SELECT * FROM fraiseql_events WHERE org_id = '123-456...';

-- View for Org B
CREATE VIEW fraiseql_events_org_789 AS
SELECT * FROM fraiseql_events WHERE org_id = '789-012...';
```

**Elasticsearch indices per organization**:
```json
{
  "fraiseql-events-org-123": {
    "settings": {
      "number_of_replicas": 2,
      "index.lifecycle.name": "fraiseql-policy"
    }
  },
  "fraiseql-events-org-789": { ... }
}
```

### 6.3 Job Queue Isolation
**Files**: Modified `crates/fraiseql-observers/src/job_queue/redis.rs` (~30 lines)

**Separate Redis queues per organization**:
```rust
pub fn queue_key(org_id: &Uuid) -> String {
    format!("fraiseql:queue:org:{}", org_id)
}

// Enqueue: adds to org-specific queue
pub async fn enqueue(&self, org_id: &Uuid, job: Job) -> Result<()> {
    let key = queue_key(org_id);
    redis_client.lpush(&key, serialize(&job)).await?;
    Ok(())
}

// Dequeue: consumes from org-specific queue
pub async fn dequeue(&self, org_id: &Uuid, count: usize) -> Result<Vec<Job>> {
    let key = queue_key(org_id);
    // ... dequeue from org-specific queue
}
```

### 6.4 Quota Enforcement
**Files**: `crates/fraiseql-server/src/quota.rs` (NEW, ~200 lines)

**Per-organization quotas**:
```toml
[quota.default]
max_rules = 100
max_actions_per_rule = 10
max_storage_gb = 100
max_qps = 1000
max_concurrent_connections = 50

[quota.enterprise]
max_rules = 10000
max_actions_per_rule = 1000
max_storage_gb = 1000
max_qps = 100000
max_concurrent_connections = 500
```

**Enforcement**:
```rust
pub struct QuotaManager {
    limits: HashMap<Uuid, QuotaLimits>,
    usage: Arc<Mutex<HashMap<Uuid, QuotaUsage>>>,
}

impl QuotaManager {
    pub async fn check_quota(&self, org_id: &Uuid, resource: &str) -> Result<()> {
        let usage = self.usage.lock().await;
        let limit = self.limits.get(org_id).ok_or(OrgNotFound)?;

        match resource {
            "rules" => {
                if usage[org_id].rule_count >= limit.max_rules {
                    return Err(QuotaExceeded::Rules.into());
                }
            }
            // ... other resources
        }
        Ok(())
    }
}
```

### 6.5 Audit Logging
**Files**: `crates/fraiseql-server/src/audit.rs` (NEW, ~150 lines)

**Log all operations per organization**:
```rust
pub struct AuditLog {
    timestamp: DateTime,
    org_id: Uuid,
    user_id: Uuid,
    action: AuditAction,
    resource: String,
    result: Result<(), String>,
}

impl AuditLog {
    pub async fn log(&self, entry: AuditLog) -> Result<()> {
        // Write to dedicated audit table
        sqlx::query(
            "INSERT INTO audit_logs (timestamp, org_id, user_id, action, resource, result)
             VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(entry.timestamp)
        .bind(entry.org_id)
        // ... etc
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

### Tests
- [ ] Org A cannot read Org B's rules
- [ ] Org A cannot execute Org B's actions
- [ ] Org A jobs isolated in separate queue
- [ ] Quota exceeded blocks operation
- [ ] Audit log records all operations per org

### Verification
- [ ] Run data isolation tests (try to read cross-org data, should fail)
- [ ] Run quota tests (exceed limit, should reject)
- [ ] Run audit tests (all ops logged)

---

## Phase 10.7: Distributed Tracing (1-2 days)

### Objective
Trace requests end-to-end for debugging and performance analysis.

### 7.1 OpenTelemetry Integration
**Files**: `crates/fraiseql-server/src/tracing.rs` (NEW, ~150 lines)

```rust
use opentelemetry::{api::KeyValue, sdk::trace as sdktrace, exporter::trace};

pub fn init_tracing() -> Result<()> {
    let exporter = jaeger::new_pipeline()
        .install_simple()
        .context("Failed to initialize Jaeger")?;

    let tracer = opentelemetry::global::tracer("fraiseql");
    Ok(())
}

// Middleware: create span for each request
pub async fn tracing_middleware(req: HttpRequest, next: Next) -> Result<HttpResponse> {
    let tracer = opentelemetry::global::tracer("fraiseql");
    let span = tracer.start("http_request");

    span.set_attribute(KeyValue::new("http.method", req.method().to_string()));
    span.set_attribute(KeyValue::new("http.url", req.uri().to_string()));

    let response = next.call(req).await?;

    span.set_attribute(KeyValue::new("http.status_code", response.status().as_u16() as i64));
    Ok(response)
}
```

### 7.2 Instrumentation Points
**Spans for**:
- HTTP request handling
- Database queries
- Rule evaluation
- Action execution
- Job queue operations

### Tests
- [ ] Traces exported to Jaeger
- [ ] Span hierarchy is correct
- [ ] Attributes are recorded

---

## Phase 10.8: Secrets Management (1-2 days)

### Objective
Secure handling of sensitive configuration (webhook URLs, API keys, tokens).

### 8.1 HashiCorp Vault Integration
**Files**: `crates/fraiseql-server/src/secrets.rs` (NEW, ~200 lines)

```rust
pub struct SecretManager {
    vault_client: VaultClient,
    cache: Arc<Mutex<HashMap<String, Secret>>>,
    cache_ttl: Duration,
}

impl SecretManager {
    pub async fn get_secret(&self, path: &str) -> Result<Secret> {
        // Check cache
        if let Some(secret) = self.cache.lock().await.get(path) {
            if !secret.is_expired() {
                return Ok(secret.clone());
            }
        }

        // Fetch from Vault
        let secret = self.vault_client.read(path).await?;
        self.cache.lock().await.insert(path.to_string(), secret.clone());
        Ok(secret)
    }
}
```

### 8.2 Configuration
**No secrets in TOML**:
```toml
[webhook_actions.example]
# ❌ Wrong: hardcoded secret
webhook_url = "https://hooks.example.com/SECRET123"

# ✅ Right: reference to Vault
webhook_url_secret = "vault://webhook-urls/example"
```

**Bootstrap**:
```bash
# Register secrets with Vault before starting
vault kv put secret/fraiseql/webhook-urls/example \
  url="https://hooks.example.com/SECRET123"

# Start server (reads from Vault)
VAULT_ADDR=http://vault:8200 \
VAULT_TOKEN=s.XXXXXX \
fraiseql-server --config fraiseql.toml
```

### Tests
- [ ] Secret fetching works
- [ ] Cache invalidation works
- [ ] Rotation without restart works

---

## Phase 10.9: Backup & Disaster Recovery (1 day)

### Objective
Ensure data recovery from failures.

### 9.1 Backup Strategy
**Components to backup**:
- PostgreSQL (observer rules, user data)
- Redis (job queue state)
- ClickHouse (event analytics)
- Elasticsearch (operational search indices)

**Backup frequency**:
- PostgreSQL: Hourly snapshots + WAL replication
- Redis: Daily dumps (AOF enabled for persistence)
- ClickHouse: Daily snapshots
- Elasticsearch: Daily snapshots

### 9.2 Recovery Runbook
**Document**:
1. Restore PostgreSQL from hourly backup
2. Restore Redis AOF or dump
3. Restore ClickHouse from snapshot
4. Restore Elasticsearch indices from snapshot
5. Verify data consistency
6. Run acceptance tests

**Expected RTO**: 1 hour
**Expected RPO**: Last hourly backup (max 1 hour data loss)

### 9.3 Disaster Recovery Tests
- Quarterly restore from backup
- Document any issues found
- Update runbook

---

## Phase 10.10: Encryption at Rest & In Transit (1-2 days)

### Objective
Protect data from unauthorized access.

### 10.1 TLS for All Connections

**HTTP/gRPC**:
```toml
[server]
bind_address = "0.0.0.0:8000"
tls_cert_path = "/etc/fraiseql/cert.pem"
tls_key_path = "/etc/fraiseql/key.pem"
# mTLS for Arrow Flight gRPC
require_client_cert = true
client_ca_path = "/etc/fraiseql/client-ca.pem"
```

**Database connections**:
```toml
[database]
url = "postgresql://user:pass@localhost:5432/fraiseql?sslmode=require"

[redis]
url = "rediss://localhost:6379"  # Secure Redis

[clickhouse]
url = "https://localhost:8123"
verify_cert = true
```

### 10.2 At-Rest Encryption
**ClickHouse** (if supported by version):
```sql
CREATE TABLE fraiseql_events (
    ...
) ENGINE = MergeTree()
WITH SETTINGS
    storage_disk_name = 'encrypted';
```

**Elasticsearch** (ILM with encryption):
```json
{
  "policy": "fraiseql-policy",
  "phases": {
    "hot": {
      "min_age": "0d",
      "actions": {
        "rollover": { "max_size": "50gb" }
      }
    }
  }
}
```

### Tests
- [ ] All connections use TLS
- [ ] Certificate validation works
- [ ] At-rest encryption enabled

---

## Summary: Phase 10 Phases at a Glance

| Phase | Effort | Dependencies | Go/No-Go |
|-------|--------|--------------|----------|
| 10.1 | 3 days | None | 🟡 Recommended |
| 10.2 | 2 days | None | 🟡 Recommended |
| 10.3 | 3 days | 10.1 | 🟡 Recommended |
| 10.4 | 2 days | None | 🟢 Optional (nice-to-have) |
| **10.5** | **3 days** | **None** | **🔴 CRITICAL** |
| **10.6** | **3-4 days** | **10.5** | **🔴 CRITICAL (SaaS)** |
| 10.7 | 1-2 days | None | 🟡 Recommended |
| **10.8** | **1-2 days** | **10.5** | **🔴 CRITICAL** |
| **10.9** | **1 day** | **None** | **🔴 CRITICAL** |
| **10.10** | **1-2 days** | **None** | **🔴 CRITICAL** |

**Total Critical Path**: 10.5 → 10.6, 10.8, 10.9, 10.10
**Total Effort**: 3-4 weeks

---

## Implementation Order (Recommended)

```
Week 1:
├─ 10.1: Rate limiting & admission control [3 days]
└─ 10.5: Auth & RBAC [3 days]  ← Critical, enables everything

Week 2:
├─ 10.3: Circuit breakers & resilience [3 days]
└─ 10.6: Multi-tenancy (if needed) [3-4 days]

Week 3:
├─ 10.8: Secrets management [1-2 days]
├─ 10.9: Backup & DR [1 day]
└─ 10.10: Encryption [1-2 days]

Week 4:
├─ 10.2: Deployment patterns [2 days]
├─ 10.7: Distributed tracing [1-2 days]
├─ 10.4: Performance optimization [2 days]
└─ Integration testing & polish [2-3 days]
```

---

## Success Criteria

- [ ] All Phase 10 tests passing
- [ ] Zero clippy warnings in new code
- [ ] Performance benchmarks met (15-50x Arrow vs HTTP)
- [ ] Security audit passed (no critical issues)
- [ ] Multi-tenant isolation verified
- [ ] Backup/restore tested successfully
- [ ] Documentation complete and accurate
- [ ] Deployment to Kubernetes working

---

**Status**: Ready to begin Phase 10
**Next Step**: Execute Phase 9.9 testing, then start Phase 10.5 (Auth)
**Owner**: You
**Timeline**: 3-4 weeks to production-ready

