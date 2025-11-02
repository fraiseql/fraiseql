# Extracted from: docs/production/observability.md
# Block number: 1

from fraiseql.monitoring import init_error_tracker


# Initialize in application startup
async def startup():
    db_pool = await create_pool(DATABASE_URL)

    tracker = init_error_tracker(
        db_pool,
        environment="production",
        auto_notify=True,  # Automatic notifications
    )

    # Store in app state for use in middleware
    app.state.error_tracker = tracker
