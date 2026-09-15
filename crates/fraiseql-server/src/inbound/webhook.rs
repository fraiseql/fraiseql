//! The webhook push adapter — the first inbound [`Source`].
//!
//! Mounts `POST /webhooks/{segment}` and turns a signed provider callback into a
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
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
};
use fraiseql_functions::{
    InboundMessage, IngestError, IngestSource, PushSource, RawDelivery, Source, Transport,
};
use fraiseql_webhooks::{
    Authenticated, Delivery, Disposition, EventHandler, Handled, InboundRequest,
    PostgresIdempotencyStore, Result as WebhookResult, SignatureVerifier, StaticSecretProvider,
    VerifiedEvent, WebhookError, WebhookPipeline, build_scheme,
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};

use crate::{config::WebhookRouteConfig, inbound::spine::emit_in_tx};

/// A push [`Source`] for one configured webhook route.
///
/// Normalization is pure: signature verification and the delivery transaction are
/// the pipeline's job, so [`normalize`](PushSource::normalize) only maps a
/// verified [`RawDelivery`] into an [`InboundMessage`], carrying the JSON body as
/// the message [`payload`](InboundMessage::payload).
///
/// # Why the route is carried alongside the provider (#1046)
///
/// The spine dedups on `(source, idempotency_key)`, and `source` is
/// `webhook:<provider>` — the `after:ingest` routing discriminant, which must stay
/// provider-shaped or every declared trigger breaks. So the *route* has to enter
/// the other half: [`normalize`](PushSource::normalize) namespaces the idempotency
/// key as `<route length>:<route>:<event id>`. Without it, two routes serving one
/// provider share a spine namespace, and the second sender's event `1001` is
/// discarded as a redelivery of the first sender's. The email adapter reached the
/// same shape from the other direction (#775), which is why its keys are
/// `<message-id>:sha256:…`.
///
/// The length prefix is what makes that join injective for *any* segment; the
/// sender picks the event id, so a bare `<route>:<id>` join would let one route's
/// sender aim at another's key. See `normalize` for the concrete collision.
pub struct WebhookSource {
    provider: String,
    route:    String,
}

impl WebhookSource {
    /// Build a source for a provider (e.g. `stripe`) received on a named route
    /// (the `/webhooks/{segment}` path segment).
    ///
    /// Both are needed because they answer different questions: the provider is
    /// the `after:ingest:webhook:<provider>` routing discriminant, while the route
    /// is the dedup namespace (#1046).
    #[must_use]
    pub fn new(provider: impl Into<String>, route: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            route:    route.into(),
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
        // #1046: the route, not the provider, is the dedup scope — see the type's
        // documentation for why it cannot live in `source` instead.
        //
        // Length-prefixed, because the sender chooses the event id and a plain
        // `<route>:<id>` join is not injective: route `a` with the id `b:1` lands on
        // the same key as route `a:b` with the id `1`. The ledger key is a real
        // tuple and stays distinct, so such a forgery would claim cleanly and lose
        // only the durable spine row — the exact silent drop this issue is about.
        let idempotency_key = format!("{}:{}:{}", self.route.len(), self.route, delivery.event_id);
        let mut message = InboundMessage::new(self.source(), idempotency_key, delivery.received_at);
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
    ) -> WebhookResult<Handled> {
        let message: InboundMessage = serde_json::from_value(params)?;
        let emitted = emit_in_tx(tx, &message)
            .await
            .map_err(|error| WebhookError::Database(error.to_string()))?;

        // #1176: report what the spine did. This handler runs only when the
        // delivery ledger's `(route, event_id)` claim was fresh, so a spine
        // `Duplicate` means the two dedup layers disagree about the same
        // delivery — and answering "processed" there told the sender its message
        // had been accepted while dispatching `after:ingest` on a row this
        // delivery never wrote.
        //
        // Today the two keys are derived from the same material (#1046), so they
        // agree by construction. That is a property of how they happen to be
        // derived, not a guarantee anything enforces: it breaks if a retention
        // job prunes one table and not the other, if another caller drives
        // `WebhookPipeline` with a different derivation, or if the two drift —
        // which is exactly what #1046 was.
        if !emitted.is_new() {
            tracing::warn!(
                source = ?message.source,
                idempotency_key = %message.idempotency_key,
                "inbound spine refused a delivery whose ledger claim was fresh: the delivery \
                 ledger and the spine disagree about this event. Reported as duplicate; \
                 after:ingest not dispatched."
            );
            return Ok(Handled::Duplicate);
        }

        // Hand the normalized message back so the route can dispatch `after:ingest`.
        Ok(Handled::Recorded(serde_json::to_value(&message)?))
    }
}

