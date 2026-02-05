# Phase 13: Configuration Placeholders Wiring

**Objective**: Wire deferred configuration structures to functional implementations

**Duration**: 1-2 weeks

**Estimated LOC**: 1200-1500

**Dependencies**: Phases 10-12 complete

---

## Success Criteria

- [ ] All 7 configuration structs wired to implementations
- [ ] Configuration loading from TOML and environment variables
- [ ] Validation and error handling for all configs
- [ ] Configuration available in AppState
- [ ] Admin API for runtime configuration inspection
- [ ] Tests for each configuration module
- [ ] Zero clippy warnings

---

## Configuration Architecture

### Hierarchical Structure

Configurations are organized hierarchically to avoid TOML bloat and enable clear precedence:

```
fraiseql.toml
├── [core]                    # Core settings (database, auth)
├── [integrations]            # External service integrations
│   ├── [integrations.notifications]
│   ├── [integrations.logging]
│   ├── [integrations.search]
│   ├── [integrations.caching]
│   ├── [integrations.jobs]
│   ├── [integrations.realtime]
│   └── [integrations.secrets]
└── [extensions]              # Custom endpoints
```

### Loading Precedence

1. **Compiled defaults** (in binary during build)
2. **TOML file** (fraiseql.toml, if present)
3. **Environment variables** (overrides via PREFIX_SECTION_KEY pattern)

Example precedence:
```bash
# TOML defines it
[integrations.notifications]
slack_webhook = "https://hooks.slack.com/..."

# Can override via env var
FRAISEQL_NOTIFICATIONS_SLACK_WEBHOOK="https://override.example.com/..."
```

### Configuration Versioning

Configuration schema versioning ensures forward/backward compatibility:

```toml
[config_version]
schema = "2.1"  # Version in config file
# Runtime checks if loaded schema >= minimum_required
```

---

## Deferred Configuration Structures

These were placeholders from Phase 4. Now wire them to actual implementations:

```rust
// From crates/fraiseql-server/src/config/mod.rs

pub struct NotificationConfig {
    pub enabled: bool,
    pub slack: Option<SlackNotificationConfig>,
    pub email: Option<EmailNotificationConfig>,
    pub sms: Option<SmsNotificationConfig>,
}

pub struct AdvancedLoggingConfig {
    pub enabled: bool,
    pub elasticsearch: Option<ElasticsearchConfig>,
    pub datadog: Option<DatadogConfig>,
    pub splunk: Option<SplunkConfig>,
}

pub struct SearchIndexingConfig {
    pub enabled: bool,
    pub elasticsearch: Option<ElasticsearchIndexConfig>,
}

pub struct AdvancedCachingConfig {
    pub enabled: bool,
    pub redis: Option<RedisConfig>,
    pub memcached: Option<MemcachedConfig>,
}

pub struct JobQueueConfig {
    pub enabled: bool,
    pub backend: JobQueueBackend,
    pub redis: Option<RedisJobQueueConfig>,
}

pub struct RealtimeUpdatesConfig {
    pub enabled: bool,
    pub websocket: Option<WebSocketConfig>,
}

pub struct CustomEndpointConfig {
    pub enabled: bool,
    pub endpoints: Vec<CustomEndpoint>,
}

pub struct IntegrationsConfig {
    pub notifications: NotificationConfig,
    pub logging: AdvancedLoggingConfig,
    pub search: SearchIndexingConfig,
    pub caching: AdvancedCachingConfig,
    pub jobs: JobQueueConfig,
    pub realtime: RealtimeUpdatesConfig,
}

pub struct FullConfig {
    pub integrations: IntegrationsConfig,
    pub extensions: CustomEndpointConfig,
}
```

---

## TDD Cycles

### Cycle 13.1: Notification Configuration

**Objective**: Wire notification config to actual notification system

#### Files
- `crates/fraiseql-server/src/notifications/mod.rs` (new module)
- `crates/fraiseql-server/src/notifications/slack.rs`
- `crates/fraiseql-server/src/notifications/email.rs`
- `crates/fraiseql-server/src/notifications/sms.rs`

