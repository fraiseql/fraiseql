//! Server-streaming gRPC response handler for list queries.
//!
//! When a list query RPC is declared as `returns (stream Entity)`, this
//! module streams rows from the database, encoding each row as an individual
//! gRPC frame (5-byte header + protobuf bytes).
//!
//! Memory usage is bounded by the framing group size rather than `O(total_rows)`,
//! and the rows come from **one** statement rather than a walking `OFFSET` (#958),
//! so the delivery is a single snapshot.

use std::sync::Arc;

use bytes::Bytes;
use fraiseql_core::{
    db::{traits::DatabaseAdapter, types::ColumnSpec},
    schema::TypeDefinition,
    security::SecurityContext,
};
use futures::{StreamExt as _, stream};
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

/// Internal state carried through the framing unfold loop.
struct StreamState {
    /// Row groups from the RPC's single statement (#958).
    chunks:         futures::stream::ReadyChunks<fraiseql_core::db::traits::ColumnRowStream>,
    columns:        Vec<ColumnSpec>,
    row_descriptor: MessageDescriptor,
    /// Set once trailers have been emitted, so the next poll ends the body.
    sent_trailers:  bool,
}

/// The whole response as a single error-trailers frame, for a failure before any row.
///
/// One function rather than three inline `stream::once(async move { … })` blocks: each
/// would be a distinct opaque type, so the `Either::Left` arms would not unify. #1348
/// added two of those returns (a failing RLS evaluation and a clause that cannot
/// generate, both of which used to be swallowed into "no filter").
fn error_body(
    message: String,
) -> impl futures::Stream<Item = Result<Frame<Bytes>, std::convert::Infallible>> + Send {
    stream::once(async move { Ok(error_trailers(&message)) })
}

/// Build a gRPC server-streaming response body for a list query.
///
/// Returns a stream of [`Frame<Bytes>`] — each data frame carries one or more
/// gRPC-framed protobuf messages (one per row).  The final frame is an
/// HTTP/2 trailers frame with `grpc-status: 0`.
///
/// # Delivery
///
/// One statement over one portal (#958). The previous implementation re-executed
/// the query per batch with a walking `OFFSET`, which is `O(offset)` per batch and
/// gives each batch its own snapshot — a concurrent write between two batches
/// shifts rows across the boundary, so the RPC silently repeats one row and drops
/// another. A server-streaming RPC is exactly the shape where that is least
/// visible to the client, since the frames look identical either way.
///
/// # Gates
///
/// Resolved by the engine (#1351), through the same entry as the unary arm. This
/// arm used to open the read itself and pass `None` for both `limit` and `offset`,
/// so the compiled page-size ceiling (#421) was not merely unenforced — there was
/// no bound of any kind on the number of frames a client could ask for.
///
/// # Errors
///
/// A failure before the first row is returned as an error trailers frame;
/// one after it is surfaced as trailers at the point the stream stops.
// Reason: mirrors the unary arm's shape; grouping into a struct adds indirection
// without reducing call-site complexity, and the two arms staying parallel is what
// #1348 showed matters most here.
#[allow(clippy::too_many_arguments)]
pub async fn build_streaming_body<A: DatabaseAdapter + 'static>(
    executor: Arc<fraiseql_core::runtime::Executor<A>>,
    query_name: String,
    columns: Vec<ColumnSpec>,
    row_descriptor: MessageDescriptor,
    type_def: &TypeDefinition,
    request_msg: &prost_reflect::DynamicMessage,
    security_context: Option<&SecurityContext>,
    batch_size: u32,
) -> impl futures::Stream<Item = Result<Frame<Bytes>, std::convert::Infallible>> + Send {
    let query_match = match handler::grpc_query_match(
        executor.schema(),
        &query_name,
        &columns,
        true,
        request_msg,
        type_def,
    ) {
        Ok(qm) => qm,
        Err(e) => return futures::future::Either::Left(error_body(e.to_string())),
    };

    debug!(query = %query_name, batch_size, "Opening gRPC streaming read through the engine");

    // The engine narrows the projection when field-level RBAC withholds a field,
    // and the values it streams are positional — so the columns the frames are
    // encoded with must be the ones the read used, never the ones asked for.
    let opened = executor.stream_row_read(&query_match, None, security_context, &columns).await;

    let (columns, rows) = match opened {
        Ok(read) => (read.columns, read.stream),
        Err(e) => {
            // The read never started, so the whole response is one trailers frame.
            return futures::future::Either::Left(error_body(e.to_string()));
        },
    };

    let framed = stream::unfold(
        StreamState {
            chunks: rows.ready_chunks(usize::try_from(batch_size.max(1)).unwrap_or(usize::MAX)),
            columns,
            row_descriptor,
            sent_trailers: false,
        },
        |mut state| async move {
            if state.sent_trailers {
                return None;
            }

            let Some(chunk) = state.chunks.next().await else {
                state.sent_trailers = true;
                return Some((Ok(Frame::trailers(ok_trailers())), state));
            };

            let mut all_frames = Vec::new();
            for row in chunk {
                match row {
                    Ok(row) => {
                        let row_msg =
                            handler::encode_row(&row, &state.columns, &state.row_descriptor);
                        all_frames.extend_from_slice(&grpc_frame(&row_msg.encode_to_vec()));
                    },
                    Err(e) => {
                        // Rows encoded before the failure still go out; the client
                        // learns the delivery was cut short from the trailers.
                        state.sent_trailers = true;
                        return Some((Ok(error_trailers(&e.to_string())), state));
                    },
                }
            }

            Some((Ok(Frame::data(Bytes::from(all_frames))), state))
        },
    );

    futures::future::Either::Right(framed)
}

/// `grpc-status: 0` — the delivery completed.
fn ok_trailers() -> http::HeaderMap {
    let mut trailers = http::HeaderMap::new();
    trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
    trailers
}

/// `grpc-status: 13 (INTERNAL)` carrying `message`.
fn error_trailers(message: &str) -> Frame<Bytes> {
    let mut trailers = http::HeaderMap::new();
    trailers.insert("grpc-status", http::HeaderValue::from_static("13"));
    if let Ok(msg) = http::HeaderValue::from_str(message) {
        trailers.insert("grpc-message", msg);
    }
    Frame::trailers(trailers)
}
