//! Adaptive chunk sizing based on channel occupancy patterns
//!
//! This module implements self-tuning chunk sizes that automatically adjust batch sizes
//! based on observed backpressure (channel occupancy).
//!
//! **Critical Semantics**:
//! `chunk_size` controls **both**:
//! 1. MPSC channel capacity (backpressure buffer)
//! 2. Batch size for Postgres row parsing
//!
//! **Control Signal Interpretation**:
//! - **High occupancy** (>80%): Producer waiting on channel capacity, consumer slow
//!   → **Reduce `chunk_size`**: smaller batches reduce pressure, lower latency per item
//!
//! - **Low occupancy** (<20%): Consumer faster than producer, frequent context switches
//!   → **Increase `chunk_size`**: larger batches amortize parsing cost, less frequent wakeups
//!
//! **Design Principles**:
//! - Measurement-based adjustment (50-item window) for stability
//! - Hysteresis band (20%-80%) prevents frequent oscillation
//! - Minimum adjustment interval (1 second) prevents thrashing
//! - Conservative bounds (16-1024) prevent pathological extremes
//! - Clear window reset after adjustment (fresh observations)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Single observation of channel occupancy
#[derive(Copy, Clone, Debug)]
pub struct Occupancy {
    /// Percentage of channel capacity in use (0-100)
    percentage: usize,
}

/// Tracks channel occupancy and automatically adjusts chunk size based on backpressure
///
/// # Examples
///
/// ```rust
/// use fraiseql_wire::stream::AdaptiveChunking;
/// let mut adaptive = AdaptiveChunking::new();
/// let (buffered_items, channel_capacity) = (50usize, 256usize);
///
/// // Periodically observe channel occupancy
/// for _chunk_sent in 0..100 {
///     if let Some(new_size) = adaptive.observe(buffered_items, channel_capacity) {
///         println!("Adjusted chunk size to {}", new_size);
///     }
/// }
/// ```
pub struct AdaptiveChunking {
    /// Current chunk size (mutable, adjusted over time)
    current_size: usize,

    /// Absolute minimum chunk size (never decrease below this)
    pub min_size: usize,

    /// Absolute maximum chunk size (never increase beyond this)
    pub max_size: usize,

    /// Number of measurements to collect before making adjustment decision
    pub adjustment_window: usize,

    /// Rolling window of recent occupancy observations
    pub measurements: VecDeque<Occupancy>,

    /// Timestamp of last chunk size adjustment (for rate limiting)
    pub last_adjustment_time: Option<Instant>,

    /// Minimum time between adjustments (prevents thrashing/oscillation)
    min_adjustment_interval: Duration,
}

impl AdaptiveChunking {
    /// Create a new adaptive chunking controller with default bounds
    ///
    /// **Defaults**:
    /// - Initial chunk size: 256 items
    /// - Min size: 16 items
    /// - Max size: 1024 items
    /// - Adjustment window: 50 observations
    /// - Min adjustment interval: 1 second
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_wire::stream::AdaptiveChunking;
    /// let adaptive = AdaptiveChunking::new();
    /// assert_eq!(adaptive.current_size(), 256);
    /// ```
    #[must_use] 
    pub fn new() -> Self {
        Self {
            current_size: 256,
            min_size: 16,
            max_size: 1024,
            adjustment_window: 50,
            measurements: VecDeque::with_capacity(50),
            last_adjustment_time: None,
            min_adjustment_interval: Duration::from_secs(1),
        }
    }

    /// Record an occupancy observation and check if chunk size adjustment is warranted
    ///
    /// Call this method after each chunk is sent to the channel.
    /// Returns `Some(new_size)` if an adjustment should be applied, `None` otherwise.
    ///
    /// # Arguments
    ///
    /// * `items_buffered` - Number of items currently in the channel
    /// * `capacity` - Total capacity of the channel (usually equal to `chunk_size`)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_wire::stream::AdaptiveChunking;
    /// let mut adaptive = AdaptiveChunking::new();
    ///
    /// // Simulate high occupancy (90%)
    /// for _ in 0..50 {
    ///     adaptive.observe(230, 256);  // ~90% occupancy
    /// }
    ///
    /// // On the 51st observation, should trigger adjustment
    /// if let Some(new_size) = adaptive.observe(230, 256) {
    ///     println!("Adjusted to {}", new_size);  // Will be < 256
    /// }
    /// ```
    pub fn observe(&mut self, items_buffered: usize, capacity: usize) -> Option<usize> {
        // Calculate occupancy percentage (clamped at 100% if buffer exceeds capacity)
        // Special case: if capacity is 0, treat occupancy as 0% (consumer draining instantly)
        let pct = if capacity == 0 {
            0
        } else {
            (items_buffered * 100)
                .checked_div(capacity)
                .unwrap_or(100)
                .min(100)
        };

        // Record this observation
        self.measurements.push_back(Occupancy { percentage: pct });

        // Keep only the most recent measurements in the window
        while self.measurements.len() > self.adjustment_window {
            self.measurements.pop_front();
        }

        // Only consider adjustment if we have a FULL window of observations
        // (i.e., exactly equal to the window size, not more)
        // This ensures we only evaluate after collecting N measurements
        if self.measurements.len() == self.adjustment_window && self.should_adjust() {
            return self.calculate_adjustment();
        }

        None
    }

