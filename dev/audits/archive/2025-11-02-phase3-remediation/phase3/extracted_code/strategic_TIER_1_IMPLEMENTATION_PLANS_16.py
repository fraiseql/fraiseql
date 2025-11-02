# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 16
class AuditInterceptor:
    """GraphQL mutation interceptor with configurable audit logging."""

    def __init__(
        self,
        audit_logger: AuditLogger,
        exclude_fields: set[str] | None = None,
        pii_fields: set[str] | None = None,
    ):
        self.logger = audit_logger
        self.exclude_fields = exclude_fields or set()
        self.pii_fields = pii_fields or {"password", "ssn", "credit_card"}

    async def intercept_mutation(
        self, next_resolver: Callable, obj: Any, info: GraphQLResolveInfo, **kwargs
    ):
        """Intercept and log mutation with PII filtering."""
        mutation_name = info.field_name

        # Skip excluded mutations
        if mutation_name in self.exclude_fields:
            return await next_resolver(obj, info, **kwargs)

        # Filter PII from input
        filtered_input = self._filter_pii(kwargs)

        # Execute mutation
        start_time = datetime.utcnow()
        try:
            result = await next_resolver(obj, info, **kwargs)
            success = True
            error = None
        except Exception as e:
            success = False
            error = str(e)
            raise
        finally:
            # Log audit event (even on failure)
            duration_ms = (datetime.utcnow() - start_time).total_seconds() * 1000

            context = info.context
            await self.logger.log_event(
                event_type=f"mutation.{mutation_name}",
                event_data={
                    "input": filtered_input,
                    "success": success,
                    "error": error,
                    "duration_ms": duration_ms,
                },
                user_id=context.get("user_id"),
                tenant_id=context.get("tenant_id"),
                ip_address=context.get("ip"),
            )

        return result

    def _filter_pii(self, data: dict[str, Any]) -> dict[str, Any]:
        """Remove PII fields from data before logging."""
        filtered = {}
        for key, value in data.items():
            if key in self.pii_fields:
                filtered[key] = "[REDACTED]"
            elif isinstance(value, dict):
                filtered[key] = self._filter_pii(value)
            else:
                filtered[key] = value
        return filtered
