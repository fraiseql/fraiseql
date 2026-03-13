//! Rate limit middleware function and supporting helpers.
//!
//! Contains the axum middleware entry-point, IP extraction logic, and the
//! JWT subject parser used for per-user rate limiting.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::warn;

use super::{config::RateLimitConfig, dispatch::RateLimiter, key::is_private_or_loopback};
use std::net::IpAddr;

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

/// Extract the real client IP from request headers when behind a trusted reverse proxy.
///
/// Checks `X-Real-IP` first, then the first address in `X-Forwarded-For` (set by
/// the proxy to the original client).  Falls back to the TCP peer address when
/// neither header is present or `trust_proxy` is false.
///
/// **Security**: only enable `trust_proxy` when the server is guaranteed to sit
/// behind a proxy that sets these headers; otherwise clients can spoof the IP.
pub(super) fn extract_real_ip(
    req: &Request<Body>,
    trust_proxy: bool,
    trusted_cidrs: &[ipnet::IpNet],
    addr: &SocketAddr,
) -> String {
    if trust_proxy {
        // If trusted_cidrs is non-empty, verify the direct connection IP is a known proxy.
        if !trusted_cidrs.is_empty() {
            let direct: IpAddr = addr.ip();
            let from_trusted_proxy = trusted_cidrs.iter().any(|cidr| cidr.contains(&direct));
            if !from_trusted_proxy {
                tracing::debug!(
                    %direct,
                    "Connection not from a trusted proxy CIDR; ignoring X-Forwarded-For"
                );
                return direct.to_string();
            }
        }

        if let Some(real_ip) = req
            .headers()
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return real_ip.to_string();
        }
        if let Some(xff) = req.headers().get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next().map(str::trim).filter(|s| !s.is_empty()) {
                return first.to_string();
            }
        }
    } else if is_private_or_loopback(addr.ip())
        && !PROXY_WARNING_LOGGED.load(std::sync::atomic::Ordering::Relaxed)
        && !PROXY_WARNING_LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        warn!(
            peer_ip = %addr.ip(),
            "Rate limiter: peer address is loopback/RFC-1918 — server appears to be \
             behind a reverse proxy. All requests will share a single rate-limit bucket \
             unless you set `trust_proxy_headers = true` in [security.rate_limiting]."
        );
    }
    addr.ip().to_string()
}

/// Decode a JWT bearer token's payload section and extract the `sub` claim
/// without performing cryptographic signature verification.
///
/// Signature verification is intentionally omitted: rate limiting is a
/// best-effort control that degrades gracefully — an invalid or forged JWT
/// simply returns `None`, falling back to IP-based limiting.  Verified
/// identity is handled by the auth middleware upstream.
pub(super) fn extract_jwt_subject(authorization: &str) -> Option<String> {
    use base64::Engine as _;
    let token = authorization.strip_prefix("Bearer ")?;
    let payload_b64 = token.split('.').nth(1)?;
    let decoded =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    json.get("sub").and_then(|v| v.as_str()).map(String::from)
}

/// Rate limiting middleware for GraphQL requests.
///
/// Decision order:
/// 1. Per-path limit (auth endpoints) — always checked, uses path-specific window.
/// 2. Per-user limit (authenticated requests) — checked when a JWT `sub` claim is
///    present in the `Authorization` header; authenticated users get `rps_per_user`
///    (default 10× `rps_per_ip`) instead of the shared IP bucket.
/// 3. Per-IP limit (unauthenticated or no bearer token) — fallback.
///
/// # Errors
///
/// Returns `RateLimitExceeded` if the per-path, per-user, or per-IP rate limit is exceeded.
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
        &req,
        limiter.config().trust_proxy_headers,
        &limiter.config().trusted_proxy_cidrs,
        &addr,
    );
    let path = req.uri().path().to_string();

    // Extract JWT subject for per-user limiting (no signature verification needed here).
    let user_id = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_jwt_subject);

    // ── Per-path limit (strictest, always enforced) ───────────────────────
    let path_result = limiter.check_path_limit(&path, &ip).await;
    if !path_result.allowed {
        warn!(ip = %ip, path = %path, "Per-path rate limit exceeded");
        return Err(RateLimitExceeded { retry_after_secs: path_result.retry_after_secs });
    }

    // ── Per-user or per-IP limit ──────────────────────────────────────────
    let limit_result = if let Some(ref uid) = user_id {
        // Authenticated: apply the higher per-user bucket.
        limiter.check_user_limit(uid).await
    } else {
        // Unauthenticated: apply the shared IP bucket.
        limiter.check_ip_limit(&ip).await
    };

    if !limit_result.allowed {
        if let Some(ref uid) = user_id {
            warn!(user_id = %uid, "Per-user rate limit exceeded");
        } else {
            warn!(ip = %ip, "IP rate limit exceeded");
        }
        return Err(RateLimitExceeded { retry_after_secs: limit_result.retry_after_secs });
    }

    let remaining = limit_result.remaining;

    let response = next.run(req).await;

    // Add rate limit headers
    let mut response = response;
    let limit = if user_id.is_some() {
        limiter.config().rps_per_user
    } else {
        limiter.config().rps_per_ip
    };
    if let Ok(limit_value) = format!("{limit}").parse() {
        response.headers_mut().insert("X-RateLimit-Limit", limit_value);
    }
    if let Ok(remaining_value) = format!("{}", remaining as u32).parse() {
        response.headers_mut().insert("X-RateLimit-Remaining", remaining_value);
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics acceptable

    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use axum::{body::Body, http::Request};

    use super::extract_real_ip;

    fn socket_addr(ip: [u8; 4]) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), 12345)
    }

    fn req_with_xff(xff: &str) -> Request<Body> {
        Request::builder()
            .uri("http://example.com/graphql")
            .header("x-forwarded-for", xff)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn test_spoofed_xforwardedfor_ignored_when_direct_ip_not_in_trusted_cidrs() {
        // Direct IP is a public internet address — NOT in the trusted proxy CIDR 10.0.0.0/8.
        // Even though trust_proxy_headers = true, X-Forwarded-For must be ignored.
        let cidrs: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let addr = socket_addr([203, 0, 113, 1]); // TEST-NET-3, public
        let req = req_with_xff("1.2.3.4");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "203.0.113.1", "Should use direct IP, not spoofed X-Forwarded-For");
    }

    #[test]
    fn test_forwarded_ip_used_when_direct_ip_is_trusted_proxy() {
        // Direct IP is inside 10.0.0.0/8 (trusted proxy CIDR).
        // X-Forwarded-For should be honoured.
        let cidrs: Vec<ipnet::IpNet> = vec!["10.0.0.0/8".parse().unwrap()];
        let addr = socket_addr([10, 0, 1, 5]); // inside trusted CIDR
        let req = req_with_xff("5.6.7.8");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "5.6.7.8", "Should use X-Forwarded-For from trusted proxy");
    }

    #[test]
    fn test_no_cidrs_trusts_all_proxies() {
        // When trusted_proxy_cidrs is empty and trust_proxy_headers = true,
        // all direct IPs are treated as trusted proxies.
        let cidrs: Vec<ipnet::IpNet> = vec![];
        let addr = socket_addr([203, 0, 113, 1]); // public IP
        let req = req_with_xff("9.9.9.9");

        let ip = extract_real_ip(&req, true, &cidrs, &addr);
        assert_eq!(ip, "9.9.9.9", "Empty CIDRs: all proxies trusted");
    }
}