    /// Get the current chunk size
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_wire::stream::AdaptiveChunking;
    /// let adaptive = AdaptiveChunking::new();
    /// assert_eq!(adaptive.current_size(), 256);
    /// ```
    #[must_use] 
    pub const fn current_size(&self) -> usize {
        self.current_size
    }

    /// Set custom min/max bounds for chunk size adjustments
    ///
    /// Allows overriding the default bounds (16-1024) with custom limits.
    /// The current chunk size will be clamped to the new bounds.
    ///
    /// # Arguments
    ///
    /// * `min_size` - Minimum chunk size (must be > 0)
    /// * `max_size` - Maximum chunk size (must be >= `min_size`)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use fraiseql_wire::stream::AdaptiveChunking;
    /// let mut adaptive = AdaptiveChunking::new();
    /// adaptive = adaptive.with_bounds(32, 512);  // Custom range 32-512
    /// assert!(adaptive.current_size() >= 32);
    /// assert!(adaptive.current_size() <= 512);
    /// ```
    pub fn with_bounds(mut self, min_size: usize, max_size: usize) -> Self {
        // Basic validation
        if min_size == 0 || max_size < min_size {
            tracing::warn!(
                "invalid chunk bounds: min={}, max={}, keeping defaults",
                min_size,
                max_size
            );
            return self;
        }

        self.min_size = min_size;
        self.max_size = max_size;

        // Clamp current size to new bounds
        if self.current_size < min_size {
            self.current_size = min_size;
        } else if self.current_size > max_size {
            self.current_size = max_size;
        }

        tracing::debug!(
            "adaptive chunking bounds set: min={}, max={}, current={}",
            self.min_size,
            self.max_size,
            self.current_size
        );

        self
    }

    /// Calculate average occupancy percentage over the measurement window
    #[must_use] 
    pub fn average_occupancy(&self) -> usize {
        if self.measurements.is_empty() {
            return 0;
        }

        let sum: usize = self.measurements.iter().map(|m| m.percentage).sum();
        sum / self.measurements.len()
    }

    /// Check if adjustment conditions are met
    ///
    /// Adjustment is only considered if:
    /// 1. At least 1 second has elapsed since the last adjustment
    /// 2. Average occupancy is outside the hysteresis band (< 20% or > 80%)
    fn should_adjust(&self) -> bool {
        // Rate limit: don't adjust too frequently
        if let Some(last_adj) = self.last_adjustment_time {
            if last_adj.elapsed() < self.min_adjustment_interval {
                return false;
            }
        }

        // Hysteresis: only adjust if we're clearly outside the comfort zone
        let avg = self.average_occupancy();
        !(20..=80).contains(&avg)
    }

    /// Calculate the new chunk size based on average occupancy
    ///
    /// **Logic**:
    /// - If avg > 80%: **DECREASE** by factor of 1.5 (high occupancy = producer backed up)
    /// - If avg < 20%: **INCREASE** by factor of 1.5 (low occupancy = consumer fast)
    /// - Clamps to [`min_size`, `max_size`]
    /// - Clears measurements after adjustment
    ///
    /// Returns `Some(new_size)` if size actually changed, `None` if no change needed.
    fn calculate_adjustment(&mut self) -> Option<usize> {
        let avg = self.average_occupancy();
        let old_size = self.current_size;

        let new_size = if avg > 80 {
            // High occupancy: producer is waiting on channel, consumer is slow
            // → DECREASE chunk_size to reduce backpressure and latency
            ((self.current_size as f64 / 1.5).floor() as usize).max(self.min_size)
        } else if avg < 20 {
            // Low occupancy: consumer is draining fast, producer could batch more
            // → INCREASE chunk_size to amortize parsing cost and reduce context switches
            ((self.current_size as f64 * 1.5).ceil() as usize).min(self.max_size)
        } else {
            old_size
        };

        // Only return if there was an actual change
        if new_size != old_size {
            self.current_size = new_size;
            self.last_adjustment_time = Some(Instant::now());
            self.measurements.clear(); // Reset window for fresh observations
            Some(new_size)
        } else {
            None
        }
    }
}

impl Default for AdaptiveChunking {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
