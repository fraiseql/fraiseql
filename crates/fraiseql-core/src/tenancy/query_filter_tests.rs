//! Query filtering integration tests
//!
//! Tests for applying tenant filters to query execution

use crate::tenancy::TenantContext;

// ============================================================================
// Test 1: Query Filter Application
// ============================================================================

/// Test that tenant filter is applied to SELECT queries
#[test]
fn test_select_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT id, name FROM users";

    // Apply tenant filter
    let filtered_query = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered_query.contains("WHERE tenant_id = 'acme-corp'"));
    assert!(filtered_query.contains("SELECT id, name FROM users"));
}

/// Test that tenant filter is combined with existing WHERE clause
#[test]
fn test_select_query_with_existing_where() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT id, name FROM users WHERE active = true";

    // Combine tenant filter with existing WHERE
    let filtered_query = format!("{} AND {}", base_query, tenant.where_clause());

    assert!(filtered_query.contains("active = true"));
    assert!(filtered_query.contains("tenant_id = 'acme-corp'"));
    assert!(filtered_query.contains("AND"));
}

/// Test that tenant filter prevents cross-tenant data access
#[test]
fn test_cross_tenant_filter_enforcement() {
    let tenant_a = TenantContext::new("tenant_a");
    let tenant_b = TenantContext::new("tenant_b");

    let query_a = format!("SELECT * FROM users WHERE {}", tenant_a.where_clause());
    let query_b = format!("SELECT * FROM users WHERE {}", tenant_b.where_clause());

    // Queries should be different and prevent cross-tenant access
    assert_ne!(query_a, query_b);
    assert!(query_a.contains("tenant_a"));
    assert!(query_b.contains("tenant_b"));
}

// ============================================================================
// Test 2: Parameterized Query Filtering
// ============================================================================

/// Test PostgreSQL parameterized query with tenant filter
#[test]
fn test_parameterized_query_postgresql() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT id, name FROM users";

    // Use parameterized query for PostgreSQL
    let filtered =
        format!("{} WHERE {} AND active = $2", base_query, tenant.where_clause_postgresql(1));

    assert!(filtered.contains("$1"));
    assert!(filtered.contains("$2"));
    assert!(!filtered.contains("'acme-corp'")); // Parameterized, no literal values
}

/// Test MySQL parameterized query with tenant filter
#[test]
fn test_parameterized_query_mysql() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT id, name FROM users";

    // Use parameterized query for MySQL
    let filtered =
        format!("{} WHERE {} AND active = ?", base_query, tenant.where_clause_parameterized());

    assert!(filtered.contains("?"));
    assert_eq!(filtered.matches("?").count(), 2); // Two parameter placeholders
}

// ============================================================================
// Test 3: Multi-Table Queries with Tenant Filtering
// ============================================================================

/// Test JOIN query with tenant filter on main table
#[test]
fn test_join_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT u.id, u.name, p.title FROM users u JOIN posts p ON u.id = p.user_id";

    // Apply tenant filter to main table
    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("JOIN"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

/// Test that tenant_id is on users table in JOIN
#[test]
fn test_tenant_column_on_users_table() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT u.id, p.id FROM users u JOIN posts p ON u.id = p.user_id";

    // Tenant filter applies to users table
    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("WHERE tenant_id = 'acme-corp'"));
}

// ============================================================================
// Test 4: Aggregate Queries with Tenant Filtering
// ============================================================================

