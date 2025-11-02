# Extracted from: docs/advanced/database-patterns.md
# Block number: 5
from fraiseql import mutation


@mutation
async def update_product(info, id: UUID, name: str, price: float) -> MutationLogResult:
    db = info.context["db"]

    # Get current state
    old_product = await db.find_one("v_product", {"id": id})

    # Update
    await db.execute("UPDATE tb_product SET name = $1, price = $2 WHERE id = $3", name, price, id)

    # Get new state
    new_product = await db.find_one("v_product", {"id": id})

    return MutationLogResult(
        status="updated",
        message=f"Product {name} updated successfully",
        op="update",
        entity="product",
        payload_before=old_product,
        payload_after=new_product,
        extra_metadata={"updated_fields": ["name", "price"]},
    )
