//! Gateway HTTP server — routes GraphQL requests to subgraphs.
//!
//! Starts an axum server that:
//! 1. Accepts GraphQL POST requests at `/graphql`
//! 2. Plans execution via the query planner
//! 3. Forwards fetches to subgraphs in parallel
//! 4. Merges responses and returns to the client
//! 5. Exposes `/health` and `/ready` endpoints

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use super::config::{GatewayConfig, SubgraphConfig};
use super::merger::{self, MergedResponse, SubgraphResponse};
use super::planner::{self, FieldOwnership, QueryPlan};

/// Shared gateway state passed to all request handlers.
#[derive(Clone)]
pub struct GatewayState {
    /// HTTP client for subgraph requests.
    client: reqwest::Client,

    /// Subgraph name → config.
    subgraphs: HashMap<String, SubgraphConfig>,

    /// Root field → owning subgraph.
    ownership: Arc<FieldOwnership>,

    /// Per-subgraph request timeout.
    subgraph_timeout: Duration,
}

/// Incoming GraphQL request body.
#[derive(Debug, Deserialize)]
struct GraphQLRequest {
    query:     String,
    #[serde(default)]
    variables: Option<Value>,
    #[serde(default, rename = "operationName")]
    operation_name: Option<String>,
}

/// Build and start the gateway HTTP server.
///
/// # Errors
///
/// Returns an error if the server fails to bind or encounters a fatal error.
pub async fn serve(config: &GatewayConfig, ownership: FieldOwnership) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(config.timeouts.subgraph_request_ms))
        .build()?;

    let state = GatewayState {
        client,
        subgraphs: config.subgraphs.clone(),
        ownership: Arc::new(ownership),
        subgraph_timeout: Duration::from_millis(config.timeouts.subgraph_request_ms),
    };

    let app = build_router(state);

    let listener = TcpListener::bind(&config.listen).await?;
    eprintln!("Gateway listening on {}", config.listen);
    eprintln!("  POST /graphql  — GraphQL endpoint");
    eprintln!("  GET  /health   — Health check");
    eprintln!("  GET  /ready    — Readiness check");

    axum::serve(listener, app).await?;
    Ok(())
}

/// Build the axum Router (also used in tests).
pub fn build_router(state: GatewayState) -> Router {
    Router::new()
        .route("/graphql", post(handle_graphql))
        .route("/health", get(handle_health))
        .route("/ready", get(handle_ready))
        .with_state(state)
}

/// Handle a GraphQL POST request.
async fn handle_graphql(
    State(state): State<GatewayState>,
    Json(request): Json<GraphQLRequest>,
) -> impl IntoResponse {
    // Extract root fields from the query
    let root_fields = planner::extract_root_fields(&request.query);
    if root_fields.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "errors": [{"message": "Could not extract root fields from query"}]
            })),
        );
    }

    // Plan the query
    let plan = match planner::plan_query(&root_fields, &state.ownership) {
        Ok(plan) => plan,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "errors": [{"message": e.to_string()}]
                })),
            );
        },
    };

    // Execute fetches in parallel
    let responses = execute_plan(&state, &plan, &request).await;

    // Merge responses
    let merged = merger::merge_responses(&responses);

    (StatusCode::OK, Json(merged_to_value(&merged)))
}

