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

    validate_url_ssrf(&url)?;
    Ok(url)
}

/// Validate a URL against Server-Side Request Forgery (SSRF) attack vectors.
///
/// Rejects:
/// - Non-http/https schemes
/// - URLs exceeding [`MAX_WEBHOOK_URL_LEN`] bytes
/// - URLs with no host
/// - `localhost` / `*.localhost` hostnames
/// - Literal IP addresses that fall inside private, loopback, link-local,
///   shared-address-space, or IPv4-mapped IPv6 ranges
///
/// # Note
///
/// DNS-based SSRF (where a public hostname resolves to a private IP) requires
/// a connect-time check at the HTTP client level and is beyond the scope of
/// this static validation.
fn validate_url_ssrf(url: &str) -> Result<()> {
    if url.len() > MAX_WEBHOOK_URL_LEN {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!(
                "Webhook URL too long ({} bytes, max {MAX_WEBHOOK_URL_LEN})",
                url.len()
            ),
        });
    }

    // Require http or https scheme.
    let rest = if let Some(r) = url.strip_prefix("https://") {
        r
    } else if let Some(r) = url.strip_prefix("http://") {
        r
    } else {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!("Webhook URL must use http:// or https:// scheme (got: {url})"),
        });
    };

    // Extract the host, handling IPv6 bracket notation ([::1]:port or [::1]).
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        // IPv6 literal: strip surrounding brackets and any trailing :port.
        authority
            .split(']')
            .next()
            .unwrap_or("")
            .trim_start_matches('[')
    } else {
        // IPv4 or hostname: strip optional :port.
        authority.split(':').next().unwrap_or("")
    };

    if host.is_empty() {
        return Err(ObserverError::InvalidActionConfig {
            reason: "Webhook URL has no host".to_string(),
        });
    }

    // Block loopback hostnames before attempting IP parsing.
    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(ObserverError::InvalidActionConfig {
            reason: format!("Webhook URL targets a loopback host: {host}"),
        });
    }

    // If the host is a literal IP address, block private / reserved ranges.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_ssrf_blocked_ip(&ip) {
            return Err(ObserverError::InvalidActionConfig {
                reason: format!("Webhook URL targets a private or reserved IP address: {ip}"),
            });
        }
    }

    Ok(())
}

/// Returns `true` for IP addresses that outbound webhooks must never contact.
///
/// Covers: loopback (127/8, ::1), RFC 1918 private (10/8, 172.16/12, 192.168/16),
/// link-local / APIPA (169.254/16, fe80::/10), shared address space (100.64/10,
/// RFC 6598), IPv4-mapped IPv6 (::ffff:0:0/96), and ULA (fc00::/7).
fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                                // 127.0.0.0/8  loopback
            || o[0] == 10                                              // 10.0.0.0/8   RFC 1918
            || (o[0] == 172 && (16..=31).contains(&o[1]))             // 172.16.0.0/12 RFC 1918
            || (o[0] == 192 && o[1] == 168)                           // 192.168.0.0/16 RFC 1918
            || (o[0] == 169 && o[1] == 254)                           // 169.254.0.0/16 link-local
            || (o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000)  // 100.64.0.0/10 RFC 6598
            || o[0] == 0                                               // 0.0.0.0/8 this-network
        },
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            *v6 == std::net::Ipv6Addr::LOCALHOST                      // ::1 loopback
            || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0
                && s[4] == 0 && s[5] == 0xffff)                      // ::ffff:0:0/96 IPv4-mapped
            || (s[0] & 0xfe00) == 0xfc00                             // fc00::/7  ULA
            || (s[0] & 0xffc0) == 0xfe80                             // fe80::/10 link-local
        },
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
                        .execute(
                            &slack_url,
                            channel.as_deref(),
                            message_template.as_deref(),
                            event,
                        )
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

                    match self
                        .sms_action
                        .execute(sms_phone.clone(), message_template.as_deref(), event)
                    {
                        Ok(response) => Ok(ActionResult {
                            action_type: "sms".to_string(),
                            success:     response.success,
                            message:     response
                                .message_id
                                .unwrap_or_else(|| "sent".to_string()),
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
                    match self
                        .search_action
                        .execute(index.clone(), id_template.as_deref(), event)
                    {
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code — panics are intentional

    use super::*;

    #[test]
    fn test_valid_urls_pass() {
        assert!(resolve_url(Some("https://hooks.example.com/notify"), None, "Test").is_ok());
        assert!(resolve_url(Some("http://api.example.org/webhook"), None, "Test").is_ok());
    }

    #[test]
    fn test_invalid_scheme_rejected() {
        assert!(resolve_url(Some("ftp://example.com/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("file:///etc/passwd"), None, "Test").is_err());
    }

    #[test]
    fn test_localhost_rejected() {
        assert!(resolve_url(Some("http://localhost/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://localhost.localdomain/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("https://subdomain.localhost/hook"), None, "Test").is_err());
    }

    #[test]
    fn test_private_ipv4_rejected() {
        // RFC 1918
        assert!(resolve_url(Some("http://10.0.0.1/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://172.16.0.1/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://172.31.255.255/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://192.168.1.1/hook"), None, "Test").is_err());
        // Loopback
        assert!(resolve_url(Some("http://127.0.0.1/hook"), None, "Test").is_err());
        // Link-local / AWS IMDS
        assert!(
            resolve_url(Some("http://169.254.169.254/latest/meta-data/"), None, "Test").is_err()
        );
    }

    #[test]
    fn test_private_ipv6_rejected() {
        assert!(resolve_url(Some("http://[::1]/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://[fc00::1]/hook"), None, "Test").is_err());
        assert!(resolve_url(Some("http://[fe80::1]/hook"), None, "Test").is_err());
    }

    #[test]
    fn test_no_url_provided_error() {
        assert!(resolve_url(None, None, "Test").is_err());
    }

    #[test]
    fn test_url_too_long_rejected() {
        let long_url = format!("https://example.com/{}", "a".repeat(2_100));
        assert!(resolve_url(Some(&long_url), None, "Test").is_err());
    }
}
