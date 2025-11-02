# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 6
class OperatorStrategy(Protocol):
    def can_handle(self, op: str) -> bool:
        """Check if this strategy can handle the given operator."""

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None,
    ) -> Composed:
        """Build the SQL for this operator."""
