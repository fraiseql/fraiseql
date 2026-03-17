//! Async HTTP client for FraiseQL GraphQL servers.
//!
//! # Example
//!
//! ```rust,no_run
//! // Requires: a running FraiseQL server
//! use fraiseql_client::FraiseQLClientBuilder;
//!
//! #[tokio::main]
//! async fn main() -> fraiseql_client::Result<()> {
//!     let client = FraiseQLClientBuilder::new("http://localhost:8000/graphql")
//!         .authorization("Bearer my-token")
//!         .build();
//!
//!     let data: serde_json::Value = client
//!         .query("{ users { id name } }", None)
//!         .await?;
//!
//!     println!("{data}");
//!     Ok(())
//! }
//! ```

pub mod client;
pub mod error;
pub mod retry;
pub mod types;

#[cfg(feature = "candle")]
pub mod integrations;

pub use client::{FraiseQLClient, FraiseQLClientBuilder};
pub use error::{FraiseQLError, Result};
pub use retry::RetryConfig;

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    #[tokio::test]
    async fn query_returns_data_on_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {"user": {"id": 1, "name": "Alice"}}
                })),
            )
            .mount(&server)
            .await;

        let client =
            FraiseQLClientBuilder::new(format!("{}/graphql", server.uri())).build();
        let result: serde_json::Value =
            client.query("{ user { id name } }", None).await.unwrap();
        assert_eq!(result["user"]["name"], "Alice");
    }

    #[tokio::test]
    async fn returns_graphql_error_when_errors_present() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": null,
                    "errors": [{"message": "Not found"}]
                })),
            )
            .mount(&server)
            .await;

        let client =
            FraiseQLClientBuilder::new(format!("{}/graphql", server.uri())).build();
        let err = client
            .query::<serde_json::Value>("{ user { id } }", None)
            .await
            .unwrap_err();
        assert!(matches!(err, FraiseQLError::GraphQL { .. }));
    }

    #[tokio::test]
    async fn null_errors_is_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": {"users": []},
                    "errors": null
                })),
            )
            .mount(&server)
            .await;

        let client =
            FraiseQLClientBuilder::new(format!("{}/graphql", server.uri())).build();
        let result: serde_json::Value =
            client.query("{ users { id } }", None).await.unwrap();
        assert_eq!(result["users"], serde_json::json!([]));
    }
}
