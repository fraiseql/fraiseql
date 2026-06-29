//! Action dispatcher trait and production implementation.
//!
//! Provides the `ActionDispatcher` seam that enables unit-testing of retry
//! logic and failure policies without making real network calls.  Production
//! code always uses `DefaultActionDispatcher`; tests inject `MockActionDispatcher`.

use std::sync::Arc;

use tracing::debug;

use crate::{
    actions::{EmailAction, SlackAction, WebhookAction},
    config::ActionConfig,
    error::{ObserverError, Result},
    event::EntityEvent,
    traits::ActionResult,
};

/// Internal trait for dispatching actions to their concrete implementations.
///
/// This seam exists solely to enable unit-testing of retry logic and failure
/// policies without making real network calls. Production code always uses
/// `DefaultActionDispatcher`; tests inject `MockActionDispatcher`.
pub trait ActionDispatcher: Send + Sync {
    /// Dispatch a single action and return its result.
    fn dispatch<'a>(
        &'a self,
        action: &'a ActionConfig,
        event: &'a EntityEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>>;
}

/// Production action dispatcher that delegates to the concrete action structs.
///
/// Webhook / Slack / Email / Cache (the last only with the `caching` feature and
/// a wired Redis invalidator) have real transports. SMS / Push / Search remain
/// rejected as unsupported (H24), so they have no executor.
#[allow(clippy::struct_field_names)] // Reason: `_action` postfix clarifies executor vs config fields
pub(super) struct DefaultActionDispatcher {
    /// Webhook action executor
    pub(super) webhook_action:    Arc<WebhookAction>,
    /// Slack action executor
    pub(super) slack_action:      Arc<SlackAction>,
    /// Email action executor
    pub(super) email_action:      Arc<EmailAction>,
    /// Redis cache-invalidation transport (#428).
    ///
    /// `None` means no Redis backend was wired: a `cache` action then fails loud
    /// (permanent) rather than silently no-opping, exactly like an email action
    /// with no SMTP backend.
    #[cfg(feature = "caching")]
    pub(super) cache_invalidator: Option<Arc<crate::cache::redis::RedisCacheInvalidator>>,
}

/// Maximum byte length accepted for a webhook URL.
const MAX_WEBHOOK_URL_LEN: usize = 2_048;

/// Resolve a URL from an explicit value or an environment variable, then
/// validate it against SSRF attack vectors.
///
/// Returns `Ok(url)` if `explicit` is set, or falls back to reading `env_var`.
/// Returns `Err(ObserverError::InvalidActionConfig)` if neither is set, the
/// env var is missing, or the URL fails SSRF validation.
pub(super) fn resolve_url(
    explicit: Option<&str>,
    env_var: Option<&str>,
    action_name: &str,
) -> Result<String> {
    let url = if let Some(u) = explicit {
        u.to_owned()
    } else if let Some(var_name) = env_var {
        std::env::var(var_name).map_err(|_| ObserverError::InvalidActionConfig {
            reason: format!("{action_name} URL env var {var_name} not found"),
        })?
    } else {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!("{action_name} URL not provided"),
        });
    };

    // Webhook-specific abuse guard: cap URL length before SSRF validation.
    if url.len() > MAX_WEBHOOK_URL_LEN {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!(
                "Webhook URL too long ({} bytes, max {MAX_WEBHOOK_URL_LEN})",
                url.len()
            ),
        });
    }

    // Webhook transport accepts only http/https. The canonical SSRF guard is
    // scheme-agnostic (it is shared with the NATS validator), so the http/https
    // requirement is enforced here, at the webhook call site.
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!("{action_name} URL must use http:// or https:// scheme (got: {url})"),
        });
    }

    // Static SSRF validation via the canonical guard (host/loopback/literal-IP).
    // DNS-rebinding is handled at dispatch time via `dns_resolve_and_check`.
    // The canonical guard surfaces `InvalidConfig`; remap to `InvalidActionConfig`
    // so the dispatch path keeps its action-config error contract.
    crate::ssrf::validate_outbound_url(&url).map_err(|e| ObserverError::InvalidActionConfig {
        reason: e.to_string(),
    })?;
    Ok(url)
}

