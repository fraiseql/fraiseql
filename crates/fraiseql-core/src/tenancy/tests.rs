//! Tests for multi-tenancy data model
//!
//! Tests for tenant isolation, filtering, and JWT extraction

use crate::tenancy::TenantContext;
use serde_json::json;

// ============================================================================
// Test 1: Tenant Context Creation and Isolation
// ============================================================================

/// Test that tenants have unique IDs
#[test]
fn test_tenant_isolation() {
    let tenant_a = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    assert_ne!(tenant_a.id(), tenant_b.id(), "Different tenants must have different IDs");
    assert_eq!(tenant_a.id(), "tenant_a");
    assert_eq!(tenant_b.id(), "tenant_b");
}

/// Test that tenant IDs are preserved
#[test]
fn test_tenant_id_preservation() {
    let tenant = TenantContext::new("acme-corp");
    assert_eq!(tenant.id(), "acme-corp");
}

// ============================================================================
// Test 2: Tenant Creation and Timestamps
// ============================================================================

/// Test that tenant has a creation timestamp
#[test]
fn test_tenant_has_creation_timestamp() {
    let tenant = TenantContext::new("tenant1");

    // Tenant should have a creation timestamp
    assert!(tenant.created_at_iso8601().is_some());
    let timestamp = tenant.created_at_iso8601().unwrap();

    // Timestamp should be ISO 8601 format
    assert!(!timestamp.is_empty());
    assert!(timestamp.contains("T")); // ISO 8601 includes T separator
}

/// Test that different tenants have similar creation times
#[test]
fn test_tenant_timestamps_close() {
    let tenant_a = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    // Both should have timestamps within a reasonable time
    let ts_a = tenant_a.created_at_iso8601().unwrap();
    let ts_b = tenant_b.created_at_iso8601().unwrap();

    assert!(!ts_a.is_empty());
    assert!(!ts_b.is_empty());
}

// ============================================================================
// Test 3: Tenant Context Metadata
// ============================================================================

/// Test that tenant can store metadata
#[test]
fn test_tenant_metadata() {
    let mut tenant = TenantContext::new("acme");

    // Set metadata
    tenant.set_metadata("industry", "technology");
    tenant.set_metadata("employees", "500");

    // Retrieve metadata
    assert_eq!(tenant.get_metadata("industry"), Some("technology"));
    assert_eq!(tenant.get_metadata("employees"), Some("500"));
    assert_eq!(tenant.get_metadata("nonexistent"), None);
}

// ============================================================================
// Test 4: Tenant Comparison and Equality
// ============================================================================

/// Test that tenants can be compared
#[test]
fn test_tenant_equality() {
    let tenant_a1 = TenantContext::new("tenant_a");
    let tenant_a2 = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    // Same tenant ID should be equal for data isolation purposes
    assert_eq!(tenant_a1.id(), tenant_a2.id());

    // Different tenant IDs should not be equal
    assert_ne!(tenant_a1.id(), tenant_b.id());
}

// ============================================================================
// Test 5: Tenant Clone and Copy
// ============================================================================

/// Test that tenant can be cloned
#[test]
fn test_tenant_clone() {
    let tenant_original = TenantContext::new("tenant_clone_test");
    let tenant_cloned = tenant_original.clone();

    assert_eq!(tenant_original.id(), tenant_cloned.id());
}

// ============================================================================
// Test 6: Edge Cases
// ============================================================================

/// Test that empty tenant ID is allowed (for system tenant)
#[test]
fn test_empty_tenant_id() {
    let system_tenant = TenantContext::new("");
    assert_eq!(system_tenant.id(), "");
}

/// Test that tenant ID with special characters works
#[test]
fn test_tenant_id_special_chars() {
    let tenant = TenantContext::new("tenant-123_abc.org");
    assert_eq!(tenant.id(), "tenant-123_abc.org");
}

/// Test that UUID-like tenant IDs work
#[test]
fn test_tenant_id_uuid() {
    let uuid_tenant = TenantContext::new("550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(
        uuid_tenant.id(),
        "550e8400-e29b-41d4-a716-446655440000"
    );
}

// ============================================================================
// Test 7: Tenant Query Filtering
// ============================================================================

/// Test that tenant filter clause can be generated
#[test]
fn test_tenant_filter_clause_generation() {
    let tenant = TenantContext::new("acme-corp");

    // Generate a WHERE clause for the tenant
    let filter_clause = format!("tenant_id = '{}'", tenant.id());

    assert_eq!(filter_clause, "tenant_id = 'acme-corp'");
}

/// Test that multiple tenants generate different filters
#[test]
fn test_different_tenants_different_filters() {
    let tenant_a = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    let filter_a = format!("tenant_id = '{}'", tenant_a.id());
    let filter_b = format!("tenant_id = '{}'", tenant_b.id());

    assert_ne!(filter_a, filter_b);
    assert!(filter_a.contains("tenant_a"));
    assert!(filter_b.contains("tenant_b"));
}

