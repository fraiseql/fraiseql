//! Rate limit middleware function and supporting helpers.
//!
//! Contains the axum middleware entry-point and IP extraction logic. The subject a
//! per-user decision is keyed on comes from [`super::identity`], which verifies it —
//! this module never reads an unverified claim.

use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

use super::{
    config::RateLimitConfig,
    dispatch::RateLimiter,
    identity::VerifiedSubject,
    key::{is_private_or_loopback, normalise_ip_key},
};

/// Rate limit middleware response.
///
/// Carries the number of seconds the client should wait before retrying,
/// derived from the active rate-limit configuration at the time the request
/// was rejected.  This value is emitted as both the `Retry-After` HTTP header
/// and in the GraphQL error message body.
#[derive(Debug)]
pub struct RateLimitExceeded {
    /// Seconds until the token bucket refills by at least one token.
    pub retry_after_secs: u32,
}

impl IntoResponse for RateLimitExceeded {
    fn into_response(self) -> Response {
        let retry = self.retry_after_secs;
        let retry_str = retry.to_string();
        let body = format!(
            r#"{{"errors":[{{"message":"Rate limit exceeded. Please retry after {retry} second{s}."}}]}}"#,
            s = if retry == 1 { "" } else { "s" }
        );
        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Content-Type", "application/json"),
                ("Retry-After", retry_str.as_str()),
            ],
            body,
        )
            .into_response()
    }
}

/// Emitted at most once when the server appears to be behind a proxy but
/// `trust_proxy_headers` is `false` — rate limiting would bucket all requests
/// under the proxy's IP in that configuration.
static PROXY_WARNING_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// The client IP a request is rate-limited as: the forwarded client when the peer is a
/// trusted proxy, the peer otherwise.
///
/// Checks `X-Real-IP` first, then the first address in `X-Forwarded-For` (set by
/// the proxy to the original client).  Falls back to the TCP peer address when
/// neither header is present or `trust_proxy` is false.
///
/// Every transport keys its IP limit by this one rule (#1638): it takes the headers
/// and the peer rather than an axum request so gRPC, which never has one, can call it.
/// A request whose peer is unknown and whose headers are not trusted is keyed
/// `unknown`, one bucket for all such requests.
///
/// **Security**: only enable `trust_proxy` when the server is guaranteed to sit
/// behind a proxy that sets these headers; otherwise clients can spoof the IP.
pub fn extract_real_ip(
    headers: &axum::http::HeaderMap,
    trust_proxy: bool,
    trusted_cidrs: &[ipnet::IpNet],
    peer: Option<IpAddr>,
) -> String {
    // The peer always parses; `normalise_ip_key` still applies the IPv6 /64 collapse, so
    // a direct IPv6 client cannot mint a bucket per address either.
    let peer_key = || {
        peer.map_or_else(
            || "unknown".to_string(),
            |ip| normalise_ip_key(&ip.to_string()).unwrap_or_else(|| ip.to_string()),
        )
    };
    if trust_proxy {
        // If trusted_cidrs is non-empty, verify the direct connection IP is a known proxy.
        if !trusted_cidrs.is_empty() {
            let from_trusted_proxy =
                peer.is_some_and(|direct| trusted_cidrs.iter().any(|cidr| cidr.contains(&direct)));
            if !from_trusted_proxy {
                tracing::debug!(
                    direct = ?peer,
                    "Connection not from a trusted proxy CIDR; ignoring X-Forwarded-For"
                );
                return peer_key();
            }
        }

        if let Some(real_ip) = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if let Some(key) = normalise_ip_key(real_ip) {
                return key;
            }
        }
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
                if let Some(key) = normalise_ip_key(first) {
                    return key;
                }
            }
        }
    } else if peer.is_some_and(is_private_or_loopback)
        && !PROXY_WARNING_LOGGED.load(std::sync::atomic::Ordering::Relaxed)
        && !PROXY_WARNING_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        warn!(
            peer_ip = ?peer,
            "Rate limiter: peer address is loopback/RFC-1918 — server appears to be \
             behind a reverse proxy. All requests will share a single rate-limit bucket \
             unless you set `trust_proxy_headers = true` in [security.rate_limiting]."
        );
    }
    peer_key()
}

