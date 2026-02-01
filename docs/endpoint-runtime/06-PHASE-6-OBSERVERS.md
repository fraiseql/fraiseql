# Phase 6: Observer Runtime

## Objective

Implement the post-mutation observer system that processes side effects asynchronously after database transactions commit. Observers trigger actions (email, Slack, webhooks, etc.) based on entity lifecycle events.

## Dependencies

- Phase 1: Configuration system (TOML parsing)
- Phase 2: Core runtime (metrics, tracing)
- Phase 5: Auth runtime (user context in events)

---

## 6.0 Backpressure, Testing Seams & Transaction Semantics

### Backpressure Handling

Observers can receive events faster than they can process them. Key protections:

```
┌─────────────────────────────────────────────────────────────┐
│ Event Processing Pipeline with Backpressure                 │
├─────────────────────────────────────────────────────────────┤
│ PostgreSQL NOTIFY → Bounded Channel → Worker Pool           │
│       │                    │              │                 │
│       │              Max Capacity    Max Concurrency        │
│       │                (1000)            (50)               │
│       │                    │              │                 │
│       ▼                    ▼              ▼                 │
│  If channel full:     If backlogged:  If all busy:         │
│  - Events dropped     - Log warning   - Queue or           │
│  - Metrics updated    - Scale alert     drop excess        │
│  - Alert sent                                              │
└─────────────────────────────────────────────────────────────┘
```

### Task: Backpressure configuration

```rust
// crates/fraiseql-observers/src/config.rs (additions)

#[derive(Debug, Clone, Deserialize)]
pub struct ObserverRuntimeConfig {
    /// Channel buffer size for incoming events
    #[serde(default = "default_channel_capacity")]
    pub channel_capacity: usize,

    /// Maximum concurrent action executions
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    /// What to do when channel is full
    #[serde(default)]
    pub overflow_policy: OverflowPolicy,

    /// Backlog threshold for alerts
    #[serde(default = "default_backlog_threshold")]
    pub backlog_alert_threshold: usize,

    /// Graceful shutdown timeout
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout: String,
}

fn default_channel_capacity() -> usize { 1000 }
fn default_max_concurrency() -> usize { 50 }
fn default_backlog_threshold() -> usize { 500 }
fn default_shutdown_timeout() -> String { "30s".to_string() }

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverflowPolicy {
    /// Drop new events when channel is full
    #[default]
    Drop,
    /// Block sender (can cause issues with PG listener)
    Block,
    /// Drop oldest events to make room
    DropOldest,
}
```

### Task: Define testing seams

```rust
// crates/fraiseql-observers/src/traits.rs

use async_trait::async_trait;
use crate::event::EntityEvent;
use crate::error::ObserverError;

/// Event source abstraction for testing
#[async_trait]
pub trait EventSource: Send + Sync {
    async fn next_event(&mut self) -> Option<EntityEvent>;
}

/// Action executor abstraction for testing
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    async fn execute(
        &self,
        event: &EntityEvent,
        action: &ActionConfig,
    ) -> Result<ActionResult, ObserverError>;
}

/// Dead letter queue abstraction for testing
#[async_trait]
pub trait DeadLetterQueue: Send + Sync {
    async fn push(&self, event: EntityEvent, action: ActionConfig, error: String) -> Result<uuid::Uuid, ObserverError>;
    async fn get_pending(&self, limit: i64) -> Result<Vec<DlqItem>, ObserverError>;
    async fn mark_success(&self, id: uuid::Uuid) -> Result<(), ObserverError>;
    async fn mark_retry_failed(&self, id: uuid::Uuid, error: &str) -> Result<(), ObserverError>;
}

/// Condition evaluator abstraction for testing
pub trait ConditionEvaluator: Send + Sync {
    fn evaluate(&self, condition: &str, event: &EntityEvent) -> Result<bool, ObserverError>;
}

/// Template renderer abstraction for testing
pub trait TemplateRenderer: Send + Sync {
    fn render(&self, template: &str, data: &serde_json::Value) -> Result<String, ObserverError>;
}
```

### Task: Mock implementations for testing

```rust
// crates/fraiseql-observers/src/testing.rs

#[cfg(any(test, feature = "testing"))]
pub mod mocks {
    use super::*;
    use std::sync::Mutex;
    use std::collections::VecDeque;

    /// Mock event source that yields predefined events
    pub struct MockEventSource {
        events: Mutex<VecDeque<EntityEvent>>,
    }

    impl MockEventSource {
        pub fn new(events: Vec<EntityEvent>) -> Self {
            Self {
                events: Mutex::new(events.into()),
            }
        }

        pub fn empty() -> Self {
            Self {
                events: Mutex::new(VecDeque::new()),
            }
        }

        pub fn push(&self, event: EntityEvent) {
            self.events.lock().unwrap().push_back(event);
        }
    }

    #[async_trait]
    impl EventSource for MockEventSource {
        async fn next_event(&mut self) -> Option<EntityEvent> {
            self.events.lock().unwrap().pop_front()
        }
    }

    /// Mock action executor that records executions
    pub struct MockActionExecutor {
        pub executions: Mutex<Vec<ExecutionRecord>>,
        pub should_fail: Mutex<bool>,
        pub failure_message: Mutex<Option<String>>,
    }

    #[derive(Debug, Clone)]
    pub struct ExecutionRecord {
        pub event_type: String,
        pub entity_id: uuid::Uuid,
        pub action_type: String,
        pub timestamp: chrono::DateTime<chrono::Utc>,
    }

    impl MockActionExecutor {
        pub fn new() -> Self {
            Self {
                executions: Mutex::new(Vec::new()),
                should_fail: Mutex::new(false),
                failure_message: Mutex::new(None),
            }
        }

        pub fn failing(message: &str) -> Self {
            Self {
                executions: Mutex::new(Vec::new()),
                should_fail: Mutex::new(true),
                failure_message: Mutex::new(Some(message.to_string())),
            }
        }

        pub fn set_should_fail(&self, fail: bool, message: Option<&str>) {
            *self.should_fail.lock().unwrap() = fail;
            *self.failure_message.lock().unwrap() = message.map(|s| s.to_string());
        }

        pub fn get_executions(&self) -> Vec<ExecutionRecord> {
            self.executions.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ActionExecutor for MockActionExecutor {
        async fn execute(
            &self,
            event: &EntityEvent,
            action: &ActionConfig,
        ) -> Result<ActionResult, ObserverError> {
            let should_fail = *self.should_fail.lock().unwrap();

            self.executions.lock().unwrap().push(ExecutionRecord {
                event_type: event.event_type.clone(),
                entity_id: event.entity_id,
                action_type: action_type_name(action),
                timestamp: chrono::Utc::now(),
            });

            if should_fail {
                let message = self.failure_message.lock().unwrap()
                    .clone()
                    .unwrap_or_else(|| "Mock failure".to_string());
                return Err(ObserverError::ActionFailed(message));
            }

            Ok(ActionResult {
                action_type: action_type_name(action),
                success: true,
                message: Some("Mock success".into()),
                duration_ms: 10,
            })
        }
    }

    /// Mock DLQ for testing
    pub struct MockDeadLetterQueue {
        items: Mutex<Vec<MockDlqItem>>,
    }

    struct MockDlqItem {
        id: uuid::Uuid,
        event: EntityEvent,
        action: ActionConfig,
        error: String,
        attempts: i32,
    }

    impl MockDeadLetterQueue {
        pub fn new() -> Self {
            Self {
                items: Mutex::new(Vec::new()),
            }
        }

        pub fn item_count(&self) -> usize {
            self.items.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl DeadLetterQueue for MockDeadLetterQueue {
        async fn push(
            &self,
            event: EntityEvent,
            action: ActionConfig,
            error: String,
        ) -> Result<uuid::Uuid, ObserverError> {
            let id = uuid::Uuid::new_v4();
            self.items.lock().unwrap().push(MockDlqItem {
                id,
                event,
                action,
                error,
                attempts: 1,
            });
            Ok(id)
        }

        async fn get_pending(&self, limit: i64) -> Result<Vec<DlqItem>, ObserverError> {
            let items = self.items.lock().unwrap();
            Ok(items.iter()
                .take(limit as usize)
                .map(|item| DlqItem {
                    id: item.id,
                    event: item.event.clone(),
                    action: item.action.clone(),
                    error_message: item.error.clone(),
                    attempts: item.attempts,
                    first_attempt_at: chrono::Utc::now(),
                    last_attempt_at: chrono::Utc::now(),
                })
                .collect())
        }

        async fn mark_success(&self, id: uuid::Uuid) -> Result<(), ObserverError> {
            self.items.lock().unwrap().retain(|item| item.id != id);
            Ok(())
        }

        async fn mark_retry_failed(&self, id: uuid::Uuid, error: &str) -> Result<(), ObserverError> {
            if let Some(item) = self.items.lock().unwrap()
                .iter_mut()
                .find(|item| item.id == id)
            {
                item.attempts += 1;
                item.error = error.to_string();
            }
            Ok(())
        }
    }

    /// Mock condition evaluator for testing
    pub struct MockConditionEvaluator {
        pub results: Mutex<std::collections::HashMap<String, bool>>,
    }

    impl MockConditionEvaluator {
        pub fn always_true() -> Self {
            Self {
                results: Mutex::new(std::collections::HashMap::new()),
            }
        }

        pub fn with_result(mut self, condition: &str, result: bool) -> Self {
            self.results.lock().unwrap().insert(condition.to_string(), result);
            self
        }
    }

    impl ConditionEvaluator for MockConditionEvaluator {
        fn evaluate(&self, condition: &str, _event: &EntityEvent) -> Result<bool, ObserverError> {
            Ok(*self.results.lock().unwrap()
                .get(condition)
                .unwrap_or(&true))
        }
    }

    fn action_type_name(action: &ActionConfig) -> String {
        match action {
            ActionConfig::Email(_) => "email",
            ActionConfig::Slack(_) => "slack",
            ActionConfig::Sms(_) => "sms",
            ActionConfig::Webhook(_) => "webhook",
            ActionConfig::Push(_) => "push",
            ActionConfig::Search(_) => "search",
            ActionConfig::Cache(_) => "cache",
            ActionConfig::Custom(_) => "custom",
        }.to_string()
    }
}
```

