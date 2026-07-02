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
//! ```text
//! // Requires: distributed saga infrastructure (PostgreSQL + message broker).
//! // See: tests/integration/ for runnable examples.
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
#[non_exhaustive]
pub enum SagaStoreError {
    /// Database connection or query error
    Database(String),
    /// Invalid state transition
    InvalidStateTransition {
        /// Current state of the saga.
        from: String,
        /// Attempted target state.
        to:   String,
    },
    /// Saga not found
    SagaNotFound(Uuid),
    /// Step not found
    StepNotFound(Uuid),
    /// A stored value could not be parsed into a known enum variant.
    ///
    /// Raised instead of silently coercing an unrecognised state/type string to a
    /// default — coercion can re-execute completed work (M-saga-store-defaults).
    CorruptStoredValue {
        /// The column whose value failed to parse.
        column: String,
        /// The unrecognised value read from the database.
        value:  String,
    },
    /// The requested saga operation is not implemented.
    ///
    /// Distributed saga execution (forward, compensation, recovery, coordination)
    /// has no real transport wired; the paths return this rather than fabricate
    /// and persist success (H32/H33).
    NotImplemented {
        /// The operation that is not implemented.
        operation: String,
    },
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
            Self::CorruptStoredValue { column, value } => {
                write!(f, "Corrupt stored value in column '{column}': {value:?}")
            },
            Self::NotImplemented { operation } => {
                write!(f, "Saga operation not implemented: {operation}")
            },
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

/// Convenience `Result` alias for saga store operations.
pub type Result<T> = std::result::Result<T, SagaStoreError>;

/// Lifecycle state of a distributed saga.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SagaState {
    /// Saga is created but has not started executing.
    Pending,
    /// At least one step is currently running.
    Executing,
    /// All steps finished successfully.
    Completed,
    /// One or more steps failed; compensation may be needed.
    Failed,
    /// Compensation steps are running to undo partial work.
    Compensating,
    /// All compensation steps have finished.
    Compensated,
    /// The saga was cancelled by an operator; any completed steps were rolled
    /// back before it reached this terminal state (#429).
    Cancelled,
}

impl SagaState {
    /// Return a lowercase string identifier for this state.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            SagaState::Pending => "pending",
            SagaState::Executing => "executing",
            SagaState::Completed => "completed",
            SagaState::Failed => "failed",
            SagaState::Compensating => "compensating",
            SagaState::Compensated => "compensated",
            SagaState::Cancelled => "cancelled",
        }
    }

    /// Parse a `SagaState` from its lowercase string representation.
    ///
    /// Returns `None` if `s` does not match a known state name.
    #[allow(clippy::should_implement_trait)] // Reason: infallible Option-returning conversion, not a full FromStr implementation
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(SagaState::Pending),
            "executing" => Some(SagaState::Executing),
            "completed" => Some(SagaState::Completed),
            "failed" => Some(SagaState::Failed),
            "compensating" => Some(SagaState::Compensating),
            "compensated" => Some(SagaState::Compensated),
            "cancelled" => Some(SagaState::Cancelled),
            _ => None,
        }
    }
}

/// Lifecycle state of a single saga step.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StepState {
    /// Step is queued but has not started.
    Pending,
    /// Step is currently running.
    Executing,
    /// Step finished successfully.
    Completed,
    /// Step encountered an error.
    Failed,
    /// Step was rolled back by a successful compensation mutation.
    ///
    /// Distinct from `Completed`: the forward mutation ran, then its inverse was
    /// executed during the compensation phase (#429). A step only reaches this
    /// state from `Completed`.
    Compensated,
}

impl StepState {
    /// Return a lowercase string identifier for this step state.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            StepState::Pending => "pending",
            StepState::Executing => "executing",
            StepState::Completed => "completed",
            StepState::Failed => "failed",
            StepState::Compensated => "compensated",
        }
    }

    /// Parse a `StepState` from its lowercase string representation.
    ///
    /// Returns `None` if `s` does not match a known state name.
    #[allow(clippy::should_implement_trait)] // Reason: infallible Option-returning conversion, not a full FromStr implementation
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(StepState::Pending),
            "executing" => Some(StepState::Executing),
            "completed" => Some(StepState::Completed),
            "failed" => Some(StepState::Failed),
            "compensated" => Some(StepState::Compensated),
            _ => None,
        }
    }
}

