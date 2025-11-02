# Extracted from: docs/database/TABLE_NAMING_CONVENTIONS.md
# Block number: 5
from fraiseql import query, type


@type(sql_source="users", jsonb_column="data")
class User:
    id: int
    first_name: str


@query
async def user(info, id: int) -> User:
    repo = Repository(info.context["db"], info.context)
    return await repo.find_one("users", id=id)
