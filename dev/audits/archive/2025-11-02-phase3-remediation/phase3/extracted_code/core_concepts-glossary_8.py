# Extracted from: docs/core/concepts-glossary.md
# Block number: 8
import fraiseql


@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    """Simple mutation that returns the type directly."""
    db = info.context["db"]
    # Call PostgreSQL function with business logic
    result = await db.call_function("fn_create_user", input.name, input.email)
    return User(**result)
