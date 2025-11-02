# Extracted from: docs/core/database-api.md
# Block number: 31
options = QueryOptions(
    order_by=OrderByInstructions(
        instructions=[
            OrderByInstruction(field="created_at", direction=OrderDirection.DESC),
            OrderByInstruction(field="total_amount", direction=OrderDirection.ASC),
        ]
    )
)
