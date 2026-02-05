"""Tests for database observability and metrics integration.

Tests database operation logging, metrics recording, and audit trails.
"""

import time
from unittest.mock import MagicMock, patch

import pytest

from fraisier.db.observability import (
    DatabaseObservability,
    DeploymentDatabaseAudit,
    get_audit_logger,
    get_database_observability,
)
from fraisier.metrics import MetricsRecorder


class TestDatabaseObservability:
    """Test database operation observability."""

    def test_observability_creation(self):
        """Test creating observability instance."""
        obs = DatabaseObservability("sqlite")
        assert obs.database_type == "sqlite"
        assert obs.metrics is not None

    def test_query_start_logging(self):
        """Test logging query start."""
        obs = DatabaseObservability("postgresql")
        context = obs.log_query_start("select", "tb_fraise_state")

        assert context["database_type"] == "postgresql"
        assert context["operation"] == "select"
        assert context["table"] == "tb_fraise_state"
        assert "start_time" in context

    def test_query_success_logging(self):
        """Test logging successful query."""
        obs = DatabaseObservability("mysql")

        with patch.object(obs.metrics, "record_db_query") as mock_record:
            context = obs.log_query_start("insert", "tb_deployment")
            time.sleep(0.01)  # Simulate query execution
            obs.log_query_success(context, rows_affected=1)

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "mysql"
            assert call_args.kwargs["operation"] == "insert"
            assert call_args.kwargs["status"] == "success"
            assert call_args.kwargs["duration_seconds"] >= 0.01

    def test_query_error_logging(self):
        """Test logging query error."""
        obs = DatabaseObservability("sqlite")

        with patch.object(obs.metrics, "record_db_query") as mock_query, patch.object(
            obs.metrics, "record_db_error"
        ) as mock_error:
            context = obs.log_query_start("update", "tb_fraise_state")
            error = Exception("Connection timeout")
            obs.log_query_error(context, error, error_type="timeout")

            mock_query.assert_called_once()
            mock_error.assert_called_once()
            call_args = mock_error.call_args
            assert call_args.kwargs["database_type"] == "sqlite"
            assert call_args.kwargs["error_type"] == "timeout"

    def test_transaction_logging(self):
        """Test logging database transactions."""
        obs = DatabaseObservability("postgresql")

        with patch.object(obs.metrics, "record_db_transaction") as mock_record:
            context = obs.log_transaction_start("migration")
            time.sleep(0.01)
            obs.log_transaction_complete(context, status="success")

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "postgresql"
            assert call_args.kwargs["duration_seconds"] >= 0.01

    def test_pool_metrics_logging_normal(self):
        """Test logging normal pool utilization."""
        obs = DatabaseObservability("sqlite")

        with patch.object(obs.metrics, "update_db_pool_metrics") as mock_record:
            obs.log_pool_metrics(
                active_connections=2,
                idle_connections=8,
                waiting_requests=0,
            )

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "sqlite"
            assert call_args.kwargs["active_connections"] == 2
            assert call_args.kwargs["idle_connections"] == 8
            assert call_args.kwargs["waiting_requests"] == 0

    def test_pool_metrics_logging_high_utilization(self):
        """Test logging high pool utilization warning."""
        obs = DatabaseObservability("mysql")

        with patch.object(obs.metrics, "update_db_pool_metrics") as mock_record:
            # 9 active out of 10 = 90% utilization
            obs.log_pool_metrics(
                active_connections=9,
                idle_connections=1,
                waiting_requests=3,
            )

            mock_record.assert_called_once()
            # Should still record metrics even when high
            call_args = mock_record.call_args
            assert call_args.kwargs["active_connections"] == 9
            assert call_args.kwargs["waiting_requests"] == 3


class TestDeploymentDatabaseAudit:
    """Test deployment audit logging."""

    def test_audit_creation(self):
        """Test creating audit logger."""
        audit = DeploymentDatabaseAudit("postgresql")
        assert audit.database_type == "postgresql"

    def test_fraise_state_change_logging(self):
        """Test logging fraise state changes."""
        audit = DeploymentDatabaseAudit("sqlite")

        with patch.object(audit.metrics, "record_deployment_db_operation") as mock_record:
            audit.log_fraise_state_changed(
                fraise_id="uuid-123",
                identifier="api:prod",
                old_state={"status": "healthy", "version": "1.0.0"},
                new_state={"status": "deploying", "version": "1.1.0"},
                changed_by="deployment_manager",
            )

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "sqlite"
            assert call_args.kwargs["operation_type"] == "update_fraise_state"

    def test_deployment_recorded_logging(self):
        """Test logging deployment recording."""
        audit = DeploymentDatabaseAudit("mysql")

        with patch.object(audit.metrics, "record_deployment_db_operation") as mock_record:
            audit.log_deployment_recorded(
                deployment_id="deploy-456",
                fraise_identifier="api:prod",
                environment="production",
                status="success",
                triggered_by="github_webhook",
            )

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "mysql"
            assert call_args.kwargs["operation_type"] == "record_deployment"

    def test_webhook_linked_logging(self):
        """Test logging webhook linking."""
        audit = DeploymentDatabaseAudit("postgresql")

        with patch.object(audit.metrics, "record_deployment_db_operation") as mock_record:
            audit.log_webhook_linked(
                webhook_id="webhook-789",
                deployment_id="deploy-456",
                event_type="push",
            )

            mock_record.assert_called_once()
            call_args = mock_record.call_args
            assert call_args.kwargs["database_type"] == "postgresql"
            assert call_args.kwargs["operation_type"] == "link_webhook"


