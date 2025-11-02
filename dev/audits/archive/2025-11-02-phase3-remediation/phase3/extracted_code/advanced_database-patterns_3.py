# Extracted from: docs/advanced/database-patterns.md
# Block number: 3
from fraiseql import mutation


@mutation
async def update_order(info, id: UUID, name: str) -> MutationResult:
    db = info.context["db"]

    # Log the mutation
    result = await db.execute(
        """
        INSERT INTO core.tb_entity_change_log
            (tenant_id, user_id, object_type, object_id,
             modification_type, change_status, object_data)
        VALUES
            ($1, $2, 'order', $3, 'UPDATE', 'updated', $4::jsonb)
        RETURNING id
        """,
        info.context["tenant_id"],
        info.context["user_id"],
        id,
        json.dumps({"before": {"name": old_name}, "after": {"name": name}}),
    )

    return MutationResult(status="updated", id=id)
