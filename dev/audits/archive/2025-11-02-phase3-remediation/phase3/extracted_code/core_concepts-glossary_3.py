# Extracted from: docs/core/concepts-glossary.md
# Block number: 3
@fraiseql.type(sql_source="tv_user", jsonb_column="data")
class User:
    id: UUID
    name: str
    posts: list[Post]  # Pre-composed!
