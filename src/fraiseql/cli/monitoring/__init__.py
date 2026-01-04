"""CLI monitoring commands for FraiseQL (Phase 19, Commit 7).

Provides command-line interface for monitoring and analyzing FraiseQL
system health, database performance, cache behavior, and GraphQL operations.

Usage:
    fraiseql monitoring database recent [--limit 20]
    fraiseql monitoring database slow [--threshold 100]
    fraiseql monitoring database pool
    fraiseql monitoring database stats

    fraiseql monitoring cache stats
    fraiseql monitoring cache health

    fraiseql monitoring graphql recent [--limit 20]
    fraiseql monitoring graphql stats
    fraiseql monitoring graphql slow [--threshold 500]

    fraiseql monitoring health
    fraiseql monitoring health database
    fraiseql monitoring health cache
"""

from .cache_commands import cache
from .database_commands import database
from .graphql_commands import graphql
from .health_commands import health

__all__ = [
    "cache",
    "database",
    "graphql",
    "health",
]
