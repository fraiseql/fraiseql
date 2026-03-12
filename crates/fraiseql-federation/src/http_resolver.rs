//! HTTP entity resolution for federated subgraphs.
//!
//! Resolves entities from remote GraphQL subgraphs via HTTP POST requests
//! to their `_entities` endpoint. Includes retry logic, timeout handling,
//! and error recovery.

use std::time::Duration;

use serde_json::{Value, json};

use fraiseql_error::Result;
use crate::{
    selection_parser::FieldSelection,
    tracing::FederationTraceContext,
    types::EntityRepresentation,
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
    client: reqwest::Client,
    config: HttpClientConfig,
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

#[derive(serde::Deserialize, Debug)]
struct GraphQLError {
    message: String,
}

/// Validate that a subgraph URL is safe to contact.
///
/// Blocks SSRF attacks by:
/// 1. Requiring `http` or `https` scheme.
/// 2. Blocking `localhost` and `.localhost` hostnames.
/// 3. Blocking literal private/reserved IP addresses (RFC 1918, loopback,
///    link-local, CGNAT, ULA, IPv4-mapped IPv6).
///
/// Note: DNS-level SSRF (attacker-controlled domain that resolves to a
/// private IP) is not mitigated here; that requires egress filtering at the
/// network layer.
///
/// # Errors
///
/// Returns `FraiseQLError::Internal` if the scheme, host, or IP is forbidden.
fn validate_subgraph_url(url: &str) -> fraiseql_error::Result<()> {
    // Require http or https scheme.
    let rest = if let Some(r) = url.strip_prefix("https://") {
        r
    } else if let Some(r) = url.strip_prefix("http://") {
        r
    } else {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!(
                "Subgraph URL must use http:// or https:// scheme (got: {url})"
            ),
            source: None,
        });
    };

    // Extract the authority (host[:port]), handling IPv6 bracket notation.
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        // IPv6 literal: strip brackets and optional trailing :port.
        authority.split(']').next().unwrap_or("").trim_start_matches('[')
    } else {
        authority.split(':').next().unwrap_or("")
    };

    if host.is_empty() {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("Subgraph URL has no host: {url}"),
            source: None,
        });
    }

    // Block loopback hostnames.
    let lower_host = host.to_ascii_lowercase();
    if lower_host == "localhost" || lower_host.ends_with(".localhost") {
        return Err(fraiseql_error::FraiseQLError::Internal {
            message: format!("Subgraph URL targets a loopback host: {host}"),
            source: None,
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
                source: None,
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
fn is_ssrf_blocked_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 127                                                  // loopback
                || o[0] == 10                                            // RFC 1918 /8
                || (o[0] == 172 && (16..=31).contains(&o[1]))            // RFC 1918 /12
                || (o[0] == 192 && o[1] == 168)                          // RFC 1918 /16
                || (o[0] == 169 && o[1] == 254)                          // link-local
                || (o[0] == 100 && (o[1] & 0b1100_0000) == 0b0100_0000) // CGNAT RFC 6598
                || o[0] == 0                                             // unspecified
        },
        std::net::IpAddr::V6(v6) => {
            let s = v6.segments();
            *v6 == std::net::Ipv6Addr::LOCALHOST                         // ::1
                || (s[0] == 0 && s[1] == 0 && s[2] == 0 && s[3] == 0
                    && s[4] == 0 && s[5] == 0xffff)                      // IPv4-mapped
                || (s[0] & 0xfe00) == 0xfc00                             // ULA fc00::/7
                || (s[0] & 0xffc0) == 0xfe80                             // link-local fe80::/10
        },
    }
}

impl HttpEntityResolver {
    /// Create a new HTTP entity resolver
    pub fn new(config: HttpClientConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .unwrap_or_else(|e| {
                tracing::warn!(
                    error = %e,
                    "Failed to build reqwest client for federation HTTP resolver; \
                     using default client. Configured timeout will not be applied."
                );
                reqwest::Client::default()
            });

        Self { client, config }
    }

    /// Resolve entities via HTTP _entities query
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
        validate_subgraph_url(subgraph_url)?;

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
                Ok(response) if response.status().is_success() => {
                    match response.json::<GraphQLResponse>().await {
                        Ok(gql_response) => return Ok(gql_response),
                        Err(e) => {
                            last_error = Some(format!("Failed to parse response: {}", e));
                        },
                    }
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

    #[test]
    fn test_http_resolver_creation() {
        let config = HttpClientConfig::default();
        let _resolver = HttpEntityResolver::new(config);
        // Should not panic
    }

    #[test]
    fn test_empty_representations() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let result = resolver
                .resolve_entities("http://example.com/graphql", &[], &FieldSelection::default())
                .await;

            assert!(result.is_ok());
            assert_eq!(result.unwrap().len(), 0);
        });
    }

    #[test]
    fn test_graphql_query_building() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
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
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
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
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
        let representations = vec![mock_representation("User", "123")];

        let response = GraphQLResponse {
            data:   Some(json!({
                "_entities": [
                    { "id": "123", "email": "user@example.com" }
                ]
            })),
            errors: None,
        };

        let result = resolver.parse_response(&response, &representations);
        assert!(result.is_ok());

        let entities = result.unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].is_some());
    }

    #[test]
    fn test_response_parsing_with_errors() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
        let representations = vec![mock_representation("User", "123")];

        let response = GraphQLResponse {
            data:   None,
            errors: Some(vec![GraphQLError {
                message: "Entity not found".to_string(),
            }]),
        };

        let result = resolver.parse_response(&response, &representations);
        assert!(result.is_err());
    }

    #[test]
    fn test_response_parsing_entity_count_mismatch() {
        let resolver = HttpEntityResolver::new(HttpClientConfig::default());
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
        assert!(result.is_err());
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
}
