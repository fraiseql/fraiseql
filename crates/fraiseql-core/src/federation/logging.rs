//! Structured logging for federation operations.
//!
//! Provides a context struct for federation logs that includes:
//! - Operation metadata (type, query ID, entity count)
//! - Resolution details (strategy, typename, subgraph)
//! - Timing and status information

use std::time::Instant;

use serde::Serialize;

/// Federation operation types for logging.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum FederationOperationType {
    /// Entity resolution (_entities query)
    #[serde(rename = "entity_resolution")]
    EntityResolution,
    /// Service schema resolution (_service query)
    #[serde(rename = "service_schema")]
    ServiceSchema,
    /// Entity resolution via database
    #[serde(rename = "resolve_db")]
    ResolveDb,
    /// Entity resolution via HTTP subgraph
    #[serde(rename = "resolve_http")]
    ResolveHttp,
    /// Mutation execution
    #[serde(rename = "mutation_execute")]
    MutationExecute,
}

/// Federation resolution strategy for logging.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum ResolutionStrategy {
    /// Local resolution (in-memory cache)
    #[serde(rename = "local")]
    Local,
    /// Direct database query
    #[serde(rename = "db")]
    Db,
    /// HTTP request to subgraph
    #[serde(rename = "http")]
    Http,
}

/// Structured log context for federation operations.
#[derive(Debug, Clone, Serialize)]
pub struct FederationLogContext {
    /// Type of federation operation
    pub operation_type: FederationOperationType,

    /// Unique query identifier for correlation
    pub query_id: String,

    /// Total number of entities in request
    pub entity_count: usize,

    /// Number of unique entities (after deduplication)
    pub entity_count_unique: Option<usize>,

    /// Resolution strategy used
    pub strategy: Option<ResolutionStrategy>,

    /// GraphQL typename being resolved
    pub typename: Option<String>,

    /// Subgraph name (for HTTP resolution)
    pub subgraph_name: Option<String>,

    /// Operation duration in milliseconds
    pub duration_ms: f64,

    /// Operation status
    pub status: OperationStatus,

    /// Error message if operation failed
    pub error_message: Option<String>,

    /// HTTP status code (for subgraph requests)
    pub http_status: Option<u16>,

    /// Number of entities resolved
    pub resolved_count: Option<usize>,

    /// Trace ID for distributed tracing correlation
    pub trace_id: Option<String>,

    /// Request ID for end-to-end request correlation
    pub request_id: Option<String>,
}

/// Operation status for federation logs.
#[derive(Debug, Clone, Copy, Serialize)]
pub enum OperationStatus {
    /// Operation started (but not completed)
    #[serde(rename = "started")]
    Started,
    /// Operation completed successfully
    #[serde(rename = "success")]
    Success,
    /// Operation failed with error
    #[serde(rename = "error")]
    Error,
    /// Operation timed out
    #[serde(rename = "timeout")]
    Timeout,
}

impl FederationLogContext {
    /// Create new federation log context.
    pub fn new(
        operation_type: FederationOperationType,
        query_id: String,
        entity_count: usize,
    ) -> Self {
        Self {
            operation_type,
            query_id,
            entity_count,
            entity_count_unique: None,
            strategy: None,
            typename: None,
            subgraph_name: None,
            duration_ms: 0.0,
            status: OperationStatus::Started,
            error_message: None,
            http_status: None,
            resolved_count: None,
            trace_id: None,
            request_id: None,
        }
    }

    /// Set resolution strategy.
    pub fn with_strategy(mut self, strategy: ResolutionStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }

    /// Set typename.
    pub fn with_typename(mut self, typename: String) -> Self {
        self.typename = Some(typename);
        self
    }

    /// Set subgraph name.
    pub fn with_subgraph_name(mut self, subgraph_name: String) -> Self {
        self.subgraph_name = Some(subgraph_name);
        self
    }

    /// Set entity count after deduplication.
    pub fn with_entity_count_unique(mut self, count: usize) -> Self {
        self.entity_count_unique = Some(count);
        self
    }

    /// Set resolved entity count.
    pub fn with_resolved_count(mut self, count: usize) -> Self {
        self.resolved_count = Some(count);
        self
    }

    /// Set HTTP status code.
    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    /// Set trace ID for correlation.
    pub fn with_trace_id(mut self, trace_id: String) -> Self {
        self.trace_id = Some(trace_id);
        self
    }

    /// Set request ID for correlation.
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Mark operation as completed successfully.
    pub fn complete(mut self, duration_ms: f64) -> Self {
        self.status = OperationStatus::Success;
        self.duration_ms = duration_ms;
        self
    }

    /// Mark operation as failed.
    pub fn fail(mut self, duration_ms: f64, error_message: String) -> Self {
        self.status = OperationStatus::Error;
        self.duration_ms = duration_ms;
        self.error_message = Some(error_message);
        self
    }

    /// Mark operation as timed out.
    pub fn timeout(mut self, duration_ms: f64) -> Self {
        self.status = OperationStatus::Timeout;
        self.duration_ms = duration_ms;
        self
    }
}

/// Timer for measuring operation duration.
pub struct LogTimer {
    start: Instant,
}

impl LogTimer {
    /// Create new timer.
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

impl Default for LogTimer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_log_context_creation() {
        let ctx = FederationLogContext::new(
            FederationOperationType::EntityResolution,
            "query-123".to_string(),
            10,
        );

        assert_eq!(ctx.entity_count, 10);
        assert_eq!(ctx.query_id, "query-123");
        assert!(ctx.typename.is_none());
        assert!(ctx.error_message.is_none());
    }

    #[test]
    fn test_federation_log_context_builder() {
        let ctx = FederationLogContext::new(
            FederationOperationType::ResolveDb,
            "query-456".to_string(),
            20,
        )
        .with_strategy(ResolutionStrategy::Db)
        .with_typename("User".to_string())
        .with_entity_count_unique(15)
        .with_resolved_count(15)
        .complete(25.5);

        assert_eq!(ctx.entity_count, 20);
        assert_eq!(ctx.entity_count_unique, Some(15));
        assert_eq!(ctx.resolved_count, Some(15));
        assert_eq!(ctx.duration_ms, 25.5);
        assert!(matches!(ctx.status, OperationStatus::Success));
    }

    #[test]
    fn test_federation_log_context_error() {
        let ctx = FederationLogContext::new(
            FederationOperationType::ResolveHttp,
            "query-789".to_string(),
            5,
        )
        .fail(15.2, "Connection refused".to_string());

        assert!(matches!(ctx.status, OperationStatus::Error));
        assert_eq!(ctx.error_message, Some("Connection refused".to_string()));
        assert_eq!(ctx.duration_ms, 15.2);
    }

    #[test]
    fn test_log_timer_elapsed() {
        let timer = LogTimer::new();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 10.0);
        assert!(elapsed < 100.0); // Should be much less than 100ms
    }

    #[test]
    fn test_federation_log_context_serialization() {
        let ctx = FederationLogContext::new(
            FederationOperationType::EntityResolution,
            "query-123".to_string(),
            10,
        )
        .with_strategy(ResolutionStrategy::Db)
        .with_typename("User".to_string())
        .complete(25.5);

        let json = serde_json::to_string(&ctx).expect("JSON serialization failed");
        assert!(json.contains("\"entity_count\":10"));
        assert!(json.contains("\"duration_ms\":25.5"));
        assert!(json.contains("\"status\":\"success\""));
    }
}
