# Phases 6-10: Extended fraiseql-server Features

## Overview

After Phase 4B (restructuring) and Phase 5 (auth), remaining phases extend the unified `fraiseql-server` crate with additional capabilities. Each phase adds a new module following the same pattern:

1. **Configuration** - Add config structs to `crate::config`
2. **Module** - Create `crate::feature_name/`
3. **Routes** - Add HTTP handlers to `crate::routes/`
4. **Integration** - Register in `AppState` and route builder
5. **Tests** - Unit + integration tests

---

## Phase 6: Observers & Events

**Objective**: Implement event/observer pattern for reactive business logic (e.g., trigger actions when data changes).

### Module Structure
```
crates/fraiseql-server/src/observers/
├── mod.rs           # Observer trait and registry
├── config.rs        # ObserverConfig
├── handler.rs       # Event dispatch and listener coordination
├── template.rs      # Template engine for actions
├── builtin/
│   ├── email.rs     # Email action observer
│   ├── webhook.rs   # Webhook action observer
│   ├── db.rs        # Database action observer
│   └── log.rs       # Logging observer
└── routes.rs        # Management endpoints
```

### Key Features

- **Event Types**: Define which database events trigger observers (INSERT, UPDATE, DELETE)
- **Action Templates**: Handlebars/Tera templates for parameterized actions
- **Built-in Observers**:
  - Email notifications
  - Webhook triggers
  - Database function calls
  - Logging
- **Custom Observers**: User-defined Lua/WASM (Phase 9)
- **Async Processing**: Queue or execute immediately based on config

### Configuration

```toml
[observers.user_created]
event = "INSERT"
entity = "User"

[observers.user_created.actions]
# Action 1: Send welcome email
type = "email"
template = "welcome_email"
to = "email"

# Action 2: Call webhook
[[observers.user_created.actions]]
type = "webhook"
url_env = "WELCOME_WEBHOOK_URL"

# Action 3: Call database function
[[observers.user_created.actions]]
type = "database"
function = "on_user_created"
```

### Example Implementation

```rust
// crates/fraiseql-server/src/observers/handler.rs

pub trait EventListener: Send + Sync {
    async fn on_event(&self, event: &Event) -> Result<(), ObserverError>;
}

pub struct ObserverHandler {
    listeners: HashMap<(String, EventType), Vec<Arc<dyn EventListener>>>,
}

impl ObserverHandler {
    pub async fn dispatch(&self, event: &Event) -> Result<(), ObserverError> {
        let key = (event.entity.clone(), event.event_type);
        if let Some(listeners) = self.listeners.get(&key) {
            for listener in listeners {
                listener.on_event(event).await?;
            }
        }
        Ok(())
    }
}

// Built-in observers
pub struct EmailObserver {
    template_engine: Arc<TemplateEngine>,
    mailer: Arc<Mailer>,
}

#[async_trait]
impl EventListener for EmailObserver {
    async fn on_event(&self, event: &Event) -> Result<(), ObserverError> {
        let config = self.get_config(&event.entity)?;
        let email_data = self.template_engine.render(&config.template, &event.data)?;
        self.mailer.send(email_data).await?;
        Ok(())
    }
}
```

### Database Schema

```sql
CREATE TABLE IF NOT EXISTS _system.observers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity TEXT NOT NULL,
    event_type TEXT NOT NULL,  -- INSERT, UPDATE, DELETE, CUSTOM
    name TEXT NOT NULL,
    enabled BOOLEAN DEFAULT true,
    config JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS _system.observer_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    observer_id UUID NOT NULL REFERENCES _system.observers(id),
    event_data JSONB NOT NULL,
    status TEXT NOT NULL,  -- success, failed, pending
    error_message TEXT,
    executed_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## Phase 7: Notifications & Delivery

**Objective**: Multi-channel notification delivery (email, SMS, push, Slack, Discord, webhooks).

### Module Structure
```
crates/fraiseql-server/src/notifications/
├── mod.rs           # Notification types and traits
├── config.rs        # Provider configs
├── handler.rs       # Orchestration
├── queue.rs         # Background delivery queue
├── providers/
│   ├── email/       # SMTP, SES, SendGrid
│   ├── sms/         # Twilio, AWS SNS
│   ├── push/        # Firebase, OneSignal
│   ├── slack.rs     # Slack API
│   ├── discord.rs   # Discord webhooks
│   ├── telegram.rs  # Telegram Bot API
│   └── webhook.rs   # Generic HTTP webhooks
└── routes.rs        # Send endpoints, delivery logs
```

### Key Features

- **Multi-Provider**: Use different providers for reliability (fallback to SMS if email fails)
- **Templates**: Handlebars templates with variable substitution
- **Delivery Queue**: Background processing with retry logic (exponential backoff)
- **Status Tracking**: Track delivery status (pending, sent, failed, bounced)
- **Batch Sending**: Efficient bulk notifications
- **Deduplication**: Avoid sending duplicate notifications within a time window

### Configuration

```toml
[notifications.email]
primary_provider = "sendgrid"

