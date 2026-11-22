//! Observer repository for database operations.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery, MANAGED_BY_PROJECT_PREFIX,
    MANAGED_BY_SCHEMA, Observer, ObserverLog, ObserverScope, ObserverStats, UpdateObserverRequest,
    default_timeout,
};
use crate::ServerError;

/// Repository for observer CRUD operations.
#[derive(Clone)]
pub struct ObserverRepository {
    pool: PgPool,
}

impl ObserverRepository {
    /// Create a new observer repository.
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The pool the repository reads and writes through.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// List the observers `scope` can see, with optional filters.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn list(
        &self,
        query: &ListObserversQuery,
        scope: ObserverScope,
    ) -> Result<(Vec<Observer>, i64), ServerError> {
        let mut conn =
            self.pool.acquire().await.map_err(|e| ServerError::Database(e.to_string()))?;
        Self::list_on(&mut conn, query, scope).await
    }

    /// [`list`](Self::list) on `conn`: inside a transaction, it reads what the transaction
    /// has written.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn list_on(
        conn: &mut sqlx::PgConnection,
        query: &ListObserversQuery,
        scope: ObserverScope,
    ) -> Result<(Vec<Observer>, i64), ServerError> {
        let offset = (query.page - 1) * query.page_size;

        // --- count query ---
        let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer WHERE 1=1");

        if !query.include_deleted {
            count_qb.push(" AND deleted_at IS NULL");
        }
        if let Some(ref entity_type) = query.entity_type {
            count_qb.push(" AND entity_type = ").push_bind(entity_type);
        }
        if let Some(ref event_type) = query.event_type {
            count_qb.push(" AND event_type = ").push_bind(event_type);
        }
        if let Some(enabled) = query.enabled {
            count_qb.push(" AND enabled = ").push_bind(enabled);
        }
        scope.push_visible(&mut count_qb);

        let total_count: (i64,) = count_qb
            .build_query_as()
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        // --- select query ---
        let mut select_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r"SELECT pk_observer, id, name, description, entity_type, event_type,
              condition_expression, actions, enabled, priority, retry_config,
              timeout_ms, fk_customer_org, created_at, updated_at,
              created_by, updated_by, deleted_at, managed_by
              FROM core.tb_observer WHERE 1=1",
        );

        if !query.include_deleted {
            select_qb.push(" AND deleted_at IS NULL");
        }
        if let Some(ref entity_type) = query.entity_type {
            select_qb.push(" AND entity_type = ").push_bind(entity_type);
        }
        if let Some(ref event_type) = query.event_type {
            select_qb.push(" AND event_type = ").push_bind(event_type);
        }
        if let Some(enabled) = query.enabled {
            select_qb.push(" AND enabled = ").push_bind(enabled);
        }
        scope.push_visible(&mut select_qb);

        select_qb
            .push(" ORDER BY priority ASC, pk_observer ASC")
            .push(" LIMIT ")
            .push_bind(query.page_size)
            .push(" OFFSET ")
            .push_bind(offset);

        let observers: Vec<Observer> = select_qb
            .build_query_as()
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok((observers, total_count.0))
    }

    /// Get an observer `scope` can see by ID.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn get_by_id(
        &self,
        id: Uuid,
        scope: ObserverScope,
    ) -> Result<Option<Observer>, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r"SELECT pk_observer, id, name, description, entity_type, event_type,
              condition_expression, actions, enabled, priority, retry_config,
              timeout_ms, fk_customer_org, created_at, updated_at,
              created_by, updated_by, deleted_at, managed_by
              FROM core.tb_observer WHERE id = ",
        );
        qb.push_bind(id).push(" AND deleted_at IS NULL");

        scope.push_visible(&mut qb);

        let observer: Option<Observer> = qb
            .build_query_as()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(observer)
    }

    /// Create a new observer.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` or `ServerError::Validation` on failure.
    pub async fn create(
        &self,
        request: &CreateObserverRequest,
        scope: ObserverScope,
        created_by: Option<&str>,
    ) -> Result<Observer, ServerError> {
        let actions_json = serde_json::to_value(&request.actions)
            .map_err(|e| ServerError::Validation(format!("Invalid actions: {}", e)))?;

        let retry_config = request.retry_config.clone().unwrap_or_default();
        let retry_config_json = serde_json::to_value(&retry_config)
            .map_err(|e| ServerError::Validation(format!("Invalid retry config: {}", e)))?;

        let observer: Observer = sqlx::query_as(
            r"
            INSERT INTO core.tb_observer (
                name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority,
                retry_config, timeout_ms, fk_customer_org, created_by, updated_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $12)
            RETURNING
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at, managed_by
            ",
        )
        .bind(&request.name)
        .bind(&request.description)
        .bind(&request.entity_type)
        .bind(&request.event_type)
        .bind(&request.condition_expression)
        .bind(&actions_json)
        .bind(request.enabled)
        .bind(request.priority)
        .bind(&retry_config_json)
        .bind(request.timeout_ms)
        .bind(scope.organisation())
        .bind(created_by)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("idx_observer_name_unique") {
                ServerError::Conflict(format!(
                    "Observer with name '{}' already exists",
                    request.name
                ))
            } else {
                ServerError::Database(e.to_string())
            }
        })?;

        Ok(observer)
    }

    /// Update an existing observer the scope owns.
    ///
    /// Returns `None` when no live observer with this id is owned by `scope`: an organisation
    /// does not change another organisation's observers, nor the global ones.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` or `ServerError::Validation` on failure.
    pub async fn update(
        &self,
        id: Uuid,
        request: &UpdateObserverRequest,
        scope: ObserverScope,
        updated_by: Option<&str>,
    ) -> Result<Option<Observer>, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("UPDATE core.tb_observer SET updated_by = ");
        qb.push_bind(updated_by);

        if let Some(ref name) = request.name {
            qb.push(", name = ").push_bind(name);
        }
        // A description sent as `null` binds `NULL`.
        if let Some(ref description) = request.description {
            qb.push(", description = ").push_bind(description);
        }
        if let Some(ref entity_type) = request.entity_type {
            qb.push(", entity_type = ").push_bind(entity_type);
        }
        if let Some(ref event_type) = request.event_type {
            qb.push(", event_type = ").push_bind(event_type);
        }
        // A condition sent as `null` binds `NULL`: the observer fires on every matching event.
        if let Some(ref condition_expression) = request.condition_expression {
            qb.push(", condition_expression = ").push_bind(condition_expression);
        }
        if let Some(ref actions) = request.actions {
            let actions_json = serde_json::to_value(actions)
                .map_err(|e| ServerError::Validation(format!("Invalid actions: {}", e)))?;
            qb.push(", actions = ").push_bind(actions_json);
        }
        if let Some(enabled) = request.enabled {
            qb.push(", enabled = ").push_bind(enabled);
        }
        if let Some(priority) = request.priority {
            qb.push(", priority = ").push_bind(priority);
        }
        // A retry config or timeout sent as `null` stores what a create without it stores.
        if let Some(ref retry_config) = request.retry_config {
            let retry_config_json = serde_json::to_value(retry_config.clone().unwrap_or_default())
                .map_err(|e| ServerError::Validation(format!("Invalid retry config: {}", e)))?;
            qb.push(", retry_config = ").push_bind(retry_config_json);
        }
        if let Some(timeout_ms) = request.timeout_ms {
            qb.push(", timeout_ms = ").push_bind(timeout_ms.unwrap_or_else(default_timeout));
        }

        qb.push(" WHERE id = ").push_bind(id).push(" AND deleted_at IS NULL");
        if !request.changes_only_enabled() {
            qb.push(" AND managed_by IS NULL");
        }
        scope.push_owned(&mut qb, "pk_observer");
        qb.push(
            r" RETURNING
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at, managed_by",
        );

        let observer: Option<Observer> = qb
            .build_query_as()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;
        if observer.is_none() {
            self.refuse_schema_managed(id, scope, "changed").await?;
        }

        Ok(observer)
    }

    /// Refuse with `ServerError::Conflict` when `id` is a live observer `scope` owns and
    /// the compiled schema declares: the write that found no row was refused for that.
    async fn refuse_schema_managed(
        &self,
        id: Uuid,
        scope: ObserverScope,
        verb: &str,
    ) -> Result<(), ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT name FROM core.tb_observer WHERE id = ");
        qb.push_bind(id).push(" AND deleted_at IS NULL AND managed_by IS NOT NULL");
        scope.push_owned(&mut qb, "pk_observer");
        let declared: Option<(String,)> = qb
            .build_query_as()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;
        match declared {
            Some((name,)) => Err(ServerError::Conflict(format!(
                "Observer '{name}' is declared in the schema and cannot be {verb} here: the \
                 server writes it from the schema at every boot. Change the declaration and \
                 redeploy. It can be disabled and enabled here."
            ))),
            None => Ok(()),
        }
    }

    /// Soft delete an observer the scope owns.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure, and `ServerError::Conflict` for an
    /// observer the schema declares.
    pub async fn delete(&self, id: Uuid, scope: ObserverScope) -> Result<bool, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("UPDATE core.tb_observer SET deleted_at = NOW() WHERE id = ");
        qb.push_bind(id).push(" AND deleted_at IS NULL AND managed_by IS NULL");

        scope.push_owned(&mut qb, "pk_observer");

        let result = qb
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;
        if result.rows_affected() == 0 {
            self.refuse_schema_managed(id, scope, "deleted").await?;
        }

        Ok(result.rows_affected() > 0)
    }

    /// Make `core.tb_observer` hold the observers `owner`'s compiled schema declares.
    ///
    /// Each declared observer is a global row (`fk_customer_org IS NULL`) whose
    /// `managed_by` is `owner`: inserted, or rewritten where the declaration changed. A
    /// global row of the same name the admin API wrote is adopted. A row of `owner` its
    /// schema no longer declares is soft-deleted. Whether a row is enabled is left as it
    /// stands, so an observer an operator disabled stays disabled across a deploy.
    ///
    /// Another owner's rows are neither rewritten nor removed. A declared name another
    /// owner already holds is refused: the two would otherwise rewrite one row in turn.
    ///
    /// The rows of the unnamed owner ([`MANAGED_BY_SCHEMA`], what every schema was stored
    /// under before owners carried a project name) belong to whichever named owner
    /// reconciles first: one it still declares is taken over in place, and one it does
    /// not is soft-deleted, since no declaration stands behind it any more.
    ///
    /// One transaction under an advisory lock: replicas booting together write one set.
    /// An organisation's observers are never read or written.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` for a name another owner holds, and
    /// `ServerError::Database` on query failure. Nothing is written in either case.
    pub async fn reconcile_declared(
        &self,
        owner: &str,
        declared: &[fraiseql_core::schema::ObserverDefinition],
    ) -> Result<Reconciled, ServerError> {
        let mut tx = self.pool.begin().await.map_err(|e| ServerError::Database(e.to_string()))?;
        let outcome = Self::reconcile_declared_in(&mut tx, owner, declared).await?;
        tx.commit().await.map_err(|e| ServerError::Database(e.to_string()))?;
        Ok(outcome)
    }

    /// [`reconcile_declared`](Self::reconcile_declared) inside the caller's transaction,
    /// which holds the advisory lock until it ends: a schema reload builds the runtime's
    /// observers from the rows before it commits, so a reload that fails writes nothing.
    ///
    /// # Errors
    ///
    /// As [`reconcile_declared`](Self::reconcile_declared); the caller rolls back.
    pub async fn reconcile_declared_in(
        tx: &mut sqlx::PgConnection,
        owner: &str,
        declared: &[fraiseql_core::schema::ObserverDefinition],
    ) -> Result<Reconciled, ServerError> {
        let db = |e: sqlx::Error| ServerError::Database(e.to_string());
        let json = |e: serde_json::Error| ServerError::Validation(e.to_string());
        // Not the DDL lock: it orders reconciliations of the declared observers, which
        // write rows, and no DDL runs under it.
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(RECONCILE_LOCK_KEY)
            .execute(&mut *tx)
            .await
            .map_err(db)?;

        // A named owner inherits the unnamed owner's rows; the unnamed owner inherits none.
        let inherits = |holder: &str| holder == MANAGED_BY_SCHEMA && owner != MANAGED_BY_SCHEMA;
        let mut outcome = Reconciled::default();
        for observer in declared {
            let actions = serde_json::Value::Array(observer.actions.clone());
            let retry = serde_json::to_value(&observer.retry).map_err(json)?;
            let stored: Option<(i64, Option<String>, bool, bool)> = sqlx::query_as(
                "SELECT pk_observer, managed_by, enabled, \
                        ROW(entity_type::text, event_type::text, condition_expression, actions, \
                            retry_config) \
                        IS NOT DISTINCT FROM ROW($2::text, $3::text, $4::text, $5::jsonb, $6::jsonb) \
                 FROM core.tb_observer \
                 WHERE name = $1 AND fk_customer_org IS NULL AND deleted_at IS NULL \
                 FOR UPDATE",
            )
            .bind(&observer.name)
            .bind(&observer.entity)
            .bind(&observer.event)
            .bind(&observer.condition)
            .bind(&actions)
            .bind(&retry)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;

            match stored {
                None => {
                    sqlx::query(
                        "INSERT INTO core.tb_observer (name, entity_type, event_type, \
                             condition_expression, actions, retry_config, managed_by, \
                             created_by, updated_by) \
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $7, $7)",
                    )
                    .bind(&observer.name)
                    .bind(&observer.entity)
                    .bind(&observer.event)
                    .bind(&observer.condition)
                    .bind(&actions)
                    .bind(&retry)
                    .bind(owner)
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                    outcome.created.push(observer.name.clone());
                },
                Some((_, Some(other), ..)) if other != owner && !inherits(&other) => {
                    return Err(ServerError::ConfigError(format!(
                        "observer `{}` is declared by {}, and core.tb_observer already holds \
                         a global observer of that name declared by {}. Two projects on one \
                         database cannot declare the same observer name: rename one of the \
                         two, and check that each schema's `[project] name` is the one \
                         intended",
                        observer.name,
                        owner_shown(owner),
                        owner_shown(&other)
                    )));
                },
                Some((pk, managed_by, enabled, same)) => {
                    let adopted = managed_by.is_none();
                    let taken_over = managed_by.as_deref().is_some_and(inherits);
                    if adopted || taken_over || !same {
                        sqlx::query(
                            "UPDATE core.tb_observer SET entity_type = $2, event_type = $3, \
                                 condition_expression = $4, actions = $5, retry_config = $6, \
                                 managed_by = $7, updated_by = $7 \
                             WHERE pk_observer = $1",
                        )
                        .bind(pk)
                        .bind(&observer.entity)
                        .bind(&observer.event)
                        .bind(&observer.condition)
                        .bind(&actions)
                        .bind(&retry)
                        .bind(owner)
                        .execute(&mut *tx)
                        .await
                        .map_err(db)?;
                        let changed = if adopted {
                            &mut outcome.adopted
                        } else if taken_over {
                            &mut outcome.taken_over
                        } else {
                            &mut outcome.updated
                        };
                        changed.push(observer.name.clone());
                    }
                    if !enabled {
                        outcome.disabled.push(observer.name.clone());
                    }
                },
            }
        }

        let names: Vec<&str> = declared.iter().map(|o| o.name.as_str()).collect();
        let removed: Vec<(String, bool)> = sqlx::query_as(
            "UPDATE core.tb_observer SET deleted_at = NOW(), updated_by = $1 \
             WHERE (managed_by = $1 OR managed_by = $3) AND deleted_at IS NULL \
               AND NOT (name = ANY($2)) \
             RETURNING name, managed_by = $1",
        )
        .bind(owner)
        .bind(&names)
        .bind(MANAGED_BY_SCHEMA)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        for (name, own) in removed {
            if own {
                outcome.removed.push(name);
            } else {
                outcome.removed_unnamed.push(name);
            }
        }

        Ok(outcome)
    }

    /// Get statistics for the observers `scope` owns.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn get_stats(
        &self,
        observer_id: Option<Uuid>,
        scope: ObserverScope,
    ) -> Result<Vec<ObserverStats>, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT * FROM core.vw_observer_stats WHERE 1=1");

        if let Some(id) = observer_id {
            qb.push(" AND observer_id = ").push_bind(id);
        }

        scope.push_owned(&mut qb, "pk_observer");

        qb.push(" ORDER BY observer_name ASC");

        let stats: Vec<ObserverStats> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(stats)
    }

    /// List execution logs of the observers `scope` owns.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn list_logs(
        &self,
        query: &ListObserverLogsQuery,
        scope: ObserverScope,
    ) -> Result<(Vec<ObserverLog>, i64), ServerError> {
        let offset = (query.page - 1) * query.page_size;

        // --- count query ---
        let mut count_qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM core.tb_observer_log WHERE 1=1");

        if let Some(observer_id) = query.observer_id {
            count_qb
                .push(" AND fk_observer = (SELECT pk_observer FROM core.tb_observer WHERE id = ")
                .push_bind(observer_id)
                .push(")");
        }
        if let Some(ref status) = query.status {
            count_qb.push(" AND status = ").push_bind(status);
        }
        if let Some(event_id) = query.event_id {
            count_qb.push(" AND event_id = ").push_bind(event_id);
        }
        if let Some(ref trace_id) = query.trace_id {
            count_qb.push(" AND trace_id = ").push_bind(trace_id);
        }
        scope.push_owned(&mut count_qb, "fk_observer");

        let total_count: (i64,) = count_qb
            .build_query_as()
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        // --- select query ---
        let mut select_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r"SELECT pk_observer_log, id, fk_observer, event_id, entity_type, entity_id,
              event_type, status, action_index, action_type, started_at, completed_at,
              duration_ms, error_code, error_message, attempt_number, trace_id, created_at
              FROM core.tb_observer_log WHERE 1=1",
        );

        if let Some(observer_id) = query.observer_id {
            select_qb
                .push(" AND fk_observer = (SELECT pk_observer FROM core.tb_observer WHERE id = ")
                .push_bind(observer_id)
                .push(")");
        }
        if let Some(ref status) = query.status {
            select_qb.push(" AND status = ").push_bind(status);
        }
        if let Some(event_id) = query.event_id {
            select_qb.push(" AND event_id = ").push_bind(event_id);
        }
        if let Some(ref trace_id) = query.trace_id {
            select_qb.push(" AND trace_id = ").push_bind(trace_id);
        }
        scope.push_owned(&mut select_qb, "fk_observer");

        select_qb
            .push(" ORDER BY created_at DESC")
            .push(" LIMIT ")
            .push_bind(query.page_size)
            .push(" OFFSET ")
            .push_bind(offset);

        let logs: Vec<ObserverLog> = select_qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok((logs, total_count.0))
    }

    /// Refuse to run over an empty `core` home while an earlier release's observer
    /// tables still hold rows elsewhere (#1893).
    ///
    /// Earlier releases created `tb_observer` and `observer_checkpoints` unqualified,
    /// so in whichever schema led that deployment's `search_path`, which need not be
    /// on today's. Running over the empty `core` tables would load no observers, or
    /// replay the whole change log from cursor 0, without a word. A table that cannot
    /// be read is logged and skipped: its contents are unknown, not known to be rows.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::ConfigError` naming the statements that move the stranded
    /// tables into `core`, or `ServerError::Database` if the catalog cannot be read.
    pub async fn refuse_stranded_legacy_tables(&self) -> Result<(), ServerError> {
        let moves = crate::stranded_tables::stranded_moves(&self.pool, &STRANDABLE_TABLES).await?;
        if moves.is_empty() {
            return Ok(());
        }
        Err(ServerError::ConfigError(format!(
            "observer tables from an earlier release hold rows outside `core`, and their \
             `core` tables are empty. FraiseQL keeps its observer tables in `core` whatever \
             the search_path (#1893); starting now would run with none of them. Move them, \
             then restart:\n{}",
            moves.join("\n")
        )))
    }
}

