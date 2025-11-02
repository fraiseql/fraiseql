# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 19
# Add GraphQL query type
@strawberry.type
class AuditQuery:
    """GraphQL queries for audit system."""

    @strawberry.field
    async def verify_audit_chain(
        self, info: Info, tenant_id: Optional[UUID] = None
    ) -> AuditChainVerification:
        """Verify integrity of audit event chain."""
        repo = info.context["repo"]
        result = await verify_chain(repo, tenant_id=str(tenant_id) if tenant_id else None)

        return AuditChainVerification(
            valid=result["valid"],
            total_events=result["total_events"],
            broken_links=result["broken_links"],
            verification_timestamp=datetime.utcnow(),
        )


@strawberry.type
class AuditChainVerification:
    """Result of audit chain verification."""

    valid: bool
    total_events: int
    broken_links: int
    verification_timestamp: datetime
