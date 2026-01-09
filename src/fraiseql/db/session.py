"""PostgreSQL session variable management for FraiseQL.

Sets application-level context variables for Row-Level Security (RLS) and
multi-tenant support via PostgreSQL session variables.

Session Variables Set:
- app.tenant_id: Current tenant ID (multi-tenancy)
- app.contact_id: Current contact/user ID (optional, fallback to app.user)
- app.user_id: Current user ID (RBAC)
- app.is_super_admin: Boolean flag for super admin role

These variables are set at transaction start via SET LOCAL to scope
them to the current transaction, ensuring proper RLS application.

Supports multiple database connection APIs:
- psycopg3 AsyncConnection and AsyncCursor
- asyncpg Connection (compatibility mode)
"""

import logging
from typing import Any

logger = logging.getLogger(__name__)


async def set_session_variables(cursor_or_conn: Any, context: dict[str, Any]) -> None:
    """Set PostgreSQL session variables from context for RLS.

    Sets app.tenant_id, app.contact_id, app.user_id, and app.is_super_admin
    session variables if present in context. Uses SET LOCAL to scope variables
    to the current transaction.

    Supports both psycopg cursor and asyncpg connection APIs by detecting
    the connection type automatically.

    Args:
        cursor_or_conn: Either a psycopg AsyncCursor or an asyncpg Connection
        context: Dictionary containing context variables:
            - tenant_id: Tenant ID (multi-tenancy)
            - contact_id or user_id: Contact/user identifier (optional)
            - is_super_admin: Boolean flag for super admin (optional)
    """
    from psycopg.sql import SQL, Literal

    # Check if this is a cursor (psycopg) or connection (asyncpg)
    is_cursor = hasattr(cursor_or_conn, "execute") and hasattr(cursor_or_conn, "fetchone")

    if "tenant_id" in context:
        if is_cursor:
            await cursor_or_conn.execute(
                SQL("SET LOCAL app.tenant_id = {}").format(Literal(context["tenant_id"])),
            )
        else:
            # asyncpg connection
            await cursor_or_conn.execute(f"SET LOCAL app.tenant_id = '{context['tenant_id']}'")

    # app.contact_id (with fallback to app.user)
    contact_id = context.get("contact_id") or context.get("user_id")
    if contact_id:
        if is_cursor:
            await cursor_or_conn.execute(
                SQL("SET LOCAL app.contact_id = {}").format(Literal(contact_id)),
            )
        else:
            await cursor_or_conn.execute(f"SET LOCAL app.contact_id = '{contact_id}'")

        # Also set app.user for backward compatibility
        if is_cursor:
            await cursor_or_conn.execute(
                SQL("SET LOCAL app.user = {}").format(Literal(contact_id)),
            )
        else:
            await cursor_or_conn.execute(f"SET LOCAL app.user = '{contact_id}'")

    # app.user_id for RBAC
    if "user_id" in context:
        if is_cursor:
            await cursor_or_conn.execute(
                SQL("SET LOCAL app.user_id = {}").format(Literal(context["user_id"])),
            )
        else:
            await cursor_or_conn.execute(f"SET LOCAL app.user_id = '{context['user_id']}'")

    # app.is_super_admin: Determine from context or database
    is_super_admin = context.get("is_super_admin", False)

    if not is_super_admin and "user_id" in context:
        # Try to query the database for super admin status if not provided
        try:
            if is_cursor:
                await cursor_or_conn.execute(
                    SQL("SELECT is_super_admin FROM app_users WHERE id = {}").format(
                        Literal(context["user_id"]),
                    ),
                )
                result = await cursor_or_conn.fetchone()
                is_super_admin = result[0] if result else False
            else:
                result = await cursor_or_conn.fetchrow(
                    "SELECT is_super_admin FROM app_users WHERE id = $1",
                    context["user_id"],
                )
                is_super_admin = result["is_super_admin"] if result else False
        except Exception as e:
            logger.debug(f"Could not query super admin status: {e}")
            is_super_admin = False

    # Set app.is_super_admin
    if is_cursor:
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.is_super_admin = {}").format(
                Literal("true" if is_super_admin else "false"),
            ),
        )
    else:
        await cursor_or_conn.execute(
            f"SET LOCAL app.is_super_admin = {'true' if is_super_admin else 'false'}",
        )


async def clear_session_variables(cursor_or_conn: Any) -> None:
    """Clear PostgreSQL session variables.

    Resets all app.* session variables to default values.
    Note: SET LOCAL automatically clears at transaction end,
    so this is optional unless using persistent connections.

    Args:
        cursor_or_conn: Either a psycopg AsyncCursor or an asyncpg Connection
    """
    from psycopg.sql import SQL

    # Check if this is a cursor (psycopg) or connection (asyncpg)
    is_cursor = hasattr(cursor_or_conn, "execute") and hasattr(cursor_or_conn, "fetchone")

    session_vars = [
        "app.tenant_id",
        "app.contact_id",
        "app.user_id",
        "app.is_super_admin",
        "app.user",
    ]

    for var in session_vars:
        try:
            if is_cursor:
                await cursor_or_conn.execute(SQL(f"SET LOCAL {var} = DEFAULT"))
            else:
                await cursor_or_conn.execute(f"SET LOCAL {var} = DEFAULT")
        except Exception as e:
            logger.debug(f"Could not reset session variable {var}: {e}")
