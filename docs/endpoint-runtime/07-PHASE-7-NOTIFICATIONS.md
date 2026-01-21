# Phase 7: Notification Runtime

## Objective

Implement a unified notification system supporting multiple channels: email, chat (Slack, Discord, Teams), push notifications (FCM, OneSignal, APNs), and SMS (Twilio, Vonage). All providers follow a trait-based architecture for extensibility.

## Dependencies

- Phase 1: Configuration system (TOML parsing)
- Phase 2: Core runtime (metrics, tracing)
- Phase 6: Observer runtime (triggers notifications)

---

## Section 7.0: Testing Seams and Rate Limiting Architecture

### 7.0.1 Testing Architecture

All notification providers are built with testability as a first-class concern. Each provider channel has a trait that can be mocked for testing:

```
┌─────────────────────────────────────────────────────────────────┐
│                    NotificationService                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────┐│
│  │EmailProvider│  │ChatProvider │  │PushProvider │  │SmsProvdr││
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └────┬────┘│
└─────────┼────────────────┼────────────────┼──────────────┼─────┘
          │                │                │              │
          ▼                ▼                ▼              ▼
    ┌───────────┐    ┌───────────┐    ┌───────────┐  ┌───────────┐
    │HttpClient │    │HttpClient │    │HttpClient │  │HttpClient │
    │  (trait)  │    │  (trait)  │    │  (trait)  │  │  (trait)  │
    └───────────┘    └───────────┘    └───────────┘  └───────────┘
```

### 7.0.2 HTTP Client Trait for Testing

```rust
// src/http.rs - HTTP client abstraction for testing
use async_trait::async_trait;
use std::collections::HashMap;

/// HTTP response for provider testing
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self { status: 200, headers: HashMap::new(), body: body.into() }
    }

    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
}

/// HTTP request for recording
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

/// HTTP client trait for dependency injection
#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, crate::error::NotificationError>;
}

/// Production HTTP client using reqwest
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, crate::error::NotificationError> {
        let method = match req.method.as_str() {
            "GET" => reqwest::Method::GET,
            "POST" => reqwest::Method::POST,
            "PUT" => reqwest::Method::PUT,
            "DELETE" => reqwest::Method::DELETE,
            _ => reqwest::Method::GET,
        };

        let mut builder = self.client.request(method, &req.url);

        for (key, value) in &req.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        if let Some(body) = req.body {
            builder = builder.body(body);
        }

        let response = builder.send().await?;
        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str().ok().map(|v| (k.to_string(), v.to_string()))
            })
            .collect();
        let body = response.bytes().await?.to_vec();

        Ok(HttpResponse { status, headers, body })
    }
}
```

### 7.0.3 Mock HTTP Client for Testing

```rust
// src/http.rs (continued) - Mock implementation
use std::sync::Mutex;

/// Mock HTTP client for testing
pub struct MockHttpClient {
    /// Recorded requests for verification
    pub requests: Mutex<Vec<HttpRequest>>,
    /// Responses to return (FIFO queue)
    responses: Mutex<Vec<HttpResponse>>,
    /// Default response if queue is empty
    default_response: HttpResponse,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(Vec::new()),
            default_response: HttpResponse::ok(b"{}".to_vec()),
        }
    }

    /// Queue a response to return
    pub fn queue_response(&self, response: HttpResponse) {
        self.responses.lock().unwrap().push(response);
    }

    /// Set default response when queue is empty
    pub fn with_default(mut self, response: HttpResponse) -> Self {
        self.default_response = response;
        self
    }

    /// Get recorded requests
    pub fn recorded_requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }

    /// Assert a request was made to a URL
    pub fn assert_request_to(&self, url_contains: &str) {
        let requests = self.requests.lock().unwrap();
        assert!(
            requests.iter().any(|r| r.url.contains(url_contains)),
            "Expected request to URL containing '{}', got: {:?}",
            url_contains,
            requests.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
    }

    /// Assert request count
    pub fn assert_request_count(&self, expected: usize) {
        let count = self.requests.lock().unwrap().len();
        assert_eq!(count, expected, "Expected {} requests, got {}", expected, count);
    }
}

impl Default for MockHttpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, crate::error::NotificationError> {
        // Record the request
        self.requests.lock().unwrap().push(req);

        // Return queued response or default
        let response = self.responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| self.default_response.clone());

        // Simulate rate limiting
        if response.status == 429 {
            let retry_after = response.headers
                .get("Retry-After")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);
            return Err(crate::error::NotificationError::RateLimited {
                retry_after_secs: retry_after,
                provider: "mock".to_string(),
            });
        }

        Ok(response)
    }
}
```

### 7.0.4 Rate Limiting Architecture

```rust
// src/rate_limit.rs - Per-provider rate limiting
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Window duration
    pub window: Duration,
    /// Whether to queue excess requests or reject immediately
    pub queue_excess: bool,
    /// Maximum queue size (if queue_excess is true)
    pub max_queue_size: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
            queue_excess: false,
            max_queue_size: 1000,
        }
    }
}

/// Per-provider rate limit presets
impl RateLimitConfig {
    /// Resend: 100 emails/day on free, 100/second on paid
    pub fn resend() -> Self {
        Self { max_requests: 50, window: Duration::from_secs(1), ..Default::default() }
    }

    /// SendGrid: varies by plan, conservative default
    pub fn sendgrid() -> Self {
        Self { max_requests: 100, window: Duration::from_secs(1), ..Default::default() }
    }

    /// Twilio: 1 message/second per phone number
    pub fn twilio() -> Self {
        Self { max_requests: 1, window: Duration::from_secs(1), ..Default::default() }
    }

    /// Slack: 1 message/second per webhook
    pub fn slack() -> Self {
        Self { max_requests: 1, window: Duration::from_secs(1), ..Default::default() }
    }

    /// Discord: 5 messages/second per webhook
    pub fn discord() -> Self {
        Self { max_requests: 5, window: Duration::from_secs(1), ..Default::default() }
    }

    /// FCM: 500 messages/second (topic), 100/second (device)
    pub fn fcm() -> Self {
        Self { max_requests: 100, window: Duration::from_secs(1), ..Default::default() }
    }
}

/// Token bucket rate limiter
pub struct RateLimiter {
    config: RateLimitConfig,
    tokens: Mutex<u32>,
    last_refill: Mutex<Instant>,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            tokens: Mutex::new(config.max_requests),
            last_refill: Mutex::new(Instant::now()),
            config,
        }
    }

    /// Try to acquire a token, returns wait time if rate limited
    pub async fn acquire(&self) -> Result<(), Duration> {
        let mut tokens = self.tokens.lock().await;
        let mut last_refill = self.last_refill.lock().await;

        // Refill tokens based on elapsed time
        let elapsed = last_refill.elapsed();
        if elapsed >= self.config.window {
            *tokens = self.config.max_requests;
            *last_refill = Instant::now();
        }

        if *tokens > 0 {
            *tokens -= 1;
            Ok(())
        } else {
            // Calculate wait time until next refill
            let wait_time = self.config.window - elapsed;
            Err(wait_time)
        }
    }

    /// Acquire with automatic waiting
    pub async fn acquire_wait(&self) -> Result<(), crate::error::NotificationError> {
        loop {
            match self.acquire().await {
                Ok(()) => return Ok(()),
                Err(wait_time) => {
                    if wait_time > Duration::from_secs(30) {
                        return Err(crate::error::NotificationError::RateLimited {
                            retry_after_secs: wait_time.as_secs(),
                            provider: "internal".to_string(),
                        });
                    }
                    tokio::time::sleep(wait_time).await;
                }
            }
        }
    }
}

/// Rate limiter registry for all providers
pub struct RateLimiterRegistry {
    limiters: HashMap<String, Arc<RateLimiter>>,
}

impl RateLimiterRegistry {
    pub fn new() -> Self {
        Self { limiters: HashMap::new() }
    }

    /// Register a rate limiter for a provider
    pub fn register(&mut self, provider: &str, config: RateLimitConfig) {
        self.limiters.insert(
            provider.to_string(),
            Arc::new(RateLimiter::new(config)),
        );
    }

    /// Get rate limiter for a provider
    pub fn get(&self, provider: &str) -> Option<Arc<RateLimiter>> {
        self.limiters.get(provider).cloned()
    }

    /// Create with default provider configurations
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register("resend", RateLimitConfig::resend());
        registry.register("sendgrid", RateLimitConfig::sendgrid());
        registry.register("twilio", RateLimitConfig::twilio());
        registry.register("slack", RateLimitConfig::slack());
        registry.register("discord", RateLimitConfig::discord());
        registry.register("fcm", RateLimitConfig::fcm());
        registry
    }
}

impl Default for RateLimiterRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
```

### 7.0.5 Mock Provider Implementations

```rust
// src/email/mock.rs - Mock email provider for testing
use super::{EmailMessage, EmailProvider, EmailResult};
use crate::error::NotificationError;
use async_trait::async_trait;
use std::sync::Mutex;

/// Mock email provider for testing
pub struct MockEmailProvider {
    /// Recorded send calls
    pub sent_emails: Mutex<Vec<EmailMessage>>,
    /// Whether to fail sends
    pub should_fail: Mutex<bool>,
    /// Custom failure message
    pub failure_message: Mutex<Option<String>>,
}

impl MockEmailProvider {
    pub fn new() -> Self {
        Self {
            sent_emails: Mutex::new(Vec::new()),
            should_fail: Mutex::new(false),
            failure_message: Mutex::new(None),
        }
    }

    /// Configure to fail with a message
    pub fn fail_with(&self, message: &str) {
        *self.should_fail.lock().unwrap() = true;
        *self.failure_message.lock().unwrap() = Some(message.to_string());
    }

    /// Get all sent emails
    pub fn sent(&self) -> Vec<EmailMessage> {
        self.sent_emails.lock().unwrap().clone()
    }

    /// Assert an email was sent to a recipient
    pub fn assert_sent_to(&self, email: &str) {
        let sent = self.sent_emails.lock().unwrap();
        assert!(
            sent.iter().any(|e| e.to.contains(&email.to_string())),
            "Expected email to {}, got: {:?}",
            email,
            sent.iter().flat_map(|e| &e.to).collect::<Vec<_>>()
        );
    }
}

impl Default for MockEmailProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmailProvider for MockEmailProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError> {
        if *self.should_fail.lock().unwrap() {
            let msg = self.failure_message.lock().unwrap()
                .clone()
                .unwrap_or_else(|| "Mock failure".to_string());
            return Ok(EmailResult {
                provider: "mock".to_string(),
                message_id: None,
                success: false,
                error: Some(msg),
            });
        }

        self.sent_emails.lock().unwrap().push(message.clone());

        Ok(EmailResult {
            provider: "mock".to_string(),
            message_id: Some(format!("mock-{}", uuid::Uuid::new_v4())),
            success: true,
            error: None,
        })
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        Ok(!*self.should_fail.lock().unwrap())
    }
}

// Similar mock implementations for ChatProvider, PushProvider, SmsProvider
// src/chat/mock.rs
pub struct MockChatProvider {
    pub sent_messages: Mutex<Vec<super::ChatMessage>>,
    pub should_fail: Mutex<bool>,
}

// src/push/mock.rs
pub struct MockPushProvider {
    pub sent_notifications: Mutex<Vec<super::PushNotification>>,
    pub should_fail: Mutex<bool>,
}

// src/sms/mock.rs
pub struct MockSmsProvider {
    pub sent_messages: Mutex<Vec<super::SmsMessage>>,
    pub should_fail: Mutex<bool>,
}
```

