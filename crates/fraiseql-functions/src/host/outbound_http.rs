//! The outbound HTTP op, shared by every host surface that has one.
//!
//! Extracted from `LiveHostContext` when the request-serving host gained the same
//! op (#1329). It is one *implementation*, not one rule stated twice: the SSRF
//! allowlist, the literal-IP and DNS-rebinding checks, a client that connects only to
//! the addresses its own lookup checked, the disabled redirect policy (so a 3xx cannot
//! bounce a validated request at an unvalidated internal target) and the response-size
//! ceiling all apply identically wherever a guest can make an
//! outbound call, because there is only one place they are written.
//!
//! #1329 states the constraint as "the SSRF allowlist, unchanged". A second copy of
//! the guard is exactly how it would change.

use std::sync::Arc;

use fraiseql_error::Result;
use fraiseql_guard::net::pinned::PinnedResolver;

use crate::host::{
    HttpResponse,
    live::{HostContextConfig, http_validator},
};

/// The client a guest's request is sent with, built from `config`'s timeouts.
///
/// The URL check sees the URL the guest named, so the client goes there and nowhere
/// else: a 3xx is returned to the guest, not followed; every lookup the client makes is
/// `resolver`'s, which checks the addresses it answers, so it connects only to one that
/// passed; and no proxy is used, through which the lookup would name the proxy instead.
pub(crate) fn outbound_client(
    config: &HostContextConfig,
    resolver: PinnedResolver,
) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .dns_resolver(Arc::new(resolver))
        .no_proxy()
        .connect_timeout(std::time::Duration::from_millis(config.http_connect_timeout_ms))
        .timeout(std::time::Duration::from_millis(config.http_read_timeout_ms))
        .build()
        .map_err(|e| fraiseql_error::FraiseQLError::Internal {
            message: format!("failed to create HTTP client: {e}"),
            source:  None,
        })
}

/// Make one SSRF-validated outbound HTTP request.
///
/// `client` is the host's shared client when it has one, used as given; otherwise
/// `outbound_client` builds one per call from `config`'s timeouts.
///
/// # Errors
///
/// - [`FraiseQLError::Authorization`](fraiseql_error::FraiseQLError::Authorization) or
///   [`Validation`](fraiseql_error::FraiseQLError::Validation) — the URL failed SSRF validation.
/// - [`FraiseQLError::Validation`](fraiseql_error::FraiseQLError::Validation) — an unsupported HTTP
///   method, or a response body over `max_http_response_bytes`.
/// - [`FraiseQLError::Internal`](fraiseql_error::FraiseQLError::Internal) — the client could not be
///   built, the request failed, or the body could not be read.
pub async fn perform(
    config: &HostContextConfig,
    client: Option<&Arc<reqwest::Client>>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<HttpResponse> {
    // Validate URL for SSRF attacks
    let http_config = http_validator::HttpClientConfig {
        allowed_domains:    config.allowed_domains.clone(),
        max_response_bytes: config.max_http_response_bytes,
        connect_timeout_ms: config.http_connect_timeout_ms,
        read_timeout_ms:    config.http_read_timeout_ms,
    };
    // SSRF validation: allowlist + literal-IP + DNS-rebinding checks. Async
    // because it resolves the host before any network contact.
    http_validator::validate_outbound_url(url, &http_config).await?;

    // Get or create HTTP client
    let client = if let Some(client) = client {
        Arc::clone(client)
    } else {
        Arc::new(outbound_client(config, PinnedResolver::new())?)
    };

    // Build request
    let mut req = match method.to_uppercase().as_str() {
        "GET" => client.get(url),
        "POST" => client.post(url),
        "PUT" => client.put(url),
        "PATCH" => client.patch(url),
        "DELETE" => client.delete(url),
        "HEAD" => client.head(url),
        _ => {
            return Err(fraiseql_error::FraiseQLError::Validation {
                message: format!("unsupported HTTP method: {}", method),
                path:    None,
            });
        },
    };

    // Add headers
    for (key, value) in headers {
        req = req.header(key.clone(), value.clone());
    }

    // Add body if present
    if let Some(body_bytes) = body {
        req = req.body(body_bytes.to_vec());
    }

    // Execute request
    let response = req.send().await.map_err(|e| fraiseql_error::FraiseQLError::Internal {
        message: format!("HTTP request failed: {}", e),
        source:  None,
    })?;

    let status = response.status().as_u16();

    // Collect response headers
    let response_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    // Read response body with size limit
    let body_bytes =
        response.bytes().await.map_err(|e| fraiseql_error::FraiseQLError::Internal {
            message: format!("failed to read response body: {}", e),
            source:  None,
        })?;

    if body_bytes.len() > config.max_http_response_bytes {
        return Err(fraiseql_error::FraiseQLError::Validation {
            message: format!(
                "response body too large: {} > {}",
                body_bytes.len(),
                config.max_http_response_bytes
            ),
            path:    None,
        });
    }

    Ok(crate::host::HttpResponse {
        status,
        headers: response_headers,
        body: body_bytes.to_vec(),
    })
}

#[cfg(test)]
mod tests;
