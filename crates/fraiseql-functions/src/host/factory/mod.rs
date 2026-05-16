//! Factory for creating per-invocation `HostContext` instances.
//!
//! This module provides the factory pattern for creating fresh `LiveHostContext` instances
//! for each function invocation, with properly configured backends and security context.
//!
//! The factory handles:
//! - Per-invocation `SecurityContext` injection
//! - Backend configuration (GraphQL, storage, HTTP)
//! - Resource limit enforcement
//! - Proper isolation between invocations

use std::sync::Arc;

use fraiseql_core::security::SecurityContext;
use fraiseql_error::Result;

use crate::{host::live::LiveHostContext, types::EventPayload};

/// Trait for creating per-invocation host contexts.
///
/// Implementations provide a factory that can create fresh `LiveHostContext` instances
/// with proper backend wiring and security configuration.
pub trait HostContextFactory: Send + Sync {
    /// Create a new host context for a function invocation.
    ///
    /// # Arguments
    ///
    /// - `security_context`: The authenticated user's security context
    /// - `event`: The triggering event for this invocation
    ///
    /// # Returns
    ///
    /// A new `LiveHostContext` configured with all backends and security info
    ///
    /// # Errors
    ///
    /// Returns `Err` if the host context cannot be constructed (e.g. missing configuration).
    fn create(
        &self,
        security_context: SecurityContext,
        event: EventPayload,
    ) -> Result<Arc<dyn Send + Sync>>;
}

/// Production implementation of `HostContextFactory`.
///
/// Wires together all FraiseQL backend services for function execution.
///
/// This factory creates stub host contexts suitable for function invocations that
/// do not require database, storage, or HTTP access. For full backend wiring,
/// configure the factory with the appropriate services.
pub struct LiveHostContextFactory {
    // Backend services will be added here
    // This is where QueryExecutor, DatabaseAdapter, StorageBackend, etc. would be stored
}

impl LiveHostContextFactory {
    /// Create a new factory with default configuration.
    ///
    /// # Future Enhancement
    ///
    /// This will accept fully configured backend services from the caller.
    pub const fn new() -> Self {
        Self {}
    }
}

impl Default for LiveHostContextFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl HostContextFactory for LiveHostContextFactory {
    fn create(
        &self,
        security_context: SecurityContext,
        event: EventPayload,
    ) -> Result<Arc<dyn Send + Sync>> {
        // Create a new LiveHostContext with the security context injected
        let mut ctx = LiveHostContext::new(event, crate::host::live::HostContextConfig::default());
        ctx.security_context = security_context;

        // Return as Arc<dyn Send + Sync> for dynamic dispatch
        Ok(Arc::new(ctx))
    }
}

#[cfg(test)]
mod tests;
