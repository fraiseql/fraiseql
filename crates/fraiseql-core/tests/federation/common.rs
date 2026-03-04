//! Shared fixtures and helpers for federation tests.

use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use fraiseql_core::{
    db::{
        traits::{DatabaseAdapter, MutationCapable},
        types::{DatabaseType, JsonbValue, PoolMetrics},
        where_clause::WhereClause,
    },
    error::Result,
    federation::types::{EntityRepresentation, FederatedType, FederationMetadata, KeyDirective},
    schema::SqlProjectionHint,
};
use serde_json::{Value, json};

// =============================================================================
// Mock Database Adapters
// =============================================================================

/// Mock database adapter for entity resolution tests.
///
/// Parses simple `SELECT ... FROM <table>` queries and returns pre-loaded data.
pub struct MockDatabaseAdapter {
    data: HashMap<String, Vec<HashMap<String, Value>>>,
}

impl MockDatabaseAdapter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    pub fn with_table_data(mut self, table: String, rows: Vec<HashMap<String, Value>>) -> Self {
        self.data.insert(table, rows);
        self
    }
}

#[async_trait]
impl DatabaseAdapter for MockDatabaseAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(Vec::new())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  10,
            idle_connections:   8,
            active_connections: 2,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(&self, sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        if let Some(start) = sql.to_uppercase().find("FROM ") {
            let after_from = &sql[start + 5..].trim();
            if let Some(space_pos) = after_from.find(' ') {
                let table = after_from[..space_pos].trim().to_lowercase();
                if let Some(rows) = self.data.get(&table) {
                    return Ok(rows.clone());
                }
            } else {
                let table = after_from.to_lowercase();
                if let Some(rows) = self.data.get(&table) {
                    return Ok(rows.clone());
                }
            }
        }
        Ok(Vec::new())
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }

}

impl MutationCapable for MockDatabaseAdapter {}

/// Mock database adapter for mutation tests (returns empty results).
pub struct MockMutationDatabaseAdapter {
    #[allow(dead_code)]
    data: HashMap<String, Vec<HashMap<String, Value>>>,
}

impl MockMutationDatabaseAdapter {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

#[async_trait]
impl DatabaseAdapter for MockMutationDatabaseAdapter {
    async fn execute_with_projection(
        &self,
        view: &str,
        _projection: Option<&SqlProjectionHint>,
        where_clause: Option<&WhereClause>,
        limit: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        self.execute_where_query(view, where_clause, limit, None).await
    }

    async fn execute_where_query(
        &self,
        _view: &str,
        _where_clause: Option<&WhereClause>,
        _limit: Option<u32>,
        _offset: Option<u32>,
    ) -> Result<Vec<JsonbValue>> {
        Ok(Vec::new())
    }

    fn database_type(&self) -> DatabaseType {
        DatabaseType::PostgreSQL
    }

    async fn health_check(&self) -> Result<()> {
        Ok(())
    }

    fn pool_metrics(&self) -> PoolMetrics {
        PoolMetrics {
            total_connections:  10,
            idle_connections:   8,
            active_connections: 2,
            waiting_requests:   0,
        }
    }

    async fn execute_raw_query(&self, _sql: &str) -> Result<Vec<HashMap<String, Value>>> {
        Ok(Vec::new())
    }

    async fn execute_function_call(
        &self,
        _function_name: &str,
        _args: &[serde_json::Value],
    ) -> Result<Vec<HashMap<String, Value>>> {
        Ok(vec![])
    }

}

impl MutationCapable for MockMutationDatabaseAdapter {}

// =============================================================================
// FederationMetadata Builders
// =============================================================================

/// Create a `FederationMetadata` with a single owned type with one key field.
pub fn metadata_single_key(type_name: &str, key_field: &str) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             type_name.to_string(),
            keys:             vec![KeyDirective {
                fields:     vec![key_field.to_string()],
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    }
}

/// Create a `FederationMetadata` with a single extended type.
pub fn metadata_extended_type(
    type_name: &str,
    key_field: &str,
    external_fields: &[&str],
    shareable_fields: &[&str],
) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             type_name.to_string(),
            keys:             vec![KeyDirective {
                fields:     vec![key_field.to_string()],
                resolvable: true,
            }],
            is_extends:       true,
            external_fields:  external_fields.iter().map(|s| s.to_string()).collect(),
            shareable_fields: shareable_fields.iter().map(|s| s.to_string()).collect(),
            field_directives: std::collections::HashMap::new(),
        }],
    }
}

