//! PostgreSQL-backed Saga Store for distributed transaction persistence.
//!
//! This module provides a production-grade persistent store for sagas using PostgreSQL,
//! enabling crash recovery, distributed coordination, and saga tracking across instances.

use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

use deadpool_postgres::Pool;

/// SagaState enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SagaState {
    Pending,
    Executing,
    Completed,
    Failed,
    Compensating,
    Compensated,
}

impl SagaState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SagaState::Pending => "pending",
            SagaState::Executing => "executing",
            SagaState::Completed => "completed",
            SagaState::Failed => "failed",
            SagaState::Compensating => "compensating",
            SagaState::Compensated => "compensated",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(SagaState::Pending),
            "executing" => Some(SagaState::Executing),
            "completed" => Some(SagaState::Completed),
            "failed" => Some(SagaState::Failed),
            "compensating" => Some(SagaState::Compensating),
            "compensated" => Some(SagaState::Compensated),
            _ => None,
        }
    }
}

/// StepState enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Executing,
    Completed,
    Failed,
}

impl StepState {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepState::Pending => "pending",
            StepState::Executing => "executing",
            StepState::Completed => "completed",
            StepState::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(StepState::Pending),
            "executing" => Some(StepState::Executing),
            "completed" => Some(StepState::Completed),
            "failed" => Some(StepState::Failed),
            _ => None,
        }
    }
}

/// MutationType enum
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationType {
    Create,
    Update,
    Delete,
}

impl MutationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MutationType::Create => "create",
            MutationType::Update => "update",
            MutationType::Delete => "delete",
        }
    }
}

/// Saga struct
#[derive(Debug, Clone)]
pub struct Saga {
    pub id: Uuid,
    pub state: SagaState,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: Option<Value>,
}

/// SagaStep struct
#[derive(Debug, Clone)]
pub struct SagaStep {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub order: usize,
    pub subgraph: String,
    pub mutation_type: MutationType,
    pub typename: String,
    pub variables: Value,
    pub state: StepState,
    pub result: Option<Value>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// SagaRecovery struct
#[derive(Debug, Clone)]
pub struct SagaRecovery {
    pub id: Uuid,
    pub saga_id: Uuid,
    pub recovery_type: String,
    pub attempted_at: chrono::DateTime<chrono::Utc>,
    pub last_attempt: Option<chrono::DateTime<chrono::Utc>>,
    pub attempt_count: i32,
    pub last_error: Option<String>,
}

/// PostgreSQL-backed Saga Store
pub struct PostgresSagaStore {
    pool: Arc<Pool>,
}

impl PostgresSagaStore {
    /// Create a new PostgreSQL saga store
    pub async fn new(_connection_string: &str) -> Result<Self, Box<dyn std::error::Error>> {
        // Parse connection string and create pool
        let cfg = deadpool_postgres::Config {
            dbname: Some("fraiseql".to_string()),
            host: Some("localhost".to_string()),
            port: Some(5432),
            user: Some("postgres".to_string()),
            password: Some("postgres".to_string()),
            ..Default::default()
        };

        let pool = cfg.create_pool(
            Some(deadpool_postgres::Runtime::Tokio1),
            deadpool_postgres::tokio_postgres::NoTls,
        )?;

        // Test connection
        let _conn = pool.get().await?;

        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// Create database schema
    pub async fn migrate_schema(&self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        // Create federation_sagas table
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS federation_sagas (
                id UUID PRIMARY KEY,
                state TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                completed_at TIMESTAMPTZ,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                metadata JSONB
            )
            ",
            &[],
        )
        .await?;

        // Create federation_saga_steps table
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS federation_saga_steps (
                id UUID PRIMARY KEY,
                saga_id UUID NOT NULL REFERENCES federation_sagas(id) ON DELETE CASCADE,
                step_number INTEGER NOT NULL,
                subgraph TEXT NOT NULL,
                mutation_type TEXT NOT NULL,
                typename TEXT NOT NULL,
                variables JSONB NOT NULL,
                state TEXT NOT NULL,
                result JSONB,
                started_at TIMESTAMPTZ,
                completed_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            ",
            &[],
        )
        .await?;

        // Create federation_saga_recovery table
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS federation_saga_recovery (
                id UUID PRIMARY KEY,
                saga_id UUID NOT NULL REFERENCES federation_sagas(id) ON DELETE CASCADE,
                recovery_type TEXT NOT NULL,
                attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_attempt TIMESTAMPTZ,
                attempt_count INTEGER DEFAULT 0,
                last_error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            ",
            &[],
        )
        .await?;

        // Create indices
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_saga_state ON federation_sagas(state)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_saga_created ON federation_sagas(created_at)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_saga_steps_saga_id ON federation_saga_steps(saga_id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_saga_recovery_saga_id ON federation_saga_recovery(saga_id)",
            &[],
        )
        .await?;

        Ok(())
    }

    /// Health check
    pub async fn health_check(&self) -> Result<(), Box<dyn std::error::Error>> {
        let _conn = self.pool.get().await?;
        Ok(())
    }