#### RED: Tests
```rust
#[tokio::test]
async fn test_notification_config_loading() {
    let config_str = r#"
    [notifications]
    enabled = true

    [notifications.slack]
    webhook_url = "https://hooks.slack.com/services/..."
    channel = "#alerts"

    [notifications.email]
    smtp_host = "smtp.example.com"
    from_address = "alerts@example.com"
    "#;

    let config: NotificationConfig = toml::from_str(config_str).unwrap();
    assert!(config.enabled);
    assert!(config.slack.is_some());
    assert!(config.email.is_some());
}

#[tokio::test]
async fn test_send_slack_notification() {
    let config = NotificationConfig {
        slack: Some(SlackNotificationConfig {
            webhook_url: "https://hooks.slack.com/...".to_string(),
            channel: "#alerts".to_string(),
        }),
        ..Default::default()
    };

    let notifier = NotificationService::new(config);
    let result = notifier.send_slack(
        "#alerts",
        "Test message",
    ).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_audit_event_trigger_notification() {
    // When security event occurs, should trigger notification
    let notifier = setup_notifier_with_slack();
    let event = AuditEvent::new(
        "suspicious_login_attempt",
        "u1",
        "alice",
        "192.168.1.100",
        json!({"failed_attempts": 5}),
    );

    notifier.on_audit_event(&event).await.unwrap();

    // Verify Slack notification was sent
}
```

#### GREEN: Implement notification service
```rust
pub trait NotificationBackend: Send + Sync {
    async fn send(&self, message: &str, metadata: &Value) -> Result<()>;
}

pub struct NotificationService {
    config: NotificationConfig,
    slack: Option<Arc<SlackBackend>>,
    email: Option<Arc<EmailBackend>>,
    sms: Option<Arc<SmsBackend>>,
}

impl NotificationService {
    pub fn new(config: NotificationConfig) -> Self {
        let slack = config.slack.as_ref().map(|c| {
            Arc::new(SlackBackend::new(c.clone()))
        });

        NotificationService {
            config,
            slack,
            email: None,
            sms: None,
        }
    }

    pub async fn notify_event(&self, event_type: &str, data: &Value) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let message = format!("Event: {} - {}", event_type, data);

        if let Some(slack) = &self.slack {
            slack.send(&message, data).await?;
        }

        Ok(())
    }
}
```

#### CLEANUP
- Test all notification backends
- Verify configuration parsing

---

### Cycle 13.2: Advanced Logging Configuration

**Objective**: Wire AdvancedLoggingConfig to centralized logging

#### Files
- `crates/fraiseql-server/src/logging/mod.rs`
- `crates/fraiseql-server/src/logging/elasticsearch.rs`
- `crates/fraiseql-server/src/logging/datadog.rs`

#### Tests
```rust
#[tokio::test]
async fn test_elasticsearch_logging_config() {
    let config_str = r#"
    [advanced_logging]
    enabled = true

    [advanced_logging.elasticsearch]
    hosts = ["localhost:9200"]
    index_pattern = "fraiseql-logs-{yyyy.MM.dd}"
    "#;

    let config: AdvancedLoggingConfig = toml::from_str(config_str).unwrap();
    assert!(config.enabled);

    let logger = ElasticsearchLogger::new(config.elasticsearch.unwrap()).await?;

    logger.log(json!({
        "level": "ERROR",
        "message": "Database connection failed",
        "timestamp": Utc::now()
    })).await?;
}

#[tokio::test]
async fn test_datadog_logging_config() {
    let config = AdvancedLoggingConfig {
        datadog: Some(DatadogConfig {
            api_key: "secret-key".to_string(),
            endpoint: "https://http-intake.logs.datadoghq.com/v1/input".to_string(),
        }),
        ..Default::default()
    };

    let logger = DatadogLogger::new(config.datadog.unwrap());

    logger.log(json!({
        "message": "Query executed",
        "duration_ms": 123
    })).await?;
}
```

