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
//! together. A persisted message then fires its `after:ingest` functions on the
//! I/O-capable host, including the `fraiseql_query` bridge under each function's
//! `run_as` ceiling (#594).

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
    /// The exact public URL the provider signed, for URL-signing schemes
    /// (Twilio). `None` for providers that sign the body only.
    public_url:  Option<String>,
}

/// The concrete pipeline used by the inbound webhook adapter.
type InboundPipeline =
    WebhookPipeline<StaticSecretProvider, PostgresIdempotencyStore, SpineEventHandler>;

/// Shared state for the inbound webhook route.
#[derive(Clone)]
pub struct WebhookInboundState {
    pipeline:               Arc<InboundPipeline>,
    registry:               Arc<ProviderRegistry>,
    /// Path segment (`/webhooks/{segment}`) → resolved route.
    routes:                 Arc<BTreeMap<String, ResolvedRoute>>,
    /// Function-dispatch hooks used to fire `after:ingest` on a persisted
    /// message. `None` (no function runtime configured) persists the message but
    /// dispatches nothing.
    hooks:                  Option<Arc<crate::subsystems::BeforeMutationHooks>>,
    /// The `fraiseql_query` bridge builder (#594) for `after:ingest` functions —
    /// the same request-path executor factory the route handlers thread into
    /// after:mutation. `None` → an after:ingest function's `fraiseql_query` fails
    /// loud ("query executor not configured"), the pre-#594 behavior. Set together
    /// with [`hooks`](Self::hooks) at mount time (both need the app's executor).
    query_executor_factory: Option<crate::routes::after_mutation::QueryExecutorFactory>,
}

