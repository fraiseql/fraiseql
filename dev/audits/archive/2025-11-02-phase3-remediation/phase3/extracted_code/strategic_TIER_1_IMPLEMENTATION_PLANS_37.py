# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 37
# Update FraiseQLRepository to set RLS variables
# (Already in src/fraiseql/db.py - enhance it)


async def _set_session_variables(self, cursor_or_conn) -> None:
    """Set PostgreSQL session variables for RLS."""
    from psycopg.sql import SQL, Literal

    if "tenant_id" in self.context:
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.tenant_id = {}").format(Literal(str(self.context["tenant_id"])))
        )

    if "user_id" in self.context:
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.user_id = {}").format(Literal(str(self.context["user_id"])))
        )

    # Set super_admin flag based on user roles
    if "roles" in self.context:
        is_super_admin = any(r.name == "super_admin" for r in self.context["roles"])
        await cursor_or_conn.execute(
            SQL("SET LOCAL app.is_super_admin = {}").format(Literal(is_super_admin))
        )
