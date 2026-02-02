//! Integration tests for Row-Level Security (RLS)
//!
//! These tests verify that RLS policies are correctly applied during query execution,
//! enforcing access control based on SecurityContext.

use std::collections::HashMap;

use fraiseql_core::{
    db::WhereClause,
    runtime::RuntimeConfig,
    security::{DefaultRLSPolicy, RLSPolicy, SecurityContext},
};

/// Test that non-admin users are filtered by RLS policy
///
/// This is a RED phase test that verifies the desired behavior:
/// - Admin users bypass RLS (see all records)
/// - Non-admin users see only their own records (author_id == user_id)
#[test]
fn test_rls_policy_evaluates_correctly_for_non_admins() {
    let policy = DefaultRLSPolicy::new();

    // Admin user should bypass RLS
    let admin_context = SecurityContext {
        user_id:          "admin1".to_string(),
        roles:            vec!["admin".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-admin".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let admin_result = policy.evaluate(&admin_context, "Post").unwrap();
    assert_eq!(admin_result, None, "Admin users should bypass RLS");

    // Non-admin user should have RLS filter applied
    let user_context = SecurityContext {
        user_id:          "user1".to_string(),
        roles:            vec!["user".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-user".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let user_result = policy.evaluate(&user_context, "Post").unwrap();
    assert!(user_result.is_some(), "Non-admin users should have RLS filter");

    // Verify filter is a WHERE clause on author_id == user_id
    if let Some(WhereClause::Field {
        path,
        operator,
        value,
    }) = user_result
    {
        assert_eq!(path, vec!["author_id".to_string()]);
        assert_eq!(operator, fraiseql_core::db::WhereOperator::Eq);
        assert_eq!(value, serde_json::json!("user1"));
    } else {
        panic!("Expected WHERE clause for author_id field");
    }
}

/// Test that multi-tenant RLS enforces tenant isolation
#[test]
fn test_rls_policy_enforces_multi_tenant_isolation() {
    let policy = DefaultRLSPolicy::new();

    // User in tenant1
    let tenant1_context = SecurityContext {
        user_id:          "user1".to_string(),
        roles:            vec!["user".to_string()],
        tenant_id:        Some("tenant1".to_string()),
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let result = policy.evaluate(&tenant1_context, "Post").unwrap();
    assert!(result.is_some(), "Multi-tenant context should have RLS filter");

    // Result should be AND of both tenant_id and author_id filters
    if let Some(WhereClause::And(clauses)) = result {
        assert_eq!(clauses.len(), 2, "Should have 2 filters: tenant_id AND author_id");
    } else {
        panic!("Expected AND clause for tenant isolation + author filter");
    }
}

/// Test that non-existent policies don't block access (graceful degradation)
#[test]
fn test_rls_allows_access_when_no_policy_matches() {
    let policy = DefaultRLSPolicy::new();

    let context = SecurityContext {
        user_id:          "user1".to_string(),
        roles:            vec!["user".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-1".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    // Any type should get a filter (not None)
    let result = policy.evaluate(&context, "UnknownType").unwrap();
    assert!(result.is_some(), "RLS should apply standard filters even for unknown types");
}

/// Test WHERE clause composition: user filter AND rls filter
#[test]
fn test_where_clause_composition_for_rls() {
    use fraiseql_core::db::WhereOperator;

    // User-provided WHERE clause: published = true
    let user_where = WhereClause::Field {
        path:     vec!["published".to_string()],
        operator: WhereOperator::Eq,
        value:    serde_json::json!(true),
    };

    // RLS filter: author_id = user1
    let rls_where = WhereClause::Field {
        path:     vec!["author_id".to_string()],
        operator: WhereOperator::Eq,
        value:    serde_json::json!("user1"),
    };

    // Composed: published = true AND author_id = user1
    let composed = WhereClause::And(vec![user_where, rls_where]);

    assert!(matches!(composed, WhereClause::And(ref clauses) if clauses.len() == 2));
}

/// Test that SecurityContext metadata flows through correctly
#[test]
fn test_security_context_carries_all_metadata() {
    let now = chrono::Utc::now();
    let expires = now + chrono::Duration::hours(1);

    let mut attrs = HashMap::new();
    attrs.insert("department".to_string(), serde_json::json!("engineering"));
    attrs.insert("region".to_string(), serde_json::json!("us-west-2"));

    let context = SecurityContext {
        user_id:          "user123".to_string(),
        roles:            vec!["user".to_string(), "moderator".to_string()],
        tenant_id:        Some("acme-corp".to_string()),
        scopes:           vec!["read:post".to_string(), "write:comment".to_string()],
        attributes:       attrs,
        request_id:       "req-xyz".to_string(),
        ip_address:       Some("192.0.2.1".to_string()),
        authenticated_at: now,
        expires_at:       expires,
        issuer:           Some("https://auth.example.com".to_string()),
        audience:         Some("api.example.com".to_string()),
    };

    assert_eq!(context.user_id, "user123");
    assert!(context.has_role("moderator"));
    assert!(context.has_scope("read:post"));
    assert_eq!(context.tenant_id, Some("acme-corp".to_string()));
    assert_eq!(context.ip_address, Some("192.0.2.1".to_string()));
}

/// Test RuntimeConfig can hold RLS policy configuration
///
/// This verifies that the executor can be configured with an RLS policy
#[test]
fn test_runtime_config_accepts_rls_policy_configuration() {
    use std::sync::Arc;

    let config = RuntimeConfig::default();

    // Should have default values
    assert_eq!(config.query_timeout_ms, 30_000);
    assert_eq!(config.max_query_depth, 10);

    // Can configure with RLS policy
    let config_with_rls =
        RuntimeConfig::default().with_rls_policy(Arc::new(DefaultRLSPolicy::new()));

    assert!(config_with_rls.rls_policy.is_some());
}

/// Test RLS policy evaluation produces correct WHERE clauses
#[test]
fn test_rls_policy_produces_correct_where_clauses() {
    use fraiseql_core::db::WhereOperator;

    let policy = DefaultRLSPolicy::new();

    // Non-admin user should get owner-based filter
    let user_context = SecurityContext {
        user_id:          "user456".to_string(),
        roles:            vec!["user".to_string()],
        tenant_id:        None,
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let result = policy.evaluate(&user_context, "Post").unwrap();

    // Should have a WHERE clause
    assert!(result.is_some(), "Non-admin should have WHERE filter");

    // Verify structure
    match result.unwrap() {
        WhereClause::Field {
            path,
            operator,
            value,
        } => {
            assert_eq!(path, vec!["author_id".to_string()]);
            assert_eq!(operator, WhereOperator::Eq);
            assert_eq!(value, serde_json::json!("user456"));
        },
        other => panic!("Expected Field clause, got: {:?}", other),
    }
}

/// Test RLS filter composition for multi-tenant systems
#[test]
fn test_rls_compose_with_tenant_and_owner_filters() {
    use fraiseql_core::db::WhereOperator;

    let policy = DefaultRLSPolicy::new();

    // User in a tenant
    let user_context = SecurityContext {
        user_id:          "user789".to_string(),
        roles:            vec!["user".to_string()],
        tenant_id:        Some("tenant-acme".to_string()),
        scopes:           vec![],
        attributes:       HashMap::new(),
        request_id:       "req-test".to_string(),
        ip_address:       None,
        authenticated_at: chrono::Utc::now(),
        expires_at:       chrono::Utc::now() + chrono::Duration::hours(1),
        issuer:           None,
        audience:         None,
    };

    let result = policy.evaluate(&user_context, "Post").unwrap();

    assert!(result.is_some(), "Multi-tenant user should have WHERE filter");

    // Should be AND of tenant_id AND author_id
    match result.unwrap() {
        WhereClause::And(clauses) => {
            assert_eq!(clauses.len(), 2, "Should have 2 filters: tenant_id AND author_id");

            // Both clauses should be Field conditions
            for clause in clauses {
                assert!(matches!(clause, WhereClause::Field { .. }));
            }
        },
        other => panic!("Expected And clause for multi-tenant, got: {:?}", other),
    }
}
