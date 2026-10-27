//! The write capability: [`Writer`], one method, [`Writer::execute_write`] (ruling AA 6), and
//! its batch form, [`Writer::execute_writes`] (#1574).

use async_trait::async_trait;
use fraiseql_error::{FraiseQLError, Result};

use super::{BatchRowGate, ChangeLogWrite, DatabaseAdapter, MutationRowGate};

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
/// the function, then reads the reported rows of each type in `views` from that type's view,
/// one statement per type (`WHERE id = ANY(…)`), on the write's own connection (so its session
/// variables and row security apply), and attaches the result to the first row under
/// [`DERIVED_CASCADE_KEY`]:
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
    /// GraphQL type name → the relation its rows are read from (`id`, `data`). A type left
    /// out is not read: its rows come back with `entity: null`.
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

/// The key the report of the rows a write changed is attached under, on the first returned
/// row, when its request asked for one ([`WriteRequest::with_changes_reported`], #1576).
///
/// `pg_tviews`' report, as `tviews.pg_tviews_flush_and_report(max_entities, false, true)`
/// returns it after the function: `{"updated": [{"__typename", "id"}], "deleted": [...],
/// "invalidated_types": [...], "collections": {"Comment.fk_post": [...]}}` (`collections`
/// only for rows written with `pg_tviews.publish_changes = 'rows'`). No row is read. Read it
/// with [`ChangedRows::from_report`].
pub const CHANGED_ROWS_KEY: &str = "__fraiseql_changed_rows";

/// Parent ids, as text, by collection: `<child type>.<reference column>`, as `pg_tviews`
/// names it (`Comment.fk_post`).
pub type ChangedCollections =
    std::collections::BTreeMap<String, std::collections::BTreeSet<String>>;

/// The TVIEW rows a committed write changed, by GraphQL type and public id, as `pg_tviews`
/// reported them (#1576). Cascaded rows are included: `pg_tviews` journals every row it
/// refreshes.
///
/// The default is a write that changed nothing: no row, no type, no collection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChangedRows {
    /// Rows inserted, updated or deleted: public ids, as text, by type.
    pub rows:        std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    /// Types whose rows are not all listed (the report was truncated): every row of each may
    /// have changed, and every collection of each (#1603).
    pub types:       std::collections::BTreeSet<String>,
    /// The collections whose members changed: the parents whose child lists the rows left,
    /// joined or changed in (#1603). `None` when the report captured none (the rows were
    /// written without `pg_tviews.publish_changes = 'rows'`): which changed is then unknown.
    pub collections: Option<ChangedCollections>,
}

impl Default for ChangedRows {
    fn default() -> Self {
        Self {
            rows:        std::collections::BTreeMap::new(),
            types:       std::collections::BTreeSet::new(),
            collections: Some(ChangedCollections::new()),
        }
    }
}

impl ChangedRows {
    /// Read a `pg_tviews` report (see [`CHANGED_ROWS_KEY`]). `None` when `report` is not one:
    /// what changed is then unknown. An entry whose id is neither text nor a number names
    /// its type as wholly changed, and a collection's parent likewise its child type. A
    /// report without `collections`, or with a key that is not `<type>.<column>`, captured
    /// none ([`collections`](Self::collections) is `None`).
    #[must_use]
    pub fn from_report(report: &serde_json::Value) -> Option<Self> {
        let report = report.as_object()?;
        let mut changed = Self {
            collections: None,
            ..Self::default()
        };
        for arm in ["updated", "deleted"] {
            let entries = match report.get(arm) {
                None => continue,
                Some(entries) => entries.as_array()?,
            };
            for entry in entries {
                let typename = entry.get("__typename")?.as_str()?;
                match entry.get("id") {
                    Some(serde_json::Value::String(id)) => {
                        changed.rows.entry(typename.to_string()).or_default().insert(id.clone());
                    },
                    Some(serde_json::Value::Number(id)) => {
                        changed
                            .rows
                            .entry(typename.to_string())
                            .or_default()
                            .insert(id.to_string());
                    },
                    _ => {
                        changed.types.insert(typename.to_string());
                    },
                }
            }
        }
        if let Some(types) = report.get("invalidated_types") {
            for typename in types.as_array()? {
                changed.types.insert(typename.as_str()?.to_string());
            }
        }
        if let Some(collections) = report.get("collections") {
            let mut read = ChangedCollections::new();
            let mut readable = true;
            for (key, parents) in collections.as_object()? {
                let Some((child_type, _)) = key
                    .split_once('.')
                    .filter(|(child_type, column)| !child_type.is_empty() && !column.is_empty())
                else {
                    readable = false;
                    continue;
                };
                for parent in parents.as_array()? {
                    match parent {
                        serde_json::Value::String(id) => {
                            read.entry(key.clone()).or_default().insert(id.clone());
                        },
                        serde_json::Value::Number(id) => {
                            read.entry(key.clone()).or_default().insert(id.to_string());
                        },
                        _ => {
                            changed.types.insert(child_type.to_string());
                        },
                    }
                }
            }
            changed.collections = readable.then_some(read);
        }
        Some(changed)
    }

