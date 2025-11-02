# Extracted from: docs/core/database-api.md
# Block number: 33
order_by = OrderByInstructions(
    instructions=[OrderByInstruction(field="customer_name", direction=OrderDirection.ASC)]
)
# SQL: ORDER BY json_data->>'customer_name' ASC
