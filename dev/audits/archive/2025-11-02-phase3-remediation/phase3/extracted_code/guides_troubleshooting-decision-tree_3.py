# Extracted from: docs/guides/troubleshooting-decision-tree.md
# Block number: 3
# Wrong
@type(sql_source="v_user")
class User:
    id: str  # PostgreSQL has INTEGER


# Correct
@type(sql_source="v_user")
class User:
    id: int  # Matches PostgreSQL INTEGER
