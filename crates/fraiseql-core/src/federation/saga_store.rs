//! PostgreSQL-backed Saga Store for distributed transaction persistence.
//!
//! This module provides a production-grade persistent store for sagas using PostgreSQL,
//! enabling crash recovery, distributed coordination, and saga tracking across instances.
//!
//! # Architecture
//!
//! The saga store implements the saga pattern for distributed transactions:
//! - **Forward phase**: Execute steps sequentially across subgraphs
//! - **Compensation phase**: Rollback failures by executing inverse operations
//! - **Persistence**: All saga state and steps stored in PostgreSQL
//! - **Recovery**: Background processes recover interrupted sagas on restart
//!
//! # State Machine
//!
//! ```text
//! Pending → Executing → Completed (success)
//!           ↓
//!       Failed → Compensating → Compensated (rolledback)
//! ```
//!
//! # Example
//!
//! ```ignore
//! let store = PostgresSagaStore::new("postgresql://localhost/fraiseql").await?;
//! store.migrate_schema().await?;
//!
//! let saga = Saga {
//!     id: Uuid::new_v4(),
//!     state: SagaState::Pending,
//!     created_at: chrono::Utc::now(),
//!     completed_at: None,
//!     metadata: None,
//! };
//!
//! store.save_saga(&saga).await?;
//! ```

use std::sync::Arc;

use deadpool_postgres::Pool;
use serde_json::Value;
use uuid::Uuid;

/// Error type for saga store operations
#[derive(Debug)]
pub enum SagaStoreError {
    /// Database connection or query error
    Database(String),
    /// Invalid state transition
    InvalidStateTransition { from: String, to: String },
    /// Saga not found
    SagaNotFound(Uuid),
    /// Step not found
    StepNotFound(Uuid),
}

impl std::fmt::Display for SagaStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(msg) => write!(f, "Database error: {}", msg),
            Self::InvalidStateTransition { from, to } => {
                write!(f, "Invalid state transition from {} to {}", from, to)
            },
            Self::SagaNotFound(id) => write!(f, "Saga {} not found", id),
            Self::StepNotFound(id) => write!(f, "Step {} not found", id),
        }
    }
}

impl std::error::Error for SagaStoreError {}

impl From<tokio_postgres::Error> for SagaStoreError {
    fn from(err: tokio_postgres::Error) -> Self {
        Self::Database(err.to_string())
    }
}

impl From<deadpool_postgres::PoolError> for SagaStoreError {
    fn from(err: deadpool_postgres::PoolError) -> Self {
        Self::Database(err.to_string())
    }
}

impl<E> From<deadpool::managed::CreatePoolError<E>> for SagaStoreError
where
    E: std::fmt::Display,
{
    fn from(err: deadpool::managed::CreatePoolError<E>) -> Self {
        Self::Database(format!("Failed to create connection pool: {}", err))
    }
}

pub type Result<T> = std::result::Result<T, SagaStoreError>;

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
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            MutationType::Create => "create",
            MutationType::Update => "update",
            MutationType::Delete => "delete",
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create" => Some(MutationType::Create),
            "update" => Some(MutationType::Update),
            "delete" => Some(MutationType::Delete),
            _ => None,
        }
    }
}

/// Saga struct
#[derive(Debug, Clone)]
pub struct Saga {
    pub id:           Uuid,
    pub state:        SagaState,
    pub created_at:   chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata:     Option<Value>,
}

/// SagaStep struct
#[derive(Debug, Clone)]
pub struct SagaStep {
    pub id:            Uuid,
    pub saga_id:       Uuid,
    pub order:         usize,
    pub subgraph:      String,
    pub mutation_type: MutationType,
    pub typename:      String,
    pub variables:     Value,
    pub state:         StepState,
    pub result:        Option<Value>,
    pub started_at:    Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at:  Option<chrono::DateTime<chrono::Utc>>,
}

/// SagaRecovery struct
#[derive(Debug, Clone)]
pub struct SagaRecovery {
    pub id:            Uuid,
    pub saga_id:       Uuid,
    pub recovery_type: String,
    pub attempted_at:  chrono::DateTime<chrono::Utc>,
    pub last_attempt:  Option<chrono::DateTime<chrono::Utc>>,
    pub attempt_count: i32,
    pub last_error:    Option<String>,
}

