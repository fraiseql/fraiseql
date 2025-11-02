# Extracted from: docs/advanced/authentication.md
# Block number: 28
from fraiseql.audit import SecurityEventType, get_security_logger

security_logger = get_security_logger()

# Log successful authentication
security_logger.log_auth_success(
    user_id=user.user_id, user_email=user.email, metadata={"provider": "auth0", "roles": user.roles}
)

# Log failed authentication
security_logger.log_auth_failure(
    reason="Invalid token", metadata={"token_type": "bearer", "error": str(error)}
)

# Log authorization failure
security_logger.log_event(
    SecurityEvent(
        event_type=SecurityEventType.AUTH_PERMISSION_DENIED,
        severity=SecurityEventSeverity.WARNING,
        user_id=user.user_id,
        metadata={"required_permission": "orders:delete", "resource": order_id},
    )
)
