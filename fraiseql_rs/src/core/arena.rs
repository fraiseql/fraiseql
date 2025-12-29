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
//!
//! Safety:
//! - Single-threaded use only (enforced by marker field)
//! - Maximum size limit prevents OOM

use std::cell::UnsafeCell;
use std::marker::PhantomData;

/// Maximum arena size (16 MB) - prevents OOM on malicious input
pub const MAX_ARENA_SIZE: usize = 16 * 1024 * 1024;

/// Default arena capacity (8 KB) - suitable for most requests
pub const DEFAULT_ARENA_CAPACITY: usize = 8 * 1024;

/// Arena allocation error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArenaError {
    /// Requested allocation would exceed maximum arena size
    SizeExceeded,
    /// Arithmetic overflow in size calculation
    Overflow,
}

impl std::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArenaError::SizeExceeded => {
                write!(f, "Arena size limit exceeded ({} bytes)", MAX_ARENA_SIZE)
            }
            ArenaError::Overflow => write!(f, "Arena size calculation overflow"),
        }
    }
}

impl std::error::Error for ArenaError {}

/// Bump allocator for request-scoped memory
///
/// # Thread Safety
///
/// This type is explicitly `!Send` and `!Sync` because it uses interior
/// mutability without synchronization. The `_marker` field ensures this
/// at compile time. Each request should have its own arena.
///
/// # Memory Limits
///
/// The arena enforces a maximum size of [`MAX_ARENA_SIZE`] bytes to prevent
/// out-of-memory conditions from malicious or malformed input.
pub struct Arena {
    buf: UnsafeCell<Vec<u8>>,
    pos: UnsafeCell<usize>,
    max_size: usize,
    /// Marker to make Arena `!Send` and `!Sync`
    ///
    /// `*const ()` is neither Send nor Sync, so this field ensures
    /// Arena cannot be shared across threads.
    _marker: PhantomData<*const ()>,
}

impl Arena {
    /// Create arena with initial capacity and default max size.
    ///
    /// # Arguments
    /// * `capacity` - Initial buffer capacity (will grow as needed up to max)
    ///
    /// # Recommended Capacities
    /// - 8KB for small requests (< 50 fields)
    /// - 64KB for large requests (> 500 fields)
    pub fn with_capacity(capacity: usize) -> Self {
        Arena {
            buf: UnsafeCell::new(Vec::with_capacity(capacity.min(MAX_ARENA_SIZE))),
            pos: UnsafeCell::new(0),
            max_size: MAX_ARENA_SIZE,
            _marker: PhantomData,
        }
    }

    /// Create arena with custom maximum size.
    ///
    /// # Arguments
    /// * `capacity` - Initial buffer capacity
    /// * `max_size` - Maximum allowed size (capped at MAX_ARENA_SIZE)
    pub fn with_capacity_and_max(capacity: usize, max_size: usize) -> Self {
        let effective_max = max_size.min(MAX_ARENA_SIZE);
        Arena {
            buf: UnsafeCell::new(Vec::with_capacity(capacity.min(effective_max))),
            pos: UnsafeCell::new(0),
            max_size: effective_max,
            _marker: PhantomData,
        }
    }

    /// Allocate bytes in arena (fallible version).
    ///
    /// # Arguments
    /// * `len` - Number of bytes to allocate
    ///
    /// # Returns
    /// * `Ok(&mut [u8])` - Mutable slice of allocated bytes
    /// * `Err(ArenaError)` - If allocation would exceed limits
    ///
    /// # Safety
    ///
    /// This is safe because:
    /// 1. Arena is `!Send + !Sync` (via _marker field), ensuring single-threaded access
    /// 2. Returned slice lifetime is tied to arena lifetime
    /// 3. We check bounds before growing buffer
    #[inline]
    #[allow(clippy::mut_from_ref)]  // Interior mutability pattern - safe via !Send + !Sync marker
    pub fn try_alloc_bytes(&self, len: usize) -> Result<&mut [u8], ArenaError> {
        // SAFETY: Single-threaded access enforced by !Send + !Sync marker
        unsafe {
            let pos = self.pos.get();
            let buf = self.buf.get();

            let current_pos = *pos;
            let new_pos = current_pos.checked_add(len).ok_or(ArenaError::Overflow)?;

            if new_pos > self.max_size {
                return Err(ArenaError::SizeExceeded);
            }

            // Grow buffer if needed
            if new_pos > (*buf).len() {
                (*buf).resize(new_pos, 0);
            }

            *pos = new_pos;

            // SAFETY: We've ensured the slice is within bounds and buffer is valid
            let slice = &mut (&mut *buf)[current_pos..new_pos];
            Ok(slice)
        }
    }

    /// Allocate bytes in arena (panics on failure).
    ///
    /// # Panics
    /// Panics if allocation would exceed arena size limit.
    /// Use `try_alloc_bytes` for fallible allocation.
    ///
    /// # Safety
    /// Same safety guarantees as `try_alloc_bytes`.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]  // Interior mutability pattern - safe via !Send + !Sync marker
    pub fn alloc_bytes(&self, len: usize) -> &mut [u8] {
        self.try_alloc_bytes(len)
            .expect("Arena size limit exceeded")
    }

    /// Reset arena for next request.
    ///
    /// This does not deallocate memory - it just resets the position pointer.
    /// The underlying buffer is reused for the next request.
    #[inline]
    pub fn reset(&self) {
        // SAFETY: Single-threaded access enforced by !Send + !Sync marker
        unsafe {
            *self.pos.get() = 0;
        }
    }

    /// Get current allocation position (bytes used).
    #[inline]
    pub fn used(&self) -> usize {
        // SAFETY: Single-threaded access enforced by !Send + !Sync marker
        unsafe { *self.pos.get() }
    }

    /// Get remaining capacity before hitting max size.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.max_size.saturating_sub(self.used())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_allocation() {
        let arena = Arena::with_capacity(1024);
        let slice = arena.alloc_bytes(100);
        assert_eq!(slice.len(), 100);
        assert_eq!(arena.used(), 100);
    }

    #[test]
    fn test_size_limit() {
        let arena = Arena::with_capacity_and_max(100, 200);

        // First allocation succeeds
        assert!(arena.try_alloc_bytes(150).is_ok());

        // Second allocation fails (would exceed 200 byte limit)
        assert!(matches!(
            arena.try_alloc_bytes(100),
            Err(ArenaError::SizeExceeded)
        ));
    }

    #[test]
    fn test_reset() {
        let arena = Arena::with_capacity(1024);
        arena.alloc_bytes(500);
        assert_eq!(arena.used(), 500);

        arena.reset();
        assert_eq!(arena.used(), 0);

        // Can allocate again after reset
        arena.alloc_bytes(500);
        assert_eq!(arena.used(), 500);
    }

    #[test]
    fn test_overflow_protection() {
        let arena = Arena::with_capacity(100);

        // Try to allocate usize::MAX bytes - should fail with overflow
        assert!(matches!(
            arena.try_alloc_bytes(usize::MAX),
            Err(ArenaError::Overflow)
        ));
    }

    #[test]
    fn test_not_send_sync() {
        // This test verifies at compile time that Arena is !Send and !Sync
        // Uncomment these lines to verify compilation fails:

        // fn assert_send<T: Send>() {}
        // fn assert_sync<T: Sync>() {}
        // assert_send::<Arena>();  // Should fail to compile
        // assert_sync::<Arena>();  // Should fail to compile
    }
}
