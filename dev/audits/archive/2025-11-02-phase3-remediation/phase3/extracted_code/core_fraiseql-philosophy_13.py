# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 13
from fraiseql import query

# PostgreSQL JSONB → GraphQL JSON
# No intermediate Python objects!


@query
async def users(info) -> list[User]:
    db = info.context["db"]
    # Returns JSONB directly - 10-100x faster
    return await db.find("v_user")


# With Rust transformer: 80x faster
# With APQ: 3-5x additional speedup
# With TurboRouter: 2-3x additional speedup
