"""Multi-database adapter layer for Fraisier.

Provides a unified interface for working with different database backends:
- SQLite (development, testing)
- PostgreSQL (production)
- MySQL (alternative production)

Follows trait-based abstraction pattern from FraiseQL.
"""

from .adapter import (
    DatabaseType,
    FraiserDatabaseAdapter,
    PoolMetrics,
    QueryResult,
)
from .factory import create_adapter_from_url, get_database_adapter

__all__ = [
    "DatabaseType",
    "FraiserDatabaseAdapter",
    "PoolMetrics",
    "QueryResult",
    "create_adapter_from_url",
    "get_database_adapter",
]
