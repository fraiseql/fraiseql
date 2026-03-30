//! Server-streaming gRPC response handler for list queries.
//!
//! When a list query RPC is declared as `returns (stream Entity)`, this
//! module streams rows from the database in batches, encoding each row
//! as an individual gRPC frame (5-byte header + protobuf bytes).
//!
//! Memory usage is bounded by `O(batch_size)` rather than `O(total_rows)`.

use std::sync::Arc;

use bytes::Bytes;
use fraiseql_core::{
    db::{
        traits::DatabaseAdapter,
        types::{ColumnSpec, ColumnValue},
        where_clause::WhereClause,
    },
    schema::TypeDefinition,
    security::SecurityContext,
};
use futures::stream;
use http_body::Frame;
use prost::Message as _;
use prost_reflect::MessageDescriptor;
use tracing::debug;

use super::handler;

/// Encode a single protobuf message with gRPC framing (5-byte header).
fn grpc_frame(msg_bytes: &[u8]) -> Bytes {
    let len = msg_bytes.len();
    let mut framed = Vec::with_capacity(5 + len);
    framed.push(0); // no compression
    #[allow(clippy::cast_possible_truncation)]
    // Reason: individual protobuf messages won't exceed u32::MAX
    framed.extend_from_slice(&(len as u32).to_be_bytes());
    framed.extend_from_slice(msg_bytes);
    Bytes::from(framed)
}

/// Internal state for the streaming unfold loop.
struct StreamState<A: DatabaseAdapter> {
    adapter:        Arc<A>,
    view_name:      String,
    columns:        Vec<ColumnSpec>,
    row_descriptor: MessageDescriptor,
    where_sql:      Option<String>,
    order_by:       Option<String>,
    batch_size:     u32,
    offset:         u32,
    done:           bool,
    sent_trailers:  bool,
}

/// Build a gRPC server-streaming response body for a list query.
///
/// Returns a stream of [`Frame<Bytes>`] — each data frame contains one
/// gRPC-framed protobuf message (a single row).  The final frame is an
/// HTTP/2 trailers frame with `grpc-status: 0`.
///
/// # Errors
///
/// Errors that occur mid-stream are surfaced as a trailers frame with
/// the appropriate gRPC status code and message.
#[allow(clippy::too_many_arguments)] // Reason: mirrors execute_grpc_query() signature; grouping into a struct adds indirection without reducing call-site complexity
pub fn build_streaming_body<A: DatabaseAdapter + 'static>(
    adapter: Arc<A>,
    view_name: String,
    columns: Vec<ColumnSpec>,
    row_descriptor: MessageDescriptor,
    type_def: &TypeDefinition,
    request_msg: &prost_reflect::DynamicMessage,
    security_context: Option<&SecurityContext>,
    batch_size: u32,
) -> impl futures::Stream<Item = Result<Frame<Bytes>, std::convert::Infallible>> + Send {
    // Extract filters and build WHERE clause up front.
    let user_where = handler::extract_filters(request_msg, type_def);

    let rls_where = security_context.and_then(|ctx| {
        use fraiseql_core::security::{DefaultRLSPolicy, RLSPolicy as _};
        let policy = DefaultRLSPolicy::new();
        policy
            .evaluate(ctx, type_def.name.as_str())
            .ok()
            .flatten()
            .map(|rls| rls.into_where_clause())
    });

    let combined = match (rls_where, user_where) {
        (Some(rls), Some(user)) => Some(WhereClause::And(vec![rls, user])),
        (Some(rls), None) => Some(rls),
        (None, user) => user,
    };

    let where_sql = combined.and_then(|clause| {
        use fraiseql_core::db::{dialect::PostgresDialect, where_generator::GenericWhereGenerator};
        let gen = GenericWhereGenerator::new(PostgresDialect);
        gen.generate(&clause).ok().map(|(sql, _)| sql)
    });

    let order_by = handler::extract_order_by(request_msg, type_def);

    stream::unfold(
        StreamState {
            adapter,
            view_name,
            columns,
            row_descriptor,
            where_sql,
            order_by,
            batch_size: batch_size.max(1),
            offset: 0,
            done: false,
            sent_trailers: false,
        },
        |mut state| async move {
            if state.sent_trailers {
                return None;
            }

            if state.done {
                // Send final trailers with gRPC status OK.
                state.sent_trailers = true;
                let mut trailers = http::HeaderMap::new();
                trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
                return Some((Ok(Frame::trailers(trailers)), state));
            }

            match fetch_and_encode_batch(&mut state).await {
                Ok(Some(frames)) => Some((Ok(Frame::data(frames)), state)),
                Ok(None) => {
                    // No more rows — send trailers.
                    state.sent_trailers = true;
                    let mut trailers = http::HeaderMap::new();
                    trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
                    Some((Ok(Frame::trailers(trailers)), state))
                },
                Err(e) => {
                    // Error mid-stream — send error trailers.
                    state.sent_trailers = true;
                    let mut trailers = http::HeaderMap::new();
                    trailers.insert(
                        "grpc-status",
                        http::HeaderValue::from_static("13"), // INTERNAL
                    );
                    if let Ok(msg) = http::HeaderValue::from_str(&e) {
                        trailers.insert("grpc-message", msg);
                    }
                    Some((Ok(Frame::trailers(trailers)), state))
                },
            }
        },
    )
}

/// Fetch the next batch of rows and encode them as concatenated gRPC frames.
///
/// Returns:
/// - `Ok(Some(bytes))` — batch encoded successfully
/// - `Ok(None)` — no more rows
/// - `Err(message)` — error description
async fn fetch_and_encode_batch<A: DatabaseAdapter>(
    state: &mut StreamState<A>,
) -> Result<Option<Bytes>, String> {
    let rows: Vec<Vec<ColumnValue>> = state
        .adapter
        .execute_row_query(
            &state.view_name,
            &state.columns,
            state.where_sql.as_deref(),
            state.order_by.as_deref(),
            Some(state.batch_size),
            Some(state.offset),
        )
        .await
        .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        state.done = true;
        return Ok(None);
    }

    debug!(
        view = %state.view_name,
        batch_size = state.batch_size,
        offset = state.offset,
        rows = rows.len(),
        "gRPC streaming batch fetched"
    );

    // Encode each row as a gRPC-framed protobuf message.
    let mut all_frames = Vec::new();
    for row in &rows {
        let row_msg = handler::encode_row(row, &state.columns, &state.row_descriptor);
        let msg_bytes = row_msg.encode_to_vec();
        all_frames.extend_from_slice(&grpc_frame(&msg_bytes));
    }

    // If we got fewer rows than the batch size, this is the last batch.
    #[allow(clippy::cast_possible_truncation)] // Reason: rows.len() won't exceed u32 range
    let row_count = rows.len() as u32;
    if row_count < state.batch_size {
        state.done = true;
    } else {
        state.offset += state.batch_size;
    }

    Ok(Some(Bytes::from(all_frames)))
}