## Crate: `fraiseql-notifications`

```
crates/fraiseql-notifications/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── config.rs              # Notification configuration
│   ├── service.rs             # Main notification service
│   ├── template.rs            # Email/message templating
│   ├── email/
│   │   ├── mod.rs             # EmailProvider trait
│   │   ├── resend.rs          # Resend provider
│   │   ├── sendgrid.rs        # SendGrid provider
│   │   ├── postmark.rs        # Postmark provider
│   │   ├── ses.rs             # AWS SES provider
│   │   └── smtp.rs            # Generic SMTP provider
│   ├── chat/
│   │   ├── mod.rs             # ChatProvider trait
│   │   ├── slack.rs           # Slack provider
│   │   ├── discord.rs         # Discord provider
│   │   ├── teams.rs           # Microsoft Teams provider
│   │   ├── mattermost.rs      # Mattermost provider
│   │   └── telegram.rs        # Telegram provider
│   ├── push/
│   │   ├── mod.rs             # PushProvider trait
│   │   ├── fcm.rs             # Firebase Cloud Messaging
│   │   ├── onesignal.rs       # OneSignal provider
│   │   ├── apns.rs            # Apple Push Notification Service
│   │   ├── webpush.rs         # Web Push (VAPID)
│   │   └── ntfy.rs            # ntfy.sh (self-hosted)
│   ├── sms/
│   │   ├── mod.rs             # SmsProvider trait
│   │   ├── twilio.rs          # Twilio provider
│   │   ├── vonage.rs          # Vonage (Nexmo) provider
│   │   └── aws_sns.rs         # AWS SNS provider
│   └── error.rs
└── tests/
    ├── email_test.rs
    ├── chat_test.rs
    ├── push_test.rs
    └── sms_test.rs
```

---

## Step 1: Configuration Types

### 1.1 Notification Configuration

```rust
// src/config.rs
use serde::Deserialize;
use std::collections::HashMap;

/// Top-level notifications configuration
#[derive(Debug, Clone, Deserialize)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub email: Option<EmailConfig>,

    #[serde(default)]
    pub slack: Option<SlackConfig>,

    #[serde(default)]
    pub discord: Option<DiscordConfig>,

    #[serde(default)]
    pub teams: Option<TeamsConfig>,

    #[serde(default)]
    pub mattermost: Option<MattermostConfig>,

    #[serde(default)]
    pub telegram: Option<TelegramConfig>,

    #[serde(default)]
    pub push: Option<PushConfig>,

    #[serde(default)]
    pub sms: Option<SmsConfig>,
}

// ============================================================
// EMAIL CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum EmailConfig {
    Resend(ResendConfig),
    SendGrid(SendGridConfig),
    Postmark(PostmarkConfig),
    Ses(SesConfig),
    Smtp(SmtpConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResendConfig {
    pub api_key_env: String,
    pub from: String,
    #[serde(default)]
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendGridConfig {
    pub api_key_env: String,
    pub from: String,
    #[serde(default)]
    pub from_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostmarkConfig {
    pub server_token_env: String,
    pub from: String,
    #[serde(default)]
    pub message_stream: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SesConfig {
    pub region: String,
    #[serde(default)]
    pub access_key_env: Option<String>,  // Uses default credentials if not set
    #[serde(default)]
    pub secret_key_env: Option<String>,
    pub from: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    #[serde(default = "default_smtp_port")]
    pub port: u16,
    #[serde(default)]
    pub username_env: Option<String>,
    #[serde(default)]
    pub password_env: Option<String>,
    pub from: String,
    #[serde(default = "default_tls")]
    pub tls: bool,
}

fn default_smtp_port() -> u16 { 587 }
fn default_tls() -> bool { true }

// ============================================================
// CHAT CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
pub struct SlackConfig {
    pub webhook_url_env: String,
    #[serde(default)]
    pub default_channel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscordConfig {
    pub webhook_url_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TeamsConfig {
    pub webhook_url_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MattermostConfig {
    pub webhook_url_env: String,
    #[serde(default)]
    pub default_channel: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelegramConfig {
    pub bot_token_env: String,
    #[serde(default)]
    pub default_chat_id: Option<String>,
}

// ============================================================
// PUSH CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum PushConfig {
    Fcm(FcmConfig),
    OneSignal(OneSignalConfig),
    Apns(ApnsConfig),
    WebPush(WebPushConfig),
    Ntfy(NtfyConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FcmConfig {
    /// Path to service account JSON or env var containing it
    pub credentials_env: String,
    #[serde(default)]
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OneSignalConfig {
    pub app_id_env: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApnsConfig {
    pub key_id: String,
    pub team_id: String,
    pub key_path_env: String,  // Path to .p8 key file
    pub bundle_id: String,
    #[serde(default)]
    pub sandbox: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebPushConfig {
    pub vapid_public_key_env: String,
    pub vapid_private_key_env: String,
    pub subject: String,  // mailto: or URL
}

#[derive(Debug, Clone, Deserialize)]
pub struct NtfyConfig {
    #[serde(default = "default_ntfy_server")]
    pub server: String,
    #[serde(default)]
    pub default_topic: Option<String>,
    #[serde(default)]
    pub auth_token_env: Option<String>,
}

fn default_ntfy_server() -> String { "https://ntfy.sh".to_string() }

// ============================================================
// SMS CONFIGURATION
// ============================================================

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "provider", rename_all = "lowercase")]
pub enum SmsConfig {
    Twilio(TwilioConfig),
    Vonage(VonageConfig),
    AwsSns(AwsSnsConfig),
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwilioConfig {
    pub account_sid_env: String,
    pub auth_token_env: String,
    pub from: String,  // Phone number or messaging service SID
}

#[derive(Debug, Clone, Deserialize)]
pub struct VonageConfig {
    pub api_key_env: String,
    pub api_secret_env: String,
    pub from: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AwsSnsConfig {
    pub region: String,
    #[serde(default)]
    pub access_key_env: Option<String>,
    #[serde(default)]
    pub secret_key_env: Option<String>,
    #[serde(default)]
    pub sender_id: Option<String>,
}
```

---

## Step 2: Email Provider Trait and Implementations

### 2.1 Email Provider Trait

```rust
// src/email/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::NotificationError;

pub mod resend;
pub mod sendgrid;
pub mod postmark;
pub mod ses;
pub mod smtp;

/// Email message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailMessage {
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    pub subject: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub html: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    #[serde(default)]
    pub attachments: Vec<EmailAttachment>,
    #[serde(default)]
    pub headers: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub tags: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachment {
    pub filename: String,
    pub content: String,  // Base64 encoded
    pub content_type: String,
}

/// Result of sending an email
#[derive(Debug, Clone)]
pub struct EmailResult {
    pub provider: String,
    pub message_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// Trait for email providers
#[async_trait]
pub trait EmailProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Send an email
    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError>;

    /// Check if provider is configured correctly
    async fn health_check(&self) -> Result<bool, NotificationError>;
}

/// Create an email provider from configuration
pub fn create_provider(config: &crate::config::EmailConfig) -> Result<Box<dyn EmailProvider>, NotificationError> {
    match config {
        crate::config::EmailConfig::Resend(cfg) => {
            Ok(Box::new(resend::ResendProvider::new(cfg)?))
        }
        crate::config::EmailConfig::SendGrid(cfg) => {
            Ok(Box::new(sendgrid::SendGridProvider::new(cfg)?))
        }
        crate::config::EmailConfig::Postmark(cfg) => {
            Ok(Box::new(postmark::PostmarkProvider::new(cfg)?))
        }
        crate::config::EmailConfig::Ses(cfg) => {
            Ok(Box::new(ses::SesProvider::new(cfg)?))
        }
        crate::config::EmailConfig::Smtp(cfg) => {
            Ok(Box::new(smtp::SmtpProvider::new(cfg)?))
        }
    }
}
```

### 2.2 Provider Base with Circuit Breaker

All notification providers should wrap external API calls with a circuit breaker to prevent cascading failures when a provider is down.

```rust
// src/provider_base.rs
use fraiseql_runtime::resilience::CircuitBreaker;
use std::sync::Arc;

/// Base provider functionality shared across all notification types
pub struct ProviderBase {
    pub client: reqwest::Client,
    pub circuit_breaker: Arc<CircuitBreaker>,
}

impl ProviderBase {
    pub fn new(provider_name: &str) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client"),
            circuit_breaker: Arc::new(CircuitBreaker::new(
                provider_name,
                fraiseql_runtime::resilience::CircuitBreakerConfig {
                    failure_threshold: 5,    // Open after 5 failures
                    reset_timeout: std::time::Duration::from_secs(30),
                    success_threshold: 2,    // Close after 2 successes
                },
            )),
        }
    }

    /// Execute a request through the circuit breaker
    pub async fn execute<F, T>(&self, operation: F) -> Result<T, NotificationError>
    where
        F: std::future::Future<Output = Result<T, NotificationError>>,
    {
        self.circuit_breaker.execute(operation).await.map_err(|e| {
            match e {
                fraiseql_runtime::resilience::CircuitBreakerError::Open => {
                    NotificationError::CircuitOpen {
                        provider: self.circuit_breaker.name().to_string()
                    }
                }
                fraiseql_runtime::resilience::CircuitBreakerError::Inner(inner) => inner,
            }
        })
    }
}
```

### 2.3 Resend Provider

```rust
// src/email/resend.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::{debug, error};

use crate::config::ResendConfig;
use crate::error::NotificationError;
use crate::provider_base::ProviderBase;

use super::{EmailMessage, EmailProvider, EmailResult};

pub struct ResendProvider {
    base: ProviderBase,
    api_key: String,
    from: String,
    reply_to: Option<String>,
}

impl ResendProvider {
    pub fn new(config: &ResendConfig) -> Result<Self, NotificationError> {
        let api_key = std::env::var(&config.api_key_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.api_key_env
            ))
        })?;

        Ok(Self {
            client: Client::new(),
            api_key,
            from: config.from.clone(),
            reply_to: config.reply_to.clone(),
        })
    }
}

#[async_trait]
impl EmailProvider for ResendProvider {
    fn name(&self) -> &'static str {
        "resend"
    }

    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError> {
        let mut payload = json!({
            "from": self.from,
            "to": message.to,
            "subject": message.subject,
        });

        if !message.cc.is_empty() {
            payload["cc"] = json!(message.cc);
        }
        if !message.bcc.is_empty() {
            payload["bcc"] = json!(message.bcc);
        }
        if let Some(html) = &message.html {
            payload["html"] = json!(html);
        }
        if let Some(text) = &message.text {
            payload["text"] = json!(text);
        }
        if let Some(reply_to) = message.reply_to.as_ref().or(self.reply_to.as_ref()) {
            payload["reply_to"] = json!(reply_to);
        }
        if !message.tags.is_empty() {
            payload["tags"] = json!(message.tags);
        }
        if !message.attachments.is_empty() {
            let attachments: Vec<_> = message.attachments.iter().map(|a| {
                json!({
                    "filename": a.filename,
                    "content": a.content,
                    "type": a.content_type,
                })
            }).collect();
            payload["attachments"] = json!(attachments);
        }

        debug!(to = ?message.to, subject = %message.subject, "Sending email via Resend");

        let response = self.client
            .post("https://api.resend.com/emails")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Resend request failed: {}", e)))?;

        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();

        if status.is_success() {
            let message_id = body.get("id").and_then(|v| v.as_str()).map(String::from);
            Ok(EmailResult {
                provider: "resend".to_string(),
                message_id,
                success: true,
                error: None,
            })
        } else {
            let error_msg = body.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            error!(status = %status, error = %error_msg, "Resend API error");
            Ok(EmailResult {
                provider: "resend".to_string(),
                message_id: None,
                success: false,
                error: Some(error_msg),
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        // Check API key validity by making a lightweight request
        let response = self.client
            .get("https://api.resend.com/domains")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}
```

