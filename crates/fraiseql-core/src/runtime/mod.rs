//! Runtime query executor - executes compiled queries.
//!
//! # Architecture
//!
//! The runtime loads a CompiledSchema and executes incoming GraphQL queries by:
//! 1. Parsing the GraphQL query
//! 2. Matching it to a compiled query template
//! 3. Binding variables
//! 4. Executing the pre-compiled SQL
//! 5. Projecting JSONB results to GraphQL response
//!
//! # Key Concepts
//!
//! - **Zero runtime compilation**: All SQL is pre-compiled
//! - **Pattern matching**: Match incoming query structure to templates
//! - **Variable binding**: Safe parameter substitution
//! - **Result projection**: JSONB → GraphQL JSON transformation
//!
//! # Example
//!
//! ```ignore
//! use fraiseql_core::runtime::Executor;
//! use fraiseql_core::schema::CompiledSchema;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Load compiled schema
//! let schema = CompiledSchema::from_json(include_str!("schema.compiled.json"))?;
//!
//! // Create executor (connects to database)
//! let executor = Executor::new(schema, db_pool).await?;
//!
//! // Execute GraphQL query
//! let query = r#"query { users { id name } }"#;
//! let result = executor.execute(query, None).await?;
//!
//! println!("{}", result);
//! # Ok(())
//! # }
//! ```

mod aggregate_parser;
mod aggregate_projector;
pub mod aggregation;
mod executor;
mod matcher;
mod planner;
mod projection;
pub mod subscription;
pub mod window;
mod window_parser;
mod window_projector;

pub use aggregate_parser::AggregateQueryParser;
pub use aggregate_projector::AggregationProjector;
pub use aggregation::{AggregationSql, AggregationSqlGenerator};
pub use executor::Executor;
pub use matcher::{QueryMatch, QueryMatcher};
pub use planner::{ExecutionPlan, QueryPlanner};
pub use projection::{FieldMapping, ProjectionMapper, ResultProjector};
pub use subscription::{
    ActiveSubscription, DeliveryResult, KafkaAdapter, KafkaConfig, KafkaMessage, ListenerConfig,
    ListenerHandle, PostgresListener, SubscriptionError, SubscriptionEvent, SubscriptionId,
    SubscriptionManager, SubscriptionOperation, SubscriptionPayload, TransportAdapter,
    TransportManager, WebhookAdapter, WebhookConfig, WebhookPayload, protocol,
};
pub use window::{WindowSql, WindowSqlGenerator};
pub use window_parser::WindowQueryParser;
pub use window_projector::WindowProjector;

use crate::security::{FieldFilter, FieldFilterConfig};

/// Runtime configuration.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Enable query plan caching.
    pub cache_query_plans: bool,

    /// Maximum query depth (prevents deeply nested queries).
    pub max_query_depth: usize,

    /// Maximum query complexity score.
    pub max_query_complexity: usize,

    /// Enable performance tracing.
    pub enable_tracing: bool,

    /// Optional field filter for access control.
    /// When set, validates that users have required scopes to access fields.
    pub field_filter: Option<FieldFilter>,

    /// Query timeout in milliseconds (0 = no timeout).
    pub query_timeout_ms: u64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            cache_query_plans:    true,
            max_query_depth:      10,
            max_query_complexity: 1000,
            enable_tracing:       false,
            field_filter:         None,
            query_timeout_ms:     30_000, // 30 second default timeout
        }
    }
}

impl RuntimeConfig {
    /// Create a new runtime config with a field filter.
    ///
    /// # Example
    ///
    /// ```
    /// use fraiseql_core::runtime::RuntimeConfig;
    /// use fraiseql_core::security::FieldFilterConfig;
    ///
    /// let config = RuntimeConfig::default()
    ///     .with_field_filter(
    ///         FieldFilterConfig::new()
    ///             .protect_field("User", "salary")
    ///             .protect_field("User", "ssn")
    ///     );
    /// ```
    #[must_use]
    pub fn with_field_filter(mut self, config: FieldFilterConfig) -> Self {
        self.field_filter = Some(FieldFilter::new(config));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RuntimeConfig::default();
        assert!(config.cache_query_plans);
        assert_eq!(config.max_query_depth, 10);
        assert_eq!(config.max_query_complexity, 1000);
        assert!(!config.enable_tracing);
    }
}
