//! Core action implementations (webhook, Slack, email).
//!
//! This module implements the core action executors:
//! - Webhook: POST to HTTP endpoint
//! - Slack: Send messages to Slack webhook
//! - Email: Send emails via SMTP
//!
//! Each action handles template rendering, retry logic, and error handling.

use std::{collections::HashMap, time::Duration};

use reqwest::Client;
use serde_json::{Value, json};
use tracing::{debug, info};

use crate::{
    error::{ObserverError, Result},
    event::EntityEvent,
};

#[cfg(test)]
mod tests;

/// Default HTTP request timeout for outbound webhook calls.
///
/// Prevents a slow or non-responsive endpoint from blocking the executor
/// indefinitely.  Operators can override this by constructing the client
/// manually via [`WebhookAction::with_timeout`].
pub(crate) const DEFAULT_WEBHOOK_TIMEOUT_SECS: u64 = 30;

/// Header carrying the HMAC-SHA256 signature of the webhook body.
///
/// Stripe-compatible shape (`t=<unix_ts>,v1=<hex>`); verifiable with
/// `fraiseql-webhooks`'s `StripeVerifier` (#345).
pub(crate) const WEBHOOK_SIGNATURE_HEADER: &str = "X-FraiseQL-Signature-256";

/// Compute the Stripe-shape signature header value for the exact `body_bytes`.
///
/// Returns `t=<ts>,v1=<hex>` where the HMAC-SHA256 is taken over the byte
/// sequence `"<ts>.<body>"` — byte-identical to the signing base used by
/// `fraiseql_webhooks::signature::stripe::StripeVerifier`
/// (`format!("{t}.{}", String::from_utf8_lossy(body))`), so a receiver can
/// verify the payload with that verifier.
///
/// `body_bytes` MUST be the exact bytes transmitted on the wire (see
/// [`WebhookAction::execute`], which signs the same buffer it sends) — signing a
/// re-serialization can diverge from the transmitted bytes and fail every
/// external verification.
pub(crate) fn webhook_signature(secret: &str, ts: i64, body_bytes: &[u8]) -> String {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;

    let ts_str = ts.to_string();
    let mut signed = Vec::with_capacity(ts_str.len() + 1 + body_bytes.len());
    signed.extend_from_slice(ts_str.as_bytes());
    signed.push(b'.');
    signed.extend_from_slice(body_bytes);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC-SHA256 accepts a key of any length");
    mac.update(&signed);
    let hex = hex::encode(mac.finalize().into_bytes());

    format!("t={ts},v1={hex}")
}

/// Validate an outbound URL for SSRF risk before sending a request.
///
/// Rejects:
/// - Non-HTTP(S) schemes (`file://`, `ftp://`, etc.)
/// - Loopback addresses (`localhost`, `127.x.x.x`, `::1`)
/// - RFC 1918 private ranges (10/8, 172.16/12, 192.168/16)
/// - Link-local (169.254/16), CGNAT (100.64/10), ULA (`fc00::/7`)
///
/// Attacker-controlled observer configs could redirect outbound webhook
/// calls to AWS EC2 metadata (`169.254.169.254`), internal Kubernetes
/// services (`svc.cluster.local`), or any other SSRF target.
///
/// # Errors
///
/// Returns `ObserverError::ActionPermanentlyFailed` if the URL uses a
/// non-HTTP(S) scheme or resolves to a blocked address range.
pub(crate) fn validate_outbound_url(url: &str) -> Result<()> {
    // The `FRAISEQL_OBSERVERS_ALLOW_INSECURE` bypass is honored only in
    // development environments; the centralised guard refuses it when any
    // production marker is set (KUBERNETES_SERVICE_HOST, FRAISEQL_ENV=production,
    // FRAISEQL_PROFILE=production).  See `insecure_guard` module docs.
    if crate::insecure_guard::is_outbound_insecure_allowed() {
        return Ok(());
    }

    let lower = url.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(ObserverError::ActionPermanentlyFailed {
            reason: format!("Outbound URL must use http:// or https:// scheme: {url}"),
        });
    }

    // Extract the host portion (strip scheme, then take up to the first / : ? #).
    // IPv6 literals use bracket notation [addr]:port — handle separately so we
    // don't split on the `:` inside the brackets and miss the closing `]`.
    let after_scheme = if lower.starts_with("https://") {
        &url[8..]
    } else {
        &url[7..]
    };
    let host = if after_scheme.starts_with('[') {
        // IPv6 bracket notation: extract everything up to and including the `]`.
        after_scheme.find(']').map_or_else(
            || after_scheme.split(['/', '?', '#']).next().unwrap_or(""),
            |end| &after_scheme[..=end],
        )
    } else {
        after_scheme.split(['/', ':', '?', '#']).next().unwrap_or("")
    };

    if is_ssrf_blocked_host_obs(host) {
        return Err(ObserverError::ActionPermanentlyFailed {
            reason: format!(
                "Outbound URL targets a private/loopback address (SSRF protection): {url}"
            ),
        });
    }

    Ok(())
}

