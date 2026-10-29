//! The cross-server invalidation feed of the result cache (#1577).
//!
//! The result cache lives in one process: a write evicts entries on the server that ran it.
//! With the feed on, that server also **announces** each eviction, after its transaction
//! committed, and every other server **applies** it. This module is the part that does not
//! know how messages travel: what is announced ([`Eviction`]), how it is written to and read
//! from a message ([`encode`], [`decode`]), the queue between the cache and the sender
//! ([`FeedOutbox`], [`OutboundQueue`]), and what applies a received eviction
//! ([`CacheEvictor`]). The server carries messages over PostgreSQL `NOTIFY` on
//! [`CHANNEL`].
//!
//! # After commit, not in the transaction
//!
//! A `pg_notify` in the write transaction would be delivered exactly when the write commits,
//! but PostgreSQL serialises the commits of every transaction that notified on one cluster-wide
//! lock, held across the commit's WAL flush. Measured with 40 concurrent writers over 20 rows
//! and durable commits, it cost the write path 13% of its throughput and 16% of its median
//! latency; announced after commit, a message costs the write path nothing it waits for. The price
//! is that a server that crashes between its `COMMIT` and its announcement never announces: the
//! entry's TTL is then what bounds the other servers' staleness, as it is today for every write.
//!
//! # What is sound when messages are lost
//!
//! An announcement the queue could not hold is not dropped silently: the queue remembers it
//! overflowed, and the sender then announces [`Eviction::Everything`]. A server whose listening
//! connection was lost cannot know what it missed; it [suspends](CacheEvictor::suspend) its
//! cache (clears it, and stores nothing) until it listens again, then
//! [resumes](CacheEvictor::resume) (clears it once more, refusing every fill that straddled
//! the gap).

use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use fraiseql_db::ViewName;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{CascadeInvalidator, QueryResultCache};
use crate::error::Result;

/// The `NOTIFY` channel the feed travels on.
pub const CHANNEL: &str = "fraiseql_cache";

/// The longest payload PostgreSQL accepts: `pg_notify` refuses 8000 bytes or more.
pub const MAX_PAYLOAD_BYTES: usize = 7999;

/// The wire format's version. A message of any other version is applied as
/// [`Eviction::Everything`]: a server cannot evict precisely what it cannot read, and a
/// rolling deploy runs two versions at once.
const VERSION: u8 = 1;

/// One eviction a server made because of a write it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eviction {
    /// Every entry reading one of these views (before the embedding graph expands them:
    /// each server expands with its own).
    Views(Vec<String>),
    /// Every entry indexed under this entity.
    Entity {
        /// The entity's GraphQL type.
        entity_type: String,
        /// Its id.
        entity_id:   String,
    },
    /// Every entry.
    Everything,
}

/// A message on the wire. Every field but `v` and `from` is optional, so an empty message
/// evicts nothing.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Wire {
    v:        u8,
    from:     String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    views:    Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    entities: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    all:      bool,
    /// A server's check that it receives what it sends; evicts nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    hello:    bool,
}

impl Wire {
    fn new(from: &str) -> Self {
        Self {
            v: VERSION,
            from: from.to_string(),
            ..Self::default()
        }
    }

    fn to_payload(&self) -> String {
        serde_json::to_string(self).expect("a feed message serialises")
    }
}

