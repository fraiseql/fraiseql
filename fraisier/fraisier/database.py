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
    """Initialize database schema following CQRS pattern."""
    with get_connection() as conn:
        conn.executescript("""
            -- ================================================================
            -- WRITE SIDE (tb_* tables)
            -- ================================================================

            -- Current state of each fraise/environment
            CREATE TABLE IF NOT EXISTS tb_fraise_state (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fraise TEXT NOT NULL,
                environment TEXT NOT NULL,
                job TEXT,  -- NULL for non-job fraises
                current_version TEXT,
                last_deployed_at TEXT,
                last_deployed_by TEXT,
                status TEXT DEFAULT 'unknown',  -- healthy, degraded, down, unknown
                UNIQUE(fraise, environment, job)
            );

            -- Deployment history log
            CREATE TABLE IF NOT EXISTS tb_deployment (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                fraise TEXT NOT NULL,
                environment TEXT NOT NULL,
                job TEXT,
                started_at TEXT NOT NULL,
                completed_at TEXT,
                duration_seconds REAL,
                old_version TEXT,
                new_version TEXT,
                status TEXT NOT NULL,  -- pending, in_progress, success, failed, rolled_back
                triggered_by TEXT,  -- webhook, manual, scheduled
                triggered_by_user TEXT,
                git_commit TEXT,
                git_branch TEXT,
                error_message TEXT,
                details TEXT  -- JSON for additional data
            );

            -- Webhook events received
            CREATE TABLE IF NOT EXISTS tb_webhook_event (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                received_at TEXT NOT NULL,
                event_type TEXT NOT NULL,  -- push, ping, etc.
                branch TEXT,
                commit_sha TEXT,
                sender TEXT,
                payload TEXT,  -- Full JSON payload
                processed INTEGER DEFAULT 0,
                deployment_id INTEGER REFERENCES tb_deployment(id)
            );

            -- ================================================================
            -- READ SIDE (v_* views)
            -- ================================================================

            -- Fraise status view
            CREATE VIEW IF NOT EXISTS v_fraise_status AS
            SELECT
                fs.fraise,
                fs.environment,
                fs.job,
                fs.current_version,
                fs.status,
                fs.last_deployed_at,
                fs.last_deployed_by,
                (SELECT COUNT(*) FROM tb_deployment d
                 WHERE d.fraise = fs.fraise
                   AND d.environment = fs.environment
                   AND (d.job = fs.job OR (d.job IS NULL AND fs.job IS NULL))
                   AND d.status = 'success') as successful_deployments,
                (SELECT COUNT(*) FROM tb_deployment d
                 WHERE d.fraise = fs.fraise
                   AND d.environment = fs.environment
                   AND (d.job = fs.job OR (d.job IS NULL AND fs.job IS NULL))
                   AND d.status = 'failed') as failed_deployments
            FROM tb_fraise_state fs;

            -- Deployment history view with computed fields
            CREATE VIEW IF NOT EXISTS v_deployment_history AS
            SELECT
                d.id,
                d.fraise,
                d.environment,
                d.job,
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
                END as deployment_type
            FROM tb_deployment d
            ORDER BY d.started_at DESC;

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_deployment_fraise_env
                ON tb_deployment(fraise, environment);
            CREATE INDEX IF NOT EXISTS idx_deployment_started
                ON tb_deployment(started_at DESC);
            CREATE INDEX IF NOT EXISTS idx_webhook_received
                ON tb_webhook_event(received_at DESC);
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
        """Get current state of a fraise."""
        with get_connection() as conn:
            if job:
                row = conn.execute(
                    "SELECT * FROM tb_fraise_state WHERE fraise=? AND environment=? AND job=?",
                    (fraise, environment, job),
                ).fetchone()
            else:
                row = conn.execute(
                    "SELECT * FROM tb_fraise_state WHERE fraise=? AND environment=? AND job IS NULL",
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
        """Update or insert fraise state."""
        now = datetime.now().isoformat()
        with get_connection() as conn:
            conn.execute(
                """
                INSERT INTO tb_fraise_state (fraise, environment, job, current_version,
                                             last_deployed_at, last_deployed_by, status)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(fraise, environment, job) DO UPDATE SET
                    current_version = excluded.current_version,
                    last_deployed_at = excluded.last_deployed_at,
                    last_deployed_by = excluded.last_deployed_by,
                    status = excluded.status
                """,
                (fraise, environment, job, version, now, deployed_by, status),
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
        """Record start of a deployment, returns deployment ID."""
        now = datetime.now().isoformat()
        with get_connection() as conn:
            cursor = conn.execute(
                """
                INSERT INTO tb_deployment
                    (fraise, environment, job, started_at, status, triggered_by,
                     triggered_by_user, git_branch, git_commit, old_version)
                VALUES (?, ?, ?, ?, 'in_progress', ?, ?, ?, ?, ?)
                """,
                (fraise, environment, job, now, triggered_by, triggered_by_user,
                 git_branch, git_commit, old_version),
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
        """Record completion of a deployment."""
        now = datetime.now().isoformat()
        status = "success" if success else "failed"

        with get_connection() as conn:
            # Get start time to calculate duration
            row = conn.execute(
                "SELECT started_at FROM tb_deployment WHERE id=?",
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
                    error_message=?, details=?
                WHERE id=?
                """,
                (now, status, new_version, duration, error_message, details, deployment_id),
            )
            conn.commit()

    def mark_deployment_rolled_back(self, deployment_id: int) -> None:
        """Mark a deployment as rolled back."""
        with get_connection() as conn:
            conn.execute(
                "UPDATE tb_deployment SET status='rolled_back' WHERE id=?",
                (deployment_id,),
            )
            conn.commit()

    def get_deployment(self, deployment_id: int) -> dict[str, Any] | None:
        """Get a specific deployment record."""
        with get_connection() as conn:
            row = conn.execute(
                "SELECT * FROM v_deployment_history WHERE id=?",
                (deployment_id,),
            ).fetchone()
            return dict(row) if row else None

    def get_recent_deployments(
        self,
        limit: int = 20,
        fraise: str | None = None,
        environment: str | None = None,
    ) -> list[dict[str, Any]]:
        """Get recent deployment history."""
        query = "SELECT * FROM v_deployment_history WHERE 1=1"
        params: list[Any] = []

        if fraise:
            query += " AND fraise=?"
            params.append(fraise)
        if environment:
            query += " AND environment=?"
            params.append(environment)

        query += " ORDER BY started_at DESC LIMIT ?"
        params.append(limit)

        with get_connection() as conn:
            rows = conn.execute(query, params).fetchall()
            return [dict(row) for row in rows]

    def get_deployment_stats(
        self, fraise: str | None = None, days: int = 30
    ) -> dict[str, Any]:
        """Get deployment statistics."""
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
                query += " AND fraise=?"
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
    ) -> int:
        """Record a received webhook event."""
        now = datetime.now().isoformat()
        with get_connection() as conn:
            cursor = conn.execute(
                """
                INSERT INTO tb_webhook_event
                    (received_at, event_type, branch, commit_sha, sender, payload)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (now, event_type, branch, commit_sha, sender, payload),
            )
            conn.commit()
            return cursor.lastrowid

    def link_webhook_to_deployment(self, webhook_id: int, deployment_id: int) -> None:
        """Link a webhook event to its triggered deployment."""
        with get_connection() as conn:
            conn.execute(
                """
                UPDATE tb_webhook_event
                SET processed=1, deployment_id=?
                WHERE id=?
                """,
                (deployment_id, webhook_id),
            )
            conn.commit()

    def get_recent_webhooks(self, limit: int = 20) -> list[dict[str, Any]]:
        """Get recent webhook events."""
        with get_connection() as conn:
            rows = conn.execute(
                """
                SELECT id, received_at, event_type, branch, commit_sha, sender,
                       processed, deployment_id
                FROM tb_webhook_event
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
