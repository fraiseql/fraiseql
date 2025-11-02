# Extracted from: docs/performance/cascade-invalidation.md
# Block number: 4
from fraiseql import create_app
from fraiseql.caching import setup_auto_cascade_rules

app = create_app()


@app.on_event("startup")
async def setup_cascade():
    """Setup CASCADE invalidation rules from GraphQL schema."""
    # Auto-detect and setup CASCADE rules
    await setup_auto_cascade_rules(
        cache=app.cache,
        schema=app.schema,
        verbose=True,  # Log detected rules
    )

    logger.info("CASCADE rules configured")
