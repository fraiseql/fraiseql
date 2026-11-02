//! Observer repository for database operations.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery, Observer, ObserverLog,
    ObserverScope, ObserverStats, UpdateObserverRequest,
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
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        // --- select query ---
        let mut select_qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r"SELECT pk_observer, id, name, description, entity_type, event_type,
              condition_expression, actions, enabled, priority, retry_config,
              timeout_ms, fk_customer_org, created_at, updated_at,
              created_by, updated_by, deleted_at
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
            .fetch_all(&self.pool)
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
              created_by, updated_by, deleted_at
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
                created_by, updated_by, deleted_at
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
        if let Some(ref description) = request.description {
            qb.push(", description = ").push_bind(description);
        }
        if let Some(ref entity_type) = request.entity_type {
            qb.push(", entity_type = ").push_bind(entity_type);
        }
        if let Some(ref event_type) = request.event_type {
            qb.push(", event_type = ").push_bind(event_type);
        }
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
        if let Some(ref retry_config) = request.retry_config {
            let retry_config_json = serde_json::to_value(retry_config)
                .map_err(|e| ServerError::Validation(format!("Invalid retry config: {}", e)))?;
            qb.push(", retry_config = ").push_bind(retry_config_json);
        }
        if let Some(timeout_ms) = request.timeout_ms {
            qb.push(", timeout_ms = ").push_bind(timeout_ms);
        }

        qb.push(" WHERE id = ").push_bind(id).push(" AND deleted_at IS NULL");
        scope.push_owned(&mut qb, "pk_observer");
        qb.push(
            r" RETURNING
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at",
        );

        let observer = qb
            .build_query_as()
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(observer)
    }

    /// Soft delete an observer the scope owns.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn delete(&self, id: Uuid, scope: ObserverScope) -> Result<bool, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("UPDATE core.tb_observer SET deleted_at = NOW() WHERE id = ");
        qb.push_bind(id).push(" AND deleted_at IS NULL");

        scope.push_owned(&mut qb, "pk_observer");

        let result = qb
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
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
        let mut moves = Vec::new();
        for (table, companion) in STRANDABLE_TABLES {
            if self.has_rows("core", table).await? == Some(true) {
                continue;
            }
            let schemas: Vec<(String,)> = sqlx::query_as(
                "SELECT n.nspname::text FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE c.relname = $1 AND c.relkind IN ('r', 'p') AND n.nspname <> 'core' \
                 ORDER BY 1",
            )
            .bind(table)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;
            for (schema,) in schemas {
                if self.has_rows(&schema, table).await? == Some(true) {
                    moves.push(move_into_core(&schema, table, companion));
                }
            }
        }
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

    /// Whether `schema.table` holds a row: `None` when it is absent or unreadable.
    async fn has_rows(&self, schema: &str, table: &str) -> Result<Option<bool>, ServerError> {
        let qualified = format!("{}.{}", quote_ident(schema), quote_ident(table));
        let exists: (Option<String>,) = sqlx::query_as("SELECT to_regclass($1)::text")
            .bind(&qualified)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;
        if exists.0.is_none() {
            return Ok(None);
        }
        match sqlx::query_as::<_, (bool,)>(&format!("SELECT EXISTS (SELECT 1 FROM {qualified})"))
            .fetch_one(&self.pool)
            .await
        {
            Ok((populated,)) => Ok(Some(populated)),
            Err(e) => {
                tracing::warn!(
                    table = %qualified,
                    error = %e,
                    "Cannot read a possible legacy observer table; not checking it"
                );
                Ok(None)
            },
        }
    }
}

/// The observer tables an earlier release may have left outside `core`, each with
/// the table that moves with it.
const STRANDABLE_TABLES: [(&str, &str); 2] = [
    ("tb_observer", "tb_observer_log"),
    ("observer_checkpoints", "observer_checkpoints_history"),
];

/// The statements that replace the empty `core` tables boot created with the
/// populated ones in `schema`.
fn move_into_core(schema: &str, table: &str, companion: &str) -> String {
    let schema = quote_ident(schema);
    format!(
        "  BEGIN;\n  \
         DROP TABLE IF EXISTS core.{companion}, core.{table} CASCADE;\n  \
         ALTER TABLE {schema}.{table} SET SCHEMA core;\n  \
         ALTER TABLE IF EXISTS {schema}.{companion} SET SCHEMA core;\n  \
         COMMIT;"
    )
}

/// `ident` as a PostgreSQL identifier, quoted only when it has to be.
fn quote_ident(ident: &str) -> String {
    let plain = ident.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && ident.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if plain {
        ident.to_owned()
    } else {
        format!("\"{}\"", ident.replace('"', "\"\""))
    }
}
