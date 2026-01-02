"""Audit logging for GraphQL operations.

This module provides production-ready audit logging with:
- Multi-tenant isolation
- PostgreSQL backend with JSONB storage
- Async/await support
- Comprehensive query tracking
"""

from __future__ import annotations

import json
from datetime import datetime
from enum import Enum
from typing import Any

from fraiseql._fraiseql_rs import DatabasePool as RustDatabasePool
from fraiseql._fraiseql_rs import PyAuditLogger


class AuditLevel(Enum):
    """Audit log severity levels."""

    INFO = "INFO"
    WARN = "WARN"
    ERROR = "ERROR"


class AuditLogger:
    """High-performance audit logger with PostgreSQL backend.

    Uses Rust implementation for 10-100x faster logging than Python.

    Examples:
        >>> from fraiseql.db import DatabasePool
        >>> from fraiseql.enterprise.security import AuditLogger, AuditLevel
        >>>
        >>> pool = DatabasePool("postgresql://localhost/mydb")
        >>> logger = AuditLogger(pool)
        >>>
        >>> # Log a successful query
        >>> entry_id = await logger.log(
        ...     level=AuditLevel.INFO,
        ...     user_id=123,
        ...     tenant_id=1,
        ...     operation="query",
        ...     query="{ users { id name } }",
        ...     variables={},
        ...     ip_address="192.168.1.100",
        ...     user_agent="GraphQL Client/1.0",
        ...     duration_ms=42
        ... )
        >>>
        >>> # Log a failed mutation
        >>> entry_id = await logger.log(
        ...     level=AuditLevel.ERROR,
        ...     user_id=456,
        ...     tenant_id=1,
        ...     operation="mutation",
        ...     query="mutation { deleteUser(id: 999) }",
        ...     variables={"id": 999},
        ...     ip_address="10.0.0.50",
        ...     user_agent="Mobile App/2.0",
        ...     error="User not found",
        ...     duration_ms=15
        ... )
        >>>
        >>> # Get recent logs for tenant
        >>> logs = await logger.get_recent_logs(
        ...     tenant_id=1,
        ...     level=AuditLevel.ERROR,
        ...     limit=100
        ... )
    """

    def __init__(self, pool: RustDatabasePool) -> None:
        """Initialize audit logger.

        Args:
            pool: Rust database pool for PostgreSQL operations
        """
        self._logger = PyAuditLogger(pool)

    async def log(
        self,
        *,
        level: AuditLevel,
        user_id: int,
        tenant_id: int,
        operation: str,
        query: str,
        variables: dict[str, Any] | None = None,
        ip_address: str,
        user_agent: str,
        error: str | None = None,
        duration_ms: int | None = None,
    ) -> int:
        """Log an audit entry.

        Args:
            level: Log severity level
            user_id: User performing the operation
            tenant_id: Tenant ID for multi-tenant isolation
            operation: Operation type ("query" or "mutation")
            query: GraphQL query string
            variables: Query variables (optional)
            ip_address: Client IP address
            user_agent: Client user agent string
            error: Error message if operation failed (optional)
            duration_ms: Query execution time in milliseconds (optional)

        Returns:
            ID of the created audit log entry

        Raises:
            RuntimeError: If logging fails
        """
        variables_json = json.dumps(variables if variables is not None else {})

        return await self._logger.log(
            level=level.value,
            user_id=user_id,
            tenant_id=tenant_id,
            operation=operation,
            query=query,
            variables=variables_json,
            ip_address=ip_address,
            user_agent=user_agent,
            error=error,
            duration_ms=duration_ms,
        )

    async def get_recent_logs(
        self,
        tenant_id: int,
        level: AuditLevel | None = None,
        limit: int = 100,
    ) -> list[dict[str, Any]]:
        """Get recent audit logs for a tenant.

        Args:
            tenant_id: Tenant ID to filter by
            level: Optional log level filter
            limit: Maximum number of logs to return (default: 100)

        Returns:
            List of audit log entries as dictionaries with keys:
            - id: Log entry ID
            - timestamp: ISO 8601 timestamp
            - level: Log level (INFO, WARN, ERROR)
            - user_id: User ID
            - tenant_id: Tenant ID
            - operation: Operation type
            - query: GraphQL query string
            - variables: Query variables (JSON string)
            - ip_address: Client IP
            - user_agent: Client user agent
            - error: Error message (if any)
            - duration_ms: Execution time in ms (if recorded)

        Raises:
            RuntimeError: If retrieval fails
        """
        level_str = level.value if level is not None else None

        logs = await self._logger.get_recent_logs(
            tenant_id=tenant_id,
            level=level_str,
            limit=limit,
        )

        # Convert variables from JSON string to dict
        for log in logs:
            if "variables" in log and isinstance(log["variables"], str):
                log["variables"] = json.loads(log["variables"])
            if "timestamp" in log and isinstance(log["timestamp"], str):
                log["timestamp"] = datetime.fromisoformat(log["timestamp"].replace("Z", "+00:00"))

        return logs


__all__ = ["AuditLevel", "AuditLogger"]