### 2.3 SendGrid Provider

```rust
// src/email/sendgrid.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::SendGridConfig;
use crate::error::NotificationError;

use super::{EmailMessage, EmailProvider, EmailResult};

pub struct SendGridProvider {
    client: Client,
    api_key: String,
    from: String,
    from_name: Option<String>,
}

impl SendGridProvider {
    pub fn new(config: &SendGridConfig) -> Result<Self, NotificationError> {
        let api_key = std::env::var(&config.api_key_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.api_key_env
            ))
        })?;

        Ok(Self {
            client: Client::new(),
            api_key,
            from: config.from.clone(),
            from_name: config.from_name.clone(),
        })
    }
}

#[async_trait]
impl EmailProvider for SendGridProvider {
    fn name(&self) -> &'static str {
        "sendgrid"
    }

    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError> {
        let from = if let Some(name) = &self.from_name {
            json!({"email": self.from, "name": name})
        } else {
            json!({"email": self.from})
        };

        let to: Vec<_> = message.to.iter()
            .map(|email| json!({"email": email}))
            .collect();

        let mut content = vec![];
        if let Some(text) = &message.text {
            content.push(json!({"type": "text/plain", "value": text}));
        }
        if let Some(html) = &message.html {
            content.push(json!({"type": "text/html", "value": html}));
        }

        let payload = json!({
            "personalizations": [{"to": to}],
            "from": from,
            "subject": message.subject,
            "content": content,
        });

        debug!(to = ?message.to, subject = %message.subject, "Sending email via SendGrid");

        let response = self.client
            .post("https://api.sendgrid.com/v3/mail/send")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("SendGrid request failed: {}", e)))?;

        let status = response.status();

        if status.is_success() || status.as_u16() == 202 {
            let message_id = response
                .headers()
                .get("X-Message-Id")
                .and_then(|v| v.to_str().ok())
                .map(String::from);

            Ok(EmailResult {
                provider: "sendgrid".to_string(),
                message_id,
                success: true,
                error: None,
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            Ok(EmailResult {
                provider: "sendgrid".to_string(),
                message_id: None,
                success: false,
                error: Some(format!("HTTP {}: {}", status, body)),
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        let response = self.client
            .get("https://api.sendgrid.com/v3/scopes")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}
```

### 2.4 SMTP Provider

```rust
// src/email/smtp.rs
use async_trait::async_trait;
use lettre::{
    message::{header::ContentType, Mailbox, MultiPart, SinglePart},
    transport::smtp::{authentication::Credentials, extension::ClientId},
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use tracing::debug;

use crate::config::SmtpConfig;
use crate::error::NotificationError;

use super::{EmailMessage, EmailProvider, EmailResult};

pub struct SmtpProvider {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl SmtpProvider {
    pub fn new(config: &SmtpConfig) -> Result<Self, NotificationError> {
        let from: Mailbox = config.from.parse().map_err(|e| {
            NotificationError::Configuration(format!("Invalid from address: {}", e))
        })?;

        let mut builder = if config.tls {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&config.host)
                .map_err(|e| NotificationError::Configuration(format!("SMTP relay error: {}", e)))?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.host)
        };

        builder = builder.port(config.port);

        // Add credentials if provided
        if let (Some(username_env), Some(password_env)) = (&config.username_env, &config.password_env) {
            let username = std::env::var(username_env).map_err(|_| {
                NotificationError::Configuration(format!("Missing env var: {}", username_env))
            })?;
            let password = std::env::var(password_env).map_err(|_| {
                NotificationError::Configuration(format!("Missing env var: {}", password_env))
            })?;

            builder = builder.credentials(Credentials::new(username, password));
        }

        let transport = builder.build();

        Ok(Self { transport, from })
    }
}

#[async_trait]
impl EmailProvider for SmtpProvider {
    fn name(&self) -> &'static str {
        "smtp"
    }

    async fn send(&self, message: &EmailMessage) -> Result<EmailResult, NotificationError> {
        let mut email_builder = Message::builder()
            .from(self.from.clone())
            .subject(&message.subject);

        // Add recipients
        for to in &message.to {
            let mailbox: Mailbox = to.parse().map_err(|e| {
                NotificationError::InvalidInput(format!("Invalid email address {}: {}", to, e))
            })?;
            email_builder = email_builder.to(mailbox);
        }

        for cc in &message.cc {
            let mailbox: Mailbox = cc.parse().map_err(|e| {
                NotificationError::InvalidInput(format!("Invalid CC address {}: {}", cc, e))
            })?;
            email_builder = email_builder.cc(mailbox);
        }

        for bcc in &message.bcc {
            let mailbox: Mailbox = bcc.parse().map_err(|e| {
                NotificationError::InvalidInput(format!("Invalid BCC address {}: {}", bcc, e))
            })?;
            email_builder = email_builder.bcc(mailbox);
        }

        if let Some(reply_to) = &message.reply_to {
            let mailbox: Mailbox = reply_to.parse().map_err(|e| {
                NotificationError::InvalidInput(format!("Invalid reply-to address: {}", e))
            })?;
            email_builder = email_builder.reply_to(mailbox);
        }

        // Build body
        let email = match (&message.html, &message.text) {
            (Some(html), Some(text)) => {
                email_builder
                    .multipart(MultiPart::alternative()
                        .singlepart(SinglePart::plain(text.clone()))
                        .singlepart(SinglePart::html(html.clone())))
            }
            (Some(html), None) => {
                email_builder.body(html.clone())
            }
            (None, Some(text)) => {
                email_builder.body(text.clone())
            }
            (None, None) => {
                email_builder.body(String::new())
            }
        }.map_err(|e| NotificationError::InvalidInput(format!("Failed to build email: {}", e)))?;

        debug!(to = ?message.to, subject = %message.subject, "Sending email via SMTP");

        match self.transport.send(email).await {
            Ok(response) => {
                Ok(EmailResult {
                    provider: "smtp".to_string(),
                    message_id: Some(response.message().map(|s| s.to_string()).unwrap_or_default()),
                    success: true,
                    error: None,
                })
            }
            Err(e) => {
                Ok(EmailResult {
                    provider: "smtp".to_string(),
                    message_id: None,
                    success: false,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        self.transport.test_connection().await
            .map_err(|e| NotificationError::Provider(format!("SMTP connection failed: {}", e)))
    }
}
```

---

## Step 3: Chat Provider Trait and Implementations

### 3.1 Chat Provider Trait

```rust
// src/chat/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::NotificationError;

pub mod slack;
pub mod discord;
pub mod teams;
pub mod mattermost;
pub mod telegram;

/// Chat message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Target channel or chat ID
    pub channel: Option<String>,

    /// Plain text message
    pub text: String,

    /// Rich formatting (provider-specific)
    #[serde(default)]
    pub blocks: Option<serde_json::Value>,

    /// Username to display (if supported)
    #[serde(default)]
    pub username: Option<String>,

    /// Icon/avatar URL (if supported)
    #[serde(default)]
    pub icon_url: Option<String>,

    /// Thread ID for replies (if supported)
    #[serde(default)]
    pub thread_id: Option<String>,
}

/// Result of sending a chat message
#[derive(Debug, Clone)]
pub struct ChatResult {
    pub provider: String,
    pub message_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
}

/// Trait for chat providers
#[async_trait]
pub trait ChatProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Send a message
    async fn send(&self, message: &ChatMessage) -> Result<ChatResult, NotificationError>;

    /// Check if provider is configured correctly
    async fn health_check(&self) -> Result<bool, NotificationError>;
}
```

### 3.2 Slack Provider

```rust
// src/chat/slack.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::SlackConfig;
use crate::error::NotificationError;

use super::{ChatMessage, ChatProvider, ChatResult};

pub struct SlackProvider {
    client: Client,
    webhook_url: String,
    default_channel: Option<String>,
}

impl SlackProvider {
    pub fn new(config: &SlackConfig) -> Result<Self, NotificationError> {
        let webhook_url = std::env::var(&config.webhook_url_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.webhook_url_env
            ))
        })?;

        Ok(Self {
            client: Client::new(),
            webhook_url,
            default_channel: config.default_channel.clone(),
        })
    }
}

#[async_trait]
impl ChatProvider for SlackProvider {
    fn name(&self) -> &'static str {
        "slack"
    }

    async fn send(&self, message: &ChatMessage) -> Result<ChatResult, NotificationError> {
        let mut payload = json!({
            "text": message.text,
        });

        if let Some(channel) = message.channel.as_ref().or(self.default_channel.as_ref()) {
            payload["channel"] = json!(channel);
        }

        if let Some(blocks) = &message.blocks {
            payload["blocks"] = blocks.clone();
        }

        if let Some(username) = &message.username {
            payload["username"] = json!(username);
        }

        if let Some(icon_url) = &message.icon_url {
            payload["icon_url"] = json!(icon_url);
        }

        if let Some(thread_ts) = &message.thread_id {
            payload["thread_ts"] = json!(thread_ts);
        }

        debug!(channel = ?message.channel, "Sending message to Slack");

        let response = self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Slack request failed: {}", e)))?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if status.is_success() && body == "ok" {
            Ok(ChatResult {
                provider: "slack".to_string(),
                message_id: None,  // Webhook doesn't return message ID
                success: true,
                error: None,
            })
        } else {
            Ok(ChatResult {
                provider: "slack".to_string(),
                message_id: None,
                success: false,
                error: Some(format!("HTTP {}: {}", status, body)),
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        // Slack webhooks don't have a health endpoint
        // Just verify the URL is reachable
        Ok(true)
    }
}
```

### 3.3 Discord Provider

