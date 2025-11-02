# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 11
class OperatorRegistry:
    def __init__(self) -> None:
        """Initialize with all available strategies in precedence order."""
        self.strategies: list[OperatorStrategy] = [
            NullOperatorStrategy(),
            DateRangeOperatorStrategy(),  # Must come BEFORE ComparisonOperatorStrategy
            LTreeOperatorStrategy(),  # Must come BEFORE ComparisonOperatorStrategy
            MacAddressOperatorStrategy(),  # Must come BEFORE ComparisonOperatorStrategy
            NetworkOperatorStrategy(),  # Must come BEFORE ComparisonOperatorStrategy
            ComparisonOperatorStrategy(),
            PatternMatchingStrategy(),
            JsonOperatorStrategy(),
            ListOperatorStrategy(),
            PathOperatorStrategy(),
        ]

    def get_strategy(self, op: str, field_type: type | None = None) -> OperatorStrategy:
        """Get the appropriate strategy for an operator."""
        # Tries specialized strategies first, then falls back to generic ones
