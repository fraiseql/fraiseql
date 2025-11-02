# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 15
# src/fraiseql/enterprise/audit/interceptors.py

from typing import Any, Callable

from graphql import GraphQLResolveInfo

from fraiseql.enterprise.audit.event_logger import AuditLogger


class AuditInterceptor:
    """Intercepts GraphQL mutations for audit logging."""

    def __init__(self, audit_logger: AuditLogger):
        self.logger = audit_logger

    async def intercept_mutation(
        self, next_resolver: Callable, obj: Any, info: GraphQLResolveInfo, **kwargs
    ):
        """Intercept mutation execution and log to audit trail."""
        # Execute mutation
        result = await next_resolver(obj, info, **kwargs)

        # Log to audit trail
        context = info.context
        await self.logger.log_event(
            event_type=f"mutation.{info.field_name}",
            event_data={"input": kwargs, "result": result},
            user_id=context.get("user_id"),
            tenant_id=context.get("tenant_id"),
            ip_address=context.get("ip"),
        )

        return result
