# Extracted from: docs/performance/index.md
# Block number: 2
from fraiseql import query


@query
def get_users(info: Info) -> list[User]:
    # Automatically uses optimized view
    return info.context.repo.find("users_view")
