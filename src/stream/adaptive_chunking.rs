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
//!   → **Reduce chunk_size**: smaller batches reduce pressure, lower latency per item
//!
//! - **Low occupancy** (<20%): Consumer faster than producer, frequent context switches
//!   → **Increase chunk_size**: larger batches amortize parsing cost, less frequent wakeups
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
struct Occupancy {
    /// Percentage of channel capacity in use (0-100)
    percentage: usize,
    /// Number of items buffered in the channel
    items_buffered: usize,
    /// Time of observation
    timestamp: Instant,
}

/// Tracks channel occupancy and automatically adjusts chunk size based on backpressure
///
/// # Examples
///
/// ```ignore
/// let mut adaptive = AdaptiveChunking::new();
///
/// // Periodically observe channel occupancy
/// for chunk_sent in 0..100 {
///     let occupancy_pct = (buffered_items * 100) / channel_capacity;
///     if let Some(new_size) = adaptive.observe(buffered_items, channel_capacity) {
///         println!("Adjusted chunk size: {} -> {}", adaptive.current_size() - new_size, new_size);
///     }
/// }
/// ```
pub struct AdaptiveChunking {
    /// Current chunk size (mutable, adjusted over time)
    current_size: usize,

    /// Absolute minimum chunk size (never decrease below this)
    min_size: usize,

    /// Absolute maximum chunk size (never increase beyond this)
    max_size: usize,

    /// Number of measurements to collect before making adjustment decision
    adjustment_window: usize,

    /// Rolling window of recent occupancy observations
    measurements: VecDeque<Occupancy>,

    /// Timestamp of last chunk size adjustment (for rate limiting)
    last_adjustment_time: Option<Instant>,

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
    /// ```ignore
    /// let adaptive = AdaptiveChunking::new();
    /// assert_eq!(adaptive.current_size(), 256);
    /// ```
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
    /// * `capacity` - Total capacity of the channel (usually equal to chunk_size)
    ///
    /// # Examples
    ///
    /// ```ignore
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
        let pct = if capacity == 0 {
            0
        } else {
            ((items_buffered * 100) / capacity).min(100)
        };

        // Record this observation
        self.measurements.push_back(Occupancy {
            percentage: pct,
            items_buffered,
            timestamp: Instant::now(),
        });

        // Keep only the most recent measurements in the window
        while self.measurements.len() > self.adjustment_window {
            self.measurements.pop_front();
        }

        // Only consider adjustment if we have a FULL window of observations
        // (i.e., exactly equal to the window size, not more)
        // This ensures we only evaluate after collecting N measurements
        if self.measurements.len() == self.adjustment_window {
            if self.should_adjust() {
                return self.calculate_adjustment();
            }
        }

