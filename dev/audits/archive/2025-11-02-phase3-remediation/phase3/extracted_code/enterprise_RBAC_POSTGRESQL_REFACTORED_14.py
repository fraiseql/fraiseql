# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 14
# Add to PermissionResolver class


async def check_permission(
    self,
    user_id: UUID,
    resource: str,
    action: str,
    tenant_id: UUID | None = None,
    raise_on_deny: bool = True,
) -> bool:
    """Check permission and optionally raise error.

    Args:
        user_id: User ID
        resource: Resource name
        action: Action name
        tenant_id: Optional tenant scope
        raise_on_deny: If True, raise PermissionError when denied

    Returns:
        True if permitted

    Raises:
        PermissionError: If raise_on_deny=True and permission denied
    """
    has_perm = await self.has_permission(user_id, resource, action, tenant_id)

    if not has_perm and raise_on_deny:
        raise PermissionError(f"Permission denied: requires {resource}.{action}")

    return has_perm


async def get_user_roles(self, user_id: UUID, tenant_id: UUID | None = None) -> list[Role]:
    """Get roles assigned to user (public method)."""
    return await self._get_user_roles(user_id, tenant_id)
