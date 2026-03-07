//! SR-2: AA1 — Tenant ID was interpolated into SQL via format!(),
//!       enabling cross-tenant access via SQL injection.
//!       Fix: parameterized WHERE clauses (`$N` / `?`) never embed the
//!       tenant ID in the SQL string; direct interpolation validates the ID
//!       with an allowlist and panics on violation (fail-fast protection).
//!
//! **Execution engine:** none (pure SQL generation)
//! **Infrastructure:** none
//! **Parallelism:** safe

use fraiseql_core::tenancy::{TenantContext, where_clause_parameterized, where_clause_postgresql};

/// Inputs that would be dangerous if interpolated verbatim into SQL.
const MALICIOUS_TENANT_IDS: &[&str] = &[
    "'; DROP TABLE users; --",
    "1 OR 1=1",
    "1; SELECT * FROM secrets",
    "tenant' UNION SELECT password FROM admins --",
    "admin'--",
    "0'; DELETE FROM roles; --",
];

// ============================================================================
// SR-2a — Parameterized WHERE clauses must never embed raw tenant ID
// ============================================================================

/// `where_clause_postgresql()` must return a positional placeholder, never the
/// tenant ID itself. This is the primary AA1 regression guard.
#[test]
fn postgresql_where_clause_never_contains_raw_tenant_id() {
    for &tenant_id in MALICIOUS_TENANT_IDS {
        let ctx = TenantContext::new(tenant_id);

        // The parameterized clause must be a pure placeholder.
        let sql = ctx.where_clause_postgresql(1);

        assert!(
            !sql.contains(tenant_id),
            "AA1 regression: tenant_id `{tenant_id}` appeared raw in postgresql WHERE clause: `{sql}`"
        );
        assert_eq!(
            sql, "tenant_id = $1",
            "AA1 regression: postgresql WHERE clause must be `tenant_id = $1`, got: `{sql}`"
        );
    }
}

/// `where_clause_parameterized()` (MySQL/SQLite style) must return `?`,
/// never the tenant ID.
#[test]
fn parameterized_where_clause_never_contains_raw_tenant_id() {
    for &tenant_id in MALICIOUS_TENANT_IDS {
        let ctx = TenantContext::new(tenant_id);

        let sql = ctx.where_clause_parameterized();

        assert!(
            !sql.contains(tenant_id),
            "AA1 regression: tenant_id `{tenant_id}` appeared raw in parameterized WHERE clause: `{sql}`"
        );
        assert_eq!(
            sql, "tenant_id = ?",
            "AA1 regression: parameterized WHERE clause must be `tenant_id = ?`, got: `{sql}`"
        );
    }
}

/// Module-level `where_clause_postgresql()` helper must also return a placeholder.
#[test]
fn module_level_where_clause_postgresql_returns_placeholder() {
    let sql = where_clause_postgresql(1);
    assert_eq!(sql, "tenant_id = $1");

    let sql2 = where_clause_postgresql(5);
    assert_eq!(sql2, "tenant_id = $5");
}

/// Module-level `where_clause_parameterized()` helper must return `?`.
#[test]
fn module_level_where_clause_parameterized_returns_question_mark() {
    let sql = where_clause_parameterized();
    assert_eq!(sql, "tenant_id = ?");
}

// ============================================================================
// SR-2b — Direct interpolation path has allowlist validation (fail-fast)
// ============================================================================

/// `where_clause()` must panic when the tenant ID contains SQL metacharacters.
///
/// This is the fail-fast safety net: if production code ever calls the unsafe
/// interpolation helper with an externally-supplied ID, it panics immediately
/// rather than silently producing injectable SQL.
#[test]
#[should_panic(expected = "unsafe for SQL interpolation")]
fn where_clause_panics_for_sql_injection_payload() {
    let ctx = TenantContext::new("'; DROP TABLE users; --");
    let _ = ctx.where_clause(); // must panic
}

/// `where_clause()` must panic for IDs containing spaces (another injection vector).
#[test]
#[should_panic(expected = "unsafe for SQL interpolation")]
fn where_clause_panics_for_space_in_tenant_id() {
    let ctx = TenantContext::new("tenant id with spaces");
    let _ = ctx.where_clause(); // must panic
}

/// `where_clause()` must succeed for IDs using only the safe allowlist characters.
/// This verifies the allowlist check doesn't block legitimate tenant IDs.
#[test]
fn where_clause_accepts_safe_tenant_ids() {
    let safe_ids = ["acme-corp", "company_123", "tenant.prod", "a1b2c3"];

    for &id in &safe_ids {
        let ctx = TenantContext::new(id);
        let sql = ctx.where_clause();

        // The ID is safe to interpolate, so it appears in the clause.
        assert!(
            sql.contains(id),
            "Safe tenant_id `{id}` should appear in the WHERE clause; got: `{sql}`"
        );
    }
}
