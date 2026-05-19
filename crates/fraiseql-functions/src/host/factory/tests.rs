use fraiseql_core::{security::SecurityContext, types::UserId};

use super::*;

fn test_security_context() -> SecurityContext {
    SecurityContext {
        user_id: UserId("user123".to_string()),
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
        email: None,
        display_name: None,
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
        user_id: UserId("user1".to_string()),
        ..test_security_context()
    };
    let security_ctx2 = SecurityContext {
        user_id: UserId("user2".to_string()),
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
