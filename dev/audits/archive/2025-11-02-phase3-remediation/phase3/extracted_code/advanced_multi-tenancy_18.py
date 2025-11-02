# Extracted from: docs/advanced/multi-tenancy.md
# Block number: 18
from uuid import uuid4

from fraiseql import mutation


@mutation
@requires_role("super_admin")
async def provision_tenant(
    info, name: str, subdomain: str, admin_email: str, plan: str = "basic"
) -> Organization:
    """Provision new tenant with admin user."""
    tenant_id = str(uuid4())

    async with db.connection() as conn, conn.transaction():
        # 1. Create organization
        result = await conn.execute(
            """
                INSERT INTO organizations (id, name, subdomain, plan, created_at)
                VALUES ($1, $2, $3, $4, NOW())
                RETURNING *
            """,
            tenant_id,
            name,
            subdomain,
            plan,
        )

        org = await result.fetchone()

        # 2. Create admin user
        admin_id = str(uuid4())
        await conn.execute(
            """
                INSERT INTO users (id, tenant_id, email, name, roles, created_at)
                VALUES ($1, $2, $3, $4, $5, NOW())
            """,
            admin_id,
            tenant_id,
            admin_email,
            "Admin User",
            ["admin"],
        )

        # 3. Create default data (optional)
        await conn.execute(
            """
                INSERT INTO settings (tenant_id, key, value)
                VALUES
                    ($1, 'theme', 'default'),
                    ($1, 'timezone', 'UTC'),
                    ($1, 'locale', 'en-US')
            """,
            tenant_id,
        )

        # 4. Initialize schema (if using schema-per-tenant)
        # await conn.execute(f"CREATE SCHEMA IF NOT EXISTS tenant_{tenant_id}")
        # Run migrations for tenant schema

    # 5. Send welcome email
    await send_welcome_email(admin_email, subdomain)

    return Organization(**org)