/// The kind of GraphQL mutation a saga step executes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutationType {
    /// Step creates a new entity.
    Create,
    /// Step updates an existing entity.
    Update,
    /// Step deletes an entity.
    Delete,
}

impl MutationType {
    /// Convert to string representation
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            MutationType::Create => "create",
            MutationType::Update => "update",
            MutationType::Delete => "delete",
        }
    }

    /// Parse from string representation
    #[allow(clippy::should_implement_trait)] // Reason: infallible Option-returning conversion, not a full FromStr implementation
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create" => Some(MutationType::Create),
            "update" => Some(MutationType::Update),
            "delete" => Some(MutationType::Delete),
            _ => None,
        }
    }
}

/// A distributed saga instance managing a multi-step cross-subgraph mutation.
#[derive(Debug, Clone)]
pub struct Saga {
    /// Unique identifier for this saga.
    pub id:           Uuid,
    /// Current lifecycle state.
    pub state:        SagaState,
    /// Timestamp when the saga was created.
    pub created_at:   chrono::DateTime<chrono::Utc>,
    /// Timestamp when the saga reached a terminal state, if any.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Arbitrary JSON metadata attached to the saga.
    pub metadata:     Option<Value>,
}

/// A cross-subgraph field a saga step depends on (Apollo-Federation `@requires`).
///
/// Before the step's mutation runs, the coordinator fetches this field from the
/// owning subgraph's `_entities` endpoint and merges it into the step's mutation
/// variables. This lets a step whose input depends on data owned by another
/// subgraph (e.g. a `chargeCard` step that `@requires product.price` from the
/// catalog subgraph) run correctly in a distributed saga.
///
/// Sagas are *runtime-constructed* (via [`crate::saga_coordinator::SagaStep`]),
/// not schema-authored, so the application building the saga supplies these specs
/// directly. Each is persisted as JSONB alongside the step and resolved before
/// dispatch — a field that cannot be resolved fails the step *before* its mutation
/// runs, so a mutation never executes with missing inputs (#429).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::derive_partial_eq_without_eq)]
// Reason: `key` is a serde_json::Value (not `Eq` — a JSON number may be a float),
// so `Eq` cannot be derived; `PartialEq` is enough for round-trip assertions.
pub struct RequiredField {
    /// Subgraph that owns the entity carrying this field. Must be a registered
    /// peer of the coordinator; an unregistered subgraph is rejected at
    /// `create_saga` time (fail-loud-at-setup).
    pub subgraph:   String,
    /// GraphQL type name of the entity to resolve (e.g. `Product`).
    pub typename:   String,
    /// Key fields identifying the entity — the federation representation, a JSON
    /// object such as `{"id": "product-1"}`. Combined with `typename` to build the
    /// `_entities` representation sent to the owning subgraph.
    pub key:        Value,
    /// Field to read from the resolved entity. A dotted path (e.g. `price` or
    /// `dimensions.weight`) is traversed into the entity JSON; the first segment is
    /// what the `_entities` selection requests.
    pub field_path: String,
    /// Mutation variable to populate with the fetched value (e.g. `price`).
    pub target_var: String,
}

