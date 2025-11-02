# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 3
# src/fraiseql/enterprise/rbac/__init__.py

import logging

from fraiseql.caching import get_cache

logger = logging.getLogger(__name__)


async def setup_rbac_cache():
    """Initialize RBAC cache domains and CASCADE rules.

    This should be called during application startup.
    """
    cache = get_cache()

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
