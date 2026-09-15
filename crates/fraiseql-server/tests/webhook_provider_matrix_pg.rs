//! #781 e2e: genuine provider deliveries are accepted by the real inbound route.
//!
//! The verifiers were individually correct for Slack/Discord/SendGrid/Twilio,
//! but the route never threaded the timestamp header or the request URL into
//! them (`Delivery { timestamp: None, url: None }`), so every genuine delivery
//! from those providers answered 401 — and Lemon Squeezy's verifier compared the
//! wrong encoding. Unit fixtures (`fraiseql-webhooks::signature::tests`) pin the
//! verifiers; this suite pins the **route**: each request below is built exactly
//! as the provider would send it, sent to the mounted router over a live
//! database, and must land as `processed`.
//!
//! Self-skips when no `DATABASE_URL` is set; wired into the Dagger
//! `integration: server` suite next to `webhook_replay_header_dedup_pg`.
//!
//! **Execution engine:** `PostgreSQL` · **Infrastructure:** `DATABASE_URL` ·
//! **Parallelism:** truncates the shared `webhooks` ledger on setup → run
//! `--test-threads=1`.
#![cfg(feature = "inbound")]
#![allow(clippy::unwrap_used, clippy::panic, clippy::print_stderr)] // Reason: test code — panics and skip diagnostics are acceptable

use std::collections::HashMap;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use fraiseql_server::{
    ServerConfig,
    config::WebhookRouteConfig,
    inbound::{WebhookInboundState, WebhookRoutes, webhook_router, webhook_routes_check},
};
use fraiseql_test_support::try_database_url;
use fraiseql_webhooks::PostgresIdempotencyStore;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use sha2::{Digest as _, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tempfile::NamedTempFile;
use tower::ServiceExt as _;

/// The validated route set the mount takes. `webhook_routes_check` is the only way
/// to one (#1321) — which is the point: the router serves what boot accepted, not a
/// second set built from the same configuration.
fn built(routes: &HashMap<String, WebhookRouteConfig>) -> WebhookRoutes {
    webhook_routes_check(routes, |_| Some("configured".to_string()), false)
        .expect("these fixtures are valid configurations")
}

const SLACK_SECRET_ENV: &str = "FRAISEQL_TEST_SLACK_SIGNING_SECRET";
const TWILIO_SECRET_ENV: &str = "FRAISEQL_TEST_TWILIO_AUTH_TOKEN";
const LEMON_SECRET_ENV: &str = "FRAISEQL_TEST_LEMONSQUEEZY_SECRET";
const SECRET: &str = "whsec_781";
const TWILIO_PUBLIC_URL: &str = "https://hooks.example.com/webhooks/twilio";

fn hmac_sha256(message: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(message);
    mac.finalize().into_bytes().to_vec()
}

fn routes() -> HashMap<String, WebhookRouteConfig> {
    let mut routes = HashMap::new();
    routes.insert(
        "slack".to_string(),
        WebhookRouteConfig {
            secret_env:    SLACK_SECRET_ENV.to_string(),
            provider:      "slack".to_string(),
            path:          None,
            public_url:    None,
            credential:    None,
            encoding:      None,
            prefix:        None,
            header_prefix: None,
        },
    );
    routes.insert(
        "twilio".to_string(),
        WebhookRouteConfig {
            secret_env:    TWILIO_SECRET_ENV.to_string(),
            provider:      "twilio".to_string(),
            path:          None,
            public_url:    Some(TWILIO_PUBLIC_URL.to_string()),
            credential:    None,
            encoding:      None,
            prefix:        None,
            header_prefix: None,
        },
    );
    routes.insert(
        "lemonsqueezy".to_string(),
        WebhookRouteConfig {
            secret_env:    LEMON_SECRET_ENV.to_string(),
            provider:      "lemonsqueezy".to_string(),
            path:          None,
            public_url:    None,
            credential:    None,
            encoding:      None,
            prefix:        None,
            header_prefix: None,
        },
    );
    routes
}

fn router(pool: PgPool) -> Router {
    let state = WebhookInboundState::new(pool, &built(&routes()), |_| Some(SECRET.to_string()));
    webhook_router(state)
}

/// Connect, create the ledger + spine, and truncate so each test starts clean.
async fn setup() -> Option<PgPool> {
    let url = try_database_url()?;
    let pool = PgPoolOptions::new().max_connections(4).connect(&url).await.unwrap();
    PostgresIdempotencyStore::new(pool.clone()).init().await.unwrap();
    WebhookInboundState::init_spine(&pool).await.unwrap();
    sqlx::query("TRUNCATE webhooks.tb_inbound_delivery RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();
    Some(pool)
}

async fn send(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        .to_string()
}

/// A value distinct on every run. `setup` truncates the ledger but not the spine,
/// so a suite re-run inside the same second would otherwise present a key the spine
/// already holds.
fn unique() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string()
}

#[tokio::test]
async fn a_genuine_slack_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_slack_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"slack-{}","type":"event_callback"}}"#, now());
    let ts = now();
    let base = format!("v0:{ts}:{body}");
    let signature = format!("v0={}", hex::encode(hmac_sha256(base.as_bytes())));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/slack")
        .header("X-Slack-Signature", signature)
        .header("X-Slack-Request-Timestamp", &ts)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine Slack delivery must be accepted — before #781 the route never \
         read X-Slack-Request-Timestamp and every delivery got 401; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[tokio::test]