    /// Also the rows, types and collections of `other`, committed in the same transaction.
    /// When either captured no collections, which changed is unknown.
    pub fn absorb(&mut self, other: Self) {
        for (typename, ids) in other.rows {
            self.rows.entry(typename).or_default().extend(ids);
        }
        self.types.extend(other.types);
        match (self.collections.as_mut(), other.collections) {
            (Some(mine), Some(theirs)) => {
                for (key, parents) in theirs {
                    mine.entry(key).or_default().extend(parents);
                }
            },
            _ => self.collections = None,
        }
    }

    /// Every type named, by a row or wholly.
    pub fn type_names(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().chain(&self.types).map(String::as_str)
    }
}

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
    /// Report the rows the call changed, at most this many (#1576): see
    /// [`CHANGED_ROWS_KEY`]. A call with a [`DerivedCascade`] carries its report there
    /// instead.
    pub report_changes:  Option<i32>,
    /// Where the response row's documents hold a `Decimal`, read as its digits; `None`:
    /// decoded as JSON numbers.
    pub decimals:        Option<&'a super::WriteDecimals>,
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
            report_changes: None,
            decimals: None,
        }
    }

    /// Reading the response row's `Decimal`s as `decimals` places them.
    #[must_use]
    pub const fn with_decimals(mut self, decimals: Option<&'a super::WriteDecimals>) -> Self {
        self.decimals = decimals;
        self
    }

    /// Reporting the rows the call changes, at most `max_entities` (#1576).
    #[must_use]
    pub const fn with_changes_reported(mut self, max_entities: Option<i32>) -> Self {
        self.report_changes = max_entities;
        self
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

    /// Run every request in `requests` in **one** transaction, under `gate` (#1574).
    ///
    /// The contract of [`execute_write`](Self::execute_write), over several calls: in one
    /// transaction, each call in order with its own `session_vars`, outbox row and derived
    /// cascade, `gate` called with each call's index and rows before the transaction ends.
    /// The first database error or gate error rolls everything back and is returned: a
    /// database error names its call ([`FraiseQLError::in_batch_item`]), a gate error is
    /// returned verbatim. Otherwise every call commits, or none does: the requests must
    /// share one [`WriteMode`]. Returns each call's rows, in order.
    ///
    /// Provided: refuses. An adapter that cannot hold several calls in one transaction must
    /// not fall back to one transaction per call, which would commit a batch in part.
    ///
    /// # Errors
    ///
    /// [`FraiseQLError::Unsupported`] unless the adapter overrides it; otherwise as above.
    async fn execute_writes(
        &self,
        _requests: &[WriteRequest<'_>],
        _gate: BatchRowGate<'_>,
    ) -> Result<Vec<Vec<std::collections::HashMap<String, serde_json::Value>>>> {
        Err(FraiseQLError::Unsupported {
            message: "this database adapter cannot run a batch of writes in one transaction"
                .to_string(),
        })
    }

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
