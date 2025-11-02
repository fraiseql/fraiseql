# Extracted from: docs/advanced/database-patterns.md
# Block number: 1
from uuid import UUID

from fraiseql import mutation


@mutation
async def update_order(info, id: UUID, status: str, notes: str | None = None) -> MutationLogResult:
    """Update order status."""
    db = info.context["db"]
    tenant_id = info.context["tenant_id"]
    user_id = info.context["user_id"]

    # Call SQL function (5-step pattern executed)
    result = await db.execute_mutation(
        """
        SELECT * FROM update_order(
            p_tenant_id := $1,
            p_user_id := $2,
            p_order_id := $3,
            p_status := $4,
            p_notes := $5
        )
        """,
        tenant_id,
        user_id,
        id,
        status,
        notes,
    )

    return MutationLogResult(
        status=result["status"],
        message=result["message"],
        op="update",
        entity="order",
        payload_before=result["object_data"].get("before"),
        payload_after=result["object_data"].get("after"),
        extra_metadata=result["extra_metadata"],
    )