async fn a_genuine_twilio_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_twilio_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    // The JSON half of Twilio's scheme: it appends `bodySHA256=<hex>` to the request URI
    // and signs the URI *including* that parameter (#1069) — this request is built the
    // way Twilio actually sends one. The form half is
    // `a_genuine_form_encoded_twilio_delivery_is_processed` below (#1044).
    // Before #1069 the test signed `TWILIO_PUBLIC_URL` bare, which is why it passed
    // against a verifier whose MAC covered no body material at all.
    let body = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let body_hash = hex::encode(Sha256::digest(body.as_bytes()));
    let signed_url = format!("{TWILIO_PUBLIC_URL}?bodySHA256={body_hash}");
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(signed_url.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());

    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/twilio?bodySHA256={body_hash}"))
        .header("X-Twilio-Signature", signature.clone())
        .header("content-type", "application/json")
        .body(Body::from(body.clone()))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine Twilio delivery must be accepted — before #781 the route never \
         passed the configured public_url, so verification always errored; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

/// #1044: the shape Twilio actually posts for SMS and voice —
/// `application/x-www-form-urlencoded`, no `bodySHA256`, signed over the URL with the
/// body's parameters sorted by decoded key and appended as `name + value`.
///
/// The route used to reject any non-JSON body *before* verification, so a correctly
/// configured Twilio route answered `400 {"error":"webhook body is not valid JSON"}`
/// to 100% of genuine SMS callbacks, and the form arm of `build_signing_string` was
/// unreachable through the server. Asserting the persisted payload as well as the
/// status is what makes this a test of normalization and not just of the gate.
#[tokio::test]
async fn a_genuine_form_encoded_twilio_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_genuine_form_encoded_twilio_delivery_is_processed: DATABASE_URL unset"
        );
        return;
    };
    let router = router(pool.clone());

    // A repeated key and two percent-escaped values, so the assertion below covers
    // decoding and the repeat-to-array rule, not just the happy scalar case.
    let call_sid = format!("CA{}", unique());
    let body = format!("Body=hi+there&CallSid={call_sid}&From=%2B15550001111&Tag=a&Tag=b");

    // Twilio's form signing string: the URL, then each decoded `name + value` pair
    // sorted by decoded key, concatenated with no delimiter. Equal keys keep wire
    // order. No `bodySHA256` — that parameter belongs to the non-form scheme.
    let signing_string =
        format!("{TWILIO_PUBLIC_URL}Bodyhi thereCallSid{call_sid}From+15550001111TagaTagb");
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(signing_string.as_bytes());
    let signature = BASE64.encode(mac.finalize().into_bytes());

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/twilio")
        .header("X-Twilio-Signature", signature)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body.clone()))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "#1044: a genuine form-encoded Twilio callback must be verified and accepted; \
         it used to be 400'd on its content type before verification ever ran. \
         Response: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");

    let stored = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT payload -> 'payload' FROM _fraiseql_inbound_message \
         WHERE payload -> 'payload' ->> 'CallSid' = $1",
    )
    .bind(&call_sid)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        stored,
        serde_json::json!({
            "Body":    "hi there",
            "CallSid": call_sid,
            "From":    "+15550001111",
            "Tag":     ["a", "b"],
        }),
        "#1044: the form body must reach after:ingest as a decoded JSON object — \
         `+`/`%2B` decoded, and a repeated key kept as every value rather than one",
    );
}

