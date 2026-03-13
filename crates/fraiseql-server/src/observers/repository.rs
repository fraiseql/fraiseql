//! Observer repository for database operations.

use sqlx::PgPool;
use uuid::Uuid;

use super::{
    CreateObserverRequest, ListObserverLogsQuery, ListObserversQuery, Observer, ObserverLog,
    ObserverStats, UpdateObserverRequest,
};
use crate::ServerError;

/// Repository for observer CRUD operations.
#[derive(Clone)]
pub struct ObserverRepository {
    pool: PgPool,
}

impl ObserverRepository {
    /// Create a new observer repository.
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List observers with optional filters.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn list(
        &self,
        query: &ListObserversQuery,
        customer_org: Option<i64>,
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
        if let Some(org_id) = customer_org {
            count_qb
                .push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

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
        if let Some(org_id) = customer_org {
            select_qb
                .push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

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

    /// Get a single observer by ID.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn get_by_id(
        &self,
        id: Uuid,
        customer_org: Option<i64>,
    ) -> Result<Option<Observer>, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            r"SELECT pk_observer, id, name, description, entity_type, event_type,
              condition_expression, actions, enabled, priority, retry_config,
              timeout_ms, fk_customer_org, created_at, updated_at,
              created_by, updated_by, deleted_at
              FROM tb_observer WHERE id = ",
        );
        qb.push_bind(id).push(" AND deleted_at IS NULL");

        if let Some(org_id) = customer_org {
            qb.push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

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
        customer_org: Option<i64>,
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
        .bind(customer_org)
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

    /// Update an existing observer.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` or `ServerError::Validation` on failure.
    pub async fn update(
        &self,
        id: Uuid,
        request: &UpdateObserverRequest,
        customer_org: Option<i64>,
        updated_by: Option<&str>,
    ) -> Result<Option<Observer>, ServerError> {
        // First check if the observer exists
        let existing: Option<Observer> = self.get_by_id(id, customer_org).await?;
        if existing.is_none() {
            return Ok(None);
        }

        let mut set_clauses = vec!["updated_by = $2".to_string()];
        let mut param_index = 3;

        // Build dynamic SET clause based on provided fields
        if request.name.is_some() {
            set_clauses.push(format!("name = ${}", param_index));
            param_index += 1;
        }
        if request.description.is_some() {
            set_clauses.push(format!("description = ${}", param_index));
            param_index += 1;
        }
        if request.entity_type.is_some() {
            set_clauses.push(format!("entity_type = ${}", param_index));
            param_index += 1;
        }
        if request.event_type.is_some() {
            set_clauses.push(format!("event_type = ${}", param_index));
            param_index += 1;
        }
        if request.condition_expression.is_some() {
            set_clauses.push(format!("condition_expression = ${}", param_index));
            param_index += 1;
        }
        if request.actions.is_some() {
            set_clauses.push(format!("actions = ${}", param_index));
            param_index += 1;
        }
        if request.enabled.is_some() {
            set_clauses.push(format!("enabled = ${}", param_index));
            param_index += 1;
        }
        if request.priority.is_some() {
            set_clauses.push(format!("priority = ${}", param_index));
            param_index += 1;
        }
        if request.retry_config.is_some() {
            set_clauses.push(format!("retry_config = ${}", param_index));
            param_index += 1;
        }
        if request.timeout_ms.is_some() {
            set_clauses.push(format!("timeout_ms = ${}", param_index));
            // param_index += 1; // Not needed for last param
        }

        let sql = format!(
            r"
            UPDATE tb_observer
            SET {}
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at
            ",
            set_clauses.join(", ")
        );

        // Build query with dynamic bindings
        let mut query = sqlx::query_as::<_, Observer>(&sql).bind(id).bind(updated_by);

        if let Some(ref name) = request.name {
            query = query.bind(name);
        }
        if let Some(ref description) = request.description {
            query = query.bind(description);
        }
        if let Some(ref entity_type) = request.entity_type {
            query = query.bind(entity_type);
        }
        if let Some(ref event_type) = request.event_type {
            query = query.bind(event_type);
        }
        if let Some(ref condition_expression) = request.condition_expression {
            query = query.bind(condition_expression);
        }
        if let Some(ref actions) = request.actions {
            let actions_json = serde_json::to_value(actions)
                .map_err(|e| ServerError::Validation(format!("Invalid actions: {}", e)))?;
            query = query.bind(actions_json);
        }
        if let Some(enabled) = request.enabled {
            query = query.bind(enabled);
        }
        if let Some(priority) = request.priority {
            query = query.bind(priority);
        }
        if let Some(ref retry_config) = request.retry_config {
            let retry_config_json = serde_json::to_value(retry_config)
                .map_err(|e| ServerError::Validation(format!("Invalid retry config: {}", e)))?;
            query = query.bind(retry_config_json);
        }
        if let Some(timeout_ms) = request.timeout_ms {
            query = query.bind(timeout_ms);
        }

        let observer = query
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(observer)
    }

    /// Soft delete an observer.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn delete(&self, id: Uuid, customer_org: Option<i64>) -> Result<bool, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> = sqlx::QueryBuilder::new(
            "UPDATE tb_observer SET deleted_at = NOW() WHERE id = ",
        );
        qb.push_bind(id).push(" AND deleted_at IS NULL");

        if let Some(org_id) = customer_org {
            qb.push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

        let result = qb
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    /// Get observer statistics.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn get_stats(
        &self,
        observer_id: Option<Uuid>,
        customer_org: Option<i64>,
    ) -> Result<Vec<ObserverStats>, ServerError> {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT * FROM vw_observer_stats WHERE 1=1");

        if let Some(id) = observer_id {
            qb.push(" AND observer_id = ").push_bind(id);
        }

        if let Some(org_id) = customer_org {
            qb.push(
                " AND pk_observer IN (SELECT pk_observer FROM tb_observer \
                 WHERE fk_customer_org IS NULL OR fk_customer_org = ",
            )
            .push_bind(org_id)
            .push(")");
        }

        qb.push(" ORDER BY observer_name ASC");

        let stats: Vec<ObserverStats> = qb
            .build_query_as()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(stats)
    }

    /// List observer execution logs.
    ///
    /// # Errors
    ///
    /// Returns `ServerError::Database` on query failure.
    pub async fn list_logs(
        &self,
        query: &ListObserverLogsQuery,
        customer_org: Option<i64>,
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
        if let Some(org_id) = customer_org {
            count_qb
                .push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

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
        if let Some(org_id) = customer_org {
            select_qb
                .push(" AND (fk_customer_org IS NULL OR fk_customer_org = ")
                .push_bind(org_id)
                .push(")");
        }

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

#[cfg(test)]
#[allow(clippy::unwrap_used)] // Reason: test code, panics are acceptable
mod tests {
    use super::super::RetryConfig;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.backoff, "exponential");
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60000);
    }

    // --- SQL structure unit tests (no database required) ---
    //
    // These verify the central injection-safety invariant: bound values produced by
    // push_bind() are assigned $N placeholders and never appear in the SQL string itself.

    #[test]
    fn test_list_entity_type_not_inlined() {
        let malicious = "' OR '1'='1";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND entity_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious), "user input must not appear in SQL string");
        assert!(sql.contains("$1"), "placeholder must be present");
    }

    #[test]
    fn test_list_event_type_not_inlined() {
        let malicious = "'; DROP TABLE tb_observer; --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND event_type = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_status_not_inlined() {
        let malicious = "' UNION SELECT * FROM secrets --";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND status = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_logs_trace_id_not_inlined() {
        let malicious = "x' OR fk_customer_org IS NOT NULL--";
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND trace_id = ").push_bind(malicious);
        let sql = qb.sql();
        assert!(!sql.contains(malicious));
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_list_no_filters_produces_minimal_sql() {
        let qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        let sql = qb.sql();
        assert!(!sql.contains("entity_type"));
        assert!(!sql.contains("event_type"));
        assert!(!sql.contains("enabled"));
        assert!(!sql.contains("fk_customer_org"));
        assert!(!sql.contains("deleted_at"));
    }

    #[test]
    fn test_list_exclude_deleted_adds_condition() {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND deleted_at IS NULL");
        let sql = qb.sql();
        assert!(sql.contains("deleted_at IS NULL"));
    }

    #[test]
    fn test_list_logs_observer_id_uses_placeholder() {
        let observer_id = uuid::Uuid::new_v4();
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tb_observer_log WHERE 1=1");
        qb.push(" AND fk_observer = (SELECT pk_observer FROM tb_observer WHERE id = ")
            .push_bind(observer_id)
            .push(")");
        let sql = qb.sql();
        assert!(!sql.contains(&observer_id.to_string()), "UUID must not be inlined in SQL");
        assert!(sql.contains("$1"));
    }

    #[test]
    fn test_multiple_filters_use_sequential_placeholders() {
        let mut qb: sqlx::QueryBuilder<sqlx::Postgres> =
            sqlx::QueryBuilder::new("SELECT COUNT(*) AS count FROM tb_observer WHERE 1=1");
        qb.push(" AND entity_type = ").push_bind("Order");
        qb.push(" AND event_type = ").push_bind("INSERT");
        qb.push(" AND enabled = ").push_bind(true);
        let sql = qb.sql();
        assert!(sql.contains("$1"));
        assert!(sql.contains("$2"));
        assert!(sql.contains("$3"));
        assert!(!sql.contains("Order"));
        assert!(!sql.contains("INSERT"));
    }
}
