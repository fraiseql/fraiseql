//! Resource limiter implementation for WASM execution.
//!
//! Enforces memory limits and tracks peak memory usage during WASM component execution.

use std::{error::Error, fmt};

use wasmtime::ResourceLimiter;

/// Error type for resource limit violations.
#[derive(Debug)]
pub struct ResourceLimitError {
    message: String,
}

impl ResourceLimitError {
    /// Create a new resource limit error.
    #[allow(clippy::missing_const_for_fn)] // String isn't const-compatible
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

impl fmt::Display for ResourceLimitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for ResourceLimitError {}

/// Statistics tracking for a WASM execution.
#[derive(Debug, Clone, Copy)]
pub struct LimitStats {
    /// Peak memory usage in bytes.
    pub peak_memory:    u64,
    /// Current memory usage in bytes.
    pub current_memory: u64,
}

impl LimitStats {
    /// Create new limit stats with zero values.
    pub const fn new() -> Self {
        Self {
            peak_memory:    0,
            current_memory: 0,
        }
    }
}

impl Default for LimitStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource limiter for WASM module execution.
///
/// Enforces memory limits and tracks peak memory usage.
#[derive(Debug)]
pub struct FunctionStoreLimiter {
    /// Maximum memory allowed in bytes.
    max_limit: u64,
    /// Current memory usage.
    current:   u64,
    /// Peak memory usage seen.
    peak:      u64,
}

impl FunctionStoreLimiter {
    /// Create a new resource limiter with the given memory limit.
    #[must_use]
    pub const fn new(max_limit: u64) -> Self {
        Self {
            max_limit,
            current: 0,
            peak: 0,
        }
    }

    /// Get the current statistics.
    #[must_use]
    pub const fn stats(&self) -> LimitStats {
        LimitStats {
            peak_memory:    self.peak,
            current_memory: self.current,
        }
    }
}

impl ResourceLimiter for FunctionStoreLimiter {
    /// Handle memory growth requests.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the requested memory would exceed the limit.
    fn memory_growing(
        &mut self,
        _current: usize,
        desired: usize,
        _maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        // Update current memory
        self.current = desired as u64;

        // Track peak
        if self.current > self.peak {
            self.peak = self.current;
        }

        // Check against limit
        if self.current > self.max_limit {
            // Memory limit exceeded
            let msg = format!("Memory limit exceeded: {} > {}", self.current, self.max_limit);
            return Err(wasmtime::Error::new(ResourceLimitError::new(msg)));
        }

        Ok(true)
    }

    /// Handle table growth requests.
    ///
    /// For now, we allow table growth without limits (only memory is limited).
    fn table_growing(
        &mut self,
        _current: u32,
        _desired: u32,
        _maximum: Option<u32>,
    ) -> wasmtime::Result<bool> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
