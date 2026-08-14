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
    db::{traits::DatabaseAdapter, types::ColumnSpec, where_clause::WhereClause},
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
/// # Errors
///
/// A failure before the first row is returned as an error trailers frame;
/// one after it is surfaced as trailers at the point the stream stops.
#[allow(clippy::too_many_arguments)] // Reason: mirrors execute_grpc_query() signature; grouping into a struct adds indirection without reducing call-site complexity
pub async fn build_streaming_body<A: DatabaseAdapter + 'static>(
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

    let opened = adapter
        .stream_row_query(
            &view_name,
            &columns,
            where_sql.as_deref(),
            order_by.as_deref(),
            None,
            None,
        )
        .await;

    let rows = match opened {
        Ok(rows) => rows,
        Err(e) => {
            // The read never started, so the whole response is one trailers frame.
            return futures::future::Either::Left(stream::once(async move {
                Ok(error_trailers(&e.to_string()))
            }));
        },
    };

    debug!(view = %view_name, batch_size, "gRPC streaming response opened");

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