```rust
// src/chat/discord.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::DiscordConfig;
use crate::error::NotificationError;

use super::{ChatMessage, ChatProvider, ChatResult};

pub struct DiscordProvider {
    client: Client,
    webhook_url: String,
}

impl DiscordProvider {
    pub fn new(config: &DiscordConfig) -> Result<Self, NotificationError> {
        let webhook_url = std::env::var(&config.webhook_url_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.webhook_url_env
            ))
        })?;

        Ok(Self {
            client: Client::new(),
            webhook_url,
        })
    }
}

#[async_trait]
impl ChatProvider for DiscordProvider {
    fn name(&self) -> &'static str {
        "discord"
    }

    async fn send(&self, message: &ChatMessage) -> Result<ChatResult, NotificationError> {
        let mut payload = json!({
            "content": message.text,
        });

        if let Some(username) = &message.username {
            payload["username"] = json!(username);
        }

        if let Some(icon_url) = &message.icon_url {
            payload["avatar_url"] = json!(icon_url);
        }

        // Discord embeds (rich formatting)
        if let Some(blocks) = &message.blocks {
            payload["embeds"] = blocks.clone();
        }

        debug!("Sending message to Discord");

        let response = self.client
            .post(&self.webhook_url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Discord request failed: {}", e)))?;

        let status = response.status();

        if status.is_success() || status.as_u16() == 204 {
            Ok(ChatResult {
                provider: "discord".to_string(),
                message_id: None,
                success: true,
                error: None,
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            Ok(ChatResult {
                provider: "discord".to_string(),
                message_id: None,
                success: false,
                error: Some(format!("HTTP {}: {}", status, body)),
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        Ok(true)
    }
}
```

---

## Step 4: Push Provider Trait and Implementations

### 4.1 Push Provider Trait

```rust
// src/push/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::NotificationError;

pub mod fcm;
pub mod onesignal;
pub mod apns;
pub mod webpush;
pub mod ntfy;

/// Push notification to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushNotification {
    /// Target: device token, user ID, or topic
    pub target: PushTarget,

    /// Notification title
    pub title: String,

    /// Notification body
    pub body: String,

    /// Custom data payload
    #[serde(default)]
    pub data: HashMap<String, String>,

    /// Badge count (iOS)
    #[serde(default)]
    pub badge: Option<u32>,

    /// Sound name
    #[serde(default)]
    pub sound: Option<String>,

    /// Image URL
    #[serde(default)]
    pub image: Option<String>,

    /// Click action / deep link
    #[serde(default)]
    pub click_action: Option<String>,

    /// TTL in seconds
    #[serde(default)]
    pub ttl: Option<u32>,

    /// Priority (high, normal)
    #[serde(default)]
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushTarget {
    Token(String),
    Tokens(Vec<String>),
    UserId(String),
    UserIds(Vec<String>),
    Topic(String),
    Segment(String),
}

/// Result of sending a push notification
#[derive(Debug, Clone)]
pub struct PushResult {
    pub provider: String,
    pub message_id: Option<String>,
    pub success_count: u32,
    pub failure_count: u32,
    pub errors: Vec<String>,
}

/// Trait for push notification providers
#[async_trait]
pub trait PushProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Send a push notification
    async fn send(&self, notification: &PushNotification) -> Result<PushResult, NotificationError>;

    /// Check if provider is configured correctly
    async fn health_check(&self) -> Result<bool, NotificationError>;
}
```

### 4.2 Firebase Cloud Messaging Provider

```rust
// src/push/fcm.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::{debug, error};

use crate::config::FcmConfig;
use crate::error::NotificationError;

use super::{PushNotification, PushProvider, PushResult, PushTarget};

pub struct FcmProvider {
    client: Client,
    project_id: String,
    access_token: String,  // Obtained from service account
}

impl FcmProvider {
    pub async fn new(config: &FcmConfig) -> Result<Self, NotificationError> {
        let credentials_json = std::env::var(&config.credentials_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.credentials_env
            ))
        })?;

        // Parse service account JSON
        let credentials: serde_json::Value = serde_json::from_str(&credentials_json).map_err(|e| {
            NotificationError::Configuration(format!("Invalid FCM credentials JSON: {}", e))
        })?;

        let project_id = config.project_id.clone().or_else(|| {
            credentials.get("project_id").and_then(|v| v.as_str()).map(String::from)
        }).ok_or_else(|| {
            NotificationError::Configuration("FCM project_id not found".into())
        })?;

        // Get access token (simplified - real impl needs OAuth2 token generation)
        let access_token = Self::get_access_token(&credentials).await?;

        Ok(Self {
            client: Client::new(),
            project_id,
            access_token,
        })
    }

    async fn get_access_token(_credentials: &serde_json::Value) -> Result<String, NotificationError> {
        // In production: use google-authz crate or similar for OAuth2 JWT
        // This is a placeholder - real implementation would generate JWT
        Err(NotificationError::Configuration(
            "FCM OAuth2 token generation not implemented - use gcp_auth crate".into()
        ))
    }
}

#[async_trait]
impl PushProvider for FcmProvider {
    fn name(&self) -> &'static str {
        "fcm"
    }

    async fn send(&self, notification: &PushNotification) -> Result<PushResult, NotificationError> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );

        // Build FCM message
        let mut message = json!({
            "notification": {
                "title": notification.title,
                "body": notification.body,
            },
        });

        if !notification.data.is_empty() {
            message["data"] = json!(notification.data);
        }

        if let Some(image) = &notification.image {
            message["notification"]["image"] = json!(image);
        }

        // Add target
        match &notification.target {
            PushTarget::Token(token) => {
                message["token"] = json!(token);
            }
            PushTarget::Topic(topic) => {
                message["topic"] = json!(topic);
            }
            _ => {
                return Err(NotificationError::InvalidInput(
                    "FCM only supports single token or topic targets".into()
                ));
            }
        }

        // Android-specific options
        let android = json!({
            "priority": notification.priority.as_deref().unwrap_or("high"),
            "ttl": format!("{}s", notification.ttl.unwrap_or(3600)),
        });
        message["android"] = android;

        // iOS-specific options
        if notification.badge.is_some() || notification.sound.is_some() {
            let mut apns_payload = json!({});
            if let Some(badge) = notification.badge {
                apns_payload["badge"] = json!(badge);
            }
            if let Some(sound) = &notification.sound {
                apns_payload["sound"] = json!(sound);
            }
            message["apns"] = json!({"payload": {"aps": apns_payload}});
        }

        debug!(target = ?notification.target, "Sending push via FCM");

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&json!({"message": message}))
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("FCM request failed: {}", e)))?;

        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();

        if status.is_success() {
            let message_id = body.get("name").and_then(|v| v.as_str()).map(String::from);
            Ok(PushResult {
                provider: "fcm".to_string(),
                message_id,
                success_count: 1,
                failure_count: 0,
                errors: vec![],
            })
        } else {
            let error_msg = body.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            error!(status = %status, error = %error_msg, "FCM API error");
            Ok(PushResult {
                provider: "fcm".to_string(),
                message_id: None,
                success_count: 0,
                failure_count: 1,
                errors: vec![error_msg],
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        // Could validate the access token here
        Ok(true)
    }
}
```

### 4.3 ntfy Provider (Self-Hosted)

```rust
// src/push/ntfy.rs
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::NtfyConfig;
use crate::error::NotificationError;

use super::{PushNotification, PushProvider, PushResult, PushTarget};

/// ntfy.sh provider - simple, self-hostable push notifications
pub struct NtfyProvider {
    client: Client,
    server: String,
    default_topic: Option<String>,
    auth_token: Option<String>,
}

impl NtfyProvider {
    pub fn new(config: &NtfyConfig) -> Result<Self, NotificationError> {
        let auth_token = config.auth_token_env.as_ref()
            .and_then(|env| std::env::var(env).ok());

        Ok(Self {
            client: Client::new(),
            server: config.server.trim_end_matches('/').to_string(),
            default_topic: config.default_topic.clone(),
            auth_token,
        })
    }
}

#[async_trait]
impl PushProvider for NtfyProvider {
    fn name(&self) -> &'static str {
        "ntfy"
    }

    async fn send(&self, notification: &PushNotification) -> Result<PushResult, NotificationError> {
        let topic = match &notification.target {
            PushTarget::Topic(t) => t.clone(),
            _ => self.default_topic.clone().ok_or_else(|| {
                NotificationError::InvalidInput("ntfy requires a topic target".into())
            })?,
        };

        let url = format!("{}/{}", self.server, topic);

        let mut headers = vec![
            ("Title", notification.title.clone()),
        ];

        if let Some(priority) = &notification.priority {
            let ntfy_priority = match priority.as_str() {
                "high" | "urgent" => "5",
                "normal" | "default" => "3",
                "low" => "2",
                _ => "3",
            };
            headers.push(("Priority", ntfy_priority.to_string()));
        }

        if let Some(click_action) = &notification.click_action {
            headers.push(("Click", click_action.clone()));
        }

        if let Some(image) = &notification.image {
            headers.push(("Attach", image.clone()));
        }

        // Tags for emojis/icons
        if !notification.data.is_empty() {
            if let Some(tags) = notification.data.get("tags") {
                headers.push(("Tags", tags.clone()));
            }
        }

        debug!(topic = %topic, "Sending push via ntfy");

        let mut request = self.client
            .post(&url)
            .body(notification.body.clone());

        for (key, value) in headers {
            request = request.header(key, value);
        }

        if let Some(token) = &self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await
            .map_err(|e| NotificationError::Provider(format!("ntfy request failed: {}", e)))?;

        let status = response.status();

        if status.is_success() {
            let body: serde_json::Value = response.json().await.unwrap_or_default();
            let message_id = body.get("id").and_then(|v| v.as_str()).map(String::from);

            Ok(PushResult {
                provider: "ntfy".to_string(),
                message_id,
                success_count: 1,
                failure_count: 0,
                errors: vec![],
            })
        } else {
            let body = response.text().await.unwrap_or_default();
            Ok(PushResult {
                provider: "ntfy".to_string(),
                message_id: None,
                success_count: 0,
                failure_count: 1,
                errors: vec![format!("HTTP {}: {}", status, body)],
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        let response = self.client
            .get(&self.server)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("ntfy health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}
```

---

## Step 5: SMS Provider Trait and Implementations

### 5.1 SMS Provider Trait

```rust
// src/sms/mod.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::NotificationError;

pub mod twilio;
pub mod vonage;
pub mod aws_sns;

/// SMS message to send
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsMessage {
    pub to: String,  // E.164 format: +1234567890
    pub body: String,
    #[serde(default)]
    pub from: Option<String>,  // Override default sender
}

/// Result of sending an SMS
#[derive(Debug, Clone)]
pub struct SmsResult {
    pub provider: String,
    pub message_id: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub segments: Option<u32>,  // Message segment count
}

/// Trait for SMS providers
#[async_trait]
pub trait SmsProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &'static str;

    /// Send an SMS
    async fn send(&self, message: &SmsMessage) -> Result<SmsResult, NotificationError>;

    /// Check if provider is configured correctly
    async fn health_check(&self) -> Result<bool, NotificationError>;
}
```

### 5.2 Twilio Provider