class TestMetricsRecorderDatabaseMethods:
    """Test new database metrics methods in MetricsRecorder."""

    def test_record_db_query(self):
        """Test recording database query metric."""
        recorder = MetricsRecorder()

        # Should not raise
        recorder.record_db_query(
            database_type="sqlite",
            operation="select",
            status="success",
            duration_seconds=0.05,
        )

    def test_record_db_error(self):
        """Test recording database error metric."""
        recorder = MetricsRecorder()

        # Should not raise
        recorder.record_db_error(
            database_type="postgresql",
            error_type="timeout",
        )

    def test_record_db_transaction(self):
        """Test recording database transaction metric."""
        recorder = MetricsRecorder()

        # Should not raise
        recorder.record_db_transaction(
            database_type="mysql",
            duration_seconds=1.2,
        )

    def test_record_deployment_db_operation(self):
        """Test recording deployment database operation metric."""
        recorder = MetricsRecorder()

        # Should not raise
        recorder.record_deployment_db_operation(
            database_type="sqlite",
            operation_type="record_deployment",
        )

    def test_update_db_pool_metrics(self):
        """Test updating database pool metrics."""
        recorder = MetricsRecorder()

        # Should not raise
        recorder.update_db_pool_metrics(
            database_type="postgresql",
            active_connections=5,
            idle_connections=10,
            waiting_requests=2,
        )


class TestObservabilityFactoryFunctions:
    """Test factory functions for creating observability instances."""

    def test_get_database_observability(self):
        """Test getting database observability instance."""
        obs = get_database_observability("sqlite")
        assert isinstance(obs, DatabaseObservability)
        assert obs.database_type == "sqlite"

    def test_get_audit_logger(self):
        """Test getting audit logger instance."""
        audit = get_audit_logger("postgresql")
        assert isinstance(audit, DeploymentDatabaseAudit)
        assert audit.database_type == "postgresql"

    def test_multiple_instances_independent(self):
        """Test that multiple instances are independent."""
        obs1 = get_database_observability("sqlite")
        obs2 = get_database_observability("postgresql")

        assert obs1.database_type == "sqlite"
        assert obs2.database_type == "postgresql"
        # They share the same metrics recorder (singleton)
        assert obs1.metrics is obs2.metrics


class TestObservabilityIntegration:
    """Integration tests for observability system."""

    def test_full_query_lifecycle(self):
        """Test complete query lifecycle with logging and metrics."""
        obs = DatabaseObservability("sqlite")

        with patch.object(obs.metrics, "record_db_query") as mock_query, patch.object(
            obs.metrics, "record_db_error"
        ) as mock_error:

            # Simulate successful query
            context = obs.log_query_start("select", "tb_fraise_state")
            time.sleep(0.01)
            obs.log_query_success(context, rows_affected=5)

            # Verify metrics were recorded
            assert mock_query.call_count == 1
            assert mock_error.call_count == 0

    def test_deployment_audit_trail(self):
        """Test complete deployment audit trail."""
        audit = DeploymentDatabaseAudit("postgresql")

        with patch.object(audit.metrics, "record_deployment_db_operation") as mock_record:

            # Simulate deployment operations
            audit.log_deployment_recorded(
                deployment_id="d1",
                fraise_identifier="api:prod",
                environment="production",
                status="in_progress",
            )

            audit.log_fraise_state_changed(
                fraise_id="f1",
                identifier="api:prod",
                old_state={"status": "healthy"},
                new_state={"status": "deploying"},
            )

            audit.log_webhook_linked(
                webhook_id="w1",
                deployment_id="d1",
                event_type="push",
            )

            # All operations should be logged
            assert mock_record.call_count == 3

    def test_pool_metrics_tracking(self):
        """Test tracking pool metrics over time."""
        obs = DatabaseObservability("mysql")

        with patch.object(obs.metrics, "update_db_pool_metrics") as mock_update:

            # Simulate pool state changes
            obs.log_pool_metrics(5, 5, 0)
            obs.log_pool_metrics(8, 2, 1)
            obs.log_pool_metrics(9, 1, 3)

            assert mock_update.call_count == 3
            # Verify each update has correct values
            calls = [call.kwargs for call in mock_update.call_args_list]
            assert calls[0]["active_connections"] == 5
            assert calls[1]["active_connections"] == 8
            assert calls[2]["active_connections"] == 9


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
