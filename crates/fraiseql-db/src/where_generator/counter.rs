//! Interior-mutable parameter counter for positional placeholders.

use std::cell::Cell;

/// Interior-mutable parameter counter.
///
/// Wraps `Cell<usize>` so `GenericWhereGenerator` can increment the counter
/// via shared (`&self`) references.
///
/// # Safety rationale
///
/// `Cell<T>` is not `Sync`, so `GenericWhereGenerator` (which contains this)
/// is not `Sync` either — correct, because generators must not be shared
/// across threads simultaneously.  Each query uses its own generator instance.
pub(super) struct ParamCounter(Cell<usize>);

impl ParamCounter {
    pub(super) const fn new() -> Self {
        Self(Cell::new(0))
    }

    /// Reset counter to `start` (used at the beginning of `generate()`).
    pub(super) fn reset_to(&self, start: usize) {
        self.0.set(start);
    }

    /// Increment counter and return the new 1-based value.
    pub(super) fn next(&self) -> usize {
        let n = self.0.get() + 1;
        self.0.set(n);
        n
    }
}