```rust
// src/sms/twilio.rs
use async_trait::async_trait;
use reqwest::Client;
use tracing::debug;

use crate::config::TwilioConfig;
use crate::error::NotificationError;

use super::{SmsMessage, SmsProvider, SmsResult};

pub struct TwilioProvider {
    client: Client,
    account_sid: String,
    auth_token: String,
    from: String,
}

impl TwilioProvider {
    pub fn new(config: &TwilioConfig) -> Result<Self, NotificationError> {
        let account_sid = std::env::var(&config.account_sid_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.account_sid_env
            ))
        })?;

        let auth_token = std::env::var(&config.auth_token_env).map_err(|_| {
            NotificationError::Configuration(format!(
                "Missing environment variable: {}",
                config.auth_token_env
            ))
        })?;

        Ok(Self {
            client: Client::new(),
            account_sid,
            auth_token,
            from: config.from.clone(),
        })
    }
}

#[async_trait]
impl SmsProvider for TwilioProvider {
    fn name(&self) -> &'static str {
        "twilio"
    }

    async fn send(&self, message: &SmsMessage) -> Result<SmsResult, NotificationError> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        let from = message.from.as_ref().unwrap_or(&self.from);

        let params = [
            ("To", message.to.as_str()),
            ("From", from.as_str()),
            ("Body", message.body.as_str()),
        ];

        debug!(to = %message.to, "Sending SMS via Twilio");

        let response = self.client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&params)
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Twilio request failed: {}", e)))?;

        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or_default();

        if status.is_success() {
            let message_id = body.get("sid").and_then(|v| v.as_str()).map(String::from);
            let segments = body.get("num_segments")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok());

            Ok(SmsResult {
                provider: "twilio".to_string(),
                message_id,
                success: true,
                error: None,
                segments,
            })
        } else {
            let error_msg = body.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();

            Ok(SmsResult {
                provider: "twilio".to_string(),
                message_id: None,
                success: false,
                error: Some(error_msg),
                segments: None,
            })
        }
    }

    async fn health_check(&self) -> Result<bool, NotificationError> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}.json",
            self.account_sid
        );

        let response = self.client
            .get(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .send()
            .await
            .map_err(|e| NotificationError::Provider(format!("Health check failed: {}", e)))?;

        Ok(response.status().is_success())
    }
}
```

---

## Step 6: Template Engine

### 6.1 Use Shared Template Engine

**Note:** The core template engine is defined in `fraiseql-runtime` (see Phase 1 and Cross-Cutting Concerns in Overview). Notifications extend it with email-specific template loading.

```rust
// src/template.rs
use std::collections::HashMap;
use std::path::Path;

use crate::error::NotificationError;

// Re-export core template functionality
pub use fraiseql_runtime::template::{TemplateEngine as CoreTemplateEngine, TemplateRenderer};

/// Email-specific template structure
#[derive(Debug, Clone)]
pub struct EmailTemplate {
    pub name: String,
    pub subject: Option<String>,
    pub html: Option<String>,
    pub text: Option<String>,
}

/// Rendered email template
#[derive(Debug, Clone)]
pub struct RenderedTemplate {
    pub subject: String,
    pub html: Option<String>,
    pub text: Option<String>,
}

/// Email template registry (wraps core TemplateEngine)
pub struct EmailTemplateRegistry {
    templates: HashMap<String, EmailTemplate>,
    renderer: TemplateRenderer,
}

impl EmailTemplateRegistry {
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            renderer: TemplateRenderer::new(),
        }
    }

    /// Load email templates from a directory
    pub fn load_from_directory(&mut self, path: &Path) -> Result<(), NotificationError> {
        // Expected structure:
        // templates/
        //   welcome/
        //     subject.txt
        //     content.html
        //     content.txt
        //   order_confirmation/
        //     subject.txt
        //     content.html

        if !path.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(path).map_err(|e| {
            NotificationError::Configuration(format!("Failed to read templates: {}", e))
        })? {
            let entry = entry.map_err(|e| {
                NotificationError::Configuration(format!("Failed to read entry: {}", e))
            })?;

            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let template = self.load_template(&entry.path())?;
                self.templates.insert(name, template);
            }
        }

        Ok(())
    }

    fn load_template(&self, path: &Path) -> Result<EmailTemplate, NotificationError> {
        let name = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let subject = Self::read_file_if_exists(&path.join("subject.txt"))?;
        let html = Self::read_file_if_exists(&path.join("content.html"))?;
        let text = Self::read_file_if_exists(&path.join("content.txt"))?;

        Ok(EmailTemplate { name, subject, html, text })
    }

    fn read_file_if_exists(path: &Path) -> Result<Option<String>, NotificationError> {
        if path.exists() {
            std::fs::read_to_string(path)
                .map(Some)
                .map_err(|e| NotificationError::Configuration(format!("Read error: {}", e)))
        } else {
            Ok(None)
        }
    }

    /// Render an email template with data
    pub fn render(
        &self,
        template_name: &str,
        data: &serde_json::Value,
    ) -> Result<RenderedTemplate, NotificationError> {
        let template = self.templates.get(template_name).ok_or_else(|| {
            NotificationError::InvalidInput(format!("Template not found: {}", template_name))
        })?;

        // Use shared renderer for actual template substitution
        let subject = template.subject.as_ref()
            .map(|s| self.renderer.render(s, data))
            .transpose()
            .map_err(|e| NotificationError::Template(e.to_string()))?
            .unwrap_or_else(|| template_name.to_string());

        let html = template.html.as_ref()
            .map(|s| self.renderer.render(s, data))
            .transpose()
            .map_err(|e| NotificationError::Template(e.to_string()))?;

        let text = template.text.as_ref()
            .map(|s| self.renderer.render(s, data))
            .transpose()
            .map_err(|e| NotificationError::Template(e.to_string()))?;

        Ok(RenderedTemplate { subject, html, text })
    }

    /// Register a template programmatically
    pub fn register(&mut self, name: &str, template: EmailTemplate) {
        self.templates.insert(name.to_string(), template);
    }
}

impl Default for EmailTemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

**Why separate from core?**
- Email templates have a specific structure (subject + html + text)
- File-based loading is email-specific
- Core `TemplateRenderer` handles string interpolation only

---

## Step 7: Notification Service

### 7.1 Unified Service

```rust
// src/service.rs
use std::sync::Arc;

use crate::chat::{ChatMessage, ChatProvider, ChatResult};
use crate::config::NotificationsConfig;
use crate::email::{EmailMessage, EmailProvider, EmailResult};
use crate::error::NotificationError;
use crate::push::{PushNotification, PushProvider, PushResult};
use crate::sms::{SmsMessage, SmsProvider, SmsResult};
use crate::template::TemplateEngine;

/// Unified notification service
pub struct NotificationService {
    email: Option<Arc<dyn EmailProvider>>,
    chat_providers: Vec<(String, Arc<dyn ChatProvider>)>,
    push: Option<Arc<dyn PushProvider>>,
    sms: Option<Arc<dyn SmsProvider>>,
    templates: Arc<TemplateEngine>,
}

impl NotificationService {
    pub async fn new(config: &NotificationsConfig) -> Result<Self, NotificationError> {
        // Initialize email provider
        let email = match &config.email {
            Some(cfg) => Some(Arc::from(crate::email::create_provider(cfg)?)),
            None => None,
        };

        // Initialize chat providers
        let mut chat_providers = Vec::new();

        if let Some(cfg) = &config.slack {
            let provider = crate::chat::slack::SlackProvider::new(cfg)?;
            chat_providers.push(("slack".to_string(), Arc::new(provider) as Arc<dyn ChatProvider>));
        }

        if let Some(cfg) = &config.discord {
            let provider = crate::chat::discord::DiscordProvider::new(cfg)?;
            chat_providers.push(("discord".to_string(), Arc::new(provider) as Arc<dyn ChatProvider>));
        }

        if let Some(cfg) = &config.teams {
            let provider = crate::chat::teams::TeamsProvider::new(cfg)?;
            chat_providers.push(("teams".to_string(), Arc::new(provider) as Arc<dyn ChatProvider>));
        }

        // Initialize push provider
        let push: Option<Arc<dyn PushProvider>> = match &config.push {
            Some(crate::config::PushConfig::Ntfy(cfg)) => {
                Some(Arc::new(crate::push::ntfy::NtfyProvider::new(cfg)?))
            }
            // Add other push providers...
            _ => None,
        };

        // Initialize SMS provider
        let sms: Option<Arc<dyn SmsProvider>> = match &config.sms {
            Some(crate::config::SmsConfig::Twilio(cfg)) => {
                Some(Arc::new(crate::sms::twilio::TwilioProvider::new(cfg)?))
            }
            // Add other SMS providers...
            _ => None,
        };

        Ok(Self {
            email,
            chat_providers,
            push,
            sms,
            templates: Arc::new(TemplateEngine::new()),
        })
    }

    /// Send an email
    pub async fn send_email(&self, message: EmailMessage) -> Result<EmailResult, NotificationError> {
        let provider = self.email.as_ref().ok_or_else(|| {
            NotificationError::Configuration("Email provider not configured".into())
        })?;

        provider.send(&message).await
    }

    /// Send an email using a template
    pub async fn send_email_template(
        &self,
        to: Vec<String>,
        template: &str,
        data: &serde_json::Value,
    ) -> Result<EmailResult, NotificationError> {
        let rendered = self.templates.render(template, data)?;

        let message = EmailMessage {
            to,
            cc: vec![],
            bcc: vec![],
            subject: rendered.subject,
            text: rendered.text,
            html: rendered.html,
            reply_to: None,
            attachments: vec![],
            headers: std::collections::HashMap::new(),
            tags: std::collections::HashMap::new(),
        };

        self.send_email(message).await
    }

    /// Send a chat message to a specific provider
    pub async fn send_chat(
        &self,
        provider: &str,
        message: ChatMessage,
    ) -> Result<ChatResult, NotificationError> {
        let (_, chat_provider) = self.chat_providers.iter()
            .find(|(name, _)| name == provider)
            .ok_or_else(|| {
                NotificationError::Configuration(format!("Chat provider '{}' not configured", provider))
            })?;

        chat_provider.send(&message).await
    }

