# Extracted from: docs/enterprise/RBAC_POSTGRESQL_REFACTORED.md
# Block number: 10
# tests/integration/enterprise/rbac/test_role_hierarchy.py


async def test_role_inheritance_chain():
    """Verify role inherits permissions from parent roles."""
    from fraiseql.enterprise.rbac.hierarchy import RoleHierarchy

    # Create role chain: admin -> manager -> developer -> junior_dev
    hierarchy = RoleHierarchy(db_repo)
    inherited_roles = await hierarchy.get_inherited_roles("junior-dev-role-id")

    role_names = [r.name for r in inherited_roles]
    assert "junior_dev" in role_names
    assert "developer" in role_names
    assert "manager" in role_names
    assert "admin" in role_names
    # Expected failure: get_inherited_roles not implemented