/// The #1069 exploit, at the route: the same captured `X-Twilio-Signature`, a body the
/// attacker wrote. It used to reach `emit_in_tx` and fire `after:ingest`; the route must
/// now answer 401.
#[tokio::test]
async fn a_forged_twilio_body_under_a_captured_signature_is_refused() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_forged_twilio_body_under_a_captured_signature_is_refused: DATABASE_URL unset"
        );
        return;
    };
    let router = router(pool);

    let genuine = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let body_hash = hex::encode(Sha256::digest(genuine.as_bytes()));
    let signed_url = format!("{TWILIO_PUBLIC_URL}?bodySHA256={body_hash}");
    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(signed_url.as_bytes());
    let captured = BASE64.encode(mac.finalize().into_bytes());

    // Same header, same query string, attacker-chosen payload.
    let forged = r#"{"id":"forged-1","type":"anything","amount":999}"#;
    let request = Request::builder()
        .method("POST")
        .uri(format!("/webhooks/twilio?bodySHA256={body_hash}"))
        .header("X-Twilio-Signature", captured)
        .header("content-type", "application/json")
        .body(Body::from(forged))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a body that is not the one the signature covers must be refused; got: {response}"
    );
}

/// A JSON delivery carrying no body hash at all cannot verify against the pre-#1069
/// constant `HMAC(public_url)` — the value that, once captured anywhere, authorised
/// arbitrary bodies forever.
#[tokio::test]
async fn the_legacy_body_free_twilio_signature_is_refused() {
    let Some(pool) = setup().await else {
        eprintln!("skipping the_legacy_body_free_twilio_signature_is_refused: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let mut mac = Hmac::<Sha1>::new_from_slice(SECRET.as_bytes()).unwrap();
    mac.update(TWILIO_PUBLIC_URL.as_bytes());
    let legacy = BASE64.encode(mac.finalize().into_bytes());

    let body = format!(r#"{{"id":"twilio-{}","type":"sms"}}"#, now());
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/twilio")
        .header("X-Twilio-Signature", legacy)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the body-free signature must no longer authorise anything; got: {response}"
    );
}

#[tokio::test]
async fn a_genuine_lemonsqueezy_delivery_is_processed() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_genuine_lemonsqueezy_delivery_is_processed: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"ls-{}","type":"order_created"}}"#, now());
    // hash_hmac('sha256', $payload, $secret) — hex, as Lemon Squeezy sends it.
    let signature = hex::encode(hmac_sha256(body.as_bytes()));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/lemonsqueezy")
        .header("X-Signature", signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "a genuine (hex-signed) Lemon Squeezy delivery must be accepted (#781); body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[tokio::test]
async fn a_tampered_slack_delivery_is_rejected() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_tampered_slack_delivery_is_rejected: DATABASE_URL unset");
        return;
    };
    let router = router(pool);

    let body = format!(r#"{{"id":"slack-tampered-{}","type":"event_callback"}}"#, now());
    let ts = now();
    let base = format!("v0:{ts}:{body}");
    let signature = format!("v0={}", hex::encode(hmac_sha256(base.as_bytes())));

    // Same signature, body altered after signing.
    let tampered = body.replace("event_callback", "event_tampered");
    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/slack")
        .header("X-Slack-Signature", signature)
        .header("X-Slack-Request-Timestamp", &ts)
        .header("content-type", "application/json")
        .body(Body::from(tampered))
        .unwrap();
    let (status, _) = send(&router, request).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "a tampered Slack delivery must 401");
}

