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
    client: Option<reqwest::Client>,
    /// Configuration
    config: HttpMutationConfig,
}

impl HttpMutationClient {
    /// Create a new HTTP mutation client.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::Internal` if the HTTP client cannot be initialised.
    pub fn new(config: HttpMutationConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| FraiseQLError::Internal {
                message: format!("HTTP client initialisation failed for mutation client: {e}"),
                source:  None,
            })?;

        Ok(Self {
            client: Some(client),
            config,
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
        crate::http_resolver::validate_subgraph_url(subgraph_url)?;

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
                // Infer type from value (simplified)
                let var_type = match &obj[key] {
                    Value::String(_) => "String!",
                    Value::Number(_) => "Int!",
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

#[cfg(test)]
mod tests;