[notifications.email.providers.sendgrid]
type = "sendgrid"
api_key_env = "SENDGRID_API_KEY"

[notifications.email.providers.smtp_fallback]
type = "smtp"
host = "smtp.example.com"
port = 587

[notifications.sms]
provider = "twilio"
account_sid_env = "TWILIO_ACCOUNT_SID"
auth_token_env = "TWILIO_AUTH_TOKEN"

[notifications.push]
provider = "firebase"
credentials_env = "FIREBASE_CREDENTIALS_JSON"
```

### Example Implementation

```rust
// crates/fraiseql-server/src/notifications/handler.rs

#[async_trait]
pub trait NotificationProvider: Send + Sync {
    async fn send(&self, notification: &Notification) -> Result<MessageId, NotificationError>;
    async fn get_status(&self, message_id: &MessageId) -> Result<DeliveryStatus, NotificationError>;
}

pub struct NotificationHandler {
    providers: HashMap<Channel, Vec<Arc<dyn NotificationProvider>>>,
    queue: Arc<NotificationQueue>,
}

impl NotificationHandler {
    pub async fn send(
        &self,
        notification: Notification,
    ) -> Result<NotificationId, NotificationError> {
        let notification_id = Uuid::new_v4();

        // Queue for background delivery
        self.queue.enqueue(notification_id, notification).await?;

        Ok(notification_id)
    }

