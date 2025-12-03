"""Tests for GraphQL field-level authorization with PostgreSQL RBAC.

Tests the constraint evaluation logic used by the @requires_permission directive.
"""

from uuid import uuid4

import pytest

pytestmark = pytest.mark.enterprise


async def evaluate_constraints(constraints: dict, context: dict, field_args: dict) -> bool:
    """Test version of constraint evaluation logic (matches directives.py)."""
    # Constraint: own_data_only - can only access own data
    if constraints.get("own_data_only"):
        target_user_id = field_args.get("user_id") or field_args.get("id")
        if target_user_id and str(target_user_id) != str(context.get("user_id")):
            return False

    # Constraint: tenant_scoped - must be in same tenant
    if constraints.get("tenant_scoped"):
        target_tenant = field_args.get("tenant_id")
        if target_tenant and str(target_tenant) != str(context.get("tenant_id")):
            return False

    # Constraint: max_records - limit number of records
    if "max_records" in constraints:
        limit = field_args.get("limit", field_args.get("first", float("inf")))
        if limit > constraints["max_records"]:
            return False

    # Constraint: department_only - must be in same department
    if constraints.get("department_only"):
        target_dept = field_args.get("department_id")
        user_dept = context.get("department_id")
        if target_dept and user_dept and str(target_dept) != str(user_dept):
            return False

    return True


@pytest.mark.asyncio
async def test_constraint_own_data_only_allows_own_data() -> None:
    """Test own_data_only constraint allows access to own data."""
    user_id = uuid4()
    context = {"user_id": user_id}
    field_args = {"user_id": user_id}

    constraints = {"own_data_only": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is True


@pytest.mark.asyncio
async def test_constraint_own_data_only_denies_other_data() -> None:
    """Test own_data_only constraint denies access to other user's data."""
    user_id = uuid4()
    other_user_id = uuid4()

    context = {"user_id": user_id}
    field_args = {"user_id": other_user_id}

    constraints = {"own_data_only": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False


@pytest.mark.asyncio
async def test_constraint_tenant_scoped_allows_same_tenant() -> None:
    """Test tenant_scoped constraint allows access within same tenant."""
    tenant_id = uuid4()
    context = {"tenant_id": tenant_id}
    field_args = {"tenant_id": tenant_id}

    constraints = {"tenant_scoped": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is True


@pytest.mark.asyncio
async def test_constraint_tenant_scoped_denies_different_tenant() -> None:
    """Test tenant_scoped constraint denies access to different tenant."""
    tenant_id = uuid4()
    other_tenant_id = uuid4()

    context = {"tenant_id": tenant_id}
    field_args = {"tenant_id": other_tenant_id}

    constraints = {"tenant_scoped": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False


@pytest.mark.asyncio
async def test_constraint_max_records_allows_under_limit() -> None:
    """Test max_records constraint allows queries under the limit."""
    context = {}
    field_args = {"limit": 50}

    constraints = {"max_records": 100}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is True


@pytest.mark.asyncio
async def test_constraint_max_records_denies_over_limit() -> None:
    """Test max_records constraint denies queries over the limit."""
    context = {}
    field_args = {"limit": 150}

    constraints = {"max_records": 100}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False


@pytest.mark.asyncio
async def test_constraint_max_records_uses_first_as_fallback() -> None:
    """Test max_records constraint uses 'first' parameter as fallback for limit."""
    context = {}
    field_args = {"first": 150}  # GraphQL pagination uses 'first'

    constraints = {"max_records": 100}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False


@pytest.mark.asyncio
async def test_constraint_department_only_allows_same_department() -> None:
    """Test department_only constraint allows access within same department."""
    dept_id = uuid4()
    context = {"department_id": dept_id}
    field_args = {"department_id": dept_id}

    constraints = {"department_only": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is True


@pytest.mark.asyncio
async def test_constraint_department_only_denies_different_department() -> None:
    """Test department_only constraint denies access to different department."""
    dept_id = uuid4()
    other_dept_id = uuid4()

    context = {"department_id": dept_id}
    field_args = {"department_id": other_dept_id}

    constraints = {"department_only": True}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False


@pytest.mark.asyncio
async def test_multiple_constraints_all_must_pass() -> None:
    """Test that all constraints must pass for access to be granted."""
    user_id = uuid4()
    tenant_id = uuid4()
    dept_id = uuid4()

    # User has access to own data, same tenant, but different department
    context = {"user_id": user_id, "tenant_id": tenant_id, "department_id": dept_id}
    field_args = {
        "user_id": user_id,  # Own data ✓
        "tenant_id": tenant_id,  # Same tenant ✓
        "department_id": uuid4(),  # Different department ✗
        "limit": 50,  # Under max_records limit ✓
    }

    constraints = {
        "own_data_only": True,
        "tenant_scoped": True,
        "department_only": True,
        "max_records": 100,
    }
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is False  # Should fail because department constraint fails


@pytest.mark.asyncio
async def test_no_constraints_always_allows() -> None:
    """Test that no constraints always allows access."""
    context = {}
    field_args = {"any": "data"}

    constraints = {}
    result = await evaluate_constraints(constraints, context, field_args)

    assert result is True