/// An owner as a refusal shows it: the project behind `schema:<name>`.
fn owner_shown(owner: &str) -> String {
    owner
        .strip_prefix(MANAGED_BY_PROJECT_PREFIX)
        .map_or_else(|| format!("the owner `{owner}`"), |project| format!("project `{project}`"))
}

/// The advisory lock [`ObserverRepository::reconcile_declared`] writes under.
const RECONCILE_LOCK_KEY: i64 = 0x6672_6f62_7372_6563;

/// What [`ObserverRepository::reconcile_declared`] changed, by observer name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Declared observers that had no row.
    pub created:         Vec<String>,
    /// Schema-managed rows rewritten to a changed declaration.
    pub updated:         Vec<String>,
    /// Global rows the admin API wrote, taken over by a declaration of the same name.
    pub adopted:         Vec<String>,
    /// Schema-managed rows the schema no longer declares, soft-deleted.
    pub removed:         Vec<String>,
    /// Declared observers whose row is disabled, and so do not fire.
    pub disabled:        Vec<String>,
    /// Rows of the unnamed owner ([`MANAGED_BY_SCHEMA`]) the schema still declares, now
    /// its named owner's: same row, same id, same log.
    pub taken_over:      Vec<String>,
    /// Rows of the unnamed owner the schema does not declare, soft-deleted.
    pub removed_unnamed: Vec<String>,
}

impl Reconciled {
    /// Every row the reconcile of `owner` soft-deleted, with the owner that held it: its
    /// own rows, then the unnamed owner's.
    pub fn removed_with_owner<'a>(
        &'a self,
        owner: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a str)> {
        let own = self.removed.iter().map(move |name| (name.as_str(), owner));
        let unnamed = self.removed_unnamed.iter().map(|name| (name.as_str(), MANAGED_BY_SCHEMA));
        own.chain(unnamed)
    }
}

/// The observer tables an earlier release may have left outside `core`, each with
/// the table that moves with it.
const STRANDABLE_TABLES: [crate::stranded_tables::Strandable; 2] = [
    crate::stranded_tables::Strandable {
        table:     "tb_observer",
        companion: Some("tb_observer_log"),
    },
    crate::stranded_tables::Strandable {
        table:     "observer_checkpoints",
        companion: Some("observer_checkpoints_history"),
    },
];