    pub async fn get_delivery_status(
        &self,
        notification_id: &NotificationId,
    ) -> Result<DeliveryStatus, NotificationError> {
        // Check database for status
        sqlx::query_scalar!(
            "SELECT status FROM _system.notification_logs WHERE id = $1",
            notification_id
        )
        .fetch_optional(&self.db)
        .await?
        .ok_or(NotificationError::NotFound)
    }
}
```

### Database Schema

```sql
CREATE TABLE IF NOT EXISTS _system.notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID,
    channel TEXT NOT NULL,  -- email, sms, push, slack
    recipient TEXT NOT NULL,
    subject TEXT,
    template TEXT NOT NULL,
    data JSONB NOT NULL,
    status TEXT DEFAULT 'pending',  -- pending, sent, failed, bounced
    error_message TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    sent_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS _system.notification_queue (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    notification_id UUID NOT NULL REFERENCES _system.notifications(id),
    attempt INT DEFAULT 0,
    next_retry TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_notifications_user ON _system.notifications(user_id);
CREATE INDEX idx_notification_queue_next_retry ON _system.notification_queue(next_retry);
```

---

## Phase 8: Advanced Features

**Objective**: Full-text search, caching, and job queues.

### 8A: Full-Text Search

```
crates/fraiseql-server/src/search/
├── mod.rs           # Search trait
├── postgres.rs      # PostgreSQL full-text search
├── meilisearch.rs   # Meilisearch integration
├── algolia.rs       # Algolia integration
└── routes.rs        # Search endpoints
```

**Features**:
- Entity indexing (automatic or manual)
- Faceted search
- Search suggestions/autocomplete
- Ranking and relevance

### 8B: Caching

```
crates/fraiseql-server/src/cache/
├── mod.rs           # Cache trait
├── memory.rs        # In-memory cache
├── redis.rs         # Redis backend
├── invalidation.rs  # Cache coherency and invalidation
└── routes.rs        # Cache management endpoints
```

**Features**:
- Query result caching
- Entity caching
- Cache invalidation on writes
- TTL management
- Cache warming

### 8C: Job Queues

```
crates/fraiseql-server/src/jobs/
├── mod.rs           # Job trait and scheduler
├── postgres.rs      # PostgreSQL LISTEN/NOTIFY
├── redis.rs         # Redis queue
├── worker.rs        # Background worker
└── routes.rs        # Job management
```

**Features**:
- Async job execution
- Scheduled jobs (cron-like)
- Job retries with backoff
- Job monitoring and logging

---

## Phase 9: Interceptors & Customization

**Objective**: User-defined logic via WASM/Lua (request/response interception, custom validation).

### Module Structure
```
crates/fraiseql-server/src/interceptors/
├── mod.rs           # Interceptor trait
├── wasm.rs          # WASM runtime (wasmtime)
├── lua.rs           # Lua runtime (mlua)
├── registry.rs      # Interceptor registry
├── sandbox.rs       # Security sandboxing
└── routes.rs        # Deploy/manage endpoints
```

### Key Features

- **Request Interceptors**: Modify incoming requests (auth, validation, transformation)
- **Response Interceptors**: Modify outgoing responses (filtering, transformation)
- **Custom Resolvers**: User-defined field resolution in GraphQL
- **Validation Rules**: Custom validation before mutations
- **Security Boundaries**: Isolate untrusted code with sandboxing

### Configuration

```toml
[interceptors.request_validation]
type = "wasm"
module_url = "https://example.com/validators/request.wasm"
events = ["before_query", "before_mutation"]

[interceptors.custom_field_resolver]
type = "lua"
source = "lua/resolvers/user.lua"
event = "field_resolve"
fields = ["User.avatar"]
```

### Example

```rust
// crates/fraiseql-server/src/interceptors/wasm.rs

pub struct WasmInterceptor {
    module: wasmtime::Module,
    instance: wasmtime::Instance,
}

impl WasmInterceptor {
    pub fn new(wasm_bytes: &[u8]) -> Result<Self, InterceptorError> {
        let engine = wasmtime::Engine::default();
        let module = wasmtime::Module::new(&engine, wasm_bytes)?;
        let mut store = wasmtime::Store::new(&engine, ());
        let instance = wasmtime::Instance::new(&mut store, &module, &[])?;

        Ok(Self { module, instance })
    }

    pub async fn intercept_request(
        &self,
        request: &GraphQLRequest,
    ) -> Result<GraphQLRequest, InterceptorError> {
        // Call WASM function with request data
        // Return modified request
        todo!("Call WASM intercept_request function")
    }
}
```

---

## Phase 10: Polish & Optimization

**Objective**: Performance optimization, observability, deployment tooling.

### 10A: Performance Optimization

- **Query Optimization**: Automatic N+1 query detection and fix
- **Batch Loading**: Automatic batching of resolver calls (DataLoader pattern)
- **Connection Pooling**: Optimize database connection reuse
- **Compression**: gzip/brotli response compression
- **Caching**: Full-page caching for expensive queries

### 10B: Observability

```
crates/fraiseql-server/src/observability/
├── tracing.rs       # Distributed tracing (OpenTelemetry)
├── logging.rs       # Structured logging (tracing + JSON)
├── metrics.rs       # Prometheus metrics
├── health.rs        # Health checks and diagnostic endpoints
└── profiling.rs     # Performance profiling
```

**Features**:
- OpenTelemetry integration
- Structured JSON logging
- Custom metrics (query latency, error rates, etc.)
- Performance profiling (flame graphs)
- Diagnostic endpoints

### 10C: Deployment

```
crates/fraiseql-server/src/deployment/
├── config_validation.rs  # Pre-flight validation
├── migrations.rs         # Database migration runner
├── health_checks.rs      # Startup/readiness checks
└── graceful_shutdown.rs  # Drain connections on exit
```

**Features**:
- Configuration validation on startup
- Database migration management
- Health check endpoints
- Graceful shutdown with connection draining

---

## Integration Pattern (All Phases)

Each phase follows this pattern:

### 1. Configuration Integration

```rust
// crates/fraiseql-server/src/config.rs
#[derive(Debug, Deserialize)]
pub struct RuntimeConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub lifecycle: LifecycleConfig,
    pub auth: Option<AuthConfig>,      // Phase 5
    pub observers: Option<ObserversConfig>,  // Phase 6
    pub notifications: Option<NotificationsConfig>,  // Phase 7
    pub search: Option<SearchConfig>,   // Phase 8A
    pub cache: Option<CacheConfig>,     // Phase 8B
    pub jobs: Option<JobsConfig>,       // Phase 8C
    pub interceptors: Option<InterceptorsConfig>,  // Phase 9
}
```

### 2. AppState Integration

```rust
// crates/fraiseql-server/src/state.rs
pub struct AppState {
    pub db: PgPool,
    pub config: RuntimeConfig,
    pub lifecycle: Arc<ShutdownCoordinator>,

