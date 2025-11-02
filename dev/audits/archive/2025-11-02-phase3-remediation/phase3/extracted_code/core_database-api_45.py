# Extracted from: docs/core/database-api.md
# Block number: 45
from psycopg.sql import SQL, Identifier, Placeholder


class CustomFilter:
    def __init__(self, field: str, value: object):
        self.field = field
        self.value = value

    def to_sql(self, view_name: str) -> Composed:
        return SQL("{} = {}").format(Identifier(self.field), Placeholder())


custom_filter = CustomFilter("status", "active")
options = QueryOptions(where=custom_filter)
