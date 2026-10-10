//! The write capability: [`Writer`], one method, [`Writer::execute_write`] (ruling AA 6).

use async_trait::async_trait;
use fraiseql_error::Result;

use super::{ChangeLogWrite, DatabaseAdapter, MutationRowGate};

/// Whether a write commits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WriteMode {
    /// Commit once the gate admits the rows the function returned.
    Commit,
    /// Everything [`Commit`](Self::Commit) does — session variables, the change-log outbox
    /// row, the timing stamp, the gate — then roll back: a dry run exercises exactly the
    /// write it stands in for, and nothing persists.
    DryRun,
}

/// The rows `pg_tviews` reports a write's transaction changed, asked for before the commit
/// (#1391).
///
/// The adapter calls `tviews.pg_tviews_flush_and_report(max_entities, false, true)` after
/// the function, then reads each reported row of a type in `views` from that type's view,
/// on the write's own connection (so its session variables and row security apply), and
/// attaches the result to the first row under [`DERIVED_CASCADE_KEY`]:
///
/// `{"updated": [{"__typename", "id", "operation", "entity"}], "deleted": [{"__typename",
/// "id", "deletedAt"}], "truncated": bool, "invalidated_types": [...]}`
///
/// `entity` is `null` for a row the view did not return (not visible to this caller) or a
/// type `views` does not name. Only on a row that succeeded.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DerivedCascade<'a> {
    /// The most rows `pg_tviews` reports before truncating.
    pub max_entities: i32,
    /// GraphQL type name → the relation its rows are read from (`id`, `data`).
    pub views:        &'a [(&'a str, &'a str)],
}

impl<'a> DerivedCascade<'a> {
    /// Report at most `max_entities` rows, reading each from the view `views` names for its
    /// type.
    #[must_use]
    pub const fn new(max_entities: i32, views: &'a [(&'a str, &'a str)]) -> Self {
        Self {
            max_entities,
            views,
        }
    }
}

/// The key a [`DerivedCascade`] report is attached under, on the first returned row.
pub const DERIVED_CASCADE_KEY: &str = "__fraiseql_derived_cascade";

/// One call of a write function.
///
/// `#[non_exhaustive]`: build it with [`WriteRequest::new`] and the `with_*` builders, so a
/// field added later breaks no adapter and no caller.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct WriteRequest<'a> {
    /// The function to call.
    pub function:        &'a str,
    /// Its arguments, positionally.
    pub args:            &'a [serde_json::Value],
    /// Transaction-local settings applied before the call (`set_config(.., true)`).
    pub session_vars:    &'a [(&'a str, &'a str)],
    /// The change-log outbox row to write in the same transaction, if any.
    pub changelog:       Option<&'a ChangeLogWrite<'a>>,
    /// Whether the write commits.
    pub mode:            WriteMode,
    /// The `pg_tviews` report to attach before the gate, if any (#1391).
    pub derived_cascade: Option<&'a DerivedCascade<'a>>,
}

impl<'a> WriteRequest<'a> {
    /// A committing call of `function` with `args`, no session variables and no outbox row.
    #[must_use]
    pub const fn new(function: &'a str, args: &'a [serde_json::Value]) -> Self {
        Self {
            function,
            args,
            session_vars: &[],
            changelog: None,
            mode: WriteMode::Commit,
            derived_cascade: None,
        }
    }

    /// With the `pg_tviews` report attached before the gate (#1391).
    #[must_use]
    pub const fn with_derived_cascade(
        mut self,
        derived_cascade: Option<&'a DerivedCascade<'a>>,
    ) -> Self {
        self.derived_cascade = derived_cascade;
        self
    }

    /// With these transaction-local session variables.
    #[must_use]
    pub const fn with_session_vars(mut self, session_vars: &'a [(&'a str, &'a str)]) -> Self {
        self.session_vars = session_vars;
        self
    }

    /// With this change-log outbox row, written in the write's transaction.
    #[must_use]
    pub const fn with_changelog(mut self, changelog: Option<&'a ChangeLogWrite<'a>>) -> Self {
        self.changelog = changelog;
        self
    }

    /// In `mode`.
    #[must_use]
    pub const fn with_mode(mut self, mode: WriteMode) -> Self {
        self.mode = mode;
        self
    }
}

/// A database adapter that writes.
///
/// Implementing [`execute_write`](Self::execute_write) **is** the capability: there is no
/// flag to set beside it and no default to override, so an adapter cannot be write-capable
/// to the type system and read-only at run time, or the other way round. A write-capable
/// `Executor` is constructed only from a `Writer` (`Executor::new`, `Executor::with_config`);
/// `Executor::read_only` takes any adapter.
///
/// # Which adapters implement this?
///
/// | Adapter | Implements |
/// |---------|-----------|
/// | `PostgresAdapter` | ✅ Yes |
/// | `FraiseWireAdapter` | ❌ No — read-only wire protocol |
/// | `CachedDatabaseAdapter<A>` | ✅ When `A: Writer` |
///
/// # Contract
///
/// In ONE transaction: apply `session_vars` transaction-locally, run the function (and write
/// the change-log outbox row when `changelog` is `Some`), call `gate` on the rows it returned
/// **before** the transaction ends, then roll back and return the gate's error verbatim if it
/// errs; otherwise commit in [`WriteMode::Commit`] and roll back in [`WriteMode::DryRun`], and
/// return the rows. An adapter that cannot hold a transaction open for the gate must not
/// implement this trait: it stays read-only.
// async_trait: dyn-dispatch required; remove when RTN + Send is stable (RFC 3425)
#[async_trait]
pub trait Writer: DatabaseAdapter {
    /// Run `request` under `gate` (see the trait's contract).
    ///
    /// # Errors
    ///
    /// The gate's error, verbatim, after the rollback; a database error from any step.
    async fn execute_write(
        &self,
        request: &WriteRequest<'_>,
        gate: MutationRowGate<'_>,
    ) -> Result<Vec<std::collections::HashMap<String, serde_json::Value>>>;

    /// Bump the version counters of `tables` after a committed write that changed them, so
    /// cached aggregates over them are invalidated by version key.
    ///
    /// Provided: nothing to bump for an adapter with no aggregate cache. The cache wrapper
    /// overrides it, bumping through [`execute_write`](Self::execute_write).
    ///
    /// # Errors
    ///
    /// A database error from the bump.
    async fn bump_fact_table_versions(&self, _tables: &[String]) -> Result<()> {
        Ok(())
    }
}
