# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 17
from fraiseql import mutation


@mutation
@requires_permission("tenant:import")
async def import_tenant_data(info, data: str) -> bool:
    """Import tenant data from JSON."""
    tenant_id = info.context["tenant_id"]
    import_data = json.loads(data)

    async with db.connection() as conn, conn.transaction():
        # Import users
        for user_data in import_data.get("users", []):
            user_data["tenant_id"] = tenant_id  # Force current tenant
            await conn.execute(
                """
                    INSERT INTO users (id, tenant_id, email, name, created_at)
                    VALUES ($1, $2, $3, $4, $5)
                    ON CONFLICT (id) DO UPDATE SET
                        email = EXCLUDED.email,
                        name = EXCLUDED.name
                """,
                user_data["id"],
                user_data["tenant_id"],
                user_data["email"],
                user_data["name"],
                user_data["created_at"],
            )

        # Import orders
        for order_data in import_data.get("orders", []):
            order_data["tenant_id"] = tenant_id
            await conn.execute(
                """
                    INSERT INTO orders (id, tenant_id, user_id, total, status, created_at)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (id) DO UPDATE SET
                        total = EXCLUDED.total,
                        status = EXCLUDED.status
                """,
                order_data["id"],
                order_data["tenant_id"],
                order_data["user_id"],
                order_data["total"],
                order_data["status"],
                order_data["created_at"],
            )

    return True