/// A configured inbound webhook route, with its verification scheme already built.
///
/// The scheme is a **value**, not a name to be looked up again (#1321). Two routes
/// may both be `hmac-sha256` and read different headers with different encodings,
/// so a per-request lookup by provider name could not serve them — and, more to the
/// point, a second construction is a second chance to disagree with what boot
/// validated.
#[derive(Clone)]
struct BuiltRoute {
    /// The config key this route was declared under, for boot diagnostics.
    name:        String,
    /// Provider key — the `after:ingest:webhook:<provider>` routing discriminant.
    provider:    String,
    /// The verification scheme built from this route's configuration.
    scheme:      Arc<dyn SignatureVerifier>,
    /// Secret name resolved by the pipeline's secret provider.
    secret_name: String,
    /// The exact public URL the provider signed, for URL-signing schemes
    /// (Twilio). `None` for providers that sign the body only.
    public_url:  Option<String>,
}

impl std::fmt::Debug for BuiltRoute {
    /// Names the scheme rather than the verifier, which is a `dyn` value with no
    /// `Debug`. `secret_name` is the **environment variable's** name, never its
    /// value — nothing here may print key material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BuiltRoute")
            .field("name", &self.name)
            .field("provider", &self.provider)
            .field("scheme", &self.scheme.name())
            .field("secret_env", &self.secret_name)
            .field("public_url", &self.public_url)
            .finish()
    }
}

/// Every configured webhook route, **built and validated**.
///
/// The only way to obtain one is [`webhook_routes_check`], so a
/// [`WebhookInboundState`] cannot be assembled from a configuration nothing
/// validated — and the route that boot accepted is, by construction, the route
/// that serves. Before this the boot check and the mount each built their own set
/// from the same config: pure, so they agreed, but agreement by derivation is a
/// property of how two copies happen to be written, not one anything enforces.
/// That is the shape #1046 and #1048 both had.
#[derive(Clone, Debug, Default)]
pub struct WebhookRoutes {
    /// Path segment (`/webhooks/{segment}`) → the route built for it.
    by_segment: BTreeMap<String, BuiltRoute>,
}

impl WebhookRoutes {
    /// Whether any route is configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_segment.is_empty()
    }

    /// How many routes were configured.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_segment.len()
    }
}

/// The replay window handed to the schemes that sign a timestamp.
///
/// The value `ProviderRegistry::new()` used before the registry was deleted, kept
/// so this change moves no provider's freshness behaviour.
const TIMESTAMP_TOLERANCE_SECS: u64 = 300;

