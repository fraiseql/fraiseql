# Extracted from: docs/production/health-checks.md
# Block number: 3
from fraiseql.monitoring import CheckResult, HealthStatus

result = CheckResult(
    name="database",
    status=HealthStatus.HEALTHY,
    message="Connection successful",
    metadata={"version": "16.3", "pool_size": 10},
)
