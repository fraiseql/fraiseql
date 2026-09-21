//! Handler for the Arrow Flight `do_put` RPC method.
//!
//! Authenticates the caller, resolves its database identity, receives a stream of
//! `FlightData` messages containing Arrow `RecordBatch`es, and writes each batch into
//! the allow-listed target table together with its Change Spine outbox rows.
//!
//! # Batch granularity is the transaction (#1355)
//!
//! Each batch is one `execute_gated_upload`, so a batch's rows and the
//! `core.tb_entity_change_log` rows recording them commit together or not at all. The
//! stream as a whole is deliberately **not** one transaction: `DoPut` is the bulk-load
//! verb, so the whole upload is sized by the client, and making it atomic would mean
//! either buffering an unbounded number of client-supplied rows in memory or holding a
//! database transaction — and its locks — open for as long as an untrusted client cares
//! to keep the stream alive. Per-batch instead gives the `PutResult` acknowledgement a
//! precise meaning: the batch it answers is committed and recorded.
//!
//! `DoExchange`'s `Upload` carries a single `RecordBatch`, so #953 never had to answer
//! this; the two verbs converge on the same adapter seam either way.

use std::sync::Arc;

use arrow_flight::{FlightData, PutResult};
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use super::{
    super::{
        FraiseQLFlightService, PutResultStream, build_insert_query, decode_flight_data_to_batch,
        extract_session_token, validate_session_token,
    },
    send_helpers::{send_err, send_ok},
    upload_guard::authorize_upload,
};

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

    // #1349: an unresolved subject must not reach a write. `do_exchange`'s Upload arm
    // resolves; this one did not, so the one verb that had never resolved was also the
    // one whose rows the Change Spine could not attribute — the outbox row's tenant comes
    // from this context and nowhere else.
    let mut security_context = fraiseql_core::security::SecurityContext::from_user(
        &authenticated_user,
        uuid::Uuid::new_v4().to_string(),
    );
    super::resolve_identity(svc, &mut security_context).await?;

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
    let upload_allowed_tables = svc.upload_allowed_tables.clone();
    let tenant_id = security_context.tenant_id.map(|t| t.0);
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

                // #1028: the table is named by the *client* and these rows bypass the
                // mutation pipeline entirely, exactly as in `do_exchange`'s Upload. The
                // #953 allow-list was applied there and not here, so DoPut was a second,
                // ungated door to the same capability. Checked before any batch is read,
                // so a refused upload does no work and leaves no trace but the refusal.
                if let Err(message) =
                    authorize_upload(upload_allowed_tables.as_ref(), &user_id.0, &table_name)
                {
                    send_err(&tx, Status::permission_denied(message)).await;
                    return;
                }

                info!(
                    user_id = %user_id,
                    table = %table_name,
                    "Starting data upload"
                );

                let mut total_rows: u64 = 0;

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

                                    // #1355: the batch's rows and their change-log outbox
                                    // rows commit together or not at all. `execute_raw_query`
                                    // is deliberately no longer on this path — it cannot
                                    // express the transaction, and using it left the Change
                                    // Spine blind to every DoPut, exactly as it had left it
                                    // blind to every DoExchange Upload before #953.
                                    let upload = crate::db::GatedUpload {
                                        table:      &table_name,
                                        insert_sql: &sql,
                                        user_id:    &user_id.0,
                                        tenant_id:  tenant_id.as_deref(),
                                    };
                                    match db_adapter.execute_gated_upload(&upload).await {
                                        Ok(written) => {
                                            total_rows += written;
                                            // The mutation-audit event (#953). `runners/mutation`
                                            // emits this for every path through the mutation
                                            // pipeline; an Upload does not, so it emits its own
                                            // with the same target and shape. Per batch, because
                                            // per batch is what committed.
                                            tracing::info!(
                                                target: "fraiseql::mutation_audit",
                                                mutation_name = "flightUpload",
                                                entity_type = %table_name,
                                                operation = "INSERT",
                                                tenant_id = tenant_id.as_deref().unwrap_or(""),
                                                actor = %user_id,
                                                transport = "flight",
                                                rows = written,
                                                "mutation.executed"
                                            );
                                            // Send success result for this batch
                                            let metadata =
                                                format!("Inserted {} rows", written).into_bytes();
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
                                            let err_msg = format!("Database insert failed: {}", e);
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
                let metadata = format!("Upload complete: {} total rows", total_rows).into_bytes();
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