/// Execute all fetches in the query plan concurrently.
async fn execute_plan(
    state: &GatewayState,
    plan: &QueryPlan,
    original: &GraphQLRequest,
) -> Vec<(String, SubgraphResponse)> {
    let mut handles = Vec::new();

    for fetch in &plan.fetches {
        let client = state.client.clone();
        let subgraph_name = fetch.subgraph.clone();
        let query = if fetch.is_entity_fetch {
            fetch.query.clone()
        } else {
            // For root fetches, forward the original query to the subgraph.
            // In a full implementation the planner would rewrite the query to
            // only include fields owned by this subgraph. For the MVP, forward
            // the full query (single-subgraph case) or the planned query.
            if plan.fetches.len() == 1 {
                original.query.clone()
            } else {
                fetch.query.clone()
            }
        };
        let variables = original.variables.clone().unwrap_or(Value::Null);
        let operation_name = original.operation_name.clone();

        let url = state
            .subgraphs
            .get(&fetch.subgraph)
            .map(|s| s.url.clone())
            .unwrap_or_default();

        let timeout = state.subgraph_timeout;

        handles.push(tokio::spawn(async move {
            let result = execute_subgraph_request(
                &client, &url, &query, &variables, operation_name.as_deref(), timeout,
            )
            .await;

            let response = match result {
                Ok(resp) => resp,
                Err(e) => SubgraphResponse {
                    data:   None,
                    errors: vec![merger::GraphQLError {
                        message:    format!("Subgraph '{subgraph_name}' request failed: {e}"),
                        path:       None,
                        locations:  None,
                        extensions: Some(json!({"code": "SUBGRAPH_REQUEST_FAILED"})),
                    }],
                },
            };

            (subgraph_name, response)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(pair) => results.push(pair),
            Err(e) => {
                results.push((
                    "unknown".to_string(),
                    SubgraphResponse {
                        data:   None,
                        errors: vec![merger::GraphQLError {
                            message:    format!("Task join error: {e}"),
                            path:       None,
                            locations:  None,
                            extensions: None,
                        }],
                    },
                ));
            },
        }
    }

    results
}

/// Execute a single HTTP request to a subgraph.
async fn execute_subgraph_request(
    client: &reqwest::Client,
    url: &str,
    query: &str,
    variables: &Value,
    operation_name: Option<&str>,
    _timeout: Duration,
) -> Result<SubgraphResponse, reqwest::Error> {
    let mut body = json!({
        "query": query,
        "variables": variables,
    });
    if let Some(op) = operation_name {
        body["operationName"] = Value::String(op.to_string());
    }

    let resp = client.post(url).json(&body).send().await?;
    let sg_resp: SubgraphResponse = resp.json().await?;
    Ok(sg_resp)
}

/// Convert `MergedResponse` to a `serde_json::Value`.
fn merged_to_value(merged: &MergedResponse) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("data".to_string(), merged.data.clone());
    if !merged.errors.is_empty() {
        map.insert(
            "errors".to_string(),
            serde_json::to_value(&merged.errors).unwrap_or(Value::Array(Vec::new())),
        );
    }
    Value::Object(map)
}

/// Health check endpoint.
async fn handle_health() -> impl IntoResponse {
    Json(json!({"status": "healthy"}))
}

/// Readiness check endpoint.
async fn handle_ready(State(state): State<GatewayState>) -> impl IntoResponse {
    let subgraph_count = state.subgraphs.len();
    Json(json!({
        "status": "ready",
        "subgraphs": subgraph_count,
    }))
}

#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    fn test_state() -> GatewayState {
        let mut subgraphs = HashMap::new();
        subgraphs.insert("users".to_string(), SubgraphConfig {
            url:    "http://localhost:4001/graphql".to_string(),
            schema: None,
        });

        let mut ownership = FieldOwnership::default();
        ownership.insert("users".to_string(), "users".to_string());

        GatewayState {
            client: reqwest::Client::new(),
            subgraphs,
            ownership: Arc::new(ownership),
            subgraph_timeout: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "healthy");
    }

    #[tokio::test]
    async fn test_ready_endpoint() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ready");
        assert_eq!(json["subgraphs"], 1);
    }

    #[tokio::test]
    async fn test_graphql_empty_query() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/graphql")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query": ""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_graphql_unknown_field() {
        let app = build_router(test_state());

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/graphql")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"query": "{ nonexistent }"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: Value = serde_json::from_slice(&body).unwrap();
        assert!(json["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("nonexistent"));
    }
}
