//! Memory estimator for buffered items
//!
//! Provides pluggable memory estimation strategy for buffered JSON items.
//! Default: conservative 2KB per item estimation.

/// Trait for estimating memory usage of buffered items
///
/// This allows customization of memory estimation if workload characteristics differ
/// from the default conservative 2KB per item assumption.
pub trait MemoryEstimator: Send + Sync {
    /// Estimate total memory in bytes for given number of buffered items
    fn estimate_bytes(&self, items_buffered: usize) -> usize;

    /// Human-readable name for this estimator (for debugging/logging)
    fn name(&self) -> &'static str;
}

/// Default conservative memory estimator: 2KB per item
///
/// Used by default for all streams. Suitable for typical JSON documents (1-5KB).
/// - Underestimates small objects (< 2KB) → hits limit later (safe)
/// - Overestimates large objects (> 2KB) → hits limit earlier (safe)
#[derive(Debug, Clone)]
pub struct ConservativeEstimator;

impl MemoryEstimator for ConservativeEstimator {
    fn estimate_bytes(&self, items_buffered: usize) -> usize {
        items_buffered * 2048  // 2 KB per item
    }

    fn name(&self) -> &'static str {
        "conservative_2kb"
    }
}

/// Custom memory estimator using fixed bytes per item
///
/// Use this if your analysis shows different typical item sizes.
/// Example: if your JSON averages 4KB, use `FixedEstimator::new(4096)`
#[derive(Debug, Clone)]
pub struct FixedEstimator {
    bytes_per_item: usize,
}

impl FixedEstimator {
    /// Create estimator with custom bytes-per-item
    pub fn new(bytes_per_item: usize) -> Self {
        Self { bytes_per_item }
    }
}

impl MemoryEstimator for FixedEstimator {
    fn estimate_bytes(&self, items_buffered: usize) -> usize {
        items_buffered.saturating_mul(self.bytes_per_item)
    }

    fn name(&self) -> &'static str {
        "fixed_custom"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conservative_estimator() {
        let est = ConservativeEstimator;
        assert_eq!(est.estimate_bytes(0), 0);
        assert_eq!(est.estimate_bytes(1), 2048);
        assert_eq!(est.estimate_bytes(100), 204_800);
        assert_eq!(est.estimate_bytes(256), 524_288);
    }

    #[test]
    fn test_conservative_name() {
        let est = ConservativeEstimator;
        assert_eq!(est.name(), "conservative_2kb");
    }

    #[test]
    fn test_fixed_estimator() {
        let est = FixedEstimator::new(4096);
        assert_eq!(est.estimate_bytes(0), 0);
        assert_eq!(est.estimate_bytes(1), 4096);
        assert_eq!(est.estimate_bytes(100), 409_600);
        assert_eq!(est.estimate_bytes(256), 1_048_576);
    }

    #[test]
    fn test_fixed_estimator_custom_sizes() {
        for size in &[1024, 2048, 4096, 8192] {
            let est = FixedEstimator::new(*size);
            assert_eq!(est.estimate_bytes(10), 10 * size);
        }
    }

    #[test]
    fn test_fixed_estimator_overflow_safe() {
        let est = FixedEstimator::new(usize::MAX / 2);
        // Should not panic on overflow
        let _ = est.estimate_bytes(usize::MAX);
    }
}
