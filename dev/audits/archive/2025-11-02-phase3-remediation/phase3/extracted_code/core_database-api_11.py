# Extracted from: docs/core/database-api.md
# Block number: 11
query = SQL("UPDATE {} SET status = {} WHERE id = {}").format(
    Identifier("tb_orders"), Placeholder(), Placeholder()
)

await repo.execute(query, ("shipped", order_id))
