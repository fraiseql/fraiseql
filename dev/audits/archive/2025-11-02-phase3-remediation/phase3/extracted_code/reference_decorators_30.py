# Extracted from: docs/reference/decorators.md
# Block number: 30
from fraiseql import query
from fraiseql.auth import requires_any_role


@query
@requires_any_role("admin", "moderator")
async def moderate_content(info, id: UUID) -> ModerationResult:
    # Can be performed by admin OR moderator
    pass
