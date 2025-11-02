# Extracted from: docs/production/monitoring.md
# Block number: 1

from fraiseql.monitoring import ErrorNotificationChannel, init_error_tracker

# Initialize error tracker
tracker = init_error_tracker(
    db_pool,
    environment="production",
    notification_channels=[ErrorNotificationChannel.EMAIL, ErrorNotificationChannel.SLACK],
)

# Capture exceptions
try:
    await process_payment(order_id)
except Exception as error:
    await tracker.capture_exception(
        error,
        context={
            "user_id": user.id,
            "order_id": order_id,
            "request_id": request.state.request_id,
            "operation": "process_payment",
        },
    )
    raise
