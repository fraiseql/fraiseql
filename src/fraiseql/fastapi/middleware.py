"""Middleware for cache statistics."""

from starlette.middleware.base import BaseHTTPMiddleware
from fraiseql.core.query_builder import RustQueryBuilder
import logging

logger = logging.getLogger(__name__)


class CacheStatsMiddleware(BaseHTTPMiddleware):
    """Log cache statistics periodically."""

    def __init__(self, app, log_interval: int = 100):
        super().__init__(app)
        self.log_interval = log_interval
        self.request_count = 0

    async def dispatch(self, request, call_next):
        response = await call_next(request)

        # Log stats every N requests
        self.request_count += 1
        if self.request_count % self.log_interval == 0:
            try:
                stats = RustQueryBuilder.get_stats()
                logger.info(
                    "Query cache stats: "
                    f"hits={stats['hits']}, "
                    f"misses={stats['misses']}, "
                    f"hit_rate={stats['hit_rate']:.1%}, "
                    f"cached={stats['cached_plans']}/{stats['max_cached_plans']}"
                )
            except Exception as e:
                logger.warning(f"Failed to get cache stats: {e}")

        return response
