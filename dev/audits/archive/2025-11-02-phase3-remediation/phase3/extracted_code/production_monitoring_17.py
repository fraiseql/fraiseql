# Extracted from: docs/production/monitoring.md
# Block number: 17
from ddtrace import patch_all, tracer
from ddtrace.contrib.fastapi import patch as patch_fastapi

from fraiseql import query

# Patch all supported libraries
patch_all()

# FastAPI tracing
patch_fastapi(app)


# Custom span
@query
async def get_user(info, id: UUID) -> User:
    with tracer.trace("get_user", service="fraiseql") as span:
        span.set_tag("user.id", id)
        span.set_tag("operation", "query")

        user = await fetch_user(id)

        span.set_tag("user.found", user is not None)

        return user