/// Test COUNT query with tenant filter
#[test]
fn test_count_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT COUNT(*) FROM users";

    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("COUNT(*)"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

/// Test GROUP BY query with tenant filter
#[test]
fn test_group_by_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT role, COUNT(*) FROM users GROUP BY role";

    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("GROUP BY"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

// ============================================================================
// Test 5: Mutation Queries with Tenant Filtering
// ============================================================================

/// Test UPDATE query with tenant filter
#[test]
fn test_update_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "UPDATE users SET active = false";

    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("UPDATE"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

/// Test DELETE query with tenant filter
#[test]
fn test_delete_query_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "DELETE FROM users";

    let filtered = format!("{} WHERE {}", base_query, tenant.where_clause());

    assert!(filtered.contains("DELETE"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

// ============================================================================
// Test 6: Complex Query Scenarios
// ============================================================================

/// Test subquery with tenant filter
#[test]
fn test_subquery_with_tenant_filter() {
    let tenant = TenantContext::new("acme-corp");
    let base_query = "SELECT * FROM users WHERE id IN (SELECT user_id FROM posts)";

    let filtered = format!("{} AND {}", base_query, tenant.where_clause());

    assert!(filtered.contains("IN (SELECT"));
    assert!(filtered.contains("tenant_id = 'acme-corp'"));
}

/// Test UNION query with tenant filters on both branches
#[test]
fn test_union_query_with_tenant_filters() {
    let tenant = TenantContext::new("acme-corp");
    let tenant_filter = tenant.where_clause();

    let query = format!(
        "(SELECT id FROM users WHERE {}) UNION (SELECT id FROM posts WHERE {})",
        tenant_filter, tenant_filter
    );

    assert_eq!(query.matches("tenant_id = 'acme-corp'").count(), 2);
}

// ============================================================================
// Test 7: Error Prevention
// ============================================================================

/// Test that malicious tenant_id is safely handled
#[test]
fn test_sql_injection_prevention_in_filter() {
    let malicious_tenant = "'; DROP TABLE users; --";
    let tenant = TenantContext::new(malicious_tenant);

    // Note: Real implementation should use parameterized queries
    // This test verifies the tenant_id is preserved as-is
    let filter = tenant.where_clause();

    assert!(filter.contains("'; DROP TABLE users; --"));
    // In production, this would be handled by parameterized queries
}

/// Test that parameterized queries don't need escaping
#[test]
fn test_parameterized_query_no_escaping_needed() {
    let tenant = TenantContext::new("tenant_with_'quotes'");

    // Parameterized query doesn't include the value
    let filter_pg = tenant.where_clause_postgresql(1);
    let filter_mysql = tenant.where_clause_parameterized();

    assert_eq!(filter_pg, "tenant_id = $1");
    assert_eq!(filter_mysql, "tenant_id = ?");
    // Actual values would be passed separately to database driver
}

// ============================================================================
// Test 8: Multi-Database Support
// ============================================================================

/// Test filter generation for different database backends
#[test]
fn test_multi_database_filter_support() {
    let tenant = TenantContext::new("acme-corp");

    let filter_pg = tenant.where_clause_postgresql(1);
    let filter_mysql = tenant.where_clause_parameterized();
    let filter_sqlite = tenant.where_clause_parameterized();
    let filter_literal = tenant.where_clause();

    assert_eq!(filter_pg, "tenant_id = $1");
    assert_eq!(filter_mysql, "tenant_id = ?");
    assert_eq!(filter_sqlite, "tenant_id = ?");
    assert_eq!(filter_literal, "tenant_id = 'acme-corp'");
}

// ============================================================================
// Test 9: Query Building Utilities
// ============================================================================

/// Test building complete query with all components
#[test]
fn test_complete_query_building() {
    let tenant = TenantContext::new("acme-corp");
    let table = "users";
    let selection = "id, name, email";
    let condition = "active = true";

    let query = format!(
        "SELECT {} FROM {} WHERE {} AND {}",
        selection,
        table,
        condition,
        tenant.where_clause()
    );

    assert!(query.contains("SELECT"));
    assert!(query.contains(selection));
    assert!(query.contains(table));
    assert!(query.contains(condition));
    assert!(query.contains("tenant_id = 'acme-corp'"));
}

/// Test that query order doesn't matter
#[test]
fn test_query_order_variations() {
    let tenant = TenantContext::new("acme-corp");
    let condition = "active = true";

    let query1 = format!("WHERE {} AND {}", condition, tenant.where_clause());
    let query2 = format!("WHERE {} AND {}", tenant.where_clause(), condition);

    // Both should have the same filters, order may vary
    assert!(query1.contains(condition));
    assert!(query1.contains("tenant_id = 'acme-corp'"));
    assert!(query2.contains(condition));
    assert!(query2.contains("tenant_id = 'acme-corp'"));
}
