//! Observer authoring: actions the server runs when a row of an entity is inserted,
//! updated or deleted.
//!
//! An action is a webhook, a Slack message or an email, written with the keys the
//! server's observer runtime reads; `fraiseql compile` checks them against it. Each
//! action's destination is an enum, so an action has exactly one destination by
//! construction.
//!
//! ```
//! use fraiseql_rust::{
//!     EmailAction, Observer, ObserverEvent, SchemaRegistry, SlackAction, WebhookAction,
//!     WebhookDestination,
//! };
//!
//! let mut registry = SchemaRegistry::new();
//! registry
//!     .register_observer(
//!         Observer::new("orderShipped", "Order", ObserverEvent::Update)
//!             .with_condition("status == 'shipped'")
//!             .with_action(WebhookAction::new(WebhookDestination::UrlEnv(
//!                 "ORDER_HOOK_URL".to_string(),
//!             )))
//!             .with_action(SlackAction::new("#orders", "Order {{ id }} shipped"))
//!             .with_action(EmailAction::new("ops@example.com", "Shipped", "Order {{ id }}")),
//!     )
//!     .expect("the name is not yet registered");
//! ```

use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;

/// The row event an observer runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ObserverEvent {
    /// A row is inserted.
    Insert,
    /// A row is updated.
    Update,
    /// A row is deleted.
    Delete,
}

/// How the delay between retries grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackoffStrategy {
    /// The same delay every time.
    Fixed,
    /// The delay grows by the initial delay each time.
    Linear,
    /// The delay doubles each time.
    Exponential,
}

/// How an observer retries an action that fails. An observer given none runs on the
/// server's own policy.
///
/// Not `RetryConfig`: `fraiseql_client::RetryConfig` already names the client's retry
/// of a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObserverRetry {
    /// The attempts, the first included.
    pub max_attempts: u32,
    /// How the delay grows.
    pub backoff_strategy: BackoffStrategy,
    /// The delay before the first retry.
    pub initial_delay_ms: u64,
    /// The longest delay between retries.
    pub max_delay_ms: u64,
}

impl ObserverRetry {
    /// A retry policy.
    #[must_use]
    pub const fn new(
        max_attempts: u32,
        backoff_strategy: BackoffStrategy,
        initial_delay_ms: u64,
        max_delay_ms: u64,
    ) -> Self {
        Self {
            max_attempts,
            backoff_strategy,
            initial_delay_ms,
            max_delay_ms,
        }
    }
}

/// Where a webhook is sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookDestination {
    /// A URL written into the schema.
    Url(String),
    /// The variable the server reads the URL from at delivery.
    UrlEnv(String),
}

/// An HTTP request to a [`WebhookDestination`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookAction {
    #[serde(flatten)]
    destination: WebhookDestination,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    signing_secret_env: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    headers_env: BTreeMap<String, String>,
}

impl WebhookAction {
    /// A webhook to `destination`.
    #[must_use]
    pub const fn new(destination: WebhookDestination) -> Self {
        Self {
            destination,
            method: None,
            body_template: None,
            signing_secret_env: None,
            headers: BTreeMap::new(),
            headers_env: BTreeMap::new(),
        }
    }

    /// The HTTP method, e.g. `PUT`.
    #[must_use]
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    /// The request body, a `{{ field }}` template.
    #[must_use]
    pub fn with_body_template(mut self, template: impl Into<String>) -> Self {
        self.body_template = Some(template.into());
        self
    }

    /// The variable holding the key the request is signed with.
    #[must_use]
    pub fn with_signing_secret_env(mut self, env: impl Into<String>) -> Self {
        self.signing_secret_env = Some(env.into());
        self
    }

    /// A header with a value written into the schema.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// A header whose value the server reads from the variable `env`.
    #[must_use]
    pub fn with_header_env(mut self, name: impl Into<String>, env: impl Into<String>) -> Self {
        self.headers_env.insert(name.into(), env.into());
        self
    }
}

/// Where a Slack message is posted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlackDestination {
    /// An incoming-webhook URL written into the schema.
    WebhookUrl(String),
    /// The variable the server reads the incoming-webhook URL from.
    WebhookUrlEnv(String),
}

/// A Slack message posted to a channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SlackAction {
    channel: String,
    message_template: String,
    #[serde(flatten)]
    destination: SlackDestination,
}

impl SlackAction {
    /// The variable a Slack action reads its incoming-webhook URL from, unless told
    /// another.
    pub const DEFAULT_WEBHOOK_URL_ENV: &'static str = "SLACK_WEBHOOK_URL";

    /// Posts `message`, a `{{ field }}` template, to `channel`, at the URL in
    /// [`Self::DEFAULT_WEBHOOK_URL_ENV`].
    #[must_use]
    pub fn new(channel: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            channel: channel.into(),
            message_template: message.into(),
            destination: SlackDestination::WebhookUrlEnv(Self::DEFAULT_WEBHOOK_URL_ENV.to_string()),
        }
    }

    /// Posts to `destination` instead of the default variable.
    #[must_use]
    pub fn with_destination(mut self, destination: SlackDestination) -> Self {
        self.destination = destination;
        self
    }
}

/// An email. The sender is the server's SMTP configuration: an action cannot set one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EmailAction {
    to: String,
    subject: String,
    body_template: String,
}

impl EmailAction {
    /// An email to `to`; `body` is a `{{ field }}` template.
    #[must_use]
    pub fn new(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            subject: subject.into(),
            body_template: body.into(),
        }
    }
}

/// An action an observer runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ObserverAction {
    /// An HTTP request.
    Webhook(WebhookAction),
    /// A Slack message.
    Slack(SlackAction),
    /// An email.
    Email(EmailAction),
}

impl From<WebhookAction> for ObserverAction {
    fn from(action: WebhookAction) -> Self {
        Self::Webhook(action)
    }
}

impl From<SlackAction> for ObserverAction {
    fn from(action: SlackAction) -> Self {
        Self::Slack(action)
    }
}

impl From<EmailAction> for ObserverAction {
    fn from(action: EmailAction) -> Self {
        Self::Email(action)
    }
}

/// An observer: its actions run when a row of `entity` sees `event`, when the
/// condition, an expression on the row, holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observer {
    name: String,
    entity: String,
    event: ObserverEvent,
    #[serde(skip_serializing_if = "Option::is_none")]
    condition: Option<String>,
    actions: Vec<ObserverAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retry: Option<ObserverRetry>,
}

impl Observer {
    /// An observer named `name`, published as written, on `event` for rows of `entity`.
    #[must_use]
    pub fn new(name: impl Into<String>, entity: impl Into<String>, event: ObserverEvent) -> Self {
        Self {
            name: name.into(),
            entity: entity.into(),
            event,
            condition: None,
            actions: Vec::new(),
            retry: None,
        }
    }

    /// The name the observer is published under.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Runs the actions only when `condition` holds.
    #[must_use]
    pub fn with_condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Adds an action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<ObserverAction>) -> Self {
        self.actions.push(action.into());
        self
    }

    /// Retries a failed action by `retry` instead of the server's own policy.
    #[must_use]
    pub const fn with_retry(mut self, retry: ObserverRetry) -> Self {
        self.retry = Some(retry);
        self
    }
}

/// An observer was registered under a name already taken in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateObserver {
    /// The name registered twice.
    pub name: String,
}

impl fmt::Display for DuplicateObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "observer '{}' is already registered; each name must be unique within a schema",
            self.name
        )
    }
}

impl std::error::Error for DuplicateObserver {}
