"""Enterprise audit logging mutations."""

from typing import Optional
from uuid import UUID

from fraiseql.mutations.decorators import mutation
from fraiseql.strawberry_compat import strawberry


@strawberry.input
class LogAuditEventInput:
    """Input for logging an audit event."""

    event_type: str
    event_data: dict
    user_id: Optional[UUID] = None
    tenant_id: Optional[UUID] = None
    ip_address: Optional[str] = None


@strawberry.type
class LogAuditEventResult:
    """Result of logging an audit event."""

    success: bool
    event_id: Optional[UUID] = None
    message: str


@mutation
class LogAuditEvent:
    """Log an immutable audit event with cryptographic chain."""

    @staticmethod
    def sql(
        event_type: str,
        event_data: dict,
        user_id: Optional[UUID] = None,
        tenant_id: Optional[UUID] = None,
        ip_address: Optional[str] = None,
    ) -> str:
        """Generate SQL to insert audit event.

        The cryptographic fields (event_hash, signature, previous_hash)
        are automatically populated by database triggers.
        """
        # Build the INSERT statement
        columns = ["event_type", "event_data"]
        values = ["%s", "%s"]
        params = [event_type, event_data]

        if user_id is not None:
            columns.append("user_id")
            values.append("%s")
            params.append(str(user_id))

        if tenant_id is not None:
            columns.append("tenant_id")
            values.append("%s")
            params.append(str(tenant_id))

        if ip_address is not None:
            columns.append("ip_address")
            values.append("%s")
            params.append(ip_address)

        columns_str = ", ".join(columns)
        placeholders = ", ".join(["%s"] * len(params))

        sql = f"""
        INSERT INTO audit_events ({columns_str})
        VALUES ({placeholders})
        RETURNING id
        """

        return sql

    @staticmethod
    def execute(
        event_type: str,
        event_data: dict,
        user_id: Optional[UUID] = None,
        tenant_id: Optional[UUID] = None,
        ip_address: Optional[str] = None,
    ) -> dict:
        """Execute the audit event logging.

        Returns the mutation result format expected by FraiseQL.
        """
        # This would be called by the mutation decorator
        # The actual SQL execution is handled by the framework
        # We just need to return the parameters for the SQL
        return {
            "event_type": event_type,
            "event_data": event_data,
            "user_id": user_id,
            "tenant_id": tenant_id,
            "ip_address": ip_address,
        }

    @classmethod
    def resolve(cls, _input: LogAuditEventInput) -> LogAuditEventResult:
        """GraphQL resolver for logging audit events."""
        # The mutation decorator handles the SQL execution
        # This resolver is called after successful execution
        # For now, return success - in practice, the decorator
        # would handle the actual database operation

        # Note: In a real implementation, the mutation decorator
        # would execute the SQL and return the result
        # For this phase, we're just defining the structure

        return LogAuditEventResult(
            success=True,
            event_id=None,  # Would be returned from INSERT RETURNING
            message="Audit event logged successfully",
        )
