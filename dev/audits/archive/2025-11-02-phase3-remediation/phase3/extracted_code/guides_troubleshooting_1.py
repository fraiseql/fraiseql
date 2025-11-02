# Extracted from: docs/guides/troubleshooting.md
# Block number: 1
from fraiseql import type
from uuid import UUID

# Check your view definition
psql your_db -c "SELECT * FROM v_note LIMIT 1;"

# Compare with Python type
@type(sql_source="v_note")
class Note:
    id: UUID        # Must match database column type
    title: str      # Must match database column type
    content: str    # Must match database column type