/// PostgreSQL-backed Saga Store
///
/// Manages persistent storage of sagas and their execution state using PostgreSQL.
/// Provides crash recovery and distributed coordination across federation instances.
pub struct PostgresSagaStore {
    pool: Arc<Pool>,
}

impl PostgresSagaStore {
    /// Create a new PostgreSQL saga store with default configuration.
    ///
    /// Connects to PostgreSQL and verifies connectivity.
    ///
    /// # Arguments
    ///
    /// * `_connection_string` - PostgreSQL connection string (currently unused, uses default
    ///   config)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if connection fails.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let store = PostgresSagaStore::new("postgresql://localhost/fraiseql").await?;
    /// ```
    pub async fn new(_connection_string: &str) -> Result<Self> {
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

    /// Create database schema and indices if they don't exist
    ///
    /// Uses the trinity pattern with proper table naming:
    /// - `pk_` (BIGINT PRIMARY KEY): Surrogate key for efficient internal joins
    /// - `id` (UUID NOT NULL UNIQUE): Natural key for distributed systems
    /// - `tb_` prefix: Table naming convention for trinity pattern
    /// - Foreign keys use surrogate keys for better performance
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if schema creation fails.
    pub async fn migrate_schema(&self) -> Result<()> {
        let conn = self.pool.get().await?;

        // Create sequence for auto-increment (with tb_ prefix)
        conn.execute(
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_tb_federation_sagas START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_tb_federation_sagas table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_tb_federation_sagas (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_tb_federation_sagas'),
                id UUID NOT NULL UNIQUE,
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

        // Create sequence for steps
        conn.execute(
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_tb_federation_saga_steps START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_tb_federation_saga_steps table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_tb_federation_saga_steps (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_tb_federation_saga_steps'),
                id UUID NOT NULL UNIQUE,
                saga_pk_ BIGINT NOT NULL REFERENCES tb_tb_federation_sagas(pk_) ON DELETE CASCADE,
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

        // Create sequence for recovery
        conn.execute(
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_tb_federation_saga_recovery START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_tb_federation_saga_recovery table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_tb_federation_saga_recovery (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_tb_federation_saga_recovery'),
                id UUID NOT NULL UNIQUE,
                saga_pk_ BIGINT NOT NULL REFERENCES tb_tb_federation_sagas(pk_) ON DELETE CASCADE,
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

        // Create indices (primary composite indices for natural + surrogate keys)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_sagas_id ON tb_tb_federation_sagas(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_sagas_state ON tb_tb_federation_sagas(state)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_sagas_created ON tb_tb_federation_sagas(created_at)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_saga_steps_id ON tb_tb_federation_saga_steps(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_saga_steps_saga_pk ON tb_tb_federation_saga_steps(saga_pk_)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_saga_recovery_id ON tb_tb_federation_saga_recovery(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_tb_federation_saga_recovery_saga_pk ON tb_tb_federation_saga_recovery(saga_pk_)",
            &[],
        )
        .await?;

        Ok(())
    }

    /// Health check - verifies database connectivity
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if connection fails.
    pub async fn health_check(&self) -> Result<()> {
        let _conn = self.pool.get().await?;
        Ok(())
    }

    // Helper functions for row mapping to reduce duplication

    /// Map a database row to a Saga struct
    fn map_saga_row(row: &tokio_postgres::Row) -> Saga {
        Saga {
            id:           row.get(0),
            state:        SagaState::from_str(row.get::<_, String>(1).as_str())
                .unwrap_or(SagaState::Pending),
            created_at:   row.get(2),
            completed_at: row.get(3),
            metadata:     row.get(4),
        }
    }

    /// Map a database row to a SagaStep struct
    fn map_saga_step_row(row: &tokio_postgres::Row) -> SagaStep {
        SagaStep {
            id:            row.get(0),
            saga_id:       row.get(1),
            order:         row.get::<_, i32>(2) as usize,
            subgraph:      row.get(3),
            mutation_type: MutationType::from_str(row.get::<_, String>(4).as_str())
                .unwrap_or(MutationType::Update),
            typename:      row.get(5),
            variables:     row.get(6),
            state:         StepState::from_str(row.get::<_, String>(7).as_str())
                .unwrap_or(StepState::Pending),
            result:        row.get(8),
            started_at:    row.get(9),
            completed_at:  row.get(10),
        }
    }

    /// Save or update a saga
    ///
    /// Uses upsert semantics - inserts if new, updates if exists.
    /// Trinity pattern: surrogate pk_ auto-generated, natural key id (UUID) maintained.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn save_saga(&self, saga: &Saga) -> Result<()> {
        let conn = self.pool.get().await?;
        let state = saga.state.as_str();
        let now = chrono::Utc::now();

        conn.execute(
            "INSERT INTO tb_federation_sagas (id, state, created_at, completed_at, updated_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (id) DO UPDATE SET
                 state = $2, completed_at = $4, updated_at = $5, metadata = $6",
            &[&saga.id, &state, &saga.created_at, &saga.completed_at, &now, &saga.metadata],
        )
        .await?;

        Ok(())
    }

    /// Load a saga by ID
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn load_saga(&self, saga_id: Uuid) -> Result<Option<Saga>> {
        let conn = self.pool.get().await?;

        let row = conn
            .query_opt(
                "SELECT id, state, created_at, completed_at, metadata FROM tb_federation_sagas WHERE id = $1",
                &[&saga_id],
            )
            .await?;

        Ok(row.map(|r| Self::map_saga_row(&r)))
    }

    /// Load all sagas ordered by creation time (newest first)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn load_all_sagas(&self) -> Result<Vec<Saga>> {
        let conn = self.pool.get().await?;

        let rows = conn
            .query(
                "SELECT id, state, created_at, completed_at, metadata FROM tb_federation_sagas ORDER BY created_at DESC",
                &[],
            )
            .await?;

        Ok(rows.into_iter().map(|r| Self::map_saga_row(&r)).collect())
    }

    /// Load sagas filtered by state
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn load_sagas_by_state(&self, state: &SagaState) -> Result<Vec<Saga>> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();

        let rows = conn
            .query(
                "SELECT id, state, created_at, completed_at, metadata FROM tb_federation_sagas WHERE state = $1 ORDER BY created_at DESC",
                &[&state_str],
            )
            .await?;

        Ok(rows.into_iter().map(|r| Self::map_saga_row(&r)).collect())
    }

    /// Update saga state and automatically set completion time for terminal states
    ///
    /// Terminal states (Completed, Compensated) automatically receive completed_at timestamp.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the update fails.
    pub async fn update_saga_state(&self, saga_id: Uuid, state: &SagaState) -> Result<()> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at = if matches!(state, SagaState::Completed | SagaState::Compensated) {
            Some(now)
        } else {
            None
        };

        conn.execute(
            "UPDATE tb_federation_sagas SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
            &[&state_str, &completed_at, &now, &saga_id],
        )
        .await?;

        Ok(())
    }

    /// Load a saga step by ID
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn load_saga_step(&self, step_id: Uuid) -> Result<Option<SagaStep>> {
        let conn = self.pool.get().await?;

        let row = conn
            .query_opt(
                "SELECT fss.id, fs.id as saga_id, fss.step_number, fss.subgraph, fss.mutation_type, fss.typename, fss.variables, fss.state, fss.result, fss.started_at, fss.completed_at
                 FROM tb_federation_saga_steps fss
                 INNER JOIN tb_federation_sagas fs ON fss.saga_pk_ = fs.pk_
                 WHERE fss.id = $1",
                &[&step_id],
            )
            .await?;

        Ok(row.map(|r| Self::map_saga_step_row(&r)))
    }

    /// Load all saga steps for a saga, ordered by step number (Trinity pattern with JOIN)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn load_saga_steps(&self, saga_id: Uuid) -> Result<Vec<SagaStep>> {
        let conn = self.pool.get().await?;

        let rows = conn
            .query(
                "SELECT fss.id, fs.id as saga_id, fss.step_number, fss.subgraph, fss.mutation_type, fss.typename, fss.variables, fss.state, fss.result, fss.started_at, fss.completed_at
                 FROM tb_federation_saga_steps fss
                 INNER JOIN tb_federation_sagas fs ON fss.saga_pk_ = fs.pk_
                 WHERE fs.id = $1
                 ORDER BY fss.step_number ASC",
                &[&saga_id],
            )
            .await?;

        Ok(rows.into_iter().map(|r| Self::map_saga_step_row(&r)).collect())
    }

    /// Update saga step state and automatically set completion time for terminal states
    ///
    /// Terminal states (Completed, Failed) automatically receive completed_at timestamp.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the update fails.
    pub async fn update_saga_step_state(&self, step_id: Uuid, state: &StepState) -> Result<()> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at = if matches!(state, StepState::Completed | StepState::Failed) {
            Some(now)
        } else {
            None
        };

        conn.execute(
            "UPDATE tb_federation_saga_steps SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
            &[&state_str, &completed_at, &now, &step_id],
        )
        .await?;

        Ok(())
    }

