//! Operator-facing JWKS cache control (#361).
//!
//! `POST /admin/v1/auth/refresh-jwks` lets an operator force an immediate JWKS
//! refetch in response to a known IdP-side key compromise, closing the stolen-key
//! replay window without waiting up to `jwks_cache_ttl_secs` for the cache to
//! expire or restarting every replica. The route is mounted behind the admin
//! bearer token (see `server/routing/admin.rs`).

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode};
use fraiseql_core::security::OidcValidator;
use serde_json::json;
use tracing::{info, warn};

/// Force an immediate JWKS refetch, replacing the cached key set.
///
/// On success the cache holds the provider's current keys; any rotated-out
/// (potentially compromised) key is evicted. If the provider cannot be reached,
/// the cache is still invalidated (fail-closed) so the next token validation
/// refetches rather than trusting the stale, possibly-compromised cache.
///
/// # Responses
///
/// - `200` — refetch succeeded; body reports the number of keys fetched.
/// - `502` — provider unreachable; the cache was invalidated and keys will refetch lazily on the
///   next request.
pub async fn refresh_jwks_handler(
    State(validator): State<Arc<OidcValidator>>,
) -> (StatusCode, Json<serde_json::Value>) {
    match validator.refresh_jwks().await {
        Ok(key_count) => {
            info!(key_count, "JWKS cache force-refreshed via /admin/v1/auth/refresh-jwks");
            (
                StatusCode::OK,
                Json(json!({
                    "refreshed": true,
                    "key_count": key_count,
                })),
            )
        },
        Err(e) => {
            // Provider unreachable — drop the (possibly compromised) cache anyway so
            // rotated-out keys stop validating; the next request refetches lazily.
            validator.invalidate_jwks_cache();
            warn!(
                error = %e,
                "JWKS force-refresh could not reach the provider; cache invalidated (fail-closed)"
            );
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({
                    "refreshed": false,
                    "cache_invalidated": true,
                    "error": "failed to fetch JWKS from the provider; cache invalidated, keys \
                              will be refetched on the next token validation",
                })),
            )
        },
    }
}
