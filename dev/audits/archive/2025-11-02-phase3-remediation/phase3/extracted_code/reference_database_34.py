# Extracted from: docs/reference/database.md
# Block number: 34
# Use transactions for multi-step operations
async def complex_operation(conn, data):
    # All operations succeed or all fail
    await conn.execute("INSERT INTO table1 ...")
    await conn.execute("UPDATE table2 ...")
    await conn.execute("DELETE FROM table3 ...")


result = await db.run_in_transaction(complex_operation, data)
