//! What the first-party issuer publishes: its discovery document and its JWK set, so a
//! service beside this server verifies the sessions it mints the way it verifies any
//! OIDC provider's tokens.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};

use super::FirstPartyIssuer;
use crate::server_config::IssuerConfig;

/// Where the discovery document is served.
pub const DISCOVERY_PATH: &str = "/.well-known/openid-configuration";
/// Where the JWK set is served.
pub const JWKS_PATH: &str = "/.well-known/jwks.json";

/// How long a verifier may cache the JWK set. A verifier that meets a `kid` it does not
/// hold re-fetches regardless.
const JWKS_MAX_AGE_SECS: u64 = 300;

/// The discovery document for `config`: the issuer, where its keys are published, and
/// the one algorithm it signs with.
#[must_use]
pub fn discovery_document(config: &IssuerConfig) -> Value {
    json!({
        "issuer": config.url,
        "jwks_uri": format!("{}{JWKS_PATH}", config.url.trim_end_matches('/')),
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["ES256"],
    })
}

/// The routes publishing `issuer`'s discovery document and JWK set.
pub fn routes(issuer: Arc<FirstPartyIssuer>) -> Router {
    Router::new()
        .route(DISCOVERY_PATH, get(openid_configuration))
        .route(JWKS_PATH, get(jwks))
        .with_state(issuer)
}

async fn openid_configuration(State(issuer): State<Arc<FirstPartyIssuer>>) -> Json<Value> {
    Json(discovery_document(issuer.config()))
}

/// The keys the ring publishes: the active key and every retired key still in its
/// window, public halves only. `503` until the ring's first read.
async fn jwks(State(issuer): State<Arc<FirstPartyIssuer>>) -> Response {
    match issuer.ring().current() {
        Ok(ring) => (
            [(header::CACHE_CONTROL, format!("public, max-age={JWKS_MAX_AGE_SECS}"))],
            Json(ring.jwks()),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "The first-party signing keys are not available");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        },
    }
}
