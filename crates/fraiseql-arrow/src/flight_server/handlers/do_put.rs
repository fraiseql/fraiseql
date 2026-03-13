//! Handler for the Arrow Flight `do_put` RPC method.
//!
//! Authenticates the caller, receives a stream of `FlightData` messages containing
//! Arrow `RecordBatch`es, and inserts each batch into the target table via the
//! configured database adapter.

use std::sync::Arc;

use arrow_flight::{FlightData, PutResult};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use super::super::{
    FraiseQLFlightService, PutResultStream, build_insert_query, decode_flight_data_to_batch,
    extract_session_token, validate_session_token,
};
use super::send_helpers::{send_err, send_ok};

/// `do_put` handler: receive a client data stream and INSERT batches into the target table.
pub(super) async fn handle(
    svc: &FraiseQLFlightService,
    request: Request<Streaming<FlightData>>,
) -> std::result::Result<Response<PutResultStream>, Status> {
    // Validate session token for data uploads
    let session_token = extract_session_token(&request)?;
    let secret = svc
        .session_secret
        .as_deref()
        .ok_or_else(|| Status::internal("FLIGHT_SESSION_SECRET not configured"))?;
    let authenticated_user = validate_session_token(&session_token, secret)?;

    info!(
        user_id = %authenticated_user.user_id,
        "Authenticated do_put request"
    );

    // Check if database adapter is available
    let db_adapter = svc
        .db_adapter
        .as_ref()
        .ok_or_else(|| Status::internal("Database adapter not configured"))?;

    // Get the incoming stream
    let mut stream = request.into_inner();

    // Create channel for responses
    let (tx, rx) = tokio::sync::mpsc::channel(100);

    // Clone database adapter for spawned task
    let db_adapter = Arc::clone(db_adapter);
    let user_id = authenticated_user.user_id;

    // Spawn handler task to process incoming data
    tokio::spawn(async move {
        // First message should contain schema and FlightDescriptor
        match stream.message().await {
            Ok(Some(first_msg)) => {
                // Extract target table name from FlightDescriptor
                let table_name = if let Some(descriptor) = first_msg.flight_descriptor {
                    if descriptor.path.is_empty() {
                        send_err(
                            &tx,
                            Status::invalid_argument("FlightDescriptor path cannot be empty"),
                        )
                        .await;
                        return;
                    }
                    // descriptor.path contains UTF8 strings
                    descriptor.path[0].clone()
                } else {
                    send_err(&tx, Status::invalid_argument("Missing FlightDescriptor")).await;
                    return;
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
                                            let sent = send_ok(
                                                &tx,
                                                PutResult {
                                                    app_metadata: metadata.into(),
                                                },
                                            )
                                            .await;
                                            if !sent {
                                                break;
                                            }
                                        },
                                        Err(e) => {
                                            let err_msg =
                                                format!("Database insert failed: {}", e);
                                            warn!("{}", err_msg);
                                            send_err(&tx, Status::internal(err_msg)).await;
                                            break;
                                        },
                                    }
                                },
                                Err(e) => {
                                    let err_msg = format!("Failed to build INSERT query: {}", e);
                                    warn!("{}", err_msg);
                                    send_err(&tx, Status::invalid_argument(err_msg)).await;
                                    break;
                                },
                            }
                        },
                        Err(e) => {
                            let err_msg = format!("Failed to decode Arrow batch: {}", e);
                            warn!("{}", err_msg);
                            send_err(&tx, Status::invalid_argument(err_msg)).await;
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
                send_ok(
                    &tx,
                    PutResult {
                        app_metadata: metadata.into(),
                    },
                )
                .await;
            },
            Ok(None) => {
                send_err(&tx, Status::invalid_argument("Empty stream")).await;
            },
            Err(e) => {
                send_err(&tx, Status::internal(format!("Stream error: {}", e))).await;
            },
        }
    });

    // Return response stream
    let output_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    Ok(Response::new(Box::pin(output_stream) as PutResultStream))
}