/// Write `evictions`, announced by server `from`, as the fewest payloads that each fit in a
/// `NOTIFY`.
///
/// Views and entities are de-duplicated. One [`Eviction::Everything`] makes the whole batch a
/// single message evicting everything, and so does a single name too long to fit a message
/// on its own: evicting more than needed is a re-read, evicting less a stale answer.
#[must_use]
pub fn encode(from: &str, evictions: impl IntoIterator<Item = Eviction>) -> Vec<String> {
    let mut views = BTreeSet::new();
    let mut entities = BTreeSet::new();
    for eviction in evictions {
        match eviction {
            Eviction::Views(v) => views.extend(v),
            Eviction::Entity {
                entity_type,
                entity_id,
            } => {
                entities.insert((entity_type, entity_id));
            },
            Eviction::Everything => return vec![everything(from)],
        }
    }

    let mut packer = Packer::new(from);
    for view in views {
        let cost = json_len(&view) + 1;
        if !packer.make_room(cost) {
            return vec![everything(from)];
        }
        packer.message.views.push(view);
    }
    for (entity_type, entity_id) in entities {
        let cost = json_len(&entity_type) + json_len(&entity_id) + 4;
        if !packer.make_room(cost) {
            return vec![everything(from)];
        }
        packer.message.entities.push((entity_type, entity_id));
    }
    packer.finish()
}

/// Fills messages up to [`MAX_PAYLOAD_BYTES`], starting a new one when the next item would
/// not fit.
struct Packer<'a> {
    from:     &'a str,
    payloads: Vec<String>,
    message:  Wire,
    /// The current message's length, over-estimated: each item counts its own JSON plus a
    /// separator, and the first of each array needs none.
    size:     usize,
    /// The length of a message with one empty view and one empty entity.
    empty:    usize,
}

impl<'a> Packer<'a> {
    fn new(from: &'a str) -> Self {
        let mut probe = Wire::new(from);
        probe.views.push(String::new());
        probe.entities.push((String::new(), String::new()));
        let empty = probe.to_payload().len();
        Self {
            from,
            payloads: Vec::new(),
            message: Wire::new(from),
            size: empty,
            empty,
        }
    }

    /// Make room for an item of `cost` bytes, sending the current message when it is full;
    /// `false` when no message could hold the item.
    fn make_room(&mut self, cost: usize) -> bool {
        if self.empty + cost > MAX_PAYLOAD_BYTES {
            return false;
        }
        if self.size + cost > MAX_PAYLOAD_BYTES {
            let full = std::mem::replace(&mut self.message, Wire::new(self.from));
            self.payloads.push(full.to_payload());
            self.size = self.empty;
        }
        self.size += cost;
        true
    }

    fn finish(mut self) -> Vec<String> {
        if !self.message.views.is_empty() || !self.message.entities.is_empty() {
            self.payloads.push(self.message.to_payload());
        }
        self.payloads
    }
}

/// The payload of server `from`'s check that it receives what it sends.
#[must_use]
pub fn encode_hello(from: &str) -> String {
    let mut wire = Wire::new(from);
    wire.hello = true;
    wire.to_payload()
}

fn everything(from: &str) -> String {
    let mut wire = Wire::new(from);
    wire.all = true;
    wire.to_payload()
}

fn json_len(s: &str) -> usize {
    serde_json::to_string(s).map_or(usize::MAX / 4, |j| j.len())
}

/// A message read off the feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Received {
    /// Evictions server `from` announced.
    Evictions {
        /// The announcing server.
        from:      String,
        /// What it evicted.
        evictions: Vec<Eviction>,
    },
    /// Server `from`'s check that it receives what it sends.
    Hello {
        /// The checking server.
        from: String,
    },
}

/// Read a payload received on [`CHANNEL`].
///
/// A payload that is not a message of this version evicts everything, from no server in
/// particular: a rolling deploy runs two versions at once.
#[must_use]
pub fn decode(payload: &str) -> Received {
    let unreadable = || Received::Evictions {
        from:      String::new(),
        evictions: vec![Eviction::Everything],
    };
    let Ok(wire) = serde_json::from_str::<Wire>(payload) else {
        return unreadable();
    };
    if wire.v != VERSION {
        return unreadable();
    }
    if wire.hello {
        return Received::Hello { from: wire.from };
    }
    if wire.all {
        return Received::Evictions {
            from:      wire.from,
            evictions: vec![Eviction::Everything],
        };
    }
    let mut evictions = Vec::with_capacity(1 + wire.entities.len());
    if !wire.views.is_empty() {
        evictions.push(Eviction::Views(wire.views));
    }
    evictions.extend(wire.entities.into_iter().map(|(entity_type, entity_id)| Eviction::Entity {
        entity_type,
        entity_id,
    }));
    Received::Evictions {
        from: wire.from,
        evictions,
    }
}

