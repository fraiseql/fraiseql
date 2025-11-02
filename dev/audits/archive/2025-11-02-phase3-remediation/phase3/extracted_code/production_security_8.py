# Extracted from: docs/production/security.md
# Block number: 8


class PerUserRateLimiter:
    """Rate limit per authenticated user."""

    def __init__(self, redis_client):
        self.redis = redis_client

    async def check_rate_limit(self, user_id: str, limit: int = 100, window: int = 60) -> bool:
        """Check if user is within rate limit."""
        key = f"rate_limit:user:{user_id}"
        current = await self.redis.incr(key)

        if current == 1:
            await self.redis.expire(key, window)

        if current > limit:
            return False

        return True


@app.middleware("http")
async def user_rate_limit_middleware(request: Request, call_next):
    if not hasattr(request.state, "user"):
        return await call_next(request)

    user_id = request.state.user.user_id

    limiter = PerUserRateLimiter(redis_client)
    allowed = await limiter.check_rate_limit(user_id)

    if not allowed:
        return Response(
            content=json.dumps(
                {
                    "errors": [
                        {
                            "message": "Rate limit exceeded for user",
                            "extensions": {"code": "USER_RATE_LIMIT_EXCEEDED"},
                        }
                    ]
                }
            ),
            status_code=429,
            media_type="application/json",
        )

    return await call_next(request)