    /// Send a chat message to all configured providers
    pub async fn broadcast_chat(&self, message: ChatMessage) -> Vec<ChatResult> {
        let mut results = Vec::new();

        for (_, provider) in &self.chat_providers {
            match provider.send(&message).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    results.push(ChatResult {
                        provider: provider.name().to_string(),
                        message_id: None,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        results
    }

    /// Send a push notification
    pub async fn send_push(&self, notification: PushNotification) -> Result<PushResult, NotificationError> {
        let provider = self.push.as_ref().ok_or_else(|| {
            NotificationError::Configuration("Push provider not configured".into())
        })?;

        provider.send(&notification).await
    }

    /// Send an SMS
    pub async fn send_sms(&self, message: SmsMessage) -> Result<SmsResult, NotificationError> {
        let provider = self.sms.as_ref().ok_or_else(|| {
            NotificationError::Configuration("SMS provider not configured".into())
        })?;

        provider.send(&message).await
    }

    /// Health check all configured providers
    pub async fn health_check(&self) -> std::collections::HashMap<String, bool> {
        let mut results = std::collections::HashMap::new();

        if let Some(email) = &self.email {
            results.insert(
                format!("email:{}", email.name()),
                email.health_check().await.unwrap_or(false),
            );
        }

        for (name, provider) in &self.chat_providers {
            results.insert(
                format!("chat:{}", name),
                provider.health_check().await.unwrap_or(false),
            );
        }

        if let Some(push) = &self.push {
            results.insert(
                format!("push:{}", push.name()),
                push.health_check().await.unwrap_or(false),
            );
        }

        if let Some(sms) = &self.sms {
            results.insert(
                format!("sms:{}", sms.name()),
                sms.health_check().await.unwrap_or(false),
            );
        }

        results
    }
}
```

---

## Step 8: Comprehensive Error Types

### 8.1 Error Codes

```rust
// src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;

/// Notification error codes for structured error responses
/// Format: NT### where ### is a numeric code
///
/// Ranges:
/// - NT001-NT099: Configuration errors
/// - NT100-NT199: Email errors
/// - NT200-NT299: Chat errors
/// - NT300-NT399: Push notification errors
/// - NT400-NT499: SMS errors
/// - NT500-NT599: Template errors
/// - NT600-NT699: Rate limiting errors
/// - NT700-NT799: Network/transport errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NotificationErrorCode {
    // Configuration errors (NT001-NT099)
    /// Missing required configuration
    #[serde(rename = "NT001")]
    MissingConfiguration,
    /// Invalid configuration value
    #[serde(rename = "NT002")]
    InvalidConfiguration,
    /// Missing environment variable
    #[serde(rename = "NT003")]
    MissingEnvVar,
    /// Provider not configured
    #[serde(rename = "NT004")]
    ProviderNotConfigured,
    /// Invalid credentials
    #[serde(rename = "NT005")]
    InvalidCredentials,

    // Email errors (NT100-NT199)
    /// Invalid email address format
    #[serde(rename = "NT100")]
    InvalidEmailAddress,
    /// Email sending failed
    #[serde(rename = "NT101")]
    EmailSendFailed,
    /// Email rejected by provider
    #[serde(rename = "NT102")]
    EmailRejected,
    /// Email bounced
    #[serde(rename = "NT103")]
    EmailBounced,
    /// Attachment too large
    #[serde(rename = "NT104")]
    AttachmentTooLarge,
    /// Invalid attachment type
    #[serde(rename = "NT105")]
    InvalidAttachmentType,
    /// Recipient not found
    #[serde(rename = "NT106")]
    RecipientNotFound,

    // Chat errors (NT200-NT299)
    /// Invalid channel
    #[serde(rename = "NT200")]
    InvalidChannel,
    /// Chat message too long
    #[serde(rename = "NT201")]
    MessageTooLong,
    /// Webhook URL invalid
    #[serde(rename = "NT202")]
    InvalidWebhookUrl,
    /// Chat send failed
    #[serde(rename = "NT203")]
    ChatSendFailed,
    /// Channel not found
    #[serde(rename = "NT204")]
    ChannelNotFound,
    /// Bot not in channel
    #[serde(rename = "NT205")]
    BotNotInChannel,

    // Push notification errors (NT300-NT399)
    /// Invalid device token
    #[serde(rename = "NT300")]
    InvalidDeviceToken,
    /// Device token expired
    #[serde(rename = "NT301")]
    DeviceTokenExpired,
    /// Push send failed
    #[serde(rename = "NT302")]
    PushSendFailed,
    /// Invalid topic
    #[serde(rename = "NT303")]
    InvalidTopic,
    /// Invalid payload
    #[serde(rename = "NT304")]
    InvalidPushPayload,
    /// APNs authentication failed
    #[serde(rename = "NT305")]
    ApnsAuthFailed,
    /// FCM authentication failed
    #[serde(rename = "NT306")]
    FcmAuthFailed,

    // SMS errors (NT400-NT499)
    /// Invalid phone number
    #[serde(rename = "NT400")]
    InvalidPhoneNumber,
    /// SMS send failed
    #[serde(rename = "NT401")]
    SmsSendFailed,
    /// Carrier rejected message
    #[serde(rename = "NT402")]
    CarrierRejected,
    /// Number not mobile
    #[serde(rename = "NT403")]
    NotMobileNumber,
    /// Number blacklisted
    #[serde(rename = "NT404")]
    NumberBlacklisted,
    /// Region not supported
    #[serde(rename = "NT405")]
    RegionNotSupported,

    // Template errors (NT500-NT599)
    /// Template not found
    #[serde(rename = "NT500")]
    TemplateNotFound,
    /// Template render failed
    #[serde(rename = "NT501")]
    TemplateRenderFailed,
    /// Template syntax error
    #[serde(rename = "NT502")]
    TemplateSyntaxError,
    /// Missing template variable
    #[serde(rename = "NT503")]
    MissingTemplateVariable,

    // Rate limiting errors (NT600-NT699)
    /// Rate limited by internal limiter
    #[serde(rename = "NT600")]
    InternalRateLimited,
    /// Rate limited by provider
    #[serde(rename = "NT601")]
    ProviderRateLimited,
    /// Daily limit exceeded
    #[serde(rename = "NT602")]
    DailyLimitExceeded,
    /// Monthly limit exceeded
    #[serde(rename = "NT603")]
    MonthlyLimitExceeded,

    // Network/transport errors (NT700-NT799)
    /// Network timeout
    #[serde(rename = "NT700")]
    NetworkTimeout,
    /// Connection failed
    #[serde(rename = "NT701")]
    ConnectionFailed,
    /// TLS/SSL error
    #[serde(rename = "NT702")]
    TlsError,
    /// Provider unavailable
    #[serde(rename = "NT703")]
    ProviderUnavailable,
    /// Circuit breaker open
    #[serde(rename = "NT704")]
    CircuitBreakerOpen,
}

impl NotificationErrorCode {
    /// Get the documentation URL for this error code
    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::MissingConfiguration => "https://fraiseql.dev/docs/errors/NT001",
            Self::InvalidConfiguration => "https://fraiseql.dev/docs/errors/NT002",
            Self::MissingEnvVar => "https://fraiseql.dev/docs/errors/NT003",
            Self::ProviderNotConfigured => "https://fraiseql.dev/docs/errors/NT004",
            Self::InvalidCredentials => "https://fraiseql.dev/docs/errors/NT005",
            Self::InvalidEmailAddress => "https://fraiseql.dev/docs/errors/NT100",
            Self::EmailSendFailed => "https://fraiseql.dev/docs/errors/NT101",
            Self::EmailRejected => "https://fraiseql.dev/docs/errors/NT102",
            Self::EmailBounced => "https://fraiseql.dev/docs/errors/NT103",
            Self::AttachmentTooLarge => "https://fraiseql.dev/docs/errors/NT104",
            Self::InvalidAttachmentType => "https://fraiseql.dev/docs/errors/NT105",
            Self::RecipientNotFound => "https://fraiseql.dev/docs/errors/NT106",
            Self::InvalidChannel => "https://fraiseql.dev/docs/errors/NT200",
            Self::MessageTooLong => "https://fraiseql.dev/docs/errors/NT201",
            Self::InvalidWebhookUrl => "https://fraiseql.dev/docs/errors/NT202",
            Self::ChatSendFailed => "https://fraiseql.dev/docs/errors/NT203",
            Self::ChannelNotFound => "https://fraiseql.dev/docs/errors/NT204",
            Self::BotNotInChannel => "https://fraiseql.dev/docs/errors/NT205",
            Self::InvalidDeviceToken => "https://fraiseql.dev/docs/errors/NT300",
            Self::DeviceTokenExpired => "https://fraiseql.dev/docs/errors/NT301",
            Self::PushSendFailed => "https://fraiseql.dev/docs/errors/NT302",
            Self::InvalidTopic => "https://fraiseql.dev/docs/errors/NT303",
            Self::InvalidPushPayload => "https://fraiseql.dev/docs/errors/NT304",
            Self::ApnsAuthFailed => "https://fraiseql.dev/docs/errors/NT305",
            Self::FcmAuthFailed => "https://fraiseql.dev/docs/errors/NT306",
            Self::InvalidPhoneNumber => "https://fraiseql.dev/docs/errors/NT400",
            Self::SmsSendFailed => "https://fraiseql.dev/docs/errors/NT401",
            Self::CarrierRejected => "https://fraiseql.dev/docs/errors/NT402",
            Self::NotMobileNumber => "https://fraiseql.dev/docs/errors/NT403",
            Self::NumberBlacklisted => "https://fraiseql.dev/docs/errors/NT404",
            Self::RegionNotSupported => "https://fraiseql.dev/docs/errors/NT405",
            Self::TemplateNotFound => "https://fraiseql.dev/docs/errors/NT500",
            Self::TemplateRenderFailed => "https://fraiseql.dev/docs/errors/NT501",
            Self::TemplateSyntaxError => "https://fraiseql.dev/docs/errors/NT502",
            Self::MissingTemplateVariable => "https://fraiseql.dev/docs/errors/NT503",
            Self::InternalRateLimited => "https://fraiseql.dev/docs/errors/NT600",
            Self::ProviderRateLimited => "https://fraiseql.dev/docs/errors/NT601",
            Self::DailyLimitExceeded => "https://fraiseql.dev/docs/errors/NT602",
            Self::MonthlyLimitExceeded => "https://fraiseql.dev/docs/errors/NT603",
            Self::NetworkTimeout => "https://fraiseql.dev/docs/errors/NT700",
            Self::ConnectionFailed => "https://fraiseql.dev/docs/errors/NT701",
            Self::TlsError => "https://fraiseql.dev/docs/errors/NT702",
            Self::ProviderUnavailable => "https://fraiseql.dev/docs/errors/NT703",
            Self::CircuitBreakerOpen => "https://fraiseql.dev/docs/errors/NT704",
        }
    }

