# Extracted from: docs/production/monitoring.md
# Block number: 8
import sentry_sdk

sentry_sdk.init(
    dsn="https://key@sentry.io/project", environment="production", traces_sample_rate=0.1
)

# Capture exception
sentry_sdk.capture_exception(error)