/// Names whose values are likely to carry secrets and so must be masked
/// before any debug-level logging.  Case-insensitive substring match —
/// catches `Authorization`, `X-API-Key`, `x-api-key`, `Cookie`,
/// `X-Auth-Secret`, etc.  Intentionally broad: false-positives (masking a
/// non-sensitive header) are acceptable; false-negatives (printing a real
/// bearer token) are not (#346).
const SECRET_HEADER_NEEDLES: &[&str] = &["authorization", "api-key", "cookie", "secret", "token"];

/// Returns a copy of `headers` with the values of any name matching
/// [`SECRET_HEADER_NEEDLES`] (case-insensitive substring) replaced with
/// `"<redacted>"`.  Used by the webhook dispatch debug-logging path so
/// bearer tokens and API keys do not leak to log aggregation.
pub(crate) fn redact_secret_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let lower = name.to_ascii_lowercase();
            let masked = SECRET_HEADER_NEEDLES.iter().any(|needle| lower.contains(needle));
            let safe_value = if masked {
                "<redacted>".to_owned()
            } else {
                value.clone()
            };
            (name.clone(), safe_value)
        })
        .collect()
}

/// Extracts the host portion of a URL for INFO-level logging.  Strips
/// scheme, userinfo, port, path, query, and fragment so the INFO line
/// carries only the destination host — not embedded credentials or
/// per-request query-string secrets (#346).  Falls back to `"<invalid>"`
/// when the URL cannot be parsed by `reqwest::Url`.
pub(crate) fn url_host_only(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "<invalid>".to_owned())
}

/// Returns `true` for hostnames and literal IPs that are blocked as SSRF targets.
fn is_ssrf_blocked_host_obs(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || lower == "::1" || lower == "[::1]" {
        return true;
    }

    // Literal IPv4
    if let Ok(addr) = host.parse::<std::net::Ipv4Addr>() {
        return addr.is_loopback()    // 127.0.0.0/8
            || addr.is_private()     // 10/8, 172.16/12, 192.168/16
            || addr.is_link_local()  // 169.254/16
            || is_cgnat_v4_obs(addr); // 100.64/10
    }

    // Literal IPv6 (strip optional brackets)
    let ipv6 = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(addr) = ipv6.parse::<std::net::Ipv6Addr>() {
        return addr.is_loopback()       // ::1
            || addr.is_unspecified()    // ::
            || is_ula_v6_obs(addr); // fc00::/7
    }

    false
}

/// Returns `true` for CGNAT range 100.64.0.0/10.
const fn is_cgnat_v4_obs(addr: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = addr.octets();
    a == 100 && (b & 0xC0) == 64
}

/// Returns `true` for ULA range `fc00::/7`.
const fn is_ula_v6_obs(addr: std::net::Ipv6Addr) -> bool {
    (addr.segments()[0] & 0xFE00) == 0xFC00
}

