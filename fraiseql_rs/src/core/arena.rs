//! Bump allocator for request-scoped memory
//!
//! All temporary allocations (transformed keys, intermediate buffers)
//! use this arena. When request completes, entire arena is freed at once.
//!
//! Performance:
//! - Allocation: O(1) - just bump a pointer!
//! - Deallocation: O(1) - free entire arena
//! - Cache-friendly: Linear memory layout
//! - No fragmentation: Reset pointer between requests

use std::cell::UnsafeCell;

/// Bump allocator for request-scoped memory
///
/// All temporary allocations (transformed keys, intermediate buffers)
/// use this arena. When request completes, entire arena is freed at once.
///
/// Performance:
/// - Allocation: O(1) - just bump a pointer!
/// - Deallocation: O(1) - free entire arena
/// - Cache-friendly: Linear memory layout
/// - No fragmentation: Reset pointer between requests
pub struct Arena {
    buf: UnsafeCell<Vec<u8>>,
    pos: UnsafeCell<usize>,
}

impl Arena {
    /// Create arena with initial capacity
    ///
    /// Recommended: 8KB for small requests, 64KB for large
    pub fn with_capacity(capacity: usize) -> Self {
        Arena {
            buf: UnsafeCell::new(Vec::with_capacity(capacity)),
            pos: UnsafeCell::new(0),
        }
    }

    /// Allocate bytes in arena
    ///
    /// SAFETY: Single-threaded use only (per-request)
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub fn alloc_bytes(&self, len: usize) -> &mut [u8] {
        unsafe {
            let pos = self.pos.get();
            let buf = self.buf.get();

            let current_pos = *pos;
            let new_pos = current_pos + len;

            // Ensure capacity
            if new_pos > (*buf).len() {
                (*buf).resize(new_pos, 0);
            }

            *pos = new_pos;

            &mut (&mut *buf)[current_pos..new_pos]
        }
    }

    /// Reset arena for next request
    #[inline]
    pub fn reset(&self) {
        unsafe {
            *self.pos.get() = 0;
        }
    }
}
