# Extracted from: docs/production/observability.md
# Block number: 2
from fraiseql import query


# Automatic capture in middleware
@app.middleware("http")
async def error_tracking_middleware(request: Request, call_next):
    try:
        response = await call_next(request)
        return response
    except Exception as error:
        # Capture with context
        await app.state.error_tracker.capture_exception(
            error,
            context={
                "request_id": request.state.request_id,
                "user_id": getattr(request.state, "user_id", None),
                "path": request.url.path,
                "method": request.method,
                "headers": dict(request.headers),
            },
        )
        raise


# Manual capture in resolvers
@query
async def process_payment(info, order_id: str) -> PaymentResult:
    try:
        result = await charge_payment(order_id)
        return result
    except PaymentError as error:
        await info.context["error_tracker"].capture_exception(
            error,
            context={
                "order_id": order_id,
                "user_id": info.context["user_id"],
                "operation": "process_payment",
            },
        )
        raise
