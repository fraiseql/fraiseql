# Extracted from: docs/production/security.md
# Block number: 15
from fraiseql import mutation, query
from fraiseql.audit import SecurityEventSeverity, SecurityEventType, get_security_logger

security_logger = get_security_logger()


# Log authentication events
@mutation
async def login(info, username: str, password: str) -> dict:
    try:
        user = await authenticate_user(username, password)

        security_logger.log_auth_success(
            user_id=user.id,
            user_email=user.email,
            metadata={"ip": info.context["request"].client.host},
        )

        return {"token": generate_token(user)}

    except AuthenticationError as e:
        security_logger.log_auth_failure(
            reason=str(e),
            metadata={"username": username, "ip": info.context["request"].client.host},
        )
        raise


# Log data access
@query
@requires_permission("pii:read")
async def get_user_pii(info, user_id: str) -> UserPII:
    user = await fetch_user_pii(user_id)

    security_logger.log_event(
        SecurityEvent(
            event_type=SecurityEventType.DATA_ACCESS,
            severity=SecurityEventSeverity.INFO,
            user_id=info.context["user"].user_id,
            metadata={"accessed_user": user_id, "pii_fields": ["ssn", "credit_card"]},
        )
    )

    return user