#### GREEN: Implement centralized logging
```rust
pub trait LogBackend: Send + Sync {
    async fn log(&self, entry: LogEntry) -> Result<()>;
    async fn query(&self, filter: &LogFilter) -> Result<Vec<LogEntry>>;
}

pub struct CentralizedLogger {
    backends: Vec<Arc<dyn LogBackend>>,
}

impl CentralizedLogger {
    pub async fn log(&self, entry: LogEntry) -> Result<()> {
        for backend in &self.backends {
            backend.log(entry.clone()).await?;
        }
        Ok(())
    }
}
```

---

### Cycle 13.3: Search Indexing Configuration

**Objective**: Wire SearchIndexingConfig to Elasticsearch

#### Files
- `crates/fraiseql-server/src/search/mod.rs`
- `crates/fraiseql-server/src/search/elasticsearch.rs`

#### Features
- Index GraphQL queries for analysis
- Full-text search over execution history
- Performance trending

#### Tests
```rust
#[tokio::test]
async fn test_search_indexing_config() {
    let config_str = r#"
    [search_indexing]
    enabled = true

    [search_indexing.elasticsearch]
    hosts = ["localhost:9200"]
    index_pattern = "fraiseql-search-{yyyy.MM.dd}"
    "#;

    let config: SearchIndexingConfig = toml::from_str(config_str).unwrap();
    let search = SearchService::new(config).await?;

    // Index a query
    search.index_query("query { users { id name } }", 45.2, true).await?;

    // Search queries
    let results = search.search("users").await?;
    assert!(results.len() > 0);
}
```

---

### Cycle 13.4: Advanced Caching Configuration

**Objective**: Wire AdvancedCachingConfig to multi-tier caching

#### Files
- `crates/fraiseql-server/src/cache_advanced/mod.rs`

#### Features
- Two-tier caching (memory + Redis)
- Distributed cache invalidation
- Cache warming

#### Tests
```rust
#[tokio::test]
async fn test_redis_caching_config() {
    let config_str = r#"
    [advanced_caching]
    enabled = true

    [advanced_caching.redis]
    host = "localhost"
    port = 6379
    db = 0
    ttl_seconds = 3600
    "#;

    let config: AdvancedCachingConfig = toml::from_str(config_str).unwrap();
    let cache = AdvancedCache::new(config).await?;

    cache.set("key1", "value1").await?;
    let value = cache.get("key1").await?;
    assert_eq!(value, Some("value1"));

    // Verify distributed cache
    assert!(cache.is_distributed());
}
```

---

### Cycle 13.5: Job Queue Configuration

**Objective**: Wire JobQueueConfig to background job system

#### Files
- `crates/fraiseql-server/src/jobs/mod.rs`
- `crates/fraiseql-server/src/jobs/queue.rs`

#### Features
- Asynchronous query execution
- Scheduled tasks
- Job status tracking

#### Tests
```rust
#[tokio::test]
async fn test_job_queue_config() {
    let config_str = r#"
    [job_queue]
    enabled = true
    backend = "redis"

    [job_queue.redis]
    host = "localhost"
    port = 6379
    "#;

    let config: JobQueueConfig = toml::from_str(config_str).unwrap();
    let queue = JobQueue::new(config).await?;

    // Enqueue job
    let job_id = queue.enqueue(Job {
        query: "query { expensiveQuery }".to_string(),
        priority: 5,
    }).await?;

    // Poll status
    let status = queue.status(&job_id).await?;
    assert_eq!(status, JobStatus::Queued);
}
```

---

### Cycle 13.6: Realtime Updates Configuration

**Objective**: Wire RealtimeUpdatesConfig to WebSocket subscriptions

#### Files
- `crates/fraiseql-server/src/realtime/mod.rs`
- `crates/fraiseql-server/src/realtime/websocket.rs`

#### Features
- GraphQL subscriptions
- WebSocket connections
- Subscription management