/// A single step within a distributed saga.
#[derive(Debug, Clone)]
pub struct SagaStep {
    /// Unique identifier for this step.
    pub id:                     Uuid,
    /// Parent saga this step belongs to.
    pub saga_id:                Uuid,
    /// Zero-based execution order within the saga.
    pub order:                  usize,
    /// Subgraph service name that owns this step.
    pub subgraph:               String,
    /// Kind of mutation this step performs.
    pub mutation_type:          MutationType,
    /// Full GraphQL mutation operation name (e.g. `createOrder`), if known. The
    /// store also records the coarse [`MutationType`] *kind*; this carries the
    /// exact name a remote subgraph expects. `None`/empty (pre-migration rows or
    /// steps created without a name) falls back to the kind's canonical verb.
    pub mutation_name:          Option<String>,
    /// GraphQL type name the mutation targets.
    pub typename:               String,
    /// Input variables for the mutation.
    pub variables:              Value,
    /// Current lifecycle state of this step.
    pub state:                  StepState,
    /// Mutation result payload, if the step has completed.
    pub result:                 Option<Value>,
    /// Timestamp when execution began, if started.
    pub started_at:             Option<chrono::DateTime<chrono::Utc>>,
    /// Timestamp when execution finished, if finished.
    pub completed_at:           Option<chrono::DateTime<chrono::Utc>>,
    /// Name of the compensation (inverse) mutation for this step, if one is
    /// registered. `None`/empty means the step has no rollback and cannot be
    /// compensated. Distinct from [`crate::saga_coordinator::SagaStep`], whose
    /// non-optional field carries the same intent at the coordinator layer.
    pub compensation_mutation:  Option<String>,
    /// Input variables for the compensation mutation, if registered. When absent
    /// the compensator falls back to the step's forward `variables` (they carry
    /// the entity key needed by an inverse delete/update).
    pub compensation_variables: Option<Value>,
    /// Cross-subgraph `@requires` fields to pre-fetch and merge into `variables`
    /// before this step's mutation runs (empty = none). Persisted as JSONB; a
    /// NULL column (pre-migration rows or steps created without any) loads as an
    /// empty vector. See [`RequiredField`].
    pub required_fields:        Vec<RequiredField>,
}

/// A crash-recovery record for a saga that could not complete normally.
#[derive(Debug, Clone)]
pub struct SagaRecovery {
    /// Unique identifier for this recovery record.
    pub id:            Uuid,
    /// Saga being recovered.
    pub saga_id:       Uuid,
    /// Strategy used for this recovery attempt (e.g. `"retry"`, `"compensate"`).
    pub recovery_type: String,
    /// When the first recovery attempt was initiated.
    pub attempted_at:  chrono::DateTime<chrono::Utc>,
    /// Timestamp of the most recent attempt, if more than one.
    pub last_attempt:  Option<chrono::DateTime<chrono::Utc>>,
    /// Number of recovery attempts made so far.
    pub attempt_count: i32,
    /// Error message from the last failed attempt, if any.
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
    /// Trinity-convention table name for the saga recovery table.
    pub const TABLE_RECOVERY: &'static str = "tb_federation_saga_recovery";
    /// Trinity-convention table name for the main sagas table.
    pub const TABLE_SAGAS: &'static str = "tb_federation_sagas";
    /// Trinity-convention table name for the saga steps table.
    pub const TABLE_STEPS: &'static str = "tb_federation_saga_steps";

