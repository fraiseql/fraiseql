//! Action dispatcher trait and production implementation.
//!
//! Provides the `ActionDispatcher` seam that enables unit-testing of retry
//! logic and failure policies without making real network calls.  Production
//! code always uses `DefaultActionDispatcher`; tests inject `MockActionDispatcher`.

use std::sync::Arc;

use tracing::debug;

use crate::{
    actions::{EmailAction, SlackAction, WebhookAction},
    actions_additional::{CacheAction, PushAction, SearchAction, SmsAction},
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
pub(super) struct DefaultActionDispatcher {
    /// Webhook action executor
    pub(super) webhook_action: Arc<WebhookAction>,
    /// Slack action executor
    pub(super) slack_action:   Arc<SlackAction>,
    /// Email action executor
    pub(super) email_action:   Arc<EmailAction>,
    /// SMS action executor
    pub(super) sms_action:     Arc<SmsAction>,
    /// Push notification action executor
    pub(super) push_action:    Arc<PushAction>,
    /// Search index action executor
    pub(super) search_action:  Arc<SearchAction>,
    /// Cache action executor
    pub(super) cache_action:   Arc<CacheAction>,
}

/// Resolve a URL from an explicit value or an environment variable.
///
/// Returns `Ok(url)` if `explicit` is set, or falls back to reading `env_var`.
/// Returns `Err(ObserverError::InvalidActionConfig)` if neither is set or the
/// env var is missing.
pub(super) fn resolve_url(
    explicit: Option<&str>,
    env_var: Option<&str>,
    action_name: &str,
) -> Result<String> {
    if let Some(u) = explicit {
        return Ok(u.to_owned());
    }
    if let Some(var_name) = env_var {
        return std::env::var(var_name).map_err(|_| ObserverError::InvalidActionConfig {
            reason: format!("{action_name} URL env var {var_name} not found"),
        });
    }
    Err(ObserverError::InvalidActionConfig {
        reason: format!("{action_name} URL not provided"),
    })
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
                } => {
                    debug!("Webhook action: url={:?}, url_env={:?}", url, url_env);
                    let webhook_url = resolve_url(url.as_deref(), url_env.as_deref(), "Webhook")?;

                    match self
                        .webhook_action
                        .execute(&webhook_url, headers, body_template.as_deref(), event)
                        .await
                    {
                        Ok(response) => Ok(ActionResult {
                            action_type: "webhook".to_string(),
                            success:     true,
                            message:     format!("HTTP {}", response.status_code),
                            duration_ms: response.duration_ms,
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
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Sms {
                    phone,
                    phone_template: _,
                    message_template,
                } => {
                    let sms_phone = phone.as_ref().ok_or(ObserverError::InvalidActionConfig {
                        reason: "SMS 'phone' not provided".to_string(),
                    })?;

                    match self.sms_action.execute(
                        sms_phone.clone(),
                        message_template.as_deref(),
                        event,
                    ) {
                        Ok(response) => Ok(ActionResult {
                            action_type: "sms".to_string(),
                            success:     response.success,
                            message:     response.message_id.unwrap_or_else(|| "sent".to_string()),
                            duration_ms: response.duration_ms,
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Push {
                    device_token,
                    title_template,
                    body_template,
                } => {
                    let token =
                        device_token.as_ref().ok_or(ObserverError::InvalidActionConfig {
                            reason: "Push 'device_token' not provided".to_string(),
                        })?;

                    let title =
                        title_template.as_ref().ok_or(ObserverError::InvalidActionConfig {
                            reason: "Push 'title_template' not provided".to_string(),
                        })?;

                    let body =
                        body_template.as_ref().ok_or(ObserverError::InvalidActionConfig {
                            reason: "Push 'body_template' not provided".to_string(),
                        })?;

                    match self.push_action.execute(token.clone(), title.clone(), body.clone()) {
                        Ok(response) => Ok(ActionResult {
                            action_type: "push".to_string(),
                            success:     response.success,
                            message:     response
                                .notification_id
                                .unwrap_or_else(|| "sent".to_string()),
                            duration_ms: response.duration_ms,
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Search { index, id_template } => {
                    match self.search_action.execute(index.clone(), id_template.as_deref(), event) {
                        Ok(response) => Ok(ActionResult {
                            action_type: "search".to_string(),
                            success:     response.success,
                            message:     if response.indexed {
                                "indexed".to_string()
                            } else {
                                "not_indexed".to_string()
                            },
                            duration_ms: response.duration_ms,
                        }),
                        Err(e) => Err(e),
                    }
                },
                ActionConfig::Cache {
                    key_pattern,
                    action,
                } => match self.cache_action.execute(key_pattern.clone(), action) {
                    Ok(response) => Ok(ActionResult {
                        action_type: "cache".to_string(),
                        success:     response.success,
                        message:     format!("affected: {}", response.keys_affected),
                        duration_ms: response.duration_ms,
                    }),
                    Err(e) => Err(e),
                },
            }
        })
    }
}