#### Tests
```rust
#[tokio::test]
async fn test_websocket_config() {
    let config_str = r#"
    [realtime_updates]
    enabled = true

    [realtime_updates.websocket]
    max_connections = 10000
    heartbeat_interval_secs = 30
    message_buffer_size = 1000
    "#;

    let config: RealtimeUpdatesConfig = toml::from_str(config_str).unwrap();
    let ws_service = WebSocketService::new(config).await?;

    // Test subscription
    let subscription = "subscription { userLoggedIn { userId timestamp } }";
    let result = ws_service.subscribe("client-1", subscription).await?;

    assert!(result.subscription_id.is_some());
}
```

---

### Cycle 13.7: Custom Endpoint Configuration

**Objective**: Wire CustomEndpointConfig to extensibility system

#### Files
- `crates/fraiseql-server/src/extensions/mod.rs`

#### Features
- Custom HTTP endpoints
- Middleware hooks
- Plugin system

#### Tests
```rust
#[tokio::test]
async fn test_custom_endpoint_config() {
    let config_str = r#"
    [custom_endpoints]
    enabled = true

    [[custom_endpoints.endpoints]]
    path = "/api/v1/custom/health"
    method = "GET"
    handler = "custom_health_handler"

    [[custom_endpoints.endpoints]]
    path = "/api/v1/custom/stats"
    method = "GET"
    handler = "custom_stats_handler"
    "#;

    let config: CustomEndpointConfig = toml::from_str(config_str).unwrap();
    let server = Server::new(config)?;

    // Register custom endpoints
    for endpoint in config.endpoints {
        server.register_custom_endpoint(&endpoint)?;
    }

    // Test custom endpoint
    let client = setup_test_client(&server);
    let response = client.get("/api/v1/custom/health").send().await?;
    assert_eq!(response.status(), 200);
}
```

---

### Cycle 13.8: Configuration Versioning & Precedence

**Objective**: Implement configuration schema versioning and environment override precedence

#### Files
- `crates/fraiseql-server/src/config/versioning.rs`
- `crates/fraiseql-server/src/config/loader.rs`

#### Tests
```rust
#[test]
fn test_config_version_schema() {
    let config_str = r#"
    [config_version]
    schema = "2.1"
    "#;

    let config: ConfigWithVersion = toml::from_str(config_str).unwrap();
    assert_eq!(config.version.schema, "2.1");
}

#[test]
fn test_config_loading_precedence() {
    // Set environment variable
    std::env::set_var("FRAISEQL_NOTIFICATIONS_SLACK_WEBHOOK", "https://env.example.com");

    let config = ConfigLoader::new()
        .with_toml_file("fraiseql.toml")
        .with_env_overrides()
        .load()
        .unwrap();

    // Env var should override TOML
    assert_eq!(
        config.integrations.notifications.slack.as_ref().unwrap().webhook_url,
        "https://env.example.com"
    );
}

#[test]
fn test_env_var_pattern_parsing() {
    // FRAISEQL_INTEGRATIONS_LOGGING_ELASTICSEARCH_HOSTS=localhost:9200
    let (section, key, value) = parse_env_var(
        "FRAISEQL_INTEGRATIONS_LOGGING_ELASTICSEARCH_HOSTS"
    ).unwrap();

    assert_eq!(section, "integrations.logging.elasticsearch");
    assert_eq!(key, "hosts");
}

#[test]
fn test_config_backward_compatibility() {
    // Old v2.0 config should load with v2.1 schema
    let old_config_str = r#"
    [notifications]  # Old flat structure
    slack_webhook = "https://..."
    "#;

    let migrator = ConfigMigrator::new();
    let result = migrator.migrate_from_v2_0(old_config_str).unwrap();

    // Should be transformed to new hierarchical structure
    assert!(result.contains("[integrations.notifications]"));
}

#[test]
fn test_config_validation_error_reporting() {
    let config_str = r#"
    [integrations.notifications]
    slack_webhook = "not-a-valid-url"
    "#;

    let result = validate_config(config_str);
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(err.contains("invalid URL format"));
    assert!(err.contains("integrations.notifications.slack_webhook"));
}

#[tokio::test]
async fn test_config_audit_trail() {
    let loader = ConfigLoader::new();
    loader.with_env_overrides().load().unwrap();

    let audit = loader.get_audit_trail();
    // Should show which configs were loaded from where
    assert!(audit.contains("Loaded from: fraiseql.toml"));
    assert!(audit.contains("Override: FRAISEQL_NOTIFICATIONS_..."));
}
```

