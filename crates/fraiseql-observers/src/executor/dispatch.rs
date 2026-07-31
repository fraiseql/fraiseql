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
/// Webhook / Slack / Email / Cache (the last only with the `caching` feature
/// and a wired Redis invalidator) / Database (#632, with a wired pool) / Log
/// (#632) have real transports. SMS / Push / Search remain rejected as
/// unsupported (H24 / #428), so they have no executor — failing loud is
/// correct behaviour until a real provider transport is wired.
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
    /// PostgreSQL pool slot for `database` actions (#632).
    ///
    /// A shared `OnceLock` set by [`super::ObserverExecutor::with_database_pool`]
    /// after construction. Unset means no pool was wired: a `database` action
    /// then fails loud (permanent) rather than silently no-opping, exactly like
    /// an email action with no SMTP backend.
    #[cfg(feature = "postgres")]
    pub(super) database_pool:     Arc<std::sync::OnceLock<sqlx::PgPool>>,
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
                    method,
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
                            method.as_deref(),
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
                // #632: real database dispatcher — call a PostgreSQL function
                // with the event envelope; fails loud without a wired pool.
                ActionConfig::Database {
                    function_name,
                    params,
                } => self.dispatch_database(function_name, params.as_ref(), event).await,
                // #632: real log dispatcher — a structured tracing event at the
                // configured level, message rendered from the event data.
                ActionConfig::Log {
                    level,
                    message_template,
                } => Ok(dispatch_log(level, message_template, event)),
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

