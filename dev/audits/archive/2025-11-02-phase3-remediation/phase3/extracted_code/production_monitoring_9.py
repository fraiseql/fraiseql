# Extracted from: docs/production/monitoring.md
# Block number: 9
from fraiseql.monitoring import init_error_tracker

tracker = init_error_tracker(db_pool, environment="production")

# Capture exception (same interface)
await tracker.capture_exception(error, context={"user_id": user.id, "request_id": request_id})