#### Implementation
```rust
pub struct ConfigVersion {
    pub schema: String,
    pub minimum_required: String,
}

pub struct ConfigWithVersion {
    pub version: ConfigVersion,
    pub integrations: IntegrationsConfig,
}

pub struct ConfigLoader {
    defaults: FullConfig,
    toml_file: Option<String>,
    env_overrides: bool,
    audit_trail: Vec<String>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        ConfigLoader {
            defaults: FullConfig::defaults(),
            toml_file: None,
            env_overrides: false,
            audit_trail: vec![],
        }
    }

    pub fn with_toml_file(mut self, path: &str) -> Self {
        self.toml_file = Some(path.to_string());
        self
    }

    pub fn with_env_overrides(mut self) -> Self {
        self.env_overrides = true;
        self
    }

    pub fn load(mut self) -> Result<FullConfig> {
        let mut config = self.defaults.clone();

        // Load from TOML file
        if let Some(path) = &self.toml_file {
            let content = fs::read_to_string(path)?;
            let file_config: FullConfig = toml::from_str(&content)?;
            config = self.merge_configs(config, file_config)?;
            self.audit_trail.push(format!("Loaded from: {}", path));
        }

        // Apply environment variable overrides
        if self.env_overrides {
            config = self.apply_env_overrides(config)?;
        }

        // Validate configuration
        self.validate(&config)?;

        Ok(config)
    }

    fn apply_env_overrides(&mut self, mut config: FullConfig) -> Result<FullConfig> {
        let prefix = "FRAISEQL_";

        for (key, value) in std::env::vars() {
            if key.starts_with(prefix) {
                let config_key = key[prefix.len()..].to_lowercase();
                self.apply_override(&mut config, &config_key, &value)?;
                self.audit_trail.push(format!("Override: {} = {}", key, "***"));
            }
        }

        Ok(config)
    }

    fn merge_configs(&self, mut base: FullConfig, overlay: FullConfig) -> Result<FullConfig> {
        // Hierarchical merge (not replace)
        // Only override fields that are explicitly set in overlay
        base.integrations.merge_with(overlay.integrations);
        Ok(base)
    }

    fn validate(&self, config: &FullConfig) -> Result<()> {
        // Validate all configurations
        if config.integrations.notifications.enabled {
            if config.integrations.notifications.slack.is_none()
                && config.integrations.notifications.email.is_none()
                && config.integrations.notifications.sms.is_none()
            {
                return Err("Notifications enabled but no backend configured".into());
            }
        }

        // Similar validation for other integrations...

        Ok(())
    }

    pub fn get_audit_trail(&self) -> String {
        self.audit_trail.join("\n")
    }
}

pub struct ConfigMigrator;

impl ConfigMigrator {
    pub fn migrate_from_v2_0(&self, old_config: &str) -> Result<String> {
        // Parse old format and transform to new hierarchical format
        let old: serde_toml::Value = toml::from_str(old_config)?;

        // Transform old flat [notifications] to new [integrations.notifications]
        let mut transformed = old.clone();

        // Move sections under [integrations]
        if let Some(notifications) = old.get("notifications") {
            transformed["integrations"]["notifications"] = notifications.clone();
            transformed.as_table_mut().unwrap().remove("notifications");
        }

        Ok(toml::to_string_pretty(&transformed)?)
    }
}
```

#### CLEANUP
- Verify config loading precedence
- Test env var parsing
- Validate backward compatibility

---

### Cycle 13.9: Configuration Integration & Admin API

**Objective**: Integrate all configurations into AppState and expose via admin API

