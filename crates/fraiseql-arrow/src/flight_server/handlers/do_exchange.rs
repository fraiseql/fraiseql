//! Handler for the Arrow Flight `do_exchange` RPC method.
//!
//! Implements a bidirectional streaming protocol with correlation-ID matching.
//! Supports three request types dispatched from `ExchangeMessage`:
//! - `Query`     — execute a GraphQL query and return Arrow-encoded results
//! - `Upload`    — insert Arrow `RecordBatch` data into a target table
//! - `Subscribe` — stream real-time entity change events (requires `SubscriptionManager`)

use std::sync::Arc;

use arrow_flight::FlightData;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use super::super::{
    FlightDataStream, FraiseQLFlightService, QueryExecutor, build_insert_query,
    decode_upload_batch, encode_json_to_arrow_batch, extract_session_token,
    record_batch_to_flight_data, validate_session_token,
};
use crate::{
    exchange_protocol::{ExchangeMessage, RequestType},
    subscription::SubscriptionManager,
};

/// Process a `Query` exchange request: run GraphQL and forward Arrow-encoded results.
#[allow(clippy::cognitive_complexity)] // Reason: multi-step protocol handler with sequential error handling branches
async fn handle_query(
    tx: &Sender<Result<FlightData, Status>>,
    executor: &Option<Arc<dyn QueryExecutor>>,
    security_context: &fraiseql_core::security::SecurityContext,
    user_id: &str,
    correlation_id: &str,
    query: String,
    variables: Option<serde_json::Value>,
) {
    info!(user_id, correlation_id, "Executing exchange query");

    let result = match executor {
        Some(exec) => {
            exec.execute_with_security(&query, variables.as_ref(), security_context).await
        },
        None => Err("No executor configured".to_string()),
    };

    match result {
        Ok(json_result) => {
            info!(user_id, correlation_id, "Converting query result to Arrow");
            let batch_result = encode_json_to_arrow_batch(&json_result)
                .map_err(|e| format!("Encoding error: {e}"));
            match batch_result {
                Ok(batch) => {
                    match record_batch_to_flight_data(&batch) {
                        Ok(flight_batch) => {
                            if let Err(e) = tx.send(Ok(flight_batch)).await {
                                warn!("Failed to send batch: {}", e);
                                return;
                            }
                            // Send completion marker
                            if let Ok(bytes) = (ExchangeMessage::Complete {
                                correlation_id: correlation_id.to_string(),
                            })
                            .to_json_bytes()
                            {
                                let _ = tx
                                    .send(Ok(FlightData {
                                        app_metadata: bytes.into(),
                                        ..Default::default()
                                    }))
                                    .await;
                            }
                        },
                        Err(e) => {
                            send_exchange_error(
                                tx,
                                correlation_id,
                                &format!("Conversion error: {e}"),
                            )
                            .await;
                        },
                    }
                },
                Err(e) => send_exchange_error(tx, correlation_id, &e).await,
            }
        },
        Err(e) => {
            warn!("Query execution failed: {}", e);
            send_exchange_error(tx, correlation_id, &format!("Query execution failed: {e}")).await;
        },
    }
}

/// Process an `Upload` exchange request: decode Arrow batch and INSERT into target table.
#[allow(clippy::cognitive_complexity)] // Reason: multi-step upload protocol with sequential validation and error handling
async fn handle_upload(
    tx: &Sender<Result<FlightData, Status>>,
    db_adapter: &Option<Arc<dyn crate::db::ArrowDatabaseAdapter>>,
    user_id: &str,
    correlation_id: &str,
    table: String,
    batch: Vec<u8>,
) {
    info!(user_id, correlation_id, table = %table, "Processing exchange upload");

    let Some(ref adapter) = db_adapter else {
        warn!("Database adapter not configured");
        send_exchange_error(tx, correlation_id, "Database adapter not configured").await;
        return;
    };

    let record_batch = match decode_upload_batch(&batch) {
        Ok(b) => b,
        Err(e) => {
            warn!("Failed to decode batch: {}", e);
            send_exchange_error(tx, correlation_id, &format!("Failed to decode batch: {e}")).await;
            return;
        },
    };

    let sql = match build_insert_query(&table, &record_batch) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to build INSERT: {}", e);
            send_exchange_error(tx, correlation_id, &format!("Failed to build INSERT: {e}")).await;
            return;
        },
    };

    match adapter.execute_raw_query(&sql).await {
        Ok(_) => {
            let rows_inserted = record_batch.num_rows();
            info!(user_id, table = %table, rows = rows_inserted, "Upload successful");
            let success_msg = format!("Inserted {} rows", rows_inserted).into_bytes();
            let response = ExchangeMessage::Response {
                correlation_id: correlation_id.to_string(),
                result: Ok(success_msg),
            };
            if let Ok(bytes) = response.to_json_bytes() {
                let _ = tx
                    .send(Ok(FlightData {
                        app_metadata: bytes.into(),
                        ..Default::default()
                    }))
                    .await;
            }
        },
        Err(e) => {
            warn!("Insert failed: {}", e);
            send_exchange_error(tx, correlation_id, &format!("Insert failed: {e}")).await;
        },
    }
}

