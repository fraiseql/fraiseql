//! The two wire framings for an incremental `/graphql` response.
//!
//! `@stream` and `@defer` produce the *same* sequence of JSON payloads; only the
//! framing differs. Keeping the sequence in one place and the framings here is what
//! stops the two transports from drifting into delivering different results — the
//! failure this module exists to prevent, since a client will only ever exercise one
//! of them.
//!
//! - **`text/event-stream`** — GraphQL-over-SSE "distinct connections": one `next` event per
//!   payload, then `complete`. Each event carries the `id:` a client resumes from (see
//!   `sse::resume_offset`).
//! - **`multipart/mixed`** — the Apollo/Relay framing (`deferSpec=20220824`): one MIME part per
//!   payload, terminated by the closing boundary. There is no terminal event; `hasNext: false` on
//!   the last payload is the signal, which is why every payload carries it.
//!
//! The two are negotiated by `Accept` and gated by the same operator flag: an
//! operator enabling incremental delivery is enabling the capability, not one
//! spelling of it.

use axum::{
    http::{HeaderMap, HeaderValue, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use bytes::Bytes;
use futures::{Stream, StreamExt as _, stream};
use serde_json::Value;

/// MIME boundary token for the multipart framing.
///
/// A single `-`, per the `deferSpec=20220824` convention, so the delimiter line
/// reads `---` and the terminator `-----`.
const BOUNDARY: &str = "-";

/// One payload in an incremental delivery.
pub(super) struct Chunk {
    /// The JSON payload: an execution result, or an `incremental` envelope.
    pub payload:   Value,
    /// The offset a client resumes from after this payload, when the delivery is
    /// resumable. `None` for `@defer`, whose payloads are not positions in a row
    /// sequence and so cannot be resumed from — only re-requested.
    pub resume_id: Option<u64>,
}

/// The negotiated framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Wire {
    /// `text/event-stream`.
    Sse,
    /// `multipart/mixed`.
    Multipart,
}

/// Which incremental framing the request asks for, if any.
///
/// SSE wins a tie: it is the framing this transport shipped first, and a client
/// listing both is saying "either".
pub(in super::super) fn negotiate(headers: &HeaderMap) -> Option<Wire> {
    let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok())?;
    let mut multipart = false;
    for part in accept.split(',') {
        // Media-type parameters (`;q=…`, `;deferSpec=…`) are tolerated.
        let Some(media) = part.trim().split(';').next().map(str::trim) else {
            continue;
        };
        if media.eq_ignore_ascii_case("text/event-stream") {
            return Some(Wire::Sse);
        }
        if media.eq_ignore_ascii_case("multipart/mixed") {
            multipart = true;
        }
    }
    multipart.then_some(Wire::Multipart)
}

/// Frame `chunks` for the negotiated wire.
pub(super) fn respond<S>(wire: Wire, chunks: S) -> Response
where
    S: Stream<Item = Chunk> + Send + 'static,
{
    match wire {
        Wire::Sse => sse_response(chunks),
        Wire::Multipart => multipart_response(chunks),
    }
}

/// SSE: a `next` event per payload, a terminal `complete`, and keep-alive comments
/// so an idle delivery survives an intermediary's read timeout.
fn sse_response<S>(chunks: S) -> Response
where
    S: Stream<Item = Chunk> + Send + 'static,
{
    let events = chunks
        .map(|chunk| {
            let event = Event::default().event("next").data(chunk.payload.to_string());
            Ok::<_, std::convert::Infallible>(match chunk.resume_id {
                Some(id) => event.id(id.to_string()),
                None => event,
            })
        })
        .chain(stream::once(async { Ok(Event::default().event("complete").data("")) }));

    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)).text(""))
        .into_response()
}

/// `multipart/mixed`: one part per payload, then the closing boundary.
///
/// `content-length` is emitted per part because the reference implementations do and
/// some clients read it; it is the byte length of the JSON, not of the part.
fn multipart_response<S>(chunks: S) -> Response
where
    S: Stream<Item = Chunk> + Send + 'static,
{
    let body = chunks
        .map(|chunk| {
            let json = chunk.payload.to_string();
            // Delimiter is `--` + boundary (so `---`); terminator is `--` + boundary +
            // `--` (so `-----`). Getting these the same length is the classic multipart
            // bug: a client then reads the terminator as another part header and hangs
            // waiting for a body.
            let part = format!(
                "\r\n--{BOUNDARY}\r\ncontent-type: application/json; charset=utf-8\r\n\
                 content-length: {}\r\n\r\n{json}\r\n",
                json.len()
            );
            Ok::<_, std::convert::Infallible>(Bytes::from(part))
        })
        .chain(stream::once(async { Ok(Bytes::from(format!("--{BOUNDARY}--\r\n"))) }));

    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("multipart/mixed; boundary=\"-\"; deferSpec=20220824"),
        )],
        axum::body::Body::from_stream(body),
    )
        .into_response()
}
