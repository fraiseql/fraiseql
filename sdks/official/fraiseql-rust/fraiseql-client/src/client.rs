//! The FraiseQL HTTP client.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::{
    error::{FraiseQLError, Result},
    retry::RetryConfig,
    types::{GraphQLRequest, GraphQLResponse},
};

/// Builder for configuring a [`FraiseQLClient`].
///
/// # Example
///
/// ```rust,no_run
/// // Requires: a running FraiseQL server
/// use fraiseql_client::FraiseQLClientBuilder;
///
/// let client = FraiseQLClientBuilder::new("http://localhost:8000/graphql")
///     .authorization("Bearer my-token")
///     .timeout(std::time::Duration::from_secs(30))
///     .build();
/// ```
#[derive(Debug)]
#[must_use = "call .build() to construct the client"]
pub struct FraiseQLClientBuilder {
    url: String,
    authorization: Option<String>,
    timeout: Duration,
    retry: Option<RetryConfig>,
}

impl FraiseQLClientBuilder {
    /// Create a new builder with the given server URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            authorization: None,
            timeout: Duration::from_secs(30),
            retry: None,
        }
    }

    /// Set the `Authorization` header value (e.g., `"Bearer <token>"`).
    pub fn authorization(mut self, token: impl Into<String>) -> Self {
        self.authorization = Some(token.into());
        self
    }

    /// Set the per-request timeout.
    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Configure automatic retries.
    pub const fn retry(mut self, retry: RetryConfig) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Build the [`FraiseQLClient`].
    ///
    /// # Panics
    ///
    /// Panics if the URL or TLS configuration is invalid (reqwest builder).
    #[must_use]
    pub fn build(self) -> FraiseQLClient {
        // Reason: reqwest::Client::build() panics on invalid config (invalid URL
        // scheme, bad TLS certs, etc.). This is a programming error, not a runtime
        // error, so panic is appropriate here.
        let http = reqwest::Client::builder()
            .timeout(self.timeout)
            .build()
            .expect("valid reqwest client config");

        FraiseQLClient {
            url: self.url,
            authorization: self.authorization,
            timeout_ms: self.timeout.as_millis().try_into().unwrap_or(u64::MAX),
            retry: self.retry,
            http,
        }
    }
}

/// An async HTTP client for FraiseQL GraphQL servers.
#[derive(Debug)]
pub struct FraiseQLClient {
    pub(crate) url: String,
    pub(crate) authorization: Option<String>,
    pub(crate) timeout_ms: u64,
    pub(crate) retry: Option<RetryConfig>,
    pub(crate) http: reqwest::Client,
}

impl FraiseQLClient {
    /// Execute a GraphQL query and deserialize the `data` field into `T`.
    ///
    /// # Errors
    ///
    /// Returns [`FraiseQLError::GraphQL`] if the response contains errors.
    /// Returns [`FraiseQLError::Network`] on transport failure.
    /// Returns [`FraiseQLError::Timeout`] if the request exceeds the timeout.
    /// Returns [`FraiseQLError::Authentication`] on 401/403.
    pub async fn query<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<T> {
        self.execute(query, variables).await
    }

    /// Execute a GraphQL mutation and deserialize the `data` field into `T`.
    ///
    /// # Errors
    ///
    /// Same error conditions as [`FraiseQLClient::query`].
    pub async fn mutate<T: DeserializeOwned>(
        &self,
        mutation: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<T> {
        self.execute(mutation, variables).await
    }

    pub(crate) async fn execute<T: DeserializeOwned>(
        &self,
        gql_query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<T> {
        let max_attempts = self.retry.as_ref().map_or(1, |r| r.max_attempts);
        let mut last_err: Option<FraiseQLError> = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                if let Some(retry) = &self.retry {
                    if let Some(ref err) = last_err {
                        if !is_retryable(err) {
                            break;
                        }
                        tokio::time::sleep(retry.delay_for(attempt - 1)).await;
                    }
                }
            }

            match self.do_request(gql_query, variables).await {
                Ok(val) => return Ok(val),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| FraiseQLError::Timeout { timeout_ms: self.timeout_ms }))
    }

    async fn do_request<T: DeserializeOwned>(
        &self,
        gql_query: &str,
        variables: Option<&serde_json::Value>,
    ) -> Result<T> {
        let body = GraphQLRequest {
            query: gql_query,
            variables,
        };

        let mut req = self.http.post(&self.url).json(&body);
        if let Some(auth) = &self.authorization {
            req = req.header("Authorization", auth);
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                FraiseQLError::Timeout { timeout_ms: self.timeout_ms }
            } else {
                FraiseQLError::Network(e)
            }
        })?;

        let status = response.status().as_u16();
        match status {
            401 | 403 => return Err(FraiseQLError::Authentication { status_code: status }),
            429 => {
                let retry_after = response
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs);
                return Err(FraiseQLError::RateLimit { retry_after });
            }
            _ => {}
        }

        let gql_resp: GraphQLResponse<T> = response.json().await.map_err(FraiseQLError::Network)?;

        // null errors = success (cross-SDK invariant: None and Some([]) both succeed)
        if let Some(errors) = gql_resp.errors {
            if !errors.is_empty() {
                return Err(FraiseQLError::GraphQL { errors });
            }
        }

        Ok(gql_resp.data.unwrap_or_else(|| {
            // If data is null but no errors, return null deserialized as T
            serde_json::from_value(serde_json::Value::Null)
                .expect("T must be nullable or Option<_>")
        }))
    }
}

const fn is_retryable(err: &FraiseQLError) -> bool {
    matches!(err, FraiseQLError::Network(_) | FraiseQLError::Timeout { .. })
}