    /// Save or update a saga step
    ///
    /// Uses upsert semantics - inserts if new, updates if exists.
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn save_saga_step(&self, step: &SagaStep) -> Result<()> {
        let conn = self.pool.get().await?;
        let mutation_type = step.mutation_type.as_str();
        let state = step.state.as_str();
        let now = chrono::Utc::now();

        // Note: step.order is casted to i32 for PostgreSQL storage.
        // In practice, sagas rarely exceed 2 billion steps, so this is safe.
        #[allow(clippy::cast_possible_wrap)]
        let step_number = step.order as i32;

        // Use subquery to convert saga natural key (UUID) to surrogate key (BIGINT) for foreign key
        conn.execute(
            "INSERT INTO tb_federation_saga_steps (id, saga_pk_, step_number, subgraph, mutation_type, typename, variables, state, result, started_at, completed_at, created_at, updated_at)
             SELECT $1, fs.pk_, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13
             FROM tb_federation_sagas fs
             WHERE fs.id = $2
             ON CONFLICT (id) DO UPDATE SET state = $8, result = $9, completed_at = $11, updated_at = $13",
            &[
                &step.id,
                &step.saga_id,  // Used in subquery to find saga_pk_
                &step_number,
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

    /// Update the result of a completed saga step
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the update fails.
    pub async fn update_saga_step_result(&self, step_id: Uuid, result: &Value) -> Result<()> {
        let conn = self.pool.get().await?;
        let now = chrono::Utc::now();

        conn.execute(
            "UPDATE tb_federation_saga_steps SET result = $1, updated_at = $2 WHERE id = $3",
            &[&result, &now, &step_id],
        )
        .await?;

        Ok(())
    }

    /// Mark a saga for recovery
    ///
    /// Creates a recovery record tracking an attempt to recover a failed saga.
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn mark_saga_for_recovery(&self, saga_id: Uuid, reason: &str) -> Result<()> {
        let conn = self.pool.get().await?;
        let recovery_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        // Use subquery to convert saga natural key to surrogate key
        conn.execute(
            "INSERT INTO tb_federation_saga_recovery (id, saga_pk_, recovery_type, attempted_at, attempt_count)
             SELECT $1, fs.pk_, $3, $4, $5
             FROM tb_federation_sagas fs
             WHERE fs.id = $2",
            &[&recovery_id, &saga_id, &reason, &now, &0i32],
        )
        .await?;

        Ok(())
    }

    /// Find all pending sagas (not yet started)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn find_pending_sagas(&self) -> Result<Vec<Saga>> {
        self.load_sagas_by_state(&SagaState::Pending).await
    }

    /// Clear recovery record for a saga
    ///
    /// Trinity pattern: uses subquery to convert saga natural key to surrogate key.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn clear_recovery_record(&self, saga_id: Uuid) -> Result<()> {
        let conn = self.pool.get().await?;
        conn.execute(
            "DELETE FROM tb_federation_saga_recovery
             WHERE saga_pk_ = (SELECT pk_ FROM tb_federation_sagas WHERE id = $1)",
            &[&saga_id],
        )
        .await?;
        Ok(())
    }

    /// Delete a saga and all associated steps and recovery records
    ///
    /// CASCADE constraints ensure related records are deleted.
    /// Uses natural key (UUID) for deletion.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn delete_saga(&self, saga_id: Uuid) -> Result<()> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM tb_federation_sagas WHERE id = $1", &[&saga_id])
            .await?;
        Ok(())
    }

