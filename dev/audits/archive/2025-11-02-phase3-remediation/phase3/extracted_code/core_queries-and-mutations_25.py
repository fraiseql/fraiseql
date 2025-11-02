# Extracted from: docs/core/queries-and-mutations.md
# Block number: 25
from fraiseql import mutation


@mutation
async def transfer_funds(info, input: TransferInput) -> TransferResult:
    db = info.context["db"]

    async with db.transaction():
        # Validate source account
        source = await db.find_one("v_account", where={"id": input.source_account_id})
        if not source or source.balance < input.amount:
            raise GraphQLError("Insufficient funds")

        # Validate destination account
        dest = await db.find_one("v_account", where={"id": input.destination_account_id})
        if not dest:
            raise GraphQLError("Destination account not found")

        # Perform transfer
        await db.update_one(
            "v_account", where={"id": source.id}, updates={"balance": source.balance - input.amount}
        )
        await db.update_one(
            "v_account", where={"id": dest.id}, updates={"balance": dest.balance + input.amount}
        )

        # Log transaction
        transfer = await db.create_one(
            "v_transfer",
            data={
                "source_account_id": input.source_account_id,
                "destination_account_id": input.destination_account_id,
                "amount": input.amount,
                "created_at": datetime.utcnow(),
            },
        )

        return TransferResult(
            transfer=transfer,
            new_source_balance=source.balance - input.amount,
            new_dest_balance=dest.balance + input.amount,
        )
