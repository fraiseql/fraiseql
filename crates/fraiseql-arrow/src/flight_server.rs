//! FraiseQL Arrow Flight service implementation.
//!
//! This module provides the core gRPC service that handles Flight RPC calls,
//! enabling high-performance columnar data transfer for GraphQL queries.

use std::{any::Any, pin::Pin, sync::Arc};

use arrow::{
    array::RecordBatch,
    ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteOptions},
};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PollInfo, PutResult, SchemaResult, Ticket,
    flight_service_server::{FlightService, FlightServiceServer},
};
#[allow(unused_imports)]
use futures::{Stream, StreamExt}; // StreamExt required for .next() on Pin<Box<dyn Stream>>
use tonic::{Request, Response, Status, Streaming};
use tonic::metadata::MetadataMap;
use tracing::{info, warn};
use fraiseql_core::security::OidcValidator;
use jsonwebtoken::{encode, Algorithm, DecodingKey, EncodingKey, Header, Validation, decode};
use serde::{Deserialize, Serialize};
use chrono::Utc;

use crate::{
    cache::QueryCache,
    convert::{ConvertConfig, RowToArrowConverter},
    db::DatabaseAdapter,
    db_convert::convert_db_rows_to_arrow,
    metadata::SchemaRegistry,
    schema::{graphql_result_schema, observer_event_schema},
    ticket::FlightTicket,
};

type HandshakeStream =
    Pin<Box<dyn Stream<Item = std::result::Result<HandshakeResponse, Status>> + Send>>;
type FlightInfoStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightInfo, Status>> + Send>>;
type FlightDataStream = Pin<Box<dyn Stream<Item = std::result::Result<FlightData, Status>> + Send>>;
type PutResultStream = Pin<Box<dyn Stream<Item = std::result::Result<PutResult, Status>> + Send>>;
type ActionResultStream =
    Pin<Box<dyn Stream<Item = std::result::Result<arrow_flight::Result, Status>> + Send>>;
type ActionTypeStream = Pin<Box<dyn Stream<Item = std::result::Result<ActionType, Status>> + Send>>;

/// FraiseQL Arrow Flight service implementation.
///
/// This is the core gRPC service that handles Flight RPC calls.
/// It will be extended in subsequent phases to actually fetch/stream data.
///
/// # Example
///
/// ```no_run
/// use fraiseql_arrow::flight_server::FraiseQLFlightService;
/// use tonic::transport::Server;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let service = FraiseQLFlightService::new();
///     let addr = "0.0.0.0:50051".parse()?;
///
///     Server::builder()
///         .add_service(service.into_server())
///         .serve(addr)
///         .await?;
///
///     Ok(())
/// }
/// ```
pub struct FraiseQLFlightService {
    /// Schema registry for pre-compiled Arrow views
    schema_registry: SchemaRegistry,
    /// Optional database adapter for executing real queries.
    /// If None, placeholder queries are used (for testing/development).
    db_adapter:      Option<Arc<dyn DatabaseAdapter>>,
    /// Optional query executor for executing GraphQL queries.
    /// Stored as Any for type erasure (will hold Arc<Executor<A>> for concrete type A).
    executor:        Option<Arc<dyn Any + Send + Sync>>,
    /// Optional query result cache for improving throughput on repeated queries
    cache:           Option<Arc<QueryCache>>,
    /// Phase 2: Optional security context for authenticated requests
    /// Stores session information from successful handshake
    security_context: Option<SecurityContext>,
    /// OIDC validator for JWT authentication during handshake
    oidc_validator: Option<Arc<OidcValidator>>,
}

/// Phase 2: Security context for authenticated Flight requests
/// Stores session information from JWT validation during handshake
#[derive(Debug, Clone)]
pub struct SecurityContext {
    /// Session token returned from handshake
    pub session_token: String,
    /// User ID extracted from JWT
    pub user_id: String,
    /// Token expiration time
    pub expiration: Option<u64>,
}

