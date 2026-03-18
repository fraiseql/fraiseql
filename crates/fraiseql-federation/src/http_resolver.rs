//! HTTP entity resolution for federated subgraphs.
//!
//! Resolves entities from remote GraphQL subgraphs via HTTP POST requests
//! to their `_entities` endpoint. Includes retry logic, timeout handling,
//! and error recovery.

/// Maximum byte size for a federation entity resolution response.
///
/// `_entities` responses contain resolved entity fields, not bulk data.
/// 50 `MiB` is generous while preventing allocation-bomb payloads from
/// a compromised or misconfigured subgraph.
const MAX_ENTITY_RESPONSE_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

use std::time::Duration;

use fraiseql_error::{GraphQLError, Result};
use serde_json::{Value, json};

use crate::{
    selection_parser::FieldSelection, tracing::FederationTraceContext, types::EntityRepresentation,
};

/// Configuration for HTTP client behavior
#[derive(Debug, Clone)]
pub struct HttpClientConfig {
    /// Request timeout in milliseconds
    pub timeout_ms:     u64,
    /// Maximum number of retry attempts
    pub max_retries:    u32,
    /// Initial delay between retries in milliseconds (exponential backoff)
    pub retry_delay_ms: u64,
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            timeout_ms:     5000,
            max_retries:    3,
            retry_delay_ms: 100,
        }
    }
}

/// HTTP entity resolver
#[derive(Clone)]
pub struct HttpEntityResolver {
    client:    reqwest::Client,
    config:    HttpClientConfig,
    /// When `true`, URL validation is skipped. Only settable in test code.
    #[cfg(any(test, feature = "test-utils"))]
    skip_ssrf: bool,
}

#[derive(serde::Serialize)]
struct GraphQLRequest {
    query:     String,
    variables: Value,
}

#[derive(serde::Deserialize, Debug)]
struct GraphQLResponse {
    data:   Option<Value>,
    errors: Option<Vec<GraphQLError>>,
}

/// Validate that a subgraph URL is safe to contact.
///
/// Blocks SSRF attacks by:
/// 1. Requiring `https://` scheme by default; `http://` is allowed only when the environment
///    variable `FRAISEQL_FEDERATION_ALLOW_INSECURE=true` is set.
/// 2. Blocking `localhost` and `.localhost` hostnames.
/// 3. Blocking literal private/reserved IP addresses (RFC 1918, loopback, link-local, CGNAT, ULA,
///    IPv4-mapped IPv6).
///
/// Note: DNS-level SSRF (attacker-controlled domain that resolves to a
/// private IP) is not mitigated here; that requires egress filtering at the
/// network layer.
///
/// # Errors
///
/// Returns `FraiseQLError::Internal` if the scheme, host, or IP is forbidden.
pub fn validate_subgraph_url(url: &str) -> fraiseql_error::Result<()> {
    // When `FRAISEQL_FEDERATION_ALLOW_INSECURE=true` all SSRF guards are disabled.
    // This is intended for local development and testing only — never set in production.
    let allow_insecure = std::env::var("FRAISEQL_FEDERATION_ALLOW_INSECURE")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    // Require https:// by default; allow http:// only when insecure mode is opt-in.
    if url.starts_with("https://") {
        // always allowed
    } else if url.starts_with("http://") {
        if !allow_insecure {
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: "Subgraph URL must use https:// scheme (got http://). \
                          Set FRAISEQL_FEDERATION_ALLOW_INSECURE=true to permit plain HTTP \
                          in development environments."
                    .to_string(),
                source:  None,
            });
        }
    } else {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("Subgraph URL must use https:// scheme (got: {url})"),
            source:  None,
        });
    }

    // When insecure mode is enabled, skip IP/hostname checks too (dev/test only).
    if allow_insecure {
        return Ok(());
    }

    // Parse the full URL to extract the host safely — manual string splitting
    // is fragile in the presence of IPv6 literals and non-standard authority forms.
    let parsed = reqwest::Url::parse(url).map_err(|e| fraiseql_error::FraiseQLError::Internal {
        message: format!("Subgraph URL is not a valid URL ({url}): {e}"),
        source:  None,
    })?;

    let host_raw = parsed.host_str().unwrap_or("");

    if host_raw.is_empty() {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("Subgraph URL has no host: {url}"),
            source:  None,
        });
    }

    // The `url` crate wraps IPv6 literals in brackets in `host_str()` (e.g. "[::1]").
    // Strip them before parsing to `IpAddr` so IPv6 SSRF checks work correctly.
    let host = if host_raw.starts_with('[') && host_raw.ends_with(']') {
        &host_raw[1..host_raw.len() - 1]
    } else {
        host_raw
    };

    // Block loopback hostnames.
    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("Subgraph URL targets a loopback host: {host}"),
            source:  None,
        });
    }

    // Block literal private/reserved IP addresses.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_ssrf_blocked_ip(&ip) {
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: format!(
                    "Subgraph URL targets a private or reserved IP address ({ip}) — \
                     SSRF protection blocked the request"
                ),
                source:  None,
            });
        }
    }

    Ok(())
}