### Transaction Semantics

Observer events are emitted AFTER the database transaction commits:

```
┌─────────────────────────────────────────────────────────────┐
│ Transaction Timeline                                        │
├─────────────────────────────────────────────────────────────┤
│ BEGIN                                                       │
│ ├── INSERT INTO orders (...)                                │
│ ├── UPDATE inventory SET ...                                │
│ ├── Prepare event payload (in memory)                       │
│ COMMIT  ◄─── Only now is NOTIFY sent                        │
│       │                                                     │
│       ▼                                                     │
│ pg_notify('fraiseql_events', payload)                       │
│       │                                                     │
│       ▼                                                     │
│ Observer receives event (async, separate connection)        │
└─────────────────────────────────────────────────────────────┘
```

**Critical**: If the transaction rolls back, no event is emitted. This ensures observers only see committed data.

```rust
// Transaction-safe event emission pattern in SQL

CREATE OR REPLACE FUNCTION app.order_create(p_input JSONB)
RETURNS cascade_response AS $$
DECLARE
    v_result app.order;
BEGIN
    -- Do all database work
    INSERT INTO app.order (...) RETURNING * INTO v_result;
    UPDATE app.inventory SET ...;

    -- pg_notify is deferred until COMMIT
    -- If we ROLLBACK, the notification is discarded
    PERFORM pg_notify(
        'fraiseql_events',
        jsonb_build_object(
            'id', gen_random_uuid(),
            'event_type', 'order_created',
            'entity', 'Order',
            'entity_id', v_result.id,
            'data', to_jsonb(v_result),
            'timestamp', NOW()
        )::TEXT
    );

    RETURN build_cascade_success(to_jsonb(v_result), NULL);
    -- COMMIT happens here, NOTIFY is actually sent
END;
$$ LANGUAGE plpgsql;
```

---

## Crate: `fraiseql-observers`

```
crates/fraiseql-observers/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs              # Observer configuration
│   ├── event.rs               # Event types and payload
│   ├── listener.rs            # PostgreSQL LISTEN/NOTIFY
│   ├── matcher.rs             # Event-to-observer matching
│   ├── condition.rs           # Condition evaluation
│   ├── executor.rs            # Action execution orchestration
│   ├── template.rs            # Template rendering (Jinja-style)
│   ├── actions/
│   │   ├── mod.rs
│   │   ├── email.rs           # Email action
│   │   ├── slack.rs           # Slack action
│   │   ├── webhook.rs         # Webhook action
│   │   ├── sms.rs             # SMS action
│   │   ├── push.rs            # Push notification action
│   │   ├── search.rs          # Search index update
│   │   └── cache.rs           # Cache invalidation
│   ├── retry.rs               # Retry logic with backoff
│   ├── dlq.rs                 # Dead letter queue handling
│   └── error.rs
└── tests/
    ├── listener_test.rs
    ├── condition_test.rs
    ├── executor_test.rs
    └── integration_test.rs
```

---

## Step 1: Event Types and Payload

### 1.1 Define Event Structure

```rust
// src/event.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// Event emitted after a database mutation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEvent {
    /// Unique event ID
    pub id: Uuid,

    /// Event type: entity_created, entity_updated, entity_deleted
    pub event_type: String,

    /// Entity type name (e.g., "Order", "User")
    pub entity: String,

    /// Entity primary key
    pub entity_id: Uuid,

    /// Full entity data after mutation
    pub data: Value,

    /// Changes for update events (old/new values)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<FieldChanges>,

    /// User who triggered the mutation (from auth context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// When the event occurred
    pub timestamp: DateTime<Utc>,
}

/// Field changes for update events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChanges {
    pub fields: std::collections::HashMap<String, FieldChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldChange {
    pub old: Value,
    pub new: Value,
}

impl EntityEvent {
    /// Parse event type into components
    pub fn parse_event_type(&self) -> (String, EventKind) {
        if self.event_type.ends_with("_created") {
            let entity = self.event_type.trim_end_matches("_created");
            (entity.to_string(), EventKind::Created)
        } else if self.event_type.ends_with("_updated") {
            let entity = self.event_type.trim_end_matches("_updated");
            (entity.to_string(), EventKind::Updated)
        } else if self.event_type.ends_with("_deleted") {
            let entity = self.event_type.trim_end_matches("_deleted");
            (entity.to_string(), EventKind::Deleted)
        } else {
            (self.event_type.clone(), EventKind::Custom)
        }
    }

    /// Check if a field changed to a specific value
    pub fn field_changed_to(&self, field: &str, value: &Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|c| c.fields.get(field))
            .map(|change| &change.new == value)
            .unwrap_or(false)
    }

    /// Check if a field changed from a specific value
    pub fn field_changed_from(&self, field: &str, value: &Value) -> bool {
        self.changes
            .as_ref()
            .and_then(|c| c.fields.get(field))
            .map(|change| &change.old == value)
            .unwrap_or(false)
    }

    /// Check if a field was modified
    pub fn field_changed(&self, field: &str) -> bool {
        self.changes
            .as_ref()
            .map(|c| c.fields.contains_key(field))
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Created,
    Updated,
    Deleted,
    Custom,
}
```

---

## Step 2: PostgreSQL Event Listener

### 2.1 LISTEN/NOTIFY Listener

```rust
// src/listener.rs
use sqlx::postgres::{PgListener, PgPool};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::event::EntityEvent;
use crate::error::ObserverError;

const CHANNEL_NAME: &str = "fraiseql_events";

/// Listens for PostgreSQL NOTIFY events
pub struct EventListener {
    pool: PgPool,
    sender: mpsc::Sender<EntityEvent>,
}

impl EventListener {
    pub fn new(pool: PgPool, sender: mpsc::Sender<EntityEvent>) -> Self {
        Self { pool, sender }
    }

    /// Start listening for events (blocks forever)
    pub async fn run(&self) -> Result<(), ObserverError> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(CHANNEL_NAME).await?;

        info!(channel = CHANNEL_NAME, "Observer listener started");

        loop {
            match listener.recv().await {
                Ok(notification) => {
                    debug!(payload = notification.payload(), "Received notification");

                    match serde_json::from_str::<EntityEvent>(notification.payload()) {
                        Ok(event) => {
                            if let Err(e) = self.sender.send(event).await {
                                error!(error = %e, "Failed to send event to processor");
                            }
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                payload = notification.payload(),
                                "Failed to parse event payload"
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Listener error, reconnecting...");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

                    // Attempt reconnection
                    if let Err(reconnect_err) = listener.listen(CHANNEL_NAME).await {
                        error!(error = %reconnect_err, "Failed to reconnect");
                    }
                }
            }
        }
    }
}

/// Create listener and processor channels
pub fn create_event_pipeline(
    pool: PgPool,
    buffer_size: usize,
) -> (EventListener, mpsc::Receiver<EntityEvent>) {
    let (sender, receiver) = mpsc::channel(buffer_size);
    let listener = EventListener::new(pool, sender);
    (listener, receiver)
}
```