/// Where a cache announces its evictions: the sending half of the queue to the feed.
///
/// Never waits: an announcement made while the queue is full is recorded as an overflow,
/// which the sender turns into [`Eviction::Everything`].
#[derive(Debug, Clone)]
pub struct FeedOutbox {
    sender:     mpsc::Sender<Eviction>,
    overflowed: Arc<AtomicBool>,
}

impl FeedOutbox {
    /// Queue `eviction` for every other server.
    pub fn announce(&self, eviction: Eviction) {
        if self.sender.try_send(eviction).is_err() {
            self.overflowed.store(true, Ordering::SeqCst);
        }
    }
}

/// The receiving half of the queue to the feed, read by the sender.
#[derive(Debug)]
pub struct OutboundQueue {
    receiver:   mpsc::Receiver<Eviction>,
    overflowed: Arc<AtomicBool>,
}

impl OutboundQueue {
    /// Most evictions one batch takes off the queue.
    const BATCH: usize = 1024;

    /// Wait for the next announcement, and take it with every one already queued behind it
    /// (up to a bound). `None` once every outbox is gone.
    pub async fn next_batch(&mut self) -> Option<Vec<Eviction>> {
        let mut batch = vec![self.receiver.recv().await?];
        while batch.len() < Self::BATCH {
            match self.receiver.try_recv() {
                Ok(eviction) => batch.push(eviction),
                Err(_) => break,
            }
        }
        Some(batch)
    }

    /// Whether an announcement was lost since the last call (the queue was full).
    #[must_use]
    pub fn take_overflow(&self) -> bool {
        self.overflowed.swap(false, Ordering::SeqCst)
    }

    /// Record that announcements were lost after they left the queue (a send failed).
    pub fn mark_lost(&self) {
        self.overflowed.store(true, Ordering::SeqCst);
    }
}

/// A queue to the feed holding up to `capacity` announcements.
#[must_use]
pub fn outbox(capacity: usize) -> (FeedOutbox, OutboundQueue) {
    let (sender, receiver) = mpsc::channel(capacity.max(1));
    let overflowed = Arc::new(AtomicBool::new(false));
    (
        FeedOutbox {
            sender,
            overflowed: Arc::clone(&overflowed),
        },
        OutboundQueue {
            receiver,
            overflowed,
        },
    )
}

/// Applies evictions another server announced to this server's cache, without announcing them.
#[derive(Clone)]
pub struct CacheEvictor {
    pub(super) cache:   Arc<QueryResultCache>,
    pub(super) cascade: Option<Arc<Mutex<CascadeInvalidator>>>,
}

impl CacheEvictor {
    /// Evict what `eviction` names, as a write through this server would have: views are
    /// expanded through this server's embedding graph.
    ///
    /// # Errors
    ///
    /// When the embedding graph's lock is poisoned.
    pub fn apply(&self, eviction: &Eviction) -> Result<()> {
        match eviction {
            Eviction::Views(views) => {
                let views: Vec<ViewName> =
                    views.iter().map(|v| ViewName::from(v.as_str())).collect();
                super::invalidation_api::invalidate_expanded(
                    &self.cache,
                    self.cascade.as_ref(),
                    &views,
                )
            },
            Eviction::Entity {
                entity_type,
                entity_id,
            } => self.cache.invalidate_by_entity(entity_type, entity_id).map(drop),
            Eviction::Everything => self.cache.clear(),
        }
    }

    /// Clear the cache and store nothing until [`resume`](Self::resume): this server can no
    /// longer hear other servers' writes.
    pub fn suspend(&self) {
        self.cache.suspend();
    }

    /// Store again, from a cleared cache: this server hears other servers' writes again.
    pub fn resume(&self) {
        self.cache.resume();
    }
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod feed_tests;
