# Extracted from: docs/production/monitoring.md
# Block number: 15
import time
import uuid

from fastapi import Request
from starlette.middleware.base import BaseHTTPMiddleware


class RequestLoggingMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next):
        request_id = str(uuid.uuid4())
        request.state.request_id = request_id

        # Log request
        logger.info(
            "Request started",
            extra={
                "request_id": request_id,
                "method": request.method,
                "path": request.url.path,
                "client_ip": request.client.host if request.client else None,
                "user_agent": request.headers.get("user-agent"),
            },
        )

        start_time = time.time()

        try:
            response = await call_next(request)

            duration = (time.time() - start_time) * 1000

            # Log response
            logger.info(
                "Request completed",
                extra={
                    "request_id": request_id,
                    "status_code": response.status_code,
                    "duration_ms": duration,
                },
            )

            # Add request ID to response headers
            response.headers["X-Request-ID"] = request_id

            return response

        except Exception as e:
            duration = (time.time() - start_time) * 1000

            logger.error(
                "Request failed",
                extra={"request_id": request_id, "duration_ms": duration, "error": str(e)},
                exc_info=True,
            )
            raise


app.add_middleware(RequestLoggingMiddleware)