### 2.2 SQL Function Template for Event Emission

```sql
-- Template for generating event emission in mutation functions
-- This is generated by fraiseql-compiler, shown here for reference

CREATE OR REPLACE FUNCTION app.order_create(p_input JSONB)
RETURNS cascade_response AS $$
DECLARE
    v_result app.order;
    v_user_id TEXT;
BEGIN
    -- Get current user from context
    v_user_id := current_setting('fraiseql.user_id', true);

    -- Perform the insert
    INSERT INTO app.order (customer_id, total, status)
    VALUES (
        (p_input->>'customer_id')::UUID,
        (p_input->>'total')::NUMERIC,
        COALESCE(p_input->>'status', 'pending')
    )
    RETURNING * INTO v_result;

    -- Emit event via NOTIFY (processed after COMMIT by trigger)
    PERFORM pg_notify(
        'fraiseql_events',
        jsonb_build_object(
            'id', gen_random_uuid(),
            'event_type', 'order_created',
            'entity', 'Order',
            'entity_id', v_result.id,
            'data', to_jsonb(v_result),
            'user_id', v_user_id,
            'timestamp', NOW()
        )::TEXT
    );

    RETURN build_cascade_success(to_jsonb(v_result), NULL);
END;
$$ LANGUAGE plpgsql;
```

---

## Step 3: Observer Configuration

### 3.1 Configuration Types

```rust
// src/config.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Observer configuration from fraiseql.toml
#[derive(Debug, Clone, Deserialize)]
pub struct ObserversConfig {
    /// Map of event_type -> observer definition
    #[serde(flatten)]
    pub observers: HashMap<String, ObserverDefinition>,

    /// Global failure alert settings
    #[serde(default)]
    pub failure_alerts: Option<FailureAlertConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ObserverDefinition {
    /// Condition to evaluate before executing actions
    #[serde(default)]
    pub condition: Option<String>,

    /// Actions to execute when event matches
    pub actions: Vec<ActionConfig>,

    /// Retry configuration
    #[serde(default)]
    pub retry: RetryConfig,

    /// What to do on failure: log, alert, dlq
    #[serde(default = "default_on_failure")]
    pub on_failure: FailurePolicy,
}

fn default_on_failure() -> FailurePolicy {
    FailurePolicy::Log
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionConfig {
    Email(EmailActionConfig),
    Slack(SlackActionConfig),
    Sms(SmsActionConfig),
    Webhook(WebhookActionConfig),
    Push(PushActionConfig),
    Search(SearchActionConfig),
    Cache(CacheActionConfig),
    Custom(CustomActionConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailActionConfig {
    pub to: String,                      // Template: "{{ customer.email }}"
    pub template: String,                // Template name
    #[serde(default)]
    pub data: HashMap<String, String>,   // Additional template data
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackActionConfig {
    pub channel: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub blocks: Option<Vec<SlackBlock>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SlackBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmsActionConfig {
    pub to: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebhookActionConfig {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    #[serde(default = "default_timeout")]
    pub timeout: String,  // "10s"
    #[serde(default)]
    pub retry: bool,
}

fn default_method() -> String {
    "POST".to_string()
}

fn default_timeout() -> String {
    "30s".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushActionConfig {
    pub user: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchActionConfig {
    pub action: SearchAction,
    pub index: String,
    pub document: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchAction {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheActionConfig {
    pub action: CacheAction,
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheAction {
    Invalidate,
    Refresh,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomActionConfig {
    pub handler: String,  // "observers.erp.sync_to_erp"
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RetryConfig {
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_initial_delay")]
    pub initial_delay: String,  // "1s"
    #[serde(default = "default_max_delay")]
    pub max_delay: String,      // "60s"
    #[serde(default)]
    pub backoff: BackoffStrategy,
}

fn default_max_attempts() -> u32 {
    3
}

fn default_initial_delay() -> String {
    "1s".to_string()
}

fn default_max_delay() -> String {
    "60s".to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffStrategy {
    #[default]
    Exponential,
    Linear,
    Fixed,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    Log,
    Alert,
    Dlq,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FailureAlertConfig {
    #[serde(default)]
    pub slack: Option<String>,   // Channel
    #[serde(default)]
    pub email: Option<String>,   // Email address
}
```

---

## Step 4: Condition Evaluation

### 4.1 Simple Expression Parser

```rust
// src/condition.rs
use serde_json::Value;
use crate::event::EntityEvent;
use crate::error::ObserverError;

/// Evaluates conditions against events
pub struct ConditionEvaluator;

impl ConditionEvaluator {
    /// Evaluate a condition expression against an event
    pub fn evaluate(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        let condition = condition.trim();

        // Handle function calls
        if condition.contains("status_changed_to(") {
            return Self::eval_status_changed_to(condition, event);
        }
        if condition.contains("status_changed_from(") {
            return Self::eval_status_changed_from(condition, event);
        }
        if condition.contains("field_changed(") {
            return Self::eval_field_changed(condition, event);
        }
        if condition == "is_new()" {
            return Ok(event.changes.is_none());
        }
        if condition == "is_deleted()" {
            let (_, kind) = event.parse_event_type();
            return Ok(kind == crate::event::EventKind::Deleted);
        }

        // Handle comparison expressions: "status == 'shipped'"
        if condition.contains("==") {
            return Self::eval_equality(condition, event);
        }
        if condition.contains("!=") {
            return Self::eval_inequality(condition, event);
        }
        if condition.contains(">") {
            return Self::eval_comparison(condition, event, |a, b| a > b);
        }
        if condition.contains("<") {
            return Self::eval_comparison(condition, event, |a, b| a < b);
        }

        // Handle && and ||
        if condition.contains("&&") {
            return Self::eval_and(condition, event);
        }
        if condition.contains("||") {
            return Self::eval_or(condition, event);
        }

        Err(ObserverError::InvalidCondition(format!(
            "Unknown condition syntax: {}",
            condition
        )))
    }

    fn eval_status_changed_to(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        // Parse: status_changed_to('shipped')
        let re = regex::Regex::new(r#"(\w+)_changed_to\(['"]([^'"]+)['"]\)"#)?;
        if let Some(caps) = re.captures(condition) {
            let field = &caps[1];
            let value = &caps[2];
            let json_value = Value::String(value.to_string());
            return Ok(event.field_changed_to(field, &json_value));
        }
        Err(ObserverError::InvalidCondition(condition.to_string()))
    }

    fn eval_status_changed_from(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        let re = regex::Regex::new(r#"(\w+)_changed_from\(['"]([^'"]+)['"]\)"#)?;
        if let Some(caps) = re.captures(condition) {
            let field = &caps[1];
            let value = &caps[2];
            let json_value = Value::String(value.to_string());
            return Ok(event.field_changed_from(field, &json_value));
        }
        Err(ObserverError::InvalidCondition(condition.to_string()))
    }

    fn eval_field_changed(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        let re = regex::Regex::new(r#"field_changed\(['"]([^'"]+)['"]\)"#)?;
        if let Some(caps) = re.captures(condition) {
            let field = &caps[1];
            return Ok(event.field_changed(field));
        }
        Err(ObserverError::InvalidCondition(condition.to_string()))
    }

    fn eval_equality(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        let parts: Vec<&str> = condition.split("==").collect();
        if parts.len() != 2 {
            return Err(ObserverError::InvalidCondition(condition.to_string()));
        }

        let field_path = parts[0].trim();
        let expected = Self::parse_value(parts[1].trim())?;
        let actual = Self::get_field_value(field_path, event)?;

        Ok(actual == expected)
    }

    fn eval_inequality(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        let parts: Vec<&str> = condition.split("!=").collect();
        if parts.len() != 2 {
            return Err(ObserverError::InvalidCondition(condition.to_string()));
        }

        let field_path = parts[0].trim();
        let expected = Self::parse_value(parts[1].trim())?;
        let actual = Self::get_field_value(field_path, event)?;

        Ok(actual != expected)
    }

    fn eval_comparison<F>(
        condition: &str,
        event: &EntityEvent,
        compare: F,
    ) -> Result<bool, ObserverError>
    where
        F: Fn(f64, f64) -> bool,
    {
        let (op, parts) = if condition.contains(">=") {
            (">=", condition.split(">=").collect::<Vec<_>>())
        } else if condition.contains("<=") {
            ("<=", condition.split("<=").collect::<Vec<_>>())
        } else if condition.contains(">") {
            (">", condition.split(">").collect::<Vec<_>>())
        } else {
            ("<", condition.split("<").collect::<Vec<_>>())
        };

        if parts.len() != 2 {
            return Err(ObserverError::InvalidCondition(condition.to_string()));
        }

        let field_path = parts[0].trim();
        let actual = Self::get_field_value(field_path, event)?;
        let expected = Self::parse_value(parts[1].trim())?;

        let actual_num = actual.as_f64().ok_or_else(|| {
            ObserverError::InvalidCondition(format!("{} is not a number", field_path))
        })?;
        let expected_num = expected.as_f64().ok_or_else(|| {
            ObserverError::InvalidCondition(format!("Expected number, got {:?}", expected))
        })?;

        let result = match op {
            ">=" => actual_num >= expected_num,
            "<=" => actual_num <= expected_num,
            _ => compare(actual_num, expected_num),
        };

        Ok(result)
    }

    fn eval_and(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        for part in condition.split("&&") {
            if !Self::evaluate(part.trim(), event)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn eval_or(condition: &str, event: &EntityEvent) -> Result<bool, ObserverError> {
        for part in condition.split("||") {
            if Self::evaluate(part.trim(), event)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn get_field_value(path: &str, event: &EntityEvent) -> Result<Value, ObserverError> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = &event.data;

        for part in parts {
            current = current.get(part).ok_or_else(|| {
                ObserverError::InvalidCondition(format!("Field not found: {}", path))
            })?;
        }

        Ok(current.clone())
    }

    fn parse_value(s: &str) -> Result<Value, ObserverError> {
        // String literal
        if (s.starts_with('\'') && s.ends_with('\''))
            || (s.starts_with('"') && s.ends_with('"'))
        {
            return Ok(Value::String(s[1..s.len() - 1].to_string()));
        }

        // Number
        if let Ok(n) = s.parse::<i64>() {
            return Ok(Value::Number(n.into()));
        }
        if let Ok(n) = s.parse::<f64>() {
            return Ok(Value::Number(
                serde_json::Number::from_f64(n).unwrap_or_else(|| 0.into()),
            ));
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

        Err(ObserverError::InvalidCondition(format!(
            "Cannot parse value: {}",
            s
        )))
    }
}
```

