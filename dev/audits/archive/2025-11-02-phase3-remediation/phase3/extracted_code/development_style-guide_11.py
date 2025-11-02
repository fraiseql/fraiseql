# Extracted from: docs/development/style-guide.md
# Block number: 11
# Old ❌
from fraiseql import field as gql_type
from fraiseql import type


@gql_type(sql_source="v_user")
class User:
    id: UUID  # Wrong type
    name: str


# New ✅


@type(sql_source="v_user")
class User:
    id: UUID  # Correct type
    name: str