/// Returns `true` for IP addresses the federation engine must not contact.
///
/// Covers: loopback (127/8, ::1), RFC 1918 (10/8, 172.16/12, 192.168/16),
/// link-local (169.254/16, fe80::/10), CGNAT (100.64/10), unspecified (0.0.0.0),
/// IPv4-mapped IPv6 (::ffff:0:0/96), and ULA (fc00::/7).
pub fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                                  // loopback
                || o[0] == 10                                            // RFC 1918 /8
                || (o[0] == 172 && (16..=31).contains(&o[1]))            // RFC 1918 /12
                || (o[0] == 192 && o[1] == 168)                          // RFC 1918 /16
                || (o[0] == 169 && o[1] == 254)                          // link-local
                || (o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000) // CGNAT RFC 6598
                || o[0] == 0 // unspecified
        },
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            *v6 == std::net::Ipv6Addr::LOCALHOST                         // ::1
                || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0
                    && s[4] == 0 && s[5] == 0xffff)                      // IPv4-mapped
                || (s[0] & 0xfe00) == 0xfc00                             // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        },
    }
}

/// Resolve the host via DNS and reject if any address is private/reserved.
///
/// Prevents DNS rebinding attacks where an attacker-controlled domain initially
/// resolves to a public IP (passing URL validation) but later resolves to a
/// private IP during the actual HTTP request.
///
/// # Errors
///
/// Returns `FraiseQLError::Internal` if DNS resolution fails, returns no
/// addresses, or any resolved address is in a private/reserved range.
async fn dns_resolve_and_check(url: &str) -> fraiseql_error::Result<()> {
    let parsed = reqwest::Url::parse(url).map_err(|e| fraiseql_error::FraiseQLError::Internal {
        message: format!("Invalid URL '{url}': {e}"),
        source:  None,
    })?;
    let host = parsed.host_str().ok_or_else(|| fraiseql_error::FraiseQLError::Internal {
        message: format!("URL has no host: {url}"),
        source:  None,
    })?;
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| fraiseql_error::FraiseQLError::Internal {
            message: format!("DNS resolution failed for host '{host}': {e}"),
            source:  None,
        })?
        .collect();
    if addrs.is_empty() {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("DNS resolved to no addresses for host '{host}'"),
            source:  None,
        });
    }
    for addr in &addrs {
        if is_ssrf_blocked_ip(&addr.ip()) {
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: format!(
                    "DNS rebinding attack blocked: host '{host}' resolved to private/reserved IP {}",
                    addr.ip()
                ),
                source:  None,
            });
        }
    }
    Ok(())
}

