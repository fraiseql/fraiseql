# Extracted from: docs/advanced/where_input_types.md
# Block number: 1
import fraiseql


@fraiseql.type(sql_source="users")
class User:
    id: UUID
    name: str
    email: str
    age: int
    is_active: bool
    tags: list[str]
    created_at: datetime
