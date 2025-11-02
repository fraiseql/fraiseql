# Extracted from: docs/production/health-checks.md
# Block number: 4
from fraiseql.monitoring import HealthStatus

# Individual check statuses
HealthStatus.HEALTHY  # Check passed
HealthStatus.UNHEALTHY  # Check failed
HealthStatus.DEGRADED  # Partial failure (unused in individual checks)

# Overall system status (from run_checks)
# - HEALTHY: All checks passed
# - DEGRADED: One or more checks failed
