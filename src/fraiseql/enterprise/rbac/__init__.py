"""RBAC (Role-Based Access Control) Module.

This module implements hierarchical role-based access control with PostgreSQL-native caching.
Supports 10,000+ users with automatic permission invalidation via domain versioning.

Key Features:
- Hierarchical roles with inheritance
- PostgreSQL-native permission caching (0.1-0.3ms)
- Automatic cache invalidation via domain versioning
- Multi-tenant support
- Field-level authorization integration

Architecture:
- 2-layer cache: request-level (in-memory) + PostgreSQL (UNLOGGED table)
- Domain versioning for automatic invalidation
- CASCADE rules for hierarchical cache invalidation
"""

import logging

from psycopg_pool import AsyncConnectionPool

logger = logging.getLogger(__name__)


async def setup_rbac_cache(db_pool: AsyncConnectionPool) -> None:
    """Initialize RBAC cache domains and CASCADE rules.

    This should be called during application startup with a database pool.

    Args:
        db_pool: PostgreSQL connection pool

    Sets up:
    - Table triggers for automatic domain versioning
    - CASCADE rules for hierarchical invalidation
    - RBAC-specific cache domains

    Gracefully handles missing pg_fraiseql_cache extension.
    """
    from fraiseql.caching import PostgresCache

    cache = PostgresCache(db_pool)

    if not cache.has_domain_versioning:
        logger.warning(
            "pg_fraiseql_cache extension not available. "
            "RBAC will use TTL-only caching without automatic invalidation."
        )
        return

    # Setup table triggers (idempotent)
    await cache.setup_table_trigger("roles", domain_name="role", tenant_column="tenant_id")
    await cache.setup_table_trigger("permissions", domain_name="permission")
    await cache.setup_table_trigger("role_permissions", domain_name="role_permission")
    await cache.setup_table_trigger(
        "user_roles", domain_name="user_role", tenant_column="tenant_id"
    )

    # Setup CASCADE rules (idempotent)
    await cache.register_cascade_rule("role", "user_permissions")
    await cache.register_cascade_rule("permission", "user_permissions")
    await cache.register_cascade_rule("role_permission", "user_permissions")
    await cache.register_cascade_rule("user_role", "user_permissions")

    logger.info("✓ RBAC cache domains and CASCADE rules configured")


from . import cache, hierarchy, models, resolver, types

__all__ = ["cache", "hierarchy", "models", "resolver", "setup_rbac_cache", "types"]
