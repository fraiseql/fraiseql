//! The insert fence of [`QueryResultCache`](super::QueryResultCache) (#1079, #1584).
//!
//! A cached read is `get` → miss → **await the database** → `put`. An invalidation landing
//! inside that await evicts nothing (the key is not registered yet), and the `put` would then
//! store rows fetched before the write committed. The fence refuses such a fill.
//!
//! It refuses only the fills an invalidation could have evicted. Every invalidation takes the
//! next number of one sequence and stamps it on what it reaches: a slot per view it sweeps, a
//! slot per entity type it evicts by id, a slot per child type whose collections it evicts
//! (#1589), or every fill at once (`clear`). A read snapshots the
//! sequence before its round trip; its fill is refused when a slot it would be indexed under
//! carries a later stamp. A write to `v_post` therefore no longer refuses a read of `v_user`:
//! under steady writes some invalidation is always in flight, and a global fence cached
//! nothing at all (#1584).
//!
//! Slots are a fixed array indexed by a hash of the name, so a name never outlives anything
//! and arbitrary names cost no memory. Two names sharing a slot only fence each other's fills
//! too eagerly: a missed cache write, never a stale entry.
//!
//! # Ordering
//!
//! Every access is `SeqCst`. An invalidation stamps **before** it collects keys; a fill checks
//! **after** it is in the store, with a read-modify-write on each slot. Either the fill's
//! check reads the stamp, and it withdraws its own entry, or the check precedes the stamp in
//! the slot's modification order, and the insert happens-before the invalidation's key
//! collection, which then evicts it. There is no third order.

use std::sync::atomic::{AtomicU64, Ordering};

/// Number of slots. A power of two, so the index is a mask.
const SLOTS: usize = 1024;

/// Hash domains, so a view and an entity type of the same name use different slots.
const VIEW: u8 = b'v';
const ENTITY_TYPE: u8 = b'e';
const COLLECTION_TYPE: u8 = b'c';

/// Sequence-stamped slots; see the module documentation.
pub(super) struct InvalidationFence {
    /// Last number handed out. A snapshot is its current value.
    sequence:   AtomicU64,
    /// Stamped by an invalidation that reaches every fill (`clear`).
    everything: AtomicU64,
    /// Stamped by the invalidations reaching the names hashed to each slot.
    slots:      Box<[AtomicU64]>,
}

impl InvalidationFence {
    pub(super) fn new() -> Self {
        Self {
            sequence:   AtomicU64::new(0),
            everything: AtomicU64::new(0),
            slots:      (0..SLOTS).map(|_| AtomicU64::new(0)).collect(),
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
            self.slot(VIEW, view).fetch_max(stamp, Ordering::SeqCst);
        }
    }

    /// Stamp the slot of `entity_type`. Call before collecting the keys to evict.
    pub(super) fn stamp_entity_type(&self, entity_type: &str) {
        let stamp = self.next();
        self.slot(ENTITY_TYPE, entity_type).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp the slot of the collections of `child_type` (#1589). Call before collecting the
    /// keys to evict.
    pub(super) fn stamp_collection_type(&self, child_type: &str) {
        let stamp = self.next();
        self.slot(COLLECTION_TYPE, child_type).fetch_max(stamp, Ordering::SeqCst);
    }

    /// Stamp every fill. Call before clearing.
    pub(super) fn stamp_everything(&self) {
        let stamp = self.next();
        self.everything.fetch_max(stamp, Ordering::SeqCst);
    }

    /// Whether an invalidation stamped after `snapshot` reaches a fill indexed under `views`,
    /// `entity_types` and collections of `child_types`. A plain read: cheap, and enough to
    /// skip work early.
    pub(super) fn moved_since<'a>(
        &self,
        snapshot: u64,
        views: impl IntoIterator<Item = &'a str>,
        entity_types: impl IntoIterator<Item = &'a str>,
        child_types: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        self.reaches(snapshot, views, entity_types, child_types, |slot| slot.load(Ordering::SeqCst))
    }

    /// [`moved_since`](Self::moved_since) as the authoritative check, made **after** the
    /// fill is in the store: a read-modify-write per slot, so a check that misses a stamp is
    /// ordered before it (see the module documentation).
    pub(super) fn moved_since_inserted<'a>(
        &self,
        snapshot: u64,
        views: impl IntoIterator<Item = &'a str>,
        entity_types: impl IntoIterator<Item = &'a str>,
        child_types: impl IntoIterator<Item = &'a str>,
    ) -> bool {
        self.reaches(snapshot, views, entity_types, child_types, |slot| {
            slot.fetch_add(0, Ordering::SeqCst)
        })
    }

    fn reaches<'a>(
        &self,
        snapshot: u64,
        views: impl IntoIterator<Item = &'a str>,
        entity_types: impl IntoIterator<Item = &'a str>,
        child_types: impl IntoIterator<Item = &'a str>,
        read: impl Fn(&AtomicU64) -> u64,
    ) -> bool {
        read(&self.everything) > snapshot
            || views.into_iter().any(|view| read(self.slot(VIEW, view)) > snapshot)
            || entity_types.into_iter().any(|et| read(self.slot(ENTITY_TYPE, et)) > snapshot)
            || child_types
                .into_iter()
                .any(|child| read(self.slot(COLLECTION_TYPE, child)) > snapshot)
    }

    fn next(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn slot(&self, domain: u8, name: &str) -> &AtomicU64 {
        // FNV-1a: stable across toolchains and runs, so which names share a slot is too.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in std::iter::once(domain).chain(name.bytes()) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        #[allow(clippy::cast_possible_truncation)] // Reason: masked to SLOTS - 1 first
        let index = (hash & (SLOTS as u64 - 1)) as usize;
        &self.slots[index]
    }
}
