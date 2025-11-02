# Extracted from: docs/core/fraiseql-philosophy.md
# Block number: 12
from fraiseql.monitoring import HealthCheck, check_database

# Create health check
health = HealthCheck()

# Add only checks you need
health.add_check("database", check_database)

# Optionally add custom checks
health.add_check("s3", my_s3_check)


# Use in your endpoints
@app.get("/health")
async def health_endpoint():
    return await health.run_checks()