#### Files
- `crates/fraiseql-server/src/routes/api/config.rs` (new/updated)
- `crates/fraiseql-server/src/app_state.rs` (updated)

#### Endpoints
```
GET  /api/v1/admin/config/all              # Get full config
GET  /api/v1/admin/config/integrations     # Get integrations config
POST /api/v1/admin/config/validate         # Validate config
GET  /api/v1/admin/config/audit-trail      # Config loading audit trail
```

#### Tests
```rust
#[tokio::test]
async fn test_config_admin_api() {
    let client = setup_test_client().await;
    let admin_token = get_admin_token();

    let response = client
        .get("/api/v1/admin/config/all")
        .header("Authorization", &admin_token)
        .send()
        .await?;

    assert_eq!(response.status(), 200);
    let config = response.json::<FullConfig>().await?;

    assert!(config.notifications.is_some());
    assert!(config.advanced_logging.is_some());
    assert!(config.advanced_caching.is_some());
}

#[tokio::test]
async fn test_config_sanitization() {
    // Ensure sensitive values redacted in API responses
    let response = client.get("/api/v1/admin/config/all").send().await?;
    let config_json = response.text().await?;

    assert!(!config_json.contains("slack_webhook_url"));
    assert!(!config_json.contains("smtp_password"));
    assert!(config_json.contains("***"));
}
```

#### Implementation
```rust
pub struct FullConfig {
    pub notifications: Option<NotificationConfig>,
    pub advanced_logging: Option<AdvancedLoggingConfig>,
    pub search_indexing: Option<SearchIndexingConfig>,
    pub advanced_caching: Option<AdvancedCachingConfig>,
    pub job_queue: Option<JobQueueConfig>,
    pub realtime_updates: Option<RealtimeUpdatesConfig>,
    pub custom_endpoints: Option<CustomEndpointConfig>,
}

impl AppState {
    pub fn load_all_configs(config_path: &str) -> Result<Self> {
        let config_str = fs::read_to_string(config_path)?;
        let config: FullConfig = toml::from_str(&config_str)?;

        // Apply environment variable overrides
        let config = Self::apply_env_overrides(config)?;

        // Validate all configurations
        config.validate()?;

        Ok(AppState {
            notifications: config.notifications.map(Arc::new),
            advanced_logging: config.advanced_logging.map(Arc::new),
            advanced_caching: config.advanced_caching.map(Arc::new),
            job_queue: config.job_queue.map(Arc::new),
            realtime_updates: config.realtime_updates.map(Arc::new),
            // ... other fields
        })
    }

    fn apply_env_overrides(mut config: FullConfig) -> Result<FullConfig> {
        if let Ok(slack_webhook) = std::env::var("SLACK_WEBHOOK_URL") {
            if let Some(ref mut notif) = config.notifications {
                if let Some(ref mut slack) = notif.slack {
                    slack.webhook_url = slack_webhook;
                }
            }
        }

        // Similar for other configs...

        Ok(config)
    }
}
```

---

### Cycle 13.10: TOML Configuration File Examples (Hierarchical)

#### Files
- `fraiseql.toml.example` (comprehensive example)
- `fraiseql.toml.production` (production template)
- `fraiseql.toml.development` (development template)

