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
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");

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
              FROM tb_observer WHERE 1=1",
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
              FROM tb_observer WHERE id = ",
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
            INSERT INTO tb_observer (
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
            sqlx::QueryBuilder::new("UPDATE tb_observer SET updated_by = ");
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
            sqlx::QueryBuilder::new("UPDATE tb_observer SET deleted_at = NOW() WHERE id = ");
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
            sqlx::QueryBuilder::new("SELECT * FROM vw_observer_stats WHERE 1=1");

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
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer_log WHERE 1=1");

        if let Some(observer_id) = query.observer_id {
            count_qb
                .push(" AND fk_observer = (SELECT pk_observer FROM tb_observer WHERE id = ")
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
              FROM tb_observer_log WHERE 1=1",
        );

        if let Some(observer_id) = query.observer_id {
            select_qb
                .push(" AND fk_observer = (SELECT pk_observer FROM tb_observer WHERE id = ")
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
}
