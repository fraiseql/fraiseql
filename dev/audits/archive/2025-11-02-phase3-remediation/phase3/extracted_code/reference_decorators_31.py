# Extracted from: docs/reference/decorators.md
# Block number: 31
from fraiseql import connection, query, type
from fraiseql.auth import requires_auth, requires_permission
from fraiseql.types import Connection


# Multiple decorators - order matters
@connection(node_type=User)
@query
@requires_auth
@requires_permission("users:read")
async def users_connection(info, first: int | None = None) -> Connection[User]:
    pass


# Field-level auth
@type
class User:
    id: UUID
    name: str

    @field(description="Private settings")
    @requires_auth
    async def settings(self, info) -> UserSettings:
        # Only accessible to authenticated users
        pass
