"""Database observability and monitoring integration.

Provides structured logging, metrics recording, and audit logging for all
database operations. Integrates with the metrics system for Prometheus export.
"""

import json
import logging
import time
from typing import Any

from fraisier.metrics import get_metrics_recorder

# Configure structured logging for database operations
logger = logging.getLogger(__name__)


class DatabaseObservability:
    """Database operation monitoring and observability.

    Tracks all database operations with:
    - Structured JSON logging for audit trail
    - Prometheus metrics for monitoring
    - Query performance tracking
    - Error categorization
    - Pool metrics reporting
    """

    def __init__(self, database_type: str):
        """Initialize database observability.

        Args:
            database_type: Database type (sqlite, postgresql, mysql)
        """
        self.database_type = database_type
        self.metrics = get_metrics_recorder()

    def log_query_start(self, operation: str, table: str) -> dict[str, Any]:
        """Log query start and return context for tracking.

        Args:
            operation: Query operation (select, insert, update, delete)
            table: Table being queried

        Returns:
            Context dict with start_time for later use
        """
        context = {
            "database_type": self.database_type,
            "operation": operation,
            "table": table,
            "start_time": time.time(),
        }

        logger.debug(
            "database_query_start",
            extra={
                "database_type": self.database_type,
                "operation": operation,
                "table": table,
            },
        )

        return context

    def log_query_success(
        self,
        context: dict[str, Any],
        rows_affected: int | None = None,
    ) -> None:
        """Log query success and record metrics.

        Args:
            context: Context from log_query_start
            rows_affected: Number of rows affected (for write operations)
        """
        duration_seconds = time.time() - context["start_time"]

        # Record metrics
        self.metrics.record_db_query(
            database_type=self.database_type,
            operation=context["operation"],
            status="success",
            duration_seconds=duration_seconds,
        )

        # Structured log
        logger.debug(
            "database_query_success",
            extra={
                "database_type": self.database_type,
                "operation": context["operation"],
                "table": context["table"],
                "duration_seconds": duration_seconds,
                "rows_affected": rows_affected,
            },
        )

    def log_query_error(
        self,
        context: dict[str, Any],
        error: Exception,
        error_type: str = "unknown",
    ) -> None:
        """Log query error and record metrics.

        Args:
            context: Context from log_query_start
            error: Exception that occurred
            error_type: Categorized error type (connection, syntax, timeout, etc.)
        """
        duration_seconds = time.time() - context["start_time"]

        # Record metrics
        self.metrics.record_db_query(
            database_type=self.database_type,
            operation=context["operation"],
            status="failed",
            duration_seconds=duration_seconds,
        )

        self.metrics.record_db_error(
            database_type=self.database_type,
            error_type=error_type,
        )

        # Structured log
        logger.error(
            "database_query_error",
            extra={
                "database_type": self.database_type,
                "operation": context["operation"],
                "table": context["table"],
                "duration_seconds": duration_seconds,
                "error_type": error_type,
                "error_message": str(error),
            },
            exc_info=True,
        )

    def log_transaction_start(self, operation_type: str) -> dict[str, Any]:
        """Log transaction start.

        Args:
            operation_type: Type of transaction (migration, deployment, etc.)

        Returns:
            Context dict with start_time
        """
        context = {
            "database_type": self.database_type,
            "operation_type": operation_type,
            "start_time": time.time(),
        }

        logger.info(
            "database_transaction_start",
            extra={
                "database_type": self.database_type,
                "operation_type": operation_type,
            },
        )

        return context

    def log_transaction_complete(
        self,
        context: dict[str, Any],
        status: str = "success",
    ) -> None:
        """Log transaction completion.

        Args:
            context: Context from log_transaction_start
            status: Transaction status (success, failed, rolled_back)
        """
        duration_seconds = time.time() - context["start_time"]

        # Record metrics
        self.metrics.record_db_transaction(
            database_type=self.database_type,
            duration_seconds=duration_seconds,
        )

        logger.info(
            "database_transaction_complete",
            extra={
                "database_type": self.database_type,
                "operation_type": context["operation_type"],
                "status": status,
                "duration_seconds": duration_seconds,
            },
        )

    def log_pool_metrics(
        self,
        active_connections: int,
        idle_connections: int,
        waiting_requests: int,
    ) -> None:
        """Log and update database pool metrics.

        Args:
            active_connections: Number of active connections
            idle_connections: Number of idle connections
            waiting_requests: Number of requests waiting
        """
        # Update metrics
        self.metrics.update_db_pool_metrics(
            database_type=self.database_type,
            active_connections=active_connections,
            idle_connections=idle_connections,
            waiting_requests=waiting_requests,
        )

        # Structured log for high utilization
        total_connections = active_connections + idle_connections
        if total_connections > 0:
            utilization = (active_connections / total_connections) * 100
            if utilization > 80 or waiting_requests > 0:
                logger.warning(
                    "database_pool_high_utilization",
                    extra={
                        "database_type": self.database_type,
                        "active_connections": active_connections,
                        "idle_connections": idle_connections,
                        "waiting_requests": waiting_requests,
                        "utilization_percent": utilization,
                    },
                )


