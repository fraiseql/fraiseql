//! `FlightService` trait implementation for `FraiseQLFlightService`.
//!
//! Contains all RPC handler methods: handshake, list_flights, get_flight_info,
//! do_get, do_put, do_action, do_exchange, list_actions, get_schema, poll_flight_info.

use std::sync::Arc;

use arrow::ipc::writer::{DictionaryTracker, IpcDataGenerator, IpcWriteOptions};
use arrow_flight::{
    Action, ActionType, Criteria, Empty, FlightData, FlightDescriptor, FlightInfo,
    HandshakeRequest, HandshakeResponse, PollInfo, PutResult, SchemaResult, Ticket,
    flight_service_server::FlightService,
};
#[allow(unused_imports)]
use futures::StreamExt; // StreamExt required for .next() on Pin<Box<dyn Stream>>
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use super::{
    ActionResultStream, ActionTypeStream, FlightDataStream, FlightInfoStream,
    FraiseQLFlightService, HandshakeStream, PutResultStream, build_insert_query,
    create_session_token, decode_flight_data_to_batch, decode_upload_batch,
    encode_json_to_arrow_batch, extract_session_token, map_security_error_to_status,
    record_batch_to_flight_data, validate_session_token,
};
use crate::{
    exchange_protocol::{ExchangeMessage, RequestType},
    schema::{graphql_result_schema, observer_event_schema},
    ticket::FlightTicket,
};

#[tonic::async_trait]
impl FlightService for FraiseQLFlightService {
    type DoActionStream = ActionResultStream;
    type DoExchangeStream = FlightDataStream;
    type DoGetStream = FlightDataStream;
    type DoPutStream = PutResultStream;
    type HandshakeStream = HandshakeStream;
    type ListActionsStream = ActionTypeStream;
    type ListFlightsStream = FlightInfoStream;

    /// Handshake for JWT authentication.
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

        // Extract and validate JWT from request

        // Get the first handshake request which contains the JWT
        let handshake_request = match request.get_mut().message().await {
            Ok(Some(req)) => req,
            Ok(None) => {
                warn!("Handshake: No request message received");
                return Err(Status::invalid_argument("No handshake request provided"));
            },
            Err(e) => {
                warn!("Handshake: Error reading request: {}", e);
                return Err(Status::internal(format!("Error reading handshake: {}", e)));
            },
        };

        // Extract JWT from payload
        let payload_str = String::from_utf8_lossy(&handshake_request.payload);

        // Extract token from "Bearer <token>" format
        let _token = match payload_str.strip_prefix("Bearer ") {
            Some(t) => t.to_string(),
            None => {
                warn!("Handshake: Missing 'Bearer' prefix in authentication payload");
                return Err(Status::unauthenticated("Invalid authentication format"));
            },
        };

        // CRITICAL: OIDC validator MUST be configured - authentication is mandatory
        let validator = self.oidc_validator.as_ref()
            .ok_or_else(|| {
                error!("OIDC validator not configured - authentication is mandatory. Set FLIGHT_OIDC_* environment variables.");
                Status::internal("Authentication not configured. Contact system administrator.")
            })?;

        // Validate JWT
        let authenticated_user = match validator.validate_token(&_token).await {
            Ok(user) => {
                info!(user_id = %user.user_id, "JWT validation successful");
                user
            },
            Err(e) => {
                warn!(error = %e, "JWT validation failed");
                return Err(map_security_error_to_status(e));
            },
        };

        // Create session token
        let session_token = create_session_token(&authenticated_user)?;
        info!(user_id = %authenticated_user.user_id, "Handshake complete");

        // Create response with session token
        let response = HandshakeResponse {
            protocol_version: 0,
            payload:          session_token.as_bytes().to_vec().into(),
        };

        // Build stream response
        let stream = futures::stream::once(async move { Ok(response) });
        let boxed_stream: Self::HandshakeStream = Box::pin(stream);

        Ok(Response::new(boxed_stream))
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
                    path:   vec![view_name.to_string()],
                    cmd:    b"".to_vec().into(),
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
                let schema_bytes = data_gen
                    .schema_to_bytes_with_dictionary_tracker(&schema, &mut dict_tracker, &options)
                    .ipc_message
                    .into();

