//! The one outbound request this crate makes.
//!
//! A `jwks_uri` is operator-supplied configuration pointing at a third party, so
//! it is the shape of request an SSRF guard exists for. Two things are therefore
//! true of every fetch here:
//!
//! * every address the host resolves to is checked against [`fraiseql_guard::net::is_blocked_ip`],
//!   and the connection is **pinned** to exactly those addresses. Checking and then letting the
//!   HTTP client resolve again leaves a rebinding window: the attacker's name answers with a public
//!   address for the check and a private one for the connect.
//! * redirects are refused. A `3xx` is otherwise a way to bounce a pinned request onto an un-pinned
//!   internal target.

use std::net::SocketAddr;

use crate::{FETCH_TIMEOUT, JwkSet, JwksError, MAX_RESPONSE_BYTES};

/// A validated JWKS endpoint.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// The URI as configured, used verbatim for the request and for every message.
    uri:         String,
    /// The host to resolve and pin.
    host:        String,
    /// The port to resolve on.
    port:        u16,
    /// Whether the host is a loopback literal, in which case the address guard is
    /// not applied — a local fixture or development `IdP` *is* the private address
    /// the guard exists to refuse, and `parse` has already restricted plain HTTP
    /// to this case.
    is_loopback: bool,
}

/// Whether a host is one of the loopback spellings a local `IdP` is reached by.
///
/// Literal spellings only. A name that merely *resolves* to loopback is not
/// exempt: it goes through [`fraiseql_guard::net::is_blocked_ip`] like any other,
/// which is what refuses it.
fn is_loopback_literal(host: &str) -> bool {
    matches!(host.to_ascii_lowercase().as_str(), "localhost" | "127.0.0.1" | "::1" | "[::1]")
}

impl Endpoint {
    /// Validate a `jwks_uri` and remember what a fetch will need.
    ///
    /// # Errors
    ///
    /// [`JwksError::InvalidUrl`] when it is not a URL, and
    /// [`JwksError::InvalidScheme`] when it is not `https` — or `http` on a
    /// loopback host, which is how a local fixture and a development `IdP` are
    /// reached.
    pub fn parse(uri: &str) -> Result<Self, JwksError> {
        let parsed = url::Url::parse(uri).map_err(|source| JwksError::InvalidUrl {
            uri: uri.to_string(),
            source,
        })?;
        let host = parsed
            .host_str()
            .ok_or_else(|| JwksError::InvalidScheme {
                scheme: parsed.scheme().to_string(),
            })?
            .to_string();
        let is_loopback = is_loopback_literal(&host);
        let permitted = match parsed.scheme() {
            "https" => true,
            "http" => is_loopback,
            _ => false,
        };
        if !permitted {
            return Err(JwksError::InvalidScheme {
                scheme: parsed.scheme().to_string(),
            });
        }
        // Reason: only `https` (443) and `http` (80) reach here, and both have a
        // known default port, so the fallback is unreachable.
        let port = parsed.port_or_known_default().unwrap_or(443);
        Ok(Self {
            uri: uri.to_string(),
            host,
            port,
            is_loopback,
        })
    }

    /// The URI as configured.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// The key set this endpoint publishes.
    ///
    /// # Errors
    ///
    /// [`JwksError::Client`] when the host cannot be resolved, resolves to an
    /// address the guard refuses, or the pinned client cannot be built;
    /// [`JwksError::Unreachable`] when the request fails or the publisher answers
    /// with an error status; [`JwksError::TooLarge`] past
    /// [`MAX_RESPONSE_BYTES`]; [`JwksError::Malformed`] when the body is not a
    /// JWKS document.
    pub async fn fetch(&self) -> Result<JwkSet, JwksError> {
        tracing::debug!(uri = %self.uri, "fetching JWKS");
        let client = self.client().await?;
        let response =
            client.get(&self.uri).send().await.map_err(|error| JwksError::Unreachable {
                uri:    self.uri.clone(),
                reason: error.to_string(),
            })?;
        if !response.status().is_success() {
            return Err(JwksError::Unreachable {
                uri:    self.uri.clone(),
                reason: format!("the publisher answered {}", response.status()),
            });
        }
        let body = response.bytes().await.map_err(|error| JwksError::Unreachable {
            uri:    self.uri.clone(),
            reason: format!("the response body could not be read: {error}"),
        })?;
        // Capped before deserialising, not after: the point is to not allocate the
        // parse of an oversized document.
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(JwksError::TooLarge {
                uri:   self.uri.clone(),
                bytes: body.len(),
            });
        }
        let set: JwkSet = serde_json::from_slice(&body).map_err(|error| JwksError::Malformed {
            uri:    self.uri.clone(),
            reason: error.to_string(),
        })?;
        tracing::debug!(uri = %self.uri, key_count = set.keys.len(), "JWKS fetched");
        Ok(set)
    }

    /// The client for one fetch: resolved, guarded and pinned, unless the host is
    /// a loopback literal.
    async fn client(&self) -> Result<reqwest::Client, JwksError> {
        let client_error = |reason: String| JwksError::Client {
            uri: self.uri.clone(),
            reason,
        };
        let addrs = if self.is_loopback {
            Vec::new()
        } else {
            resolve_and_guard(&self.host, self.port).await.map_err(client_error)?
        };
        pinned_client(&self.host, &addrs, FETCH_TIMEOUT)
            .map_err(|error| client_error(format!("the HTTP client could not be built: {error}")))
    }
}

/// Resolve `host` and refuse every address the shared guard blocks.
///
/// The returned addresses must be **pinned** into the client that connects — see
/// [`pinned_client`]. Validating here and then letting the HTTP client resolve
/// again independently is the DNS-rebinding window: the attacker's name answers
/// with a public address for this check and a private one for the connect.
///
/// # Errors
///
/// A message naming what happened, for the caller to wrap: resolution failed,
/// resolved to nothing, or resolved to an address the guard refuses.
pub async fn resolve_and_guard(host: &str, port: u16) -> Result<Vec<SocketAddr>, String> {
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("DNS resolution failed for {host:?}: {error}"))?
        .collect();
    if addrs.is_empty() {
        return Err(format!("DNS resolved {host:?} to no addresses"));
    }
    for addr in &addrs {
        if fraiseql_guard::net::is_blocked_ip(&addr.ip()) {
            return Err(format!(
                "{host:?} resolves to {}, which is not an address a published key set may be \
                 fetched from",
                addr.ip()
            ));
        }
    }
    Ok(addrs)
}

/// A client pinned to pre-validated addresses, with redirects refused.
///
/// `addrs` must already have passed [`resolve_and_guard`]. Pinning is what closes
/// the rebinding window: reqwest cannot re-resolve `host` to something else
/// between the check and the connect. An empty `addrs` leaves resolution to
/// reqwest, which is correct only for the loopback literals that skip the guard.
///
/// Redirects are refused because a `3xx` would otherwise bounce a pinned request
/// onto an un-pinned internal target.
pub fn pinned_client(
    host: &str,
    addrs: &[SocketAddr],
    timeout: std::time::Duration,
) -> Result<reqwest::Client, reqwest::Error> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none());
    if !addrs.is_empty() {
        builder = builder.resolve_to_addrs(host, addrs);
    }
    builder.build()
}