---

## Step 5: Template Rendering

### 5.1 Use Shared Template Engine

**Note:** The template engine is defined in `fraiseql-runtime` (see Phase 1 and Cross-Cutting Concerns in Overview). Observers use the shared implementation to avoid code duplication.

```rust
// src/template.rs
// Re-export from fraiseql-runtime

pub use fraiseql_runtime::template::{TemplateEngine, TemplateRenderer};

// Observer-specific convenience wrapper
use crate::error::ObserverError;
use serde_json::Value;

/// Render a template string with event data, mapping errors to ObserverError
pub fn render_template(
    renderer: &TemplateRenderer,
    template: &str,
    data: &Value,
) -> Result<String, ObserverError> {
    renderer.render(template, data).map_err(|e| {
        ObserverError::TemplateError(e.to_string())
    })
}
```

The shared `TemplateRenderer` in `fraiseql-runtime` provides:
- `{{ field }}` - Simple field access
- `{{ nested.path }}` - Nested object access
- `{{ items[0] }}` - Array indexing
- `{{ field | filter }}` - Filter application
- `{{ env.VAR_NAME }}` - Environment variable access

**Built-in filters:**
- `upper`, `lower` - Case conversion
- `json`, `json_pretty` - JSON serialization
- `currency` - Format as currency (cents to dollars)
- `date` - Format ISO dates
- `trim` - Remove whitespace
- `length` - Get array/string/object length
- `default('fallback')` - Default value for null

**Extensibility:**
```rust
// Register custom filters for observer-specific needs
let mut engine = TemplateEngine::new();
engine.register_filter("mask_email", |value| {
    // Custom masking logic
    Ok(mask_email_address(value.as_str().unwrap_or_default()))
});
```

---

## Step 6: Action Trait and Implementations

### 6.1 Action Trait

```rust
// src/actions/mod.rs
use async_trait::async_trait;
use serde_json::Value;

use crate::error::ObserverError;
use crate::event::EntityEvent;

pub mod email;
pub mod slack;
pub mod webhook;
pub mod sms;
pub mod push;
pub mod search;
pub mod cache;

/// Result of executing an action
#[derive(Debug, Clone)]
pub struct ActionResult {
    pub action_type: String,
    pub success: bool,
    pub message: Option<String>,
    pub duration_ms: u64,
}

/// Trait for all action types
#[async_trait]
pub trait Action: Send + Sync {
    /// Action type name
    fn action_type(&self) -> &'static str;

    /// Execute the action
    async fn execute(&self, event: &EntityEvent, data: &Value) -> Result<ActionResult, ObserverError>;
}

/// Registry of available actions
pub struct ActionRegistry {
    actions: std::collections::HashMap<String, Box<dyn Action>>,
}

impl ActionRegistry {
    pub fn new() -> Self {
        Self {
            actions: std::collections::HashMap::new(),
        }
    }

    pub fn register<A: Action + 'static>(&mut self, action: A) {
        self.actions.insert(action.action_type().to_string(), Box::new(action));
    }

    pub fn get(&self, action_type: &str) -> Option<&dyn Action> {
        self.actions.get(action_type).map(|a| a.as_ref())
    }
}
```

### 6.2 Webhook Action

```rust
// src/actions/webhook.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::time::{Duration, Instant};
use tracing::{debug, error};

use crate::config::WebhookActionConfig;
use crate::error::ObserverError;
use crate::event::EntityEvent;
use crate::template::TemplateRenderer;

use super::{Action, ActionResult};

pub struct WebhookAction {
    client: Client,
    config: WebhookActionConfig,
    renderer: TemplateRenderer,
}

impl WebhookAction {
    pub fn new(config: WebhookActionConfig) -> Self {
        let timeout = parse_duration(&config.timeout).unwrap_or(Duration::from_secs(30));
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            config,
            renderer: TemplateRenderer::new(),
        }
    }
}

#[async_trait]
impl Action for WebhookAction {
    fn action_type(&self) -> &'static str {
        "webhook"
    }

    async fn execute(&self, event: &EntityEvent, _data: &Value) -> Result<ActionResult, ObserverError> {
        let start = Instant::now();

        // Render URL
        let url = self.renderer.render(&self.config.url, &event.data)?;

        // Build request
        let mut request = match self.config.method.to_uppercase().as_str() {
            "GET" => self.client.get(&url),
            "POST" => self.client.post(&url),
            "PUT" => self.client.put(&url),
            "PATCH" => self.client.patch(&url),
            "DELETE" => self.client.delete(&url),
            _ => return Err(ObserverError::InvalidConfig(format!(
                "Invalid HTTP method: {}",
                self.config.method
            ))),
        };

        // Add headers
        for (key, value) in &self.config.headers {
            let rendered_value = self.renderer.render(value, &event.data)?;
            request = request.header(key, rendered_value);
        }

        // Add body
        if let Some(body) = &self.config.body {
            // Render body template values
            let rendered_body = self.render_body(body, &event.data)?;
            request = request.json(&rendered_body);
        } else {
            // Default: send event as body
            request = request.json(&event);
        }

        debug!(url = %url, method = %self.config.method, "Executing webhook");

        // Execute request
        let response = request.send().await.map_err(|e| {
            ObserverError::ActionFailed(format!("Webhook request failed: {}", e))
        })?;

        let status = response.status();
        let duration = start.elapsed();

        if status.is_success() {
            Ok(ActionResult {
                action_type: "webhook".to_string(),
                success: true,
                message: Some(format!("HTTP {}", status.as_u16())),
                duration_ms: duration.as_millis() as u64,
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Webhook returned error");
            Err(ObserverError::ActionFailed(format!(
                "Webhook returned {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            )))
        }
    }
}

impl WebhookAction {
    fn render_body(&self, body: &Value, data: &Value) -> Result<Value, ObserverError> {
        match body {
            Value::String(s) => {
                let rendered = self.renderer.render(s, data)?;
                Ok(Value::String(rendered))
            }
            Value::Object(map) => {
                let mut result = serde_json::Map::new();
                for (key, value) in map {
                    result.insert(key.clone(), self.render_body(value, data)?);
                }
                Ok(Value::Object(result))
            }
            Value::Array(arr) => {
                let result: Result<Vec<Value>, _> = arr
                    .iter()
                    .map(|v| self.render_body(v, data))
                    .collect();
                Ok(Value::Array(result?))
            }
            _ => Ok(body.clone()),
        }
    }
}

fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.ends_with("ms") {
        s[..s.len() - 2].parse().ok().map(Duration::from_millis)
    } else if s.ends_with('s') {
        s[..s.len() - 1].parse().ok().map(Duration::from_secs)
    } else if s.ends_with('m') {
        s[..s.len() - 1].parse::<u64>().ok().map(|m| Duration::from_secs(m * 60))
    } else {
        s.parse().ok().map(Duration::from_secs)
    }
}
```

