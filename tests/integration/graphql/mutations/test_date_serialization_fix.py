import pytest

"""Test that date fields in mutations are properly serialized."""

import asyncio
from datetime import date
from unittest.mock import AsyncMock, MagicMock

import fraiseql
from fraiseql.mutations.mutation_decorator import mutation


@pytest.mark.unit
@fraiseql.input
class CreateOrderInput:
    """Input for creating a new order."""

    client_order_id: str
    order_date: date | None = None


@fraiseql.type
class Order:
    """Order type."""

    id: str
    client_order_id: str
    order_date: date | None = None


@fraiseql.success
class CreateOrderSuccess:
    """Success response."""

    order: Order | None = None
    status: str = "success"
    message: str = "Order created"


@fraiseql.failure
class CreateOrderError:
    """Error response."""

    status: str = "error"
    message: str = "Failed to create order"
    errors: list = []


@mutation(function="create_order", schema="test")
class CreateOrder:
    """Create a new order."""

    input: CreateOrderInput
    success: CreateOrderSuccess
    failure: CreateOrderError


def test_mutation_with_date_field():
    """Test that date fields in mutations are properly serialized for database calls."""
    # Create the mutation definition
    definition = CreateOrder.__fraiseql_mutation__
    assert definition is not None
    assert definition.function_name == "create_order"

    # Create a resolver
    resolver = definition.create_resolver()

    # Create mock info object with database
    mock_db = AsyncMock()
    mock_db.execute_function = AsyncMock(
        return_value={
            "status": "success",
            "message": "Order created successfully",
            "data": {"id": "test-id", "client_order_id": "ORDER2025", "order_date": "2025-02-15"},
        }
    )

    mock_info = MagicMock()
    mock_info.context = {"db": mock_db}

    # Create input with date
    input_obj = CreateOrderInput(client_order_id="ORDER2025", order_date=date(2025, 2, 15))

    # Execute the resolver
    result = asyncio.run(resolver(mock_info, input_obj))

    # Verify the database was called with proper arguments
    mock_db.execute_function.assert_called_once()
    call_args = mock_db.execute_function.call_args

    # Check function name
    assert call_args[0][0] == "test.create_order"

    # Check input data - date should be serialized to ISO format
    input_data = call_args[0][1]
    assert input_data["client_order_id"] == "ORDER2025"
    assert input_data["order_date"] == "2025-02-15"  # Date serialized to ISO string

    # Verify result
    assert isinstance(result, CreateOrderSuccess)
    assert result.status == "success"


def test_mutation_with_context_params_and_date():
    """Test mutation with context parameters and date field."""

    @mutation(
        function="create_order_with_context",
        schema="test",
        context_params={"tenant_id": "input_pk_organization", "user": "input_created_by"},
    )
    class CreateOrderWithContext:
        """Create order with context parameters."""

        input: CreateOrderInput
        success: CreateOrderSuccess
        failure: CreateOrderError

    definition = CreateOrderWithContext.__fraiseql_mutation__
    resolver = definition.create_resolver()

    # Create mock database
    mock_db = AsyncMock()
    mock_db.execute_function_with_context = AsyncMock(
        return_value={
            "status": "success",
            "message": "Order created",
            "data": {"id": "test-id", "client_order_id": "ORDER2025", "order_date": "2025-02-15"},
        }
    )

    # Create mock user context
    mock_user = MagicMock()
    mock_user.user_id = "user-123"

    mock_info = MagicMock()
    mock_info.context = {"db": mock_db, "tenant_id": "tenant-456", "user": mock_user}

    # Create input with date
    input_obj = CreateOrderInput(client_order_id="ORDER2025", order_date=date(2025, 2, 15))

    # Execute resolver
    result = asyncio.run(resolver(mock_info, input_obj))

    # Verify database call
    mock_db.execute_function_with_context.assert_called_once()
    call_args = mock_db.execute_function_with_context.call_args

    # Check function name
    assert call_args[0][0] == "test.create_order_with_context"

    # Check context arguments
    context_args = call_args[0][1]
    assert context_args == ["tenant-456", "user-123"]  # Extracted user_id from user object

    # Check input data with date serialized
    input_data = call_args[0][2]
    assert input_data["client_order_id"] == "ORDER2025"
    assert input_data["order_date"] == "2025-02-15"

    # Verify result
    assert isinstance(result, CreateOrderSuccess)


if __name__ == "__main__":
    test_mutation_with_date_field()
    test_mutation_with_context_params_and_date()
