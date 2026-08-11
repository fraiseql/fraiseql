//! Durable per-bucket policies, and the precedence between them and the config
//! file (#974).
//!
//! A policy now has two sources of truth, so the rule between them is stated
//! once, here, and is deliberately the blunt one:
//!
//! > **A stored policy replaces the configured one wholesale. The two are never
//! > merged.**
//!
//! Merging is what makes a policy unreadable — an operator looking at
//! `[[storage.docs.policies]]` would have to also know what someone `PUT` last
//! Tuesday to answer "what can this caller do". Wholesale replacement keeps
//! #371's promise that a reader answers that question from **one** list, and
//! [`PolicySource`] is reported by the admin API and logged at boot so which
//! list is never a guess.
//!
//! Deleting a stored policy therefore hands the bucket back to its configured
//! policy — or, if it has none, to the coarse `access` mode. That can *widen*
//! access, which is why the delete is a write-token operation and answers with
//! the source that now governs.
//!
//! # The stored form is the operator's own text
//!
//! The table holds the rules as written ([`PolicyRuleSpec`] JSON), not the
//! parsed policy, so a load re-parses through exactly the door a write came
//! through. A row that cannot be parsed is a row someone edited in SQL (or one
//! written by a newer version), and [`StoredPolicyRow::parse`] reports it the
//! same way the request path does.

use chrono::{DateTime, Utc};
use fraiseql_error::{FileError, FraiseQLError};
use sqlx::PgPool;

use super::{BucketPolicy, PolicyRuleSpec, PolicySpecError, parse_policy};

fn db_err(e: sqlx::Error) -> FraiseQLError {
    FraiseQLError::File(FileError::Backend {
        message: e.to_string(),
        source:  Some(Box::new(e)),
    })
}

/// Which of a bucket's two possible policy sources is governing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicySource {
    /// `_fraiseql_storage_policies` — hot-reloadable, and wins over the config
    /// file whenever a row exists.
    Store,
    /// `[[storage.<name>.policies]]` — needs a config deploy to change.
    ConfigFile,
    /// No policy at all: the coarse `access` mode governs, as it did before
    /// #371.
    AccessMode,
}

impl PolicySource {
    /// The value reported in API responses and boot logs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::ConfigFile => "config_file",
            Self::AccessMode => "access_mode",
        }
    }
}

/// Which source governs a bucket, given whether each one has a policy for it.
///
/// The whole precedence rule, in one place: a stored policy wins, the
/// configured policy is the fallback, and with neither the `access` mode
/// governs. Both the boot log and the admin API resolve the source through
/// this, so the two cannot report different answers.
#[must_use]
pub const fn policy_source(stored: bool, configured: bool) -> PolicySource {
    match (stored, configured) {
        (true, _) => PolicySource::Store,
        (false, true) => PolicySource::ConfigFile,
        (false, false) => PolicySource::AccessMode,
    }
}

/// A row of `_fraiseql_storage_policies`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredPolicyRow {
    /// The bucket this policy governs.
    pub bucket:     String,
    /// The rule list as written, before validation. Held as raw JSON rather
    /// than as `Vec<PolicyRuleSpec>` so that a malformed row surfaces through
    /// [`parse`](Self::parse) — the same error the writer would have seen —
    /// instead of failing somewhere in the row decoder.
    pub rules:      serde_json::Value,
    /// When this policy was last written.
    pub updated_at: DateTime<Utc>,
}

impl StoredPolicyRow {
    /// Validate the stored rules into a runnable policy.
    ///
    /// # Errors
    ///
    /// [`PolicySpecError`] if the JSON is not a list of well-formed rules
    /// (`rule_index` is `None`, since a shape error is not attributable to one
    /// rule), or if a rule fails [`parse_policy`].
    pub fn parse(&self) -> Result<BucketPolicy, PolicySpecError> {
        let specs: Vec<PolicyRuleSpec> =
            serde_json::from_value(self.rules.clone()).map_err(|e| PolicySpecError {
                rule_index: None,
                message:    format!("stored rules are not a valid policy rule list: {e}"),
            })?;
        parse_policy(&specs)
    }
}

/// Durable per-bucket policies in PostgreSQL.
#[derive(Debug, Clone)]
pub struct StoragePolicyStore {
    pool: PgPool,
}

impl StoragePolicyStore {
    /// Wrap a connection pool.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The underlying pool, for tests that need to plant a row this API would
    /// refuse to write — the hand-edited-row case the boot path must catch.
    #[cfg(test)]
    pub(crate) const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Every stored policy, ordered by bucket so boot logs are stable.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn list(&self) -> Result<Vec<StoredPolicyRow>, FraiseQLError> {
        sqlx::query_as(
            "SELECT bucket, rules, updated_at FROM _fraiseql_storage_policies ORDER BY bucket",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)
    }

    /// One bucket's stored policy, or `None` when the config file (or the
    /// `access` mode) governs it.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn get(&self, bucket: &str) -> Result<Option<StoredPolicyRow>, FraiseQLError> {
        sqlx::query_as(
            "SELECT bucket, rules, updated_at FROM _fraiseql_storage_policies WHERE bucket = $1",
        )
        .bind(bucket)
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)
    }

    /// Write a bucket's policy, replacing any previous one.
    ///
    /// The caller is expected to have validated `specs` through
    /// [`parse_policy`] first — persisting an unparseable policy would create
    /// exactly the row that refuses the next boot.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` if the rules cannot be serialised or the
    /// write fails.
    pub async fn put(
        &self,
        bucket: &str,
        specs: &[PolicyRuleSpec],
    ) -> Result<StoredPolicyRow, FraiseQLError> {
        let rules = serde_json::to_value(specs).map_err(|e| {
            FraiseQLError::File(FileError::Backend {
                message: format!("policy rules could not be serialised: {e}"),
                source:  Some(Box::new(e)),
            })
        })?;
        sqlx::query_as(
            "INSERT INTO _fraiseql_storage_policies (bucket, rules, updated_at) \
             VALUES ($1, $2, now()) \
             ON CONFLICT (bucket) DO UPDATE SET rules = EXCLUDED.rules, updated_at = now() \
             RETURNING bucket, rules, updated_at",
        )
        .bind(bucket)
        .bind(&rules)
        .fetch_one(&self.pool)
        .await
        .map_err(db_err)
    }

    /// Drop a bucket's stored policy, handing it back to the config file.
    /// Returns whether a row existed.
    ///
    /// # Errors
    ///
    /// Returns `FraiseQLError::File` on database failure.
    pub async fn delete(&self, bucket: &str) -> Result<bool, FraiseQLError> {
        let result = sqlx::query("DELETE FROM _fraiseql_storage_policies WHERE bucket = $1")
            .bind(bucket)
            .execute(&self.pool)
            .await
            .map_err(db_err)?;
        Ok(result.rows_affected() > 0)
    }
}