impl HttpEntityResolver {
    /// Create a new HTTP entity resolver.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the HTTP client cannot be initialised
    /// (e.g., invalid TLS configuration).
    pub fn new(config: HttpClientConfig) -> fraiseql_error::Result<Self> {
        // Redirects are disabled to prevent redirect-chain SSRF attacks:
        // a compromised subgraph could redirect to an internal network address,
        // bypassing the URL-parse SSRF guard applied to the initial URL only.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("HTTP client initialisation failed for federation resolver: {e}"),
                source:  None,
            })?;

        Ok(Self {
            client,
            config,
            #[cfg(any(test, feature = "test-utils"))]
            skip_ssrf: false,
        })
    }

    /// Create a resolver that skips SSRF URL validation.
    ///
    /// **Only available with the `test-utils` feature or in unit-test builds.**
    /// Use to contact loopback/mock servers in integration tests without setting
    /// process-global environment variables.
    ///
    /// **Never use in production** — this bypasses all SSRF protections.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the HTTP client fails to initialize.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_test(config: HttpClientConfig) -> fraiseql_error::Result<Self> {
        // No https_only in test mode to allow contacting loopback mock servers over HTTP.
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| fraiseql_error::FraiseQLError::Internal {
                message: format!("HTTP client init failed: {e}"),
                source:  None,
            })?;
        Ok(Self {
            client,
            config,
            skip_ssrf: true,
        })
    }

    /// Resolve entities via HTTP _entities query
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError` if the HTTP request fails or the response cannot be parsed.
    pub async fn resolve_entities(
        &self,
        subgraph_url: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<Vec<Option<Value>>> {
        self.resolve_entities_with_tracing(subgraph_url, representations, selection, None)
            .await
    }

    /// Resolve entities via HTTP _entities query with optional distributed tracing.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError` if URL validation, the HTTP request, or response parsing fails.
    #[tracing::instrument(skip_all, fields(subgraph.url = subgraph_url))]
    pub async fn resolve_entities_with_tracing(
        &self,
        subgraph_url: &str,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
        _trace_context: Option<FederationTraceContext>,
    ) -> Result<Vec<Option<Value>>> {
        if representations.is_empty() {
            return Ok(Vec::new());
        }

        // SECURITY: Validate URL before any network contact to prevent SSRF.
        // In test/test-utils builds, `skip_ssrf` allows contacting local mock servers.
        #[cfg(not(any(test, feature = "test-utils")))]
        {
            validate_subgraph_url(subgraph_url)?;
            dns_resolve_and_check(subgraph_url).await?;
        }
        #[cfg(any(test, feature = "test-utils"))]
        if !self.skip_ssrf {
            validate_subgraph_url(subgraph_url)?;
            dns_resolve_and_check(subgraph_url).await?;
        }

        // Build GraphQL _entities query
        let query = self.build_entities_query(representations, selection)?;

        // Execute with retry
        let response = self.execute_with_retry(subgraph_url, &query).await?;

        // Parse response
        self.parse_response(&response, representations)
    }

    fn build_entities_query(
        &self,
        representations: &[EntityRepresentation],
        selection: &FieldSelection,
    ) -> Result<GraphQLRequest> {
        // Group representations by typename
        let mut typename_fields: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        for rep in representations {
            typename_fields.entry(rep.typename.clone()).or_default();
        }

        // Build inline fragments for each type
        let mut inline_fragments = Vec::new();
        for typename in typename_fields.keys() {
            let fields = selection.fields.join(" ");
            inline_fragments.push(format!("... on {} {{ {} }}", typename, fields));
        }

        // Build the complete query
        let query = format!(
            "query($representations: [_Any!]!) {{ _entities(representations: $representations) {{ {} }} }}",
            inline_fragments.join(" ")
        );

        // Serialize representations as variables
        let repr_values: Vec<Value> = representations
            .iter()
            .map(|rep| {
                let mut obj = rep.all_fields.clone();
                obj.insert("__typename".to_string(), Value::String(rep.typename.clone()));
                Value::Object(obj.into_iter().collect::<serde_json::Map<_, _>>())
            })
            .collect();

        Ok(GraphQLRequest {
            query,
            variables: json!({ "representations": repr_values }),
        })
    }

    async fn execute_with_retry(
        &self,
        url: &str,
        request: &GraphQLRequest,
    ) -> Result<GraphQLResponse> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.config.max_retries {
            attempts += 1;

            match self.client.post(url).json(request).send().await {
                Ok(response) if response.status().is_success() => match response.bytes().await {
                    Ok(body) if body.len() > MAX_ENTITY_RESPONSE_BYTES => {
                        last_error = Some(format!(
                            "Entity response too large ({} bytes, max {MAX_ENTITY_RESPONSE_BYTES})",
                            body.len()
                        ));
                    },
                    Ok(body) => match serde_json::from_slice::<GraphQLResponse>(&body) {
                        Ok(gql_response) => return Ok(gql_response),
                        Err(e) => {
                            last_error = Some(format!("Failed to parse response: {}", e));
                        },
                    },
                    Err(e) => {
                        last_error = Some(format!("Failed to read response: {}", e));
                    },
                },
                Ok(response) => {
                    last_error = Some(format!("HTTP {}", response.status()));
                },
                Err(e) => {
                    last_error = Some(format!("Request failed: {}", e));
                },
            }

            // Exponential backoff
            if attempts < self.config.max_retries {
                let delay = Duration::from_millis(
                    self.config.retry_delay_ms * 2_u64.saturating_pow(attempts - 1),
                );
                tokio::time::sleep(delay).await;
            }
        }

        Err(fraiseql_error::FraiseQLError::Internal {
            message: format!(
                "HTTP resolution failed after {} attempts: {}",
                attempts,
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ),
            source:  None,
        })
    }

    fn parse_response(
        &self,
        response: &GraphQLResponse,
        representations: &[EntityRepresentation],
    ) -> Result<Vec<Option<Value>>> {
        // Check for GraphQL errors
        if let Some(errors) = &response.errors {
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: format!("GraphQL errors: {}", error_messages.join("; ")),
                source:  None,
            });
        }

        // Extract entities from response
        let entities = response
            .data
            .as_ref()
            .and_then(|d| d.get("_entities"))
            .and_then(|e| e.as_array())
            .cloned()
            .unwrap_or_default();

        if entities.len() != representations.len() {
            return Err(fraiseql_error::FraiseQLError::Internal {
                message: format!(
                    "Entity count mismatch: expected {}, got {}",
                    representations.len(),
                    entities.len()
                ),
                source:  None,
            });
        }

        // Return entities in same order as representations
        Ok(entities.into_iter().map(Some).collect())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable

    use std::collections::HashMap;

    use super::*;

    fn mock_representation(typename: &str, id: &str) -> EntityRepresentation {
        let mut key_fields = HashMap::new();
        key_fields.insert("id".to_string(), Value::String(id.to_string()));

        let mut all_fields = key_fields.clone();
        all_fields.insert("__typename".to_string(), Value::String(typename.to_string()));

        EntityRepresentation {
            typename: typename.to_string(),
            key_fields,
            all_fields,
        }
    }

    // ── SSRF / URL validation ─────────────────────────────────────────────────

    #[test]
    fn test_subgraph_url_allows_public_https() {
        validate_subgraph_url("https://api.example.com/graphql")
            .unwrap_or_else(|e| panic!("public HTTPS URL should be allowed: {e}"));
        validate_subgraph_url("https://subgraph.mycompany.io/")
            .unwrap_or_else(|e| panic!("public HTTPS URL should be allowed: {e}"));
    }

    #[test]
    fn test_subgraph_url_rejects_http_scheme_by_default() {
        // http:// must be rejected unless FRAISEQL_FEDERATION_ALLOW_INSECURE=true
        let result = validate_subgraph_url("http://api.example.com/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for http:// scheme, got: {result:?}"
        );
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("https://") || msg.contains("FRAISEQL_FEDERATION_ALLOW_INSECURE"));
    }

    #[test]
    fn test_subgraph_url_rejects_non_http_scheme() {
        let result = validate_subgraph_url("ftp://example.com/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for ftp:// scheme, got: {result:?}"
        );
        let result = validate_subgraph_url("file:///etc/passwd");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for file:// scheme, got: {result:?}"
        );
        let result = validate_subgraph_url("no-scheme-at-all");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for missing scheme, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_loopback() {
        let result = validate_subgraph_url("https://localhost/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for localhost, got: {result:?}"
        );
        let result = validate_subgraph_url("https://localhost:8080/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for localhost:8080, got: {result:?}"
        );
        let result = validate_subgraph_url("https://sub.localhost/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for sub.localhost, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_loopback_ip() {
        let result = validate_subgraph_url("https://127.0.0.1/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for 127.0.0.1, got: {result:?}"
        );
        let result = validate_subgraph_url("https://127.255.255.255/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for 127.255.255.255, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_private_ranges() {
        for url in [
            "https://10.0.0.1/graphql",
            "https://172.16.0.1/graphql",
            "https://172.31.255.255/graphql",
            "https://192.168.1.1/graphql",
        ] {
            let result = validate_subgraph_url(url);
            assert!(
                matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
                "expected Internal error for private IP in {url}, got: {result:?}"
            );
        }
    }

    #[test]
    fn test_subgraph_url_rejects_link_local() {
        let result = validate_subgraph_url("https://169.254.0.1/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for link-local 169.254.0.1, got: {result:?}"
        );
        let result = validate_subgraph_url("https://169.254.169.254/graphql"); // AWS metadata
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for link-local 169.254.169.254, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_cgnat() {
        let result = validate_subgraph_url("https://100.64.0.1/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for CGNAT 100.64.0.1, got: {result:?}"
        );
        let result = validate_subgraph_url("https://100.127.255.255/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for CGNAT 100.127.255.255, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_ipv6_loopback() {
        let result = validate_subgraph_url("https://[::1]/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for IPv6 loopback, got: {result:?}"
        );
    }

    #[test]
    fn test_subgraph_url_rejects_ipv6_ula() {
        let result = validate_subgraph_url("https://[fc00::1]/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for IPv6 ULA fc00::1, got: {result:?}"
        );
        let result = validate_subgraph_url("https://[fd00::1]/graphql");
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for IPv6 ULA fd00::1, got: {result:?}"
        );
    }

    // ── Existing tests (updated for new() returning Result) ───────────────────

    #[test]
    fn test_http_resolver_creation() {
        let config = HttpClientConfig::default();
        let _resolver = HttpEntityResolver::new(config).unwrap();
    }

    #[test]
    fn test_empty_representations() {
        // Empty representations return early (no URL contact) — https:// check not triggered.
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let result = resolver
                .resolve_entities("https://example.com/graphql", &[], &FieldSelection::default())
                .await;

            let entities = result
                .unwrap_or_else(|e| panic!("empty representations should succeed: {e}"));
            assert_eq!(entities.len(), 0);
        });
    }

    #[test]
    fn test_graphql_query_building() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let reps = vec![mock_representation("User", "123")];
        let selection = FieldSelection {
            fields: vec!["id".to_string(), "email".to_string()],
        };

        let request = resolver.build_entities_query(&reps, &selection).unwrap();

        assert!(request.query.contains("_entities"));
        assert!(request.query.contains("_Any!"));
        assert!(request.query.contains("User"));
        assert!(request.query.contains("id"));
        assert!(request.query.contains("email"));
    }

    #[test]
    fn test_multiple_types_in_query() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let reps = vec![
            mock_representation("User", "123"),
            mock_representation("Order", "456"),
        ];
        let selection = FieldSelection {
            fields: vec!["id".to_string()],
        };

        let request = resolver.build_entities_query(&reps, &selection).unwrap();

        assert!(request.query.contains("User"));
        assert!(request.query.contains("Order"));
    }

    #[test]
    fn test_response_parsing_success() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let representations = vec![mock_representation("User", "123")];

        let response = GraphQLResponse {
            data:   Some(json!({
                "_entities": [
                    { "id": "123", "email": "user@example.com" }
                ]
            })),
            errors: None,
        };

        let entities = resolver
            .parse_response(&response, &representations)
            .unwrap_or_else(|e| panic!("parse_response should succeed for valid response: {e}"));
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }

    #[test]
    fn test_response_parsing_with_errors() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let representations = vec![mock_representation("User", "123")];

        let response = GraphQLResponse {
            data:   None,
            errors: Some(vec![GraphQLError::new("Entity not found")]),
        };

        let result = resolver.parse_response(&response, &representations);
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for GraphQL errors in response, got: {result:?}"
        );
    }

    #[test]
    fn test_response_parsing_entity_count_mismatch() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default()).unwrap();
        let representations = vec![
            mock_representation("User", "123"),
            mock_representation("User", "456"),
        ];

        let response = GraphQLResponse {
            data:   Some(json!({
                "_entities": [
                    { "id": "123" }
                ]
            })),
            errors: None,
        };

        let result = resolver.parse_response(&response, &representations);
        assert!(
            matches!(result, Err(fraiseql_error::FraiseQLError::Internal { .. })),
            "expected Internal error for entity count mismatch, got: {result:?}"
        );
    }

    #[test]
    fn test_config_defaults() {
        let config = HttpClientConfig::default();
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 100);
    }

    #[test]
    fn test_config_custom() {
        let config = HttpClientConfig {
            timeout_ms:     10000,
            max_retries:    5,
            retry_delay_ms: 200,
        };
        assert_eq!(config.timeout_ms, 10000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 200);
    }

    // ── URL-parser-based SSRF host extraction ─────────────────────────────────

    #[test]
    fn test_subgraph_url_rejects_ipv6_loopback_via_brackets() {
        // An attacker crafted URL with IPv6 loopback — the old split-based parser
        // was fragile against bracket notation; the url-crate parser is not.
        let result = validate_subgraph_url("https://[::1]/endpoint");
        assert!(result.is_err(), "IPv6 loopback must be rejected: {result:?}");
    }

    #[test]
    fn test_subgraph_url_rejects_ipv6_private() {
        // fc00::/7 ULA — private range.
        let result = validate_subgraph_url("https://[fc00::1]/endpoint");
        assert!(result.is_err(), "IPv6 ULA must be rejected: {result:?}");
    }

    #[test]
    fn test_subgraph_url_malformed_is_rejected() {
        let result = validate_subgraph_url("https://");
        assert!(result.is_err(), "URL with empty host must be rejected");
    }

    #[test]
    fn test_subgraph_url_accepts_public_ipv6() {
        // 2001:db8::/32 is documentation range; real public addresses should pass.
        // Using a known-public, non-reserved address for test purposes.
        // 2606:4700:4700::1111 is Cloudflare DNS — public, non-reserved.
        let result = validate_subgraph_url("https://[2606:4700:4700::1111]/graphql");
        assert!(result.is_ok(), "public IPv6 address must be accepted: {result:?}");
    }

    // ── S23-H1: Entity resolver response body cap ─────────────────────────────

    #[test]
    fn entity_response_cap_constant_is_reasonable() {
        const { assert!(MAX_ENTITY_RESPONSE_BYTES >= 1024 * 1024) }
        const { assert!(MAX_ENTITY_RESPONSE_BYTES <= 500 * 1024 * 1024) }
    }

    #[tokio::test]
    async fn entity_resolver_oversized_response_is_rejected() {
        use std::collections::HashMap;

        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        use crate::{selection_parser::FieldSelection, types::EntityRepresentation};

        let mock = MockServer::start().await;
        let oversized = vec![b'x'; MAX_ENTITY_RESPONSE_BYTES + 1];
        Mock::given(method("POST"))
            .and(path("/_entities"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(oversized))
            .mount(&mock)
            .await;

        let config = HttpClientConfig {
            timeout_ms:     5000,
            max_retries:    1,
            retry_delay_ms: 0,
        };
        // new_for_test bypasses SSRF guard so we can reach the loopback mock server.
        let resolver = HttpEntityResolver::new_for_test(config).unwrap();
        let url = format!("{}/_entities", mock.uri());
        let repr = EntityRepresentation {
            typename:   "Order".to_string(),
            key_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
            all_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
        };
        let selection = FieldSelection::new(vec!["id".to_string()]);

        let result = resolver.resolve_entities(&url, &[repr], &selection).await;

        assert!(result.is_err(), "oversized entity response must be rejected");
        let msg = result.err().unwrap().to_string();
        assert!(msg.contains("too large"), "error must mention size limit: {msg}");
    }

    #[tokio::test]
    async fn entity_resolver_valid_response_is_parsed() {
        use std::collections::HashMap;

        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        use crate::{selection_parser::FieldSelection, types::EntityRepresentation};

        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/_entities"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "_entities": [{ "id": "1", "__typename": "Order" }] }
            })))
            .mount(&mock)
            .await;

        let config = HttpClientConfig {
            timeout_ms:     5000,
            max_retries:    1,
            retry_delay_ms: 0,
        };
        let resolver = HttpEntityResolver::new_for_test(config).unwrap();
        let url = format!("{}/_entities", mock.uri());
        let repr = EntityRepresentation {
            typename:   "Order".to_string(),
            key_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
            all_fields: HashMap::from([("id".to_string(), serde_json::json!("1"))]),
        };
        let selection = FieldSelection::new(vec!["id".to_string()]);

        let result = resolver.resolve_entities(&url, &[repr], &selection).await;
        assert!(result.is_ok(), "valid entity response must be accepted");
    }
}