        None
    }

    /// Get the current chunk size
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let adaptive = AdaptiveChunking::new();
    /// assert_eq!(adaptive.current_size(), 256);
    /// ```
    pub fn current_size(&self) -> usize {
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
    /// * `max_size` - Maximum chunk size (must be >= min_size)
    ///
    /// # Examples
    ///
    /// ```ignore
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
    fn average_occupancy(&self) -> usize {
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
        avg > 80 || avg < 20
    }

    /// Calculate the new chunk size based on average occupancy
    ///
    /// **Logic**:
    /// - If avg > 80%: **DECREASE** by factor of 1.5 (high occupancy = producer backed up)
    /// - If avg < 20%: **INCREASE** by factor of 1.5 (low occupancy = consumer fast)
    /// - Clamps to [min_size, max_size]
    /// - Clears measurements after adjustment
    ///
    /// Returns `Some(new_size)` if size actually changed, `None` if no change needed.
    fn calculate_adjustment(&mut self) -> Option<usize> {
        let avg = self.average_occupancy();
        let old_size = self.current_size;

        let new_size = if avg > 80 {
            // High occupancy: producer is waiting on channel, consumer is slow
            // → DECREASE chunk_size to reduce backpressure and latency
            let decreased = ((self.current_size as f64 / 1.5).floor() as usize).max(self.min_size);
            decreased
        } else if avg < 20 {
            // Low occupancy: consumer is draining fast, producer could batch more
            // → INCREASE chunk_size to amortize parsing cost and reduce context switches
            let increased = ((self.current_size as f64 * 1.5).ceil() as usize).min(self.max_size);
            increased
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
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let adaptive = AdaptiveChunking::new();
        assert_eq!(adaptive.current_size(), 256);
        assert_eq!(adaptive.min_size, 16);
        assert_eq!(adaptive.max_size, 1024);
        assert_eq!(adaptive.adjustment_window, 50);
        assert!(adaptive.last_adjustment_time.is_none());
        assert!(adaptive.measurements.is_empty());
    }

    #[test]
    fn test_no_adjustment_in_hysteresis_band() {
        let mut adaptive = AdaptiveChunking::new();

        // Simulate 50% occupancy (inside 20-80% hysteresis band)
        // 50% of 256 = 128 items
        for _ in 0..50 {
            assert_eq!(adaptive.observe(128, 256), None);
        }

        // Should not adjust - still at 256
        assert_eq!(adaptive.current_size(), 256);
    }

    #[test]
    fn test_decrease_on_high_occupancy() {
        let mut adaptive = AdaptiveChunking::new();
        let original_size = 256;

        // Simulate 90% occupancy (producer backed up, consumer slow)
        // 90% of 256 = 230.4 ≈ 230 items
        for _ in 0..49 {
            assert_eq!(adaptive.observe(230, 256), None);
        }

        // On 50th observation, should trigger adjustment
        let result = adaptive.observe(230, 256);
        assert!(result.is_some());

        let new_size = result.unwrap();
        assert!(new_size < original_size, "Should decrease on high occupancy");
        assert!(new_size >= 16, "Should respect min bound");
    }

    #[test]
    fn test_increase_on_low_occupancy() {
        let mut adaptive = AdaptiveChunking::new();
        let original_size = 256;

        // Simulate 10% occupancy (consumer fast, producer lagging)
        // 10% of 256 = 25.6 ≈ 26 items
        for _ in 0..49 {
            assert_eq!(adaptive.observe(26, 256), None);
        }

        // On 50th observation, should trigger adjustment
        let result = adaptive.observe(26, 256);
        assert!(result.is_some());

        let new_size = result.unwrap();
        assert!(new_size > original_size, "Should increase on low occupancy");
        assert!(new_size <= 1024, "Should respect max bound");
    }

    #[test]
    fn test_respects_min_bound() {
        let mut adaptive = AdaptiveChunking::new();

        // Simulate very high occupancy repeatedly
        for iteration in 0..20 {
            // Reset measurements every iteration to allow adjustments
            for _ in 0..50 {
                adaptive.observe(250, 256);
            }
            adaptive.observe(250, 256);

            // Verify we never go below minimum
            assert!(
                adaptive.current_size() >= 16,
                "Iteration {}: size {} < min",
                iteration,
                adaptive.current_size()
            );
        }
    }

    #[test]
    fn test_respects_max_bound() {
        let mut adaptive = AdaptiveChunking::new();

        // Simulate very low occupancy repeatedly
        for iteration in 0..20 {
            // Reset measurements every iteration to allow adjustments
            for _ in 0..50 {
                adaptive.observe(10, 256);
            }
            adaptive.observe(10, 256);

            // Verify we never go above maximum
            assert!(
                adaptive.current_size() <= 1024,
                "Iteration {}: size {} > max",
                iteration,
                adaptive.current_size()
            );
        }
    }

    #[test]
    fn test_respects_min_adjustment_interval() {
        let mut adaptive = AdaptiveChunking::new();

        // Fill window with high occupancy (>80%) and trigger first adjustment
        // 230/256 ≈ 89.8%
        // Make 49 calls so window is not yet full
        for _ in 0..49 {
            let result = adaptive.observe(230, 256);
            assert_eq!(result, None, "Should not adjust yet, window not full");
        }

        // 50th call: window becomes full, should trigger adjustment
        let first_adjustment = adaptive.observe(230, 256);
        assert!(
            first_adjustment.is_some(),
            "Should adjust on 50th observation when window is full"
        );

        let first_size = adaptive.current_size();
        assert!(first_size < 256, "High occupancy should decrease chunk size");

        // Immediately try to trigger another adjustment within 1 second
        // This should NOT happen because of the 1-second minimum interval
        // Build up a new window with different occupancy, still shouldn't trigger
        for _ in 0..50 {
            let result = adaptive.observe(230, 256);
            assert_eq!(
                result, None,
                "Should not adjust again so soon (within min interval)"
            );
        }

        // Should not adjust again immediately, even though window is full again
        assert_eq!(
            adaptive.current_size(),
            first_size,
            "Size should remain unchanged due to rate limiting"
        );
    }

    #[test]
    fn test_window_resets_after_adjustment() {
        let mut adaptive = AdaptiveChunking::new();

        // First window: high occupancy triggers decrease
        // 230/256 ≈ 89.8%
        // Make 49 calls to fill window to size 49
        for _ in 0..49 {
            let result = adaptive.observe(230, 256);
            assert_eq!(result, None, "Should not adjust yet, window not full");
        }

        // 50th call: window becomes full, triggers adjustment
        let first = adaptive.observe(230, 256);
        assert!(first.is_some(), "Should adjust when window reaches 50 observations");

        // Measurements should be cleared after adjustment
        assert!(
            adaptive.measurements.is_empty(),
            "Measurements should be cleared after adjustment"
        );
    }

    #[test]
    fn test_zero_capacity_handling() {
        let mut adaptive = AdaptiveChunking::new();

        // Zero capacity edge case: percentage = 0
        // 0% occupancy is OUTSIDE hysteresis band (< 20%), so it WILL increase chunk size
        // This makes sense: consumer is draining instantly, we can send bigger batches
        // Make 49 calls so window is not yet full (size 49 < 50)
        for _ in 0..49 {
            let result = adaptive.observe(0, 0);
            // Should not adjust until window is full (50 observations)
            assert_eq!(result, None, "Should not adjust until window is full");
        }

        // On the 50th observation, window becomes full
        // We should trigger an increase because occupancy < 20%
        let result = adaptive.observe(0, 0);
        assert!(
            result.is_some(),
            "Should increase chunk size when occupancy < 20% and window is full"
        );
        assert!(
            adaptive.current_size() > 256,
            "Should increase from 256 due to low occupancy"
        );
    }

    #[test]
    fn test_average_occupancy_calculation() {
        let mut adaptive = AdaptiveChunking::new();

        // Add measurements: 10%, 20%, 30%, 40%, 50%
        // Calculate actual item counts: 25.6, 51.2, 76.8, 102.4, 128
        // Which truncate to: 25, 51, 76, 102, 128
        // And percentages: (25*100)/256=9, (51*100)/256=19, (76*100)/256=29, (102*100)/256=39, (128*100)/256=50
        for pct in [10, 20, 30, 40, 50].iter() {
            let items = (pct * 256) / 100;
            adaptive.observe(items, 256);
        }

        let avg = adaptive.average_occupancy();
        // Average of [9, 19, 29, 39, 50] = 146 / 5 = 29 (integer division)
        assert_eq!(avg, 29, "Average should account for integer division in percentages");
    }
}
