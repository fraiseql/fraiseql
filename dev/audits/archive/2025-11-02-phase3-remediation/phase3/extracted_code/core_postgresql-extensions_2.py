# Extracted from: docs/core/postgresql-extensions.md
# Block number: 2
from fraiseql.caching import setup_auto_cascade_rules


@app.on_event("startup")
async def setup():
    # Auto-detect CASCADE rules from GraphQL schema
    await setup_auto_cascade_rules(cache=app.cache, schema=app.schema, verbose=True)

    # Output:
    # CASCADE: Analyzing GraphQL schema...
    # CASCADE: Detected relationship: User -> Post (field: posts)
    # CASCADE: Created 3 CASCADE rules