// ── #787: boot-time route validation ─────────────────────────────────────────

#[test]
fn a_route_with_unset_secret_refuses_to_boot_in_production() {
    let err = webhook_routes_check(&routes(), |_| None, true)
        .expect_err("an unset signing-secret env must refuse to boot in production");
    assert!(err.to_string().contains("secret_env"), "must name the knob; got: {err}");
}

#[test]
fn a_route_with_unset_secret_is_skipped_with_a_warning_in_development() {
    assert!(webhook_routes_check(&routes(), |_| None, false).is_ok());
}

#[test]
fn an_unknown_provider_refuses_to_boot_in_every_environment() {
    let mut routes = routes();
    routes.get_mut("slack").unwrap().provider = "slak".to_string();
    for is_production in [true, false] {
        let err = webhook_routes_check(&routes, |_| Some(SECRET.to_string()), is_production)
            .expect_err("a provider the registry does not know can never verify anything");
        assert!(err.to_string().contains("slak"), "must name the bad value; got: {err}");
    }
}

#[test]
fn a_url_signing_provider_without_public_url_refuses_to_boot() {
    let mut routes = routes();
    routes.get_mut("twilio").unwrap().public_url = None;
    for is_production in [true, false] {
        let err = webhook_routes_check(&routes, |_| Some(SECRET.to_string()), is_production)
            .expect_err("Twilio signs the URL; a route without public_url cannot verify");
        assert!(err.to_string().contains("public_url"), "must name the knob; got: {err}");
    }
}

/// #1321: the boot check hands back **what it built**, and that is what the router
/// is mounted from.
///
/// Worth its own case because the failure is silent: a check that validated
/// correctly and returned an empty set would boot clean and then answer 404 to
/// every configured route — the #787 shape, one level up. The stronger half of this
/// invariant (that the *configured* scheme is the one that serves, not a default
/// rebuilt by provider name) is carried by the two generic-HMAC cases below, which
/// redden when a second construction is reinstated at mount time.
#[test]
fn boot_validation_hands_back_every_route_it_built() {
    let validated = webhook_routes_check(&routes(), |_| Some(SECRET.to_string()), true)
        .expect("a fully configured route set boots");

    assert_eq!(
        validated.len(),
        routes().len(),
        "every configured route must come back from validation; the mount has no other \
         source for them"
    );
    assert!(!validated.is_empty());
}

#[test]
fn a_fully_configured_route_set_boots() {
    assert!(webhook_routes_check(&routes(), |_| Some(SECRET.to_string()), true).is_ok());
}

// ── #1321: a route's verification scheme is its configuration ────────────────
//
// The four cases below each discriminate on one axis, so that reverting one fix
// alone reddens one case and no other:
//
//   1. the credential's **location** (a header that is not `X-Signature`) and its **encoding**
//      (base64, not hex). Both are wrong today, so this case must stay red under either mutation on
//      its own;
//   2. the **prefix** a GitHub-style sender puts in front of the hex;
//   3. a mistyped scheme key, which today parses identically to the correct spelling;
//   4. a key the **selected scheme does not read**, which `deny_unknown_fields` alone would still
//      accept and ignore.
//
// Configuration is parsed through `ServerConfig::from_file` — the producer an
// operator actually drives — and not through a `WebhookRouteConfig` struct literal.
// The literal cannot express the defect: a key the struct has no field for is
// discarded *at parse*, so only the parser can answer whether the key reached the
// route at all (`WebhookRouteConfig` carries no `deny_unknown_fields`, and
// `ServerConfig`'s own attribute does not propagate into a nested struct).