/// Rate limiting middleware for GraphQL requests.
///
/// Decision order:
/// 1. Per-path limit (auth endpoints) — always checked, uses path-specific window.
/// 2. Per-user limit — when the deployment configured a validator and this request's token
///    **verifies** against it. The subject then gets `rps_per_user` (default 10× `rps_per_ip`)
///    instead of the shared IP bucket, which is the point: many authenticated users behind one
///    egress address are not one client.
/// 3. Per-IP limit — everything else, including a token that fails to verify. A forged JWT
///    therefore cannot mint a bucket, which is what #1143 established and #1171 restored the
///    allowance without giving up.
///
/// # Errors
///
/// Returns `RateLimitExceeded` if the per-path, per-user, or per-IP rate limit is exceeded.
#[allow(clippy::cognitive_complexity)] // Reason: multi-dimension rate limiting (per-path, per-user, per-IP) with config lookups
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, RateLimitExceeded> {
    // Get or create rate limiter from state
    let limiter = req
        .extensions()
        .get::<Arc<RateLimiter>>()
        .cloned()
        .unwrap_or_else(|| Arc::new(RateLimiter::new(RateLimitConfig::default())));

    let ip = extract_real_ip(
        req.headers(),
        limiter.config().trust_proxy_headers,
        &limiter.config().trusted_proxy_cidrs,
        Some(addr.ip()),
    );
    let path = req.uri().path().to_string();

    // #1143: `X-Tenant-ID` is still not read here, and an *unverified* JWT `sub` never
    // will be. Both were folded into bucket keys without validation, and a fresh key is
    // a fresh *full* bucket — so varying either did not merely grow the map, it handed
    // the caller an unlimited budget. Measured then: 50 of 50 requests allowed against
    // `rps_per_ip = 1, burst = 1`, from one IP, unauthenticated.
    //
    // #1171 restores the per-user allowance on the only footing that is not that bypass:
    // a subject the deployment's own validator accepts. `VerifiedSubject::subject`
    // answers `None` for a missing, malformed, unsigned, expired or forged token, and
    // `None` buckets on the IP — infrastructure-derived, and not inflatable by the
    // caller. gRPC has kept its per-user limit throughout, because it authenticates
    // first.
    let verified_subject = match req.extensions().get::<Arc<VerifiedSubject>>() {
        Some(identity) => identity.subject(req.headers()).await,
        None => None,
    };

    // ── Per-path limit (strictest, always enforced) ───────────────────────
    let path_result = limiter.check_path_limit(&path, &ip).await;
    if !path_result.allowed {
        warn!(ip = %ip, path = %path, "Per-path rate limit exceeded");
        return Err(RateLimitExceeded {
            retry_after_secs: path_result.retry_after_secs,
        });
    }

    // ── Per-user limit, or per-IP ─────────────────────────────────────────
    //
    // One or the other, never both: a verified caller's budget must not also be spent
    // from the address it shares with every other caller behind the same proxy, which
    // is the gap #1171 names.
    let (limit_result, limit_for_header) = if let Some(ref subject) = verified_subject {
        let result = limiter.check_user_limit(subject).await;
        if !result.allowed {
            // The subject is verified, so this is safe to log: it names a caller the
            // deployment already authenticated, not a string an anonymous client chose.
            warn!(user_id = %subject, "Per-user rate limit exceeded");
            return Err(RateLimitExceeded {
                retry_after_secs: result.retry_after_secs,
            });
        }
        (result, limiter.config().rps_per_user)
    } else {
        let result = limiter.check_ip_limit(&ip).await;
        if !result.allowed {
            warn!(ip = %ip, "IP rate limit exceeded");
            return Err(RateLimitExceeded {
                retry_after_secs: result.retry_after_secs,
            });
        }
        (result, limiter.config().rps_per_ip)
    };

    let remaining = limit_result.remaining;

    let response = next.run(req).await;

    // Add rate limit headers
    let mut response = response;
    // The header reports the budget this request was actually measured against, which
    // is `rps_per_user` for a verified caller. Reporting `rps_per_ip` to a client whose
    // real allowance is ten times that is a client-visible lie about its own quota.
    let limit = limit_for_header;
    if let Ok(limit_value) = format!("{limit}").parse() {
        response.headers_mut().insert("X-RateLimit-Limit", limit_value);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Reason: remaining tokens is a small non-negative count that fits in u32
    if let Ok(remaining_value) = format!("{}", remaining as u32).parse() {
        response.headers_mut().insert("X-RateLimit-Remaining", remaining_value);
    }

    Ok(response)
}
