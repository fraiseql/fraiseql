# Extracted from: docs/tutorials/beginner-path.md
# Block number: 2
from fraiseql import mutation


# Python mutation
@mutation
def create_note(title: str, content: str) -> Note:
    """Create a new note."""
    # Implementation handled by framework
