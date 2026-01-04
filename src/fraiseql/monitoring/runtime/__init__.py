"""Synchronous runtime accessors for monitoring data.

Provides synchronous access to monitoring systems for CLI commands and
synchronous contexts. Wraps async monitoring APIs with sync accessors
that use existing thread-safe locks.

This layer bridges the gap between:
- Async monitoring systems (designed for FastAPI/async contexts)
- Synchronous CLI commands (Click framework)

Example:
    >>> from fraiseql.monitoring.runtime import DatabaseMonitorSync
    >>> db_monitor_sync = DatabaseMonitorSync()
    >>> queries = db_monitor_sync.get_recent_queries(limit=10)
    >>> for query in queries:
    ...     print(f"{query.query_type}: {query.duration_ms}ms")
"""

from .cache_monitor_sync import CacheMonitorSync
from .db_monitor_sync import DatabaseMonitorSync
from .operation_monitor_sync import OperationMonitorSync

__all__ = [
    "CacheMonitorSync",
    "DatabaseMonitorSync",
    "OperationMonitorSync",
]