/// Lago's documented scheme: `HMAC-SHA256` over the raw body, **base64**, carried in
/// the provider's own `X-Lago-Signature` header.
///
/// ⚠ Synthesized, not captured. The issue's gate names a captured Lago delivery;
/// Lago is self-hostable so one is obtainable, and until it exists this fixture is
/// built from the provider's documentation, exactly like every other delivery in
/// this file (see the module docs).
const LAGO_CONFIG: &str = r#"
[webhooks.lago]
provider   = "hmac-sha256"
secret_env = "FRAISEQL_TEST_LAGO_WEBHOOK_SECRET"
credential = "header:X-Lago-Signature"
encoding   = "base64"
"#;

/// A self-hosted sender that signs like `GitHub`: hex, behind a `sha256=` prefix,
/// in `X-Hub-Signature-256` — expressed as configuration over the generic scheme
/// rather than as one more Rust type.
const SELFHOSTED_CONFIG: &str = r#"
[webhooks.selfhosted]
provider   = "hmac-sha256"
secret_env = "FRAISEQL_TEST_SELFHOSTED_WEBHOOK_SECRET"
credential = "header:X-Hub-Signature-256"
encoding   = "hex"
prefix     = "sha256="
"#;

/// Stripe's scheme has no configurable encoding — it is `t=…,v1=…`, hex, by
/// definition. The route is named `billing` on purpose: `stripe` can then only have
/// reached a refusal message from `provider`.
const STRIPE_WITH_ENCODING: &str = r#"
[webhooks.billing]
provider   = "stripe"
secret_env = "FRAISEQL_TEST_LAGO_WEBHOOK_SECRET"
encoding   = "base64"
"#;

/// [`LAGO_CONFIG`] with `encoding` mistyped, and nothing else changed. Spelled out
/// rather than derived by string surgery so that what the parser is handed is
/// visible here, byte for byte.
const LAGO_CONFIG_MISTYPED: &str = r#"
[webhooks.lago]
provider   = "hmac-sha256"
secret_env = "FRAISEQL_TEST_LAGO_WEBHOOK_SECRET"
credential = "header:X-Lago-Signature"
encodng    = "base64"
"#;

/// Boot the server's view of a `[webhooks.*]` config: parse the file, then run the
/// boot-time route validation over what parsed. Returns the refusal message from
/// whichever of the two refused, because "refused at boot" is one answer to the
/// operator regardless of which half produced it.
fn boot(config_toml: &str) -> Result<HashMap<String, WebhookRouteConfig>, String> {
    let file = NamedTempFile::new().unwrap();
    std::fs::write(file.path(), config_toml).unwrap();
    let config = ServerConfig::from_file(file.path())?;
    let routes = config.webhooks;
    webhook_routes_check(&routes, |_| Some(SECRET.to_string()), true)
        .map_err(|error| error.to_string())?;
    Ok(routes)
}

fn router_for(pool: PgPool, routes: &HashMap<String, WebhookRouteConfig>) -> Router {
    webhook_router(WebhookInboundState::new(pool, &built(routes), |_| Some(SECRET.to_string())))
}

