//! The insert fence of [`QueryResultCache`](super::QueryResultCache) (#1079, #1584, #1605).
//!
//! A cached read is `get` → miss → **await the database** → `put`. An invalidation landing
//! inside that await evicts nothing (the key is not registered yet), and the `put` would then
//! store rows fetched before the write committed. The fence refuses such a fill.
//!
//! # What a stamp must reach
//!
//! The rule every slot below follows: **an invalidation fences exactly the fills its own key
//! collection would have evicted, had they been stored before it.** A fill it would not have
//! evicted cannot be made stale by it: had the fill landed first, the invalidation would
//! have left it in place, so letting it land second changes nothing. Whatever else the write
//! changed is the business of the invalidations that do reach it (its views, its
//! collections, its feed), each fenced by its own stamp. Refusing more is never unsound,
//! only a missed cache write; refusing less is a stale entry.
//!
//! So every invalidation takes the next number of one sequence and stamps it on what its
//! eviction is keyed by:
//!
//! - a sweep of views: a slot per view (a change feed's, or a write's that names its collections, a
//!   slot of its own: those do not reach the views of levels tagged by collection, #1589, #1603; a
//!   write that reported its rows, another: it does not reach reads of one row by id, #1576);
//! - an eviction of the entity `(type, id)` (`invalidate_by_entity`): the slot of that row (#1605).
//!   It evicts the entries whose entity references hold that id, so it fences the fills that hold
//!   it, and no other fill of the type. Under a steady stream of comment writes a per-type stamp
//!   refused every composed read holding *any* comment;
//! - an eviction of collections: a slot per collection and parent, or per child type (#1589);
//! - a read of one row by its id (#1576): its own `(type, id)` slot, its type's, and a full sweep
//!   of its views;
//! - `clear`: every fill at once.
//!
//! A read snapshots the sequence before its round trip; its fill is refused when a slot it
//! would be indexed under carries a later stamp. The fill is checked against the slots of
//! **every** reference it will be indexed under: its views, and each `(type, id)` its rows and
//! its composed levels hold, read from the rows it fetched, which are exactly what the entity
//! index will register.
//!
//! Slots are fixed arrays indexed by a hash of the name, so a name never outlives anything
//! and arbitrary names cost no memory. Two names sharing a slot only fence each other's fills
//! too eagerly: a missed cache write, never a stale entry. Rows, read by id or evicted by
//! entity, have an array of their own, larger: a write stamps every row it rewrote, many
//! more names than views or types.
//!
//! # Ordering
//!
//! Every access is `SeqCst`. An invalidation stamps **before** it collects keys; a fill checks
//! **after** it is in the store, with a read-modify-write on each slot. Either the fill's
//! check reads the stamp, and it withdraws its own entry, or the check precedes the stamp in
//! the slot's modification order, and the insert happens-before the invalidation's key
//! collection, which then evicts it. There is no third order (#1585).

use std::sync::atomic::{AtomicU64, Ordering};

/// Number of slots. A power of two, so the index is a mask.
const SLOTS: usize = 1024;

/// Number of slots of rows, read by id (#1576) or evicted by entity (#1605), apart: a write
/// reports every row it rewrote, a user's posts and comments with them, so under steady
/// writes thousands of rows are stamped a second, and a fill holding another row sharing a
/// slot with one would be refused. 65 536 slots (512 `KiB`) keep that to about one row in
/// sixty at a thousand rows in flight.
const ROW_SLOTS: usize = 65_536;

/// Hash domains, so a view and an entity type of the same name use different slots.
const VIEW: u8 = b'v';
/// A view swept by a change feed that names collections, or by a write whose report does
/// (#1603): it reaches an entry's own views, not the views of its levels tagged by collection
/// (#1589).
const FEED_VIEW: u8 = b'f';
/// One row evicted by entity: `(type, id)` (#1605).
const ENTITY: u8 = b'e';
/// Every collection of a child type (#1589).
const COLLECTION_TYPE: u8 = b'c';
/// One collection of one parent: `(child type, column, parent id)` (#1589).
const COLLECTION_MEMBER: u8 = b'm';
/// A view swept by a write that reported the rows it changed: it reaches an entry's own views
/// and those of its levels, not the reads of one row by id (#1576).
const WRITE_VIEW: u8 = b'w';
/// One row read by its id: `(type, id)` (#1576).
const BY_ID: u8 = b'i';
/// Every row of a type read by its id (#1576).
const BY_ID_TYPE: u8 = b't';

