# Extracted from: docs/core/migrations.md
# Block number: 2
# In application startup
from fraiseql.ivm import setup_auto_ivm


@app.on_event("startup")
async def setup_ivm():
    # Analyze schema and setup IVM
    recommendation = await setup_auto_ivm(db_pool, verbose=True)

    # Apply recommended SQL
    async with db_pool.connection() as conn:
        await conn.execute(recommendation.setup_sql)
