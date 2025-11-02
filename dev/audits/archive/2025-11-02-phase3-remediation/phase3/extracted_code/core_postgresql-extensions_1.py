# Extracted from: docs/core/postgresql-extensions.md
# Block number: 1
from fraiseql.ivm import setup_auto_ivm


@app.on_event("startup")
async def setup():
    # Analyzes tv_ tables and recommends IVM strategy
    recommendation = await setup_auto_ivm(
        db_pool,
        verbose=True,  # Shows detected extensions
    )

    # Output:
    # ✓ Detected jsonb_ivm v1.1
    # IVM Analysis: 5/8 tables benefit from incremental updates (est. 25.3x speedup)
