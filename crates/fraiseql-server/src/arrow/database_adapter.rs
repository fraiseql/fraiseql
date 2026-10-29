//! Database adapter wrapper for Arrow Flight service.
//!
//! This module provides a wrapper that adapts fraiseql-core's database adapters
//! to fraiseql-arrow's `DatabaseAdapter` trait, enabling the Arrow Flight service
//! to execute queries against real databases, through `PostgresAdapter`.

use std::sync::Arc;

use async_trait::async_trait;
#[cfg(feature = "arrow")]
use fraiseql_arrow::db::{ArrowDatabaseAdapter, DatabaseError, DescribedRows};
use fraiseql_core::db::postgres::PostgresAdapter;

/// Wrapper that adapts fraiseql-core's database adapters to fraiseql-arrow's `DatabaseAdapter`
/// trait.
///
/// This enables the Arrow Flight service to execute queries against the database without
/// requiring direct knowledge of fraiseql-core's `DatabaseAdapter` interface.
pub struct FlightDatabaseAdapter {
    /// Inner PostgreSQL adapter from fraiseql-core
    inner: Arc<PostgresAdapter>,
}

impl FlightDatabaseAdapter {
    /// Create a new Arrow Flight database adapter with PostgreSQL backend.
    ///
    /// # Arguments
    ///
    /// * `adapter` - PostgreSQL adapter from fraiseql-core
    #[must_use]
    pub fn new(adapter: PostgresAdapter) -> Self {
        Self {
            inner: Arc::new(adapter),
        }
    }

    /// Create a new Arrow Flight database adapter from an Arc (PostgreSQL).
    ///
    /// # Arguments
    ///
    /// * `adapter` - PostgreSQL adapter wrapped in Arc
    #[must_use]
    pub const fn from_arc(adapter: Arc<PostgresAdapter>) -> Self {
        Self { inner: adapter }
    }

    /// Get a reference to the inner PostgreSQL adapter.
    #[must_use]
    pub const fn inner(&self) -> &Arc<PostgresAdapter> {
        &self.inner
    }
}

#[cfg(feature = "arrow")]
// Reason: ArrowDatabaseAdapter is defined with #[async_trait]; all implementations must match
// its transformed method signatures to satisfy the trait contract
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
impl ArrowDatabaseAdapter for FlightDatabaseAdapter {
    /// Each column as PostgreSQL described it, and each `numeric` cell as its digits (#1766).
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError`] if the underlying PostgreSQL query fails.
    async fn execute_described_query(&self, sql: &str) -> Result<DescribedRows, DatabaseError> {
        self.inner
            .execute_described_query(sql)
            .await
            .map_err(|e: fraiseql_core::error::FraiseQLError| DatabaseError::new(e.to_string()))
    }

    /// Write the Upload's rows and their `core.tb_entity_change_log` outbox rows in
    /// one PostgreSQL transaction (#953).
    ///
    /// # Errors
    ///
    /// Returns [`DatabaseError`] if the transaction fails, in which case **neither**
    /// the rows nor the outbox rows were written.
    async fn execute_gated_upload(
        &self,
        upload: &fraiseql_arrow::db::GatedUpload<'_>,
    ) -> Result<u64, DatabaseError> {
        let sql = build_upload_outbox_cte(upload.insert_sql);

        let mut client = self
            .inner
            .pool()
            .get()
            .await
            .map_err(|e| DatabaseError::new(format!("Failed to acquire connection: {e}")))?;
        let tx = client
            .build_transaction()
            .start()
            .await
            .map_err(|e| DatabaseError::new(format!("Failed to begin transaction: {e}")))?;

        let row = tx
            .query_one(sql.as_str(), &[&upload.table, &upload.tenant_id, &upload.user_id])
            .await
            .map_err(|e| DatabaseError::new(format!("Upload failed: {e}")))?;
        let inserted: i64 = row.get("n");

        tx.commit()
            .await
            .map_err(|e| DatabaseError::new(format!("Failed to commit Upload: {e}")))?;

        Ok(inserted.unsigned_abs())
    }
}

/// Wrap `insert_sql` so it and its change-log outbox rows are one statement.
///
/// Mirrors the mutation executor's Change Spine CTE
/// (`build_changelog_cte_sql`): the data INSERT is a `RETURNING *` CTE, a
/// data-modifying CTE writes one outbox row per inserted row from it, and the
/// primary `SELECT` returns the count. One statement inside one transaction, so
/// the rows and the Change Spine cannot diverge.
///
/// Parameters are positional and fixed, so the text is deterministic per
/// `insert_sql`: `$1` = `object_type` (the table), `$2` = `tenant_id`, `$3` = the
/// Flight session subject.
///
/// `$2::text::uuid` rather than `$2::uuid`: a cast applied straight to a parameter
/// makes PostgreSQL infer the *parameter's* type as `uuid`, so the client is then
/// required to send uuid-encoded bytes and a bound `&str` fails with "error
/// serializing parameter". Going through `::text` pins the parameter as text and
/// performs the cast server-side.
///
/// `object_id` is the row's `id` **only when it is a UUID** — an Upload target need
/// not have one, and a `TEXT` key must not fail the whole write. The full row is in
/// `object_data` either way, so nothing is lost by a NULL here.
#[cfg(feature = "arrow")]
fn build_upload_outbox_cte(insert_sql: &str) -> String {
    format!(
        "WITH r AS ({insert_sql} RETURNING *), \
         _changelog AS ( \
           INSERT INTO core.tb_entity_change_log \
             (object_type, modification_type, object_id, object_data, tenant_id, \
              extra_metadata, commit_time) \
           SELECT \
             $1, 'INSERT', \
             CASE WHEN to_jsonb(r)->>'id' ~ \
                    '^[0-9a-fA-F]{{8}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{4}}-[0-9a-fA-F]{{12}}$' \
                  THEN (to_jsonb(r)->>'id')::uuid ELSE NULL END, \
             to_jsonb(r), $2::text::uuid, \
             jsonb_build_object('transport', 'flight', 'flight_user_id', $3::text), \
             clock_timestamp() \
           FROM r \
           RETURNING 1 \
         ) \
         SELECT count(*)::bigint AS n FROM r"
    )
}
