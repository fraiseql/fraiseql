//! The frames a `/{resource}/stream` connection emits: what the client missed, then
//! what happens next (#1310).
//!
//! # The three phases, and why there are three
//!
//! A resumed connection subscribes to the fan-out **first** and only then reads, which
//! is what makes the hand-over airtight: every event published from that instant on
//! reaches the live receiver, so the read only has to account for what came before it.
//! What came before is in one of two states, and each needs its own read:
//!
//! 1. **Recorded** — the dispatch ledger has placed it in the delivery order. Read back in exactly
//!    that order, which is the order the client saw (see [`fraiseql_observers::listener::replay`]
//!    for why sequence order is not).
//! 2. **In flight** — published to the fan-out but not yet recorded. The poller publishes a batch
//!    and records it afterwards, so this window is real and an event inside it belongs to neither
//!    the ledger (not there yet) nor the live receiver (published before it subscribed). One page
//!    in insertion order covers it: the rows at risk are the oldest undispatched ones, so they sort
//!    first.
//! 3. **Live** — the broadcast receiver, from the instant it subscribed.
//!
//! Phases 2 and 3 both over-send by design — phase 2 also carries rows the poller
//! simply has not reached yet, which phase 3 will carry again. The replayed sequences
//! are remembered and suppressed on the live side, so the client does not see the
//! overlap; erring towards sending twice rather than not at all is the whole posture of
//! this endpoint.
//!
//! A read that fails mid-replay ends the stream with an error frame rather than falling
//! through to live: falling through would deliver a healthy-looking stream with a hole
//! in it, which is the failure #873.4, #1113 and #1310 were each filed for.

use std::collections::{HashSet, VecDeque};

use axum::response::sse::Event as SseEvent;
use fraiseql_observers::{
    listener::{ChangeLogReplayReader, ReplayScope, ResumePosition},
    transport::TenantScope,
};
use futures::Stream;
use tokio::sync::broadcast::{Receiver, error::RecvError};

use super::sse::{StreamEvent, stream_event_matches, stream_lagged_payload};
use crate::subscriptions::EntityEvent as BridgeEvent;

/// How many rows one catch-up read fetches.
///
/// Not a bound on the replay — the recorded phase pages until it reaches the head —
/// only on how much is held in memory and on how long one query runs. The bound on the
/// replay as a whole is `[rest].sse_max_replay_events`, checked before the first frame.
const REPLAY_PAGE_SIZE: u32 = 500;

/// Everything a resumed connection needs to read back what it missed.
pub struct ResumeState {
    /// Reads the change log in the order this deployment delivered it.
    pub reader: std::sync::Arc<ChangeLogReplayReader>,
    /// The entity type and tenant this stream carries — the live gates, in the shape the
    /// reader filters by.
    pub scope:  ReplayScope,
    /// Where the client's `Last-Event-ID` sits in the delivery order. Fixed: the
    /// in-flight page reads forward from here, not from the advancing cursor.
    pub origin: ResumePosition,
}

/// Which read a connection is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Paging the dispatch ledger in delivery order.
    Recorded,
    /// The single page of rows the ledger has not placed yet.
    InFlight,
    /// The broadcast receiver.
    Live,
}

/// The unfold state: one connection's whole position.
struct Cursor {
    /// `None` ends the stream — the lag arm and the replay-failure arm both take it.
    rx:          Option<Receiver<BridgeEvent>>,
    entity_type: String,
    tenant:      TenantScope,
    resume:      Option<ResumeState>,
    /// Where the recorded phase has got to.
    read_at:     Option<ResumePosition>,
    /// Frames read but not yet yielded — an unfold step produces one frame.
    pending:     VecDeque<SseEvent>,
    /// Sequences already sent by a replay phase, suppressed if the live stream repeats
    /// them. Bounded by the replay bound checked before the stream was built.
    replayed:    HashSet<i64>,
    phase:       Phase,
}

/// The SSE `event:` name and payload for a replay that could not be completed.
///
/// `error`, like [`stream_lagged_payload`]'s frame and for the same reason: a browser
/// `EventSource` fires its `error` handler for a named `error` event, so a client that
/// handles nothing else still sees it.
fn replay_failed_payload(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "code": "REPLAY_FAILED",
        "message": format!(
            "This stream could not finish replaying what you missed ({reason}), so it \
             is ending rather than continuing with events from now on and a gap you \
             could not see. Reconnect to try again."
        ),
    })
}

/// Render one fanned-out or replayed event as its SSE frame.
fn frame_for(event: &BridgeEvent) -> Option<SseEvent> {
    let wire = StreamEvent::from_bridge_event(event);
    let mut frame = SseEvent::default().event(wire.event_type);
    if let Some(id) = wire.id {
        frame = frame.id(id);
    }
    frame.json_data(wire.data).ok()
}

