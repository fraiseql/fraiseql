# Extracted from: docs/enterprise/RBAC_POSTGRESQL_ASSESSMENT.md
# Block number: 6
async def assign_role(user_id, role_id):
    # 1. Update database
    await db.execute("INSERT INTO user_roles ...")

    # 2. Manually invalidate cache (MUST REMEMBER)
    await redis.delete(f"rbac:permissions:{user_id}:*")

    # 3. What if role hierarchy changed?
    # Must manually invalidate ALL users with parent roles
    # Complex logic, easy to miss edge cases
