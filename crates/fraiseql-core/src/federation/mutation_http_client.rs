//! HTTP client for executing mutations on remote subgraphs.
//!
//! Propagates extended mutations (mutations on entities owned elsewhere) to the
//! authoritative subgraph via GraphQL HTTP requests.

use serde_json::Value;
use std::time::Duration;

use crate::error::{FraiseQLError, Result};
use crate::federation::types::FederationMetadata;
use crate::federation::metadata_helpers::find_federation_type;

/// Configuration for HTTP mutation client
#[derive(Debug, Clone)]
pub struct HttpMutationConfig {
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum number of retries
    pub max_retries: u32,
    /// Delay between retries in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for HttpMutationConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 5000,
            max_retries: 3,
            retry_delay_ms: 100,
        }
    }
}

/// GraphQL request format
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphQLRequest {
    /// GraphQL query/mutation string
    pub query: String,
    /// Variables for the query
    pub variables: Value,
}

/// GraphQL response format
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GraphQLResponse {
    /// Response data (null if errors occurred)
    pub data: Option<Value>,
    /// GraphQL errors
    pub errors: Option<Vec<GraphQLError>>,
}

/// GraphQL error format
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GraphQLError {
    /// Error message
    pub message: String,
}

/// HTTP client for executing mutations on remote subgraphs
pub struct HttpMutationClient {
    /// HTTP client
    client: Option<reqwest::Client>,
    /// Configuration
    config: HttpMutationConfig,
}

impl HttpMutationClient {
    /// Create a new HTTP mutation client
    pub fn new(config: HttpMutationConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .ok();

        Self { client, config }
    }

    /// Execute a mutation on a remote subgraph
    pub async fn execute_mutation(
        &self,
        subgraph_url: &str,
        typename: &str,
        mutation_name: &str,
        variables: &Value,
        metadata: &FederationMetadata,
    ) -> Result<Value> {
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| FraiseQLError::Internal {
                message: "HTTP client not initialized".to_string(),
                source: None,
            })?;

        // Get entity type for key fields
        let fed_type = find_federation_type(typename, metadata)?;

        // Build mutation query
        let query = self.build_mutation_query(typename, mutation_name, variables, fed_type)?;

        // Execute with retry
        let response = self
            .execute_with_retry(client, subgraph_url, &query)
            .await?;

        // Parse and return response
        self.parse_response(response, mutation_name)
    }

    /// Build a GraphQL mutation query
    pub fn build_mutation_query(
        &self,
        _typename: &str,
        mutation_name: &str,
        variables: &Value,
        fed_type: &crate::federation::types::FederatedType,
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

    /// Build GraphQL variable definitions from input variables
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
                    return response.json().await.map_err(|e| FraiseQLError::Internal {
                        message: format!("Failed to parse GraphQL response: {}", e),
                        source: None,
                    });
                }
                Ok(response) => {
                    last_error = Some(format!("HTTP {}", response.status()));
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }

            if attempts < self.config.max_retries {
                tokio::time::sleep(Duration::from_millis(
                    self.config.retry_delay_ms * u64::from(attempts),
                ))
                .await;
            }
        }

        Err(FraiseQLError::Internal {
            message: format!(
                "Mutation request failed after {} attempts: {}",
                self.config.max_retries,
                last_error.unwrap_or_else(|| "Unknown error".to_string())
            ),
            source: None,
        })
    }

    /// Parse mutation response
    pub fn parse_response(&self, response: GraphQLResponse, mutation_name: &str) -> Result<Value> {
        // Check for GraphQL errors
        if let Some(errors) = response.errors {
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.clone()).collect();
            return Err(FraiseQLError::Internal {
                message: format!("GraphQL error in mutation response: {}", error_messages.join("; ")),
                source: None,
            });
        }

        // Extract mutation result from data
        let data = response.data.ok_or_else(|| FraiseQLError::Internal {
            message: "No data in mutation response".to_string(),
            source: None,
        })?;

        let result = data.get(mutation_name).cloned().ok_or_else(|| {
            FraiseQLError::Internal {
                message: format!("No {} field in response data", mutation_name),
                source: None,
            }
        })?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_http_mutation_client_creation() {
        let config = HttpMutationConfig::default();
        let _client = HttpMutationClient::new(config);
        // Should not panic
    }

    #[test]
    fn test_mutation_config_defaults() {
        let config = HttpMutationConfig::default();
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 100);
    }

    #[test]
    fn test_mutation_config_custom() {
        let config = HttpMutationConfig {
            timeout_ms: 10000,
            max_retries: 5,
            retry_delay_ms: 200,
        };
        assert_eq!(config.timeout_ms, 10000);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 200);
    }

    #[test]
    fn test_graphql_request_serialization() {
        let request = GraphQLRequest {
            query: "mutation { updateUser(id: $id) { id } }".to_string(),
            variables: json!({ "id": "123" }),
        };

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["query"], "mutation { updateUser(id: $id) { id } }");
        assert_eq!(json["variables"]["id"], "123");
    }

    #[test]
    fn test_graphql_response_parsing_success() {
        let response_json = json!({
            "data": {
                "updateUser": {
                    "__typename": "User",
                    "id": "123",
                    "name": "Alice"
                }
            }
        });

        let response: GraphQLResponse = serde_json::from_value(response_json).unwrap();
        assert!(response.data.is_some());
        assert!(response.errors.is_none());

        let data = response.data.unwrap();
        assert_eq!(data["updateUser"]["id"], "123");
    }

    #[test]
    fn test_graphql_response_with_errors() {
        let response_json = json!({
            "data": null,
            "errors": [
                {
                    "message": "User not found"
                }
            ]
        });

        let response: GraphQLResponse = serde_json::from_value(response_json).unwrap();
        assert!(response.data.is_none());
        assert!(response.errors.is_some());
        assert_eq!(response.errors.unwrap()[0].message, "User not found");
    }

    #[test]
    fn test_variable_definition_building() {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config);

        let variables = json!({
            "id": "123",
            "name": "Alice",
            "active": true
        });

        let var_defs = client.build_variable_definitions(&variables).unwrap();
        assert!(var_defs.contains("$id: String!"));
        assert!(var_defs.contains("$name: String!"));
        assert!(var_defs.contains("$active: Boolean!"));
    }

    #[test]
    fn test_variable_definition_with_numbers() {
        let config = HttpMutationConfig::default();
        let client = HttpMutationClient::new(config);

        let variables = json!({
            "count": 42,
            "price": 9.99
        });

        let var_defs = client.build_variable_definitions(&variables).unwrap();
        assert!(var_defs.contains("$count: Int!"));
        assert!(var_defs.contains("$price: Int!"));
    }
}
