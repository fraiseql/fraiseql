# Extracted from: docs/production/security.md
# Block number: 17
from fraiseql import mutation


@mutation
@requires_auth
async def export_my_data(info) -> str:
    """GDPR: Export all user data."""
    user_id = info.context["user"].user_id

    # Gather all user data
    data = {
        "user": await fetch_user(user_id),
        "orders": await fetch_user_orders(user_id),
        "activity": await fetch_user_activity(user_id),
        "consents": await fetch_user_consents(user_id),
    }

    # Log export
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_EXPORT,
            severity=SecurityEventSeverity.INFO,
            user_id=user_id,
        )
    )

    return json.dumps(data, default=str)


@mutation
@requires_auth
async def delete_my_account(info) -> bool:
    """GDPR: Right to be forgotten."""
    user_id = info.context["user"].user_id

    async with db.connection() as conn, conn.transaction():
        # Anonymize or delete data
        await conn.execute(
            "UPDATE users SET email = $1, name = $2, deleted_at = NOW() WHERE id = $3",
            f"deleted-{user_id}@deleted.com",
            "Deleted User",
            user_id,
        )

        # Delete related data
        await conn.execute("DELETE FROM user_sessions WHERE user_id = $1", user_id)
        await conn.execute("DELETE FROM user_consents WHERE user_id = $1", user_id)

    # Log deletion
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_DELETION,
            severity=SecurityEventSeverity.WARNING,
            user_id=user_id,
        )
    )

    return True
