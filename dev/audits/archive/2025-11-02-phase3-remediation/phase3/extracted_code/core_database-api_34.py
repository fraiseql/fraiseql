# Extracted from: docs/core/database-api.md
# Block number: 34
options = QueryOptions(
    aggregations={"total": "SUM"},
    order_by=OrderByInstructions(
        instructions=[OrderByInstruction(field="total", direction=OrderDirection.DESC)]
    ),
)
# SQL: SUM(total) AS total_agg ORDER BY total_agg DESC
