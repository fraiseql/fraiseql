# Extracted from: docs/guides/troubleshooting-decision-tree.md
# Block number: 1
# Check type definition
@type(sql_source="v_user")
class User:
    id: int  # Required (non-nullable)
    name: str  # Required
    email: str | None  # Optional (nullable)
