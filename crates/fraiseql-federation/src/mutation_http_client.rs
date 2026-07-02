//! HTTP client for executing mutations on remote subgraphs.
//!
//! Propagates extended mutations (mutations on entities owned elsewhere) to the
//! authoritative subgraph via GraphQL HTTP requests.

use std::time::Duration;

use fraiseql_error::{FraiseQLError, Result};
use serde_json::Value;

use crate::{metadata_helpers::find_federation_type, types::FederationMetadata};

/// Maximum byte size for a federated mutation response.
///
/// GraphQL responses for mutations are typically small (a few `KiB`).
/// 10 `MiB` is generous for responses carrying bulk data while blocking
/// allocation-bomb payloads from a compromised or misconfigured subgraph.
const MAX_MUTATION_RESPONSE_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

/// Configuration for HTTP mutation client
#[derive(Debug, Clone)]
pub struct HttpMutationConfig {
    /// Request timeout in milliseconds
    pub timeout_ms:     u64,
    /// Maximum number of retries
    pub max_retries:    u32,
    /// Delay between retries in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for HttpMutationConfig {
    fn default() -> Self {
        Self {
            timeout_ms:     5000,
            max_retries:    3,
            retry_delay_ms: 100,
        }
    }
}

/// GraphQL request format
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQLRequest {
    /// GraphQL query/mutation string
    pub query:     String,
    /// Variables for the query
    pub variables: Value,
}

/// GraphQL response format
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GraphQLResponse {
    /// Response data (null if errors occurred)
    pub data:   Option<Value>,
    /// GraphQL errors
    pub errors: Option<Vec<GraphQLError>>,
}

/// Re-export the canonical [`fraiseql_error::GraphQLError`] for consumers who import
/// federation types via this module's path.
pub use fraiseql_error::GraphQLError;

/// HTTP client for executing mutations on remote subgraphs
pub struct HttpMutationClient {
    /// HTTP client
    client:    Option<reqwest::Client>,
    /// Configuration
    config:    HttpMutationConfig,
    /// Skip the SSRF URL validation before a mutation request.
    ///
    /// Only ever `true` in a `new_for_test` client (unit-test builds or the
    /// `test-utils` feature) so integration tests can drive a loopback mock
    /// subgraph over plain HTTP. `new` always sets it `false`, so the production
    /// SSRF posture is unchanged. Mirrors `HttpEntityResolver::skip_ssrf`.
    #[cfg(any(test, feature = "test-utils"))]
    skip_ssrf: bool,
}

impl HttpMutationClient {
    /// Create a new HTTP mutation client.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the HTTP client cannot be initialised.
    pub fn new(config: HttpMutationConfig) -> Result<Self> {
        let client = Self::build_client(&config, true, None)?;
        Ok(Self {
            client: Some(client),
            config,
            #[cfg(any(test, feature = "test-utils"))]
            skip_ssrf: false,
        })
    }

    /// Create a mutation client with mutual-TLS (client-certificate) authentication.
    ///
    /// Builds the production client (SSRF posture: redirect-none, `https_only`) and
    /// attaches the client identity + trusted root CA from `mtls` (see
    /// [`crate::tls::MtlsMaterial`]). When `mtls.enabled` is false this is a no-op —
    /// an ordinary [`Self::new`] client. Fails loud if mTLS is enabled but its
    /// certificate material is missing or malformed: the client is never silently
    /// downgraded to one-way TLS.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the client cannot be initialised or the
    /// mTLS material cannot be loaded/applied.
    pub fn new_with_mtls(
        config: HttpMutationConfig,
        mtls: &crate::tls::MtlsConfig,
    ) -> Result<Self> {
        let client = Self::build_client(&config, true, Some(mtls))?;
        Ok(Self {
            client: Some(client),
            config,
            #[cfg(any(test, feature = "test-utils"))]
            skip_ssrf: false,
        })
    }