/// Validate that no header name or value contains HTTP header injection characters.
///
/// HTTP/1.1 forbids:
/// - `\r` and `\n` inside field names and values (RFC 7230 §3.2, header injection)
/// - `\0` (NUL byte) which can truncate strings in C-based HTTP stacks
/// - `:` in header *names* (RFC 7230 §3.2 token rule; colons are the name/value separator)
///
/// An attacker who can supply custom observer headers could otherwise inject
/// arbitrary headers or corrupt the HTTP request.
///
/// # Errors
///
/// Returns `ObserverError::ActionPermanentlyFailed` if any name or value
/// contains a disallowed byte.
pub(crate) fn validate_headers(headers: &HashMap<String, String>) -> Result<()> {
    for (name, value) in headers {
        if name.contains('\r') || name.contains('\n') || name.contains('\0') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header name — contains CR/LF/NUL (header injection): {name:?}"
                ),
            });
        }
        if name.contains(':') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header name — contains colon (name/value separator): {name:?}"
                ),
            });
        }
        if value.contains('\r') || value.contains('\n') || value.contains('\0') {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: format!(
                    "Invalid webhook header value for '{name}' — contains CR/LF/NUL (header injection)"
                ),
            });
        }
    }
    Ok(())
}

/// Map an HTTP response status to a `WebhookResponse` (success) or the
/// appropriate `ObserverError` variant (failure).
///
/// - 2xx → `Ok(WebhookResponse { success: true, … })`
/// - 4xx except 429 → `Err(ActionPermanentlyFailed)` — retrying will not help
/// - 429, 5xx, other → `Err(ActionExecutionFailed)` — transient, eligible for retry
///
/// 429 Too Many Requests is transient because the server indicated it *can*
/// accept the request after a retry window; routing it to DLQ immediately would
/// discard actionable work.
pub(crate) fn classify_http_status(
    status: reqwest::StatusCode,
    duration_ms: f64,
) -> Result<WebhookResponse> {
    if status.is_success() {
        Ok(WebhookResponse {
            status_code: status.as_u16(),
            success: true,
            duration_ms,
        })
    } else if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        // 4xx (except 429): permanent failure — retrying will not help; route directly to DLQ.
        Err(ObserverError::ActionPermanentlyFailed {
            reason: format!("HTTP {status} (client error — will not retry)"),
        })
    } else {
        // 5xx / 429 / other: transient failure — eligible for retry backoff.
        Err(ObserverError::ActionExecutionFailed {
            reason: format!("HTTP {status} response"),
        })
    }
}

/// Webhook action executor
pub struct WebhookAction {
    /// HTTP client for making requests
    client: Client,
}

