# Extracted from: docs/diagrams/multi-tenant-isolation.md
# Block number: 2
from fraiseql import mutation, query


@query
async def users(self, info) -> list[User]:
    tenant_id = tenant_context.get()
    return await db.execute("SELECT * FROM v_user WHERE tenant_id = $1", [tenant_id])


@mutation
async def create_user(self, info, input: CreateUserInput) -> User:
    tenant_id = tenant_context.get()
    user_id = await db.execute_scalar(
        "SELECT fn_create_user($1, $2, $3)", [input.email, input.name, tenant_id]
    )
    return await self.user(info, id=user_id)