/// The entity events one connection emits: its replay, then the live stream.
///
/// `resume` is `None` for a fresh connection, which is the live stream alone — the
/// pre-#1310 behaviour, unchanged, including the lag arm that ends the stream rather
/// than resuming past a gap the client cannot see.
pub fn resumable_event_stream(
    receiver: Receiver<BridgeEvent>,
    entity_type: String,
    tenant: TenantScope,
    resume: Option<ResumeState>,
) -> impl Stream<Item = SseEvent> {
    let start = Cursor {
        rx: Some(receiver),
        entity_type,
        tenant,
        read_at: resume.as_ref().map(|r| r.origin),
        phase: if resume.is_some() {
            Phase::Recorded
        } else {
            Phase::Live
        },
        resume,
        pending: VecDeque::new(),
        replayed: HashSet::new(),
    };

    futures::stream::unfold(start, |mut state| async move {
        loop {
            if let Some(frame) = state.pending.pop_front() {
                return Some((frame, state));
            }
            // The receiver is taken when a terminal frame has been emitted; with the
            // pending queue now drained, the stream ends.
            state.rx.as_ref()?;

            match state.phase {
                Phase::Recorded => {
                    let (Some(resume), Some(cursor)) = (state.resume.as_ref(), state.read_at)
                    else {
                        state.phase = Phase::Live;
                        continue;
                    };
                    match resume.reader.page(&resume.scope, &cursor, REPLAY_PAGE_SIZE).await {
                        Ok(events) if events.is_empty() => state.phase = Phase::InFlight,
                        Ok(events) => {
                            state.read_at = events.last().map(|e| e.position);
                            queue_replayed(&mut state, events);
                        },
                        Err(error) => return Some(end_with_replay_failure(state, &error)),
                    }
                },
                Phase::InFlight => {
                    let Some(resume) = state.resume.as_ref() else {
                        state.phase = Phase::Live;
                        continue;
                    };
                    // From the ORIGINAL anchor, not the advanced cursor: an unrecorded
                    // row can sit anywhere after the resume point in insertion order,
                    // including behind rows the recorded phase has already passed.
                    let origin = resume.origin;
                    match resume
                        .reader
                        .page_in_flight(&resume.scope, &origin, REPLAY_PAGE_SIZE)
                        .await
                    {
                        Ok(events) => {
                            queue_replayed(&mut state, events);
                            state.phase = Phase::Live;
                        },
                        Err(error) => return Some(end_with_replay_failure(state, &error)),
                    }
                },
                Phase::Live => {
                    let rx = state.rx.as_mut()?;
                    match rx.recv().await {
                        Ok(event) => {
                            if !stream_event_matches(&event, &state.entity_type, &state.tenant) {
                                continue;
                            }
                            // Already sent by a replay phase. Only a replayed sequence is
                            // ever suppressed, so an event the client has not seen cannot
                            // be dropped here.
                            if event
                                .change_spine
                                .as_ref()
                                .and_then(|envelope| envelope.seq)
                                .is_some_and(|seq| state.replayed.contains(&seq))
                            {
                                continue;
                            }
                            let Some(frame) = frame_for(&event) else {
                                continue;
                            };
                            return Some((frame, state));
                        },
                        // The client fell far enough behind that the fan-out overwrote
                        // events it had not read. Say so and end the stream, rather than
                        // resuming quietly from the new position: a silent resume is a gap
                        // the client cannot see, which is the failure mode this whole
                        // endpoint has been corrected for twice (#873.4, #1113). A closed
                        // stream at least makes `EventSource` reconnect visibly — and
                        // since #1310 that reconnect *resumes*, so the gap this frame
                        // announces is closed by the connection that follows it.
                        Err(RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                entity_type = %state.entity_type,
                                skipped,
                                "REST stream client lagged; ending the stream rather than \
                                 resuming with a gap it cannot see"
                            );
                            let frame = SseEvent::default()
                                .event(super::sse::STREAM_LAGGED_EVENT)
                                .json_data(stream_lagged_payload(skipped))
                                .ok()?;
                            state.rx = None;
                            return Some((frame, state));
                        },
                        // Every sender is gone — the bridge stopped. Nothing more will
                        // arrive on this receiver, so end the stream.
                        Err(RecvError::Closed) => return None,
                    }
                },
            }
        }
    })
}

/// Turn a page of replayed events into frames, remembering their sequences so the live
/// phase does not send them again.
fn queue_replayed(state: &mut Cursor, events: Vec<fraiseql_observers::listener::ReplayedEvent>) {
    for replayed in events {
        // The same projection the live path applies to an observer event, so a replayed
        // frame and the live frame for one row are the same bytes. `None` is a row that
        // is not subscriber-visible (#773's Debezium `'r'`), which the live path filters
        // at the same seam.
        let Some(event) = crate::observers::runtime::bridge_event_for(&replayed.event) else {
            continue;
        };
        // The scope gates again, in memory. The reader filtered in SQL; this is the same
        // gate the live path applies, so the two cannot disagree about one event.
        if !stream_event_matches(&event, &state.entity_type, &state.tenant) {
            continue;
        }
        if let Some(seq) = event.change_spine.as_ref().and_then(|envelope| envelope.seq) {
            state.replayed.insert(seq);
        }
        if let Some(frame) = frame_for(&event) {
            state.pending.push_back(frame);
        }
    }
}

/// End the stream with an error frame naming the failed replay.
fn end_with_replay_failure(
    mut state: Cursor,
    error: &fraiseql_observers::error::ObserverError,
) -> (SseEvent, Cursor) {
    tracing::error!(
        entity_type = %state.entity_type,
        %error,
        "REST stream could not complete its replay; ending the stream rather than \
         continuing live with an unseen gap"
    );
    let frame = SseEvent::default()
        .event(super::sse::STREAM_LAGGED_EVENT)
        .json_data(replay_failed_payload(&error.to_string()))
        .unwrap_or_else(|_| SseEvent::default().event(super::sse::STREAM_LAGGED_EVENT).data(""));
    state.rx = None;
    state.pending.clear();
    (frame, state)
}