/// Resolve the webhook HMAC signing secret from either a per-subscription
/// literal or the name of a process environment variable.
///
/// `literal` is the `signing_secret` field (used by DB-backed / admin-managed
/// observers, #467); `env_var` is the `signing_secret_env` name (static/config
/// model, #345). They are mutually exclusive:
///
/// - both set → `Err` (ambiguous config; fail loud, the house style);
/// - only `literal` set → that literal (empty → `Err`);
/// - only `env_var` set → the named env var's value (absent/empty → `Err`);
/// - neither set → `Ok(None)` (signing not configured).
///
/// An operator who asked for signing must never get an unsigned delivery
/// silently — every misconfiguration is an error, not a silent skip.
pub(super) fn resolve_signing_secret(
    env_var: Option<&str>,
    literal: Option<&str>,
) -> Result<Option<String>> {
    match (literal, env_var) {
        (Some(_), Some(_)) => Err(ObserverError::InvalidActionConfig {
            reason: "Webhook action sets both `signing_secret` and `signing_secret_env`; \
                     set exactly one"
                .to_string(),
        }),
        (Some(literal), None) => {
            if literal.is_empty() {
                return Err(ObserverError::InvalidActionConfig {
                    reason: "Webhook `signing_secret` is set but empty".to_string(),
                });
            }
            Ok(Some(literal.to_string()))
        },
        (None, Some(var_name)) => {
            let secret =
                std::env::var(var_name).map_err(|_| ObserverError::InvalidActionConfig {
                    reason: format!("Webhook signing secret env var {var_name} not found"),
                })?;
            if secret.is_empty() {
                return Err(ObserverError::InvalidActionConfig {
                    reason: format!("Webhook signing secret env var {var_name} is empty"),
                });
            }
            Ok(Some(secret))
        },
        (None, None) => Ok(None),
    }
}

