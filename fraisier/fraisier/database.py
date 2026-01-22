"""SQLite database for Fraisier deployment state and history.

YAML (fraises.yaml) = Configuration (what fraises exist)
SQLite (fraisier.db) = State & History (what's deployed, what happened)

Follows CQRS pattern with clear separation of write (tb_*) and read (v_*) models.
"""

import sqlite3
from contextlib import contextmanager
from datetime import datetime
from pathlib import Path
from typing import Any, Generator

# Default database location
DEFAULT_DB_PATH = Path("/opt/fraisier/fraisier.db")


def get_db_path() -> Path:
    """Get database path, preferring /opt location, falling back to local."""
    if DEFAULT_DB_PATH.parent.exists():
        return DEFAULT_DB_PATH
    return Path(__file__).parent.parent / "fraisier.db"


@contextmanager
def get_connection() -> Generator[sqlite3.Connection, None, None]:
    """Get database connection with row factory."""
    db_path = get_db_path()
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row
    try:
        yield conn
    finally:
        conn.close()


def init_database() -> None:
    """Initialize database schema following trinity pattern.

    Trinity pattern conventions:
    - pk_* = INTEGER PRIMARY KEY (internal, deterministic allocation)
    - id = UUID (public, API-exposed)
    - identifier = TEXT (business key, human-readable)
    - fk_* = Foreign key references (always to pk_*, not id)
    - tb_* = Write-side operational tables
    - v_* = Read-side views

    For multi-database reconciliation:
    - id column enables UUID-based sync across databases
    - identifier enables human-readable lookups
    - pk_* enables efficient internal references
    """
    with get_connection() as conn:
        conn.executescript("""
            -- ================================================================
            -- WRITE SIDE (tb_* tables) - Trinity Pattern
            -- ================================================================

            -- Current state of each fraise/environment
            -- Trinity identifiers follow PrintOptim order: id → identifier → pk_*
            CREATE TABLE IF NOT EXISTS tb_fraise_state (
                id TEXT NOT NULL UNIQUE,                         -- 1. Public UUID for sync
                identifier TEXT NOT NULL UNIQUE,                 -- 2. Business key: fraise:env[:job]
                pk_fraise_state INTEGER PRIMARY KEY AUTOINCREMENT,  -- 3. Internal key (last)

                -- Foreign Keys (if any)

                -- Domain Columns
                fraise_name TEXT NOT NULL,
                environment_name TEXT NOT NULL,
                job_name TEXT,                                   -- NULL for non-scheduled
                current_version TEXT,
                last_deployed_at TEXT,
                last_deployed_by TEXT,
                status TEXT DEFAULT 'unknown',                   -- healthy, degraded, down, unknown

                -- Audit Trail
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,

                -- Natural Key
                UNIQUE(fraise_name, environment_name, job_name)
            );

            -- Deployment history log
            -- Trinity identifiers follow PrintOptim order: id → identifier → pk_*
            CREATE TABLE IF NOT EXISTS tb_deployment (
                id TEXT NOT NULL UNIQUE,                         -- 1. Public UUID for sync
                identifier TEXT NOT NULL UNIQUE,                 -- 2. Business key: fraise:env:timestamp
                pk_deployment INTEGER PRIMARY KEY AUTOINCREMENT,    -- 3. Internal key (last)

                -- Foreign Keys
                fk_fraise_state INTEGER NOT NULL REFERENCES tb_fraise_state(pk_fraise_state),

                -- Domain Columns
                fraise_name TEXT NOT NULL,
                environment_name TEXT NOT NULL,
                job_name TEXT,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                duration_seconds REAL,
                old_version TEXT,
                new_version TEXT,
                status TEXT NOT NULL,                            -- pending, in_progress, success, failed, rolled_back
                triggered_by TEXT,                               -- webhook, manual, scheduled
                triggered_by_user TEXT,
                git_commit TEXT,
                git_branch TEXT,
                error_message TEXT,
                details TEXT,                                    -- JSON for additional data

                -- Audit Trail
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Webhook events received
            -- Trinity identifiers follow PrintOptim order: id → identifier → pk_*
            CREATE TABLE IF NOT EXISTS tb_webhook_event (
                id TEXT NOT NULL UNIQUE,                         -- 1. Public UUID for sync
                identifier TEXT NOT NULL UNIQUE,                 -- 2. Business key: provider:timestamp:hash
                pk_webhook_event INTEGER PRIMARY KEY AUTOINCREMENT,  -- 3. Internal key (last)

                -- Foreign Keys
                fk_deployment INTEGER REFERENCES tb_deployment(pk_deployment),

                -- Domain Columns
                received_at TEXT NOT NULL,
                event_type TEXT NOT NULL,                        -- push, ping, pull_request, etc.
                git_provider TEXT NOT NULL,                      -- github, gitlab, gitea, bitbucket
                branch_name TEXT,
                commit_sha TEXT,
                sender TEXT,
                payload TEXT,                                    -- Full JSON payload
                processed INTEGER DEFAULT 0,

                -- Audit Trail
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- ================================================================
            -- READ SIDE (v_* views) - Trinity Pattern
            -- ================================================================

            -- Fraise status view with trinity identifiers
            CREATE VIEW IF NOT EXISTS v_fraise_status AS
            SELECT
                fs.pk_fraise_state,
                fs.id,
                fs.identifier,
                fs.fraise_name,
                fs.environment_name,
                fs.job_name,
                fs.current_version,
                fs.status,
                fs.last_deployed_at,
                fs.last_deployed_by,
                (SELECT COUNT(*) FROM tb_deployment d
                 WHERE d.fk_fraise_state = fs.pk_fraise_state
                   AND d.status = 'success') as successful_deployments,
                (SELECT COUNT(*) FROM tb_deployment d
                 WHERE d.fk_fraise_state = fs.pk_fraise_state
                   AND d.status = 'failed') as failed_deployments,
                fs.created_at,
                fs.updated_at
            FROM tb_fraise_state fs;

            -- Deployment history view with trinity identifiers and computed fields
            CREATE VIEW IF NOT EXISTS v_deployment_history AS
            SELECT
                d.pk_deployment,
                d.id,
                d.identifier,
                d.fraise_name,
                d.environment_name,
                d.job_name,
                d.started_at,
                d.completed_at,
                d.duration_seconds,
                d.old_version,
                d.new_version,
                d.status,
                d.triggered_by,
                d.triggered_by_user,
                d.git_commit,
                d.git_branch,
                d.error_message,
                CASE
                    WHEN d.old_version != d.new_version THEN 'upgrade'
                    WHEN d.old_version = d.new_version THEN 'redeploy'
                    ELSE 'unknown'
                END as deployment_type,
                d.created_at,
                d.updated_at
            FROM tb_deployment d
            ORDER BY d.started_at DESC;

            -- Webhook event view with trinity identifiers
            CREATE VIEW IF NOT EXISTS v_webhook_event_history AS
            SELECT
                we.pk_webhook_event,
                we.id,
                we.identifier,
                we.git_provider,
                we.event_type,
                we.branch_name,
                we.commit_sha,
                we.sender,
                we.received_at,
                we.processed,
                we.fk_deployment,
                d.id as deployment_id,
                d.fraise_name,
                d.environment_name,
                we.created_at,
                we.updated_at
            FROM tb_webhook_event we
            LEFT JOIN tb_deployment d ON we.fk_deployment = d.pk_deployment
            ORDER BY we.received_at DESC;

            -- ================================================================
            -- INDEXES - Optimized for common queries
            -- ================================================================

            -- Fraise state lookups
            CREATE INDEX IF NOT EXISTS idx_fraise_state_name_env
                ON tb_fraise_state(fraise_name, environment_name);
            CREATE INDEX IF NOT EXISTS idx_fraise_state_identifier
                ON tb_fraise_state(identifier);
            CREATE INDEX IF NOT EXISTS idx_fraise_state_id
                ON tb_fraise_state(id);

            -- Deployment lookups
            CREATE INDEX IF NOT EXISTS idx_deployment_fraise_state_fk
                ON tb_deployment(fk_fraise_state);
            CREATE INDEX IF NOT EXISTS idx_deployment_started_at
                ON tb_deployment(started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_deployment_identifier
                ON tb_deployment(identifier);
            CREATE INDEX IF NOT EXISTS idx_deployment_id
                ON tb_deployment(id);
            CREATE INDEX IF NOT EXISTS idx_deployment_status
                ON tb_deployment(status);

            -- Webhook lookups
            CREATE INDEX IF NOT EXISTS idx_webhook_event_deployment_fk
                ON tb_webhook_event(fk_deployment);
            CREATE INDEX IF NOT EXISTS idx_webhook_event_received_at
                ON tb_webhook_event(received_at DESC);
            CREATE INDEX IF NOT EXISTS idx_webhook_event_identifier
                ON tb_webhook_event(identifier);
            CREATE INDEX IF NOT EXISTS idx_webhook_event_id
                ON tb_webhook_event(id);
            CREATE INDEX IF NOT EXISTS idx_webhook_event_processed
                ON tb_webhook_event(processed);
        """)
        conn.commit()


