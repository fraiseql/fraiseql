# Extracted from: docs/database/TABLE_NAMING_CONVENTIONS.md
# Block number: 2
from fraiseql import type


@type(sql_source="v_user")  # ⚠️ OK for small datasets, not for production APIs
class User:
    id: int
    first_name: str
    posts_json: list[dict]  # JSON, not transformed