                let flight_info = FlightInfo {
                    schema:            schema_bytes,
                    flight_descriptor: Some(descriptor),
                    endpoint:          vec![],
                    total_records:     -1, // Unknown until executed
                    total_bytes:       -1,
                    ordered:           false,
                    app_metadata:      vec![].into(),
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
    /// Requires authenticated session token from handshake.
    /// All queries require valid session tokens and pass security context to executor for RLS.
    async fn do_get(
        &self,
        request: Request<Ticket>,
    ) -> std::result::Result<Response<Self::DoGetStream>, Status> {
        // Validate session token from metadata
        let session_token = extract_session_token(&request)?;
        let authenticated_user = validate_session_token(&session_token)?;

        info!(
            user_id = %authenticated_user.user_id,
            scopes = ?authenticated_user.scopes,
            "Authenticated do_get request"
        );

        // Extract ticket
        let ticket_bytes = request.into_inner().ticket;
        let ticket = FlightTicket::decode(&ticket_bytes)
            .map_err(|e| Status::invalid_argument(format!("Invalid ticket: {e}")))?;

        info!("DoGet called (authenticated): {:?}", ticket);

        // Create security context for RLS filtering
        let security_context = fraiseql_core::security::SecurityContext::from_user(
            authenticated_user,
            uuid::Uuid::new_v4().to_string(),
        );

        match ticket {
            FlightTicket::GraphQLQuery { query, variables } => {
                // Pass security_context to execute_graphql_query for RLS
                let stream =
                    self.execute_graphql_query(&query, variables, &security_context).await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::OptimizedView {
                view,
                filter,
                order_by,
                limit,
                offset,
            } => {
                // Pass security_context to execute_optimized_view for RLS
                let stream = self
                    .execute_optimized_view(
                        &view,
                        filter,
                        order_by,
                        limit,
                        offset,
                        &security_context,
                    )
                    .await?;
                Ok(Response::new(Box::pin(stream)))
            },
            FlightTicket::ObserverEvents {
                entity_type,
                start_date,
                end_date,
                limit,
            } => self.execute_observer_events(&entity_type, start_date, end_date, limit).await,
            FlightTicket::BulkExport {
                table,
                filter,
                limit,
                format,
            } => self.execute_bulk_export(&table, filter, limit, format, &security_context).await,
            FlightTicket::BatchedQueries { queries } => {
                // Pass security_context for batched query execution with RLS
                let stream = self.execute_batched_queries(queries, &security_context).await?;
                Ok(Response::new(Box::pin(stream)))
            },
        }
    }

    /// Upload data stream (for client-to-server data transfer).
    ///
    /// Requires authenticated session token from handshake.
    /// Authenticated data uploads with RLS checks.
    async fn do_put(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoPutStream>, Status> {
        // Validate session token for data uploads
        let session_token = extract_session_token(&request)?;
        let authenticated_user = validate_session_token(&session_token)?;

        info!(
            user_id = %authenticated_user.user_id,
            "Authenticated do_put request"
        );

        // Check if database adapter is available
        let db_adapter = self
            .db_adapter
            .as_ref()
            .ok_or_else(|| Status::internal("Database adapter not configured"))?;

        // Get the incoming stream
        let mut stream = request.into_inner();

        // Create channel for responses
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Clone database adapter for spawned task
        let db_adapter = Arc::clone(db_adapter);
        let user_id = authenticated_user.user_id.clone();

        // Spawn handler task to process incoming data
        tokio::spawn(async move {
            // First message should contain schema and FlightDescriptor
            match stream.message().await {
                Ok(Some(first_msg)) => {
                    // Extract target table name from FlightDescriptor
                    let table_name = match first_msg.flight_descriptor {
                        Some(descriptor) => {
                            if descriptor.path.is_empty() {
                                let _ = tx
                                    .send(Err(Status::invalid_argument(
                                        "FlightDescriptor path cannot be empty",
                                    )))
                                    .await;
                                return;
                            }
                            // descriptor.path contains UTF8 strings
                            descriptor.path[0].clone()
                        },
                        None => {
                            let _ = tx
                                .send(Err(Status::invalid_argument("Missing FlightDescriptor")))
                                .await;
                            return;
                        },
                    };

                    info!(
                        user_id = %user_id,
                        table = %table_name,
                        "Starting data upload"
                    );

                    let mut total_rows = 0;

                    // Process incoming RecordBatch messages
                    while let Ok(Some(flight_data)) = stream.message().await {
                        // Skip empty messages or pure metadata
                        if flight_data.data_body.is_empty() {
                            continue;
                        }

                        // Decode RecordBatch from FlightData
                        match decode_flight_data_to_batch(&flight_data) {
                            Ok(batch) => {
                                let rows_in_batch = batch.num_rows();

                                // Build INSERT query from RecordBatch
                                match build_insert_query(&table_name, &batch) {
                                    Ok(sql) => {
                                        info!(
                                            user_id = %user_id,
                                            table = %table_name,
                                            rows = rows_in_batch,
                                            "Inserting batch"
                                        );

                                        // Execute INSERT via database adapter
                                        match db_adapter.execute_raw_query(&sql).await {
                                            Ok(_) => {
                                                total_rows += rows_in_batch;
                                                // Send success result for this batch
                                                let metadata =
                                                    format!("Inserted {} rows", rows_in_batch)
                                                        .into_bytes();
                                                if let Err(e) = tx
                                                    .send(Ok(PutResult {
                                                        app_metadata: metadata.into(),
                                                    }))
                                                    .await
                                                {
                                                    warn!("Failed to send result: {}", e);
                                                    break;
                                                }
                                            },
                                            Err(e) => {
                                                let err_msg =
                                                    format!("Database insert failed: {}", e);
                                                warn!("{}", err_msg);
                                                let _ =
                                                    tx.send(Err(Status::internal(err_msg))).await;
                                                break;
                                            },
                                        }
                                    },
                                    Err(e) => {
                                        let err_msg =
                                            format!("Failed to build INSERT query: {}", e);
                                        warn!("{}", err_msg);
                                        let _ =
                                            tx.send(Err(Status::invalid_argument(err_msg))).await;
                                        break;
                                    },
                                }
                            },
                            Err(e) => {
                                let err_msg = format!("Failed to decode Arrow batch: {}", e);
                                warn!("{}", err_msg);
                                let _ = tx.send(Err(Status::invalid_argument(err_msg))).await;
                                break;
                            },
                        }
                    }

                    info!(
                        user_id = %user_id,
                        table = %table_name,
                        total_rows = total_rows,
                        "Upload completed"
                    );

                    // Send final success result
                    let metadata =
                        format!("Upload complete: {} total rows", total_rows).into_bytes();
                    let _ = tx
                        .send(Ok(PutResult {
                            app_metadata: metadata.into(),
                        }))
                        .await;
                },
                Ok(None) => {
                    let _ = tx.send(Err(Status::invalid_argument("Empty stream"))).await;
                },
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(format!("Stream error: {}", e)))).await;
                },
            }
        });

        // Return response stream
        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::DoPutStream))
    }

    /// Execute an action (RPC method for operations beyond data transfer).
    ///
    /// Requires authenticated session token from handshake.
    /// Admin operations require appropriate scopes.
    ///
    /// Supported actions:
    /// - `ClearCache`: Clear all cached query results (requires "admin" scope)
    /// - `RefreshSchemaRegistry`: Reload schema definitions (requires "admin" scope)
    /// - `HealthCheck`: Return service health status (public, no auth required beyond session
    ///   token)
    async fn do_action(
        &self,
        request: Request<Action>,
    ) -> std::result::Result<Response<Self::DoActionStream>, Status> {
        // Validate session token for admin operations
        let session_token = extract_session_token(&request)?;
        let authenticated_user = validate_session_token(&session_token)?;

        let action = request.into_inner();
        info!(
            user_id = %authenticated_user.user_id,
            action_type = action.r#type,
            "Authenticated do_action request"
        );

        let stream = match action.r#type.as_str() {
            "ClearCache" => {
                // Admin-only action - verify "admin" scope
                if !authenticated_user.scopes.contains(&"admin".to_string()) {
                    return Err(Status::permission_denied(
                        "Cache invalidation requires 'admin' scope",
                    ));
                }

                self.handle_clear_cache()
            },
            "RefreshSchemaRegistry" => {
                // Admin-only action - verify "admin" scope
                if !authenticated_user.scopes.contains(&"admin".to_string()) {
                    return Err(Status::permission_denied(
                        "Schema registry refresh requires 'admin' scope",
                    ));
                }

                self.handle_refresh_schema_registry()
            },
            "GetSchemaVersions" => {
                // Admin-only action - verify "admin" scope
                if !authenticated_user.scopes.contains(&"admin".to_string()) {
                    return Err(Status::permission_denied(
                        "GetSchemaVersions requires 'admin' scope",
                    ));
                }

                self.handle_get_schema_versions()
            },
            "HealthCheck" => {
                // Public action - no special authorization needed beyond authentication
                self.handle_health_check()
            },
            _ => {
                return Err(Status::invalid_argument(format!("Unknown action: {}", action.r#type)));
            },
        };

        Ok(Response::new(Box::pin(stream)))
    }

    /// List available actions.
    ///
    /// Returns the list of supported Flight actions for admin operations.
    async fn list_actions(
        &self,
        _request: Request<Empty>,
    ) -> std::result::Result<Response<Self::ListActionsStream>, Status> {
        info!("ListActions called");

        let actions = vec![
            Ok(ActionType {
                r#type:      "ClearCache".to_string(),
                description: "Clear all cached query results".to_string(),
            }),
            Ok(ActionType {
                r#type:      "RefreshSchemaRegistry".to_string(),
                description: "Reload schema definitions from database".to_string(),
            }),
            Ok(ActionType {
                r#type:      "GetSchemaVersions".to_string(),
                description: "Get current schema versions and metadata".to_string(),
            }),
            Ok(ActionType {
                r#type:      "HealthCheck".to_string(),
                description: "Return service health status".to_string(),
            }),
        ];

        let stream = futures::stream::iter(actions);
        Ok(Response::new(Box::pin(stream)))
    }

    /// Bidirectional streaming with correlation ID matching.
    ///
    /// Supports request/response operations in a single bidirectional stream:
    /// - Query: Execute GraphQL queries and return Arrow-encoded results
    /// - Upload: Insert data batches into tables
    /// - Subscribe: Stream entity change events (deferred to v2.1)
    ///
    /// Each message includes a correlation_id to match requests to responses.
    async fn do_exchange(
        &self,
        request: Request<Streaming<FlightData>>,
    ) -> std::result::Result<Response<Self::DoExchangeStream>, Status> {
        // Validate session token for bidirectional streams
        let session_token = extract_session_token(&request)?;
        let authenticated_user = validate_session_token(&session_token)?;

        info!(
            user_id = %authenticated_user.user_id,
            "Authenticated do_exchange request"
        );

        // Create security context for RLS
        let security_context = fraiseql_core::security::SecurityContext::from_user(
            authenticated_user.clone(),
            uuid::Uuid::new_v4().to_string(),
        );

        // Get incoming stream
        let mut incoming = request.into_inner();

        // Create channel for responses
        let (tx, rx) = tokio::sync::mpsc::channel(100);

        // Clone shared state for spawned task
        let db_adapter = self.db_adapter.clone();
        let executor = self.executor.clone();
        let subscription_manager = self.subscription_manager.clone();
        let user_id = authenticated_user.user_id.clone();

        // Spawn handler task for bidirectional streaming
        tokio::spawn(async move {
            while let Ok(Some(flight_data)) = incoming.message().await {
                // Decode exchange message from FlightData.app_metadata
                let msg_bytes = flight_data.app_metadata.as_ref();

                match ExchangeMessage::from_json_bytes(msg_bytes) {
                    Ok(ExchangeMessage::Request {
                        correlation_id,
                        request_type,
                    }) => {
                        match request_type {
                            RequestType::Query { query, variables } => {
                                // Execute GraphQL query
                                info!(
                                    user_id = %user_id,
                                    correlation_id = %correlation_id,
                                    "Executing exchange query"
                                );

                                let result = match &executor {
                                    Some(exec) => {
                                        exec.execute_with_security(
                                            &query,
                                            variables.as_ref(),
                                            &security_context,
                                        )
                                        .await
                                    },
                                    None => Err("No executor configured".to_string()),
                                };

                                match result {
                                    Ok(json_result) => {
                                        // Convert JSON result to Arrow batch
                                        info!(
                                            user_id = %user_id,
                                            correlation_id = %correlation_id,
                                            "Converting query result to Arrow"
                                        );

                                        match encode_json_to_arrow_batch(&json_result) {
                                            Ok(batch) => {
                                                // Convert batch to FlightData
                                                match record_batch_to_flight_data(&batch) {
                                                    Ok(flight_batch) => {
                                                        // Send batch data
                                                        if let Err(e) =
                                                            tx.send(Ok(flight_batch)).await
                                                        {
                                                            warn!("Failed to send batch: {}", e);
                                                        }

                                                        // Send completion marker
                                                        let complete_msg =
                                                            ExchangeMessage::Complete {
                                                                correlation_id: correlation_id
                                                                    .clone(),
                                                            };
                                                        if let Ok(complete_bytes) =
                                                            complete_msg.to_json_bytes()
                                                        {
                                                            let complete_data = FlightData {
                                                                app_metadata: complete_bytes.into(),
                                                                ..Default::default()
                                                            };
                                                            let _ =
                                                                tx.send(Ok(complete_data)).await;
                                                        }
                                                    },
                                                    Err(e) => {
                                                        warn!("Failed to encode batch: {}", e);
                                                        let error_response =
                                                            ExchangeMessage::Response {
                                                                correlation_id: correlation_id
                                                                    .clone(),
                                                                result:         Err(format!(
                                                                    "Encoding error: {}",
                                                                    e
                                                                )),
                                                            };
                                                        if let Ok(err_bytes) =
                                                            error_response.to_json_bytes()
                                                        {
                                                            let err_data = FlightData {
                                                                app_metadata: err_bytes.into(),
                                                                ..Default::default()
                                                            };
                                                            let _ = tx.send(Ok(err_data)).await;
                                                        }
                                                    },
                                                }
                                            },
                                            Err(e) => {
                                                warn!("Failed to convert result to Arrow: {}", e);
                                                let error_response = ExchangeMessage::Response {
                                                    correlation_id: correlation_id.clone(),
                                                    result:         Err(format!(
                                                        "Conversion error: {}",
                                                        e
                                                    )),
                                                };
                                                if let Ok(err_bytes) =
                                                    error_response.to_json_bytes()
                                                {
                                                    let err_data = FlightData {
                                                        app_metadata: err_bytes.into(),
                                                        ..Default::default()
                                                    };
                                                    let _ = tx.send(Ok(err_data)).await;
                                                }
                                            },
                                        }
                                    },
                                    Err(e) => {
                                        warn!("Query execution failed: {}", e);
                                        let error_response = ExchangeMessage::Response {
                                            correlation_id: correlation_id.clone(),
                                            result:         Err(format!(
                                                "Query execution failed: {}",
                                                e
                                            )),
                                        };
                                        if let Ok(err_bytes) = error_response.to_json_bytes() {
                                            let err_data = FlightData {
                                                app_metadata: err_bytes.into(),
                                                ..Default::default()
                                            };
                                            let _ = tx.send(Ok(err_data)).await;
                                        }
                                    },
                                }
                            },
                            RequestType::Upload { table, batch } => {
                                // Handle upload request using do_put logic
                                info!(
                                    user_id = %user_id,
                                    correlation_id = %correlation_id,
                                    table = %table,
                                    "Processing exchange upload"
                                );

                                // Check database adapter availability
                                match db_adapter {
                                    Some(ref adapter) => {
                                        // Decode Arrow batch
                                        match decode_upload_batch(&batch) {
                                            Ok(record_batch) => {
                                                // Build INSERT query
                                                match build_insert_query(&table, &record_batch) {
                                                    Ok(sql) => {
                                                        // Execute INSERT
                                                        match adapter.execute_raw_query(&sql).await
                                                        {
                                                            Ok(_) => {
                                                                let rows_inserted =
                                                                    record_batch.num_rows();
                                                                info!(
                                                                    user_id = %user_id,
                                                                    table = %table,
                                                                    rows = rows_inserted,
                                                                    "Upload successful"
                                                                );

                                                                // Send success response
                                                                let success_msg = format!(
                                                                    "Inserted {} rows",
                                                                    rows_inserted
                                                                )
                                                                .into_bytes();
                                                                let response =
                                                                    ExchangeMessage::Response {
                                                                        correlation_id:
                                                                            correlation_id.clone(),
                                                                        result:         Ok(
                                                                            success_msg,
                                                                        ),
                                                                    };
                                                                if let Ok(resp_bytes) =
                                                                    response.to_json_bytes()
                                                                {
                                                                    let resp_data = FlightData {
                                                                        app_metadata: resp_bytes
                                                                            .into(),
                                                                        ..Default::default()
                                                                    };
                                                                    let _ = tx
                                                                        .send(Ok(resp_data))
                                                                        .await;
                                                                }
                                                            },
                                                            Err(e) => {
                                                                warn!("Insert failed: {}", e);
                                                                let error_response =
                                                                    ExchangeMessage::Response {
                                                                        correlation_id:
                                                                            correlation_id.clone(),
                                                                        result:         Err(
                                                                            format!(
                                                                                "Insert failed: {}",
                                                                                e
                                                                            ),
                                                                        ),
                                                                    };
                                                                if let Ok(err_bytes) =
                                                                    error_response.to_json_bytes()
                                                                {
                                                                    let err_data = FlightData {
                                                                        app_metadata: err_bytes
                                                                            .into(),
                                                                        ..Default::default()
                                                                    };
                                                                    let _ =
                                                                        tx.send(Ok(err_data)).await;
                                                                }
                                                            },
                                                        }
                                                    },
                                                    Err(e) => {
                                                        warn!("Failed to build INSERT: {}", e);
                                                        let error_response =
                                                            ExchangeMessage::Response {
                                                                correlation_id: correlation_id
                                                                    .clone(),
                                                                result:         Err(format!(
                                                                    "Failed to build INSERT: {}",
                                                                    e
                                                                )),
                                                            };
                                                        if let Ok(err_bytes) =
                                                            error_response.to_json_bytes()
                                                        {
                                                            let err_data = FlightData {
                                                                app_metadata: err_bytes.into(),
                                                                ..Default::default()
                                                            };
                                                            let _ = tx.send(Ok(err_data)).await;
                                                        }
                                                    },
                                                }
                                            },
                                            Err(e) => {
                                                warn!("Failed to decode batch: {}", e);
                                                let error_response = ExchangeMessage::Response {
                                                    correlation_id: correlation_id.clone(),
                                                    result:         Err(format!(
                                                        "Failed to decode batch: {}",
                                                        e
                                                    )),
                                                };
                                                if let Ok(err_bytes) =
                                                    error_response.to_json_bytes()
                                                {
                                                    let err_data = FlightData {
                                                        app_metadata: err_bytes.into(),
                                                        ..Default::default()
                                                    };
                                                    let _ = tx.send(Ok(err_data)).await;
                                                }
                                            },
                                        }
                                    },
                                    None => {
                                        warn!("Database adapter not configured");
                                        let error_response = ExchangeMessage::Response {
                                            correlation_id: correlation_id.clone(),
                                            result:         Err(
                                                "Database adapter not configured".to_string()
                                            ),
                                        };
                                        if let Ok(err_bytes) = error_response.to_json_bytes() {
                                            let err_data = FlightData {
                                                app_metadata: err_bytes.into(),
                                                ..Default::default()
                                            };
                                            let _ = tx.send(Ok(err_data)).await;
                                        }
                                    },
                                }
                            },
                            RequestType::Subscribe {
                                entity_type,
                                filter,
                            } => {
                                info!(
                                    correlation_id = %correlation_id,
                                    entity_type = %entity_type,
                                    "Starting event subscription"
                                );

                                // Create subscription and get receiver
                                let mut event_rx = subscription_manager.subscribe(
                                    correlation_id.clone(),
                                    entity_type.clone(),
                                    filter.clone(),
                                );

                                // Send subscription acknowledgment
                                let ack_msg = format!("Subscribed to {}", entity_type);
                                let ack_response = ExchangeMessage::Response {
                                    correlation_id: correlation_id.clone(),
                                    result:         Ok(ack_msg.into_bytes()),
                                };

                                if let Ok(ack_bytes) = ack_response.to_json_bytes() {
                                    let ack_data = FlightData {
                                        app_metadata: ack_bytes.into(),
                                        ..Default::default()
                                    };
                                    let _ = tx.send(Ok(ack_data)).await;
                                }

                                // Clone necessary state for event forwarding task
                                let tx_clone = tx.clone();
                                let correlation_id_clone = correlation_id.clone();

                                // Spawn task to forward events from subscription to client
                                tokio::spawn(async move {
                                    while let Some(event) = event_rx.recv().await {
                                        // Convert event to JSON for transmission
                                        match serde_json::to_vec(&event) {
                                            Ok(event_json) => {
                                                let event_data = FlightData {
                                                    data_body: event_json.into(),
                                                    app_metadata: b"observer_event".to_vec().into(),
                                                    ..Default::default()
                                                };

                                                if let Err(e) = tx_clone.send(Ok(event_data)).await
                                                {
                                                    warn!(
                                                        "Failed to send event to subscriber: {}",
                                                        e
                                                    );
                                                    break;
                                                }
                                            },
                                            Err(e) => {
                                                warn!("Failed to serialize event: {}", e);
                                                break;
                                            },
                                        }
                                    }

                                    info!(
                                        correlation_id = %correlation_id_clone,
                                        "Subscription event stream closed"
                                    );
                                });
                            },
                        }
                    },
                    Ok(ExchangeMessage::Complete { correlation_id }) => {
                        info!(
                            user_id = %user_id,
                            correlation_id = %correlation_id,
                            "Client stream complete"
                        );
                        break;
                    },
                    Ok(ExchangeMessage::Response { .. }) => {
                        warn!("Received response from client (unexpected)");
                    },
                    Err(e) => {
                        warn!("Failed to decode exchange message: {}", e);
                        // Send error but continue processing
                    },
                }
            }

            info!(user_id = %user_id, "Do-exchange stream closed");
        });

        // Return response stream
        let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(output_stream) as Self::DoExchangeStream))
    }

    /// Get flight info for a descriptor (metadata about available data).
    ///
    /// This method provides metadata about what data is available without
    /// actually fetching it.
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
            },
            FlightTicket::ObserverEvents { .. } => {
                // Observer events return event schema
                observer_event_schema()
            },
            FlightTicket::OptimizedView { view, .. } => {
                // Optimized views return pre-compiled view schema
                self.schema_registry.get(&view).map_err(|e| {
                    Status::not_found(format!("Schema not found for view {view}: {e}"))
                })?
            },
            FlightTicket::BulkExport { .. } => {
                // Bulk export not implemented yet
                return Err(Status::unimplemented("BulkExport not supported"));
            },
            FlightTicket::BatchedQueries { .. } => {
                // Batched queries have per-query schemas in the data stream
                return Err(Status::unimplemented(
                    "GetFlightInfo for BatchedQueries uses per-query schemas in data stream",
                ));
            },
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
            schema:            schema_bytes,
            flight_descriptor: Some(descriptor),
            endpoint:          vec![], // Data retrieved via DoGet with same descriptor
            total_records:     -1,     // Unknown until executed
            total_bytes:       -1,     // Unknown until executed
            ordered:           false,
            app_metadata:      vec![].into(),
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
