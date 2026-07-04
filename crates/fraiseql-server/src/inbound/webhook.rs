//! The webhook push adapter — the first inbound [`Source`].
//!
//! Mounts `POST /webhooks/{provider}` and turns a signed provider callback into a
//! normalized [`InboundMessage`] on the durable spine, reusing the
//! `fraiseql-webhooks` [`WebhookPipeline`] for the security-critical middle:
//! resolve the signing secret → verify the signature (no database work until the
//! signature is trusted) → atomically claim the delivery and run the handler in
//! one transaction.
//!
//! The adapter boundary keeps the receiver provider-generic: the pipeline handles
//! *any* configured provider, and normalization ([`WebhookSource`]) is the shared
//! layer above it. The verified delivery is normalized into an [`InboundMessage`]
//! and persisted onto the spine ([`emit_in_tx`]) *inside the delivery
//! transaction*, so the spine write and the idempotency claim commit or roll back
//! together. Firing `after:ingest` functions on the persisted message is wired in
//! the next cycle.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use fraiseql_functions::{
    InboundMessage, IngestError, IngestSource, PushSource, RawDelivery, Source, Transport,
};
use fraiseql_webhooks::{
    Delivery, Disposition, EventHandler, PostgresIdempotencyStore, Result as WebhookResult,
    StaticSecretProvider, WebhookError, WebhookPipeline, signature::ProviderRegistry,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{config::WebhookRouteConfig, inbound::spine::emit_in_tx};

/// A push [`Source`] for one webhook provider.
///
/// Normalization is pure: signature verification and the delivery transaction are
/// the pipeline's job, so [`normalize`](PushSource::normalize) only maps a
/// verified [`RawDelivery`] into an [`InboundMessage`], carrying the JSON body as
/// the message [`payload`](InboundMessage::payload).
pub struct WebhookSource {
    provider: String,
}

impl WebhookSource {
    /// Build a source for a provider (e.g. `stripe`).
    #[must_use]
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
        }
    }
}

impl Source for WebhookSource {
    fn source(&self) -> IngestSource {
        IngestSource::Webhook {
            provider: self.provider.clone(),
        }
    }

    fn transport(&self) -> Transport {
        Transport::Push
    }
}

impl PushSource for WebhookSource {
    fn normalize(&self, delivery: &RawDelivery<'_>) -> Result<InboundMessage, IngestError> {
        if delivery.event_id.is_empty() {
            return Err(IngestError::new("webhook delivery has no event id"));
        }
        let mut message =
            InboundMessage::new(self.source(), delivery.event_id, delivery.received_at);
        // The event type is the closest thing a webhook has to a subject.
        if !delivery.event_type.is_empty() {
            message.subject = Some(delivery.event_type.to_string());
        }
        message.headers = delivery.headers.clone();
        message.payload = Some(delivery.payload.clone());
        Ok(message)
    }
}

/// The [`EventHandler`] that persists a normalized message onto the spine.
///
/// The route pre-normalizes the delivery and passes the [`InboundMessage`] as the
/// delivery params; this handler runs inside the pipeline's transaction, so its
/// spine write is atomic with the pipeline's idempotency claim.
struct SpineEventHandler;

impl EventHandler for SpineEventHandler {
    async fn handle(
        &self,
        _function_name: &str,
        params: Value,
        tx: &mut Transaction<'_, Postgres>,
    ) -> WebhookResult<Value> {
        let message: InboundMessage = serde_json::from_value(params)?;
        emit_in_tx(tx, &message)
            .await
            .map_err(|error| WebhookError::Database(error.to_string()))?;
        // Hand the normalized message back so the route can dispatch `after:ingest`.
        serde_json::to_value(&message).map_err(Into::into)
    }
}

/// A configured inbound webhook route: which provider verifier to use and which
/// named secret resolves its signing key.
#[derive(Debug, Clone)]
struct ResolvedRoute {
    /// Provider key selecting the signature verifier (e.g. `stripe`).
    provider:    String,
    /// Secret name resolved by the pipeline's secret provider.
    secret_name: String,
}

