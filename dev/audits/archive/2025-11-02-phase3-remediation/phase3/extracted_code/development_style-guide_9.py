# Extracted from: docs/development/style-guide.md
# Block number: 9
from fraiseql import type


@type(sql_source="v_user")
class User:
    id: UUID  # Primary key, auto-generated
    name: str  # User's full name, required
    email: str  # Unique email address, validated
    created_at: str  # ISO 8601 timestamp, auto-set
