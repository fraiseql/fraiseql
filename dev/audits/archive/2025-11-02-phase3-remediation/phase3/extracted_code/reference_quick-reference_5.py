# Extracted from: docs/reference/quick-reference.md
# Block number: 5
from fraiseql import input, mutation


@input
class CreateUserInput:
    name: str
    email: str


@mutation
def create_user(input: CreateUserInput) -> User:
    """Create a new user."""
    # Framework calls fn_create_user