/// Process a `Subscribe` exchange request: stream entity change events to the client.
async fn handle_subscribe(
    tx: Sender<Result<FlightData, Status>>,
    subscription_manager: Arc<SubscriptionManager>,
    correlation_id: String,
    entity_type: String,
    filter: Option<String>,
) {
    info!(correlation_id = %correlation_id, entity_type = %entity_type, "Starting event subscription");

    let mut event_rx =
        subscription_manager.subscribe(correlation_id.clone(), entity_type.clone(), filter);

    // Send subscription acknowledgment
    let ack_response = ExchangeMessage::Response {
        correlation_id: correlation_id.clone(),
        result: Ok(format!("Subscribed to {}", entity_type).into_bytes()),
    };
    if let Ok(ack_bytes) = ack_response.to_json_bytes() {
        let _ = tx
            .send(Ok(FlightData {
                app_metadata: ack_bytes.into(),
                ..Default::default()
            }))
            .await;
    }

    // Spawn task to forward events from subscription to client
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match serde_json::to_vec(&event) {
                Ok(event_json) => {
                    let event_data = FlightData {
                        data_body: event_json.into(),
                        app_metadata: b"observer_event".to_vec().into(),
                        ..Default::default()
                    };
                    if let Err(e) = tx.send(Ok(event_data)).await {
                        warn!("Failed to send event to subscriber: {}", e);
                        break;
                    }
                },
                Err(e) => {
                    warn!("Failed to serialize event: {}", e);
                    break;
                },
            }
        }
        info!(correlation_id = %correlation_id, "Subscription event stream closed");
    });
}

/// Send an `ExchangeMessage::Response` with an error payload to the client.
async fn send_exchange_error(
    tx: &Sender<Result<FlightData, Status>>,
    correlation_id: &str,
    message: &str,
) {
    let error_response = ExchangeMessage::Response {
        correlation_id: correlation_id.to_string(),
        result: Err(message.to_string()),
    };
    if let Ok(err_bytes) = error_response.to_json_bytes() {
        let _ = tx
            .send(Ok(FlightData {
                app_metadata: err_bytes.into(),
                ..Default::default()
            }))
            .await;
    }
}

/// `do_exchange` handler: bidirectional streaming with correlation-ID matched request/response.
pub(super) async fn handle(
    svc: &FraiseQLFlightService,
    request: Request<Streaming<FlightData>>,
) -> std::result::Result<Response<FlightDataStream>, Status> {
    // Validate session token for bidirectional streams
    let session_token = extract_session_token(&request)?;
    let secret = svc
        .session_secret
        .as_deref()
        .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
    let authenticated_user = validate_session_token(&session_token, secret)?;

    info!(user_id = %authenticated_user.user_id, "Authenticated do_exchange request");

    // Create security context for RLS
    let security_context = fraiseql_core::security::SecurityContext::from_user(
        &authenticated_user,
        uuid::Uuid::new_v4().to_string(),
    );

    let mut incoming = request.into_inner();
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    let db_adapter = svc.db_adapter.clone();
    let executor = svc.executor.clone();
    let subscription_manager = svc.subscription_manager.clone();
    let user_id = authenticated_user.user_id;

    tokio::spawn(async move {
        while let Ok(Some(flight_data)) = incoming.message().await {
            let msg_bytes = flight_data.app_metadata.as_ref();

            match ExchangeMessage::from_json_bytes(msg_bytes) {
                Ok(ExchangeMessage::Request {
                    correlation_id,
                    request_type,
                }) => match request_type {
                    RequestType::Query { query, variables } => {
                        handle_query(
                            &tx,
                            &executor,
                            &security_context,
                            &user_id,
                            &correlation_id,
                            query,
                            variables,
                        )
                        .await;
                    },
                    RequestType::Upload { table, batch } => {
                        handle_upload(&tx, &db_adapter, &user_id, &correlation_id, table, batch)
                            .await;
                    },
                    RequestType::Subscribe {
                        entity_type,
                        filter,
                    } => {
                        handle_subscribe(
                            tx.clone(),
                            subscription_manager.clone(),
                            correlation_id,
                            entity_type,
                            filter,
                        )
                        .await;
                    },
                },
                Ok(ExchangeMessage::Complete { correlation_id }) => {
                    info!(user_id = %user_id, correlation_id = %correlation_id, "Client stream complete");
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

    let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Response::new(Box::pin(output_stream) as FlightDataStream))
}
