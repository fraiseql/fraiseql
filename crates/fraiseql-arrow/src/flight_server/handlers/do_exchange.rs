//! Handler for the Arrow Flight `do_exchange` RPC method.
//!
//! Implements a bidirectional streaming protocol with correlation-ID matching.
//! Supports three request types dispatched from `ExchangeMessage`:
//! - `Query`     — execute a GraphQL query and return Arrow-encoded results
//! - `Upload`    — insert Arrow `RecordBatch` data into a target table
//! - `Subscribe` — refused; no event source is wired to the subscription manager (#1067)

use std::sync::Arc;

use arrow_flight::FlightData;
use tokio::sync::mpsc::Sender;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn};

use super::{
    super::{
        FlightDataStream, FraiseQLFlightService, QueryExecutor, build_insert_query,
        decode_upload_batch, encode_json_to_arrow_batch, extract_session_token,
        record_batch_to_flight_data, validate_session_token,
    },
    upload_guard::authorize_upload,
};
use crate::exchange_protocol::{ExchangeMessage, RequestType};

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
            let batch_result = encode_json_to_arrow_batch(&json_result.to_string())
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

/// What an `Upload` needs from its `do_exchange` session to be authorized and
/// attributed: the operator's allow-list, and who is writing under which tenant.
///
/// Grouped rather than passed positionally so a new piece of provenance is added in
/// one place and cannot be dropped at the call site.
struct UploadSession<'a> {
    /// Operator-configured tables `Upload` may write; `None` disables Upload.
    allowed_tables: Option<&'a std::collections::HashSet<String>>,
    /// Tenant the write belongs to, from the session's `SecurityContext`.
    tenant_id:      Option<&'a str>,
    /// Authenticated Flight session subject.
    user_id:        &'a str,
    /// Correlates the response with the client's request.
    correlation_id: &'a str,
}

/// Process an `Upload` exchange request: decode Arrow batch and INSERT into target table.
#[allow(clippy::cognitive_complexity)] // Reason: multi-step upload protocol with sequential validation and error handling
async fn handle_upload(
    tx: &Sender<Result<FlightData, Status>>,
    db_adapter: &Option<Arc<dyn crate::db::ArrowDatabaseAdapter>>,
    session: &UploadSession<'_>,
    table: String,
    batch: Vec<u8>,
) {
    let UploadSession {
        allowed_tables,
        tenant_id,
        user_id,
        correlation_id,
    } = *session;
    info!(user_id, correlation_id, table = %table, "Processing exchange upload");

    // #953: the table is named by the *client* and the rows bypass the mutation
    // pipeline entirely, so the allow-list is checked before anything else — before
    // the batch is even decoded. A refused Upload must do no work and leave no trace
    // beyond the refusal.
    if let Err(message) = authorize_upload(allowed_tables, user_id, &table) {
        send_exchange_error(tx, correlation_id, &message).await;
        return;
    }

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

    // #953: the rows and their change-log outbox rows commit together or not at all.
    // `execute_raw_query` is deliberately no longer on this path — it cannot express
    // the transaction, and using it left the Change Spine blind to every Upload.
    let upload = crate::db::GatedUpload {
        table: &table,
        insert_sql: &sql,
        user_id,
        tenant_id,
    };
    match adapter.execute_gated_upload(&upload).await {
        Ok(_) => {
            let rows_inserted = record_batch.num_rows();
            info!(user_id, table = %table, rows = rows_inserted, "Upload successful");
            // The mutation-audit event (#953). `runners/mutation` emits this for every
            // path that goes through the mutation pipeline; an Upload does not, so it
            // emits its own with the same target and shape — otherwise the one write
            // surface that skips the authorizer is also the one absent from the audit.
            tracing::info!(
                target: "fraiseql::mutation_audit",
                mutation_name = "flightUpload",
                entity_type = %table,
                operation = "INSERT",
                tenant_id = tenant_id.unwrap_or(""),
                actor = user_id,
                transport = "flight",
                rows = rows_inserted,
                "mutation.executed"
            );
            let success_msg = format!("Inserted {} rows", rows_inserted).into_bytes();
            let response = ExchangeMessage::Response {
                correlation_id: correlation_id.to_string(),
                result:         Ok(success_msg),
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

/// Process a `Subscribe` exchange request.
///
/// Refuses. Nothing in this workspace — including the server that mounts Flight —
/// ever calls `SubscriptionManager::broadcast_event`, so a subscription could never
/// produce an event (#1067).
///
/// It previously acknowledged `Subscribed to {entity_type}` and spawned a forwarder
/// anyway. Three consequences, all removed by refusing:
///
/// 1. The ack was false in every shipped configuration: the client waited forever for events that
///    no code path can emit, while the module docs promise this surface returns `unimplemented`.
/// 2. Every accepted subscription leaked. The forwarder blocked on `recv()`, which only returns
///    `None` once every sender drops, and the sender was owned by the manager's process-lifetime
///    `DashMap`. Nothing calls `unsubscribe` either, so each call left a task, a channel and a map
///    entry alive for the process.
/// 3. The map key was the client-chosen `correlation_id` on a `DashMap` shared across principals,
///    so one client could evict another's entry.
///
/// Restoring this needs a real event source wired to `broadcast_event`, removal on
/// stream close, and a key that is not client-chosen.
async fn handle_subscribe(
    tx: Sender<Result<FlightData, Status>>,
    correlation_id: String,
    entity_type: String,
) {
    info!(
        correlation_id = %correlation_id,
        entity_type = %entity_type,
        "Refusing Subscribe: no event source is wired to the subscription manager"
    );

    send_exchange_error(
        &tx,
        &correlation_id,
        "Subscribe is not implemented: no event source is wired to the Flight subscription \
         manager, so no event can ever be delivered on this stream.",
    )
    .await;
}

/// Send an `ExchangeMessage::Response` with an error payload to the client.
async fn send_exchange_error(
    tx: &Sender<Result<FlightData, Status>>,
    correlation_id: &str,
    message: &str,
) {
    let error_response = ExchangeMessage::Response {
        correlation_id: correlation_id.to_string(),
        result:         Err(message.to_string()),
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
    let upload_allowed_tables = svc.upload_allowed_tables.clone();
    let executor = svc.executor.clone();
    let user_id = authenticated_user.user_id.0;

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
                        let session = UploadSession {
                            allowed_tables: upload_allowed_tables.as_ref(),
                            tenant_id:      security_context
                                .tenant_id
                                .as_ref()
                                .map(|t| t.0.as_str()),
                            user_id:        &user_id,
                            correlation_id: &correlation_id,
                        };
                        handle_upload(&tx, &db_adapter, &session, table, batch).await;
                    },
                    RequestType::Subscribe {
                        entity_type,
                        filter: _,
                    } => {
                        handle_subscribe(tx.clone(), correlation_id, entity_type).await;
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

#[cfg(test)]
mod tests;