/// Create a `FederationMetadata` with a composite key.
pub fn metadata_composite_key(type_name: &str, key_fields: &[&str]) -> FederationMetadata {
    FederationMetadata {
        enabled: true,
        version: "v2".to_string(),
        types:   vec![FederatedType {
            name:             type_name.to_string(),
            keys:             vec![KeyDirective {
                fields:     key_fields.iter().map(|s| s.to_string()).collect(),
                resolvable: true,
            }],
            is_extends:       false,
            external_fields:  vec![],
            shareable_fields: vec![],
            field_directives: std::collections::HashMap::new(),
        }],
    }
}

/// Create a mock mutation database adapter wrapped in Arc.
pub fn mock_mutation_adapter() -> Arc<MockMutationDatabaseAdapter> {
    Arc::new(MockMutationDatabaseAdapter::new())
}

// =============================================================================
// @requires Enforcement Helper
// =============================================================================

/// Enforce @requires directives at runtime.
///
/// Validates that all fields required by the @requires directives are present
/// in the entity representation.
pub fn enforce_requires(
    metadata: &FederationMetadata,
    typename: &str,
    fields: &[&str],
    representation: &EntityRepresentation,
) -> std::result::Result<(), String> {
    let federated_type = metadata
        .types
        .iter()
        .find(|t| t.name == typename)
        .ok_or_else(|| format!("Type {} not found in federation metadata", typename))?;

    for field in fields {
        if let Some(directives) = federated_type.get_field_directives(field) {
            for required in &directives.requires {
                let field_path = required.path.join(".");
                if !representation.has_field(&field_path) {
                    return Err(format!(
                        "Validation Error: Required field missing\n\
                         Type: {}\n\
                         Field: {}\n\
                         Required: {}\n\
                         Issue: Field '{}' requires '{}' but it is missing from entity \
                         representation\n\
                         Suggestion: Ensure '{}' is requested from the owning subgraph",
                        typename, field, field_path, field, field_path, field_path
                    ));
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// Docker Network Infrastructure
// =============================================================================

pub const APOLLO_GATEWAY_URL: &str = "http://localhost:4000/graphql";
pub const USERS_SUBGRAPH_URL: &str = "http://localhost:4001/graphql";
pub const ORDERS_SUBGRAPH_URL: &str = "http://localhost:4002/graphql";
pub const PRODUCTS_SUBGRAPH_URL: &str = "http://localhost:4003/graphql";

/// Wait for a service to be ready with health check.
pub async fn wait_for_service(
    url: &str,
    max_retries: u32,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let mut retries = 0;

    loop {
        match client
            .post(url)
            .json(&json!({ "query": "{ __typename }" }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                println!("✓ Service ready: {}", url);
                return Ok(());
            },
            Ok(response) => {
                println!("✗ Service {} returned status: {}", url, response.status());
            },
            Err(e) => {
                println!("✗ Service {} connection failed: {}", url, e);
            },
        }

        retries += 1;
        if retries >= max_retries {
            return Err(format!(
                "Service {} failed to become ready after {} retries",
                url, max_retries
            )
            .into());
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Execute a GraphQL query against a service.
pub async fn graphql_query(
    url: &str,
    query: &str,
) -> std::result::Result<Value, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(&json!({ "query": query }))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    let body: Value = response.json().await?;
    Ok(body)
}

/// Extract data from a GraphQL response.
pub fn extract_data(response: &Value) -> Option<&Value> {
    response.get("data")
}

/// Check for GraphQL errors.
pub fn has_errors(response: &Value) -> bool {
    response.get("errors").is_some()
}

/// Get error messages from a GraphQL response.
pub fn get_error_messages(response: &Value) -> String {
    response
        .get("errors")
        .and_then(|e| e.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|err| err.get("message")?.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_else(|| "Unknown error".to_string())
}

/// Setup test fixtures — ensures 2-subgraph services are ready.
pub async fn setup_federation_tests() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 2-subgraph federation tests ===\n");

    println!("Waiting for users subgraph...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All services ready for 2-subgraph federation tests\n");
    Ok(())
}

/// Setup helper for 3-subgraph federation tests (users -> orders -> products).
pub async fn setup_three_subgraph_tests() -> std::result::Result<(), Box<dyn std::error::Error>> {
    println!("\n=== Setting up 3-subgraph federation tests ===\n");

    println!("Waiting for users subgraph (port 4001)...");
    wait_for_service(USERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for orders subgraph (port 4002)...");
    wait_for_service(ORDERS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for products subgraph (port 4003)...");
    wait_for_service(PRODUCTS_SUBGRAPH_URL, 30).await?;

    println!("Waiting for Apollo Router gateway...");
    wait_for_service(APOLLO_GATEWAY_URL, 30).await?;

    println!("\n✓ All 3 subgraphs + gateway ready for federation tests\n");
    Ok(())
}

// =============================================================================
// Saga Test Helpers
// =============================================================================

use fraiseql_core::federation::{
    saga_compensator::SagaCompensator,
    saga_coordinator::{CompensationStrategy, SagaCoordinator, SagaStep},
    saga_executor::SagaExecutor,
};
use uuid::Uuid;

/// Test saga scenario builder for E2E testing.
pub struct TestSagaScenario {
    pub step_count:            usize,
    pub compensation_strategy: CompensationStrategy,
}

impl TestSagaScenario {
    pub fn new(step_count: usize) -> Self {
        Self {
            step_count,
            compensation_strategy: CompensationStrategy::Automatic,
        }
    }

    #[allow(dead_code)]
    pub fn with_strategy(mut self, strategy: CompensationStrategy) -> Self {
        self.compensation_strategy = strategy;
        self
    }

    pub fn build_steps(&self) -> Vec<SagaStep> {
        (1..=self.step_count as u32)
            .map(|i| {
                let subgraph = format!("service-{}", i % 3 + 1);
                let mutation = format!("mutation{}", i);
                let compensation = format!("compensation{}", i);

                SagaStep::new(
                    i,
                    &subgraph,
                    format!("Entity{}", i),
                    &mutation,
                    json!({
                        "step": i,
                        "data": format!("input_{}", i)
                    }),
                    &compensation,
                    json!({
                        "step": i,
                        "rollback": true
                    }),
                )
            })
            .collect()
    }
}

/// Create coordinator and execute saga creation.
pub async fn execute_saga_scenario(scenario: TestSagaScenario) -> (Vec<SagaStep>, Uuid) {
    let coordinator = SagaCoordinator::new(scenario.compensation_strategy);
    let steps = scenario.build_steps();
    let saga_id = coordinator.create_saga(steps.clone()).await.expect("Failed to create saga");
    (steps, saga_id)
}

/// Execute all steps of a saga.
pub async fn execute_all_steps(saga_id: Uuid, step_count: usize) {
    execute_all_steps_with_failure(saga_id, step_count, None).await;
}

/// Execute steps with optional failure injection at a specific step.
pub async fn execute_all_steps_with_failure(
    saga_id: Uuid,
    step_count: usize,
    fail_at_step: Option<u32>,
) {
    let executor = SagaExecutor::new();

    for step_number in 1..=step_count as u32 {
        let mutation_name = format!("mutation{}", step_number);
        let subgraph = format!("service-{}", step_number % 3 + 1);

        if Some(step_number) == fail_at_step {
            break;
        }

        let result = executor
            .execute_step(
                saga_id,
                step_number,
                &mutation_name,
                &json!({"step": step_number}),
                &subgraph,
            )
            .await;

        assert!(result.is_ok(), "Step {} execution failed", step_number);
        let step_result = result.unwrap();
        assert_eq!(step_result.step_number, step_number);
        assert!(step_result.success, "Step {} should succeed", step_number);
        assert!(step_result.data.is_some(), "Step {} should return data", step_number);
    }
}

/// Execute compensation for a saga in reverse order.
pub async fn execute_compensation(saga_id: Uuid, completed_step_count: usize) {
    let compensator = SagaCompensator::new();

    for step_number in (1..=completed_step_count as u32).rev() {
        let compensation_mutation = format!("compensation{}", step_number);
        let subgraph = format!("service-{}", step_number % 3 + 1);
        let result = compensator
            .compensate_step(
                saga_id,
                step_number,
                &compensation_mutation,
                &json!({"step": step_number}),
                &subgraph,
            )
            .await;

        assert!(result.is_ok(), "Compensation step {} failed", step_number);
        let comp_result = result.unwrap();
        assert_eq!(comp_result.step_number, step_number);
    }
}