/// The concrete pipeline used by the inbound webhook adapter.
type InboundPipeline =
    WebhookPipeline<StaticSecretProvider, PostgresIdempotencyStore, SpineEventHandler>;

/// Shared state for the inbound webhook route.
#[derive(Clone)]
pub struct WebhookInboundState {
    pipeline: Arc<InboundPipeline>,
    registry: Arc<ProviderRegistry>,
    /// Path segment (`/webhooks/{segment}`) → resolved route.
    routes:   Arc<BTreeMap<String, ResolvedRoute>>,
    /// Function-dispatch hooks used to fire `after:ingest` on a persisted
    /// message. `None` (no function runtime configured) persists the message but
    /// dispatches nothing.
    hooks:    Option<Arc<crate::subsystems::BeforeMutationHooks>>,
}

impl WebhookInboundState {
    /// Assemble the adapter state from the configured webhook routes.
    ///
    /// `get_env` resolves each route's `secret_env` to its signing secret (in
    /// production, `std::env::var`); a route whose secret is absent is skipped
    /// with a warning rather than mounted without a key. The path segment is the
    /// route's `path` override or, failing that, its config key.
    #[must_use]
    pub fn new(
        pool: PgPool,
        routes: &std::collections::HashMap<String, WebhookRouteConfig>,
        get_env: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let mut secrets = StaticSecretProvider::new();
        let mut resolved = BTreeMap::new();

        for (name, config) in routes {
            let segment = config.path.clone().unwrap_or_else(|| name.clone());
            match get_env(&config.secret_env) {
                Some(secret) => secrets = secrets.with_secret(config.secret_env.clone(), secret),
                None => {
                    tracing::warn!(
                        route = %name,
                        secret_env = %config.secret_env,
                        "inbound webhook route not fully configured: signing secret env is unset; \
                         deliveries will fail signature verification until it is provided"
                    );
                },
            }
            resolved.insert(
                segment,
                ResolvedRoute {
                    provider:    config.provider.clone(),
                    secret_name: config.secret_env.clone(),
                },
            );
        }

        let store = PostgresIdempotencyStore::new(pool.clone());
        let pipeline = WebhookPipeline::new(pool, secrets, store, SpineEventHandler);

        Self {
            pipeline: Arc::new(pipeline),
            registry: Arc::new(ProviderRegistry::new()),
            routes:   Arc::new(resolved),
            hooks:    None,
        }
    }

