# Extracted from: docs/performance/PERFORMANCE_GUIDE.md
# Block number: 3
# Complexity calculation
complexity = field_count + (list_size * nested_fields) + multipliers

# Example multipliers
field_multipliers = {
    "search": 5,  # Text search operations
    "aggregate": 10,  # COUNT, SUM, AVG operations
    "sort": 2,  # ORDER BY clauses
}
