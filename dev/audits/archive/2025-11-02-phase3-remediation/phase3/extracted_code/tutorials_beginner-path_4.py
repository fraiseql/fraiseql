# Extracted from: docs/tutorials/beginner-path.md
# Block number: 4
from fraiseql import type


# WRONG: Crashes on NULL
@type
class User:
    bio: str  # What if bio is NULL?


# CORRECT: Use | None for nullable fields
@type
class User:
    bio: str | None
