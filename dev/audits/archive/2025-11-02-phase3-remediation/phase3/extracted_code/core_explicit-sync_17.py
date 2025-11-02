# Extracted from: docs/core/explicit-sync.md
# Block number: 17
from fraiseql.ivm import setup_auto_ivm


@app.on_event("startup")
async def setup_ivm():
    """Setup IVM for all tb_/tv_ pairs."""
    recommendation = await setup_auto_ivm(db_pool, verbose=True)

    # Apply recommended IVM SQL
    async with db_pool.acquire() as conn:
        await conn.execute(recommendation.setup_sql)

    logger.info("IVM configured for fast sync")
