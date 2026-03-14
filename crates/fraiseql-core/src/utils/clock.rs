//! Clock abstraction for deterministic time-dependent testing.
//!
//! Inject [`Clock`] into any component that calls `SystemTime::now()`,
//! enabling unit tests to control time without real-time delays.
//!
//! # Usage
//!
//! Production code should accept `Arc<dyn Clock>` and use
//! [`SystemClock`] as the default:
//!
//! ```rust
//! use std::sync::Arc;
//! use fraiseql_core::utils::clock::{Clock, SystemClock};
//!
//! struct MyComponent {
//!     clock: Arc<dyn Clock>,
//! }
//!
//! impl MyComponent {
//!     pub fn new() -> Self {
//!         Self { clock: Arc::new(SystemClock) }
//!     }
//!
//!     pub fn new_with_clock(clock: Arc<dyn Clock>) -> Self {
//!         Self { clock }
//!     }
//! }
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

/// Abstraction over the system clock.
///
/// Inject this into any component that needs time-based logic to enable
/// deterministic testing without real-time delays.
pub trait Clock: Send + Sync + 'static {
    /// Return the current time.
    fn now(&self) -> SystemTime;

    /// Return the current Unix timestamp in whole seconds.
    ///
    /// Equivalent to `now().duration_since(UNIX_EPOCH).as_secs()`.
    fn now_secs(&self) -> u64 {
        self.now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
    }

    /// Return the current Unix timestamp as a signed 64-bit integer.
    ///
    /// Safe until the year 292,277,026,596 when `u64` overflows `i64`.
    fn now_secs_i64(&self) -> i64 {
        i64::try_from(self.now_secs()).unwrap_or(0)
    }
}

/// Production clock: delegates to [`SystemTime::now()`].
#[derive(Debug, Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[inline]
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Manually advanceable clock for deterministic tests.
///
/// Starts at `UNIX_EPOCH + 1_000_000 s` to avoid edge cases near the epoch.
/// All clones share the same underlying time via an `Arc`.
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use fraiseql_core::utils::clock::{Clock, ManualClock};
///
/// let clock = ManualClock::new();
/// let t0 = clock.now_secs();
///
/// clock.advance(Duration::from_secs(10));
/// assert_eq!(clock.now_secs(), t0 + 10);
/// ```
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct ManualClock {
    current: std::sync::Arc<std::sync::Mutex<SystemTime>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl ManualClock {
    /// Create a new clock starting at `UNIX_EPOCH + 1_000_000 s`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: std::sync::Arc::new(std::sync::Mutex::new(
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000),
            )),
        }
    }

    /// Advance the clock by `delta`. All clones see the new time immediately.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn advance(&self, delta: std::time::Duration) {
        *self.current.lock().expect("ManualClock mutex poisoned") += delta;
    }

    /// Set the clock to an absolute time.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn set(&self, t: SystemTime) {
        *self.current.lock().expect("ManualClock mutex poisoned") = t;
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Clock for ManualClock {
    fn now(&self) -> SystemTime {
        *self.current.lock().expect("ManualClock mutex poisoned")
    }
}
