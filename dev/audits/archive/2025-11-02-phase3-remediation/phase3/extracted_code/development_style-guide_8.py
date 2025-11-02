# Extracted from: docs/development/style-guide.md
# Block number: 8
from fraiseql import mutation


@mutation
def create_user(input: CreateUserInput) -> User | None:
    """Create a new user. Returns None if email already exists."""
    # Framework handles database errors
