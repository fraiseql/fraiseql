"""Synchronous accessor for GraphQL operation monitoring data.

Provides thread-safe synchronous access to operation monitoring metrics
without async/await overhead.
"""

from __future__ import annotations

import logging
from typing import Any, Optional

logger = logging.getLogger(__name__)


class OperationMonitor:
    """Placeholder for operation monitor (from Commit 4.5).

    This will be populated when OperationMonitor is available.
    """


# Global instance holder
_operation_monitor_instance: Optional[OperationMonitor] = None


def set_operation_monitor(monitor: OperationMonitor) -> None:
    """Set the global OperationMonitor instance.

    Args:
        monitor: OperationMonitor instance to use
    """
    global _operation_monitor_instance
    _operation_monitor_instance = monitor


def get_operation_monitor() -> OperationMonitor:
    """Get the global OperationMonitor instance.

    Returns:
        The current OperationMonitor instance
    """
    global _operation_monitor_instance
    if _operation_monitor_instance is None:
        _operation_monitor_instance = OperationMonitor()
    return _operation_monitor_instance


class OperationMonitorSync:
    """Synchronous accessor for GraphQL operation monitoring data.

    Provides fast, thread-safe access to operation monitoring metrics
    without async/await overhead.
    """

    def __init__(self, monitor: Optional[OperationMonitor] = None) -> None:
        """Initialize synchronous operation monitor accessor.

        Args:
            monitor: OperationMonitor instance. If None, uses global instance.
        """
        self._monitor = monitor or get_operation_monitor()

    def get_recent_operations(self, limit: int = 20) -> list[dict[str, Any]]:
        """Get recent GraphQL operations (synchronous).

        Args:
            limit: Maximum operations to return

        Returns:
            List of operation data
        """
        # Placeholder implementation
        # Will use actual OperationMonitor data when available
        if hasattr(self._monitor, "get_recent_operations"):
            return self._monitor.get_recent_operations(limit)
        return []

    def get_slow_operations(
        self, limit: int = 20, threshold_ms: float = 500.0
    ) -> list[dict[str, Any]]:
        """Get slow GraphQL operations (synchronous).

        Args:
            limit: Maximum operations to return
            threshold_ms: Slow operation threshold in milliseconds

        Returns:
            List of slow operation data
        """
        if hasattr(self._monitor, "get_slow_operations"):
            return self._monitor.get_slow_operations(limit, threshold_ms)
        return []

    def get_statistics(self) -> dict[str, Any]:
        """Get aggregate operation statistics (synchronous).

        Returns:
            Dictionary with operation statistics
        """
        if hasattr(self._monitor, "get_statistics"):
            return self._monitor.get_statistics()

        return {
            "total_operations": 0,
            "queries": 0,
            "mutations": 0,
            "subscriptions": 0,
            "success_rate": 0.0,
            "avg_duration_ms": 0.0,
            "error_rate": 0.0,
        }

    def get_operations_by_type(self) -> dict[str, int]:
        """Get operation count by type (synchronous).

        Returns:
            Dict mapping operation type to count
        """
        if hasattr(self._monitor, "get_operations_by_type"):
            return self._monitor.get_operations_by_type()

        return {
            "query": 0,
            "mutation": 0,
            "subscription": 0,
        }

    def get_status_string(self) -> str:
        """Get human-readable status string (synchronous).

        Returns:
            Status string for display
        """
        stats = self.get_statistics()

        return (
            f"GraphQL Operations\n"
            f"  Total: {stats.get('total_operations', 0)}\n"
            f"  Success Rate: {stats.get('success_rate', 0.0):.1f}%\n"
            f"  Avg Duration: {stats.get('avg_duration_ms', 0.0):.2f}ms\n"
            f"  Error Rate: {stats.get('error_rate', 0.0):.1f}%"
        )


# Create singleton instance for CLI use
operation_monitor_sync = OperationMonitorSync()
