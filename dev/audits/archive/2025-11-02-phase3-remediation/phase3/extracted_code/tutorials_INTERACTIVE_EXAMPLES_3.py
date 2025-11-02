# Extracted from: docs/tutorials/INTERACTIVE_EXAMPLES.md
# Block number: 3
from fraiseql import type


@type(sql_source="v_post_with_author")
class Post:
    id: UUID
    title: str
    content: str
    author: User  # Nested User type
    created_at: datetime


# User type defined separately
@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str
