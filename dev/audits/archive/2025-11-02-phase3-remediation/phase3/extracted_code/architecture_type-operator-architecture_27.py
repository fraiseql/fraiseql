# Extracted from: docs/architecture/type-operator-architecture.md
# Block number: 27
# In OperatorRegistry.__init__()
self.strategies: list[OperatorStrategy] = [
    # ... existing strategies ...
    MyTypeOperatorStrategy(),  # Add before ComparisonOperatorStrategy
    # ... remaining strategies ...
]
