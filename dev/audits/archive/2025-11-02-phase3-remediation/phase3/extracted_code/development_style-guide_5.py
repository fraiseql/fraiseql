# Extracted from: docs/development/style-guide.md
# Block number: 5

from fraiseql import input, mutation


@input
class CreateUserInput:
    name: str
    email: str


@mutation
def create_user(input: CreateUserInput) -> User:
    """Create a new user."""
    # Implementation handled by framework
