#![allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
#![allow(clippy::explicit_iter_loop)] // Reason: explicit `.iter()` keeps the iteration intent obvious in test setup
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
    // 90% of 256 = 230.4 ~ 230 items
    for _ in 0..49 {
        assert_eq!(adaptive.observe(230, 256), None);
    }

    // On 50th observation, should trigger adjustment
    let result = adaptive.observe(230, 256);
    assert!(result.is_some());

    let new_size = result.unwrap();
    assert!(
        new_size < original_size,
        "Should decrease on high occupancy"
    );
    assert!(new_size >= 16, "Should respect min bound");
}

#[test]
fn test_increase_on_low_occupancy() {
    let mut adaptive = AdaptiveChunking::new();
    let original_size = 256;

    // Simulate 10% occupancy (consumer fast, producer lagging)
    // 10% of 256 = 25.6 ~ 26 items
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
    // 230/256 ~ 89.8%
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
    assert!(
        first_size < 256,
        "High occupancy should decrease chunk size"
    );

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
    // 230/256 ~ 89.8%
    // Make 49 calls to fill window to size 49
    for _ in 0..49 {
        let result = adaptive.observe(230, 256);
        assert_eq!(result, None, "Should not adjust yet, window not full");
    }

    // 50th call: window becomes full, triggers adjustment
    let first = adaptive.observe(230, 256);
    assert!(
        first.is_some(),
        "Should adjust when window reaches 50 observations"
    );

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
    assert_eq!(
        avg, 29,
        "Average should account for integer division in percentages"
    );
}
