//! A gate every write passes before anything runs (#633).
//!
//! The server installs one on a tenant's executor when the tenant has a storage quota: a
//! tenant over its quota has every write refused at `execute_mutation_impl`, the chokepoint
//! every transport's write passes, before any authorization, hook or statement. Reads never
//! consult it.

use crate::error::Result;

/// Admits or refuses a write before anything of it runs.
///
/// Consulted once per mutation by `execute_mutation_impl`, right after the writer is
/// resolved: a refusal returns the gate's error and runs nothing, not even a
/// `before:mutation` hook.
pub trait WriteGate: Send + Sync {
    /// Admit a write of `mutation`, or refuse it with the error the caller is answered.
    ///
    /// # Errors
    ///
    /// The refusal, such as `FraiseQLError::StorageQuotaExceeded`.
    fn admit(&self, mutation: &str) -> Result<()>;
}