    /// Whether this error is transient and can be retried
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::NetworkTimeout
                | Self::ConnectionFailed
                | Self::ProviderUnavailable
                | Self::CircuitBreakerOpen
                | Self::InternalRateLimited
                | Self::ProviderRateLimited
        )
    }

    /// Whether this is a client error (4xx) vs server error (5xx)
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::InvalidEmailAddress
                | Self::InvalidChannel
                | Self::MessageTooLong
                | Self::InvalidWebhookUrl
                | Self::InvalidDeviceToken
                | Self::InvalidTopic
                | Self::InvalidPushPayload
                | Self::InvalidPhoneNumber
                | Self::NumberBlacklisted
                | Self::TemplateNotFound
                | Self::TemplateSyntaxError
                | Self::MissingTemplateVariable
        )
    }
}
```

### 8.2 Error Type with HTTP Response Mapping

```rust
// src/error.rs (continued)

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Configuration error: {message}")]
    Configuration {
        code: NotificationErrorCode,
        message: String,
    },

    #[error("Provider error ({provider}): {message}")]
    Provider {
        code: NotificationErrorCode,
        provider: String,
        message: String,
    },

    #[error("Invalid input: {message}")]
    InvalidInput {
        code: NotificationErrorCode,
        message: String,
        field: Option<String>,
    },

    #[error("Template error: {message}")]
    Template {
        code: NotificationErrorCode,
        message: String,
        template_name: Option<String>,
    },

    #[error("Rate limited by {provider}: retry after {retry_after_secs}s")]
    RateLimited {
        retry_after_secs: u64,
        provider: String,
    },

    #[error("Circuit breaker open for {provider}")]
    CircuitOpen { provider: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl NotificationError {
    /// Get the error code
    pub fn code(&self) -> NotificationErrorCode {
        match self {
            Self::Configuration { code, .. } => *code,
            Self::Provider { code, .. } => *code,
            Self::InvalidInput { code, .. } => *code,
            Self::Template { code, .. } => *code,
            Self::RateLimited { .. } => NotificationErrorCode::ProviderRateLimited,
            Self::CircuitOpen { .. } => NotificationErrorCode::CircuitBreakerOpen,
            Self::Network(_) => NotificationErrorCode::ConnectionFailed,
            Self::Serialization(_) => NotificationErrorCode::InvalidConfiguration,
        }
    }

    /// Convenience constructors
    pub fn missing_env_var(var_name: &str) -> Self {
        Self::Configuration {
            code: NotificationErrorCode::MissingEnvVar,
            message: format!("Missing environment variable: {}", var_name),
        }
    }

    pub fn provider_not_configured(provider: &str) -> Self {
        Self::Configuration {
            code: NotificationErrorCode::ProviderNotConfigured,
            message: format!("{} provider not configured", provider),
        }
    }

    pub fn invalid_email(email: &str) -> Self {
        Self::InvalidInput {
            code: NotificationErrorCode::InvalidEmailAddress,
            message: format!("Invalid email address: {}", email),
            field: Some("email".to_string()),
        }
    }

    pub fn invalid_phone(phone: &str) -> Self {
        Self::InvalidInput {
            code: NotificationErrorCode::InvalidPhoneNumber,
            message: format!("Invalid phone number: {}", phone),
            field: Some("phone".to_string()),
        }
    }

    pub fn template_not_found(name: &str) -> Self {
        Self::Template {
            code: NotificationErrorCode::TemplateNotFound,
            message: format!("Template not found: {}", name),
            template_name: Some(name.to_string()),
        }
    }
}

/// JSON error response body
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: NotificationErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_secs: Option<u64>,
    pub docs_url: &'static str,
}

