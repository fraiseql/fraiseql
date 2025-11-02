# Extracted from: docs/enterprise/ENTERPRISE.md
# Block number: 1
from fraiseql import FraiseQL
from fraiseql.monitoring import (
    HealthCheck,
    check_database,
    check_pool_stats,
    init_sentry,
    setup_metrics,
)

# Initialize error tracking
init_sentry(
    dsn=os.getenv("SENTRY_DSN"),
    environment="production",
    traces_sample_rate=0.1,
    profiles_sample_rate=0.1,
    release=f"fraiseql@{VERSION}",
)

# Configure metrics
setup_metrics(MetricsConfig(enabled=True, include_graphql=True, include_database=True))

# Set up health checks
health = HealthCheck()
health.add_check("database", check_database)
health.add_check("pool", check_pool_stats)


@app.get("/health")
async def health_check():
    result = await health.run_checks()
    return result


# Create FraiseQL app
fraiseql = FraiseQL(
    db_url=os.getenv("DATABASE_URL"),
    cqrs_read_urls=[os.getenv("READ_REPLICA_1"), os.getenv("READ_REPLICA_2")],
    production=True,
    enable_introspection=False,
    enable_playground=False,
    apq_enabled=True,
    apq_backend="postgresql",
)
