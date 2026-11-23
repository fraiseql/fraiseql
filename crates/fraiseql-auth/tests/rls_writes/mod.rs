//! The auth tables' writes are the owner's alone.
//!
//! Their `INSERT` policies are `WITH CHECK (true)`: row-level security does not restrict
//! who writes, the grants do. No table grants `PUBLIC` or any role but its owner a write,
//! so a role that is not the owner, even one granted `SELECT`, is refused every write for
//! want of a privilege.

use sqlx::PgPool;

/// Assert `pool`'s role is refused `INSERT` and `UPDATE` on `table` with
/// `insufficient_privilege` (42501), with `fraiseql.tenant_id` set to `tenant`, the tenant
/// of a row the role reads: the policies let that row through, so the refusal is the
/// missing grant, not row-level security hiding the row.
pub async fn are_refused(pool: &PgPool, table: &str, tenant: &str) {
    for statement in [
        format!("INSERT INTO {table} DEFAULT VALUES"),
        format!("UPDATE {table} SET tenant_id = tenant_id"),
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('fraiseql.tenant_id', $1, true)")
            .bind(tenant)
            .execute(&mut *tx)
            .await
            .unwrap();
        // Reason: test SQL built from a table name the calling suite defines.
        let result = sqlx::query(sqlx::AssertSqlSafe(statement.clone())).execute(&mut *tx).await;
        let code = result
            .as_ref()
            .err()
            .and_then(|e| e.as_database_error())
            .and_then(|e| e.code())
            .map(|code| code.into_owned());
        assert_eq!(
            code.as_deref(),
            Some("42501"),
            "`{statement}` by a role granted only SELECT must be refused for want of a \
             privilege: {result:?}"
        );
        tx.rollback().await.unwrap();
    }
}