/// Dispatch a `log` action (#632): emit one structured tracing event at the
/// configured level and report the rendered message.
///
/// The template renders with the same `{{ field }}` substitution as the Slack
/// message template. The level string was validated at config load; an
/// unexpected value (operator-edited `tb_observer.actions`) falls back to
/// `info` rather than dropping the line — the log IS the action's effect.
fn dispatch_log(level: &str, message_template: &str, event: &EntityEvent) -> ActionResult {
    let start = std::time::Instant::now();
    let message = crate::actions::render_text_template(message_template, &event.data);
    match level {
        "trace" => tracing::trace!(
            target: "fraiseql_observers::action::log",
            event_id = %event.id, entity_type = %event.entity_type,
            entity_id = %event.entity_id, event_type = ?event.event_type,
            "{message}"
        ),
        "debug" => tracing::debug!(
            target: "fraiseql_observers::action::log",
            event_id = %event.id, entity_type = %event.entity_type,
            entity_id = %event.entity_id, event_type = ?event.event_type,
            "{message}"
        ),
        "warn" => tracing::warn!(
            target: "fraiseql_observers::action::log",
            event_id = %event.id, entity_type = %event.entity_type,
            entity_id = %event.entity_id, event_type = ?event.event_type,
            "{message}"
        ),
        "error" => tracing::error!(
            target: "fraiseql_observers::action::log",
            event_id = %event.id, entity_type = %event.entity_type,
            entity_id = %event.entity_id, event_type = ?event.event_type,
            "{message}"
        ),
        _ => tracing::info!(
            target: "fraiseql_observers::action::log",
            event_id = %event.id, entity_type = %event.entity_type,
            entity_id = %event.entity_id, event_type = ?event.event_type,
            "{message}"
        ),
    }
    #[allow(clippy::cast_precision_loss)] // Reason: sub-second duration, precision irrelevant
    let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
    ActionResult {
        action_type: "log".to_string(),
        success: true,
        message,
        duration_ms,
        status_code: None,
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

    /// Dispatch a `database` action (#632): call the configured PostgreSQL
    /// function with the event envelope `{"event": ..., "params": ...}`.
    ///
    /// The function name is re-validated as a strict SQL identifier at dispatch
    /// (`tb_observer.actions` is operator-editable production data, so
    /// config-load validation alone is not a boundary), then interpolated;
    /// the envelope is bound as a real `$1` parameter. Fails loud without a
    /// wired pool — never a fabricated success.
    #[cfg(feature = "postgres")]
    async fn dispatch_database(
        &self,
        function_name: &str,
        params: Option<&serde_json::Value>,
        event: &EntityEvent,
    ) -> Result<ActionResult> {
        let identity_check = crate::config::ActionConfig::Database {
            function_name: function_name.to_string(),
            params:        None,
        };
        if identity_check.validate().is_err() {
            return Err(ObserverError::InvalidActionConfig {
                reason: format!(
                    "database action function_name {function_name:?} is not a plain or \
                     schema-qualified SQL identifier"
                ),
            });
        }

        let Some(pool) = self.database_pool.get() else {
            return Err(ObserverError::ActionPermanentlyFailed {
                reason: "Database action has no PostgreSQL pool wired (#632): build the \
                         observer executor with `with_database_pool`"
                    .to_string(),
            });
        };

        let envelope = serde_json::json!({
            "event": event,
            "params": params,
        });

        let start = std::time::Instant::now();
        // Identifier-validated function name; the payload itself is a bound
        // parameter. The function's return value is deliberately discarded.
        sqlx::query(&format!("SELECT {function_name}($1::jsonb)"))
            .bind(&envelope)
            .execute(pool)
            .await
            .map_err(|e| ObserverError::ActionExecutionFailed {
                reason: format!("database action {function_name} failed: {e}"),
            })?;
        let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
        Ok(ActionResult {
            action_type: "database".to_string(),
            success: true,
            message: format!("called {function_name}"),
            duration_ms,
            status_code: None,
        })
    }

    /// Dispatch a `database` action without the `postgres` feature: no driver
    /// exists, so the action always fails loud.
    #[cfg(not(feature = "postgres"))]
    #[allow(clippy::unused_self, clippy::unused_async)] // Reason: mirrors the `postgres` async signature so the call site is feature-agnostic
    async fn dispatch_database(
        &self,
        _function_name: &str,
        _params: Option<&serde_json::Value>,
        _event: &EntityEvent,
    ) -> Result<ActionResult> {
        Err(ObserverError::UnsupportedActionType {
            action_type: "database".to_string(),
        })
    }
}

#[cfg(test)]
mod dispatch_632_tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use uuid::Uuid;

    use super::*;
    use crate::event::EventKind;

    fn test_event() -> EntityEvent {
        EntityEvent::new(
            EventKind::Updated,
            "Order".to_string(),
            Uuid::new_v4(),
            json!({"id": "order-1", "status": "shipped"}),
        )
    }

    /// A minimal collecting subscriber: the log line IS the `log` action's
    /// effect, so the test must observe the emitted tracing event — asserting
    /// only the returned `ActionResult` would re-create the fabricated-success
    /// pattern this phase exists to kill.
    #[derive(Clone)]
    struct Collector {
        events: Arc<Mutex<Vec<(String, String)>>>, // (level, message)
    }

    impl tracing::Subscriber for Collector {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            struct MessageVisitor(Option<String>);
            impl tracing::field::Visit for MessageVisitor {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = Some(format!("{value:?}"));
                    }
                }
            }
            if event.metadata().target() == "fraiseql_observers::action::log" {
                let mut visitor = MessageVisitor(None);
                event.record(&mut visitor);
                self.events
                    .lock()
                    .expect("collector lock")
                    .push((event.metadata().level().to_string(), visitor.0.unwrap_or_default()));
            }
        }

        fn enter(&self, _: &tracing::span::Id) {}

        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn log_action_emits_a_rendered_structured_line() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let collector = Collector {
            events: Arc::clone(&events),
        };

        let result = tracing::subscriber::with_default(collector, || {
            dispatch_log("warn", "order {{ id }} is now {{ status }}", &test_event())
        });

        assert!(result.success, "log dispatch reports success");
        assert_eq!(result.action_type, "log");
        assert_eq!(result.message, "order order-1 is now shipped", "template must render");

        let captured = events.lock().expect("collector lock");
        assert_eq!(
            captured.len(),
            1,
            "#632: exactly one tracing event must actually be emitted — the line is the effect"
        );
        let (level, message) = &captured[0];
        assert_eq!(level, "WARN", "configured level must be honoured");
        assert_eq!(message, "order order-1 is now shipped");
    }

    #[test]
    fn log_action_unknown_level_falls_back_to_info_not_dropped() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let collector = Collector {
            events: Arc::clone(&events),
        };

        tracing::subscriber::with_default(collector, || {
            dispatch_log("verbose", "m", &test_event());
        });

        let captured = events.lock().expect("collector lock");
        assert_eq!(captured.len(), 1, "an unexpected level must not drop the line");
        assert_eq!(captured[0].0, "INFO");
    }

    /// #632 fail-loud: a `database` action with no wired pool must error, never
    /// fabricate success.
    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn database_action_without_pool_fails_loud() {
        let dispatcher = DefaultActionDispatcher {
            webhook_action: Arc::new(crate::actions::WebhookAction::new()),
            slack_action: Arc::new(crate::actions::SlackAction::new()),
            email_action: Arc::new(crate::actions::EmailAction::new()),
            #[cfg(feature = "caching")]
            cache_invalidator: None,
            database_pool: Arc::new(std::sync::OnceLock::new()),
        };

        let err = dispatcher
            .dispatch_database("fn_notify", None, &test_event())
            .await
            .expect_err("no pool wired must be a loud permanent failure");
        assert!(
            matches!(err, ObserverError::ActionPermanentlyFailed { .. }),
            "must be permanent (no retry can fix a missing pool): {err}"
        );
    }

    /// #632 injection guard: the function name is interpolated into SQL, so a
    /// non-identifier must be rejected at dispatch even if it somehow bypassed
    /// config-load validation (operator-edited `tb_observer.actions`).
    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn database_action_rejects_non_identifier_function_names() {
        let dispatcher = DefaultActionDispatcher {
            webhook_action: Arc::new(crate::actions::WebhookAction::new()),
            slack_action: Arc::new(crate::actions::SlackAction::new()),
            email_action: Arc::new(crate::actions::EmailAction::new()),
            #[cfg(feature = "caching")]
            cache_invalidator: None,
            database_pool: Arc::new(std::sync::OnceLock::new()),
        };

        for evil in [
            "fn; DROP TABLE tb_observer; --",
            "pg_sleep(10)",
            "schema.fn.extra",
            "fn name",
            "",
            "1fn",
        ] {
            let err = dispatcher
                .dispatch_database(evil, None, &test_event())
                .await
                .expect_err("non-identifier function name must be rejected");
            assert!(
                matches!(err, ObserverError::InvalidActionConfig { .. }),
                "identifier guard must fire before any pool access for {evil:?}: {err}"
            );
        }
    }

    /// #632 end-to-end: a `database` action calls the configured PostgreSQL
    /// function with the `{"event": ..., "params": ...}` envelope.
    #[cfg(feature = "postgres")]
    #[tokio::test]
    #[ignore = "requires PostgreSQL"]
    async fn database_action_calls_the_function_for_real() {
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set for --ignored postgres tests");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect");

        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS test_632_db_action_calls \
                 (id BIGSERIAL PRIMARY KEY, envelope JSONB NOT NULL);
             CREATE OR REPLACE FUNCTION test_632_record_event(envelope jsonb) RETURNS void \
                 LANGUAGE sql AS \
                 'INSERT INTO test_632_db_action_calls (envelope) VALUES (envelope)';",
        )
        .execute(&pool)
        .await
        .expect("setup function");
        sqlx::query("DELETE FROM test_632_db_action_calls")
            .execute(&pool)
            .await
            .expect("clean table");

        let slot = Arc::new(std::sync::OnceLock::new());
        let _ = slot.set(pool.clone());
        let dispatcher = DefaultActionDispatcher {
            webhook_action: Arc::new(crate::actions::WebhookAction::new()),
            slack_action: Arc::new(crate::actions::SlackAction::new()),
            email_action: Arc::new(crate::actions::EmailAction::new()),
            #[cfg(feature = "caching")]
            cache_invalidator: None,
            database_pool: slot,
        };

        let event = test_event();
        let params = json!({"channel": "ops"});
        let result = dispatcher
            .dispatch_database("test_632_record_event", Some(&params), &event)
            .await
            .expect("dispatch must call the function");
        assert!(result.success);

        let (count, envelope): (i64, serde_json::Value) = sqlx::query_as(
            "SELECT COUNT(*) OVER (), envelope FROM test_632_db_action_calls LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .expect("row must exist");
        assert_eq!(count, 1, "#632: exactly one function call must have landed");
        assert_eq!(envelope["params"]["channel"], "ops", "params must ride the envelope");
        assert_eq!(
            envelope["event"]["entity_type"], "Order",
            "the full event must ride the envelope"
        );

        // Failure path (honest-failure doctrine): a missing function errors.
        let err = dispatcher
            .dispatch_database("test_632_no_such_function", None, &event)
            .await
            .expect_err("a missing function must be a loud error");
        assert!(
            matches!(err, ObserverError::ActionExecutionFailed { .. }),
            "missing function surfaces as an execution failure: {err}"
        );

        sqlx::raw_sql(
            "DROP FUNCTION IF EXISTS test_632_record_event(jsonb);
             DROP TABLE IF EXISTS test_632_db_action_calls;",
        )
        .execute(&pool)
        .await
        .expect("teardown");
    }
}
