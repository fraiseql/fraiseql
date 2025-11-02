# Extracted from: docs/api-reference/database.md
# Block number: 12
async with repo.transaction() as tx:
    await tx.execute("UPDATE ...", ...)
    await tx.execute("INSERT ...", ...)
    # Automatically commits on success
    # Automatically rolls back on exception