    /// Build the underlying reqwest client with the mutation-client SSRF posture and,
    /// optionally, mutual-TLS material.
    ///
    /// Mutations are the state-changing (more dangerous) direction, so the client
    /// matches the entity resolver's SSRF posture: `redirect(Policy::none())` so a 3xx
    /// from a compromised subgraph cannot bounce the request to an un-validated
    /// internal target, and (for production) `https_only(true)` so plain http:// can
    /// never leave the client. `https_only` is false only for `new_for_test` (to reach
    /// a loopback mock over http). `mtls = Some(cfg)` loads and applies the client
    /// identity + root CA, failing loud on bad material.
    fn build_client(
        config: &HttpMutationConfig,
        https_only: bool,
        mtls: Option<&crate::tls::MtlsConfig>,
    ) -> Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(config.timeout_ms));
        if https_only {
            builder = builder.https_only(true);
        }
        if let Some(mtls_cfg) = mtls {
            builder = crate::tls::MtlsMaterial::load(mtls_cfg)?.apply(builder)?;
        }
        builder.build().map_err(|e| FraiseQLError::Internal {
            message: format!("HTTP client initialisation failed for mutation client: {e}"),
            source:  None,
        })
    }

    /// Create a mutation client that skips SSRF URL validation.
    ///
    /// **Only available with the `test-utils` feature or in unit-test builds.**
    /// The client is built *without* `https_only`, and `execute_mutation` skips
    /// `validate_subgraph_url` / `dns_resolve_and_check`, so integration tests can
    /// drive a loopback mock subgraph over plain HTTP without setting
    /// process-global environment variables. Mirrors
    /// [`crate::http_resolver::HttpEntityResolver::new_for_test`].
    ///
    /// **Never use in production** — this bypasses all SSRF protections.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the HTTP client cannot be initialised.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new_for_test(config: HttpMutationConfig) -> Result<Self> {
        // No https_only in test mode so a loopback mock over http:// is reachable.
        let client = Self::build_client(&config, false, None)?;
        Ok(Self {
            client: Some(client),
            config,
            skip_ssrf: true,
        })
    }

    /// Execute a mutation on a remote subgraph
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError` if URL validation fails, the HTTP request fails,
    /// or the response contains GraphQL errors.
    pub async fn execute_mutation(
        &self,
        subgraph_url: &str,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
        metadata: &FederationMetadata,
    ) -> Result<Value> {
        // SECURITY: Validate URL before any network contact to prevent SSRF.
        // Static scheme/host/literal-IP check, then the DNS-rebinding guard —
        // parity with the entity resolver via the shared crate helpers. In
        // test/test-utils builds a `new_for_test` client may skip the guard to
        // reach a loopback mock subgraph.
        #[cfg(not(any(test, feature = "test-utils")))]
        {
            crate::http_resolver::validate_subgraph_url(subgraph_url)?;
            crate::http_resolver::dns_resolve_and_check(subgraph_url).await?;
        }
        #[cfg(any(test, feature = "test-utils"))]
        if !self.skip_ssrf {
            crate::http_resolver::validate_subgraph_url(subgraph_url)?;
            crate::http_resolver::dns_resolve_and_check(subgraph_url).await?;
        }

        let client = self.client.as_ref().ok_or_else(|| FraiseQLError::Internal {
            message: "HTTP client not initialized".to_string(),
            source:  None,
        })?;

        // Get entity type for key fields
        let fed_type = find_federation_type(typename, metadata)?;

        // Build mutation query
        let query = self.build_mutation_query(typename, mutation_name, variables, fed_type)?;

        // Execute with retry
        let response = self.execute_with_retry(client, subgraph_url, &query).await?;

        // Parse and return response
        self.parse_response(response, mutation_name)
    }

    /// Build a GraphQL mutation query
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Validation` if the variables are not a JSON object.
    pub fn build_mutation_query(
        &self,
        _typename: &str,
        mutation_name: &str,
        variables: &Value,
        fed_type: &crate::types::FederatedType,
    ) -> Result<GraphQLRequest> {
        // Get key fields for response projection
        let key_fields = if let Some(key_directive) = fed_type.keys.first() {
            key_directive.fields.join(" ")
        } else {
            "id".to_string()
        };

        // Get mutation input fields (excluding external fields)
        let mut input_fields = Vec::new();
        if let Some(obj) = variables.as_object() {
            for key in obj.keys() {
                if !fed_type.external_fields.contains(key) {
                    input_fields.push(format!("{}: ${}", key, key));
                }
            }
        }

        // Build variable definitions
        let var_defs = self.build_variable_definitions(variables)?;

        // Build mutation query
        let response_fields = format!("__typename {}", key_fields);
        let input_clause = input_fields.join(", ");

        let query = format!(
            "mutation({}) {{ {}({}) {{ {} }} }}",
            var_defs, mutation_name, input_clause, response_fields
        );

        Ok(GraphQLRequest {
            query,
            variables: variables.clone(),
        })
    }

    /// Build GraphQL variable definitions from input variables.
    ///
    /// # Errors
    ///
    /// This function is currently infallible and always returns `Ok`. The
    /// `Result` return type is reserved for future validation of variable names.
    pub fn build_variable_definitions(&self, variables: &Value) -> Result<String> {
        let mut var_defs = Vec::new();

        if let Some(obj) = variables.as_object() {
            for key in obj.keys() {
                // Infer type from value (simplified). A fractional JSON number
                // must be typed `Float!`, not `Int!` — a saga step carrying a
                // Float input (e.g. a price) otherwise produces a variable
                // definition the remote subgraph rejects (#429).
                let var_type = match &obj[key] {
                    Value::String(_) => "String!",
                    Value::Number(n) => {
                        if n.is_f64() {
                            "Float!"
                        } else {
                            "Int!"
                        }
                    },
                    Value::Bool(_) => "Boolean!",
                    Value::Null => "String",
                    _ => "JSON", // Use generic JSON type for complex types
                };
                var_defs.push(format!("${}: {}", key, var_type));
            }
        }

        Ok(format!("({})", var_defs.join(", ")))
    }

    /// Execute request with retry logic
    async fn execute_with_retry(
        &self,
        client: &reqwest::Client,
        url: &str,
        request: &GraphQLRequest,
    ) -> Result<GraphQLResponse> {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.config.max_retries {
            attempts += 1;

            match client.post(url).json(request).send().await {
                Ok(response) if response.status().is_success() => {
                    let body_bytes =
                        response.bytes().await.map_err(|e| FraiseQLError::Internal {
                            message: format!("Failed to read GraphQL response: {}", e),
                            source:  None,
                        })?;
                    if body_bytes.len() > MAX_MUTATION_RESPONSE_BYTES {
                        return Err(FraiseQLError::Internal {
                            message: format!(
                                "Federation mutation response too large ({} bytes, max {MAX_MUTATION_RESPONSE_BYTES})",
                                body_bytes.len()
                            ),
                            source:  None,
                        });
                    }
                    return serde_json::from_slice(&body_bytes).map_err(|e| {
                        FraiseQLError::Internal {
                            message: format!("Failed to parse GraphQL response: {}", e),
                            source:  None,
                        }
                    });
                },
                Ok(response) => {
                    last_error = Some(format!("HTTP {}", response.status()));
                },
                Err(e) => {
                    last_error = Some(e.to_string());
                },
            }

            if attempts < self.config.max_retries {
                // Exponential backoff: base_delay * 2^(attempt-1).
                // Consistent with http_resolver.rs; avoids thundering-herd on
                // transient subgraph failures.
                let backoff =
                    self.config.retry_delay_ms.saturating_mul(2_u64.saturating_pow(attempts - 1));
                tokio::time::sleep(Duration::from_millis(backoff)).await;
            }
        }

        Err(FraiseQLError::Internal {
            message: format!(
                "Mutation request failed after {} attempts: {}",
                self.config.max_retries,
                last_error.unwrap_or_else(|| "Unknown error".to_string())
            ),
            source:  None,
        })
    }

    /// Parse mutation response.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::Internal`] if the response contains GraphQL
    /// errors, if the `data` field is absent, or if the `mutation_name` field
    /// is not present in the response data.
    pub fn parse_response(&self, response: GraphQLResponse, mutation_name: &str) -> Result<Value> {
        // Check for GraphQL errors
        if let Some(errors) = response.errors {
            let error_messages: Vec<String> = errors.iter().map(|e| e.message.clone()).collect();
            return Err(FraiseQLError::Internal {
                message: format!(
                    "GraphQL error in mutation response: {}",
                    error_messages.join("; ")
                ),
                source:  None,
            });
        }

        // Extract mutation result from data
        let data = response.data.ok_or_else(|| FraiseQLError::Internal {
            message: "No data in mutation response".to_string(),
            source:  None,
        })?;

        let result = data.get(mutation_name).cloned().ok_or_else(|| FraiseQLError::Internal {
            message: format!("No {} field in response data", mutation_name),
            source:  None,
        })?;

        Ok(result)
    }
}

/// Resolve the remote transport for a saga step: `Some((client, url))` when an
/// HTTP client is configured **and** the step's `subgraph` names a registered
/// peer URL; `None` (dispatch locally) otherwise. Shared by saga forward
/// execution and compensation so both route a step to the same peer identically.
#[cfg(feature = "saga")]
pub(crate) fn resolve_remote<'a>(
    subgraph: &str,
    http_client: Option<&'a HttpMutationClient>,
    subgraph_urls: &'a std::collections::HashMap<String, reqwest::Url>,
) -> Option<(&'a HttpMutationClient, &'a reqwest::Url)> {
    http_client.and_then(|client| subgraph_urls.get(subgraph).map(|url| (client, url)))
}

#[cfg(test)]
mod tests;