### 6.3 Slack Action

```rust
// src/actions/slack.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Instant;
use tracing::debug;

use crate::config::SlackActionConfig;
use crate::error::ObserverError;
use crate::event::EntityEvent;
use crate::template::TemplateRenderer;

use super::{Action, ActionResult};

pub struct SlackAction {
    client: Client,
    webhook_url: String,
    config: SlackActionConfig,
    renderer: TemplateRenderer,
}

impl SlackAction {
    pub fn new(webhook_url: String, config: SlackActionConfig) -> Self {
        Self {
            client: Client::new(),
            webhook_url,
            config,
            renderer: TemplateRenderer::new(),
        }
    }
}

#[async_trait]
impl Action for SlackAction {
    fn action_type(&self) -> &'static str {
        "slack"
    }

    async fn execute(&self, event: &EntityEvent, _data: &Value) -> Result<ActionResult, ObserverError> {
        let start = Instant::now();

        // Build Slack message payload
        let payload = if let Some(blocks) = &self.config.blocks {
            // Rich message with blocks
            json!({
                "channel": self.config.channel,
                "blocks": blocks
            })
        } else if let Some(message) = &self.config.message {
            // Simple text message
            let rendered = self.renderer.render(message, &event.data)?;
            json!({
                "channel": self.config.channel,
                "text": rendered
            })
        } else {
            // Default: event summary
            json!({
                "channel": self.config.channel,
                "text": format!(
                    "*{}* - {} `{}`",
                    event.event_type,
                    event.entity,
                    event.entity_id
                )
            })
        };

        debug!(
            channel = %self.config.channel,
            event_type = %event.event_type,
            "Sending Slack notification"
        );

        let response = self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| ObserverError::ActionFailed(format!("Slack request failed: {}", e)))?;

        let duration = start.elapsed();

        if response.status().is_success() {
            Ok(ActionResult {
                action_type: "slack".to_string(),
                success: true,
                message: Some("Message sent".to_string()),
                duration_ms: duration.as_millis() as u64,
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ObserverError::ActionFailed(format!("Slack error: {}", body)))
        }
    }
}
```

---

## Step 7: Action Executor

### 7.1 Executor Orchestration

```rust
// src/executor.rs
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::actions::{Action, ActionResult};
use crate::condition::ConditionEvaluator;
use crate::config::{ActionConfig, ObserverDefinition, ObserversConfig, FailurePolicy};
use crate::dlq::DeadLetterQueue;
use crate::error::ObserverError;
use crate::event::EntityEvent;
use crate::retry::RetryExecutor;

/// Processes events and executes matching observers
pub struct ObserverExecutor {
    config: ObserversConfig,
    actions: ActionFactory,
    retry_executor: RetryExecutor,
    dlq: Arc<DeadLetterQueue>,
    metrics: ObserverMetrics,
}

impl ObserverExecutor {
    pub fn new(
        config: ObserversConfig,
        actions: ActionFactory,
        dlq: Arc<DeadLetterQueue>,
    ) -> Self {
        Self {
            config,
            actions,
            retry_executor: RetryExecutor::new(),
            dlq,
            metrics: ObserverMetrics::new(),
        }
    }

    /// Process an event through all matching observers
    pub async fn process(&self, event: EntityEvent) -> Result<(), ObserverError> {
        let start = Instant::now();

        // Find matching observers
        let observers = self.find_matching_observers(&event);

        if observers.is_empty() {
            debug!(event_type = %event.event_type, "No observers matched");
            return Ok(());
        }

        info!(
            event_type = %event.event_type,
            entity_id = %event.entity_id,
            observer_count = observers.len(),
            "Processing event"
        );

        let mut results = Vec::new();

        for (observer_name, observer) in observers {
            // Check condition
            if let Some(condition) = &observer.condition {
                match ConditionEvaluator::evaluate(condition, &event) {
                    Ok(true) => {}
                    Ok(false) => {
                        debug!(
                            observer = %observer_name,
                            condition = %condition,
                            "Condition not met, skipping"
                        );
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            observer = %observer_name,
                            error = %e,
                            "Condition evaluation failed"
                        );
                        continue;
                    }
                }
            }

            // Execute actions
            for action_config in &observer.actions {
                let result = self.execute_action(
                    &event,
                    action_config,
                    &observer.retry,
                    &observer.on_failure,
                ).await;

                results.push(result);
            }
        }

        // Record metrics
        let duration = start.elapsed();
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;

        self.metrics.record_event(
            &event.event_type,
            success_count,
            failure_count,
            duration.as_millis() as u64,
        );

        Ok(())
    }

    fn find_matching_observers(&self, event: &EntityEvent) -> Vec<(&String, &ObserverDefinition)> {
        self.config
            .observers
            .iter()
            .filter(|(name, _)| *name == &event.event_type)
            .collect()
    }

    async fn execute_action(
        &self,
        event: &EntityEvent,
        config: &ActionConfig,
        retry_config: &crate::config::RetryConfig,
        failure_policy: &FailurePolicy,
    ) -> Result<ActionResult, ObserverError> {
        let action = self.actions.create(config)?;

        // Execute with retry
        let result = self.retry_executor.execute_with_retry(
            || async { action.execute(event, &event.data).await },
            retry_config,
        ).await;

        match &result {
            Ok(r) => {
                debug!(
                    action_type = %r.action_type,
                    duration_ms = r.duration_ms,
                    "Action succeeded"
                );
            }
            Err(e) => {
                error!(action_type = %action.action_type(), error = %e, "Action failed");

                // Handle failure based on policy
                match failure_policy {
                    FailurePolicy::Log => {
                        // Already logged
                    }
                    FailurePolicy::Alert => {
                        self.send_failure_alert(event, action.action_type(), e).await;
                    }
                    FailurePolicy::Dlq => {
                        self.dlq.push(event.clone(), config.clone(), e.to_string()).await?;
                    }
                }
            }
        }

        result
    }

    async fn send_failure_alert(&self, event: &EntityEvent, action_type: &str, error: &ObserverError) {
        if let Some(alerts) = &self.config.failure_alerts {
            for channel in &alerts.channels {
                let _ = self.notify_alert_channel(channel, event, action_type, error).await;
            }
            warn!(
                event_type = %event.event_type,
                action_type = %action_type,
                error = %error,
                "Failure alert sent"
            );
        }
    }
}

/// Factory for creating action instances
pub struct ActionFactory {
    // Service dependencies
    slack_webhook_url: Option<String>,
    email_service: Option<Arc<dyn crate::actions::email::EmailService>>,
    // ... other services
}

impl ActionFactory {
    pub fn create(&self, config: &ActionConfig) -> Result<Box<dyn Action>, ObserverError> {
        match config {
            ActionConfig::Webhook(cfg) => {
                Ok(Box::new(crate::actions::webhook::WebhookAction::new(cfg.clone())))
            }
            ActionConfig::Slack(cfg) => {
                let url = self.slack_webhook_url.as_ref()
                    .ok_or_else(|| ObserverError::InvalidConfig("Slack not configured".into()))?;
                Ok(Box::new(crate::actions::slack::SlackAction::new(url.clone(), cfg.clone())))
            }
            // ... other action types
            _ => Err(ObserverError::InvalidConfig("Unsupported action type".into())),
        }
    }
}

/// Metrics for observer execution
struct ObserverMetrics {
    // Prometheus metrics would go here
}

impl ObserverMetrics {
    fn new() -> Self {
        Self {}
    }

    fn record_event(&self, _event_type: &str, _success: usize, _failures: usize, _duration_ms: u64) {
        // Record to Prometheus
    }
}
```

---

## Step 8: Retry Logic

### 8.1 Retry Executor