/// Build every configured route, or refuse.
///
/// The **one** construction (#1321): boot validation and the mounted router are
/// both served from this, so a configuration the server accepted at boot cannot
/// meet a different scheme — or none — at request time.
///
/// Keyed by path segment, which is the route's `path` override or its config key.
///
/// # Errors
///
/// `ServerError::ConfigError` naming the route and what is wrong with it.
fn build_routes<S: std::hash::BuildHasher>(
    routes: &std::collections::HashMap<String, WebhookRouteConfig, S>,
) -> crate::Result<WebhookRoutes> {
    // #1048: two routes resolving to the same `/webhooks/{segment}` silently shadowed
    // each other. The map below is keyed by the segment, so a repeat is
    // last-write-wins — and because this iterates a `HashMap` whose `RandomState`
    // differs per process, *which* route survived changed between boots of an
    // identical config. The loser's deliveries then met the winner's verifier and
    // failed. Mirrors the duplicate-sink-name guard in
    // `server_config/cdc_outbound.rs`.
    //
    // Sorted first, so the refusal names the same pair on every boot; diagnosing a
    // non-deterministic config error with a non-deterministic message would be no
    // better than the defect.
    let mut sorted: Vec<(&String, &WebhookRouteConfig)> = routes.iter().collect();
    sorted.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut built: BTreeMap<String, BuiltRoute> = BTreeMap::new();
    for (name, config) in sorted {
        let segment = config.path.clone().unwrap_or_else(|| name.clone());
        if let Some(previous) = built.get(&segment) {
            return Err(crate::ServerError::ConfigError(format!(
                "[webhooks.{}] and [webhooks.{name}] both resolve to the path \
                 segment {segment:?}, so only one of them could ever be mounted and which \
                 one would change between restarts. Give one of them a distinct `path`, or \
                 remove it. (A route's segment is its `path` override, or its config key \
                 when `path` is absent — so an override may collide with another route's \
                 name.)",
                previous.name
            )));
        }

        let scheme =
            build_scheme(&config.provider, &config.scheme_config(), TIMESTAMP_TOLERANCE_SECS)
                .map_err(|error| {
                    crate::ServerError::ConfigError(format!("[webhooks.{name}] {error}"))
                })?;

        if scheme.requires_url() && config.public_url.is_none() {
            return Err(crate::ServerError::ConfigError(format!(
                "[webhooks.{name}] provider = {:?} signs the request URL, so the route \
                 needs `public_url` set to the exact URL registered at the provider. \
                 Reconstructing it from request headers would let the sender choose the \
                 signed material, so the server refuses to guess.",
                config.provider
            )));
        }

        built.insert(
            segment,
            BuiltRoute {
                name: name.clone(),
                provider: config.provider.clone(),
                scheme,
                secret_name: config.secret_env.clone(),
                public_url: config.public_url.clone(),
            },
        );
    }
    Ok(WebhookRoutes { by_segment: built })
}

/// The concrete pipeline used by the inbound webhook adapter.
type InboundPipeline =
    WebhookPipeline<StaticSecretProvider, PostgresIdempotencyStore, SpineEventHandler>;