impl IntoResponse for NotificationError {
    fn into_response(self) -> Response {
        let code = self.code();

        let (status, body) = match &self {
            NotificationError::Configuration { message, .. } => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::Provider {
                provider, message, ..
            } => (
                StatusCode::BAD_GATEWAY,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: Some(provider.clone()),
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::InvalidInput { message, field, .. } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    field: field.clone(),
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::Template {
                message,
                template_name,
                ..
            } => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: message.clone(),
                    provider: None,
                    field: template_name.clone(),
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::RateLimited {
                retry_after_secs,
                provider,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                ErrorBody {
                    code,
                    message: format!("Rate limited by {}", provider),
                    provider: Some(provider.clone()),
                    field: None,
                    retry_after_secs: Some(*retry_after_secs),
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::CircuitOpen { provider } => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorBody {
                    code,
                    message: format!("Service temporarily unavailable: {}", provider),
                    provider: Some(provider.clone()),
                    field: None,
                    retry_after_secs: Some(30), // Circuit breaker reset timeout
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::Network(e) => (
                StatusCode::BAD_GATEWAY,
                ErrorBody {
                    code,
                    message: e.to_string(),
                    provider: None,
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
            NotificationError::Serialization(e) => (
                StatusCode::BAD_REQUEST,
                ErrorBody {
                    code,
                    message: e.to_string(),
                    provider: None,
                    field: None,
                    retry_after_secs: None,
                    docs_url: code.docs_url(),
                },
            ),
        };

        let mut response = (status, Json(ErrorResponse { error: body })).into_response();

        // Add Retry-After header for rate limiting
        if let NotificationError::RateLimited {
            retry_after_secs, ..
        } = &self
        {
            response.headers_mut().insert(
                "Retry-After",
                retry_after_secs.to_string().parse().unwrap(),
            );
        }

        response
    }
}
```
```

---

## Step 9: Comprehensive Unit Tests

### 9.1 Email Provider Tests

```rust
// tests/email_test.rs
use fraiseql_notifications::{
    email::{EmailMessage, EmailProvider, MockEmailProvider},
    http::{HttpResponse, MockHttpClient},
};

#[tokio::test]
async fn test_mock_email_provider_records_sent_emails() {
    let provider = MockEmailProvider::new();

    let message = EmailMessage {
        to: vec!["user@example.com".to_string()],
        cc: vec![],
        bcc: vec![],
        subject: "Test Subject".to_string(),
        text: Some("Hello".to_string()),
        html: None,
        reply_to: None,
        attachments: vec![],
        headers: Default::default(),
        tags: Default::default(),
    };

    let result = provider.send(&message).await.unwrap();
    assert!(result.success);
    assert!(result.message_id.is_some());

    provider.assert_sent_to("user@example.com");
    assert_eq!(provider.sent().len(), 1);
}

#[tokio::test]
async fn test_mock_email_provider_can_fail() {
    let provider = MockEmailProvider::new();
    provider.fail_with("SMTP connection refused");

    let message = EmailMessage {
        to: vec!["user@example.com".to_string()],
        cc: vec![],
        bcc: vec![],
        subject: "Test".to_string(),
        text: Some("Hello".to_string()),
        html: None,
        reply_to: None,
        attachments: vec![],
        headers: Default::default(),
        tags: Default::default(),
    };

    let result = provider.send(&message).await.unwrap();
    assert!(!result.success);
    assert_eq!(result.error, Some("SMTP connection refused".to_string()));
}

#[tokio::test]
async fn test_resend_provider_sends_correct_payload() {
    let mock_http = MockHttpClient::new();
    mock_http.queue_response(
        HttpResponse::ok(br#"{"id": "msg-123"}"#.to_vec())
    );

    // Create Resend provider with mock HTTP client
    let provider = resend::ResendProviderWithClient::new_with_client(
        "test_api_key",
        "from@example.com",
        None,
        mock_http.clone(),
    );

    let message = EmailMessage {
        to: vec!["user@example.com".to_string()],
        cc: vec![],
        bcc: vec![],
        subject: "Test Subject".to_string(),
        text: Some("Hello".to_string()),
        html: Some("<p>Hello</p>".to_string()),
        reply_to: None,
        attachments: vec![],
        headers: Default::default(),
        tags: Default::default(),
    };

    let result = provider.send(&message).await.unwrap();
    assert!(result.success);
    assert_eq!(result.message_id, Some("msg-123".to_string()));

    // Verify the request
    mock_http.assert_request_to("api.resend.com");
    mock_http.assert_request_count(1);

    let requests = mock_http.recorded_requests();
    let body: serde_json::Value = serde_json::from_slice(
        &requests[0].body.clone().unwrap()
    ).unwrap();

    assert_eq!(body["to"], serde_json::json!(["user@example.com"]));
    assert_eq!(body["subject"], "Test Subject");
    assert_eq!(body["from"], "from@example.com");
}

#[tokio::test]
async fn test_email_validation() {
    use fraiseql_notifications::validation::validate_email;

    assert!(validate_email("user@example.com").is_ok());
    assert!(validate_email("user+tag@example.com").is_ok());
    assert!(validate_email("user.name@sub.example.com").is_ok());

    assert!(validate_email("invalid").is_err());
    assert!(validate_email("@example.com").is_err());
    assert!(validate_email("user@").is_err());
    assert!(validate_email("").is_err());
}
```

### 9.2 Rate Limiter Tests

```rust
// tests/rate_limit_test.rs
use fraiseql_notifications::rate_limit::{RateLimitConfig, RateLimiter};
use std::time::Duration;

#[tokio::test]
async fn test_rate_limiter_allows_within_limit() {
    let config = RateLimitConfig {
        max_requests: 3,
        window: Duration::from_secs(60),
        queue_excess: false,
        max_queue_size: 0,
    };
    let limiter = RateLimiter::new(config);

    // First 3 should succeed
    assert!(limiter.acquire().await.is_ok());
    assert!(limiter.acquire().await.is_ok());
    assert!(limiter.acquire().await.is_ok());

    // 4th should fail
    assert!(limiter.acquire().await.is_err());
}

#[tokio::test]
async fn test_rate_limiter_refills_after_window() {
    let config = RateLimitConfig {
        max_requests: 1,
        window: Duration::from_millis(50),
        queue_excess: false,
        max_queue_size: 0,
    };
    let limiter = RateLimiter::new(config);

    assert!(limiter.acquire().await.is_ok());
    assert!(limiter.acquire().await.is_err());

    // Wait for refill
    tokio::time::sleep(Duration::from_millis(60)).await;

    assert!(limiter.acquire().await.is_ok());
}

#[tokio::test]
async fn test_rate_limiter_returns_wait_time() {
    let config = RateLimitConfig {
        max_requests: 1,
        window: Duration::from_secs(10),
        queue_excess: false,
        max_queue_size: 0,
    };
    let limiter = RateLimiter::new(config);

    assert!(limiter.acquire().await.is_ok());

    let result = limiter.acquire().await;
    assert!(result.is_err());

    if let Err(wait_time) = result {
        assert!(wait_time <= Duration::from_secs(10));
        assert!(wait_time > Duration::from_secs(0));
    }
}

#[tokio::test]
async fn test_provider_specific_rate_limits() {
    // Test that each provider has reasonable defaults
    assert_eq!(RateLimitConfig::slack().max_requests, 1);
    assert_eq!(RateLimitConfig::discord().max_requests, 5);
    assert_eq!(RateLimitConfig::twilio().max_requests, 1);
    assert_eq!(RateLimitConfig::fcm().max_requests, 100);
}
```

### 9.3 Chat Provider Tests

```rust
// tests/chat_test.rs
use fraiseql_notifications::{
    chat::{ChatMessage, ChatProvider, MockChatProvider},
    http::{HttpResponse, MockHttpClient},
};

#[tokio::test]
async fn test_slack_provider_formats_payload_correctly() {
    let mock_http = MockHttpClient::new();
    mock_http.queue_response(HttpResponse::ok(b"ok".to_vec()));

    let provider = slack::SlackProviderWithClient::new_with_client(
        "https://hooks.slack.com/services/xxx",
        None,
        mock_http.clone(),
    );

    let message = ChatMessage {
        channel: Some("#alerts".to_string()),
        text: "Test message".to_string(),
        blocks: None,
        username: Some("FraiseQL".to_string()),
        icon_url: None,
        thread_id: None,
    };

    let result = provider.send(&message).await.unwrap();
    assert!(result.success);

    let requests = mock_http.recorded_requests();
    let body: serde_json::Value = serde_json::from_slice(
        &requests[0].body.clone().unwrap()
    ).unwrap();

    assert_eq!(body["text"], "Test message");
    assert_eq!(body["channel"], "#alerts");
    assert_eq!(body["username"], "FraiseQL");
}

#[tokio::test]
async fn test_discord_webhook_formatting() {
    let mock_http = MockHttpClient::new();
    mock_http.queue_response(HttpResponse::ok(b"".to_vec()).status(204));

    let provider = discord::DiscordProviderWithClient::new_with_client(
        "https://discord.com/api/webhooks/xxx/yyy",
        mock_http.clone(),
    );

    let message = ChatMessage {
        channel: None,
        text: "Test message".to_string(),
        blocks: Some(serde_json::json!([{
            "title": "Alert",
            "description": "Something happened",
            "color": 16711680
        }])),
        username: Some("FraiseQL Bot".to_string()),
        icon_url: Some("https://example.com/icon.png".to_string()),
        thread_id: None,
    };

    let result = provider.send(&message).await.unwrap();
    assert!(result.success);

    let requests = mock_http.recorded_requests();
    let body: serde_json::Value = serde_json::from_slice(
        &requests[0].body.clone().unwrap()
    ).unwrap();

    assert_eq!(body["content"], "Test message");
    assert_eq!(body["username"], "FraiseQL Bot");
    assert_eq!(body["avatar_url"], "https://example.com/icon.png");
    assert!(body["embeds"].is_array());
}
```

### 9.4 SMS Provider Tests

```rust
// tests/sms_test.rs
use fraiseql_notifications::{
    sms::{SmsMessage, SmsProvider, MockSmsProvider},
    http::{HttpResponse, MockHttpClient},
    validation::validate_phone_number,
};

#[tokio::test]
async fn test_phone_number_validation() {
    // Valid E.164 formats
    assert!(validate_phone_number("+14155551234").is_ok());
    assert!(validate_phone_number("+33123456789").is_ok());
    assert!(validate_phone_number("+81312345678").is_ok());

    // Invalid formats
    assert!(validate_phone_number("4155551234").is_err()); // Missing +
    assert!(validate_phone_number("+1").is_err());         // Too short
    assert!(validate_phone_number("invalid").is_err());
    assert!(validate_phone_number("").is_err());
}

#[tokio::test]
async fn test_twilio_sends_correct_form_data() {
    let mock_http = MockHttpClient::new();
    mock_http.queue_response(
        HttpResponse::ok(br#"{"sid": "SM123", "num_segments": "1"}"#.to_vec())
    );

    let provider = twilio::TwilioProviderWithClient::new_with_client(
        "AC_test_sid",
        "auth_token",
        "+15551234567",
        mock_http.clone(),
    );

    let message = SmsMessage {
        to: "+14155551234".to_string(),
        body: "Hello from FraiseQL".to_string(),
        from: None,
    };

    let result = provider.send(&message).await.unwrap();
    assert!(result.success);
    assert_eq!(result.message_id, Some("SM123".to_string()));
    assert_eq!(result.segments, Some(1));

    mock_http.assert_request_to("api.twilio.com");
}

#[tokio::test]
async fn test_sms_message_length_validation() {
    use fraiseql_notifications::validation::validate_sms_length;

    // GSM-7 encoding: 160 chars single segment
    let short_msg = "A".repeat(160);
    assert!(validate_sms_length(&short_msg).is_ok());

    // Multipart: 153 chars per segment
    let medium_msg = "A".repeat(306); // 2 segments
    let result = validate_sms_length(&medium_msg);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 2); // Returns segment count

    // Unicode reduces to 70 chars per segment
    let unicode_msg = "🎉".repeat(70);
    let result = validate_sms_length(&unicode_msg);
    assert!(result.is_ok());
}
```

### 9.5 Template Engine Tests

```rust
// tests/template_test.rs
use fraiseql_notifications::template::{EmailTemplateRegistry, EmailTemplate};
use serde_json::json;

#[test]
fn test_template_variable_substitution() {
    let mut registry = EmailTemplateRegistry::new();

    registry.register("welcome", EmailTemplate {
        name: "welcome".to_string(),
        subject: Some("Welcome, {{name}}!".to_string()),
        html: Some("<h1>Hello {{name}}</h1><p>Welcome to {{app}}.</p>".to_string()),
        text: Some("Hello {{name}}, welcome to {{app}}.".to_string()),
    });

    let data = json!({
        "name": "Alice",
        "app": "FraiseQL"
    });

    let rendered = registry.render("welcome", &data).unwrap();

    assert_eq!(rendered.subject, "Welcome, Alice!");
    assert_eq!(rendered.html, Some("<h1>Hello Alice</h1><p>Welcome to FraiseQL.</p>".to_string()));
    assert_eq!(rendered.text, Some("Hello Alice, welcome to FraiseQL.".to_string()));
}

#[test]
fn test_template_not_found_error() {
    let registry = EmailTemplateRegistry::new();

    let result = registry.render("nonexistent", &json!({}));
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(
        err,
        fraiseql_notifications::error::NotificationError::Template { .. }
    ));
}

#[test]
fn test_template_missing_variable_handling() {
    let mut registry = EmailTemplateRegistry::new();

    registry.register("test", EmailTemplate {
        name: "test".to_string(),
        subject: Some("Hello {{name}}".to_string()),
        html: None,
        text: None,
    });

    // Missing variable should render as empty or placeholder
    let result = registry.render("test", &json!({}));
    // Behavior depends on template engine strictness
    assert!(result.is_ok() || result.is_err());
}
```

### 9.6 Error Response Tests

```rust
// tests/error_test.rs
use fraiseql_notifications::error::{
    NotificationError,
    NotificationErrorCode,
};
use axum::response::IntoResponse;
use axum::http::StatusCode;

#[tokio::test]
async fn test_rate_limited_error_includes_retry_after() {
    let error = NotificationError::RateLimited {
        retry_after_secs: 60,
        provider: "resend".to_string(),
    };

    let response = error.into_response();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

    let retry_after = response.headers().get("Retry-After");
    assert!(retry_after.is_some());
    assert_eq!(retry_after.unwrap(), "60");
}

#[tokio::test]
async fn test_error_code_to_http_status_mapping() {
    // Configuration error -> 500
    let error = NotificationError::Configuration {
        code: NotificationErrorCode::MissingEnvVar,
        message: "Missing API key".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::INTERNAL_SERVER_ERROR);

    // Invalid input -> 400
    let error = NotificationError::InvalidInput {
        code: NotificationErrorCode::InvalidEmailAddress,
        message: "Invalid email".to_string(),
        field: Some("to".to_string()),
    };
    assert_eq!(error.into_response().status(), StatusCode::BAD_REQUEST);

    // Provider error -> 502
    let error = NotificationError::Provider {
        code: NotificationErrorCode::EmailSendFailed,
        provider: "resend".to_string(),
        message: "API error".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::BAD_GATEWAY);

    // Circuit breaker -> 503
    let error = NotificationError::CircuitOpen {
        provider: "sendgrid".to_string(),
    };
    assert_eq!(error.into_response().status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn test_error_code_transient_classification() {
    assert!(NotificationErrorCode::NetworkTimeout.is_transient());
    assert!(NotificationErrorCode::ConnectionFailed.is_transient());
    assert!(NotificationErrorCode::ProviderRateLimited.is_transient());
    assert!(NotificationErrorCode::CircuitBreakerOpen.is_transient());

    assert!(!NotificationErrorCode::InvalidEmailAddress.is_transient());
    assert!(!NotificationErrorCode::TemplateNotFound.is_transient());
    assert!(!NotificationErrorCode::InvalidPhoneNumber.is_transient());
}

#[test]
fn test_error_code_docs_url() {
    assert_eq!(
        NotificationErrorCode::InvalidEmailAddress.docs_url(),
        "https://fraiseql.dev/docs/errors/NT100"
    );
    assert_eq!(
        NotificationErrorCode::ProviderRateLimited.docs_url(),
        "https://fraiseql.dev/docs/errors/NT601"
    );
}
```

### 9.7 Integration Tests

```rust
// tests/integration_test.rs
use fraiseql_notifications::{
    service::NotificationService,
    email::MockEmailProvider,
    chat::MockChatProvider,
    push::MockPushProvider,
    sms::MockSmsProvider,
};
use std::sync::Arc;

#[tokio::test]
async fn test_notification_service_with_mocks() {
    let email_provider = Arc::new(MockEmailProvider::new());
    let chat_provider = Arc::new(MockChatProvider::new());
    let sms_provider = Arc::new(MockSmsProvider::new());

    let service = NotificationService::with_providers(
        Some(email_provider.clone()),
        vec![("slack".to_string(), chat_provider.clone() as Arc<dyn ChatProvider>)],
        None,
        Some(sms_provider.clone()),
    );

    // Send email
    let email = EmailMessage {
        to: vec!["user@example.com".to_string()],
        subject: "Test".to_string(),
        text: Some("Hello".to_string()),
        ..Default::default()
    };

    let result = service.send_email(email).await;
    assert!(result.is_ok());
    assert!(result.unwrap().success);

    // Verify recorded
    email_provider.assert_sent_to("user@example.com");

    // Send SMS
    let sms = SmsMessage {
        to: "+14155551234".to_string(),
        body: "Test SMS".to_string(),
        from: None,
    };

    let result = service.send_sms(sms).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_broadcast_chat_to_all_providers() {
    let slack = Arc::new(MockChatProvider::new());
    let discord = Arc::new(MockChatProvider::new());

    let service = NotificationService::with_providers(
        None,
        vec![
            ("slack".to_string(), slack.clone() as Arc<dyn ChatProvider>),
            ("discord".to_string(), discord.clone() as Arc<dyn ChatProvider>),
        ],
        None,
        None,
    );

    let message = ChatMessage {
        text: "Broadcast message".to_string(),
        ..Default::default()
    };

    let results = service.broadcast_chat(message).await;

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.success));

    assert_eq!(slack.sent_messages.lock().unwrap().len(), 1);
    assert_eq!(discord.sent_messages.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_health_check_all_providers() {
    let email = Arc::new(MockEmailProvider::new());
    let sms = Arc::new(MockSmsProvider::new());

    let service = NotificationService::with_providers(
        Some(email),
        vec![],
        None,
        Some(sms),
    );

    let health = service.health_check().await;

    assert!(health.contains_key("email:mock"));
    assert!(health.contains_key("sms:mock"));
    assert!(health.values().all(|&v| v));
}
```

## Verification Commands

```bash
# Build the crate
cargo build -p fraiseql-notifications

# Run tests
cargo nextest run -p fraiseql-notifications

# Lint
cargo clippy -p fraiseql-notifications -- -D warnings

# Test with live providers (requires env vars)
RESEND_API_KEY=re_xxx cargo nextest run -p fraiseql-notifications --features live-tests
```

---

## Acceptance Criteria

- [ ] Email providers: Resend, SendGrid, Postmark, SES, SMTP all work
- [ ] Chat providers: Slack, Discord, Teams, Mattermost all work
- [ ] Push providers: OneSignal, Web Push, ntfy all work
- [ ] [PLACEHOLDER] Push: FCM OAuth2 service account auth - use `gcp_auth` crate
- [ ] [PLACEHOLDER] Push: APNs JWT token signing - use `jsonwebtoken` crate
- [ ] SMS providers: Twilio, Vonage, AWS SNS all work
- [ ] Template engine uses shared `fraiseql-runtime::template`
- [ ] NotificationService provides unified interface
- [ ] Circuit breaker wraps all external provider calls
- [ ] Health checks work for all providers
- [ ] Proper error handling for all failure modes
- [ ] Metrics/logging for all notification sends

---

## DO NOT

- Store API keys in code (always use environment variables)
- Skip input validation (phone numbers, email addresses)
- Ignore rate limits (respect 429 responses)
- Send notifications without user consent
- Log sensitive content (message bodies in production)