/// What a fill is indexed under, as the fence checks it.
#[derive(Default)]
pub(super) struct Reach<'a> {
    /// The entry's views: reached by any sweep of them.
    pub(super) views:           Vec<&'a str>,
    /// The views of its levels tagged by collection: reached by the sweep of a write that
    /// named no collection, not the feed's (#1589) nor that of a write that did (#1603).
    pub(super) collected_views: Vec<&'a str>,
    /// The rows it holds, `(type, id)`, as its entity references index them: reached by an
    /// eviction of that row (#1605).
    pub(super) entities:        Vec<(&'a str, &'a str)>,
    /// Its collections, as `(child type, column, parent id)`.
    pub(super) collections:     Vec<(&'a str, &'a str, &'a str)>,
    /// The row it reads by id, as `(type, id)`, with its views (#1576): reached by a full
    /// sweep of those views, by that row and by its type, nothing else.
    pub(super) by_id:           Option<((&'a str, &'a str), Vec<&'a str>)>,
}

/// Sequence-stamped slots; see the module documentation.
pub(super) struct InvalidationFence {
    /// Last number handed out. A snapshot is its current value.
    sequence:   AtomicU64,
    /// Stamped by an invalidation that reaches every fill (`clear`).
    everything: AtomicU64,
    /// Stamped by the invalidations reaching the names hashed to each slot.
    slots:      Box<[AtomicU64]>,
    /// Stamped by the evictions of the rows hashed to each slot, read by id (#1576) or evicted
    /// by entity (#1605).
    rows:       Box<[AtomicU64]>,
}

impl InvalidationFence {
    pub(super) fn new() -> Self {
        Self {
            sequence:   AtomicU64::new(0),
            everything: AtomicU64::new(0),
            slots:      (0..SLOTS).map(|_| AtomicU64::new(0)).collect(),
            rows:       (0..ROW_SLOTS).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    /// The snapshot a read takes before its round trip.
    pub(super) fn snapshot(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    /// Stamp the slots of `views`. Call before collecting the keys to evict.
    pub(super) fn stamp_views<'a>(&self, views: impl IntoIterator<Item = &'a str>) {
        let stamp = self.next();
        for view in views {
            self.slot(VIEW, &[view]).fetch_max(stamp, Ordering::SeqCst);
        }
    }

    /// Stamp the slot of the row `id` of `entity_type`, evicted by entity (#1605). Call before
    /// collecting the keys to evict.
    pub(super) fn stamp_entity(&self, entity_type: &str, id: &str) {
        let stamp = self.next();
        self.row_slot(ENTITY, entity_type, id).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp every fill. Call before clearing.
    pub(super) fn stamp_everything(&self) {
        let stamp = self.next();
        self.everything.fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slots of `views` swept by a change feed that names collections, or by a
    /// write whose report does (#1603), which do not reach the views of levels tagged by
    /// collection (#1589).
    pub(super) fn stamp_feed_views<'a>(&self, views: impl IntoIterator<Item = &'a str>) {
        let stamp = self.next();
        for view in views {
            self.slot(FEED_VIEW, &[view]).fetch_max(stamp, Ordering::SeqCst);
        }
    }

    /// Stamp the slots of `views` swept by a write that reported the rows it changed: reads
    /// of one row by id are not reached (#1576).
    pub(super) fn stamp_write_views<'a>(&self, views: impl IntoIterator<Item = &'a str>) {
        let stamp = self.next();
        for view in views {
            self.slot(WRITE_VIEW, &[view]).fetch_max(stamp, Ordering::SeqCst);
        }
    }

    /// Stamp the slot of the row `id` of `type_name`, read by id (#1576).
    pub(super) fn stamp_by_id(&self, type_name: &str, id: &str) {
        let stamp = self.next();
        self.row_slot(BY_ID, type_name, id).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slot of every row of `type_name` read by id (#1576).
    pub(super) fn stamp_by_id_type(&self, type_name: &str) {
        let stamp = self.next();
        self.slot(BY_ID_TYPE, &[type_name]).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slot of every collection of `child_type` (#1589).
    pub(super) fn stamp_collection_type(&self, child_type: &str) {
        let stamp = self.next();
        self.slot(COLLECTION_TYPE, &[child_type]).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slot of the collection `column` of `child_type` under the parent `parent`
    /// (#1589).
    pub(super) fn stamp_collection(&self, child_type: &str, column: &str, parent: &str) {
        let stamp = self.next();
        self.slot(COLLECTION_MEMBER, &[child_type, column, parent])
            .fetch_max(stamp, Ordering::SeqCst);
    }

    /// Whether an invalidation stamped after `snapshot` reaches a fill indexed under
    /// `reach`. A plain read: cheap, and enough to skip work early.
    pub(super) fn moved_since(&self, snapshot: u64, reach: &Reach<'_>) -> bool {
        self.reaches(snapshot, reach, |slot| slot.load(Ordering::SeqCst))
    }

    /// [`moved_since`](Self::moved_since) as the authoritative check, made **after** the
    /// fill is in the store: a read-modify-write per slot, so a check that misses a stamp is
    /// ordered before it (see the module documentation).
    pub(super) fn moved_since_inserted(&self, snapshot: u64, reach: &Reach<'_>) -> bool {
        self.reaches(snapshot, reach, |slot| slot.fetch_add(0, Ordering::SeqCst))
    }

    fn reaches(&self, snapshot: u64, reach: &Reach<'_>, read: impl Fn(&AtomicU64) -> u64) -> bool {
        let moved = |domain: u8, parts: &[&str]| read(self.slot(domain, parts)) > snapshot;
        read(&self.everything) > snapshot
            || reach
                .views
                .iter()
                .any(|v| moved(VIEW, &[v]) || moved(FEED_VIEW, &[v]) || moved(WRITE_VIEW, &[v]))
            || reach
                .collected_views
                .iter()
                .any(|v| moved(VIEW, &[v]) || moved(WRITE_VIEW, &[v]))
            || reach.by_id.as_ref().is_some_and(|((t, id), views)| {
                read(self.row_slot(BY_ID, t, id)) > snapshot
                    || moved(BY_ID_TYPE, &[t])
                    || views.iter().any(|v| moved(VIEW, &[v]))
            })
            || reach
                .entities
                .iter()
                .any(|(t, id)| read(self.row_slot(ENTITY, t, id)) > snapshot)
            || reach.collections.iter().any(|(t, c, p)| {
                moved(COLLECTION_TYPE, &[t]) || moved(COLLECTION_MEMBER, &[t, c, p])
            })
    }

    fn next(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn row_slot(&self, domain: u8, type_name: &str, id: &str) -> &AtomicU64 {
        #[allow(clippy::cast_possible_truncation)] // Reason: masked to ROW_SLOTS - 1 first
        let index = (hash(domain, &[type_name, id]) & (ROW_SLOTS as u64 - 1)) as usize;
        &self.rows[index]
    }

    fn slot(&self, domain: u8, parts: &[&str]) -> &AtomicU64 {
        #[allow(clippy::cast_possible_truncation)] // Reason: masked to SLOTS - 1 first
        let index = (hash(domain, parts) & (SLOTS as u64 - 1)) as usize;
        &self.slots[index]
    }
}

/// FNV-1a of `domain` and `parts`: stable across toolchains and runs, so which names share a
/// slot is too. Parts are separated by a NUL, which no name holds.
fn hash(domain: u8, parts: &[&str]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = parts
        .iter()
        .enumerate()
        .flat_map(|(i, part)| (i > 0).then_some(0u8).into_iter().chain(part.bytes()));
    for byte in std::iter::once(domain).chain(bytes) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}
