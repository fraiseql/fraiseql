# Extracted from: docs/production/health-checks.md
# Block number: 18
import os

from fraiseql.monitoring import HealthCheck, check_database


def create_health_checks() -> HealthCheck:
    """Create health checks based on environment."""
    health = HealthCheck()

    # Always check database
    health.add_check("database", check_database)

    # Production-specific checks
    if os.getenv("ENV") == "production":
        health.add_check("redis", check_redis)
        health.add_check("s3", check_s3_bucket)
        health.add_check("stripe", check_payment_gateway)

    return health


health = create_health_checks()