impl WebhookInboundState {
    /// Assemble the adapter state from the configured webhook routes.
    ///
    /// `get_env` resolves each route's `secret_env` to its signing secret (in
    /// production, `std::env::var`); a route whose secret is absent is **skipped**
    /// — not mounted — with a warning, so an unconfigured route answers 404 like
    /// any other unknown path instead of 500ing with the missing env var's name in
    /// the body (#787). In production [`webhook_routes_check`] refuses to boot
    /// before this point, so the skip is reachable only in development. The path
    /// segment is the route's `path` override or, failing that, its config key.
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
            let Some(secret) = get_env(&config.secret_env) else {
                tracing::warn!(
                    route = %name,
                    secret_env = %config.secret_env,
                    "inbound webhook route SKIPPED: signing secret env is unset, so the \
                     route is not mounted (deliveries answer 404). Set the variable and \
                     restart to serve it."
                );
                continue;
            };
            secrets = secrets.with_secret(config.secret_env.clone(), secret);
            resolved.insert(
                segment,
                ResolvedRoute {
                    provider:    config.provider.clone(),
                    secret_name: config.secret_env.clone(),
                    public_url:  config.public_url.clone(),
                },
            );
        }

        let store = PostgresIdempotencyStore::new(pool.clone());
        let pipeline = WebhookPipeline::new(pool, secrets, store, SpineEventHandler);

        Self {
            pipeline:               Arc::new(pipeline),
            registry:               Arc::new(ProviderRegistry::new()),
            routes:                 Arc::new(resolved),
            hooks:                  None,
            query_executor_factory: None,
        }
    }

    /// Attach the function-dispatch hooks so a persisted message fires its
    /// `after:ingest[:<source>]` functions on the I/O-capable host context.
    #[must_use]
    pub fn with_hooks(mut self, hooks: Arc<crate::subsystems::BeforeMutationHooks>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Attach the `fraiseql_query` bridge factory (#594) so an `after:ingest`
    /// function can write back under its `run_as` ceiling — the same executor
    /// factory the after:mutation route handlers use. Built with
    /// `make_query_executor_factory` at mount time (it needs the app's
    /// hot-reloadable executor).
    #[must_use]
    pub fn with_query_executor_factory(
        mut self,
        factory: crate::routes::after_mutation::QueryExecutorFactory,
    ) -> Self {
        self.query_executor_factory = Some(factory);
        self
    }

    /// The attached `fraiseql_query` bridge factory, if any (test observability for
    /// the #594 after:ingest wiring; the dispatch path reads the field directly).
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn query_executor_factory(
        &self,
    ) -> Option<&crate::routes::after_mutation::QueryExecutorFactory> {
        self.query_executor_factory.as_ref()
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

/// Validate the configured inbound webhook routes at boot (#787/#781).
///
/// Refuses, in every environment:
///
/// * a `provider` the verifier registry does not know — the route could never verify anything, and
///   the first genuine delivery would 500;
/// * a provider whose signing scheme covers the request URL (Twilio) without a `public_url` — the
///   URL cannot be reconstructed from request headers without trusting the sender.
///
/// Refuses in production (warns in development):
///
/// * a route whose `secret_env` is unset — the route the operator configured would silently answer
///   404 (`WebhookInboundState::new` skips it).
///
/// Pure and race-free like the other boot guards: the caller supplies the env
/// reader and the deployment mode.
///
/// # Errors
///
/// Returns `ServerError::ConfigError` naming the route and what is missing.
pub fn webhook_routes_check<S: std::hash::BuildHasher>(
    routes: &std::collections::HashMap<String, WebhookRouteConfig, S>,
    get_env: impl Fn(&str) -> Option<String>,
    is_production: bool,
) -> crate::Result<()> {
    let registry = ProviderRegistry::new();
    for (name, config) in routes {
        let Some(verifier) = registry.get(&config.provider) else {
            return Err(crate::ServerError::ConfigError(format!(
                "[webhooks.{name}] provider = {:?} is not a known webhook provider; \
                 known providers: {}",
                config.provider,
                {
                    let mut names = registry.providers();
                    names.sort();
                    names.join(", ")
                }
            )));
        };
        if verifier.requires_url() && config.public_url.is_none() {
            return Err(crate::ServerError::ConfigError(format!(
                "[webhooks.{name}] provider = {:?} signs the request URL, so the route \
                 needs `public_url` set to the exact URL registered at the provider. \
                 Reconstructing it from request headers would let the sender choose the \
                 signed material, so the server refuses to guess.",
                config.provider
            )));
        }
        if get_env(&config.secret_env).is_none() {
            if is_production {
                return Err(crate::ServerError::ConfigError(format!(
                    "[webhooks.{name}] secret_env = {:?} is not set in the environment, so \
                     the configured route cannot verify any delivery. Set the variable, or \
                     remove the route. (For local development only, FRAISEQL_ENV=development \
                     downgrades this to a warning and skips the route.)",
                    config.secret_env
                )));
            }
            tracing::warn!(
                route = %name,
                secret_env = %config.secret_env,
                "inbound webhook route will be skipped: signing secret env is unset. \
                 Allowed only because FRAISEQL_ENV=development."
            );
        }
    }
    Ok(())
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

/// The dedup / idempotency key of a delivery: the payload's top-level `id`, else a
/// stable `SHA-256` of the raw body (so an identical redelivery still deduplicates).
///
/// # Why request headers are not consulted (#751)
///
/// This key is what [`PostgresIdempotencyStore::claim`] and the spine dedup on, and
/// what decides whether `after:ingest` fires. Every supported provider signs the
/// **body** only — GitHub/GitLab/Shopify/Postmark/`LemonSqueezy`/generic
/// `HMAC(body)`, Stripe `HMAC(t.body)`, Paddle `HMAC(ts:body)`. No verifier covers
/// request headers.
///
/// So keying on `webhook-id` / `x-github-delivery`, as this used to, put the entire
/// replay defence under the control of whoever sends the HTTP request: one captured
/// signed delivery replayed with a fresh header value passed signature verification,
/// claimed a fresh key, and re-fired `after:ingest` — indefinitely for providers
/// without timestamp freshness. Only signed material can key the replay defence.
///
/// If Svix-style `webhook-id` support is wanted, it needs a verifier that actually
/// signs `{id}.{timestamp}.{body}`; the header may be trusted only then.
fn extract_event_id(payload: &Value, body: &[u8]) -> String {
    payload.get("id").and_then(Value::as_str).map_or_else(
        || {
            use sha2::{Digest as _, Sha256};
            // SHA-256 rather than DefaultHasher: this key is persisted, and
            // DefaultHasher's output is explicitly not stable across Rust releases, so a
            // toolchain bump would silently reset every stored idempotency key.
            format!("body:{}", hex::encode(Sha256::digest(body)))
        },
        str::to_string,
    )
}

/// The provider's event type, from the payload's `type` field, else a known header.
///
/// The signed payload wins (#751): for Stripe and friends an injected
/// `x-github-event` header can no longer relabel a delivery whose body says
/// otherwise.
///
/// The header remains a *fallback* because `GitHub` carries the event type nowhere
/// else — its body has no `type` field and its `HMAC` does not cover headers, so
/// there is no signed alternative to prefer. For such providers the type stays
/// advisory; the replay amplification it used to enable is closed by
/// [`extract_event_id`] no longer trusting headers.
fn extract_event_type(payload: &Value, headers: &BTreeMap<String, String>) -> String {
    payload
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| headers.get("x-github-event").cloned())
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

    // #781: thread the provider's timestamp header and the configured public URL
    // into verification. `Delivery { timestamp: None, url: None }` made every
    // timestamp-requiring verifier (Slack, Discord, SendGrid) and the URL-signing
    // one (Twilio) reject 100% of genuine deliveries with a 401 blaming the sender.
    let timestamp = verifier
        .timestamp_header()
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if verifier.requires_url() && route.public_url.is_none() {
        // webhook_routes_check refuses this at boot; guard the request path too so
        // a bypassed construction cannot silently verify against no URL.
        return json_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            &json!({ "error": "server configuration error" }),
        );
    }

    let header_map = collect_headers(&headers);
    let event_id = extract_event_id(&payload, &body);
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
        timestamp: timestamp.as_deref(),
        url: route.public_url.as_deref(),
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
        // #594: an after:ingest function's `fraiseql_query` runs under its own
        // `run_as` ceiling via the request-path executor factory threaded onto the
        // state at mount time (`None` only when no executor was available — then the
        // bridge fails loud, the pre-#594 behavior).
        crate::routes::after_mutation::spawn_after_ingest(
            hooks,
            plans,
            state.query_executor_factory.clone(),
        );
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
