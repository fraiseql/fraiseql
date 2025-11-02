# Extracted from: docs/core/migrations.md
# Block number: 1
# In your application startup
from your_app.sync import EntitySync


@app.on_event("startup")
async def initial_sync():
    sync = EntitySync(db_pool)

    # Sync all existing data to query side
    await sync.sync_all_comments()
    logger.info("Initial comment sync complete")
