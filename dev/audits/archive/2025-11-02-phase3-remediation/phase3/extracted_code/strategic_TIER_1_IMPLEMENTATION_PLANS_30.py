# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 30
# tests/integration/enterprise/rbac/test_permission_resolution.py


async def test_user_effective_permissions():
    """Verify user permissions are computed from all assigned roles."""
    from fraiseql.enterprise.rbac.resolver import PermissionResolver

    # User has roles: [developer, team_lead]
    # developer inherits from: user
    # team_lead inherits from: developer
    # Expected permissions: all from user + developer + team_lead

    resolver = PermissionResolver(db_repo)
    permissions = await resolver.get_user_permissions("user-123")

    permission_actions = {f"{p.resource}.{p.action}" for p in permissions}
    assert "user.read" in permission_actions  # From 'user' role
    assert "code.write" in permission_actions  # From 'developer' role
    assert "team.manage" in permission_actions  # From 'team_lead' role
    # Expected failure: get_user_permissions not implemented