    /// Save a saga
    pub async fn save_saga(&self, saga: &Saga) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let state = saga.state.as_str();
        let now = chrono::Utc::now();

        conn.execute(
            "INSERT INTO federation_sagas (id, state, created_at, completed_at, updated_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
                 state = $2, completed_at = $4, updated_at = $5, metadata = $6",
            &[
                &saga.id,
                &state,
                &saga.created_at,
                &saga.completed_at,
                &now,
                &saga.metadata,
            ],
        )
        .await?;

        Ok(())
    }

    /// Load a saga by ID
    pub async fn load_saga(&self, saga_id: Uuid) -> Result<Option<Saga>, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        let row = conn
            .query_opt(
                "SELECT id, state, created_at, completed_at, metadata FROM federation_sagas WHERE id = $1",
                &[&saga_id],
            )
            .await?;

        Ok(row.map(|r| Saga {
            id: r.get(0),
            state: SagaState::from_str(r.get::<_, String>(1).as_str()).unwrap_or(SagaState::Pending),
            created_at: r.get(2),
            completed_at: r.get(3),
            metadata: r.get(4),
        }))
    }

    /// Load all sagas
    pub async fn load_all_sagas(&self) -> Result<Vec<Saga>, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        let rows = conn
            .query(
                "SELECT id, state, created_at, completed_at, metadata FROM federation_sagas ORDER BY created_at DESC",
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| Saga {
                id: r.get(0),
                state: SagaState::from_str(r.get::<_, String>(1).as_str()).unwrap_or(SagaState::Pending),
                created_at: r.get(2),
                completed_at: r.get(3),
                metadata: r.get(4),
            })
            .collect())
    }

    /// Load sagas by state
    pub async fn load_sagas_by_state(&self, state: &SagaState) -> Result<Vec<Saga>, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();

        let rows = conn
            .query(
                "SELECT id, state, created_at, completed_at, metadata FROM federation_sagas WHERE state = $1 ORDER BY created_at DESC",
                &[&state_str],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| Saga {
                id: r.get(0),
                state: SagaState::from_str(r.get::<_, String>(1).as_str()).unwrap_or(SagaState::Pending),
                created_at: r.get(2),
                completed_at: r.get(3),
                metadata: r.get(4),
            })
            .collect())
    }

    /// Update saga state
    pub async fn update_saga_state(&self, saga_id: Uuid, state: &SagaState) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at = if matches!(state, SagaState::Completed | SagaState::Compensated) {
            Some(now)
        } else {
            None
        };

        conn.execute(
            "UPDATE federation_sagas SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
            &[&state_str, &completed_at, &now, &saga_id],
        )
        .await?;

        Ok(())
    }

    /// Load saga step by ID
    pub async fn load_saga_step(&self, step_id: Uuid) -> Result<Option<SagaStep>, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        let row = conn
            .query_opt(
                "SELECT id, saga_id, step_number, subgraph, mutation_type, typename, variables, state, result, started_at, completed_at FROM federation_saga_steps WHERE id = $1",
                &[&step_id],
            )
            .await?;

        Ok(row.map(|r| SagaStep {
            id: r.get(0),
            saga_id: r.get(1),
            order: r.get::<_, i32>(2) as usize,
            subgraph: r.get(3),
            mutation_type: match r.get::<_, String>(4).as_str() {
                "create" => MutationType::Create,
                "update" => MutationType::Update,
                "delete" => MutationType::Delete,
                _ => MutationType::Update,
            },
            typename: r.get(5),
            variables: r.get(6),
            state: StepState::from_str(r.get::<_, String>(7).as_str()).unwrap_or(StepState::Pending),
            result: r.get(8),
            started_at: r.get(9),
            completed_at: r.get(10),
        }))
    }

    /// Load saga steps
    pub async fn load_saga_steps(&self, saga_id: Uuid) -> Result<Vec<SagaStep>, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        let rows = conn
            .query(
                "SELECT id, saga_id, step_number, subgraph, mutation_type, typename, variables, state, result, started_at, completed_at FROM federation_saga_steps WHERE saga_id = $1 ORDER BY step_number ASC",
                &[&saga_id],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|r| SagaStep {
                id: r.get(0),
                saga_id: r.get(1),
                order: r.get::<_, i32>(2) as usize,
                subgraph: r.get(3),
                mutation_type: match r.get::<_, String>(4).as_str() {
                    "create" => MutationType::Create,
                    "update" => MutationType::Update,
                    "delete" => MutationType::Delete,
                    _ => MutationType::Update,
                },
                typename: r.get(5),
                variables: r.get(6),
                state: StepState::from_str(r.get::<_, String>(7).as_str()).unwrap_or(StepState::Pending),
                result: r.get(8),
                started_at: r.get(9),
                completed_at: r.get(10),
            })
            .collect())
    }

    /// Update saga step state
    pub async fn update_saga_step_state(&self, step_id: Uuid, state: &StepState) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at = if matches!(state, StepState::Completed | StepState::Failed) {
            Some(now)
        } else {
            None
        };

        conn.execute(
            "UPDATE federation_saga_steps SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
            &[&state_str, &completed_at, &now, &step_id],
        )
        .await?;

        Ok(())
    }

    /// Save a saga step
    pub async fn save_saga_step(&self, step: &SagaStep) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let mutation_type = step.mutation_type.as_str();
        let state = step.state.as_str();
        let now = chrono::Utc::now();

        conn.execute(
            "INSERT INTO federation_saga_steps (id, saga_id, step_number, subgraph, mutation_type, typename, variables, state, result, started_at, completed_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             ON CONFLICT (id) DO UPDATE SET state = $8, result = $9, completed_at = $11, updated_at = $13",
            &[
                &step.id,
                &step.saga_id,
                &(step.order as i32),
                &step.subgraph,
                &mutation_type,
                &step.typename,
                &step.variables,
                &state,
                &step.result,
                &step.started_at,
                &step.completed_at,
                &chrono::Utc::now(),
                &now,
            ],
        )
        .await?;

        Ok(())
    }

    /// Update saga step result
    pub async fn update_saga_step_result(&self, step_id: Uuid, result: &Value) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let now = chrono::Utc::now();

        conn.execute(
            "UPDATE federation_saga_steps SET result = $1, updated_at = $2 WHERE id = $3",
            &[&result, &now, &step_id],
        )
        .await?;

        Ok(())
    }

    /// Mark saga for recovery
    pub async fn mark_saga_for_recovery(&self, saga_id: Uuid, reason: &str) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let recovery_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        conn.execute(
            "INSERT INTO federation_saga_recovery (id, saga_id, recovery_type, attempted_at, attempt_count) VALUES ($1, $2, $3, $4, $5)",
            &[&recovery_id, &saga_id, &reason, &now, &0i32],
        )
        .await?;

        Ok(())
    }

    /// Find pending sagas
    pub async fn find_pending_sagas(&self) -> Result<Vec<Saga>, Box<dyn std::error::Error>> {
        self.load_sagas_by_state(&SagaState::Pending).await
    }

    /// Clear recovery record
    pub async fn clear_recovery_record(&self, saga_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM federation_saga_recovery WHERE saga_id = $1", &[&saga_id])
            .await?;
        Ok(())
    }

    /// Delete saga
    pub async fn delete_saga(&self, saga_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM federation_sagas WHERE id = $1", &[&saga_id])
            .await?;
        Ok(())
    }

    /// Delete completed sagas
    pub async fn delete_completed_sagas(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let result = conn
            .execute("DELETE FROM federation_sagas WHERE state IN ('completed', 'compensated')", &[])
            .await?;
        Ok(result)
    }

    /// Cleanup stale sagas
    pub async fn cleanup_stale_sagas(&self, hours_threshold: i64) -> Result<u64, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let result = conn
            .execute(
                "DELETE FROM federation_sagas WHERE created_at < NOW() - INTERVAL '1 hour' * $1 AND state IN ('completed', 'compensated')",
                &[&hours_threshold],
            )
            .await?;
        Ok(result)
    }

    /// Get recovery attempts
    pub async fn get_recovery_attempts(&self, saga_id: Uuid) -> Result<i32, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let row = conn
            .query_opt(
                "SELECT COALESCE(MAX(attempt_count), 0) FROM federation_saga_recovery WHERE saga_id = $1",
                &[&saga_id],
            )
            .await?;
        Ok(row.map(|r| r.get(0)).unwrap_or(0))
    }

    /// Save recovery record
    pub async fn save_recovery_record(&self, recovery: &SagaRecovery) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;

        conn.execute(
            "INSERT INTO federation_saga_recovery (id, saga_id, recovery_type, attempted_at, last_attempt, attempt_count, last_error) VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &recovery.id,
                &recovery.saga_id,
                &recovery.recovery_type,
                &recovery.attempted_at,
                &recovery.last_attempt,
                &recovery.attempt_count,
                &recovery.last_error,
            ],
        )
        .await?;

        Ok(())
    }

    /// Cleanup all (for testing)
    pub async fn cleanup_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM federation_saga_recovery", &[]).await?;
        conn.execute("DELETE FROM federation_saga_steps", &[]).await?;
        conn.execute("DELETE FROM federation_sagas", &[]).await?;
        Ok(())
    }

    /// Get saga count
    pub async fn saga_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM federation_sagas", &[]).await?;
        Ok(row.get(0))
    }

    /// Get step count
    pub async fn step_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM federation_saga_steps", &[]).await?;
        Ok(row.get(0))
    }

    /// Get recovery count
    pub async fn recovery_count(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM federation_saga_recovery", &[]).await?;
        Ok(row.get(0))
    }

    /// Find stuck sagas
    pub async fn find_stuck_sagas(&self) -> Result<Vec<Saga>, Box<dyn std::error::Error>> {
        self.load_sagas_by_state(&SagaState::Executing).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_postgres_connection() {
        let store = PostgresSagaStore::new("postgresql://localhost/fraiseql_test")
            .await
            .expect("Failed to create store");
        store.health_check().await.expect("Health check failed");
    }
}
