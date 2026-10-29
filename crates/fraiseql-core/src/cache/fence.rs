//! The invalidation sequence of [`QueryResultCache`](super::QueryResultCache): what refuses
//! a stale entry (#1079, #1584, #1605, #1778).
//!
//! A cached read is `get` → miss → **await the database** → `put`. Every invalidation takes
//! the next number of one sequence; a read snapshots it before its round trip, and its
//! entry is refused whenever an invalidation reaching it stamped a later number. Two kinds
//! of key, refused two ways:
//!
//! - **Names** — a view (swept in full, by a change feed or a write that names its collections,
//!   #1589, #1603, or by a write that reported its rows, #1576), a type read by id, a collection's
//!   child type, and `clear`. Bounded by the schema, a few per entry. Each has an exact counter
//!   ([`super::names`]), checked on **every hit**: nothing is evicted by name, and a fill
//!   straddling a stamp is stored but never served.
//! - **Rows** — an entity `(type, id)` (#1605), a row read by id (#1576), a collection of one
//!   parent (#1589). Values, one per row an entry holds. Evicted eagerly through the row indexes,
//!   and fenced **at the fill**, below: a hit never pays for them.
//!
//! # What a row stamp must reach
//!
//! **An invalidation fences exactly the fills its own key collection would have evicted, had
//! they been stored before it.** A fill it would not have evicted cannot be made stale by it:
//! had the fill landed first, the invalidation would have left it in place. Refusing more is
//! never unsound, only a missed cache write; refusing less is a stale entry. So an eviction of
//! the entity `(type, id)` stamps that row's slot, and fences the fills holding it and no
//! other fill of the type: under a steady stream of comment writes a per-type stamp refused
//! every composed read holding *any* comment. A fill is checked against the slot of every
//! row reference it will be indexed under, read from the rows it fetched.
//!
//! Row slots are a fixed array indexed by a hash, so arbitrary ids cost no memory. Two rows
//! sharing a slot only fence each other's fills too eagerly, and only for the round trip of a
//! read. Names have slots too, never checked against a fill: a name's counter, created by
//! the first fill holding it, starts at its slot, which holds every stamp the name took
//! before it had one.
//!
//! # Ordering
//!
//! Every access is `SeqCst`. A row invalidation stamps **before** it collects keys; a fill checks
//! **after** it is in the store, with a read-modify-write on each slot. Either the fill's
//! check reads the stamp, and it withdraws its own entry, or the check precedes the stamp in
//! the slot's modification order, and the insert happens-before the invalidation's key
//! collection, which then evicts it. There is no third order (#1585).
//!
//! In the first order the entry is in the store from its insert to its withdrawal. It is
//! stored unconfirmed and confirmed only once its check passed, and a hit serves only a
//! confirmed entry (#1778): a reader landing in that window, after the invalidation has
//! returned, is not served rows older than the write.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use super::names::{Name, NameRegistry};

/// Number of slots. A power of two, so the index is a mask.
const SLOTS: usize = 1024;

/// Number of slots of rows, read by id (#1576), evicted by entity (#1605) or a collection of
/// one parent (#1589), apart: a write
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
    /// Stamped by the invalidations of the names hashed to each slot: where a name's counter
    /// starts, when the first fill holding it creates one (#1778).
    slots:      Box<[AtomicU64]>,
    /// Stamped by the evictions of the rows hashed to each slot, read by id (#1576), evicted
    /// by entity (#1605) or a collection of one parent (#1589).
    rows:       Box<[AtomicU64]>,
    /// The counter of every name a stored entry reads, checked on each hit (#1778).
    names:      NameRegistry,
}

impl InvalidationFence {
    pub(super) fn new() -> Self {
        Self {
            sequence:   AtomicU64::new(0),
            everything: AtomicU64::new(0),
            slots:      (0..SLOTS).map(|_| AtomicU64::new(0)).collect(),
            rows:       (0..ROW_SLOTS).map(|_| AtomicU64::new(0)).collect(),
            names:      NameRegistry::new(),
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
            self.names.stamp(Name::View, view, stamp);
        }
    }

    /// Stamp the slot of the row `id` of `entity_type`, evicted by entity (#1605). Call before
    /// collecting the keys to evict.
    pub(super) fn stamp_entity(&self, entity_type: &str, id: &str) {
        let stamp = self.next();
        self.row_slot(ENTITY, &[entity_type, id]).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp every fill, and retire every name's counter (#1778). Call before clearing.
    pub(super) fn stamp_everything(&self) {
        let stamp = self.next();
        self.everything.fetch_max(stamp, Ordering::SeqCst);
        self.names.clear();
    }

    /// How many name counters there are (#1778).
    #[cfg(test)]
    pub(super) fn name_count(&self) -> usize {
        self.names.len()
    }

    /// Stamp the slots of `views` swept by a change feed that names collections, or by a
    /// write whose report does (#1603), which do not reach the views of levels tagged by
    /// collection (#1589).
    pub(super) fn stamp_feed_views<'a>(&self, views: impl IntoIterator<Item = &'a str>) {
        let stamp = self.next();
        for view in views {
            self.slot(FEED_VIEW, &[view]).fetch_max(stamp, Ordering::SeqCst);
            self.names.stamp(Name::FeedView, view, stamp);
        }
    }

