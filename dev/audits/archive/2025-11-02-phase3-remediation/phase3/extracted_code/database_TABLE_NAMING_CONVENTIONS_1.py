# Extracted from: docs/database/TABLE_NAMING_CONVENTIONS.md
# Block number: 1
from fraiseql import type

# Don't query tb_* directly in GraphQL
# Use tv_* or v_* instead


@type(sql_source="tb_user")  # ❌ Slow - requires JOINs
class User: ...