    // Features
    pub webhooks: Option<Arc<WebhookHandler>>,
    pub files: Option<Arc<FileManager>>,
    pub auth: Option<Arc<AuthManager>>,
    pub observers: Option<Arc<ObserverRegistry>>,  // Phase 6
    pub notifications: Option<Arc<NotificationHandler>>,  // Phase 7
    pub search: Option<Arc<SearchService>>,  // Phase 8A
    pub cache: Option<Arc<CacheManager>>,    // Phase 8B
    pub jobs: Option<Arc<JobScheduler>>,     // Phase 8C
    pub interceptors: Option<Arc<InterceptorRegistry>>,  // Phase 9
}
```

### 3. Router Integration

```rust
// crates/fraiseql-server/src/routes/mod.rs
pub fn build_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/graphql", post(graphql::handler))
        .route("/health", get(health::liveness))
        .route("/ready", get(health::readiness))
        .nest("/webhooks", webhooks::routes(state.clone()))
        .nest("/files", files::routes(state.clone()))
        .nest("/auth", auth::routes(state.clone()))
        .nest("/observers", observers::routes(state.clone()))        // Phase 6
        .nest("/notifications", notifications::routes(state.clone()))  // Phase 7
        .nest("/search", search::routes(state.clone()))              // Phase 8A
        .nest("/cache", cache::routes(state.clone()))                // Phase 8B
        .nest("/jobs", jobs::routes(state.clone()))                  // Phase 8C
        .nest("/interceptors", interceptors::routes(state.clone()))  // Phase 9
        .with_state(state)
}
```

### 4. Testing

Each phase includes:
- Unit tests (`tests/unit/`)
- Integration tests (`tests/integration/`)
- Mock implementations (feature-gated)

---

## Summary

| Phase | Feature | Adds | Tests |
|-------|---------|------|-------|
| 4B | Restructuring | Unified server | Existing (still pass) |
| 5 | Auth | OAuth 2.0, JWT, Sessions | jwt, session, oauth |
| 6 | Observers | Event reactions | event, action, template |
| 7 | Notifications | Multi-channel delivery | email, sms, push |
| 8A | Full-Text Search | Query search | search, facets |
| 8B | Caching | Query caching | cache, invalidation |
| 8C | Jobs | Async jobs | queue, worker, scheduler |
| 9 | Interceptors | WASM/Lua customization | wasm, lua, sandbox |
| 10 | Polish | Optimization & observability | perf, metrics, health |

All phases build on `fraiseql-server`, reusing:
- Configuration system
- Error handling
- Middleware pipeline
- Database connections
- Tracing & metrics
- Testing utilities

---

## Development Order

1. **Phase 4B** - Restructure (consolidate crates)
2. **Phase 5** - Auth (foundation for user-specific features)
3. **Phases 6-7** - Observers & Notifications (business logic reactions)
4. **Phase 8** - Advanced (search, cache, jobs improve performance)
5. **Phase 9** - Interceptors (user customization)
6. **Phase 10** - Polish (optimization & observability)

Each phase is independent after Phase 4B, allowing parallel development.

---

## Notes

- Each phase adds new modules to fraiseql-server
- No new crates created after Phase 4B
- Configuration is unified in one TOML file
- Dependency injection through AppState
- Comprehensive testing throughout
- All features are optional (can be disabled via config)
