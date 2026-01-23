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
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List observers with optional filters.
    pub async fn list(
        &self,
        query: &ListObserversQuery,
        customer_org: Option<i64>,
    ) -> Result<(Vec<Observer>, i64), ServerError> {
        let offset = (query.page - 1) * query.page_size;

        // Build dynamic WHERE clause
        let mut conditions = vec!["1=1".to_string()];

        if !query.include_deleted {
            conditions.push("deleted_at IS NULL".to_string());
        }

        if let Some(ref entity_type) = query.entity_type {
            conditions.push(format!("entity_type = '{}'", entity_type.replace('\'', "''")));
        }

        if let Some(ref event_type) = query.event_type {
            conditions.push(format!("event_type = '{}'", event_type.replace('\'', "''")));
        }

        if let Some(enabled) = query.enabled {
            conditions.push(format!("enabled = {}", enabled));
        }

        if let Some(org_id) = customer_org {
            conditions.push(format!("(fk_customer_org IS NULL OR fk_customer_org = {})", org_id));
        }

        let where_clause = conditions.join(" AND ");

        // Get total count
        let count_sql = format!("SELECT COUNT(*) as count FROM tb_observer WHERE {}", where_clause);
        let total_count: (i64,) = sqlx::query_as(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        // Get paginated results
        let select_sql = format!(
            r"
            SELECT
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at
            FROM tb_observer
            WHERE {}
            ORDER BY priority ASC, pk_observer ASC
            LIMIT {} OFFSET {}
            ",
            where_clause, query.page_size, offset
        );

        let observers: Vec<Observer> = sqlx::query_as(&select_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok((observers, total_count.0))
    }

    /// Get a single observer by ID.
    pub async fn get_by_id(
        &self,
        id: Uuid,
        customer_org: Option<i64>,
    ) -> Result<Option<Observer>, ServerError> {
        let mut query = String::from(
            r"
            SELECT
                pk_observer, id, name, description, entity_type, event_type,
                condition_expression, actions, enabled, priority, retry_config,
                timeout_ms, fk_customer_org, created_at, updated_at,
                created_by, updated_by, deleted_at
            FROM tb_observer
            WHERE id = $1 AND deleted_at IS NULL
            ",
        );

        if let Some(org_id) = customer_org {
            query.push_str(&format!(
                " AND (fk_customer_org IS NULL OR fk_customer_org = {})",
                org_id
            ));
        }

        let observer: Option<Observer> = sqlx::query_as(&query)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(observer)
    }

    /// Create a new observer.
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
                ServerError::Conflict(format!("Observer with name '{}' already exists", request.name))
            } else {
                ServerError::Database(e.to_string())
            }
        })?;

        Ok(observer)
    }

    /// Update an existing observer.
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
    pub async fn delete(
        &self,
        id: Uuid,
        customer_org: Option<i64>,
    ) -> Result<bool, ServerError> {
        let mut sql = String::from(
            r"
            UPDATE tb_observer
            SET deleted_at = NOW()
            WHERE id = $1 AND deleted_at IS NULL
            ",
        );

        if let Some(org_id) = customer_org {
            sql.push_str(&format!(
                " AND (fk_customer_org IS NULL OR fk_customer_org = {})",
                org_id
            ));
        }

        let result = sqlx::query(&sql)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    /// Get observer statistics.
    pub async fn get_stats(
        &self,
        observer_id: Option<Uuid>,
        customer_org: Option<i64>,
    ) -> Result<Vec<ObserverStats>, ServerError> {
        let mut sql = String::from("SELECT * FROM vw_observer_stats WHERE 1=1");

        if let Some(id) = observer_id {
            sql.push_str(&format!(" AND observer_id = '{}'", id));
        }

        if let Some(org_id) = customer_org {
            // Note: vw_observer_stats would need to include fk_customer_org
            // For now, we filter via a join or subquery
            sql.push_str(&format!(
                " AND pk_observer IN (SELECT pk_observer FROM tb_observer WHERE fk_customer_org IS NULL OR fk_customer_org = {})",
                org_id
            ));
        }

        sql.push_str(" ORDER BY observer_name ASC");

        let stats: Vec<ObserverStats> = sqlx::query_as(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok(stats)
    }

    /// List observer execution logs.
    pub async fn list_logs(
        &self,
        query: &ListObserverLogsQuery,
        customer_org: Option<i64>,
    ) -> Result<(Vec<ObserverLog>, i64), ServerError> {
        let offset = (query.page - 1) * query.page_size;

        let mut conditions = vec!["1=1".to_string()];

        if let Some(observer_id) = query.observer_id {
            conditions.push(format!(
                "fk_observer = (SELECT pk_observer FROM tb_observer WHERE id = '{}')",
                observer_id
            ));
        }

        if let Some(ref status) = query.status {
            conditions.push(format!("status = '{}'", status.replace('\'', "''")));
        }

        if let Some(event_id) = query.event_id {
            conditions.push(format!("event_id = '{}'", event_id));
        }

        if let Some(ref trace_id) = query.trace_id {
            conditions.push(format!("trace_id = '{}'", trace_id.replace('\'', "''")));
        }

        if let Some(org_id) = customer_org {
            conditions.push(format!(
                "(fk_customer_org IS NULL OR fk_customer_org = {})",
                org_id
            ));
        }

        let where_clause = conditions.join(" AND ");

        // Get total count
        let count_sql = format!(
            "SELECT COUNT(*) as count FROM tb_observer_log WHERE {}",
            where_clause
        );
        let total_count: (i64,) = sqlx::query_as(&count_sql)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        // Get paginated results
        let select_sql = format!(
            r"
            SELECT
                pk_observer_log, id, fk_observer, event_id, entity_type, entity_id,
                event_type, status, action_index, action_type, started_at, completed_at,
                duration_ms, error_code, error_message, attempt_number, trace_id, created_at
            FROM tb_observer_log
            WHERE {}
            ORDER BY created_at DESC
            LIMIT {} OFFSET {}
            ",
            where_clause, query.page_size, offset
        );

        let logs: Vec<ObserverLog> = sqlx::query_as(&select_sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ServerError::Database(e.to_string()))?;

        Ok((logs, total_count.0))
    }
}

#[cfg(test)]
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
}