/// Shared state for the inbound webhook route.
#[derive(Clone)]
pub struct WebhookInboundState {
    pipeline:               Arc<InboundPipeline>,
    /// Path segment (`/webhooks/{segment}`) → the route built at boot.
    routes:                 Arc<BTreeMap<String, BuiltRoute>>,
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
    ///
    /// Takes the routes [`webhook_routes_check`] **already built** (#1321). This is
    /// infallible because everything that can be refused about a route was refused
    /// there: the scheme is a value that exists, so there is nothing left to
    /// construct here and no second construction that could disagree with the one
    /// boot validated.
    #[must_use]
    pub fn new(
        pool: PgPool,
        routes: &WebhookRoutes,
        get_env: impl Fn(&str) -> Option<String>,
    ) -> Self {
        let mut secrets = StaticSecretProvider::new();
        let mut mounted = BTreeMap::new();

        for (segment, route) in routes.by_segment.clone() {
            // #1045: `SECRET_ENV=""` is unset for every purpose that matters — it cannot
            // verify anything — so it takes the same skip path rather than mounting a
            // route that answers 401 to every genuine delivery.
            let Some(secret) = get_env(&route.secret_name).filter(|s| !s.is_empty()) else {
                tracing::warn!(
                    route = %route.name,
                    secret_env = %route.secret_name,
                    "inbound webhook route SKIPPED: signing secret env is unset, so the \
                     route is not mounted (deliveries answer 404). Set the variable and \
                     restart to serve it."
                );
                continue;
            };
            secrets = secrets.with_secret(route.secret_name.clone(), secret);
            mounted.insert(segment, route);
        }

        let store = PostgresIdempotencyStore::new(pool.clone());
        let pipeline = WebhookPipeline::new(pool, secrets, store, SpineEventHandler);

        Self {
            pipeline:               Arc::new(pipeline),
            routes:                 Arc::new(mounted),
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

/// Validate the configured inbound webhook routes at boot (#787/#781/#1321).
///
/// Runs `build_routes` — the same construction that serves the mounted router —
/// and then applies the one policy that depends on the environment. Everything
/// `build_routes` refuses is refused here in every environment:
///
/// * two routes resolving to one `/webhooks/{segment}`, which would shadow each other
///   non-deterministically (#1048);
/// * a `provider` that names no known scheme — the route could never verify anything;
/// * a scheme key the selected scheme does not read, or a credential location it cannot honour
///   (#1321);
/// * a scheme that covers the request URL (Twilio) without a `public_url` — the URL cannot be
///   reconstructed from request headers without trusting the sender.
///
/// Refuses in production (warns in development):
///
/// * a route whose `secret_env` is unset — the route the operator configured would silently answer
///   404 ([`WebhookInboundState::new`] skips it).
///
/// Pure and race-free like the other boot guards: the caller supplies the env
/// reader and the deployment mode.
///
/// Returns the built routes, so the caller mounts **what was validated** rather
/// than building a second set from the same configuration (#1321).
///
/// # Errors
///
/// `ServerError::ConfigError` naming the route and what is missing.
pub fn webhook_routes_check<S: std::hash::BuildHasher>(
    routes: &std::collections::HashMap<String, WebhookRouteConfig, S>,
    get_env: impl Fn(&str) -> Option<String>,
    is_production: bool,
) -> crate::Result<WebhookRoutes> {
    let built = build_routes(routes)?;
    for route in built.by_segment.values() {
        // #1045: an env var that is set but empty verifies nothing, so it is treated as
        // unset here too. Checking only `is_none()` let `SECRET_ENV=""` boot clean and
        // then fail every delivery with a 401 that blamed the sender.
        let Some(secret) = get_env(&route.secret_name).filter(|s| !s.is_empty()) else {
            if is_production {
                return Err(crate::ServerError::ConfigError(format!(
                    "[webhooks.{}] secret_env = {:?} is not set (or is empty) in the \
                     environment, so the configured route cannot verify any delivery. Set \
                     the variable, or remove the route. (For local development only, \
                     FRAISEQL_ENV=development downgrades this to a warning and skips the \
                     route.)",
                    route.name, route.secret_name
                )));
            }
            tracing::warn!(
                route = %route.name,
                secret_env = %route.secret_name,
                "inbound webhook route will be skipped: signing secret env is unset. \
                 Allowed only because FRAISEQL_ENV=development."
            );
            continue;
        };
        // #1323: a scheme that can tell usable key material from unusable gets to
        // say so here, while the operator is still watching a boot log, rather than
        // on every genuine delivery. Most schemes cannot and accept anything — see
        // `SignatureVerifier::check_key_material`, where that permissive default is
        // named. `standard-webhooks` can: `whpk_`/`whsk_` is asymmetric `v1a`
        // material this crate does not verify, and a secret that does not
        // base64-decode is not a key at all.
        //
        // Reached only with a secret actually present — the `else` arm above returns
        // or skips — so the shape of a secret that is not there is never judged: in
        // development an unset secret skips the route, and refusing the boot for it
        // would undo that.
        if let Err(error) = route.scheme.check_key_material(&secret) {
            return Err(crate::ServerError::ConfigError(format!(
                "[webhooks.{}] the {} scheme cannot use the key material in \
                 secret_env = {:?}: {error}",
                route.name, route.provider, route.secret_name
            )));
        }
    }
    Ok(built)
}

/// The query parameter Twilio appends for non-form bodies, carrying the hex SHA-256
/// of the raw request body (#1069). The verifier re-derives the digest and compares.
const BODY_SHA256_PARAM: &str = "bodySHA256";

/// The raw `bodySHA256` value from a request's query string, if it carries one.
///
/// Not percent-decoded: the value is hex, and the signing string must contain it byte
/// for byte as the sender wrote it, or the HMAC will not match.
fn body_sha256_query(query: Option<&str>) -> Option<&str> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| (key == BODY_SHA256_PARAM).then_some(value))
}

/// Append `bodySHA256=<hash>` to a configured public URL, respecting whether it
/// already carries a query string.
fn append_query_param(base: &str, hash: &str) -> String {
    let separator = if base.contains('?') { '&' } else { '?' };
    format!("{base}{separator}{BODY_SHA256_PARAM}={hash}")
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

/// The media type Twilio posts SMS/voice callbacks as, and the one the form arm of
/// its signing scheme exists to verify.
const FORM_MEDIA_TYPE: &str = "application/x-www-form-urlencoded";

/// Whether the request declares a form-encoded body (#1044).
///
/// Compares the media type alone: `; charset=UTF-8` is a legal parameter and must
/// not change the reading. The sender's declaration is what decides this, rather
/// than sniffing the bytes — guessing at a format is how a body that is valid JSON
/// *and* valid form-encoding would be read two different ways on two deployments.
fn is_form_encoded(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case(FORM_MEDIA_TYPE)
        })
}

