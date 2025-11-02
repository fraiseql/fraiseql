# Extracted from: docs/migration/v0-to-v1.md
# Block number: 11
from fraiseql import connection


@connection
def users(info: Info, first: int = 100) -> Connection[User]:
    return info.context.repo.find("users_view", limit=first)