    /// Delete all completed and compensated sagas
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    ///
    /// # Returns
    ///
    /// Number of sagas deleted.
    pub async fn delete_completed_sagas(&self) -> Result<u64> {
        let conn = self.pool.get().await?;
        let result = conn
            .execute(
                "DELETE FROM tb_federation_sagas WHERE state IN ('completed', 'compensated')",
                &[],
            )
            .await?;
        Ok(result)
    }

    /// Delete sagas older than the specified threshold that are in a terminal state
    ///
    /// # Arguments
    ///
    /// * `hours_threshold` - Delete sagas created more than this many hours ago
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    ///
    /// # Returns
    ///
    /// Number of sagas deleted.
    pub async fn cleanup_stale_sagas(&self, hours_threshold: i64) -> Result<u64> {
        let conn = self.pool.get().await?;
        let result = conn
            .execute(
                "DELETE FROM tb_federation_sagas WHERE created_at < NOW() - INTERVAL '1 hour' * $1 AND state IN ('completed', 'compensated')",
                &[&hours_threshold],
            )
            .await?;
        Ok(result)
    }

    /// Get the maximum recovery attempt count for a saga
    ///
    /// Trinity pattern: uses subquery to convert saga natural key to surrogate key.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn get_recovery_attempts(&self, saga_id: Uuid) -> Result<i32> {
        let conn = self.pool.get().await?;
        let row = conn
            .query_opt(
                "SELECT COALESCE(MAX(attempt_count), 0) FROM tb_federation_saga_recovery
                 WHERE saga_pk_ = (SELECT pk_ FROM tb_federation_sagas WHERE id = $1)",
                &[&saga_id],
            )
            .await?;
        Ok(row.map(|r| r.get(0)).unwrap_or(0))
    }

    /// Save a saga recovery record
    ///
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn save_recovery_record(&self, recovery: &SagaRecovery) -> Result<()> {
        let conn = self.pool.get().await?;

        // Use subquery to convert saga natural key to surrogate key
        conn.execute(
            "INSERT INTO tb_federation_saga_recovery (id, saga_pk_, recovery_type, attempted_at, last_attempt, attempt_count, last_error)
             SELECT $1, fs.pk_, $3, $4, $5, $6, $7
             FROM tb_federation_sagas fs
             WHERE fs.id = $2",
            &[
                &recovery.id,
                &recovery.saga_id,  // Used in subquery
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

    /// Delete all sagas, steps, and recovery records (for testing)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    pub async fn cleanup_all(&self) -> Result<()> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM tb_federation_saga_recovery", &[]).await?;
        conn.execute("DELETE FROM tb_federation_saga_steps", &[]).await?;
        conn.execute("DELETE FROM tb_federation_sagas", &[]).await?;
        Ok(())
    }

    /// Get total number of sagas in the database
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn saga_count(&self) -> Result<i64> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM tb_federation_sagas", &[]).await?;
        Ok(row.get(0))
    }

    /// Get total number of saga steps in the database
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn step_count(&self) -> Result<i64> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM tb_federation_saga_steps", &[]).await?;
        Ok(row.get(0))
    }

    /// Get total number of saga recovery records in the database
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn recovery_count(&self) -> Result<i64> {
        let conn = self.pool.get().await?;
        let row = conn.query_one("SELECT COUNT(*) FROM tb_federation_saga_recovery", &[]).await?;
        Ok(row.get(0))
    }

    /// Find all stuck sagas (in executing state that may have crashed)
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn find_stuck_sagas(&self) -> Result<Vec<Saga>> {
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
