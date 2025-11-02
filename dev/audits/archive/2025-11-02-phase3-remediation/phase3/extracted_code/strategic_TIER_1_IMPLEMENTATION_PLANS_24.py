# Extracted from: docs/strategic/TIER_1_IMPLEMENTATION_PLANS.md
# Block number: 24
# tests/integration/enterprise/rbac/test_models.py


def test_role_model_creation():
    """Verify Role model instantiation."""
    from fraiseql.enterprise.rbac.models import Role

    role = Role(
        id="123e4567-e89b-12d3-a456-426614174000",
        name="developer",
        description="Software developer",
        parent_role_id="parent-role-123",
        tenant_id="tenant-123",
    )

    assert role.name == "developer"
    assert role.parent_role_id == "parent-role-123"
    # Expected failure: Role model not defined
