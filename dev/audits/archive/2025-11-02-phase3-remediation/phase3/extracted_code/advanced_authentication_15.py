# Extracted from: docs/advanced/authentication.md
# Block number: 15
from fraiseql import mutation
from fraiseql.auth import requires_any_role


@mutation
@requires_any_role("admin", "moderator")
async def moderate_content(info, content_id: str, action: str) -> bool:
    """Moderate content - admin or moderator."""
    await moderate_content_by_id(content_id, action)
    return True