/// Parse an `application/x-www-form-urlencoded` body into a JSON object (#1044).
///
/// `after:ingest` functions consume [`InboundMessage::payload`], which is JSON, so
/// a form body has to become one. A key seen once maps to its string value; a key
/// that repeats maps to the array of its values in wire order. Form encoding
/// permits repeats, and collapsing them to a single value would drop data with
/// nothing on the wire to show for it.
///
/// Percent-decoding (and `+` as space) comes from `url::form_urlencoded`, the same
/// grammar Twilio's signing string is built from — but deliberately a separate call
/// from verification, which reads the raw bytes and never this value.
///
/// Never fails: form encoding has no invalid syntax to reject. A body with no `=`
/// is one key with an empty value, and an empty body is an empty object.
fn form_to_json(body: &[u8]) -> Value {
    use serde_json::map::Entry;

    let mut object = serde_json::Map::new();
    for (key, value) in url::form_urlencoded::parse(body) {
        let value = Value::String(value.into_owned());
        match object.entry(key.into_owned()) {
            Entry::Vacant(slot) => {
                slot.insert(value);
            },
            Entry::Occupied(mut slot) => match slot.get_mut() {
                Value::Array(values) => values.push(value),
                first => *first = Value::Array(vec![first.take(), value]),
            },
        }
    }
    Value::Object(object)
}

/// Read a **verified** body as the event it is, dispatching on the declared media
/// type (#1044, #1321).
///
/// Twilio posts SMS/voice callbacks as `application/x-www-form-urlencoded` — that
/// is what the form arm of its signing scheme is for — so refusing any non-JSON
/// body meant a correctly configured Twilio route answered 400 to 100% of genuine
/// deliveries and the form arm was unreachable. The sender's declaration decides,
/// rather than sniffing the bytes: guessing is how a body that is valid JSON *and*
/// valid form-encoding would be read two different ways on two deployments.
///
/// Called only from the `Authenticated::Body` arm, so it runs **after** the
/// signature holds. A body that does not parse is still the sender's fault and
/// still a 400 — but an unauthenticated caller can no longer learn anything by
/// sending one, and a scheme whose body is not JSON at all is no longer refused
/// before it runs.
///
/// # Errors
///
/// [`WebhookError::InvalidPayload`] when the body declares JSON and is not.
fn parse_body(body: &[u8], headers: &HeaderMap) -> WebhookResult<Value> {
    if is_form_encoded(headers) {
        return Ok(form_to_json(body));
    }
    serde_json::from_slice::<Value>(body)
        .map_err(|_| WebhookError::InvalidPayload("webhook body is not valid JSON".to_string()))
}

