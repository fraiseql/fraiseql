# Extracted from: docs/advanced/event-sourcing.md
# Block number: 5
from fraiseql import query


@type_
class AuditSummary:
    total_changes: int
    changes_by_operation: dict[str, int]
    changes_by_user: dict[str, int]
    recent_changes: list[EntityChange]


@query
@requires_role("auditor")
async def get_audit_summary(
    info,
    entity_type: str | None = None,
    from_time: datetime | None = None,
    to_time: datetime | None = None,
) -> AuditSummary:
    """Get comprehensive audit summary."""
    async with get_db_pool().connection() as conn:
        # Total changes
        result = await conn.execute(
            """
            SELECT COUNT(*) as total
            FROM audit.entity_change_log
            WHERE ($1::TEXT IS NULL OR entity_type = $1)
              AND ($2::TIMESTAMPTZ IS NULL OR changed_at >= $2)
              AND ($3::TIMESTAMPTZ IS NULL OR changed_at <= $3)
        """,
            entity_type,
            from_time,
            to_time,
        )
        total = (await result.fetchone())["total"]

        # By operation
        result = await conn.execute(
            """
            SELECT operation, COUNT(*) as count
            FROM audit.entity_change_log
            WHERE ($1::TEXT IS NULL OR entity_type = $1)
              AND ($2::TIMESTAMPTZ IS NULL OR changed_at >= $2)
              AND ($3::TIMESTAMPTZ IS NULL OR changed_at <= $3)
            GROUP BY operation
        """,
            entity_type,
            from_time,
            to_time,
        )
        by_operation = {row["operation"]: row["count"] for row in await result.fetchall()}

        # By user
        result = await conn.execute(
            """
            SELECT changed_by::TEXT, COUNT(*) as count
            FROM audit.entity_change_log
            WHERE changed_by IS NOT NULL
              AND ($1::TEXT IS NULL OR entity_type = $1)
              AND ($2::TIMESTAMPTZ IS NULL OR changed_at >= $2)
              AND ($3::TIMESTAMPTZ IS NULL OR changed_at <= $3)
            GROUP BY changed_by
            ORDER BY count DESC
            LIMIT 10
        """,
            entity_type,
            from_time,
            to_time,
        )
        by_user = {row["changed_by"]: row["count"] for row in await result.fetchall()}

        # Recent changes
        result = await conn.execute(
            """
            SELECT * FROM audit.entity_change_log
            WHERE ($1::TEXT IS NULL OR entity_type = $1)
              AND ($2::TIMESTAMPTZ IS NULL OR changed_at >= $2)
              AND ($3::TIMESTAMPTZ IS NULL OR changed_at <= $3)
            ORDER BY changed_at DESC
            LIMIT 50
        """,
            entity_type,
            from_time,
            to_time,
        )
        recent = [EntityChange(**row) for row in await result.fetchall()]

    return AuditSummary(
        total_changes=total,
        changes_by_operation=by_operation,
        changes_by_user=by_user,
        recent_changes=recent,
    )
