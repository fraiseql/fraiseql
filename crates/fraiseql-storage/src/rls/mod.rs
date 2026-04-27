//! Row-level security enforcement for storage operations.

/// Storage RLS evaluator.
pub struct StorageRlsEvaluator;

impl StorageRlsEvaluator {
    /// Create a new evaluator.
    pub fn new() -> Self {
        Self
    }
}

impl Default for StorageRlsEvaluator {
    fn default() -> Self {
        Self::new()
    }
}