#[tokio::test]
async fn a_generic_hmac_route_honours_its_configured_header_and_encoding() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_generic_hmac_route_honours_its_configured_header_and_encoding: \
             DATABASE_URL unset"
        );
        return;
    };
    let routes = boot(LAGO_CONFIG).expect("a configured generic HMAC route must boot");
    let router = router_for(pool, &routes);

    let body = format!(r#"{{"id":"lago-{}","type":"invoice.created"}}"#, unique());
    let signature = BASE64.encode(hmac_sha256(body.as_bytes()));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/lago")
        // Lower-cased deliberately. HTTP header names are case-insensitive and
        // `collect_headers` stores the lower-cased name, so a scheme that looked the
        // configured `X-Lago-Signature` up case-sensitively would refuse every
        // genuine delivery while passing a fixture that happened to match the case.
        .header("x-lago-signature", &signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "#1321: the route must take the credential from the header the config names \
         and decode it with the encoding the config names. Today both keys are \
         dropped at parse and the generic scheme reads `X-Signature` as hex, so a \
         genuine Lago delivery answers 400. Red on either axis alone: honouring the \
         header but not the encoding gives 401, honouring the encoding but not the \
         header gives 400; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[tokio::test]
async fn a_generic_hmac_route_strips_its_configured_prefix() {
    let Some(pool) = setup().await else {
        eprintln!("skipping a_generic_hmac_route_strips_its_configured_prefix: DATABASE_URL unset");
        return;
    };
    let routes = boot(SELFHOSTED_CONFIG).expect("a configured generic HMAC route must boot");
    let router = router_for(pool, &routes);

    let body = format!(r#"{{"id":"selfhosted-{}","type":"push"}}"#, unique());
    let signature = format!("sha256={}", hex::encode(hmac_sha256(body.as_bytes())));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/selfhosted")
        .header("X-Hub-Signature-256", &signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, response) = send(&router, request).await;

    assert_eq!(
        status,
        StatusCode::OK,
        "#1321: a configured `prefix` must be stripped before the credential is \
         decoded. This case is hex, so it is red only for the prefix and the header — \
         leaving the prefix on makes the comparison fail with 401; body: {response}"
    );
    assert!(response.contains("processed"), "expected processed, got: {response}");
}

#[test]
fn a_mistyped_scheme_key_refuses_to_boot_naming_the_key() {
    let error = boot(LAGO_CONFIG_MISTYPED).expect_err(
        "a mistyped scheme key must refuse to boot. Today it parses exactly like the \
         correct spelling — `WebhookRouteConfig` has no `deny_unknown_fields`, so the \
         key is discarded and the route silently serves the default scheme",
    );

    assert!(
        error.contains("encodng"),
        "the refusal must name the key the operator mistyped, or it cannot be acted \
         on; got: {error}"
    );
}

#[test]
fn a_key_the_selected_scheme_does_not_read_refuses_to_boot_naming_the_scheme() {
    let error = boot(STRIPE_WITH_ENCODING).expect_err(
        "`encoding` on a stripe route is a knob the operator believes is in force and \
         that nothing reads. `deny_unknown_fields` alone does not catch it: the key is \
         known to the *config*, just not to the *scheme* — the silent drop one level \
         down",
    );

    assert!(
        error.contains("encoding"),
        "the refusal must name the key that would have been ignored; got: {error}"
    );
    assert!(
        error.contains("stripe"),
        "the refusal must name the scheme that does not read it — the route is named \
         `billing`, so `stripe` can only have come from `provider`; got: {error}"
    );
}

#[tokio::test]
async fn a_forged_delivery_to_a_configured_route_is_still_refused() {
    let Some(pool) = setup().await else {
        eprintln!(
            "skipping a_forged_delivery_to_a_configured_route_is_still_refused: DATABASE_URL unset"
        );
        return;
    };
    let routes = boot(LAGO_CONFIG).expect("a configured generic HMAC route must boot");
    let router = router_for(pool, &routes);

    // Well-formed base64 in the configured header, over a *different* body: the
    // location and the encoding are right and only the MAC is wrong. Without this
    // case, "honour the configured header and encoding" is satisfiable by a scheme
    // that stopped comparing anything at all, and the two cases above would both
    // pass it.
    let body = format!(r#"{{"id":"lago-forged-{}","type":"invoice.created"}}"#, unique());
    let signature = BASE64.encode(hmac_sha256(b"a different body entirely"));

    let request = Request::builder()
        .method("POST")
        .uri("/webhooks/lago")
        .header("x-lago-signature", &signature)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, _) = send(&router, request).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "a forged Lago delivery must 401");
}