#### Example Content
```toml
# fraiseql.toml - Complete hierarchical configuration

[config_version]
schema = "2.1"
minimum_required = "2.1"

# ============================================================================
# Core Configuration (database, auth, etc.)
# ============================================================================
[core]
database_url = "postgresql://localhost/fraiseql"
listen_addr = "0.0.0.0:8815"
log_level = "info"
jwt_secret = "${JWT_SECRET}"  # Must be set via env var for security

# ============================================================================
# Integrations - External Services
# ============================================================================
[integrations]
# Enable/disable entire integration groups

[integrations.notifications]
enabled = true
# Can override the entire section via FRAISEQL_INTEGRATIONS_NOTIFICATIONS_ENABLED

  [integrations.notifications.slack]
  enabled = true
  webhook_url = "${SLACK_WEBHOOK_URL}"  # Override: FRAISEQL_INTEGRATIONS_NOTIFICATIONS_SLACK_WEBHOOK_URL
  channel = "#fraiseql-alerts"
  # Can also use: FRAISEQL_INTEGRATIONS_NOTIFICATIONS_SLACK_CHANNEL

  [integrations.notifications.email]
  enabled = false
  smtp_host = "smtp.example.com"
  smtp_port = 587
  from_address = "alerts@fraiseql.example.com"

[integrations.logging]
enabled = true

  [integrations.logging.elasticsearch]
  enabled = true
  hosts = ["localhost:9200"]
  index_pattern = "fraiseql-logs-{yyyy.MM.dd}"
  bulk_size = 1000

  [integrations.logging.datadog]
  enabled = false
  api_key = "${DATADOG_API_KEY}"

[integrations.search]
enabled = true

  [integrations.search.elasticsearch]
  enabled = true
  hosts = ["localhost:9200"]
  index_pattern = "fraiseql-search-{yyyy.MM.dd}"

[integrations.caching]
enabled = true

  [integrations.caching.redis]
  enabled = true
  host = "localhost"
  port = 6379
  db = 0
  ttl_seconds = 3600

[integrations.jobs]
enabled = true
backend = "redis"  # or "postgres", "memory"

  [integrations.jobs.redis]
  enabled = true
  host = "localhost"
  port = 6379
  db = 1

[integrations.realtime]
enabled = true

  [integrations.realtime.websocket]
  enabled = true
  max_connections = 10000
  heartbeat_interval_secs = 30
  message_buffer_size = 1000

[integrations.secrets]
enabled = true
backend = "vault"  # or "env", "file"

  [integrations.secrets.vault]
  enabled = true
  addr = "https://vault.example.com:8200"
  namespace = "fraiseql/prod"
  # Token: use FRAISEQL_INTEGRATIONS_SECRETS_VAULT_TOKEN env var

  [integrations.secrets.rotation]
  interval_days = 7
  grace_period_minutes = 30

    # Per-credential overrides
    [integrations.secrets.rotation.database]
    interval_days = 7

    [integrations.secrets.rotation.oauth]
    interval_days = 90

# ============================================================================
# Extensions - Custom Endpoints & Middleware
# ============================================================================
[extensions]
enabled = false

  [[extensions.endpoints]]
  path = "/api/v1/custom/health"
  method = "GET"
  handler = "custom_health_handler"

  [[extensions.endpoints]]
  path = "/api/v1/custom/stats"
  method = "GET"
  handler = "custom_stats_handler"
```

#### Environment Override Examples
```bash
# Override via environment variables (use this in production)

# Core settings
export FRAISEQL_CORE_LOG_LEVEL=debug
export FRAISEQL_CORE_JWT_SECRET="your-secret-key"

# Notifications (hierarchical path)
export FRAISEQL_INTEGRATIONS_NOTIFICATIONS_ENABLED=true
export FRAISEQL_INTEGRATIONS_NOTIFICATIONS_SLACK_WEBHOOK_URL="https://hooks.slack.com/..."

# Caching
export FRAISEQL_INTEGRATIONS_CACHING_REDIS_HOST="redis.example.com"
export FRAISEQL_INTEGRATIONS_CACHING_REDIS_PORT=6379

# Secrets
export FRAISEQL_INTEGRATIONS_SECRETS_VAULT_ADDR="https://vault.example.com"
export FRAISEQL_INTEGRATIONS_SECRETS_VAULT_TOKEN="s.xxxxxxxx"
export FRAISEQL_INTEGRATIONS_SECRETS_ROTATION_INTERVAL_DAYS=7
```

---

## Verification

```bash
# Configuration validation
cargo test --lib config

# Configuration integration
cargo test --lib route::api::config

# TOML parsing
cargo test --lib toml_parsing

# Full integration
cargo nextest run configuration_integration
```

---

## Status

- [ ] Not Started
- [ ] In Progress (Cycle X)
- [ ] Complete

---

## Next Phase

→ Phase 14: Observability & Compliance