impl ActionDispatcher for DefaultActionDispatcher {
    fn dispatch<'a>(
        &'a self,
        action: &'a ActionConfig,
        event: &'a EntityEvent,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ActionResult>> + Send + 'a>>
    {
        Box::pin(async move {
            debug!("Executing action: {} for event {}", action.action_type(), event.id);

            match action {
                ActionConfig::Webhook {
                    url,
                    url_env,
                    headers,
                    body_template,
                    signing_secret,
                    signing_secret_env,
                } => {
                    debug!("Webhook action: url={:?}, url_env={:?}", url, url_env);
                    let webhook_url = resolve_url(url.as_deref(), url_env.as_deref(), "Webhook")?;
                    // DNS-rebinding guard: re-resolve at dispatch time and reject
                    // any host whose addresses fall in a private/reserved range.
                    crate::ssrf::dns_resolve_and_check(&webhook_url).await?;
                    let resolved_secret = resolve_signing_secret(
                        signing_secret_env.as_deref(),
                        signing_secret.as_deref(),
                    )?;

                    match self
                        .webhook_action
                        .execute(
                            &webhook_url,
                            headers,
                            body_template.as_deref(),
                            resolved_secret.as_deref(),
                            event,
                        )
                        .await
                    {
                        Ok(response) => Ok(ActionResult {
                            action_type: "webhook".to_string(),
                            success:     true,
                            message:     format!("HTTP {}", response.status_code),
                            duration_ms: response.duration_ms,
                            status_code: Some(response.status_code),
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Slack {
                    webhook_url,
                    webhook_url_env,
                    channel,
                    message_template,
                } => {
                    let slack_url =
                        resolve_url(webhook_url.as_deref(), webhook_url_env.as_deref(), "Slack")?;
                    // DNS-rebinding guard: re-resolve at dispatch time.
                    crate::ssrf::dns_resolve_and_check(&slack_url).await?;

                    match self
                        .slack_action
                        .execute(&slack_url, channel.as_deref(), message_template.as_deref(), event)
                        .await
                    {
                        Ok(response) => Ok(ActionResult {
                            action_type: "slack".to_string(),
                            success:     true,
                            message:     format!("HTTP {}", response.status_code),
                            duration_ms: response.duration_ms,
                            status_code: Some(response.status_code),
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Email {
                    to,
                    to_template: _,
                    subject,
                    subject_template: _,
                    body_template,
                    reply_to: _,
                } => {
                    let email_to = to.as_ref().ok_or(ObserverError::InvalidActionConfig {
                        reason: "Email 'to' not provided".to_string(),
                    })?;

                    let email_subject =
                        subject.as_ref().ok_or(ObserverError::InvalidActionConfig {
                            reason: "Email 'subject' not provided".to_string(),
                        })?;

                    match self
                        .email_action
                        .execute(email_to, email_subject, body_template.as_deref(), event)
                        .await
                    {
                        Ok(response) => Ok(ActionResult {
                            action_type: "email".to_string(),
                            success:     response.success,
                            message:     response
                                .message_id
                                .unwrap_or_else(|| "queued".to_string()),
                            duration_ms: response.duration_ms,
                            status_code: None,
                        }),
                        Err(e) => Err(e),
                    }
                },
                // Cache invalidation has a real Redis transport (#428) when the
                // `caching` feature is compiled and an invalidator is wired;
                // otherwise it fails loud (never a fabricated success).
                ActionConfig::Cache {
                    key_pattern,
                    action: cache_action,
                } => self.dispatch_cache(key_pattern, cache_action, event).await,
                // SMS / Push / Search have no real transport wired. They
                // previously delegated to stub actions that fabricated
                // `success: true` and sent nothing (H24). They now fail loud here
                // too (belt-and-suspenders with `ActionConfig::validate`, which
                // rejects them at config-load). Real transports are tracked as
                // follow-up work.
                ActionConfig::Sms { .. }
                | ActionConfig::Push { .. }
                | ActionConfig::Search { .. } => Err(ObserverError::UnsupportedActionType {
                    action_type: action.action_type().to_string(),
                }),
            }
        })
    }
}

impl DefaultActionDispatcher {
    /// Dispatch a `cache` action.
    ///
    /// Only `action = "invalidate"` is implemented; `"refresh"` (and any other
    /// value) fails loud. With the `caching` feature and a wired Redis
    /// invalidator, the keys described by `key_pattern` are removed for real;
    /// without a wired invalidator (or without the feature) the action fails loud
    /// (permanent) so a non-functional cache integration is never silent.
    #[cfg(feature = "caching")]
    async fn dispatch_cache(
        &self,
        key_pattern: &str,
        cache_action: &str,
        event: &EntityEvent,
    ) -> Result<ActionResult> {
        if cache_action != "invalidate" {
            return Err(ObserverError::InvalidActionConfig {
                reason: format!(
                    "Cache action {cache_action:?} is not supported; only \"invalidate\" is \
                     implemented (#428)"
                ),
            });
        }

        let Some(invalidator) = self.cache_invalidator.as_ref() else {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: "Cache action has no Redis backend configured (#428): set \
                         [observers.runtime.redis] and build the executor with a cache invalidator"
                    .to_string(),
            });
        };

        let start = std::time::Instant::now();
        let removed = invalidator.invalidate(key_pattern, event).await?;
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        Ok(ActionResult {
            action_type: "cache".to_string(),
            success: true,
            message: format!("invalidated {removed} key(s)"),
            duration_ms,
            status_code: None,
        })
    }

    /// Dispatch a `cache` action when the `caching` feature is not compiled:
    /// there is no Redis transport, so the action always fails loud.
    #[cfg(not(feature = "caching"))]
    #[allow(clippy::unused_self, clippy::unused_async)] // Reason: mirrors the `caching` async signature so the call site is feature-agnostic
    async fn dispatch_cache(
        &self,
        _key_pattern: &str,
        _cache_action: &str,
        _event: &EntityEvent,
    ) -> Result<ActionResult> {
        Err(ObserverError::UnsupportedActionType {
            action_type: "cache".to_string(),
        })
    }
}