impl WebhookAction {
    /// Create a new webhook action executor with the default 30-second timeout.
    #[must_use]
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_WEBHOOK_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to build HTTP client for WebhookAction; falling back to no-timeout client");
                Client::default()
            });
        Self { client }
    }

    /// Create a webhook action executor with a custom request timeout.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to build HTTP client for WebhookAction; falling back to no-timeout client");
                Client::default()
            });
        Self { client }
    }

    /// Execute webhook action
    ///
    /// # Errors
    ///
    /// Returns `ObserverError` if the HTTP request fails or the response is not 2xx.
    #[allow(clippy::cognitive_complexity)] // Reason: sequential HTTP request/response handling with logging and error classification
    pub async fn execute(
        &self,
        url: &str,
        headers: &HashMap<String, String>,
        body_template: Option<&str>,
        signing_secret: Option<&str>,
        event: &EntityEvent,
    ) -> Result<WebhookResponse> {
        let start = std::time::Instant::now();

        // INFO emits delivery METADATA only — host, event id, dispatch lifecycle.
        // Full URL (may carry credentials in query string), headers (may carry
        // bearer tokens / API keys), and body (may carry PII rows) are demoted
        // to DEBUG and TRACE so the default INFO log level no longer leaks
        // secrets or PII into log aggregation (#346).  Headers are redacted
        // even at DEBUG: bearer-style values are replaced with `<redacted>`.
        let host = url_host_only(url);
        let event_id = event.id;
        debug!(
            "WebhookAction.execute() called: host={host}, event_id={event_id}, body_template_present={}",
            body_template.is_some()
        );
        debug!("  URL (full): {}", url);
        debug!("  Headers (redacted): {:?}", redact_secret_headers(headers));
        debug!("  Body template: {:?}", body_template);

        // SECURITY: Reject URLs that target private/loopback addresses (SSRF protection).
        validate_outbound_url(url)?;
        // SECURITY: DNS rebinding prevention — resolve and reject private IPs.
        crate::ssrf::dns_resolve_and_check(url).await?;
        // SECURITY: Reject headers that contain CR/LF to prevent header injection.
        validate_headers(headers)?;

        // Prepare request body
        let body = if let Some(template) = body_template {
            // Simple template substitution: replace {{ field }} with event.data[field]
            self.render_body_template(template, &event.data)?
        } else {
            // Default: send the event as JSON
            event.data.clone()
        };

        // Rendered body MAY contain PII (entity rows).  Reserved for TRACE so a
        // production INFO log stream never carries customer data even from a
        // valid event.
        tracing::trace!(
            event_id = %event_id,
            host = %host,
            "  Body: {}",
            serde_json::to_string(&body).unwrap_or_else(|_| "<invalid json>".to_string())
        );

        // Serialize the body ONCE so the signature (below) covers the EXACT
        // bytes sent on the wire. `.json()` re-serializes and could diverge
        // (key order / whitespace), making every external signature
        // verification fail (#345).
        let body_bytes =
            serde_json::to_vec(&body).map_err(|e| ObserverError::ActionExecutionFailed {
                reason: format!("Failed to serialize webhook body: {e}"),
            })?;

        // Build request
        let mut request = self.client.post(url);

        // Add headers (already validated above), tracking whether the operator
        // set a Content-Type so we don't duplicate it.
        let mut has_content_type = false;
        for (key, value) in headers {
            if key.eq_ignore_ascii_case("content-type") {
                has_content_type = true;
            }
            request = request.header(key, value);
        }
        if !has_content_type {
            request = request.header(reqwest::header::CONTENT_TYPE, "application/json");
        }

        // Sign the exact transmitted bytes if a signing secret is configured.
        if let Some(secret) = signing_secret {
            let ts = chrono::Utc::now().timestamp();
            let signature = webhook_signature(secret, ts, &body_bytes);
            request = request.header(WEBHOOK_SIGNATURE_HEADER, signature);
        }

        info!(
            action_type = "webhook",
            event_id = %event_id,
            host = %host,
            "Webhook dispatch starting"
        );

        // Send request — `.body(body_bytes)` sends the exact bytes we signed.
        let response = request.body(body_bytes).send().await.map_err(|e| {
            ObserverError::ActionExecutionFailed {
                reason: format!("HTTP request failed: {e}"),
            }
        })?;

        let status = response.status();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        info!(
            action_type = "webhook",
            event_id = %event_id,
            host = %host,
            status_code = status.as_u16(),
            duration_ms = duration_ms,
            "Webhook dispatch complete"
        );

        classify_http_status(status, duration_ms)
    }

    #[allow(clippy::unused_self)] // Reason: method is part of a public API / trait consistency
    pub(crate) fn render_body_template(&self, template: &str, data: &Value) -> Result<Value> {
        let mut rendered = template.to_string();

        if let Value::Object(map) = data {
            for (key, value) in map {
                let placeholder = format!("{{{{ {key} }}}}");
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                rendered = rendered.replace(&placeholder, &value_str);
            }
        }

        serde_json::from_str(&rendered).or(Ok(Value::String(rendered)))
    }
}

