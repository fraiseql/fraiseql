# Extracted from: docs/core/queries-and-mutations.md
# Block number: 5
import logging

from fraiseql import query

logger = logging.getLogger(__name__)


@query
async def get_post(info, id: UUID) -> Post | None:
    try:
        repo = info.context["repo"]
        # Exclusive Rust pipeline handles JSON processing automatically
        return await repo.find_one_rust("v_post", "post", info, id=id)
    except Exception as e:
        logger.error(f"Failed to fetch post {id}: {e}")
        return None
