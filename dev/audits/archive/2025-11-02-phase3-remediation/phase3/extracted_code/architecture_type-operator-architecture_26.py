# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 26
# src/fraiseql/sql/operator_strategies.py


class MyTypeOperatorStrategy(BaseOperatorStrategy):
    def __init__(self) -> None:
        super().__init__(
            [
                "eq",
                "neq",
                "in",
                "notin",  # Basic
                "my_special_op_1",
                "my_special_op_2",  # Custom
            ]
        )

    def can_handle(self, op: str, field_type: type | None = None) -> bool:
        if op not in self.operators:
            return False

        # Only handle specialized ops without field_type
        if field_type is None:
            return op in {"my_special_op_1", "my_special_op_2"}

        # With field_type, handle all operators
        return self._is_my_type(field_type)

    def build_sql(
        self, path_sql: SQL, op: str, val: Any, field_type: type | None = None
    ) -> Composed:
        # Implement custom SQL generation
        ...