```rust
// src/retry.rs
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::config::{BackoffStrategy, RetryConfig};
use crate::error::ObserverError;

pub struct RetryExecutor;

impl RetryExecutor {
    pub fn new() -> Self {
        Self
    }

    /// Execute an async operation with retry logic
    pub async fn execute_with_retry<F, Fut, T>(
        &self,
        operation: F,
        config: &RetryConfig,
    ) -> Result<T, ObserverError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, ObserverError>>,
    {
        let initial_delay = parse_duration(&config.initial_delay)
            .unwrap_or(Duration::from_secs(1));
        let max_delay = parse_duration(&config.max_delay)
            .unwrap_or(Duration::from_secs(60));

        let mut attempt = 0;
        let mut delay = initial_delay;

        loop {
            attempt += 1;

            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if attempt >= config.max_attempts {
                        warn!(
                            attempt = attempt,
                            max_attempts = config.max_attempts,
                            error = %e,
                            "Max retries exceeded"
                        );
                        return Err(e);
                    }

                    debug!(
                        attempt = attempt,
                        delay_ms = delay.as_millis(),
                        error = %e,
                        "Retrying after failure"
                    );

                    sleep(delay).await;

                    // Calculate next delay
                    delay = match config.backoff {
                        BackoffStrategy::Exponential => {
                            std::cmp::min(delay * 2, max_delay)
                        }
                        BackoffStrategy::Linear => {
                            std::cmp::min(delay + initial_delay, max_delay)
                        }
                        BackoffStrategy::Fixed => delay,
                    };
                }
            }
        }
    }
}

fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.ends_with("ms") {
        s[..s.len() - 2].parse().ok().map(Duration::from_millis)
    } else if s.ends_with('s') {
        s[..s.len() - 1].parse().ok().map(Duration::from_secs)
    } else if s.ends_with('m') {
        s[..s.len() - 1].parse::<u64>().ok().map(|m| Duration::from_secs(m * 60))
    } else {
        s.parse().ok().map(Duration::from_secs)
    }
}
```

---

## Step 9: Dead Letter Queue

### 9.1 DLQ Implementation

```rust
// src/dlq.rs
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::ActionConfig;
use crate::error::ObserverError;
use crate::event::EntityEvent;

/// Dead Letter Queue for failed observer events
pub struct DeadLetterQueue {
    pool: PgPool,
}

impl DeadLetterQueue {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Push a failed event to the DLQ
    pub async fn push(
        &self,
        event: EntityEvent,
        action: ActionConfig,
        error_message: String,
    ) -> Result<Uuid, ObserverError> {
        let id = Uuid::new_v4();
        let action_type = action_type_name(&action);

        sqlx::query!(
            r#"
            INSERT INTO _system.observer_dlq (
                id, event_type, entity_type, entity_id, event_data,
                action_type, action_config, error_message, attempts,
                first_attempt_at, last_attempt_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, 1, NOW(), NOW()
            )
            "#,
            id,
            event.event_type,
            event.entity,
            event.entity_id,
            serde_json::to_value(&event)?,
            action_type,
            serde_json::to_value(&action)?,
            error_message,
        )
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Get pending DLQ items for retry
    pub async fn get_pending(
        &self,
        limit: i64,
    ) -> Result<Vec<DlqItem>, ObserverError> {
        let items = sqlx::query_as!(
            DlqItemRow,
            r#"
            SELECT
                id, event_type, entity_type, entity_id, event_data,
                action_type, action_config, error_message, attempts,
                first_attempt_at, last_attempt_at, created_at
            FROM _system.observer_dlq
            WHERE last_attempt_at < NOW() - INTERVAL '5 minutes'
            ORDER BY created_at ASC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        items.into_iter().map(DlqItem::try_from).collect()
    }

    /// Mark an item as successfully retried
    pub async fn mark_success(&self, id: Uuid) -> Result<(), ObserverError> {
        sqlx::query!(
            "DELETE FROM _system.observer_dlq WHERE id = $1",
            id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Update item after failed retry
    pub async fn mark_retry_failed(
        &self,
        id: Uuid,
        error: &str,
    ) -> Result<(), ObserverError> {
        sqlx::query!(
            r#"
            UPDATE _system.observer_dlq
            SET
                attempts = attempts + 1,
                last_attempt_at = NOW(),
                error_message = $2
            WHERE id = $1
            "#,
            id,
            error
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct DlqItem {
    pub id: Uuid,
    pub event: EntityEvent,
    pub action: ActionConfig,
    pub error_message: String,
    pub attempts: i32,
    pub first_attempt_at: DateTime<Utc>,
    pub last_attempt_at: DateTime<Utc>,
}

struct DlqItemRow {
    id: Uuid,
    event_type: String,
    entity_type: String,
    entity_id: Uuid,
    event_data: serde_json::Value,
    action_type: String,
    action_config: serde_json::Value,
    error_message: Option<String>,
    attempts: Option<i32>,
    first_attempt_at: Option<DateTime<Utc>>,
    last_attempt_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
}

impl TryFrom<DlqItemRow> for DlqItem {
    type Error = ObserverError;

    fn try_from(row: DlqItemRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            event: serde_json::from_value(row.event_data)?,
            action: serde_json::from_value(row.action_config)?,
            error_message: row.error_message.unwrap_or_default(),
            attempts: row.attempts.unwrap_or(0),
            first_attempt_at: row.first_attempt_at.unwrap_or_else(Utc::now),
            last_attempt_at: row.last_attempt_at.unwrap_or_else(Utc::now),
        })
    }
}

fn action_type_name(action: &ActionConfig) -> String {
    match action {
        ActionConfig::Email(_) => "email",
        ActionConfig::Slack(_) => "slack",
        ActionConfig::Sms(_) => "sms",
        ActionConfig::Webhook(_) => "webhook",
        ActionConfig::Push(_) => "push",
        ActionConfig::Search(_) => "search",
        ActionConfig::Cache(_) => "cache",
        ActionConfig::Custom(_) => "custom",
    }.to_string()
}
```

---

## Step 10: Database Schema

### 10.1 System Tables

```sql
-- migrations/observer_tables.sql

-- Event log for observability
CREATE TABLE IF NOT EXISTS _system.observer_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    actions_executed JSONB,
    status TEXT NOT NULL CHECK (status IN ('success', 'partial', 'failure')),
    duration_ms INTEGER,
    created_at TIMESTAMPTZ DEFAULT NOW(),

    -- Indexes for querying
    CONSTRAINT idx_observer_events_type_created
        UNIQUE (event_type, created_at DESC)
);

CREATE INDEX idx_observer_events_entity
    ON _system.observer_events(entity_type, entity_id);

CREATE INDEX idx_observer_events_status
    ON _system.observer_events(status)
    WHERE status != 'success';

-- Dead letter queue for failed events
CREATE TABLE IF NOT EXISTS _system.observer_dlq (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id UUID NOT NULL,
    event_data JSONB NOT NULL,
    action_type TEXT NOT NULL,
    action_config JSONB NOT NULL,
    error_message TEXT,
    attempts INTEGER DEFAULT 0,
    first_attempt_at TIMESTAMPTZ,
    last_attempt_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_observer_dlq_retry
    ON _system.observer_dlq(last_attempt_at)
    WHERE last_attempt_at < NOW() - INTERVAL '5 minutes';

-- Function to emit events (called by mutation functions)
CREATE OR REPLACE FUNCTION _system.emit_event(
    p_event_type TEXT,
    p_entity TEXT,
    p_entity_id UUID,
    p_data JSONB,
    p_changes JSONB DEFAULT NULL
) RETURNS VOID AS $$
BEGIN
    PERFORM pg_notify(
        'fraiseql_events',
        jsonb_build_object(
            'id', gen_random_uuid(),
            'event_type', p_event_type,
            'entity', p_entity,
            'entity_id', p_entity_id,
            'data', p_data,
            'changes', p_changes,
            'user_id', current_setting('fraiseql.user_id', true),
            'timestamp', NOW()
        )::TEXT
    );
END;
$$ LANGUAGE plpgsql;
```

---

## Verification Commands

```bash
# Build the crate
cargo build -p fraiseql-observers

# Run tests
cargo nextest run -p fraiseql-observers

# Lint
cargo clippy -p fraiseql-observers -- -D warnings

# Test with database (requires PostgreSQL)
DATABASE_URL=postgres://localhost/fraiseql_test cargo nextest run -p fraiseql-observers --features integration
```

---

## Acceptance Criteria

- [ ] Event listener connects to PostgreSQL and receives NOTIFY events
- [ ] Events are correctly parsed from JSON payload
- [ ] Observer matching works for event type patterns
- [ ] Conditions evaluate correctly (status_changed_to, field_changed, etc.)
- [ ] Template rendering supports {{ field }}, {{ field | filter }}, {{ env.VAR }}
- [ ] Webhook action sends HTTP requests with rendered templates
- [ ] Slack action sends notifications to configured channel
- [ ] Retry logic respects max_attempts and backoff strategy
- [ ] Failed events are pushed to DLQ when policy is "dlq"
- [ ] Metrics are recorded for event processing

---

---

## Step 11: Comprehensive Error Handling