class DeploymentDatabaseAudit:
    """Audit logging for deployment-related database operations.

    Tracks all database changes made during deployments for compliance
    and troubleshooting purposes.
    """

    def __init__(self, database_type: str):
        """Initialize audit logging.

        Args:
            database_type: Database type
        """
        self.database_type = database_type
        self.metrics = get_metrics_recorder()
        # Audit logger writes to separate file if configured
        self.audit_logger = logging.getLogger("fraisier.audit")

    def log_fraise_state_changed(
        self,
        fraise_id: str,
        identifier: str,
        old_state: dict[str, Any],
        new_state: dict[str, Any],
        changed_by: str | None = None,
    ) -> None:
        """Log fraise state change for audit trail.

        Args:
            fraise_id: Fraise ID
            identifier: Fraise identifier
            old_state: Previous state
            new_state: New state
            changed_by: User or system that made the change
        """
        # Find what changed
        changes = {}
        for key in new_state:
            if old_state.get(key) != new_state.get(key):
                changes[key] = {
                    "old": old_state.get(key),
                    "new": new_state.get(key),
                }

        self.audit_logger.info(
            "fraise_state_changed",
            extra={
                "database_type": self.database_type,
                "fraise_id": fraise_id,
                "identifier": identifier,
                "changes": json.dumps(changes, default=str),
                "changed_by": changed_by or "system",
                "timestamp": time.time(),
            },
        )

        # Record deployment DB operation
        self.metrics.record_deployment_db_operation(
            database_type=self.database_type,
            operation_type="update_fraise_state",
        )

    def log_deployment_recorded(
        self,
        deployment_id: str,
        fraise_identifier: str,
        environment: str,
        status: str,
        triggered_by: str | None = None,
    ) -> None:
        """Log deployment recording for audit trail.

        Args:
            deployment_id: Deployment ID
            fraise_identifier: Fraise identifier
            environment: Target environment
            status: Deployment status
            triggered_by: User or system that triggered deployment
        """
        self.audit_logger.info(
            "deployment_recorded",
            extra={
                "database_type": self.database_type,
                "deployment_id": deployment_id,
                "fraise_identifier": fraise_identifier,
                "environment": environment,
                "status": status,
                "triggered_by": triggered_by or "system",
                "timestamp": time.time(),
            },
        )

        # Record deployment DB operation
        self.metrics.record_deployment_db_operation(
            database_type=self.database_type,
            operation_type="record_deployment",
        )

    def log_webhook_linked(
        self,
        webhook_id: str,
        deployment_id: str,
        event_type: str,
    ) -> None:
        """Log webhook-to-deployment linking for audit trail.

        Args:
            webhook_id: Webhook event ID
            deployment_id: Deployment ID
            event_type: Type of webhook event (push, pr, etc.)
        """
        self.audit_logger.info(
            "webhook_linked_to_deployment",
            extra={
                "database_type": self.database_type,
                "webhook_id": webhook_id,
                "deployment_id": deployment_id,
                "event_type": event_type,
                "timestamp": time.time(),
            },
        )

        # Record deployment DB operation
        self.metrics.record_deployment_db_operation(
            database_type=self.database_type,
            operation_type="link_webhook",
        )


def get_database_observability(database_type: str) -> DatabaseObservability:
    """Get database observability instance.

    Args:
        database_type: Database type

    Returns:
        DatabaseObservability instance
    """
    return DatabaseObservability(database_type)


def get_audit_logger(database_type: str) -> DeploymentDatabaseAudit:
    """Get deployment audit logger instance.

    Args:
        database_type: Database type

    Returns:
        DeploymentDatabaseAudit instance
    """
    return DeploymentDatabaseAudit(database_type)