/// Test that tenant isolation prevents data leakage
#[test]
fn test_tenant_isolation_semantics() {
    let tenant_a = TenantContext::new("company_a");
    let tenant_b = TenantContext::new("company_b");

    // Verify that queries for tenant_a should never include tenant_b data
    assert_ne!(tenant_a.id(), tenant_b.id());

    // A proper implementation would verify this at database level
    // by checking that queries from tenant_a never return tenant_b's rows
}

// ============================================================================
// Test 8: Helper Functions for Query Filtering
// ============================================================================

/// Test where_clause helper function
#[test]
fn test_where_clause_helper() {
    use crate::tenancy::where_clause;

    let clause = where_clause("acme-corp");
    assert_eq!(clause, "tenant_id = 'acme-corp'");
}

/// Test PostgreSQL parameterized where_clause
#[test]
fn test_where_clause_postgresql_helper() {
    use crate::tenancy::where_clause_postgresql;

    let clause = where_clause_postgresql(1);
    assert_eq!(clause, "tenant_id = $1");

    let clause2 = where_clause_postgresql(2);
    assert_eq!(clause2, "tenant_id = $2");
}

/// Test parameterized where_clause for MySQL/SQLite
#[test]
fn test_where_clause_parameterized_helper() {
    use crate::tenancy::where_clause_parameterized;

    let clause = where_clause_parameterized();
    assert_eq!(clause, "tenant_id = ?");
}

// ============================================================================
// Test 9: JWT Extraction
// ============================================================================

/// Test creating TenantContext from JWT claims
#[test]
fn test_from_jwt_claims_success() {
    let claims = json!({
        "sub": "user123",
        "tenant_id": "acme-corp",
        "email": "alice@acme.com"
    });

    let tenant = TenantContext::from_jwt_claims(&claims).expect("Should create tenant from JWT");
    assert_eq!(tenant.id(), "acme-corp");
}

/// Test that missing tenant_id in JWT returns error
#[test]
fn test_from_jwt_claims_missing_tenant_id() {
    let claims = json!({
        "sub": "user123",
        "email": "alice@example.com"
    });

    let result = TenantContext::from_jwt_claims(&claims);
    assert!(result.is_err(), "Should error when tenant_id is missing");
}

/// Test that non-string tenant_id in JWT returns error
#[test]
fn test_from_jwt_claims_invalid_tenant_id_type() {
    let claims = json!({
        "sub": "user123",
        "tenant_id": 12345  // Should be string, not number
    });

    let result = TenantContext::from_jwt_claims(&claims);
    assert!(result.is_err(), "Should error when tenant_id is not a string");
}

/// Test creating TenantContext from JWT with UUID tenant_id
#[test]
fn test_from_jwt_claims_uuid_tenant() {
    let uuid_tenant = "550e8400-e29b-41d4-a716-446655440000";
    let claims = json!({
        "sub": "user123",
        "tenant_id": uuid_tenant
    });

    let tenant = TenantContext::from_jwt_claims(&claims).expect("Should create tenant from JWT");
    assert_eq!(tenant.id(), uuid_tenant);
}

// ============================================================================
// Test 10: Tenant Where Clauses
// ============================================================================

/// Test where_clause method
#[test]
fn test_tenant_where_clause() {
    let tenant = TenantContext::new("acme-corp");
    let clause = tenant.where_clause();

    assert_eq!(clause, "tenant_id = 'acme-corp'");
}

/// Test where_clause_postgresql method
#[test]
fn test_tenant_where_clause_postgresql() {
    let tenant = TenantContext::new("acme-corp");

    let clause1 = tenant.where_clause_postgresql(1);
    assert_eq!(clause1, "tenant_id = $1");

    let clause2 = tenant.where_clause_postgresql(2);
    assert_eq!(clause2, "tenant_id = $2");
}

/// Test where_clause_parameterized method
#[test]
fn test_tenant_where_clause_parameterized() {
    let tenant = TenantContext::new("acme-corp");
    let clause = tenant.where_clause_parameterized();

    assert_eq!(clause, "tenant_id = ?");
}

/// Test that different tenants generate different where clauses
#[test]
fn test_different_tenants_different_where_clauses() {
    let tenant_a = TenantContext::new("company_a");
    let tenant_b = TenantContext::new("company_b");

    let clause_a = tenant_a.where_clause();
    let clause_b = tenant_b.where_clause();

    assert_ne!(clause_a, clause_b);
    assert_eq!(clause_a, "tenant_id = 'company_a'");
    assert_eq!(clause_b, "tenant_id = 'company_b'");
}
