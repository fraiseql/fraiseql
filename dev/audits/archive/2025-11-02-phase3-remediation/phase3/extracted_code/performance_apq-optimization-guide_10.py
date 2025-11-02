# Extracted from: docs/performance/apq-optimization-guide.md
# Block number: 10
import asyncio

from fraiseql.storage.apq_store import clear_storage


async def periodic_cleanup():
    """Clear cache every 24 hours."""
    while True:
        await asyncio.sleep(86400)  # 24 hours
        clear_storage()
        print("APQ cache cleared")


# Run in background
asyncio.create_task(periodic_cleanup())
