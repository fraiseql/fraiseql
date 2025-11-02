# Extracted from: docs/advanced/authentication.md
# Block number: 24
# Permission-based
@authorize_field(lambda obj, info: info.context["user"].has_permission("users:read_pii"))
async def ssn(self) -> str:
    return self._ssn


# Role-based
@authorize_field(lambda obj, info: info.context["user"].has_role("admin"))
async def audit_log(self) -> list[AuditEvent]:
    return self._audit_log


# Owner-based
@authorize_field(lambda order, info: order.user_id == info.context["user"].user_id)
async def payment_details(self) -> PaymentDetails:
    return self._payment_details


# Combined
@authorize_field(
    lambda obj, info: (
        info.context["user"].has_permission("orders:read_all")
        or obj.user_id == info.context["user"].user_id
    )
)
async def internal_status(self) -> str:
    return self._internal_status