    /// Stamp the slots of `views` swept by a write that reported the rows it changed: reads
    /// of one row by id are not reached (#1576).
    pub(super) fn stamp_write_views<'a>(&self, views: impl IntoIterator<Item = &'a str>) {
        let stamp = self.next();
        for view in views {
            self.slot(WRITE_VIEW, &[view]).fetch_max(stamp, Ordering::SeqCst);
            self.names.stamp(Name::WriteView, view, stamp);
        }
    }

    /// Stamp the slot of the row `id` of `type_name`, read by id (#1576).
    pub(super) fn stamp_by_id(&self, type_name: &str, id: &str) {
        let stamp = self.next();
        self.row_slot(BY_ID, &[type_name, id]).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slot of every row of `type_name` read by id (#1576).
    pub(super) fn stamp_by_id_type(&self, type_name: &str) {
        let stamp = self.next();
        self.slot(BY_ID_TYPE, &[type_name]).fetch_max(stamp, Ordering::SeqCst);
        self.names.stamp(Name::ByIdType, type_name, stamp);
    }

    /// Stamp the slot of every collection of `child_type` (#1589).
    pub(super) fn stamp_collection_type(&self, child_type: &str) {
        let stamp = self.next();
        self.slot(COLLECTION_TYPE, &[child_type]).fetch_max(stamp, Ordering::SeqCst);
        self.names.stamp(Name::CollectionType, child_type, stamp);
    }

    /// Stamp the slot of the collection `column` of `child_type` under the parent `parent`
    /// (#1589).
    pub(super) fn stamp_collection(&self, child_type: &str, column: &str, parent: &str) {
        let stamp = self.next();
        self.row_slot(COLLECTION_MEMBER, &[child_type, column, parent])
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

    /// The counters of the names a fill indexed under `reach` reads, one per name: what
    /// [`current`](Self::current) checks on each of its hits (#1778). The same names
    /// [`moved_since`](Self::moved_since) checks, rows apart.
    pub(super) fn counters(&self, reach: &Reach<'_>) -> Box<[Arc<AtomicU64>]> {
        let views = reach
            .views
            .iter()
            .flat_map(|v| [Name::View, Name::FeedView, Name::WriteView].map(|kind| (kind, *v)));
        let collected_views = reach
            .collected_views
            .iter()
            .flat_map(|v| [Name::View, Name::WriteView].map(|kind| (kind, *v)));
        let by_id = reach.by_id.iter().flat_map(|((t, _), views)| {
            views.iter().map(|v| (Name::View, *v)).chain([(Name::ByIdType, *t)])
        });
        let collections = reach.collections.iter().map(|(t, _, _)| (Name::CollectionType, *t));
        let mut names: Vec<(Name, &str)> =
            views.chain(collected_views).chain(by_id).chain(collections).collect();
        names.sort_unstable();
        names.dedup();
        names
            .into_iter()
            .map(|(kind, name)| {
                self.names
                    .counter(kind, name, || self.slot(domain(kind), &[name]).load(Ordering::SeqCst))
            })
            .collect()
    }

    /// Whether an entry read at `snapshot`, holding `counters`, may still be served: no
    /// invalidation of its names, nor a `clear`, has been stamped since (#1778).
    pub(super) fn current(&self, snapshot: u64, counters: &[Arc<AtomicU64>]) -> bool {
        self.everything.load(Ordering::SeqCst) <= snapshot
            && counters.iter().all(|counter| counter.load(Ordering::SeqCst) <= snapshot)
    }

    fn reaches(&self, snapshot: u64, reach: &Reach<'_>, read: impl Fn(&AtomicU64) -> u64) -> bool {
        let moved = |domain: u8, parts: &[&str]| read(self.row_slot(domain, parts)) > snapshot;
        read(&self.everything) > snapshot
            || reach
                .by_id
                .as_ref()
                .is_some_and(|(row, _)| moved(BY_ID, &<[&str; 2]>::from(*row)))
            || reach.entities.iter().any(|&row| moved(ENTITY, &<[&str; 2]>::from(row)))
            || reach
                .collections
                .iter()
                .any(|&member| moved(COLLECTION_MEMBER, &<[&str; 3]>::from(member)))
    }

    fn next(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn row_slot(&self, domain: u8, parts: &[&str]) -> &AtomicU64 {
        #[allow(clippy::cast_possible_truncation)] // Reason: masked to ROW_SLOTS - 1 first
        let index = (hash(domain, parts) & (ROW_SLOTS as u64 - 1)) as usize;
        &self.rows[index]
    }

    fn slot(&self, domain: u8, parts: &[&str]) -> &AtomicU64 {
        #[allow(clippy::cast_possible_truncation)] // Reason: masked to SLOTS - 1 first
        let index = (hash(domain, parts) & (SLOTS as u64 - 1)) as usize;
        &self.slots[index]
    }
}

/// The hash domain of the slot holding the floor of a name's counter (#1778).
const fn domain(kind: Name) -> u8 {
    match kind {
        Name::View => VIEW,
        Name::FeedView => FEED_VIEW,
        Name::WriteView => WRITE_VIEW,
        Name::ByIdType => BY_ID_TYPE,
        Name::CollectionType => COLLECTION_TYPE,
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
