#!/usr/bin/env python3
"""Demo of FraiseQL's debugging and validation utilities.

This example shows how to use the new developer experience features:
- Enhanced error handling with context and hints
- Debugging utilities for query analysis
- Validation utilities for input checking
"""

import asyncio
from dataclasses import dataclass
from typing import Optional

from fraiseql import fraise_type
from fraiseql.cqrs.repository import DatabaseQuery
from fraiseql.debug import (
    debug_partial_instance,
    explain_query,
    profile_resolver,
)
from fraiseql.errors import PartialInstantiationError, WhereClauseError
from fraiseql.partial_instantiation import create_partial_instance
from fraiseql.validation import validate_where_input


@fraise_type
@dataclass
class User:
    """Example user type."""

    id: int
    name: str
    email: str
    age: Optional[int] = None
    active: bool = True


async def demo_error_handling():
    """Demonstrate enhanced error handling."""
    print("\n=== Error Handling Demo ===\n")

    # Example 1: Partial instantiation error with helpful context
    try:
        # Try to create a partial instance with invalid data
        user_data = {"id": "not-an-int", "name": "John"}
        user = create_partial_instance(User, user_data)
    except PartialInstantiationError as e:
        print("Caught PartialInstantiationError:")
        print(e)
        print()

    # Example 2: Where clause validation error
    try:
        # Invalid where clause with unknown field
        where_input = {
            "username": {"_eq": "john"},  # Should be 'name', not 'username'
            "age": {"_invalid_op": 25},  # Invalid operator
        }
        errors = validate_where_input(where_input, User, strict=True)
    except WhereClauseError as e:
        print("Caught WhereClauseError:")
        print(e)
        print()


async def demo_debugging_utilities():
    """Demonstrate debugging utilities."""
    print("\n=== Debugging Utilities Demo ===\n")

    # Example 1: Debug partial instance
    user_data = {"id": 1, "name": "Alice", "email": "alice@example.com"}
    partial_user = create_partial_instance(User, user_data)

    print("Debug output for partial instance:")
    print(debug_partial_instance(partial_user))
    print()

    # Example 2: Explain query (mock example)
    query = DatabaseQuery(
        sql="SELECT data FROM users_view WHERE data->>'active' = $1",
        params={"p1": "true"},
    )

    print("Query explanation would show:")
    print("(Note: This requires a database connection)")
    print(f"SQL: {query.sql}")
    print(f"Params: {query.params}")
    # In real usage: print(await explain_query(query))
    print()


@profile_resolver(threshold_ms=10)
async def resolve_users(parent, info, **kwargs):
    """Example resolver with profiling."""
    # Simulate some work
    await asyncio.sleep(0.02)  # 20ms delay
    return [
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"},
    ]


async def demo_validation():
    """Demonstrate validation utilities."""
    print("\n=== Validation Demo ===\n")

    # Example 1: Valid where clause
    valid_where = {
        "name": {"_like": "%john%"},
        "age": {"_gte": 18},
        "_or": [{"active": {"_eq": True}}, {"email": {"_is_null": False}}],
    }

    errors = validate_where_input(valid_where, User)
    print(f"Valid where clause errors: {errors}")

    # Example 2: Invalid where clause (non-strict mode)
    invalid_where = {
        "unknown_field": {"_eq": "value"},
        "name": {"_between": [1, 10]},  # Invalid operator
        "age": {"_like": "%test%"},  # String operator on int field
    }

    errors = validate_where_input(invalid_where, User)
    print("\nInvalid where clause errors:")
    for error in errors:
        print(f"  - {error}")


async def main():
    """Run all demos."""
    print("FraiseQL Developer Experience Demo")
    print("==================================")

    await demo_error_handling()
    await demo_debugging_utilities()
    await demo_validation()

    print("\n=== Profile Resolver Demo ===")
    print("(Check logs for profiling output)")
    # In a real app, this would be called through GraphQL
    # await resolve_users(None, mock_info)


if __name__ == "__main__":
    asyncio.run(main())
