"""Tests for database operations."""

from datetime import datetime
from unittest.mock import patch

import pytest

from fraisier.database import FraisierDB, get_connection


class TestFraisierDB:
    """Tests for FraisierDB class."""

    def test_init_creates_tables(self, tmp_db_path):
        """Test that initialization creates database tables."""
        with patch("fraisier.database.get_db_path", return_value=tmp_db_path):
            db = FraisierDB()

            # Verify tables exist by querying sqlite_master
            with get_connection() as conn:
                cursor = conn.execute(
                    "SELECT name FROM sqlite_master WHERE type='table'"
                )
                tables = {row[0] for row in cursor.fetchall()}

                assert "tb_fraise_state" in tables
                assert "tb_deployment" in tables
                assert "tb_webhook_event" in tables

    def test_update_fraise_state_new(self, test_db):
        """Test updating fraise state for new fraise."""
        test_db.update_fraise_state(
            fraise="my_api",
            environment="production",
            version="abc123",
            status="healthy",
            deployed_by="user@example.com",
        )

        state = test_db.get_fraise_state("my_api", "production")

        assert state is not None
        assert state["fraise"] == "my_api"
        assert state["environment"] == "production"
        assert state["current_version"] == "abc123"
        assert state["status"] == "healthy"
        assert state["last_deployed_by"] == "user@example.com"

    def test_update_fraise_state_existing(self, test_db):
        """Test updating existing fraise state."""
        # Insert initial state
        test_db.update_fraise_state(
            fraise="my_api",
            environment="production",
            version="v1",
        )

        # Update state
        test_db.update_fraise_state(
            fraise="my_api",
            environment="production",
            version="v2",
            status="degraded",
        )

        state = test_db.get_fraise_state("my_api", "production")

        assert state["current_version"] == "v2"
        assert state["status"] == "degraded"

    def test_get_fraise_state_nonexistent(self, test_db):
        """Test getting state for non-existent fraise."""
        state = test_db.get_fraise_state("nonexistent", "production")

        assert state is None

    def test_get_all_fraise_states(self, test_db):
        """Test getting all fraise states."""
        test_db.update_fraise_state("api_1", "prod", "v1")
        test_db.update_fraise_state("api_1", "staging", "v2")
        test_db.update_fraise_state("api_2", "prod", "v1")

        states = test_db.get_all_fraise_states()

        assert len(states) == 3
        fraise_names = {s["fraise"] for s in states}
        assert "api_1" in fraise_names
        assert "api_2" in fraise_names

    def test_start_deployment(self, test_db):
        """Test starting a deployment."""
        deployment_id = test_db.start_deployment(
            fraise="my_api",
            environment="production",
            triggered_by="webhook",
            triggered_by_user="deployer@example.com",
            git_branch="main",
            git_commit="abc123",
            old_version="old_version",
        )

        assert deployment_id is not None
        assert isinstance(deployment_id, int)

        deployment = test_db.get_deployment(deployment_id)
        assert deployment is not None
        assert deployment["fraise"] == "my_api"
        assert deployment["environment"] == "production"
        assert deployment["status"] == "in_progress"
        assert deployment["triggered_by"] == "webhook"

    def test_complete_deployment_success(self, test_db):
        """Test completing a successful deployment."""
        deployment_id = test_db.start_deployment(
            fraise="my_api",
            environment="production",
            old_version="v1",
        )

        test_db.complete_deployment(
            deployment_id=deployment_id,
            success=True,
            new_version="v2",
        )

        deployment = test_db.get_deployment(deployment_id)

        assert deployment["status"] == "success"
        assert deployment["new_version"] == "v2"
        assert deployment["duration_seconds"] is not None
        assert deployment["error_message"] is None

    def test_complete_deployment_failure(self, test_db):
        """Test completing a failed deployment."""
        deployment_id = test_db.start_deployment(
            fraise="my_api",
            environment="production",
        )

        error_msg = "Git pull failed: connection refused"
        test_db.complete_deployment(
            deployment_id=deployment_id,
            success=False,
            error_message=error_msg,
        )

        deployment = test_db.get_deployment(deployment_id)

        assert deployment["status"] == "failed"
        assert deployment["error_message"] == error_msg
        assert deployment["duration_seconds"] is not None

    def test_mark_deployment_rolled_back(self, test_db):
        """Test marking deployment as rolled back."""
        deployment_id = test_db.start_deployment(
            fraise="my_api",
            environment="production",
        )

        test_db.mark_deployment_rolled_back(deployment_id)

        deployment = test_db.get_deployment(deployment_id)

        assert deployment["status"] == "rolled_back"

    def test_get_recent_deployments(self, test_db):
        """Test getting recent deployments."""
        # Create multiple deployments
        for i in range(5):
            deployment_id = test_db.start_deployment(
                fraise=f"api_{i}",
                environment="production",
            )
            test_db.complete_deployment(
                deployment_id=deployment_id,
                success=(i % 2 == 0),
            )

        deployments = test_db.get_recent_deployments(limit=10)

        assert len(deployments) == 5

    def test_get_recent_deployments_by_fraise(self, test_db):
        """Test filtering recent deployments by fraise."""
        test_db.start_deployment(fraise="api_1", environment="prod")
        test_db.start_deployment(fraise="api_2", environment="prod")
        test_db.start_deployment(fraise="api_1", environment="prod")

        deployments = test_db.get_recent_deployments(fraise="api_1")

        assert all(d["fraise"] == "api_1" for d in deployments)

    def test_get_recent_deployments_by_environment(self, test_db):
        """Test filtering recent deployments by environment."""
        test_db.start_deployment(fraise="api", environment="prod")
        test_db.start_deployment(fraise="api", environment="staging")
        test_db.start_deployment(fraise="api", environment="prod")

        deployments = test_db.get_recent_deployments(environment="prod")

        assert all(d["environment"] == "prod" for d in deployments)

    def test_get_deployment_stats(self, test_db):
        """Test getting deployment statistics."""
        # Create successful and failed deployments
        for i in range(3):
            deployment_id = test_db.start_deployment(fraise="my_api")
            test_db.complete_deployment(
                deployment_id=deployment_id,
                success=True,
            )

        for i in range(2):
            deployment_id = test_db.start_deployment(fraise="my_api")
            test_db.complete_deployment(
                deployment_id=deployment_id,
                success=False,
            )

        stats = test_db.get_deployment_stats(fraise="my_api")

        assert stats["total"] == 5
        assert stats["successful"] == 3
        assert stats["failed"] == 2
        assert stats["avg_duration"] is not None

    def test_get_deployment_stats_all_fraises(self, test_db):
        """Test getting stats across all fraises."""
        test_db.start_deployment(fraise="api_1")
        test_db.start_deployment(fraise="api_2")

        stats = test_db.get_deployment_stats()

        assert stats["total"] == 2

    def test_record_webhook_event(self, test_db):
        """Test recording webhook event."""
        webhook_id = test_db.record_webhook_event(
            event_type="push",
            branch="main",
            commit_sha="abc123",
            sender="user",
            payload='{"test": "data"}',
        )

        assert webhook_id is not None
        assert isinstance(webhook_id, int)

    def test_link_webhook_to_deployment(self, test_db):
        """Test linking webhook event to deployment."""
        deployment_id = test_db.start_deployment(fraise="api", environment="prod")
        webhook_id = test_db.record_webhook_event(
            event_type="push",
            branch="main",
            commit_sha="abc123",
            sender="user",
            payload="{}",
        )

        test_db.link_webhook_to_deployment(webhook_id, deployment_id)

        webhooks = test_db.get_recent_webhooks()
        webhook = next(w for w in webhooks if w["id"] == webhook_id)

        assert webhook["processed"] == 1
        assert webhook["deployment_id"] == deployment_id

    def test_get_recent_webhooks(self, test_db):
        """Test getting recent webhook events."""
        for i in range(5):
            test_db.record_webhook_event(
                event_type="push",
                branch=f"branch_{i}",
                commit_sha=f"commit_{i}",
                sender="user",
                payload="{}",
            )

        webhooks = test_db.get_recent_webhooks(limit=10)

        assert len(webhooks) == 5
        assert all(w["event_type"] == "push" for w in webhooks)

    def test_get_recent_webhooks_limit(self, test_db):
        """Test webhook limit works correctly."""
        for i in range(10):
            test_db.record_webhook_event(
                event_type="push",
                branch="main",
                commit_sha=f"commit_{i}",
                sender="user",
                payload="{}",
            )

        webhooks = test_db.get_recent_webhooks(limit=5)

        assert len(webhooks) == 5

    def test_deployment_with_job_name(self, test_db):
        """Test tracking deployments with job names (for scheduled jobs)."""
        deployment_id = test_db.start_deployment(
            fraise="backup_service",
            environment="production",
            job="hourly_backup",
        )

        deployment = test_db.get_deployment(deployment_id)

        assert deployment["job"] == "hourly_backup"

    def test_multiple_jobs_same_fraise(self, test_db):
        """Test tracking multiple jobs for same fraise."""
        test_db.update_fraise_state(
            fraise="scheduler",
            environment="production",
            version="v1",
            job="job_1",
        )
        test_db.update_fraise_state(
            fraise="scheduler",
            environment="production",
            version="v1",
            job="job_2",
        )

        state_1 = test_db.get_fraise_state("scheduler", "production", job="job_1")
        state_2 = test_db.get_fraise_state("scheduler", "production", job="job_2")

        assert state_1 is not None
        assert state_2 is not None
        assert state_1["job"] == "job_1"
        assert state_2["job"] == "job_2"
