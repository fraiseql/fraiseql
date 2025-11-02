# Extracted from: docs/tutorials/beginner-path.md
# Block number: 5
from fraiseql import type, query

# 1. Define type
@type(sql_source="v_item")
class Item:
    id: UUID
    name: str

# 2. Create view (in PostgreSQL)
CREATE VIEW v_item AS
SELECT
    id,
    jsonb_build_object(
        '__typename', 'Item',
        'id', id,
        'name', name
    ) AS data
FROM tb_item;

# 3. Query
@query
def items() -> list[Item]:
    """Get all items."""
    pass  # Implementation handled by framework