impl Default for WebhookAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from webhook execution
#[derive(Debug, Clone)]
pub struct WebhookResponse {
    /// HTTP status code
    pub status_code: u16,
    /// Whether execution was successful
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Slack action executor
pub struct SlackAction {
    /// HTTP client for making requests
    client: Client,
}

impl SlackAction {
    /// Create a new Slack action executor
    #[must_use]
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(DEFAULT_WEBHOOK_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "Failed to build HTTP client for SlackAction; falling back to no-timeout client");
                Client::default()
            });
        Self { client }
    }

    /// Execute Slack action
    ///
    /// # Errors
    ///
    /// Returns `ObserverError` if the webhook URL is invalid, the HTTP request fails,
    /// or the Slack API returns a non-success response.
    pub async fn execute(
        &self,
        webhook_url: &str,
        channel: Option<&str>,
        message_template: Option<&str>,
        event: &EntityEvent,
    ) -> Result<SlackResponse> {
        let start = std::time::Instant::now();

        // Prepare message
        let message = if let Some(template) = message_template {
            self.render_message_template(template, &event.data)
        } else {
            format!(
                "Event: {} on {} (ID: {})",
                event.event_type.as_str(),
                event.entity_type,
                event.entity_id
            )
        };

        // Build Slack payload
        let mut payload = json!({
            "text": message,
            "type": "mrkdwn"
        });

        if let Some(ch) = channel {
            payload["channel"] = Value::String(ch.to_string());
        }

        // SECURITY: Reject URLs that target private/loopback addresses (SSRF protection).
        validate_outbound_url(webhook_url)?;
        // SECURITY: DNS rebinding prevention — resolve and reject private IPs.
        crate::ssrf::dns_resolve_and_check(webhook_url).await?;

        // Send request
        let response = self.client.post(webhook_url).json(&payload).send().await.map_err(|e| {
            ObserverError::ActionExecutionFailed {
                reason: format!("Slack webhook failed: {e}"),
            }
        })?;

        let status = response.status();
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;

        if status.is_success() {
            Ok(SlackResponse {
                status_code: status.as_u16(),
                success: true,
                duration_ms,
            })
        } else if status.is_client_error() {
            Err(ObserverError::ActionPermanentlyFailed {
                reason: format!("Slack HTTP {status} (client error — will not retry)"),
            })
        } else {
            Err(ObserverError::ActionExecutionFailed {
                reason: format!("Slack HTTP {status} response"),
            })
        }
    }

    #[allow(clippy::unused_self)] // Reason: method is part of a public API / trait consistency
    pub(crate) fn render_message_template(&self, template: &str, data: &Value) -> String {
        let mut rendered = template.to_string();

        if let Value::Object(map) = data {
            for (key, value) in map {
                let placeholder = format!("{{{{ {key} }}}}");
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                rendered = rendered.replace(&placeholder, &value_str);
            }
        }

        rendered
    }
}

impl Default for SlackAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from Slack execution
#[derive(Debug, Clone)]
pub struct SlackResponse {
    /// HTTP status code
    pub status_code: u16,
    /// Whether execution was successful
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Email action executor
pub struct EmailAction {
    // Placeholder for SMTP client
}

impl EmailAction {
    /// Create a new email action executor
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }

    /// Execute email action (stub)
    ///
    /// # Errors
    ///
    /// Returns `ObserverError` if the email delivery fails.
    #[allow(clippy::unused_async)] // Reason: trait/interface requires async signature
    pub async fn execute(
        &self,
        _to: &str,
        _subject: &str,
        _body_template: Option<&str>,
        _event: &EntityEvent,
    ) -> Result<EmailResponse> {
        // Stub implementation
        Ok(EmailResponse {
            success:     true,
            message_id:  Some(uuid::Uuid::new_v4().to_string()),
            duration_ms: 10.0,
        })
    }
}

impl Default for EmailAction {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from email execution
#[derive(Debug, Clone)]
pub struct EmailResponse {
    /// Whether execution was successful
    pub success:     bool,
    /// Message ID from provider (if available)
    pub message_id:  Option<String>,
    /// Duration in milliseconds
    pub duration_ms: f64,
}

/// Generic action execution result
#[derive(Debug, Clone)]
pub struct ActionExecutionResult {
    /// Action type (webhook, slack, email, etc.)
    pub action_type: String,
    /// Success status
    pub success:     bool,
    /// Duration in milliseconds
    pub duration_ms: f64,
    /// Optional message ID or tracking info
    pub tracking_id: Option<String>,
}
