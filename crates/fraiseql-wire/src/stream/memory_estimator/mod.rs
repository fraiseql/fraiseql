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
        items_buffered * 2048 // 2 KB per item
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
    #[must_use]
    pub const fn new(bytes_per_item: usize) -> Self {
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
mod tests;
