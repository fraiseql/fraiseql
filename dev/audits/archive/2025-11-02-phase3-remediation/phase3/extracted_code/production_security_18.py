# Extracted from: docs/production/security.md
# Block number: 18
from fraiseql import mutation

# Access control matrix
ROLE_PERMISSIONS = {
    "user": ["orders:read:self", "profile:write:self"],
    "manager": ["orders:read:team", "users:read:team"],
    "admin": ["admin:all"],
}


# Audit all administrative actions
@mutation
@requires_role("admin")
async def admin_update_user(info, user_id: str, data: dict) -> User:
    """Admin action - fully audited."""
    admin_user = info.context["user"]

    # Log before change
    before_state = await fetch_user(user_id)

    # Perform change
    updated_user = await update_user(user_id, data)

    # Log after change
    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.ADMIN_ACTION,
            severity=SecurityEventSeverity.WARNING,
            user_id=admin_user.user_id,
            metadata={
                "action": "update_user",
                "target_user": user_id,
                "before": before_state,
                "after": updated_user,
                "changed_fields": list(data.keys()),
            },
        )
    )

    return updated_user