### Task: Define observer-specific errors with error codes

```rust
// crates/fraiseql-observers/src/error.rs

use thiserror::Error;

/// Observer error codes for consistent error responses
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserverErrorCode {
    /// OB001: Event parsing failed
    EventParseFailed,
    /// OB002: Invalid condition syntax
    InvalidCondition,
    /// OB003: Template rendering failed
    TemplateError,
    /// OB004: Action execution failed
    ActionFailed,
    /// OB005: Webhook timeout
    WebhookTimeout,
    /// OB006: Webhook non-2xx response
    WebhookError,
    /// OB007: Email delivery failed
    EmailFailed,
    /// OB008: Slack notification failed
    SlackFailed,
    /// OB009: SMS delivery failed
    SmsFailed,
    /// OB010: Push notification failed
    PushFailed,
    /// OB011: Database error
    DatabaseError,
    /// OB012: Invalid configuration
    InvalidConfig,
    /// OB013: Retry limit exceeded
    RetryExceeded,
    /// OB014: Channel overflow (backpressure)
    ChannelOverflow,
}

impl ObserverErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EventParseFailed => "OB001",
            Self::InvalidCondition => "OB002",
            Self::TemplateError => "OB003",
            Self::ActionFailed => "OB004",
            Self::WebhookTimeout => "OB005",
            Self::WebhookError => "OB006",
            Self::EmailFailed => "OB007",
            Self::SlackFailed => "OB008",
            Self::SmsFailed => "OB009",
            Self::PushFailed => "OB010",
            Self::DatabaseError => "OB011",
            Self::InvalidConfig => "OB012",
            Self::RetryExceeded => "OB013",
            Self::ChannelOverflow => "OB014",
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::WebhookTimeout
            | Self::WebhookError
            | Self::EmailFailed
            | Self::SlackFailed
            | Self::SmsFailed
            | Self::PushFailed
            | Self::DatabaseError
        )
    }

    pub fn should_dlq(&self) -> bool {
        !matches!(
            self,
            Self::InvalidCondition
            | Self::InvalidConfig
            | Self::EventParseFailed
        )
    }
}

#[derive(Debug, Error)]
pub enum ObserverError {
    #[error("Failed to parse event: {0}")]
    EventParseFailed(String),

    #[error("Invalid condition: {0}")]
    InvalidCondition(String),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Action failed: {0}")]
    ActionFailed(String),

    #[error("Webhook timeout after {timeout_ms}ms")]
    WebhookTimeout { timeout_ms: u64 },

    #[error("Webhook returned {status}: {body}")]
    WebhookError { status: u16, body: String },

    #[error("Email delivery failed: {0}")]
    EmailFailed(String),

    #[error("Slack notification failed: {0}")]
    SlackFailed(String),

    #[error("SMS delivery failed: {0}")]
    SmsFailed(String),

    #[error("Push notification failed: {0}")]
    PushFailed(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Retry limit exceeded after {attempts} attempts")]
    RetryExceeded { attempts: u32 },

    #[error("Channel overflow, event dropped")]
    ChannelOverflow,
}

impl ObserverError {
    pub fn error_code(&self) -> ObserverErrorCode {
        match self {
            Self::EventParseFailed(_) => ObserverErrorCode::EventParseFailed,
            Self::InvalidCondition(_) => ObserverErrorCode::InvalidCondition,
            Self::TemplateError(_) => ObserverErrorCode::TemplateError,
            Self::ActionFailed(_) => ObserverErrorCode::ActionFailed,
            Self::WebhookTimeout { .. } => ObserverErrorCode::WebhookTimeout,
            Self::WebhookError { .. } => ObserverErrorCode::WebhookError,
            Self::EmailFailed(_) => ObserverErrorCode::EmailFailed,
            Self::SlackFailed(_) => ObserverErrorCode::SlackFailed,
            Self::SmsFailed(_) => ObserverErrorCode::SmsFailed,
            Self::PushFailed(_) => ObserverErrorCode::PushFailed,
            Self::Database(_) => ObserverErrorCode::DatabaseError,
            Self::InvalidConfig(_) => ObserverErrorCode::InvalidConfig,
            Self::RetryExceeded { .. } => ObserverErrorCode::RetryExceeded,
            Self::ChannelOverflow => ObserverErrorCode::ChannelOverflow,
        }
    }

    pub fn is_retryable(&self) -> bool {
        self.error_code().is_transient()
    }
}

impl From<sqlx::Error> for ObserverError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<serde_json::Error> for ObserverError {
    fn from(e: serde_json::Error) -> Self {
        Self::EventParseFailed(e.to_string())
    }
}

impl From<regex::Error> for ObserverError {
    fn from(e: regex::Error) -> Self {
        Self::InvalidCondition(e.to_string())
    }
}
```

---

## Step 12: Unit Tests

### Task: Comprehensive unit tests for observer system

```rust
// crates/fraiseql-observers/tests/condition_test.rs

use fraiseql_observers::{condition::*, event::*};
use serde_json::json;

fn create_test_event(data: serde_json::Value, changes: Option<FieldChanges>) -> EntityEvent {
    EntityEvent {
        id: uuid::Uuid::new_v4(),
        event_type: "order_updated".into(),
        entity: "Order".into(),
        entity_id: uuid::Uuid::new_v4(),
        data,
        changes,
        user_id: Some("user_123".into()),
        timestamp: chrono::Utc::now(),
    }
}

#[test]
fn test_equality_condition() {
    let event = create_test_event(json!({"status": "shipped"}), None);

    assert!(ConditionEvaluator::evaluate("status == 'shipped'", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("status == 'pending'", &event).unwrap());
}

#[test]
fn test_inequality_condition() {
    let event = create_test_event(json!({"status": "shipped"}), None);

    assert!(ConditionEvaluator::evaluate("status != 'pending'", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("status != 'shipped'", &event).unwrap());
}

#[test]
fn test_numeric_comparison() {
    let event = create_test_event(json!({"total": 100.0}), None);

    assert!(ConditionEvaluator::evaluate("total > 50", &event).unwrap());
    assert!(ConditionEvaluator::evaluate("total >= 100", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("total > 100", &event).unwrap());
    assert!(ConditionEvaluator::evaluate("total < 200", &event).unwrap());
}

#[test]
fn test_nested_field() {
    let event = create_test_event(json!({
        "customer": {
            "tier": "premium"
        }
    }), None);

    assert!(ConditionEvaluator::evaluate("customer.tier == 'premium'", &event).unwrap());
}

#[test]
fn test_field_changed() {
    let changes = FieldChanges {
        fields: [
            ("status".to_string(), FieldChange {
                old: json!("pending"),
                new: json!("shipped"),
            }),
        ].into_iter().collect(),
    };

    let event = create_test_event(json!({"status": "shipped"}), Some(changes));

    assert!(ConditionEvaluator::evaluate("field_changed('status')", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("field_changed('total')", &event).unwrap());
}

#[test]
fn test_status_changed_to() {
    let changes = FieldChanges {
        fields: [
            ("status".to_string(), FieldChange {
                old: json!("pending"),
                new: json!("shipped"),
            }),
        ].into_iter().collect(),
    };

    let event = create_test_event(json!({"status": "shipped"}), Some(changes));

    assert!(ConditionEvaluator::evaluate("status_changed_to('shipped')", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("status_changed_to('pending')", &event).unwrap());
}

#[test]
fn test_status_changed_from() {
    let changes = FieldChanges {
        fields: [
            ("status".to_string(), FieldChange {
                old: json!("pending"),
                new: json!("shipped"),
            }),
        ].into_iter().collect(),
    };

    let event = create_test_event(json!({"status": "shipped"}), Some(changes));

    assert!(ConditionEvaluator::evaluate("status_changed_from('pending')", &event).unwrap());
    assert!(!ConditionEvaluator::evaluate("status_changed_from('shipped')", &event).unwrap());
}

#[test]
fn test_and_condition() {
    let changes = FieldChanges {
        fields: [
            ("status".to_string(), FieldChange {
                old: json!("pending"),
                new: json!("shipped"),
            }),
        ].into_iter().collect(),
    };

    let event = create_test_event(json!({"status": "shipped", "total": 100.0}), Some(changes));

    assert!(ConditionEvaluator::evaluate(
        "status_changed_to('shipped') && total > 50",
        &event
    ).unwrap());

    assert!(!ConditionEvaluator::evaluate(
        "status_changed_to('shipped') && total > 200",
        &event
    ).unwrap());
}

#[test]
fn test_or_condition() {
    let event = create_test_event(json!({"status": "shipped"}), None);

    assert!(ConditionEvaluator::evaluate(
        "status == 'shipped' || status == 'delivered'",
        &event
    ).unwrap());

    assert!(!ConditionEvaluator::evaluate(
        "status == 'pending' || status == 'canceled'",
        &event
    ).unwrap());
}

#[test]
fn test_invalid_condition() {
    let event = create_test_event(json!({}), None);

    assert!(ConditionEvaluator::evaluate("invalid syntax here", &event).is_err());
}
```

