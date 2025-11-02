# Extracted from: docs/core/database-api.md
# Block number: 44
class ToSQLProtocol(Protocol):
    def to_sql(self, view_name: str) -> Composed: ...