class FraisierDB:
    """High-level interface for Fraisier database operations."""

    def __init__(self):
        """Initialize and ensure schema exists."""
        init_database()

    # =========================================================================
    # Fraise State
    # =========================================================================

    def get_fraise_state(
        self, fraise: str, environment: str, job: str | None = None
    ) -> dict[str, Any] | None:
        """Get current state of a fraise.

        Args:
            fraise: Fraise name
            environment: Environment name
            job: Optional job name for scheduled deployments

        Returns:
            Fraise state dict or None if not found
        """
        with get_connection() as conn:
            if job:
                row = conn.execute(
                    "SELECT * FROM v_fraise_status WHERE fraise_name=? AND environment_name=? AND job_name=?",
                    (fraise, environment, job),
                ).fetchone()
            else:
                row = conn.execute(
                    "SELECT * FROM v_fraise_status WHERE fraise_name=? AND environment_name=? AND job_name IS NULL",
                    (fraise, environment),
                ).fetchone()
            return dict(row) if row else None

    def update_fraise_state(
        self,
        fraise: str,
        environment: str,
        version: str,
        status: str = "healthy",
        job: str | None = None,
        deployed_by: str | None = None,
    ) -> None:
        """Update or insert fraise state with trinity identifiers.

        Creates or updates a fraise state with:
        - pk_fraise_state: Internal key (auto-allocated)
        - id: UUID for cross-database sync
        - identifier: Business key (fraise:environment[:job])

        Args:
            fraise: Fraise name
            environment: Environment name
            version: Current deployed version
            status: Health status (healthy, degraded, down, unknown)
            job: Optional job name for scheduled deployments
            deployed_by: User who triggered deployment
        """
        import uuid

        now = datetime.now().isoformat()
        # Generate trinity identifiers
        state_uuid = str(uuid.uuid4())
        identifier = f"{fraise}:{environment}" if not job else f"{fraise}:{environment}:{job}"

        with get_connection() as conn:
            # Check if exists to decide between insert or update
            existing = conn.execute(
                "SELECT pk_fraise_state FROM tb_fraise_state WHERE fraise_name=? AND environment_name=? AND job_name IS ?",
                (fraise, environment, job),
            ).fetchone()

            if existing:
                # Update existing
                conn.execute(
                    """
                    UPDATE tb_fraise_state
                    SET current_version = ?,
                        last_deployed_at = ?,
                        last_deployed_by = ?,
                        status = ?,
                        updated_at = ?
                    WHERE fraise_name = ? AND environment_name = ? AND job_name IS ?
                    """,
                    (version, now, deployed_by, status, now, fraise, environment, job),
                )
            else:
                # Insert new
                conn.execute(
                    """
                    INSERT INTO tb_fraise_state
                        (id, identifier, fraise_name, environment_name, job_name,
                         current_version, last_deployed_at, last_deployed_by, status,
                         created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    """,
                    (state_uuid, identifier, fraise, environment, job,
                     version, now, deployed_by, status, now, now),
                )

            conn.commit()

    def get_all_fraise_states(self) -> list[dict[str, Any]]:
        """Get state of all fraises."""
        with get_connection() as conn:
            rows = conn.execute(
                "SELECT * FROM v_fraise_status ORDER BY fraise, environment"
            ).fetchall()
            return [dict(row) for row in rows]

    # =========================================================================
    # Deployment History
    # =========================================================================

    def start_deployment(
        self,
        fraise: str,
        environment: str,
        triggered_by: str = "manual",
        triggered_by_user: str | None = None,
        git_branch: str | None = None,
        git_commit: str | None = None,
        old_version: str | None = None,
        job: str | None = None,
    ) -> int:
        """Record start of a deployment with trinity identifiers.

        Creates deployment record with:
        - pk_deployment: Internal key (auto-allocated)
        - id: UUID for cross-database sync
        - identifier: Business key (fraise:environment:timestamp)
        - fk_fraise_state: Reference to fraise state (pk_fraise_state, not id)

        Args:
            fraise: Fraise name
            environment: Environment name
            triggered_by: Trigger source (webhook, manual, scheduled)
            triggered_by_user: User who triggered deployment
            git_branch: Git branch deployed
            git_commit: Git commit hash
            old_version: Previous deployed version
            job: Optional job name

        Returns:
            pk_deployment (INTEGER primary key) for this deployment
        """
        import uuid

        now = datetime.now().isoformat()
        deployment_uuid = str(uuid.uuid4())
        identifier = f"{fraise}:{environment}:{now}"

        with get_connection() as conn:
            # Get fk_fraise_state (pk_fraise_state from tb_fraise_state)
            fraise_state = conn.execute(
                "SELECT pk_fraise_state FROM tb_fraise_state WHERE fraise_name=? AND environment_name=? AND job_name IS ?",
                (fraise, environment, job),
            ).fetchone()

            fk_fraise_state = fraise_state["pk_fraise_state"] if fraise_state else None

            cursor = conn.execute(
                """
                INSERT INTO tb_deployment
                    (id, identifier, fk_fraise_state, fraise_name, environment_name, job_name,
                     started_at, status, triggered_by, triggered_by_user, git_branch, git_commit,
                     old_version, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, 'in_progress', ?, ?, ?, ?, ?, ?, ?)
                """,
                (deployment_uuid, identifier, fk_fraise_state, fraise, environment, job,
                 now, triggered_by, triggered_by_user, git_branch, git_commit, old_version, now, now),
            )
            conn.commit()
            return cursor.lastrowid

    def complete_deployment(
        self,
        deployment_id: int,
        success: bool,
        new_version: str | None = None,
        error_message: str | None = None,
        details: str | None = None,
    ) -> None:
        """Record completion of a deployment (pk_deployment).

        Args:
            deployment_id: pk_deployment (INTEGER primary key)
            success: Whether deployment succeeded
            new_version: New deployed version
            error_message: Error message if failed
            details: JSON details of deployment
        """
        now = datetime.now().isoformat()
        status = "success" if success else "failed"

        with get_connection() as conn:
            # Get start time to calculate duration using pk_deployment
            row = conn.execute(
                "SELECT started_at FROM tb_deployment WHERE pk_deployment=?",
                (deployment_id,),
            ).fetchone()

            duration = None
            if row:
                started = datetime.fromisoformat(row["started_at"])
                duration = (datetime.now() - started).total_seconds()

            conn.execute(
                """
                UPDATE tb_deployment
                SET completed_at=?, status=?, new_version=?, duration_seconds=?,
                    error_message=?, details=?, updated_at=?
                WHERE pk_deployment=?
                """,
                (now, status, new_version, duration, error_message, details, now, deployment_id),
            )
            conn.commit()

    def mark_deployment_rolled_back(self, deployment_id: int) -> None:
        """Mark a deployment as rolled back (pk_deployment).

        Args:
            deployment_id: pk_deployment (INTEGER primary key)
        """
        now = datetime.now().isoformat()
        with get_connection() as conn:
            conn.execute(
                "UPDATE tb_deployment SET status='rolled_back', updated_at=? WHERE pk_deployment=?",
                (now, deployment_id),
            )
            conn.commit()

    def get_deployment(self, deployment_id: int) -> dict[str, Any] | None:
        """Get a specific deployment record (pk_deployment).

        Args:
            deployment_id: pk_deployment (INTEGER primary key)

        Returns:
            Deployment record from v_deployment_history or None
        """
        with get_connection() as conn:
            row = conn.execute(
                "SELECT * FROM v_deployment_history WHERE pk_deployment=?",
                (deployment_id,),
            ).fetchone()
            return dict(row) if row else None

    def get_recent_deployments(
        self,
        limit: int = 20,
        fraise: str | None = None,
        environment: str | None = None,
    ) -> list[dict[str, Any]]:
        """Get recent deployment history with trinity identifiers.

        Args:
            limit: Number of deployments to return
            fraise: Filter by fraise name
            environment: Filter by environment name

        Returns:
            List of deployment records from v_deployment_history
        """
        query = "SELECT * FROM v_deployment_history WHERE 1=1"
        params: list[Any] = []

        if fraise:
            query += " AND fraise_name=?"
            params.append(fraise)
        if environment:
            query += " AND environment_name=?"
            params.append(environment)

        query += " ORDER BY started_at DESC LIMIT ?"
        params.append(limit)

        with get_connection() as conn:
            rows = conn.execute(query, params).fetchall()
            return [dict(row) for row in rows]

    def get_deployment_stats(
        self, fraise: str | None = None, days: int = 30
    ) -> dict[str, Any]:
        """Get deployment statistics with trinity identifiers.

        Args:
            fraise: Filter by fraise name
            days: Number of days to include in statistics

        Returns:
            Dictionary with stats: total, successful, failed, rolled_back, avg_duration
        """
        cutoff = datetime.now().isoformat()[:10]  # Just date part

        with get_connection() as conn:
            query = """
                SELECT
                    COUNT(*) as total,
                    SUM(CASE WHEN status='success' THEN 1 ELSE 0 END) as successful,
                    SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END) as failed,
                    SUM(CASE WHEN status='rolled_back' THEN 1 ELSE 0 END) as rolled_back,
                    AVG(duration_seconds) as avg_duration
                FROM tb_deployment
                WHERE started_at >= date(?, '-' || ? || ' days')
            """
            params: list[Any] = [cutoff, days]

            if fraise:
                query += " AND fraise_name=?"
                params.append(fraise)

            row = conn.execute(query, params).fetchone()
            return dict(row) if row else {}

    # =========================================================================
    # Webhook Events
    # =========================================================================

    def record_webhook_event(
        self,
        event_type: str,
        payload: str,
        branch: str | None = None,
        commit_sha: str | None = None,
        sender: str | None = None,
        git_provider: str = "unknown",
    ) -> int:
        """Record a received webhook event with trinity identifiers.

        Creates webhook record with:
        - pk_webhook_event: Internal key (auto-allocated)
        - id: UUID for cross-database sync
        - identifier: Business key (provider:timestamp:hash)

        Args:
            event_type: Type of event (push, ping, pull_request, etc.)
            payload: Full webhook payload JSON
            branch: Git branch name
            commit_sha: Commit hash
            sender: Who sent the event
            git_provider: Git provider (github, gitlab, gitea, bitbucket)

        Returns:
            pk_webhook_event (INTEGER primary key)
        """
        import hashlib
        import uuid

        now = datetime.now().isoformat()
        webhook_uuid = str(uuid.uuid4())
        # Create business key: provider:timestamp:hash(first 8 chars of payload hash)
        payload_hash = hashlib.sha256(payload.encode()).hexdigest()[:8]
        identifier = f"{git_provider}:{now}:{payload_hash}"

        with get_connection() as conn:
            cursor = conn.execute(
                """
                INSERT INTO tb_webhook_event
                    (id, identifier, received_at, event_type, git_provider,
                     branch_name, commit_sha, sender, payload, processed, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
                """,
                (webhook_uuid, identifier, now, event_type, git_provider,
                 branch, commit_sha, sender, payload, now, now),
            )
            conn.commit()
            return cursor.lastrowid

    def link_webhook_to_deployment(self, webhook_id: int, deployment_id: int) -> None:
        """Link a webhook event to its triggered deployment.

        Args:
            webhook_id: pk_webhook_event (INTEGER primary key)
            deployment_id: pk_deployment (INTEGER primary key) to link to
        """
        now = datetime.now().isoformat()
        with get_connection() as conn:
            conn.execute(
                """
                UPDATE tb_webhook_event
                SET processed=1, fk_deployment=?, updated_at=?
                WHERE pk_webhook_event=?
                """,
                (deployment_id, now, webhook_id),
            )
            conn.commit()

    def get_recent_webhooks(self, limit: int = 20) -> list[dict[str, Any]]:
        """Get recent webhook events with trinity identifiers.

        Args:
            limit: Number of webhook events to return

        Returns:
            List of webhook events from v_webhook_event_history
        """
        with get_connection() as conn:
            rows = conn.execute(
                """
                SELECT pk_webhook_event, id, identifier, git_provider, event_type,
                       branch_name, commit_sha, sender, received_at, processed,
                       fk_deployment, deployment_id, fraise_name, environment_name,
                       created_at, updated_at
                FROM v_webhook_event_history
                ORDER BY received_at DESC
                LIMIT ?
                """,
                (limit,),
            ).fetchall()
            return [dict(row) for row in rows]


# Global instance
_db: FraisierDB | None = None


def get_db() -> FraisierDB:
    """Get or create global database instance."""
    global _db
    if _db is None:
        _db = FraisierDB()
    return _db