    /// Create a new PostgreSQL saga store.
    ///
    /// Connects to PostgreSQL and verifies connectivity.
    ///
    /// # Arguments
    ///
    /// * `connection_string` - PostgreSQL connection URL, e.g.
    ///   `postgresql://user:password@host:port/database`
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if connection fails.
    ///
    /// # Example
    ///
    /// ```text
    /// // Requires: distributed saga infrastructure (PostgreSQL + message broker).
    /// // See: tests/integration/ for runnable examples.
    /// let store = PostgresSagaStore::new("postgresql://localhost/fraiseql").await?;
    /// ```
    pub async fn new(connection_string: &str) -> Result<Self> {
        let cfg = deadpool_postgres::Config {
            url: Some(connection_string.to_string()),
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
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_federation_sagas START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_federation_sagas table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_federation_sagas (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_federation_sagas'),
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

        // Recovery lease (nullable, added idempotently): `claim_stuck_sagas` marks a
        // claimed saga with the recovering worker's id and a lease expiry so
        // concurrent recovery workers never double-drive the same saga (FOR UPDATE
        // SKIP LOCKED). A NULL/expired lease means the saga is claimable.
        conn.execute(
            "
            ALTER TABLE tb_federation_sagas
                ADD COLUMN IF NOT EXISTS recovery_worker_id UUID,
                ADD COLUMN IF NOT EXISTS recovery_lease_expires_at TIMESTAMPTZ
            ",
            &[],
        )
        .await?;

        // Create sequence for steps
        conn.execute(
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_federation_saga_steps START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_federation_saga_steps table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_federation_saga_steps (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_federation_saga_steps'),
                id UUID NOT NULL UNIQUE,
                saga_pk_ BIGINT NOT NULL REFERENCES tb_federation_sagas(pk_) ON DELETE CASCADE,
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

        // Compensation metadata (nullable): pre-existing step rows predate
        // compensation, so both columns are added idempotently and may be NULL.
        // A step with a NULL/empty compensation_mutation has no registered
        // rollback and is skipped by the compensator (best-effort, #429).
        // `mutation_name` (also nullable, added idempotently) carries the full
        // remote operation name; NULL rows fall back to the mutation-kind verb.
        // `required_fields` (nullable JSONB) carries the step's cross-subgraph
        // `@requires` specs; NULL rows have none and skip pre-fetch.
        conn.execute(
            "
            ALTER TABLE tb_federation_saga_steps
                ADD COLUMN IF NOT EXISTS compensation_mutation TEXT,
                ADD COLUMN IF NOT EXISTS compensation_variables JSONB,
                ADD COLUMN IF NOT EXISTS mutation_name TEXT,
                ADD COLUMN IF NOT EXISTS required_fields JSONB
            ",
            &[],
        )
        .await?;

        // Create sequence for recovery
        conn.execute(
            "CREATE SEQUENCE IF NOT EXISTS seq_tb_federation_saga_recovery START 1 INCREMENT 1",
            &[],
        )
        .await?;

        // Create tb_federation_saga_recovery table (trinity pattern)
        conn.execute(
            "
            CREATE TABLE IF NOT EXISTS tb_federation_saga_recovery (
                pk_ BIGINT PRIMARY KEY DEFAULT nextval('seq_tb_federation_saga_recovery'),
                id UUID NOT NULL UNIQUE,
                saga_pk_ BIGINT NOT NULL REFERENCES tb_federation_sagas(pk_) ON DELETE CASCADE,
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
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_sagas_id ON tb_federation_sagas(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_sagas_state ON tb_federation_sagas(state)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_sagas_created ON tb_federation_sagas(created_at)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_saga_steps_id ON tb_federation_saga_steps(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_saga_steps_saga_pk ON tb_federation_saga_steps(saga_pk_)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_saga_recovery_id ON tb_federation_saga_recovery(id)",
            &[],
        )
        .await?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tb_federation_saga_recovery_saga_pk ON tb_federation_saga_recovery(saga_pk_)",
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

    /// Map a database row to a Saga struct.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::CorruptStoredValue`] if the stored `state`
    /// column does not parse into a known [`SagaState`] — coercing it to a
    /// default could re-execute completed work (M-saga-store-defaults).
    fn map_saga_row(row: &tokio_postgres::Row) -> Result<Saga> {
        let state_raw: String = row.get(1);
        let state = SagaState::from_str(state_raw.as_str()).ok_or_else(|| {
            SagaStoreError::CorruptStoredValue {
                column: "state".into(),
                value:  state_raw,
            }
        })?;

        Ok(Saga {
            id: row.get(0),
            state,
            created_at: row.get(2),
            completed_at: row.get(3),
            metadata: row.get(4),
        })
    }

    /// Map a database row to a [`SagaStep`] struct.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::CorruptStoredValue`] if the stored
    /// `mutation_type` or `state` column does not parse into a known enum
    /// variant — coercing either to a default could mis-compensate or
    /// re-execute completed work (M-saga-store-defaults).
    fn map_saga_step_row(row: &tokio_postgres::Row) -> Result<SagaStep> {
        let mutation_type_raw: String = row.get(4);
        let mutation_type =
            MutationType::from_str(mutation_type_raw.as_str()).ok_or_else(|| {
                SagaStoreError::CorruptStoredValue {
                    column: "mutation_type".into(),
                    value:  mutation_type_raw,
                }
            })?;

        let state_raw: String = row.get(7);
        let state = StepState::from_str(state_raw.as_str()).ok_or_else(|| {
            SagaStoreError::CorruptStoredValue {
                column: "state".into(),
                value:  state_raw,
            }
        })?;

        // A NULL column (pre-migration rows, or steps with no @requires) loads as
        // an empty vector; a stored-but-unparseable value fails loud rather than
        // silently dropping the step's inputs (M-saga-store-defaults).
        let required_fields = row
            .get::<_, Option<Value>>(14)
            .map(serde_json::from_value::<Vec<RequiredField>>)
            .transpose()
            .map_err(|e| SagaStoreError::CorruptStoredValue {
                column: "required_fields".into(),
                value:  e.to_string(),
            })?
            .unwrap_or_default();

        Ok(SagaStep {
            id: row.get(0),
            saga_id: row.get(1),
            #[allow(clippy::cast_sign_loss)] // Reason: step_order is always non-negative from DB
            order: row.get::<_, i32>(2) as usize,
            subgraph: row.get(3),
            mutation_type,
            mutation_name: row.get(13),
            typename: row.get(5),
            variables: row.get(6),
            state,
            result: row.get(8),
            started_at: row.get(9),
            completed_at: row.get(10),
            compensation_mutation: row.get(11),
            compensation_variables: row.get(12),
            required_fields,
        })
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

        row.map(|r| Self::map_saga_row(&r)).transpose()
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

        rows.iter().map(Self::map_saga_row).collect()
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

        rows.iter().map(Self::map_saga_row).collect()
    }

    /// Update saga state and automatically set completion time for terminal states
    ///
    /// Terminal states (Completed, Compensated, Cancelled) automatically receive a
    /// `completed_at` timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if no saga with `saga_id` exists
    /// (the `UPDATE` matched zero rows, M-saga-rowcounts), or
    /// [`SagaStoreError::Database`] if the update fails.
    pub async fn update_saga_state(&self, saga_id: Uuid, state: &SagaState) -> Result<()> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at = if matches!(
            state,
            SagaState::Completed | SagaState::Compensated | SagaState::Cancelled
        ) {
            Some(now)
        } else {
            None
        };

        let affected = conn
            .execute(
                "UPDATE tb_federation_sagas SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
                &[&state_str, &completed_at, &now, &saga_id],
            )
            .await?;

        if affected == 0 {
            return Err(SagaStoreError::SagaNotFound(saga_id));
        }

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
                "SELECT fss.id, fs.id as saga_id, fss.step_number, fss.subgraph, fss.mutation_type, fss.typename, fss.variables, fss.state, fss.result, fss.started_at, fss.completed_at, fss.compensation_mutation, fss.compensation_variables, fss.mutation_name, fss.required_fields
                 FROM tb_federation_saga_steps fss
                 INNER JOIN tb_federation_sagas fs ON fss.saga_pk_ = fs.pk_
                 WHERE fss.id = $1",
                &[&step_id],
            )
            .await?;

        row.map(|r| Self::map_saga_step_row(&r)).transpose()
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
                "SELECT fss.id, fs.id as saga_id, fss.step_number, fss.subgraph, fss.mutation_type, fss.typename, fss.variables, fss.state, fss.result, fss.started_at, fss.completed_at, fss.compensation_mutation, fss.compensation_variables, fss.mutation_name, fss.required_fields
                 FROM tb_federation_saga_steps fss
                 INNER JOIN tb_federation_sagas fs ON fss.saga_pk_ = fs.pk_
                 WHERE fs.id = $1
                 ORDER BY fss.step_number ASC",
                &[&saga_id],
            )
            .await?;

        rows.iter().map(Self::map_saga_step_row).collect()
    }

    /// Update saga step state and automatically set completion time for terminal states
    ///
    /// Terminal states (Completed, Failed, Compensated) automatically receive
    /// `completed_at` timestamp.
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::StepNotFound`] if no step with `step_id` exists
    /// (the `UPDATE` matched zero rows, M-saga-rowcounts), or
    /// [`SagaStoreError::Database`] if the update fails.
    pub async fn update_saga_step_state(&self, step_id: Uuid, state: &StepState) -> Result<()> {
        let conn = self.pool.get().await?;
        let state_str = state.as_str();
        let now = chrono::Utc::now();

        let completed_at =
            if matches!(state, StepState::Completed | StepState::Failed | StepState::Compensated) {
                Some(now)
            } else {
                None
            };

        let affected = conn
            .execute(
                "UPDATE tb_federation_saga_steps SET state = $1, completed_at = $2, updated_at = $3 WHERE id = $4",
                &[&state_str, &completed_at, &now, &step_id],
            )
            .await?;

        if affected == 0 {
            return Err(SagaStoreError::StepNotFound(step_id));
        }

        Ok(())
    }

    /// Save or update a saga step
    ///
    /// Uses upsert semantics - inserts if new, updates if exists.
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if the parent saga
    /// (`step.saga_id`) does not exist: the foreign-key subquery then yields no
    /// row and the write affects zero rows (M-saga-rowcounts). Returns
    /// [`SagaStoreError::Database`] if the operation otherwise fails.
    pub async fn save_saga_step(&self, step: &SagaStep) -> Result<()> {
        let conn = self.pool.get().await?;
        let mutation_type = step.mutation_type.as_str();
        let state = step.state.as_str();
        let now = chrono::Utc::now();

        // Note: step.order is casted to i32 for PostgreSQL storage.
        // In practice, sagas rarely exceed 2 billion steps, so this is safe.
        #[allow(clippy::cast_possible_wrap)] // Reason: value is non-negative; wrap cannot occur in practice
        #[allow(clippy::cast_possible_truncation)]
        // Reason: step count bounded well below i32::MAX
        let step_number = step.order as i32;

        // Serialize the @requires specs to JSONB; an empty list is stored NULL so
        // it round-trips to an empty vector and matches pre-migration rows.
        let required_fields: Option<Value> = if step.required_fields.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&step.required_fields).map_err(|e| {
                SagaStoreError::Database(format!("failed to serialize required_fields: {e}"))
            })?)
        };

        // Use subquery to convert saga natural key (UUID) to surrogate key (BIGINT) for foreign key
        // compensation_mutation / compensation_variables / mutation_name /
        // required_fields are part of the immutable step definition, so — like
        // variables/subgraph — they are written on INSERT but not touched by the
        // ON CONFLICT update path (which only advances runtime state/result/completed_at).
        let affected = conn
            .execute(
                "INSERT INTO tb_federation_saga_steps (id, saga_pk_, step_number, subgraph, mutation_type, typename, variables, state, result, started_at, completed_at, created_at, updated_at, compensation_mutation, compensation_variables, mutation_name, required_fields)
             SELECT $1, fs.pk_, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17
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
                    &step.compensation_mutation,
                    &step.compensation_variables,
                    &step.mutation_name,
                    &required_fields,
                ],
            )
            .await?;

        // Zero rows means the FK subquery found no parent saga: a legitimate
        // ON CONFLICT update still affects exactly one row.
        if affected == 0 {
            return Err(SagaStoreError::SagaNotFound(step.saga_id));
        }

        Ok(())
    }

    /// Update the result of a completed saga step
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::StepNotFound`] if no step with `step_id` exists
    /// (the `UPDATE` matched zero rows, M-saga-rowcounts), or
    /// [`SagaStoreError::Database`] if the update fails.
    pub async fn update_saga_step_result(&self, step_id: Uuid, result: &Value) -> Result<()> {
        let conn = self.pool.get().await?;
        let now = chrono::Utc::now();

        let affected = conn
            .execute(
                "UPDATE tb_federation_saga_steps SET result = $1, updated_at = $2 WHERE id = $3",
                &[&result, &now, &step_id],
            )
            .await?;

        if affected == 0 {
            return Err(SagaStoreError::StepNotFound(step_id));
        }

        Ok(())
    }

    /// Mark a saga for recovery
    ///
    /// Creates a recovery record tracking an attempt to recover a failed saga.
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if no saga with `saga_id` exists:
    /// the foreign-key subquery then yields no row and the insert affects zero
    /// rows (M-saga-rowcounts). Returns [`SagaStoreError::Database`] if the
    /// operation otherwise fails.
    pub async fn mark_saga_for_recovery(&self, saga_id: Uuid, reason: &str) -> Result<()> {
        let conn = self.pool.get().await?;
        let recovery_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        // Use subquery to convert saga natural key to surrogate key
        let affected = conn
            .execute(
                "INSERT INTO tb_federation_saga_recovery (id, saga_pk_, recovery_type, attempted_at, attempt_count)
             SELECT $1, fs.pk_, $3, $4, $5
             FROM tb_federation_sagas fs
             WHERE fs.id = $2",
                &[&recovery_id, &saga_id, &reason, &now, &0i32],
            )
            .await?;

        if affected == 0 {
            return Err(SagaStoreError::SagaNotFound(saga_id));
        }

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
        Ok(row.map_or(0, |r| r.get(0)))
    }

    /// Save a saga recovery record
    ///
    /// Trinity pattern: subquery converts saga natural key (UUID) to surrogate key (BIGINT).
    ///
    /// # Errors
    ///
    /// Returns [`SagaStoreError::SagaNotFound`] if no saga with
    /// `recovery.saga_id` exists: the foreign-key subquery then yields no row
    /// and the insert affects zero rows (M-saga-rowcounts). Returns
    /// [`SagaStoreError::Database`] if the operation otherwise fails.
    pub async fn save_recovery_record(&self, recovery: &SagaRecovery) -> Result<()> {
        let conn = self.pool.get().await?;

        // Use subquery to convert saga natural key to surrogate key
        let affected = conn
            .execute(
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

        if affected == 0 {
            return Err(SagaStoreError::SagaNotFound(recovery.saga_id));
        }

        Ok(())
    }

    /// Delete all sagas, steps, and recovery records.
    ///
    /// This is a crate-internal operation. For admin/CLI use, future callers
    /// must present an `AdminCredential` (following the `admin_token` surface
    /// already established in `fraiseql-server`). For test cleanup, use
    /// `cleanup_all_for_testing`.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    #[allow(dead_code)] // Reason: used only by cleanup_all_for_testing (cfg test/test-utils)
    pub(crate) async fn cleanup_all(&self) -> Result<()> {
        let conn = self.pool.get().await?;
        conn.execute("DELETE FROM tb_federation_saga_recovery", &[]).await?;
        conn.execute("DELETE FROM tb_federation_saga_steps", &[]).await?;
        conn.execute("DELETE FROM tb_federation_sagas", &[]).await?;
        Ok(())
    }

    /// Delete all sagas, steps, and recovery records — test-only wrapper.
    ///
    /// Available under `cfg(any(test, feature = "testing"))` only.
    /// Production code must use the crate-internal `cleanup_all`.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the operation fails.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cleanup_all_for_testing(&self) -> Result<()> {
        self.cleanup_all().await
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

    /// Atomically claim up to `limit` stuck (`Executing`) sagas for recovery,
    /// leasing each to `worker_id` for `lease_secs` seconds.
    ///
    /// Concurrency-safe by design: the claim is a single `UPDATE … WHERE pk_ IN
    /// (SELECT … FOR UPDATE SKIP LOCKED)` statement, so two recovery workers running
    /// this at the same time claim **disjoint** sets of sagas and never
    /// double-drive one (the #429 recovery requirement). Only sagas whose lease is
    /// `NULL` or already expired are claimable, so a crashed worker's claim is
    /// automatically reclaimable once its lease lapses. The claimed rows are
    /// returned (marked with `worker_id` + a fresh lease) for this worker to
    /// re-drive.
    ///
    /// # Errors
    ///
    /// Returns `SagaStoreError::Database` if the query fails.
    pub async fn claim_stuck_sagas(
        &self,
        worker_id: Uuid,
        lease_secs: i64,
        limit: i64,
    ) -> Result<Vec<Saga>> {
        let conn = self.pool.get().await?;
        let now = chrono::Utc::now();
        let lease_expiry = now + chrono::Duration::seconds(lease_secs);
        let state_str = SagaState::Executing.as_str();

        let rows = conn
            .query(
                "UPDATE tb_federation_sagas
                 SET recovery_worker_id = $1, recovery_lease_expires_at = $2, updated_at = $3
                 WHERE pk_ IN (
                     SELECT pk_ FROM tb_federation_sagas
                     WHERE state = $4
                       AND (recovery_lease_expires_at IS NULL OR recovery_lease_expires_at < $3)
                     ORDER BY created_at
                     LIMIT $5
                     FOR UPDATE SKIP LOCKED
                 )
                 RETURNING id, state, created_at, completed_at, metadata",
                &[&worker_id, &lease_expiry, &now, &state_str, &limit],
            )
            .await?;

        rows.iter().map(Self::map_saga_row).collect()
    }
}

#[cfg(test)]
mod tests;