```rust
// crates/fraiseql-observers/tests/executor_test.rs

use fraiseql_observers::{
    executor::*,
    event::*,
    config::*,
    testing::mocks::*,
};
use std::sync::Arc;

fn create_test_event(event_type: &str) -> EntityEvent {
    EntityEvent {
        id: uuid::Uuid::new_v4(),
        event_type: event_type.into(),
        entity: "Order".into(),
        entity_id: uuid::Uuid::new_v4(),
        data: serde_json::json!({"status": "shipped", "total": 100}),
        changes: None,
        user_id: Some("user_123".into()),
        timestamp: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_event_matches_observer() {
    let action_executor = Arc::new(MockActionExecutor::new());
    let dlq = Arc::new(MockDeadLetterQueue::new());

    let config = ObserversConfig {
        observers: [(
            "order_shipped".to_string(),
            ObserverDefinition {
                condition: None,
                actions: vec![ActionConfig::Webhook(WebhookActionConfig {
                    url: "https://example.com/webhook".into(),
                    method: "POST".into(),
                    headers: std::collections::HashMap::new(),
                    body: None,
                    timeout: "10s".into(),
                    retry: false,
                })],
                retry: RetryConfig::default(),
                on_failure: FailurePolicy::Log,
            },
        )].into_iter().collect(),
        failure_alerts: None,
    };

    let executor = ObserverExecutor::new_with_deps(
        config,
        action_executor.clone(),
        Arc::new(MockConditionEvaluator::always_true()),
        dlq,
    );

    let event = create_test_event("order_shipped");
    executor.process(event).await.unwrap();

    let executions = action_executor.get_executions();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0].event_type, "order_shipped");
}

#[tokio::test]
async fn test_event_does_not_match() {
    let action_executor = Arc::new(MockActionExecutor::new());
    let dlq = Arc::new(MockDeadLetterQueue::new());

    let config = ObserversConfig {
        observers: [(
            "order_shipped".to_string(),
            ObserverDefinition {
                condition: None,
                actions: vec![],
                retry: RetryConfig::default(),
                on_failure: FailurePolicy::Log,
            },
        )].into_iter().collect(),
        failure_alerts: None,
    };

    let executor = ObserverExecutor::new_with_deps(
        config,
        action_executor.clone(),
        Arc::new(MockConditionEvaluator::always_true()),
        dlq,
    );

    // Event type doesn't match any observer
    let event = create_test_event("order_created");
    executor.process(event).await.unwrap();

    assert!(action_executor.get_executions().is_empty());
}

#[tokio::test]
async fn test_condition_not_met() {
    let action_executor = Arc::new(MockActionExecutor::new());
    let dlq = Arc::new(MockDeadLetterQueue::new());

    let config = ObserversConfig {
        observers: [(
            "order_shipped".to_string(),
            ObserverDefinition {
                condition: Some("total > 1000".into()),
                actions: vec![ActionConfig::Webhook(WebhookActionConfig {
                    url: "https://example.com".into(),
                    method: "POST".into(),
                    headers: std::collections::HashMap::new(),
                    body: None,
                    timeout: "10s".into(),
                    retry: false,
                })],
                retry: RetryConfig::default(),
                on_failure: FailurePolicy::Log,
            },
        )].into_iter().collect(),
        failure_alerts: None,
    };

    let condition_evaluator = Arc::new(
        MockConditionEvaluator::always_true()
            .with_result("total > 1000", false)
    );

    let executor = ObserverExecutor::new_with_deps(
        config,
        action_executor.clone(),
        condition_evaluator,
        dlq,
    );

    let event = create_test_event("order_shipped");
    executor.process(event).await.unwrap();

    // Action should not execute because condition was false
    assert!(action_executor.get_executions().is_empty());
}

#[tokio::test]
async fn test_action_failure_dlq() {
    let action_executor = Arc::new(MockActionExecutor::failing("Simulated failure"));
    let dlq = Arc::new(MockDeadLetterQueue::new());

    let config = ObserversConfig {
        observers: [(
            "order_shipped".to_string(),
            ObserverDefinition {
                condition: None,
                actions: vec![ActionConfig::Webhook(WebhookActionConfig {
                    url: "https://example.com".into(),
                    method: "POST".into(),
                    headers: std::collections::HashMap::new(),
                    body: None,
                    timeout: "10s".into(),
                    retry: false,
                })],
                retry: RetryConfig { max_attempts: 1, ..Default::default() },
                on_failure: FailurePolicy::Dlq,
            },
        )].into_iter().collect(),
        failure_alerts: None,
    };

    let executor = ObserverExecutor::new_with_deps(
        config,
        action_executor,
        Arc::new(MockConditionEvaluator::always_true()),
        dlq.clone(),
    );

    let event = create_test_event("order_shipped");
    executor.process(event).await.unwrap();

    // Failed action should be in DLQ
    assert_eq!(dlq.item_count(), 1);
}

#[tokio::test]
async fn test_multiple_actions() {
    let action_executor = Arc::new(MockActionExecutor::new());
    let dlq = Arc::new(MockDeadLetterQueue::new());

    let config = ObserversConfig {
        observers: [(
            "order_shipped".to_string(),
            ObserverDefinition {
                condition: None,
                actions: vec![
                    ActionConfig::Webhook(WebhookActionConfig {
                        url: "https://example1.com".into(),
                        method: "POST".into(),
                        headers: std::collections::HashMap::new(),
                        body: None,
                        timeout: "10s".into(),
                        retry: false,
                    }),
                    ActionConfig::Slack(SlackActionConfig {
                        channel: "#orders".into(),
                        message: Some("Order shipped".into()),
                        blocks: None,
                    }),
                ],
                retry: RetryConfig::default(),
                on_failure: FailurePolicy::Log,
            },
        )].into_iter().collect(),
        failure_alerts: None,
    };

    let executor = ObserverExecutor::new_with_deps(
        config,
        action_executor.clone(),
        Arc::new(MockConditionEvaluator::always_true()),
        dlq,
    );

    let event = create_test_event("order_shipped");
    executor.process(event).await.unwrap();

    let executions = action_executor.get_executions();
    assert_eq!(executions.len(), 2);
    assert_eq!(executions[0].action_type, "webhook");
    assert_eq!(executions[1].action_type, "slack");
}
```

```rust
// crates/fraiseql-observers/tests/retry_test.rs

use fraiseql_observers::{retry::*, config::*};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn test_succeeds_first_attempt() {
    let executor = RetryExecutor::new();
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig {
        max_attempts: 3,
        initial_delay: "10ms".into(),
        max_delay: "100ms".into(),
        backoff: BackoffStrategy::Exponential,
    };

    let result = executor.execute_with_retry(
        || {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Ok::<_, fraiseql_observers::ObserverError>(42)
            }
        },
        &config,
    ).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_retries_on_failure() {
    let executor = RetryExecutor::new();
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig {
        max_attempts: 3,
        initial_delay: "10ms".into(),
        max_delay: "100ms".into(),
        backoff: BackoffStrategy::Fixed,
    };

    let result = executor.execute_with_retry(
        || {
            let a = attempts_clone.clone();
            async move {
                let count = a.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(fraiseql_observers::ObserverError::ActionFailed("temporary".into()))
                } else {
                    Ok(42)
                }
            }
        },
        &config,
    ).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_max_retries_exceeded() {
    let executor = RetryExecutor::new();
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts_clone = attempts.clone();

    let config = RetryConfig {
        max_attempts: 3,
        initial_delay: "10ms".into(),
        max_delay: "100ms".into(),
        backoff: BackoffStrategy::Fixed,
    };

    let result = executor.execute_with_retry(
        || {
            let a = attempts_clone.clone();
            async move {
                a.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(fraiseql_observers::ObserverError::ActionFailed("permanent".into()))
            }
        },
        &config,
    ).await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
```

---

## DO NOT

- Block the main event loop (all actions must be async)
- Store sensitive data (secrets) in event payloads
- Retry indefinitely (always respect max_attempts)
- Skip condition evaluation (could cause unintended side effects)
- Use synchronous HTTP clients
- Process events before database transaction commits
- Drop events silently without metrics/logging
