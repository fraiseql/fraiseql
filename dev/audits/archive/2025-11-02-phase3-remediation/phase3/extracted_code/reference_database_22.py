# Extracted from: docs/reference/database.md
# Block number: 22
from fraiseql import mutation


async def transfer_funds(conn, source_id, dest_id, amount):
    # Deduct from source
    await conn.execute(
        "UPDATE accounts SET balance = balance - $1 WHERE id = $2", amount, source_id
    )

    # Add to destination
    await conn.execute("UPDATE accounts SET balance = balance + $1 WHERE id = $2", amount, dest_id)

    return True


# Execute in transaction
@mutation
async def transfer(info, input: TransferInput) -> bool:
    db = info.context["db"]
    return await db.run_in_transaction(transfer_funds, input.source_id, input.dest_id, input.amount)
