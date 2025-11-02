# Extracted from: docs/guides/troubleshooting-decision-tree.md
# Block number: 4
# Check if user context is set
from fraiseql import authorized, mutation


@authorized(roles=["admin"])
@mutation
class DeletePost:
    async def resolve(self, info):
        # Check context
        print(f"User: {info.context.get('user')}")
        print(f"Roles: {info.context.get('roles')}")
