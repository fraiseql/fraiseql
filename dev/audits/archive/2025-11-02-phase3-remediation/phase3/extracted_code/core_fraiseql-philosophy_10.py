# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 10
from fraiseql.monitoring import init_error_tracker

tracker = init_error_tracker(db_pool, environment="production")
await tracker.capture_exception(
    error, context={"user_id": user.id, "request_id": request_id, "operation": "create_order"}
)

# Features:
# - Automatic error fingerprinting and grouping (like Sentry)
# - Full stack trace capture
# - Request/user context preservation
# - OpenTelemetry trace correlation
# - Issue management (resolve, ignore, assign)
# - Notification triggers (Email, Slack, Webhook)