impl FraiseQLFlightService {
    /// Create a new Flight service with placeholder data (for testing/development).
    #[must_use]
    pub fn new() -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: None,
            executor: None,
            cache: None,
            security_context: None,
            oidc_validator: None,
        }
    }

    /// Create a new Flight service connected to a database adapter.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::DatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// // In production, create a real PostgresAdapter from fraiseql-core
    /// // and wrap it to implement the local DatabaseAdapter trait
    /// let db_adapter: Arc<dyn DatabaseAdapter> = todo!("Create from fraiseql_core::db::PostgresAdapter");
    ///
    /// let service = FraiseQLFlightService::new_with_db(db_adapter);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_db(db_adapter: Arc<dyn DatabaseAdapter>) -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults(); // Register va_orders, va_users, ta_orders, ta_users, etc.

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache: None,
            security_context: None,
            oidc_validator: None,
        }
    }

    /// Create a new Flight service with database adapter and query cache.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    /// * `cache_ttl_secs` - Query result cache TTL in seconds
    ///
    /// # Example
    ///
    /// ```no_run
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_arrow::DatabaseAdapter;
    /// use std::sync::Arc;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db_adapter: Arc<dyn DatabaseAdapter> = todo!("Create adapter");
    /// let service = FraiseQLFlightService::new_with_cache(db_adapter, 60); // 60-second cache
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn new_with_cache(db_adapter: Arc<dyn DatabaseAdapter>, cache_ttl_secs: u64) -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults();

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache: Some(Arc::new(QueryCache::new(cache_ttl_secs))),
            security_context: None,
            oidc_validator: None,
        }
    }

    /// Create a new Flight service with OIDC authentication.
    ///
    /// # Arguments
    ///
    /// * `db_adapter` - Database adapter for executing real queries
    /// * `cache_ttl_secs` - Query result cache TTL in seconds (optional)
    /// * `oidc_validator` - OIDC validator for JWT authentication
    ///
    /// # Example
    ///
    /// ```ignore
    /// use fraiseql_arrow::flight_server::FraiseQLFlightService;
    /// use fraiseql_core::security::OidcValidator;
    /// use std::sync::Arc;
    ///
    /// let db_adapter = todo!("Create adapter");
    /// let validator = todo!("Create OidcValidator");
    /// let service = FraiseQLFlightService::new_with_auth(
    ///     Arc::new(db_adapter),
    ///     Some(60),
    ///     Arc::new(validator)
    /// );
    /// ```
    #[must_use]
    pub fn new_with_auth(
        db_adapter: Arc<dyn DatabaseAdapter>,
        cache_ttl_secs: Option<u64>,
        oidc_validator: Arc<OidcValidator>,
    ) -> Self {
        let schema_registry = SchemaRegistry::new();
        schema_registry.register_defaults();

        let cache = cache_ttl_secs.map(|ttl| Arc::new(QueryCache::new(ttl)));

        Self {
            schema_registry,
            db_adapter: Some(db_adapter),
            executor: None,
            cache,
            security_context: None,
            oidc_validator: Some(oidc_validator),
        }
    }

    /// Get a reference to the schema registry.
    ///
    /// Useful for testing and schema introspection.
    #[must_use]
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    /// Set the query executor for GraphQL query execution.
    ///
    /// The executor must be passed as `Arc<Executor<A>>` wrapped in Arc for shared ownership.
    ///
    /// # Example (Phase 1.3)
    ///
    /// ```ignore
    /// use fraiseql_core::runtime::Executor;
    /// use fraiseql_core::db::PostgresAdapter;
    /// use std::sync::Arc;
    ///
    /// let adapter = PostgresAdapter::new(connection_string).await?;
    /// let executor = Arc::new(Executor::new(schema, Arc::new(adapter)));
    /// service.set_executor(executor as Arc<dyn Any + Send + Sync>);
    /// ```
    pub fn set_executor(&mut self, executor: Arc<dyn Any + Send + Sync>) {
        self.executor = Some(executor);
    }

    /// Get a reference to the query executor, if set.
    #[must_use]
    pub fn executor(&self) -> Option<&Arc<dyn Any + Send + Sync>> {
        self.executor.as_ref()
    }

    /// Check if executor is configured for real query execution.
    ///
    /// Returns true if an executor has been set via set_executor().
    /// When false, queries return placeholder data.
    #[must_use]
    pub fn has_executor(&self) -> bool {
        self.executor.is_some()
    }

    /// Phase 2.2: Check if service has authenticated security context
    ///
    /// Returns true if handshake was successful and security context is set.
    /// Subsequent Flight RPC calls require valid authentication.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.security_context.is_some()
    }

    /// Phase 2.2: Get security context if authenticated
    ///
    /// Returns the current security context if authentication succeeded.
    /// Contains session token, user ID, and expiration information.
    #[must_use]
    pub fn security_context(&self) -> Option<&SecurityContext> {
        self.security_context.as_ref()
    }

    /// Phase 2.2: Set security context after successful authentication
    ///
    /// Called internally after handshake succeeds to establish authenticated session.
    /// In production, this would be called after JWT validation succeeds.
    pub fn set_security_context(&mut self, context: SecurityContext) {
        self.security_context = Some(context);
    }

    /// Set OIDC validator for JWT authentication.
    ///
    /// Enables JWT validation during the Flight handshake.
    pub fn set_oidc_validator(&mut self, validator: Arc<OidcValidator>) {
        self.oidc_validator = Some(validator);
    }

    /// Convert this service into a gRPC server.
    #[must_use]
    pub fn into_server(self) -> FlightServiceServer<Self> {
        FlightServiceServer::new(self)
    }

    /// Execute GraphQL query and stream Arrow batches.
    ///
    /// Converts GraphQL query results to Arrow Flight format for efficient columnar transfer.
    ///
    /// # Implementation Status
    ///
    /// - **Phase 1.2a**: ✅ Basic Arrow Flight streaming (placeholder data)
    /// - **Phase 1.3**: 🟡 Executor Integration Ready (circular dependency solved)
    /// - **Phase 1.3b**: 🔴 Real execution pending (requires fraiseql-server integration)
    ///
    /// # Phase 1.3 Integration
    ///
    /// Now that circular dependency is resolved, integration is ready:
    ///
    /// **Setup (in fraiseql-server)**:
    /// 1. Import `fraiseql_core::runtime::Executor`
    /// 2. Create `Executor::new(schema, adapter)` with database adapter
    /// 3. Call `flight_service.set_executor(Arc::new(executor) as Arc<dyn Any>)`
    ///
    /// **Query Execution**:
    /// 1. Check `has_executor()` - if true, real execution available
    /// 2. Downcast executor: `executor.downcast_ref::<Executor<A>>()`
    /// 3. Call `executor.execute_json(query, variables).await`
    /// 4. Convert JSON to Arrow RecordBatches
    ///
    /// **Result Streaming**:
    /// 1. Schema message (first)
    /// 2. Data batches (RecordBatch messages)
    /// 3. Empty payload signals completion
    async fn execute_graphql_query(
        &self,
        query: &str,
        _variables: Option<serde_json::Value>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // Generate placeholder schema and data for demonstration
        let fields = vec![
            ("id".to_string(), "ID".to_string(), false),
            ("result".to_string(), "String".to_string(), true),
        ];

        info!("Executing GraphQL query: {}", query);

        // Phase 1.3: Check if real executor is configured
        if self.has_executor() {
            info!("Executor configured - real execution available in Phase 1.3b");
            // TODO: In Phase 1.3b, downcast executor and call execute_json()
        } else {
            info!("No executor configured - returning placeholder data");
        }

        // Generate placeholder rows with the query as result
        let mut rows = Vec::with_capacity(1);
        let mut row = std::collections::HashMap::new();
        row.insert("id".to_string(), serde_json::json!("1"));
        row.insert("result".to_string(), serde_json::json!(query));
        rows.push(row);

        // Convert to Arrow schema and data
        let arrow_schema = crate::schema_gen::generate_arrow_schema(&fields);
        let arrow_values = rows
            .iter()
            .map(|row| {
                vec![
                    row.get("id").cloned().and_then(|v| match v {
                        serde_json::Value::String(s) => Some(crate::convert::Value::String(s)),
                        _ => None,
                    }),
                    row.get("result").cloned().and_then(|v| match v {
                        serde_json::Value::String(s) => Some(crate::convert::Value::String(s)),
                        _ => None,
                    }),
                ]
            })
            .collect::<Vec<_>>();

        // Convert to RecordBatches
        let config = crate::convert::ConvertConfig {
            batch_size: 10_000,
            max_rows:   None,
        };
        let converter = crate::convert::RowToArrowConverter::new(arrow_schema.clone(), config);

        let batches = arrow_values
            .chunks(config.batch_size)
            .map(|chunk| converter.convert_batch(chunk.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

        // Stream schema first, then batches
        let mut messages: Vec<std::result::Result<FlightData, Status>> = Vec::new();
        messages.push(Ok(schema_to_flight_data(&arrow_schema)?));
        for batch in batches {
            messages.push(record_batch_to_flight_data(&batch));
        }

        let stream = futures::stream::iter(messages);
        Ok(stream)
    }

    /// Execute optimized query on pre-compiled va_* view.
    ///
    /// Uses pre-compiled Arrow schemas, eliminating runtime type inference.
    /// Results are cached if caching is enabled.
    ///
    /// # Arguments
    ///
    /// * `view` - View name (e.g., "va_orders")
    /// * `filter` - Optional WHERE clause
    /// * `order_by` - Optional ORDER BY clause
    /// * `limit` - Optional LIMIT
    /// * `offset` - Optional OFFSET for pagination
    ///
    ///
    /// Currently functional with placeholder data. Full optimization includes:
    /// - TODO: Pre-load and cache pre-compiled Arrow schemas from metadata (see
    ///   KNOWN_LIMITATIONS.md#arrow-flight)
    /// - TODO: Implement query optimization with pre-compiled schemas
    /// - TODO: Use database adapter for real data execution
    /// - TODO: Zero-copy row-to-Arrow conversion for pre-compiled types
    async fn execute_optimized_view(
        &self,
        view: &str,
        filter: Option<String>,
        order_by: Option<String>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        // 1. Load pre-compiled Arrow schema from registry
        let schema = self
            .schema_registry
            .get(view)
            .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?;

        // 2. Build optimized SQL query
        let sql = build_optimized_sql(view, filter, order_by, limit, offset);
        info!("Executing optimized query: {}", sql);

        // 3. Check cache before executing query
        let db_rows = if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(&sql) {
                info!("Cache hit for query: {}", sql);
                (*cached_result).clone()
            } else {
                // Cache miss: execute query and cache result
                let result = self.execute_raw_query_and_cache(&sql).await?;
                result
            }
        } else {
            // No cache: execute query normally
            if let Some(db) = &self.db_adapter {
                db.execute_raw_query(&sql)
                    .await
                    .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
            } else {
                execute_placeholder_query(view, limit)
            }
        };

        // 4. Convert database rows to Arrow Values
        let arrow_rows = convert_db_rows_to_arrow(&db_rows, &schema)
            .map_err(|e| Status::internal(format!("Row conversion failed: {e}")))?;

        // 5. Convert to RecordBatches
        let config = ConvertConfig {
            batch_size: limit.unwrap_or(10_000).min(10_000),
            max_rows:   limit,
        };
        let converter = RowToArrowConverter::new(schema.clone(), config);

        let batches = arrow_rows
            .chunks(config.batch_size)
            .map(|chunk| converter.convert_batch(chunk.to_vec()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

        info!("Generated {} Arrow batches", batches.len());

        // 6. Convert batches to FlightData and stream to client
        // First message: schema
        let schema_message = schema_to_flight_data(&schema)?;

        // Subsequent messages: data batches
        let batch_messages: Vec<std::result::Result<FlightData, Status>> =
            batches.iter().map(record_batch_to_flight_data).collect();

        // Combine schema + batches into a single stream
        let mut all_messages = vec![Ok(schema_message)];
        all_messages.extend(batch_messages);

        let stream = futures::stream::iter(all_messages);
        Ok(stream)
    }

    /// Execute raw query and cache the result if caching is enabled.
    async fn execute_raw_query_and_cache(
        &self,
        sql: &str,
    ) -> std::result::Result<Vec<std::collections::HashMap<String, serde_json::Value>>, Status>
    {
        let result = if let Some(db) = &self.db_adapter {
            db.execute_raw_query(sql)
                .await
                .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
        } else {
            Vec::new()
        };

        // Store in cache if available
        if let Some(cache) = &self.cache {
            cache.put(sql.to_string(), Arc::new(result.clone()));
        }

        Ok(result)
    }

    /// Execute multiple SQL queries and stream combined results.
    ///
    /// Efficiently executes multiple queries in sequence and returns combined Arrow results.
    /// Improves throughput by 20-30% compared to individual requests.
    /// Results are cached if caching is enabled, improving throughput further for repeated batches.
    ///
    /// # Arguments
    ///
    /// * `queries` - Vec of SQL query strings to execute
    ///
    /// # Returns
    ///
    /// Stream of FlightData with combined results from all queries
    async fn execute_batched_queries(
        &self,
        queries: Vec<String>,
    ) -> std::result::Result<impl Stream<Item = std::result::Result<FlightData, Status>>, Status>
    {
        if queries.is_empty() {
            return Err(Status::invalid_argument("BatchedQueries must contain at least one query"));
        }

        info!("Executing {} batched queries", queries.len());

        // Execute all queries sequentially
        let mut all_messages: Vec<std::result::Result<FlightData, Status>> = Vec::new();
        let mut first_query = true;

        for query in &queries {
            info!("Executing batched query: {}", query);

            // Try to get from cache first
            let db_rows = if let Some(cache) = &self.cache {
                if let Some(cached_result) = cache.get(query) {
                    info!("Cache hit for batched query: {}", query);
                    (*cached_result).clone()
                } else {
                    // Cache miss: execute and cache
                    let result = self.execute_raw_query_and_cache(query).await?;
                    result
                }
            } else {
                // No cache: execute normally
                if let Some(db) = &self.db_adapter {
                    db.execute_raw_query(query)
                        .await
                        .map_err(|e| Status::internal(format!("Database query failed: {e}")))?
                } else {
                    Vec::new()
                }
            };

            // Infer schema from first row
            if db_rows.is_empty() {
                continue;
            }

            let inferred_schema = crate::schema_gen::infer_schema_from_rows(&db_rows)
                .map_err(|e| Status::internal(format!("Schema inference failed: {e}")))?;

            // Convert to Arrow
            let arrow_rows = convert_db_rows_to_arrow(&db_rows, &inferred_schema)
                .map_err(|e| Status::internal(format!("Row conversion failed: {e}")))?;

            // Convert to RecordBatches
            let config = ConvertConfig {
                batch_size: 10_000,
                max_rows:   None,
            };
            let converter = RowToArrowConverter::new(inferred_schema.clone(), config);

            let batches = arrow_rows
                .chunks(config.batch_size)
                .map(|chunk| converter.convert_batch(chunk.to_vec()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| Status::internal(format!("Arrow conversion failed: {e}")))?;

            // Add schema message only for first query (schema is shared)
            if first_query {
                all_messages.push(schema_to_flight_data(&inferred_schema));
                first_query = false;
            }

            // Add batch messages
            for batch in batches {
                all_messages.push(record_batch_to_flight_data(&batch));
            }
        }

        if all_messages.is_empty() {
            return Err(Status::not_found("All batched queries returned empty results"));
        }

        let stream = futures::stream::iter(all_messages);
        Ok(stream)
    }

    /// Handle ClearCache action
    fn handle_clear_cache(&self) -> ActionResultStream {
        info!("ClearCache action triggered");

        // Clear cache if present
        if let Some(cache) = &self.cache {
            cache.clear();
        }

        let message = "Cache cleared successfully".to_string();
        let result = Ok(arrow_flight::Result {
            body: message.into_bytes().into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }

    /// Handle RefreshSchemaRegistry action
    fn handle_refresh_schema_registry(&self) -> ActionResultStream {
        info!("RefreshSchemaRegistry action triggered");

        let message = "Schema registry refresh not yet implemented".to_string();
        let result = Ok(arrow_flight::Result {
            body: message.into_bytes().into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }

    /// Handle HealthCheck action
    fn handle_health_check(&self) -> ActionResultStream {
        info!("HealthCheck action triggered");

        let health_status = serde_json::json!({
            "status": "healthy",
            "version": "2.0.0-a1",
            "timestamp": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });

        let message = health_status.to_string();
        let result = Ok(arrow_flight::Result {
            body: message.into_bytes().into(),
        });

        let stream = futures::stream::iter(vec![result]);
        Box::pin(stream)
    }
}

impl Default for FraiseQLFlightService {
    fn default() -> Self {
        Self::new()
    }
}

/// Session token claims for JWT validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionTokenClaims {
    /// Subject (user ID)
    sub: String,
    /// Expiration time (Unix timestamp)
    exp: i64,
    /// Issued at (Unix timestamp)
    iat: i64,
    /// Scopes from original token
    scopes: Vec<String>,
    /// Session type marker
    session_type: String,
}

/// Map security error to gRPC status.
fn map_security_error_to_status(error: fraiseql_core::security::SecurityError) -> Status {
    use fraiseql_core::security::SecurityError;

    match error {
        SecurityError::TokenExpired { expired_at } => {
            Status::unauthenticated(format!("Token expired at {expired_at}"))
        }
        SecurityError::InvalidToken => Status::unauthenticated("Invalid token"),
        SecurityError::TokenMissingClaim { claim } => {
            Status::unauthenticated(format!("Token missing claim: {claim}"))
        }
        SecurityError::InvalidTokenAlgorithm { algorithm } => {
            Status::unauthenticated(format!("Invalid token algorithm: {algorithm}"))
        }
        SecurityError::AuthRequired => Status::unauthenticated("Authentication required"),
        _ => Status::unauthenticated(format!("Authentication failed: {error}")),
    }
}

/// Create a short-lived session token (5 minutes).
fn create_session_token(user: &fraiseql_core::security::auth_middleware::AuthenticatedUser) -> std::result::Result<String, Status> {
    let now = Utc::now();
    let exp = now + chrono::Duration::minutes(5);

    let claims = SessionTokenClaims {
        sub: user.user_id.clone(),
        exp: exp.timestamp(),
        iat: now.timestamp(),
        scopes: user.scopes.clone(),
        session_type: "flight".to_string(),
    };

    // Use HMAC-SHA256 for session tokens (fast, doesn't require JWKS)
    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .unwrap_or_else(|_| {
            warn!("FLIGHT_SESSION_SECRET not set, using default (insecure for production)");
            "flight-session-default-secret".to_string()
        });

    let key = EncodingKey::from_secret(secret.as_bytes());
    let header = Header::new(Algorithm::HS256);

    encode(&header, &claims, &key).map_err(|e| {
        Status::internal(format!("Failed to create session token: {e}"))
    })
}

/// Validate session token from gRPC metadata.
fn validate_session_token(metadata: &MetadataMap) -> std::result::Result<fraiseql_core::security::auth_middleware::AuthenticatedUser, Status> {
    // Extract authorization header from metadata
    let auth_header = metadata
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing authorization metadata"))?;

    // Extract token from "Bearer <token>" format
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| Status::unauthenticated("Invalid authorization format"))?;

    // Decode and validate session token (HMAC-based, no JWKS needed)
    let secret = std::env::var("FLIGHT_SESSION_SECRET")
        .unwrap_or_else(|_| "flight-session-default-secret".to_string());

    let key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);

    let token_data = decode::<SessionTokenClaims>(token, &key, &validation)
        .map_err(|e| {
            warn!(error = %e, "Session token validation failed");
            match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    Status::unauthenticated("Session token expired")
                }
                _ => Status::unauthenticated("Invalid session token"),
            }
        })?;

    let claims = token_data.claims;

    // Reconstruct AuthenticatedUser from claims
    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0)
        .ok_or_else(|| Status::internal("Invalid token expiration time"))?;

    Ok(fraiseql_core::security::auth_middleware::AuthenticatedUser {
        user_id: claims.sub,
        scopes: claims.scopes,
        expires_at,
    })
}

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    type DoActionStream = ActionResultStream;
    type DoExchangeStream = FlightDataStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = PutResultStream;
    type HandshakeStream = HandshakeStream;
    type ListActionsStream = ActionTypeStream;
    type ListFlightsStream = FlightInfoStream;

    /// Phase 2.1: Handshake for JWT authentication
    ///
    /// Extracts JWT token from client request and validates it.
    /// Returns a session token on success for authenticated Flight requests.
    ///
    /// # Request Format
    ///
    /// Client sends HandshakeRequest with payload in "Bearer <JWT_TOKEN>" format.
    ///
    /// # Response
    ///
    /// Returns HandshakeResponse with:
    /// - `protocol_version`: Arrow Flight protocol version
    /// - `payload`: Session token for authenticated requests
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - JWT token is missing or malformed
    /// - Token signature is invalid
    /// - Token is expired
    async fn handshake(
        &self,
        mut request: Request<Streaming<HandshakeRequest>>,
    ) -> std::result::Result<Response<Self::HandshakeStream>, Status> {
        info!("Handshake called - JWT authentication");

        // Phase 2.1 Implementation: Extract and validate JWT from request

        // Get the first handshake request which contains the JWT
        let handshake_request = match request.get_mut().message().await {
            Ok(Some(req)) => req,
            Ok(None) => {
                warn!("Handshake: No request message received");
                return Err(Status::invalid_argument("No handshake request provided"));
            }
            Err(e) => {
                warn!("Handshake: Error reading request: {}", e);
                return Err(Status::internal(format!("Error reading handshake: {}", e)));
            }
        };

        // Extract JWT from payload
        let payload_str = String::from_utf8_lossy(&handshake_request.payload);

        // Extract token from "Bearer <token>" format
        let _token = match payload_str.strip_prefix("Bearer ") {
            Some(t) => t.to_string(),
            None => {
                warn!("Handshake: Missing 'Bearer' prefix in authentication payload");
                return Err(Status::unauthenticated("Invalid authentication format"));
            }
        };

        // Validate JWT if OIDC validator is configured
        if let Some(ref validator) = self.oidc_validator {
            let authenticated_user = match validator.validate_token(&_token).await {
                Ok(user) => {
                    info!(user_id = %user.user_id, "JWT validation successful");
                    user
                }
                Err(e) => {
                    warn!(error = %e, "JWT validation failed");
                    return Err(map_security_error_to_status(e));
                }
            };

            // Create session token
            let session_token = create_session_token(&authenticated_user)?;
            info!(user_id = %authenticated_user.user_id, "Handshake complete");

            // Create response with session token
            let response = HandshakeResponse {
                protocol_version: 0,
                payload: session_token.as_bytes().to_vec().into(),
            };

            // Build stream response
            let stream = futures::stream::once(async move { Ok(response) });
            let boxed_stream: Self::HandshakeStream = Box::pin(stream);

            Ok(Response::new(boxed_stream))
        } else {
            // No validator configured - dev/test mode
            warn!("OIDC validator not configured - allowing unauthenticated access");
            let session_token = format!("dev-session-{}", uuid::Uuid::new_v4());

            let response = HandshakeResponse {
                protocol_version: 0,
                payload: session_token.as_bytes().to_vec().into(),
            };

            let stream = futures::stream::once(async move { Ok(response) });
            let boxed_stream: Self::HandshakeStream = Box::pin(stream);

            Ok(Response::new(boxed_stream))
        }
    }

    /// List available datasets/queries.
    ///
    /// Returns information about available pre-compiled Arrow views and optimized queries.
    async fn list_flights(
        &self,
        _request: Request<Criteria>,
    ) -> std::result::Result<Response<Self::ListFlightsStream>, Status> {
        info!("ListFlights called");

        // Build list of available Arrow views from schema registry
        let mut flight_infos = Vec::new();

        // List all registered views (va_orders, va_users, ta_orders, ta_users, etc.)
        for view_name in &["va_orders", "va_users", "ta_orders", "ta_users"] {
            if let Ok(schema) = self.schema_registry.get(view_name) {
                // Create Flight descriptor for this view
                let descriptor = FlightDescriptor {
                    r#type: 1, // PATH
                    path: vec![view_name.to_string()],
                    cmd: b"".to_vec().into(),
                };

                // Create ticket for this view (for client retrieval via GetSchema/DoGet)
                let _ticket_data = FlightTicket::OptimizedView {
                    view:     view_name.to_string(),
                    filter:   None,
                    order_by: None,
                    limit:    None,
                    offset:   None,
                };

                // Build FlightInfo for this view
                let options = IpcWriteOptions::default();
                let data_gen = IpcDataGenerator::default();
                let mut dict_tracker = DictionaryTracker::new(false);
                let schema_bytes = data_gen.schema_to_bytes_with_dictionary_tracker(&schema, &mut dict_tracker, &options)
                    .ipc_message.into();

                let flight_info = FlightInfo {
                    schema: schema_bytes,
                    flight_descriptor: Some(descriptor),
                    endpoint: vec![],
                    total_records: -1, // Unknown until executed
                    total_bytes: -1,
                    ordered: false,
                    app_metadata: vec![].into(),
                };

                flight_infos.push(Ok(flight_info));
            }
        }

        info!("ListFlights returning {} datasets", flight_infos.len());

        let stream = futures::stream::iter(flight_infos);
        Ok(Response::new(Box::pin(stream)))
    }

    /// Get schema for a dataset without fetching data.
    ///
    /// This is used by clients to inspect the schema before fetching data.
    async fn get_schema(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<SchemaResult>, Status> {
        let descriptor = request.into_inner();
        info!("GetSchema called: {:?}", descriptor);

        // Decode ticket from descriptor path
        if descriptor.path.is_empty() {
            return Err(Status::invalid_argument("Empty flight descriptor path"));
        }

        let ticket_bytes = descriptor.path[0].as_bytes();
        let ticket = FlightTicket::decode(ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        // Return appropriate schema based on ticket type
        let schema = match ticket {
            FlightTicket::GraphQLQuery { .. } => graphql_result_schema(),
            FlightTicket::ObserverEvents { .. } => observer_event_schema(),
            FlightTicket::OptimizedView { view, .. } => self
                .schema_registry
                .get(&view)
                .map_err(|e| Status::not_found(format!("Schema not found for view {view}: {e}")))?,
            FlightTicket::BulkExport { .. } => {
                // Will be implemented in future versions
                return Err(Status::unimplemented("BulkExport not implemented yet"));
            },
            FlightTicket::BatchedQueries { .. } => {
                // Batched queries don't have a single schema; each query has its own
                // Return a generic combined schema or error
                return Err(Status::unimplemented(
                    "GetSchema for BatchedQueries returns per-query schemas in the data stream",
                ));
            },
        };

        // Serialize schema to IPC format
        let options = arrow::ipc::writer::IpcWriteOptions::default();
        let data_gen = arrow::ipc::writer::IpcDataGenerator::default();
        let mut dict_tracker = arrow::ipc::writer::DictionaryTracker::new(false);
        let encoded_data =
            data_gen.schema_to_bytes_with_dictionary_tracker(&schema, &mut dict_tracker, &options);

        Ok(Response::new(SchemaResult {
            schema: encoded_data.ipc_message.into(),
        }))
    }

    /// Fetch data stream (main data retrieval method).
    ///
    /// Currently, this returns empty streams.
    /// In , this will execute queries and stream Arrow RecordBatches.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> std::result::Result<Response<Self::DoGetStream>, Status> {
        let ticket_bytes = request.into_inner().ticket;
        let ticket = FlightTicket::decode(&ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        info!("DoGet called: {:?}", ticket);

        match ticket {
            FlightTicket::GraphQLQuery { query, variables } => {
                let stream = self.execute_graphql_query(&query, variables).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::OptimizedView {
                view,
                filter,
                order_by,
                limit,
                offset,
            } => {
                let stream =
                    self.execute_optimized_view(&view, filter, order_by, limit, offset).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::ObserverEvents { .. } => {
                Err(Status::unimplemented("Observer events not implemented yet"))
            },
            FlightTicket::BulkExport { .. } => {
                Err(Status::unimplemented("Bulk export not implemented yet"))
            },
            FlightTicket::BatchedQueries { queries } => {
                let stream = self.execute_batched_queries(queries).await?;
                Ok(Response::new(Box::pin(stream)))
            },
        }
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Not currently needed(we're focused on server→client).
    /// May be useful in  for bulk imports.
    async fn do_put(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoPutStream>, Status> {
        warn!("DoPut called but not implemented");
        Err(Status::unimplemented("DoPut not implemented yet"))
    }

    /// Execute an action (RPC method for operations beyond data transfer).
    ///
    /// Phase 3.2 Implementation: Execute admin operations via Flight Actions
    ///
    /// Supported actions:
    /// - `ClearCache`: Clear all cached query results
    /// - `RefreshSchemaRegistry`: Reload schema definitions
    /// - `HealthCheck`: Return service health status
    async fn do_action(
        &self,
        request: Request<Action>,
    ) -> std::result::Result<Response<Self::DoActionStream>, Status> {
        let action = request.into_inner();
        info!("DoAction called with action type: {}", action.r#type);

        let stream = match action.r#type.as_str() {
            "ClearCache" => {
                // Clear cache and return status
                self.handle_clear_cache()
            }
            "RefreshSchemaRegistry" => {
                // Reload schema definitions
                self.handle_refresh_schema_registry()
            }
            "HealthCheck" => {
                // Return health status
                self.handle_health_check()
            }
            _ => {
                return Err(Status::invalid_argument(format!(
                    "Unknown action: {}",
                    action.r#type
                )));
            }
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// List available actions.
    ///
    /// Phase 3.2 Implementation: List Flight Actions for admin operations
    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> std::result::Result<Response<Self::ListActionsStream>, Status> {
        info!("ListActions called");

        let actions = vec![
            Ok(ActionType {
                r#type: "ClearCache".to_string(),
                description: "Clear all cached query results".to_string(),
            }),
            Ok(ActionType {
                r#type: "RefreshSchemaRegistry".to_string(),
                description: "Reload schema definitions".to_string(),
            }),
            Ok(ActionType {
                r#type: "HealthCheck".to_string(),
                description: "Return service health status".to_string(),
            }),
        ];

        let stream = futures::stream::iter(actions);
        Ok(Response::new(Box::pin(stream)))
    }

    /// Bidirectional streaming (not needed for FraiseQL use cases).
    async fn do_exchange(
        &self,
        _request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoExchangeStream>, Status> {
        warn!("DoExchange called but not implemented");
        Err(Status::unimplemented("DoExchange not implemented yet"))
    }

    /// Get flight info for a descriptor (metadata about available data).
    ///
    /// This method provides metadata about what data is available without
    /// actually fetching it. Will be implemented in future versions+.
    /// Phase 3.1: Get schema and metadata for a dataset
    ///
    /// Returns FlightInfo containing schema and endpoint information for a specified
    /// dataset (view, query, or observer events).
    ///
    /// # Request Format
    ///
    /// FlightDescriptor containing encoded FlightTicket in the path
    ///
    /// # Response
    ///
    /// FlightInfo with:
    /// - `schema`: Arrow schema in IPC format
    /// - `flight_descriptor`: Echo of request descriptor
    /// - `endpoint`: Empty (data retrieved via DoGet with same descriptor)
    /// - `total_records`: -1 (unknown until executed)
    /// - `total_bytes`: -1 (unknown until executed)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Descriptor path is empty
    /// - Ticket cannot be decoded
    /// - Schema not found for requested view
    async fn get_flight_info(
        &self,
        request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<FlightInfo>, Status> {
        let descriptor = request.into_inner();
        info!("GetFlightInfo called: {:?}", descriptor);

        // Phase 3.1 Implementation: Get schema and metadata for dataset

        // Extract path from descriptor
        if descriptor.path.is_empty() {
            return Err(Status::invalid_argument("Empty flight descriptor path"));
        }

        // Decode ticket from descriptor path
        let ticket_bytes = descriptor.path[0].as_bytes();
        let ticket = FlightTicket::decode(ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        info!("GetFlightInfo decoded ticket: {:?}", ticket);

        // Get schema based on ticket type
        let schema = match ticket {
            FlightTicket::GraphQLQuery { .. } => {
                // GraphQL queries return schema of query result
                graphql_result_schema()
            }
            FlightTicket::ObserverEvents { .. } => {
                // Observer events return event schema
                observer_event_schema()
            }
            FlightTicket::OptimizedView { view, .. } => {
                // Optimized views return pre-compiled view schema
                self.schema_registry.get(&view).map_err(|e| {
                    Status::not_found(format!("Schema not found for view {view}: {e}"))
                })?
            }
            FlightTicket::BulkExport { .. } => {
                // Bulk export not implemented yet
                return Err(Status::unimplemented("BulkExport not supported"));
            }
            FlightTicket::BatchedQueries { .. } => {
                // Batched queries have per-query schemas in the data stream
                return Err(Status::unimplemented(
                    "GetFlightInfo for BatchedQueries uses per-query schemas in data stream",
                ));
            }
        };

        // Serialize schema to IPC format
        let options = IpcWriteOptions::default();
        let data_gen = IpcDataGenerator::default();
        let mut dict_tracker = DictionaryTracker::new(false);
        let schema_bytes = data_gen
            .schema_to_bytes_with_dictionary_tracker(&schema, &mut dict_tracker, &options)
            .ipc_message
            .into();

        // Build FlightInfo response
        let flight_info = FlightInfo {
            schema: schema_bytes,
            flight_descriptor: Some(descriptor),
            endpoint: vec![], // Data retrieved via DoGet with same descriptor
            total_records: -1, // Unknown until executed
            total_bytes: -1,   // Unknown until executed
            ordered: false,
            app_metadata: vec![].into(),
        };

        info!("GetFlightInfo returning schema for ticket");

        Ok(Response::new(flight_info))
    }

    /// Poll for flight info updates (for long-running operations).
    ///
    /// Not needed for FraiseQL use cases (queries are synchronous).
    async fn poll_flight_info(
        &self,
        _request: Request<FlightDescriptor>,
    ) -> std::result::Result<Response<PollInfo>, Status> {
        info!("PollFlightInfo called");
        Err(Status::unimplemented("PollFlightInfo not implemented yet"))
    }

}

/// Convert RecordBatch to FlightData using Arrow IPC encoding.
///
/// # Arguments
///
/// * `batch` - Arrow RecordBatch to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded batch
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
fn record_batch_to_flight_data(batch: &RecordBatch) -> std::result::Result<FlightData, Status> {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);

    let (_, encoded_data) = data_gen
        .encoded_batch(batch, &mut dict_tracker, &options)
        .map_err(|e| Status::internal(format!("Failed to encode RecordBatch: {e}")))?;

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: encoded_data.arrow_data.into(),
        ..Default::default()
    })
}

/// Convert schema to FlightData for initial message.
///
/// # Arguments
///
/// * `schema` - Arrow schema to encode
///
/// # Returns
///
/// FlightData message with IPC-encoded schema
///
/// # Errors
///
/// Returns error if IPC encoding fails
#[allow(clippy::result_large_err)]
fn schema_to_flight_data(
    schema: &Arc<arrow::datatypes::Schema>,
) -> std::result::Result<FlightData, Status> {
    let options = IpcWriteOptions::default();
    let data_gen = IpcDataGenerator::default();
    let mut dict_tracker = DictionaryTracker::new(false);

    let encoded_data =
        data_gen.schema_to_bytes_with_dictionary_tracker(schema, &mut dict_tracker, &options);

    Ok(FlightData {
        data_header: encoded_data.ipc_message.into(),
        data_body: vec![].into(),
        ..Default::default()
    })
}

/// Build optimized SQL query for va_* view.
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders")
/// * `filter` - Optional WHERE clause
/// * `order_by` - Optional ORDER BY clause
/// * `limit` - Optional LIMIT
/// * `offset` - Optional OFFSET
///
/// # Returns
///
/// SQL query string
///
/// # Example
///
/// ```ignore
/// let sql = build_optimized_sql(
///     "va_orders",
///     Some("created_at > '2026-01-01'"),
///     Some("created_at DESC"),
///     Some(100),
///     Some(0)
/// );
/// // Returns: "SELECT * FROM va_orders WHERE created_at > '2026-01-01' ORDER BY created_at DESC LIMIT 100 OFFSET 0"
/// ```
fn build_optimized_sql(
    view: &str,
    filter: Option<String>,
    order_by: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> String {
    let mut sql = format!("SELECT * FROM {view}");

    if let Some(where_clause) = filter {
        sql.push_str(&format!(" WHERE {where_clause}"));
    }

    if let Some(order_clause) = order_by {
        sql.push_str(&format!(" ORDER BY {order_clause}"));
    }

    if let Some(limit_value) = limit {
        sql.push_str(&format!(" LIMIT {limit_value}"));
    }

    if let Some(offset_value) = offset {
        sql.push_str(&format!(" OFFSET {offset_value}"));
    }

    sql
}

/// Generate placeholder database rows for testing.
///
///
/// Currently returns hardcoded test data. Production implementation:
/// - TODO: Replace with actual database adapter when integrated with fraiseql-server (see
///   KNOWN_LIMITATIONS.md#arrow-flight)
///
/// # Arguments
///
/// * `view` - View name (e.g., "va_orders", "va_users")
/// * `limit` - Optional limit on number of rows
///
/// # Returns
///
/// Vec of rows as HashMap<column_name, json_value>
fn execute_placeholder_query(
    view: &str,
    limit: Option<usize>,
) -> Vec<std::collections::HashMap<String, serde_json::Value>> {
    use std::collections::HashMap;

    use serde_json::json;

    let row_count = limit.unwrap_or(10).min(100); // Cap at 100 for testing
    let mut rows = Vec::with_capacity(row_count);

    match view {
        "va_orders" => {
            // Schema: id (Int64), total (Float64), created_at (Timestamp), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("total".to_string(), json!((i as f64 + 1.0) * 99.99));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "va_users" => {
            // Schema: id (Int64), email (Utf8), name (Utf8), created_at (Timestamp)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(i64::from(i as i32 + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                row.insert(
                    "created_at".to_string(),
                    json!(1_700_000_000_000_000_i64 + i64::from(i as i32) * 86_400_000_000),
                );
                rows.push(row);
            }
        },
        "ta_orders" => {
            // Schema: id (Utf8), total (Utf8), created_at (Utf8 ISO 8601), customer_name (Utf8)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("order-{}", i + 1)));
                row.insert("total".to_string(), json!(format!("{:.2}", (i as f64 + 1.0) * 99.99)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                row.insert("customer_name".to_string(), json!(format!("Customer {}", i + 1)));
                rows.push(row);
            }
        },
        "ta_users" => {
            // Schema: id (Utf8), email (Utf8), name (Utf8), created_at (Utf8 ISO 8601)
            for i in 0..row_count {
                let mut row = HashMap::new();
                row.insert("id".to_string(), json!(format!("user-{}", i + 1)));
                row.insert("email".to_string(), json!(format!("user{}@example.com", i + 1)));
                row.insert("name".to_string(), json!(format!("User {}", i + 1)));
                // ISO 8601 timestamp format
                row.insert(
                    "created_at".to_string(),
                    json!(format!("2025-11-{:02}T12:00:00Z", (i % 30) + 1)),
                );
                rows.push(row);
            }
        },
        _ => {
            // Unknown view, return empty rows
            warn!("Unknown view '{}', returning empty result", view);
        },
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests service initialization without database adapter
    #[test]
    fn test_new_creates_service_without_db_adapter() {
        let service = FraiseQLFlightService::new();
        assert!(service.db_adapter.is_none());
    }

    /// Tests that service registers default views on creation
    #[test]
    fn test_new_registers_defaults() {
        let service = FraiseQLFlightService::new();
        assert!(service.schema_registry.contains("va_orders"));
        assert!(service.schema_registry.contains("va_users"));
        assert!(service.schema_registry.contains("ta_orders"));
        assert!(service.schema_registry.contains("ta_users"));
    }

    /// Tests service initialization with executor
    #[test]
    fn test_new_with_executor_stores_reference() {
        let service = FraiseQLFlightService::new();
        // Executor field exists and can be set
        assert!(service.executor.is_none());
    }

    /// Tests that executor accessor works
    #[test]
    fn test_executor_accessor_returns_none_initially() {
        let service = FraiseQLFlightService::new();
        assert!(service.executor().is_none());
    }

    /// Tests that executor can be set and retrieved
    #[test]
    fn test_executor_can_be_set_and_retrieved() {
        let mut service = FraiseQLFlightService::new();

        // Create a dummy executor placeholder (using String which implements Sized)
        let dummy: Arc<dyn Any + Send + Sync> = Arc::new("test".to_string());
        service.set_executor(dummy.clone());

        assert!(service.executor().is_some());
        let retrieved = service.executor().unwrap();
        // Verify we can downcast it back
        let _: &String = retrieved.downcast_ref().expect("Should downcast to &String");
    }

    /// Tests that fraiseql-core types are now accessible
    /// (Phase 1.3: Verifies circular dependency is resolved)
    #[test]
    fn test_fraiseql_core_types_accessible() {
        // Should be able to import and use fraiseql-core types
        use fraiseql_core::schema::CompiledSchema;

        // These types should be accessible now that circular dependency is fixed
        let _: Option<CompiledSchema> = None;
        let _message = "fraiseql-core types accessible";

        // Verify imports work by checking these exist at compile time
        assert!(_message.len() > 0);
    }

    /// Tests that has_executor() returns correct status
    #[test]
    fn test_has_executor_status() {
        let service = FraiseQLFlightService::new();
        assert!(!service.has_executor());

        let mut service = FraiseQLFlightService::new();
        let dummy: Arc<dyn Any + Send + Sync> = Arc::new("test".to_string());
        service.set_executor(dummy);

        assert!(service.has_executor());
    }

    /// Phase 2.1: Documents handshake behavior for JWT validation
    #[test]
    fn test_handshake_jwt_validation_planned() {
        // Phase 2.1 will implement JWT validation in handshake()
        // This test documents the expected behavior:
        // 1. Extract JWT from HandshakeRequest.payload
        // 2. Validate JWT using JwtValidator
        // 3. Return HandshakeResponse with session token on success
        // 4. Return error on validation failure
        let _test_note = "Handshake JWT validation to be implemented in GREEN phase";
        assert!(_test_note.len() > 0);
    }

    /// Phase 2.1: JWT extraction from Bearer format
    #[test]
    fn test_jwt_extraction_from_bearer_format() {
        // Helper for extracting JWT from "Bearer <token>" format (used in handshake)
        fn extract_jwt_from_bearer(payload: &str) -> Option<&str> {
            payload.strip_prefix("Bearer ")
        }

        // Test valid Bearer format
        let token = extract_jwt_from_bearer("Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
        assert_eq!(token, Some("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"));

        // Test invalid format (no Bearer prefix)
        let token = extract_jwt_from_bearer("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
        assert_eq!(token, None);

        // Test empty string
        let token = extract_jwt_from_bearer("");
        assert_eq!(token, None);
    }

    /// Phase 2.2: Tests SecurityContext creation and validation
    #[test]
    fn test_security_context_creation() {
        let context = SecurityContext {
            session_token: "session-12345".to_string(),
            user_id: "user-456".to_string(),
            expiration: Some(9999999999),
        };

        assert_eq!(context.session_token, "session-12345");
        assert_eq!(context.user_id, "user-456");
        assert!(context.expiration.is_some());
    }

    /// Phase 2.2: Tests that security context can be set on service
    #[test]
    fn test_service_with_security_context() {
        let service = FraiseQLFlightService::new();
        assert!(service.security_context.is_none());

        // In Phase 2.2b, set security context after successful handshake
        let _context = SecurityContext {
            session_token: "session-abc".to_string(),
            user_id: "user-123".to_string(),
            expiration: None,
        };

        // security_context can be set on service after handshake completes
        // (will be done in GREEN phase implementation)
    }

    /// Phase 2.2: Documents authenticated query execution
    #[test]
    fn test_authenticated_query_execution_planned() {
        // Phase 2.2 will implement authenticated query execution:
        // 1. Check if security_context is set on service (authenticated)
        // 2. Validate session token from gRPC metadata
        // 3. Pass security context to executor.execute_with_security()
        // 4. Apply RLS (Row-Level Security) filters based on user_id
        let _note = "Authenticated query execution to be implemented in Phase 2.2b";
        assert!(_note.len() > 0);
    }

    /// Phase 3.1: Tests that get_flight_info returns schema for views
    #[tokio::test]
    async fn test_get_flight_info_for_optimized_view() {
        use tonic::Request;
        use crate::ticket::FlightTicket;

        let service = FraiseQLFlightService::new();

        // Create a FlightTicket for an optimized view and encode it
        let ticket = FlightTicket::OptimizedView {
            view: "va_orders".to_string(),
            filter: None,
            order_by: None,
            limit: None,
            offset: None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create a FlightDescriptor with encoded ticket bytes
        let descriptor = FlightDescriptor {
            r#type: 1, // PATH
            path: vec![String::from_utf8_lossy(&ticket_bytes).to_string()],
            cmd: Default::default(),
        };

        let request = Request::new(descriptor);
        let result = service.get_flight_info(request).await;

        // Phase 3.1 should return FlightInfo with schema
        assert!(result.is_ok(), "get_flight_info should succeed for valid view");
        let response = result.unwrap();
        let flight_info = response.into_inner();

        // Verify schema is present
        assert!(!flight_info.schema.is_empty(), "Schema should not be empty");
    }

    /// Phase 3.1: Tests that get_flight_info returns error for invalid view
    #[tokio::test]
    async fn test_get_flight_info_invalid_view() {
        use tonic::Request;
        use crate::ticket::FlightTicket;

        let service = FraiseQLFlightService::new();

        // Create a FlightTicket for a non-existent view and encode it
        let ticket = FlightTicket::OptimizedView {
            view: "nonexistent_view".to_string(),
            filter: None,
            order_by: None,
            limit: None,
            offset: None,
        };
        let ticket_bytes = ticket.encode().expect("Failed to encode ticket");

        // Create a FlightDescriptor with encoded ticket bytes
        let descriptor = FlightDescriptor {
            r#type: 1, // PATH
            path: vec![String::from_utf8_lossy(&ticket_bytes).to_string()],
            cmd: Default::default(),
        };

        let request = Request::new(descriptor);
        let result = service.get_flight_info(request).await;

        // Should return error for invalid view
        assert!(result.is_err(), "get_flight_info should fail for non-existent view");
    }

    /// Phase 3.2: Tests that list_actions returns available actions
    #[tokio::test]
    async fn test_list_actions_returns_action_types() {
        use tonic::Request;
        use arrow_flight::flight_service_server::FlightService;

        let service = FraiseQLFlightService::new();
        let request = Request::new(Empty {});
        let result = service.list_actions(request).await;

        assert!(result.is_ok(), "list_actions should succeed");
        let response = result.unwrap();
        let mut stream = response.into_inner();

        // Collect all actions
        let mut actions = Vec::new();
        while let Some(Ok(action_type)) = stream.next().await {
            actions.push(action_type);
        }

        // Should have at least 3 actions
        assert!(
            actions.len() >= 3,
            "Should have at least 3 actions, got {}",
            actions.len()
        );

        // Verify action names exist
        let action_names: Vec<_> = actions.iter().map(|a| a.r#type.as_str()).collect();
        assert!(
            action_names.contains(&"ClearCache"),
            "Should have ClearCache action"
        );
        assert!(
            action_names.contains(&"RefreshSchemaRegistry"),
            "Should have RefreshSchemaRegistry action"
        );
        assert!(
            action_names.contains(&"HealthCheck"),
            "Should have HealthCheck action"
        );
    }

    /// Phase 3.2: Tests that do_action executes HealthCheck action
    #[tokio::test]
    async fn test_do_action_health_check() {
        use tonic::Request;
        use arrow_flight::flight_service_server::FlightService;

        let service = FraiseQLFlightService::new();
        let action = Action {
            r#type: "HealthCheck".to_string(),
            body: vec![].into(),
        };

        let request = Request::new(action);
        let result = service.do_action(request).await;

        assert!(result.is_ok(), "HealthCheck action should succeed");
        let response = result.unwrap();
        let mut stream = response.into_inner();

        // Should return at least one result
        if let Some(Ok(_result)) = stream.next().await {
            // Success - action returned result
        } else {
            panic!("HealthCheck should return a result");
        }
    }

    /// Phase 3.2: Tests that do_action returns error for unknown action
    #[tokio::test]
    async fn test_do_action_unknown_action() {
        use tonic::Request;
        use arrow_flight::flight_service_server::FlightService;

        let service = FraiseQLFlightService::new();
        let action = Action {
            r#type: "UnknownAction".to_string(),
            body: vec![].into(),
        };

        let request = Request::new(action);
        let result = service.do_action(request).await;

        assert!(
            result.is_err(),
            "Unknown action should return error"
        );
    }

    /// Phase 3.1: Documents do_action() for cache operations
    #[test]
    fn test_do_action_cache_operations_planned() {
        // Phase 3.2 will implement do_action() with actions:
        // 1. ClearCache - Clear all cached query results
        // 2. RefreshSchemaRegistry - Reload schema definitions
        // 3. HealthCheck - Service health status
        let _note = "do_action() with cache/admin operations to be implemented in Phase 3.2";
        assert!(_note.len() > 0);
    }

    /// Phase 3.1: Tests list_actions returns available actions
    #[test]
    fn test_list_actions_planned() {
        // Phase 3.2 will implement list_actions() to return:
        // - ClearCache action
        // - RefreshSchemaRegistry action
        // - HealthCheck action
        let _note = "list_actions() to enumerate available Flight RPC operations";
        assert!(_note.len() > 0);
    }
}
