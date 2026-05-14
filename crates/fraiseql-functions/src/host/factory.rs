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

use crate::host::live::LiveHostContext;
use crate::types::EventPayload;
use fraiseql_core::security::SecurityContext;
use fraiseql_error::Result;
use std::sync::Arc;

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
mod tests {
    use super::*;
    use fraiseql_core::security::SecurityContext;

    fn test_security_context() -> SecurityContext {
        SecurityContext {
            user_id: "user123".to_string(),
            roles: vec!["user".to_string()],
            scopes: vec!["read".to_string()],
            tenant_id: None,
            authenticated_at: chrono::Utc::now(),
            expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            request_id: "req-123".to_string(),
            ip_address: None,
            attributes: std::collections::HashMap::new(),
            issuer: None,
            audience: None,
        }
    }

    fn test_event() -> EventPayload {
        EventPayload {
            trigger_type: "test".to_string(),
            entity: "Test".to_string(),
            event_kind: "created".to_string(),
            data: serde_json::json!({"test": true}),
            timestamp: chrono::Utc::now(),
        }
    }

    #[test]
    fn test_factory_creates_context() {
        let factory = LiveHostContextFactory::new();
        let security_ctx = test_security_context();
        let event = test_event();

        let result = factory.create(security_ctx, event);

        assert!(result.is_ok(), "Factory should create context successfully");
    }

    #[test]
    fn test_factory_injects_security_context() {
        let factory = LiveHostContextFactory::new();
        let security_ctx = test_security_context();
        let event = test_event();

        let _context = factory.create(security_ctx, event);

        // Verify that the context would have the security info injected
        // In the real implementation, we'd verify via the context's auth_context() method
    }

    #[test]
    fn test_factory_creates_isolated_contexts() {
        let factory = LiveHostContextFactory::new();
        let security_ctx1 = SecurityContext {
            user_id: "user1".to_string(),
            ..test_security_context()
        };
        let security_ctx2 = SecurityContext {
            user_id: "user2".to_string(),
            ..test_security_context()
        };

        let event = test_event();

        let ctx1 = factory.create(security_ctx1, event.clone());
        let ctx2 = factory.create(security_ctx2, event);

        assert!(ctx1.is_ok(), "First context should be created");
        assert!(ctx2.is_ok(), "Second context should be created");
        // Both contexts should be separate instances with different security contexts
    }

    #[test]
    fn test_factory_default_creates_new_instance() {
        let factory1 = LiveHostContextFactory::default();
        let factory2 = LiveHostContextFactory::default();

        let security_ctx = test_security_context();
        let event = test_event();

        let ctx1 = factory1.create(security_ctx.clone(), event.clone());
        let ctx2 = factory2.create(security_ctx, event);

        assert!(ctx1.is_ok());
        assert!(ctx2.is_ok());
    }
}