    /// Attach the function-dispatch hooks so a persisted message fires its
    /// `after:ingest[:<source>]` functions on the I/O-capable host context.
    #[must_use]
    pub fn with_hooks(mut self, hooks: Arc<crate::subsystems::BeforeMutationHooks>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Create the spine table the adapter writes to (idempotent).
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Database`](fraiseql_error::FraiseQLError::Database)
    /// if the DDL fails.
    pub async fn init_spine(pool: &PgPool) -> fraiseql_error::Result<()> {
        super::spine::PostgresInboundSpine::new(pool.clone()).init().await
    }
}

/// Collect request headers into a name→value map, dropping non-UTF-8 values.
fn collect_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

/// The dedup / idempotency key of a delivery: a provider delivery-id header, else
/// the payload's top-level `id`, else a stable hash of the raw body (so an
/// identical redelivery still deduplicates).
fn extract_event_id(payload: &Value, headers: &BTreeMap<String, String>, body: &[u8]) -> String {
    headers
        .get("webhook-id")
        .or_else(|| headers.get("x-github-delivery"))
        .cloned()
        .or_else(|| payload.get("id").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            body.hash(&mut hasher);
            format!("body:{:016x}", hasher.finish())
        })
}

/// The provider's event type, from a known header or the payload's `type` field.
fn extract_event_type(payload: &Value, headers: &BTreeMap<String, String>) -> String {
    headers
        .get("x-github-event")
        .cloned()
        .or_else(|| payload.get("type").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// Render a JSON status body with the given HTTP status.
fn json_status(status: StatusCode, body: &Value) -> Response {
    (status, body.to_string()).into_response()
}

/// `POST /webhooks/{provider}` — verify, normalize, and persist an inbound delivery.
///
/// On success returns `200` with `{"status":"processed"|"duplicate"}`. A forged
/// signature is `401`, a malformed payload `400`, a server-side misconfiguration
/// `500` — routed by the pipeline's error mapping.
pub async fn webhook_handler(
    State(state): State<WebhookInboundState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(route) = state.routes.get(&provider) else {
        return json_status(
            StatusCode::NOT_FOUND,
            &json!({ "error": format!("no inbound webhook route '{provider}'") }),
        );
    };

    let Some(verifier) = state.registry.get(&route.provider) else {
        return json_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            &json!({ "error": format!("unknown webhook provider '{}'", route.provider) }),
        );
    };

    let Some(signature) = headers.get(verifier.signature_header()).and_then(|v| v.to_str().ok())
    else {
        return json_status(
            StatusCode::BAD_REQUEST,
            &json!({ "error": format!("missing signature header '{}'", verifier.signature_header()) }),
        );
    };
    let signature = signature.to_string();

    // A non-JSON body is rejected: every supported provider posts JSON, and a
    // structured payload is what `after:ingest` functions consume.
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return json_status(
            StatusCode::BAD_REQUEST,
            &json!({ "error": "webhook body is not valid JSON" }),
        );
    };

    let header_map = collect_headers(&headers);
    let event_id = extract_event_id(&payload, &header_map, &body);
    let event_type = extract_event_type(&payload, &header_map);

    // Normalize before the pipeline so the durable payload is the normalized
    // message; the pipeline persists it (as delivery params) inside its transaction.
    let source = WebhookSource::new(route.provider.clone());
    let raw = RawDelivery {
        event_id:    &event_id,
        event_type:  &event_type,
        payload:     &payload,
        headers:     &header_map,
        received_at: chrono::Utc::now(),
    };
    let message = match source.normalize(&raw) {
        Ok(message) => message,
        Err(error) => {
            return json_status(StatusCode::BAD_REQUEST, &json!({ "error": error.to_string() }));
        },
    };
    let params = serde_json::to_value(&message).unwrap_or(Value::Null);

    let delivery = Delivery {
        provider: &route.provider,
        event_id: &event_id,
        event_type: &event_type,
        function_name: &provider,
        body: &body,
        signature: &signature,
        timestamp: None,
        url: None,
        params,
    };

    match state.pipeline.process(verifier.as_ref(), &route.secret_name, &delivery).await {
        Ok(Disposition::Processed(_)) => {
            // Committed durably: now fire `after:ingest` on the persisted message.
            dispatch_after_ingest(&state, &message);
            json_status(StatusCode::OK, &json!({ "status": "processed" }))
        },
        Ok(Disposition::Duplicate) => {
            json_status(StatusCode::OK, &json!({ "status": "duplicate" }))
        },
        // `Disposition` is `#[non_exhaustive]`; a future outcome is treated as
        // accepted-but-unclassified rather than failing the sender.
        Ok(_) => json_status(StatusCode::OK, &json!({ "status": "accepted" })),
        Err(error) => {
            let mapped: fraiseql_error::FraiseQLError = error.into();
            let status = StatusCode::from_u16(mapped.status_code())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            json_status(status, &json!({ "error": mapped.to_string() }))
        },
    }
}

/// Fire the `after:ingest` functions matching a persisted message, on the
/// I/O-capable host context with the same durability as `after:mutation`.
///
/// A no-op when no function-dispatch hooks are attached (the message is still
/// persisted; there is simply nothing to dispatch).
fn dispatch_after_ingest(state: &WebhookInboundState, message: &InboundMessage) {
    let Some(ref hooks) = state.hooks else {
        return;
    };
    let plans = crate::routes::after_mutation::plan_after_ingest_dispatch(hooks, message);
    if !plans.is_empty() {
        crate::routes::after_mutation::spawn_after_ingest(hooks, plans);
    }
}

/// Build the inbound webhook sub-router. Register with [`Router::merge`]; the
/// single route is `POST /webhooks/{provider}`.
pub fn webhook_router(state: WebhookInboundState) -> Router {
    Router::new()
        .route("/webhooks/{provider}", post(webhook_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests;
