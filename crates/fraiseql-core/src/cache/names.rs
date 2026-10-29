//! The version counters of the names a cached entry reads (#1778).
//!
//! A name is a view, or a type every read of which an invalidation reaches at once. An
//! invalidation of a name takes the next number of the fence's sequence and raises that
//! name's counter to it; an entry remembers the sequence as it was before its read, and the
//! counters of its names, and is served only while none has moved past it. Nothing is
//! evicted by name: a stale entry stays in the store, refused on every hit, until a fill of
//! its key replaces it or the store's capacity policy drops it.
//!
//! Counters are exact, one per name: an entry checks them for its whole lifetime, and a
//! counter shared by two names would refuse it on every write of the other, for good.
//!
//! # Bounded by the schema
//!
//! Only a fill creates a counter, for the names its read was compiled against. An
//! invalidation of a name no fill holds stamps nothing and registers nothing, so names
//! taken from a request (an administrator's invalidation, a change feed) cannot grow the
//! registry. It is emptied with its cache: a schema reload clears the cache it keeps
//! (`on_schema_reload`), so the names of a schema no longer served leave with its entries,
//! and a tenant's cache, its own, is dropped with the tenant.
//!
//! Emptying it retires each counter, raised to `u64::MAX` under its shard's lock as it is
//! removed: an entry holding one, even a fill that took it a moment before, is never served
//! again, and nothing can take it afterwards. A fill after the clear creates a fresh one,
//! started at its floor as below.
//!
//! # Ordering
//!
//! A counter created after an invalidation of its name found none starts at that name's
//! floor: the fence's hashed slot for it, which every invalidation raises **before** looking
//! the counter up. The creation reads the floor under the map's shard lock, so either the
//! invalidation's lookup finds the counter and raises it, or it ran before the creation took
//! the lock, and the floor the creation reads already holds its stamp. Either way the counter
//! ends past it. A floor shared with another name only starts a counter later than it need,
//! which may refuse the one fill that created it, never more.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use dashmap::DashMap;

/// What an invalidation of a name reaches, as a counter of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Name {
    /// A view, swept in full.
    View,
    /// A view swept by a change feed that names collections, or by a write whose report
    /// does (#1603): an entry's own views, not those of its levels tagged by collection
    /// (#1589).
    FeedView,
    /// A view swept by a write that reported the rows it changed: not the reads of one row
    /// by id (#1576).
    WriteView,
    /// Every read of a row of a type by its id (#1576).
    ByIdType,
    /// Every collection of a child type (#1589).
    CollectionType,
}

const KINDS: usize = 5;

/// One counter per name a stored entry reads, by kind.
pub(super) struct NameRegistry {
    counters: [DashMap<Box<str>, Arc<AtomicU64>>; KINDS],
}

impl NameRegistry {
    pub(super) fn new() -> Self {
        Self {
            counters: std::array::from_fn(|_| DashMap::new()),
        }
    }

    /// Raise the counter of `name` to `stamp`, if a fill holds one.
    pub(super) fn stamp(&self, kind: Name, name: &str, stamp: u64) {
        if let Some(counter) = self.map(kind).get(name) {
            counter.fetch_max(stamp, Ordering::SeqCst);
        }
    }

    /// The counter of `name`, created at `floor` if no fill holds one.
    pub(super) fn counter(
        &self,
        kind: Name,
        name: &str,
        floor: impl FnOnce() -> u64,
    ) -> Arc<AtomicU64> {
        if let Some(counter) = self.map(kind).get(name) {
            return Arc::clone(&counter);
        }
        Arc::clone(
            &self
                .map(kind)
                .entry(name.into())
                .or_insert_with(|| Arc::new(AtomicU64::new(floor()))),
        )
    }

    /// Retire and drop every counter: no entry holding one is served again.
    pub(super) fn clear(&self) {
        for counters in &self.counters {
            counters.retain(|_, counter| {
                counter.store(u64::MAX, Ordering::SeqCst);
                false
            });
        }
    }

    /// How many counters there are, every kind together.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.counters.iter().map(DashMap::len).sum()
    }

    const fn map(&self, kind: Name) -> &DashMap<Box<str>, Arc<AtomicU64>> {
        &self.counters[kind as usize]
    }
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
