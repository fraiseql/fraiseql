# FraiseQL v2: Advanced Features Architecture

**Companion to:** RUST_CORE_ARCHITECTURE.md
**Date:** 2026-01-11
**Status:** Complete Architecture Extension

---

## Table of Contents

1. [Extension Points & Pluggability](#extension-points--pluggability)
2. [Federation Architecture](#federation-architecture)
3. [Enterprise RBAC Integration](#enterprise-rbac-integration)
4. [Subscriptions & Event Streaming](#subscriptions--event-streaming)
5. [Observability & Instrumentation](#observability--instrumentation)
6. [Custom Authorization Rules](#custom-authorization-rules)
7. [Validation Hooks](#validation-hooks)
8. [Transport Adapters](#transport-adapters)

---

## Extension Points & Pluggability

### Core Principle: Trait-Based Extension

The architecture designed in RUST_CORE_ARCHITECTURE.md uses **trait-based abstraction** throughout, making all components pluggable:

```rust
// Database backends are pluggable
pub trait DatabaseAdapter: Send + Sync { /* ... */ }

// WHERE generation is pluggable per database
pub trait WhereClauseGenerator { /* ... */ }

// Projection logic is pluggable
pub trait JsonbProjector { /* ... */ }

// Cache backends are pluggable
#[async_trait]
pub trait CacheBackend: Send + Sync { /* ... */ }
```

### Extension Point Registry

**New module:** `crates/fraiseql-core/src/extensions/mod.rs`

```rust
/// Registry for all extension points.
pub struct ExtensionRegistry {
    authorization_rules: HashMap<String, Box<dyn AuthorizationRule>>,
    validators: HashMap<String, Box<dyn Validator>>,
    middleware: Vec<Box<dyn Middleware>>,
    hooks: HookRegistry,
}

impl ExtensionRegistry {
    /// Register custom authorization rule.
    pub fn register_auth_rule<R: AuthorizationRule + 'static>(
        &mut self,
        name: impl Into<String>,
        rule: R,
    ) {
        self.authorization_rules.insert(name.into(), Box::new(rule));
    }

    /// Register custom validator.
    pub fn register_validator<V: Validator + 'static>(
        &mut self,
        name: impl Into<String>,
        validator: V,
    ) {
        self.validators.insert(name.into(), Box::new(validator));
    }

    /// Add middleware to execution pipeline.
    pub fn add_middleware<M: Middleware + 'static>(&mut self, middleware: M) {
        self.middleware.push(Box::new(middleware));
    }
}
```

### Custom Authorization Rule Trait

```rust
/// Custom authorization rule extension point.
#[async_trait]
pub trait AuthorizationRule: Send + Sync {
    /// Check if user is authorized to access resource.
    ///
    /// # Arguments
    ///
    /// * `resource` - The resource being accessed (as JSONB)
    /// * `user_context` - User's roles, permissions, tenant
    /// * `db` - Database connection for context queries
    ///
    /// # Returns
    ///
    /// `true` if authorized, `false` otherwise.
    async fn is_authorized(
        &self,
        resource: &serde_json::Value,
        user_context: &UserContext,
        db: &dyn DatabaseAdapter,
    ) -> Result<bool>;

    /// Optional: Cache key fields for performance.
    fn cache_key_fields(&self) -> Vec<&str> {
        vec![]
    }

    /// Optional: Cache TTL in seconds.
    fn cache_ttl_seconds(&self) -> Option<u64> {
        None
    }
}
```

**Example: Team Member Authorization Rule**

```rust
pub struct TeamMemberRule;

#[async_trait]
impl AuthorizationRule for TeamMemberRule {
    async fn is_authorized(
        &self,
        resource: &serde_json::Value,
        user_context: &UserContext,
        db: &dyn DatabaseAdapter,
    ) -> Result<bool> {
        // Extract team_id from resource
        let team_id = resource
            .get("team_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| FraiseQLError::validation("Resource missing team_id"))?;

        // Check if user is in team (could be cached)
        let is_member = user_context.team_ids.contains(&team_id.to_string());

        Ok(is_member)
    }

    fn cache_key_fields(&self) -> Vec<&str> {
        vec!["team_id"]
    }

    fn cache_ttl_seconds(&self) -> Option<u64> {
        Some(300) // 5 minutes
    }
}

// Register at startup
let mut registry = ExtensionRegistry::new();
registry.register_auth_rule("team_member", TeamMemberRule);
```

### Validator Trait

```rust
/// Custom validator extension point.
#[async_trait]
pub trait Validator: Send + Sync {
    /// Validate a value.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if validation fails.
    async fn validate(
        &self,
        value: &serde_json::Value,
        db: Option<&dyn DatabaseAdapter>,
    ) -> Result<()>;
}
```

**Example: Email Domain Validator**

```rust
pub struct EmailDomainValidator {
    allowed_domains: Vec<String>,
}

#[async_trait]
impl Validator for EmailDomainValidator {
    async fn validate(
        &self,
        value: &serde_json::Value,
        _db: Option<&dyn DatabaseAdapter>,
    ) -> Result<()> {
        let email = value
            .as_str()
            .ok_or_else(|| FraiseQLError::validation("Email must be string"))?;

        let domain = email.split('@').nth(1).ok_or_else(|| {
            FraiseQLError::validation("Invalid email format")
        })?;

        if !self.allowed_domains.contains(&domain.to_string()) {
            return Err(FraiseQLError::validation(format!(
                "Email domain {} not allowed",
                domain
            )));
        }

        Ok(())
    }
}

// Register
registry.register_validator(
    "email_domain",
    EmailDomainValidator {
        allowed_domains: vec!["company.com".to_string(), "trusted.com".to_string()],
    },
);
```

### Middleware & Hooks

```rust
/// Middleware for execution pipeline.
#[async_trait]
pub trait Middleware: Send + Sync {
    /// Called before query execution.
    async fn before_execute(
        &self,
        query: &str,
        variables: &serde_json::Value,
        context: &mut ExecutionContext,
    ) -> Result<()>;

    /// Called after query execution.
    async fn after_execute(
        &self,
        query: &str,
        result: &serde_json::Value,
        context: &ExecutionContext,
    ) -> Result<()>;

    /// Called on error.
    async fn on_error(
        &self,
        query: &str,
        error: &FraiseQLError,
        context: &ExecutionContext,
    );
}
```

**Example: Metrics Middleware**

```rust
pub struct MetricsMiddleware {
    metrics: Arc<Metrics>,
}

#[async_trait]
impl Middleware for MetricsMiddleware {
    async fn before_execute(
        &self,
        _query: &str,
        _variables: &serde_json::Value,
        context: &mut ExecutionContext,
    ) -> Result<()> {
        // Record query start time
        context.extensions.insert("start_time", Instant::now());
        self.metrics.increment_counter("queries_total");
        Ok(())
    }

    async fn after_execute(
        &self,
        _query: &str,
        _result: &serde_json::Value,
        context: &ExecutionContext,
    ) -> Result<()> {
        // Record query duration
        if let Some(start) = context.extensions.get("start_time") {
            let duration = start.elapsed();
            self.metrics.record_duration("query_duration_seconds", duration);
        }
        Ok(())
    }

    async fn on_error(
        &self,
        _query: &str,
        error: &FraiseQLError,
        _context: &ExecutionContext,
    ) {
        self.metrics.increment_counter_with_labels(
            "query_errors_total",
            &[("error_code", error.error_code())],
        );
    }
}
```

---

## Federation Architecture

### Entity Resolution Integration

**The JSONB projection architecture already supports federation!**

Federation requires resolving entities by ID:

```graphql
# Federation query (from Apollo Router)
query {
  _entities(representations: [{ __typename: "User", id: "user-123" }]) {
    ... on User {
      id
      email
      name
    }
  }
}
```

**How this maps to our architecture:**

1. **WHERE clause generation** handles `id = "user-123"`
2. **Database query** executes `SELECT data FROM v_user WHERE id = $1`
3. **JSONB projection** extracts requested fields

### Federation Trait Extension

**New module:** `crates/fraiseql-core/src/federation/mod.rs`

```rust
/// Federation entity resolver.
#[async_trait]
pub trait FederationResolver: Send + Sync {
    /// Resolve entities by representation.
    ///
    /// # Arguments
    ///
    /// * `representations` - List of entity references (typename + key fields)
    ///
    /// # Returns
    ///
    /// List of resolved entities (in same order as input).
    async fn resolve_entities(
        &self,
        representations: &[EntityRepresentation],
    ) -> Result<Vec<serde_json::Value>>;

    /// Resolve entities via database view (optimized path).
    ///
    /// Used when federated subgraph has direct database access.
    async fn resolve_via_view(
        &self,
        view: &str,
        ids: &[String],
    ) -> Result<Vec<serde_json::Value>>;

    /// Resolve entities via HTTP (fallback).
    ///
    /// Used when federated subgraph is remote or non-database.
    async fn resolve_via_http(
        &self,
        url: &str,
        representations: &[EntityRepresentation],
    ) -> Result<Vec<serde_json::Value>>;
}

/// Entity reference from federation query.
#[derive(Debug, Clone)]
pub struct EntityRepresentation {
    pub typename: String,
    pub id: String,
    pub additional_keys: HashMap<String, serde_json::Value>,
}
```

### Default Federation Resolver Implementation

```rust
pub struct DefaultFederationResolver {
    db_adapter: Arc<dyn DatabaseAdapter>,
    projector: Arc<dyn JsonbProjector>,
    http_client: reqwest::Client,
}

#[async_trait]
impl FederationResolver for DefaultFederationResolver {
    async fn resolve_entities(
        &self,
        representations: &[EntityRepresentation],
    ) -> Result<Vec<serde_json::Value>> {
        // Group by typename
        let mut by_type: HashMap<String, Vec<&EntityRepresentation>> = HashMap::new();
        for repr in representations {
            by_type.entry(repr.typename.clone()).or_default().push(repr);
        }

        let mut results = Vec::new();

        for (typename, reprs) in by_type {
            // Build WHERE clause: id IN (...)
            let ids: Vec<String> = reprs.iter().map(|r| r.id.clone()).collect();

            let where_clause = WhereClause::Field {
                path: vec!["id".to_string()],
                operator: WhereOperator::In,
                value: serde_json::to_value(&ids)?,
            };

            // Execute query
            let view = format!("v_{}", typename.to_lowercase());
            let jsonb_results = self
                .db_adapter
                .execute_where_query(&view, Some(&where_clause), None, None)
                .await?;

            // Project (federation needs all requested fields from representation)
            // In practice, federation query specifies selection set
            results.extend(jsonb_results.into_iter().map(|j| j.data));
        }

        Ok(results)
    }

    async fn resolve_via_view(
        &self,
        view: &str,
        ids: &[String],
    ) -> Result<Vec<serde_json::Value>> {
        // Direct database view query (optimized for same-database federation)
        let where_clause = WhereClause::Field {
            path: vec!["id".to_string()],
            operator: WhereOperator::In,
            value: serde_json::to_value(ids)?,
        };

        let jsonb_results = self
            .db_adapter
            .execute_where_query(view, Some(&where_clause), None, None)
            .await?;

        Ok(jsonb_results.into_iter().map(|j| j.data).collect())
    }

    async fn resolve_via_http(
        &self,
        url: &str,
        representations: &[EntityRepresentation],
    ) -> Result<Vec<serde_json::Value>> {
        // HTTP fallback for remote subgraphs
        let query = build_federation_query(representations);

        let response = self
            .http_client
            .post(url)
            .json(&serde_json::json!({
                "query": query,
                "variables": { "representations": representations }
            }))
            .send()
            .await
            .map_err(|e| FraiseQLError::internal(format!("Federation HTTP error: {e}")))?;

        let data: serde_json::Value = response.json().await.map_err(|e| {
            FraiseQLError::internal(format!("Federation response parse error: {e}"))
        })?;

        // Extract entities from _entities field
        Ok(data["data"]["_entities"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }
}

fn build_federation_query(representations: &[EntityRepresentation]) -> String {
    // Build federation _entities query
    format!(
        r#"
        query($representations: [_Any!]!) {{
            _entities(representations: $representations) {{
                __typename
                ... on User {{ id email name }}
                ... on Post {{ id title }}
            }}
        }}
        "#
    )
}
```

**Integration with Executor:**

```rust
impl Executor {
    pub async fn execute_federation_query(
        &self,
        representations: &[EntityRepresentation],
        context: &ExecutionContext,
    ) -> Result<serde_json::Value> {
        // Check if federation resolver is available
        let federation_resolver = self.federation_resolver.as_ref().ok_or_else(|| {
            FraiseQLError::config("Federation not enabled")
        })?;

        // Resolve entities
        let entities = federation_resolver.resolve_entities(representations).await?;

        // Apply auth masking
        let auth_mask = AuthMask::from_schema(&self.schema, &context.user);

        // Return federation response
        Ok(serde_json::json!({
            "_entities": entities
        }))
    }
}
```

---

## Enterprise RBAC Integration

### Hierarchical Role Resolution

**New module:** `crates/fraiseql-core/src/security/rbac.rs`

```rust
/// RBAC permission resolver with caching.
pub struct RBACResolver {
    db_adapter: Arc<dyn DatabaseAdapter>,
    cache: Arc<dyn CacheBackend>,
    domain_version: Arc<AtomicU64>,
}

impl RBACResolver {
    /// Resolve all permissions for user (with role hierarchy).
    pub async fn resolve_permissions(
        &self,
        user_id: &str,
        tenant_id: Option<&str>,
    ) -> Result<HashSet<String>> {
        // Check domain version (for cache invalidation)
        let current_version = self.domain_version.load(Ordering::Relaxed);

        // Check cache
        let cache_key = CacheKey(format!("rbac:{}:{}:{}", user_id, tenant_id.unwrap_or(""), current_version));

        if let Some(cached) = self.cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached.data)?);
        }

        // Query database (recursive CTE for role hierarchy)
        let sql = r#"
            WITH RECURSIVE role_hierarchy AS (
                -- Base: user's direct roles
                SELECT role_id, 0 AS depth
                FROM tb_user_role
                WHERE user_id = $1 AND (tenant_id = $2 OR tenant_id IS NULL)

                UNION ALL

                -- Recursive: parent roles
                SELECT r.parent_role_id, rh.depth + 1
                FROM role_hierarchy rh
                JOIN tb_role r ON r.id = rh.role_id
                WHERE r.parent_role_id IS NOT NULL AND rh.depth < 10
            )
            SELECT DISTINCT p.permission
            FROM role_hierarchy rh
            JOIN tb_role_permission rp ON rp.role_id = rh.role_id
            JOIN tb_permission p ON p.id = rp.permission_id
        "#;

        let params = vec![
            QueryParameter::String(user_id.to_string()),
            QueryParameter::String(tenant_id.unwrap_or("").to_string()),
        ];

        // Execute (using db adapter's raw query method)
        let rows = self.execute_raw_query(sql, &params).await?;

        // Extract permissions
        let permissions: HashSet<String> = rows
            .iter()
            .filter_map(|row| row.get("permission").and_then(|v| v.as_str()))
            .map(String::from)
            .collect();

        // Cache result
        self.cache
            .set(
                &cache_key,
                &CachedValue {
                    data: serde_json::to_value(&permissions)?,
                    cached_at: Instant::now(),
                },
                Some(Duration::from_secs(300)), // 5 minutes
            )
            .await?;

        Ok(permissions)
    }

    /// Invalidate RBAC cache (increment domain version).
    pub fn invalidate_cache(&self) {
        self.domain_version.fetch_add(1, Ordering::Relaxed);
    }
}
```

### Field-Level Authorization with RBAC

**Enhancement to `AuthMask`:**

```rust
impl AuthMask {
    /// Build auth mask from RBAC permissions.
    pub async fn from_rbac(
        schema: &CompiledSchema,
        user_permissions: &HashSet<String>,
        user_roles: &[String],
    ) -> Self {
        let mut rules = HashMap::new();

        // Iterate through schema authorization rules
        for (type_name, type_auth) in &schema.authorization {
            let mut type_rules = HashMap::new();

            for (field_name, field_auth) in type_auth {
                // Check if user has required permissions
                if let Some(required_perms) = &field_auth.required_permissions {
                    let has_permission = required_perms
                        .iter()
                        .any(|perm| user_permissions.contains(perm));

                    if !has_permission {
                        type_rules.insert(field_name.clone(), field_auth.clone());
                        continue;
                    }
                }

                // Check if user has required roles
                if let Some(required_roles) = &field_auth.required_roles {
                    let has_role = required_roles
                        .iter()
                        .any(|role| user_roles.contains(role));

                    if !has_role {
                        type_rules.insert(field_name.clone(), field_auth.clone());
                    }
                }
            }

            if !type_rules.is_empty() {
                rules.insert(type_name.clone(), type_rules);
            }
        }

        Self { rules }
    }
}
```

---

## Subscriptions & Event Streaming

### Database Event Listener

**New module:** `crates/fraiseql-core/src/subscriptions/mod.rs`

```rust
/// Subscription event stream.
#[async_trait]
pub trait EventStream: Send + Sync {
    /// Subscribe to events matching filter.
    async fn subscribe(
        &self,
        filter: &EventFilter,
    ) -> Result<Pin<Box<dyn Stream<Item = Event> + Send>>>;

    /// Emit event (for mutations).
    async fn emit(&self, event: Event) -> Result<()>;
}

/// Event filter (compiled from GraphQL subscription).
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub entity_type: String,
    pub operation: EventOperation,
    pub where_clause: Option<WhereClause>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventOperation {
    Created,
    Updated,
    Deleted,
}

/// Database change event.
#[derive(Debug, Clone)]
pub struct Event {
    pub entity_type: String,
    pub operation: EventOperation,
    pub entity_id: String,
    pub data: serde_json::Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub user_id: Option<String>,
    pub tenant_id: Option<String>,
}
```

### PostgreSQL LISTEN/NOTIFY Implementation

```rust
pub struct PostgresEventStream {
    db_adapter: Arc<PostgresAdapter>,
    event_channel: (Sender<Event>, Receiver<Event>),
}

impl PostgresEventStream {
    pub async fn new(db_adapter: Arc<PostgresAdapter>) -> Result<Self> {
        let (tx, rx) = mpsc::channel(1000);

        // Spawn listener task
        let adapter = db_adapter.clone();
        tokio::spawn(async move {
            Self::listen_for_notifications(adapter, tx).await;
        });

        Ok(Self {
            db_adapter,
            event_channel: (tx, rx),
        })
    }

    async fn listen_for_notifications(
        adapter: Arc<PostgresAdapter>,
        tx: Sender<Event>,
    ) {
        // Get dedicated connection for LISTEN
        let mut client = adapter.get_connection().await.unwrap();

        // LISTEN to entity change notifications
        client
            .execute("LISTEN entity_changes", &[])
            .await
            .unwrap();

        // Process notifications
        loop {
            let notification = client.notifications().try_recv();

            if let Ok(Some(notif)) = notification {
                // Parse notification payload (JSON)
                let event: Event = serde_json::from_str(notif.payload()).unwrap();

                // Send to subscribers
                let _ = tx.send(event).await;
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[async_trait]
impl EventStream for PostgresEventStream {
    async fn subscribe(
        &self,
        filter: &EventFilter,
    ) -> Result<Pin<Box<dyn Stream<Item = Event> + Send>>> {
        // Create filtered stream
        let rx = self.event_channel.1.clone();
        let filter = filter.clone();

        let stream = ReceiverStream::new(rx).filter(move |event| {
            let matches = event.entity_type == filter.entity_type
                && event.operation == filter.operation;

            // TODO: Apply WHERE clause filter

            futures::future::ready(matches)
        });

        Ok(Box::pin(stream))
    }

    async fn emit(&self, event: Event) -> Result<()> {
        // Emit via PostgreSQL NOTIFY
        let payload = serde_json::to_string(&event)?;

        self.db_adapter
            .execute_raw_query(&format!("NOTIFY entity_changes, '{}'", payload), &[])
            .await?;

        Ok(())
    }
}
```

### Subscription Execution Integration

```rust
impl Executor {
    pub async fn execute_subscription(
        &self,
        subscription_query: &str,
        context: &ExecutionContext,
    ) -> Result<Pin<Box<dyn Stream<Item = serde_json::Value> + Send>>> {
        // Parse subscription query
        let filter = self.parse_subscription_filter(subscription_query)?;

        // Get event stream
        let event_stream = self.event_stream.as_ref().ok_or_else(|| {
            FraiseQLError::config("Subscriptions not enabled")
        })?;

        // Subscribe
        let stream = event_stream.subscribe(&filter).await?;

        // Project events
        let projector = self.projector.clone();
        let selection_set = self.parse_selection_set(subscription_query)?;
        let auth_mask = AuthMask::from_schema(&self.schema, &context.user);

        let projected_stream = stream.map(move |event| {
            projector
                .project(&event.data, &selection_set, &auth_mask)
                .unwrap_or_default()
        });

        Ok(Box::pin(projected_stream))
    }
}
```

---

## Observability & Instrumentation

### Metrics Integration

**New module:** `crates/fraiseql-core/src/observability/metrics.rs`

```rust
/// Metrics collector trait (pluggable backends).
pub trait MetricsCollector: Send + Sync {
    fn increment_counter(&self, name: &str);
    fn increment_counter_with_labels(&self, name: &str, labels: &[(&str, &str)]);
    fn record_duration(&self, name: &str, duration: Duration);
    fn record_gauge(&self, name: &str, value: f64);
}

/// Prometheus metrics collector.
pub struct PrometheusMetrics {
    registry: prometheus::Registry,
    counters: HashMap<String, prometheus::Counter>,
    histograms: HashMap<String, prometheus::Histogram>,
    gauges: HashMap<String, prometheus::Gauge>,
}

impl MetricsCollector for PrometheusMetrics {
    fn increment_counter(&self, name: &str) {
        if let Some(counter) = self.counters.get(name) {
            counter.inc();
        }
    }

    fn record_duration(&self, name: &str, duration: Duration) {
        if let Some(histogram) = self.histograms.get(name) {
            histogram.observe(duration.as_secs_f64());
        }
    }

    // ... other methods
}
```

### Tracing Integration

```rust
use tracing::{info, warn, error, instrument};

impl Executor {
    #[instrument(skip(self), fields(operation_name, user_id))]
    pub async fn execute(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
        context: &ExecutionContext,
    ) -> Result<serde_json::Value> {
        // Tracing automatically captures function arguments and return values

        info!("Executing query");

        // Execute with tracing spans
        let result = self.execute_internal(query, variables, context).await;

        match &result {
            Ok(_) => info!("Query executed successfully"),
            Err(e) => error!("Query execution failed: {}", e),
        }

        result
    }
}
```

### Execution Context with Tracing

```rust
pub struct ExecutionContext {
    pub request_id: String,
    pub user: UserContext,
    pub tenant_id: Option<String>,
    pub start_time: Instant,
    pub tracing_span: tracing::Span,
    pub extensions: HashMap<String, serde_json::Value>,
}

impl ExecutionContext {
    pub fn new(user: UserContext) -> Self {
        let request_id = uuid::Uuid::new_v4().to_string();
        let span = tracing::info_span!("execute_query", request_id = %request_id);

        Self {
            request_id,
            user,
            tenant_id: None,
            start_time: Instant::now(),
            tracing_span: span,
            extensions: HashMap::new(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}
```

---

## Custom Authorization Rules

### Authorization Rule Registry

```rust
impl ExtensionRegistry {
    /// Get authorization rule by name.
    pub fn get_auth_rule(&self, name: &str) -> Option<&dyn AuthorizationRule> {
        self.authorization_rules.get(name).map(|b| b.as_ref())
    }
}

/// Enhanced AuthMask with custom rules.
impl AuthMask {
    pub async fn check_custom_rule(
        &self,
        rule_name: &str,
        resource: &serde_json::Value,
        user: &UserContext,
        db: &dyn DatabaseAdapter,
        registry: &ExtensionRegistry,
    ) -> Result<bool> {
        let rule = registry
            .get_auth_rule(rule_name)
            .ok_or_else(|| FraiseQLError::config(format!("Unknown auth rule: {rule_name}")))?;

        rule.is_authorized(resource, user, db).await
    }
}
```

---

## Validation Hooks

### Pre-Mutation Validation

```rust
impl Executor {
    pub async fn execute_mutation(
        &self,
        mutation: &str,
        variables: &serde_json::Value,
        context: &ExecutionContext,
    ) -> Result<serde_json::Value> {
        // Extract mutation input
        let input = self.extract_mutation_input(mutation, variables)?;

        // Run validators
        for (field_name, value) in input.as_object().unwrap() {
            if let Some(validator_name) = self.schema.get_validator_for_field(field_name) {
                let validator = self
                    .registry
                    .get_validator(validator_name)
                    .ok_or_else(|| {
                        FraiseQLError::config(format!("Unknown validator: {validator_name}"))
                    })?;

                validator.validate(value, Some(&*self.db_adapter)).await?;
            }
        }

        // Execute mutation
        self.execute_mutation_internal(mutation, variables, context)
            .await
    }
}
```

---

## Transport Adapters

### GraphQL-WS Adapter (WebSocket)

```rust
pub struct GraphQLWSAdapter {
    event_stream: Arc<dyn EventStream>,
}

impl GraphQLWSAdapter {
    pub async fn handle_subscription(
        &self,
        socket: WebSocket,
        subscription_query: String,
    ) {
        // Subscribe to events
        let filter = parse_subscription_filter(&subscription_query).unwrap();
        let mut stream = self.event_stream.subscribe(&filter).await.unwrap();

        // Send events to WebSocket
        while let Some(event) = stream.next().await {
            let message = serde_json::to_string(&event).unwrap();
            socket.send(Message::Text(message)).await.unwrap();
        }
    }
}
```

### Webhook Adapter

```rust
pub struct WebhookAdapter {
    event_stream: Arc<dyn EventStream>,
    http_client: reqwest::Client,
}

impl WebhookAdapter {
    pub async fn deliver_to_webhook(
        &self,
        webhook_url: &str,
        filter: &EventFilter,
    ) {
        let mut stream = self.event_stream.subscribe(filter).await.unwrap();

        while let Some(event) = stream.next().await {
            // POST event to webhook URL
            let _ = self
                .http_client
                .post(webhook_url)
                .json(&event)
                .send()
                .await;
        }
    }
}
```

---

## Summary: How Advanced Features Integrate

| Feature | Integration Point | Implementation |
|---------|------------------|----------------|
| **Custom Auth Rules** | `ExtensionRegistry` + `AuthorizationRule` trait | Pluggable via trait, called during projection |
| **Federation** | `FederationResolver` trait | Uses existing WHERE + projection architecture |
| **RBAC** | `RBACResolver` + `AuthMask` | Permission caching, hierarchical role resolution |
| **Subscriptions** | `EventStream` trait + database listeners | LISTEN/NOTIFY for PostgreSQL, polling for others |
| **Observability** | `Middleware` trait + tracing | Metrics/traces collected via middleware hooks |
| **Validators** | `Validator` trait + registry | Called pre-mutation, async with DB access |
| **Transport Adapters** | `EventStream` subscription | Multiple transports (WebSocket, webhook, Kafka) |

**All features are trait-based and pluggable. The core architecture from RUST_CORE_ARCHITECTURE.md supports all of them without modification.**

---

## Migration to Advanced Features

### Phase Timeline

**Phase 2-3 (Basic):**

- ✅ Core database + WHERE + projection + auth
- ✅ Basic field-level authorization
- ✅ Connection pooling + caching

**Phase 6 (HTTP Server):**

- 🔧 Add `ExtensionRegistry`
- 🔧 Add `Middleware` support
- 🔧 Add basic metrics middleware

**Phase 7 (Federation Support):**

- 🔧 Implement `FederationResolver`
- 🔧 Add `_entities` query resolver
- 🔧 Support view-based + HTTP federation

**Phase 8 (Enterprise Features):**

- 🔧 Implement `RBACResolver`
- 🔧 Add hierarchical role support
- 🔧 Permission caching with domain versioning

**Phase 9 (Subscriptions):**

- 🔧 Implement `EventStream` trait
- 🔧 Add PostgreSQL LISTEN/NOTIFY
- 🔧 Build transport adapters (WebSocket, webhooks)

**Phase 10 (Observability):**

- 🔧 Add full Prometheus metrics
- 🔧 Add OpenTelemetry tracing
- 🔧 Build dashboards

**All phases build on the foundation laid in Phases 2-5.**

---

**End of Advanced Features Architecture**
