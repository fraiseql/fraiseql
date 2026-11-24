//! Read-your-writes pins, kept per principal.
//!
//! A write pins reads to the primary for the pin window, so a client reads what it just
//! wrote rather than a replica that has not replayed it. Only the writer needs that: a
//! pin shared by every client sends every read to the primary while anyone writes.
//! So a write made in a principal's scope ([`with_write_pin_principal`]) pins that
//! principal's reads, and the pin is kept per principal.
//!
//! [`with_write_pin_principal`]: crate::with_write_pin_principal
//!
//! Every way a read could miss a write it should see is pinned instead:
//!
//! - a read outside any scope cannot say whose write it would miss: any write pins it;
//! - a write outside any scope may be any principal's: it pins every read;
//! - a write the table has no room for pins every scoped read until its window passes.

use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

pub(super) use crate::write_pin_scope::scoped_write_pin_principal as scoped_principal;

/// The principals whose read-your-writes pins are kept at once.
///
/// Past it, a write by a principal the table does not hold pins every scoped read for
/// its window, as a write outside any scope does. An entry is a principal's hash and its
/// last write, under 64 bytes with the table's overhead, so the table stays under 1 MiB.
pub const PRINCIPAL_PIN_CAPACITY: usize = 10_000;

/// The watermarks that decide whether a read must be served by the primary.
pub(super) struct WritePins {
    /// The last write by anyone: pins a read outside any scope.
    any:          AtomicU64,
    /// The last write outside any principal's scope: pins every read.
    unscoped:     AtomicU64,
    /// The last write the table had no room for: pins every scoped read.
    overflow:     AtomicU64,
    /// Each principal's last write.
    by_principal: Mutex<HashMap<u64, u64>>,
    capacity:     usize,
}

impl WritePins {
    pub(super) fn new() -> Self {
        Self::with_capacity(PRINCIPAL_PIN_CAPACITY)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            any: AtomicU64::new(0),
            unscoped: AtomicU64::new(0),
            overflow: AtomicU64::new(0),
            by_principal: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    /// Record a write at `now_ms`, by `principal` when the write ran in a principal's scope.
    ///
    /// When the table is full, the entries whose window has passed are dropped first; a
    /// table still full of live pins records the write as one it had no room for.
    pub(super) fn record(&self, principal: Option<u64>, now_ms: u64, pin_ms: u64) {
        self.any.fetch_max(now_ms, Ordering::Relaxed);
        let Some(principal) = principal else {
            self.unscoped.fetch_max(now_ms, Ordering::Relaxed);
            return;
        };
        let mut table = self.table();
        if table.len() >= self.capacity && !table.contains_key(&principal) {
            table.retain(|_, written| super::pin_active(now_ms, *written, pin_ms));
            if table.len() >= self.capacity {
                drop(table);
                self.overflow.fetch_max(now_ms, Ordering::Relaxed);
                return;
            }
        }
        let written = table.entry(principal).or_insert(0);
        *written = (*written).max(now_ms);
    }

    /// Whether a read at `now_ms`, by `principal` when it runs in one's scope, is pinned.
    pub(super) fn pinned(&self, principal: Option<u64>, now_ms: u64, pin_ms: u64) -> bool {
        let active = |watermark: &AtomicU64| {
            super::pin_active(now_ms, watermark.load(Ordering::Relaxed), pin_ms)
        };
        // No write inside the window at all: nothing to look up.
        if !active(&self.any) {
            return false;
        }
        let Some(principal) = principal else {
            return true;
        };
        active(&self.unscoped)
            || active(&self.overflow)
            || self
                .table()
                .get(&principal)
                .is_some_and(|written| super::pin_active(now_ms, *written, pin_ms))
    }

    /// The table, whatever a thread that panicked holding it left: the entries are
    /// watermarks, each valid on its own.
    fn table(&self) -> std::sync::MutexGuard<'_, HashMap<u64, u64>> {
        self.by_principal.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[cfg(test)]
    fn held(&self) -> usize {
        self.table().len()
    }
}

#[cfg(test)]
#[path = "write_pins_tests.rs"]
mod tests;