/// `POST /webhooks/{segment}` — verify, normalize, and persist an inbound delivery.
///
/// On success returns `200` with `{"status":"processed"|"duplicate"}`. A forged
/// signature is `401`, a malformed payload `400`, a server-side misconfiguration
/// `500` — routed by the pipeline's error mapping.
///
/// The captured path parameter is the **route segment**, not the provider: several
/// routes may serve one provider, and it is the segment that identifies which
/// configuration (and which signing secret) a delivery arrived under. It was named
/// `provider` here, which is how it came to key the dedup namespace (#1046).
pub async fn webhook_handler(
    State(state): State<WebhookInboundState>,
    Path(segment): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Some(route) = state.routes.get(&segment) else {
        return json_status(
            StatusCode::NOT_FOUND,
            &json!({ "error": format!("no inbound webhook route '{segment}'") }),
        );
    };

    // #1321: the scheme was built at boot from this route's configuration and is
    // held here. There is no second lookup by provider name — which could not serve
    // two `hmac-sha256` routes reading different headers anyway — and so no
    // "unknown webhook provider" 500 on a configuration that already booted.
    //
    // The route reads **nothing** out of the request before handing it over. It used
    // to refuse a request with no `X-Signature` header, and a body that is not JSON,
    // before the scheme ever ran: the first could not serve a scheme whose credential
    // is elsewhere, and the second could not serve one whose body is a bare token.
    // Both also told an unauthenticated caller what the endpoint expects.
    let verifier = route.scheme.as_ref();

    // #781: a URL-signing scheme (Twilio) needs the URL the provider signed.
    // `build_routes` refuses such a route without `public_url` at boot; guard the
    // request path too, so a bypassed construction cannot silently verify against
    // no URL.
    if verifier.requires_url() && route.public_url.is_none() {
        return json_status(
            StatusCode::INTERNAL_SERVER_ERROR,
            &json!({ "error": "server configuration error" }),
        );
    }

    // #1069: Twilio's non-form scheme appends `bodySHA256=<hex of body>` to the request
    // URI and signs the URI *including* that parameter. The host/path half still comes
    // from the configured `public_url` — reconstructing it from request headers would
    // trust attacker-controlled input, which is the whole reason `requires_url` exists —
    // but the body-hash parameter is taken from the request, because that is where the
    // sender puts it. Taking it is safe in a way reconstructing the host is not: it is
    // covered by the signature, and the verifier re-derives the digest from the body it
    // actually received, so a hash that does not describe this body cannot verify.
    let signing_url = route.public_url.as_ref().map(|base| {
        body_sha256_query(query.as_deref())
            .map_or_else(|| base.clone(), |hash| append_query_param(base, hash))
    });

    let header_map = collect_headers(&headers);
    let received_at = chrono::Utc::now();
    let source = WebhookSource::new(route.provider.clone(), segment.clone());

    // #1321: the event is read out of what verification **authenticated**, inside
    // the pipeline, after the signature holds and before any database work. The body
    // is parsed only on the arm where the scheme signed it — on the other arm the
    // body is an envelope and nothing in it is trusted, which is the #751 class the
    // `Authenticated` view removes.
    let event_of = |authenticated: Authenticated<'_>| -> WebhookResult<VerifiedEvent> {
        let (event_id, event_type, event_payload) = match authenticated {
            Authenticated::Body(verified_body) => {
                // The body is parsed **here**, after the signature holds, and only
                // on the arm where the body is the event.
                let payload = parse_body(verified_body, &headers)?;
                let event_id = extract_event_id(&payload, verified_body);
                let event_type = extract_event_type(&payload, &header_map);
                (event_id, event_type, payload)
            },
            // #1323: the body is the event and it is signed, so the caller's own
            // payload and event-type rules apply to it unchanged — but the id comes
            // out of the signature, never out of the body. `extract_event_id` is
            // deliberately NOT consulted here: its whole purpose is to find an id in
            // bytes nothing authenticated, and this arm has an authenticated one.
            Authenticated::BodyWithId {
                body: verified_body,
                id,
            } => {
                let payload = parse_body(verified_body, &headers)?;
                let event_type = extract_event_type(&payload, &header_map);
                (id.to_string(), event_type, payload)
            },
            Authenticated::Event {
                id,
                event_type,
                payload,
            } => (id.to_string(), event_type.to_string(), payload.clone()),
        };
        let raw = RawDelivery {
            event_id: &event_id,
            event_type: &event_type,
            payload: &event_payload,
            headers: &header_map,
            received_at,
        };
        // A verified delivery that cannot be read as an event is a 400: it is the
        // sender's payload that is malformed, and it is refused before any database
        // work like every other sender-caused refusal on this path.
        let message = source
            .normalize(&raw)
            .map_err(|error| WebhookError::InvalidPayload(error.to_string()))?;
        Ok(VerifiedEvent {
            id: event_id,
            event_type,
            params: serde_json::to_value(&message)?,
        })
    };

    let delivery = Delivery {
        // #1046: the dedup namespace is this route, not the provider it serves.
        // Sound as a namespace because `build_routes` refuses two routes resolving
        // to one segment (#1048), so a segment names exactly one config.
        route:         &segment,
        function_name: &segment,
        request:       InboundRequest::new(&header_map, &body, signing_url.as_deref()),
    };

    match state.pipeline.process(verifier, &route.secret_name, &delivery, event_of).await {
        Ok(Disposition::Processed(recorded)) => {
            // Committed durably: now fire `after:ingest` on the persisted message.
            // It comes back from the handler's own return value, so what is
            // dispatched is the row that was written rather than a copy built
            // beside it — the two could only differ by being derived twice.
            match serde_json::from_value::<InboundMessage>(recorded) {
                Ok(message) => dispatch_after_ingest(&state, &message),
                Err(error) => tracing::error!(
                    route = %segment,
                    %error,
                    "inbound webhook delivery committed but its persisted message could \
                     not be read back; after:ingest not dispatched"
                ),
            }
            json_status(StatusCode::OK, &json!({ "status": "processed" }))
        },
        Ok(Disposition::Duplicate) => {
            json_status(StatusCode::OK, &json!({ "status": "duplicate" }))
        },
        // `Disposition` is `#[non_exhaustive]`; a future outcome is treated as
        // accepted-but-unclassified rather than failing the sender.
        Ok(_) => json_status(StatusCode::OK, &json!({ "status": "accepted" })),
        Err(error) => {
            // #1045: render through `FraiseQLError`'s own `IntoResponse` — that impl *is*
            // the sanitizer, collapsing `Authentication` to a flat "Authentication failed"
            // and `Database` to "A database error occurred". Hand-rolling the body here
            // put the entire `Display` chain in front of an unauthenticated caller: the
            // verifier's internal reason on a 401, and raw PostgreSQL error text on a 5xx
            // via `WebhookError::Database` → "inbound spine: claim: {error}".
            let mapped: fraiseql_error::FraiseQLError = error.into();
            // The detail the client no longer sees still has to reach the operator —
            // that is the trade the sanitizer's own comment describes.
            tracing::warn!(
                route = %segment,
                provider = %route.provider,
                error = %mapped,
                "inbound webhook delivery failed"
            );
            mapped.into_response()
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
/// single route is `POST /webhooks/{segment}`.
pub fn webhook_router(state: WebhookInboundState) -> Router {
    Router::new()
        .route("/webhooks/{segment}", post(webhook_handler))
        .with_state(state)
}

#[cfg(test)]
mod tests;
