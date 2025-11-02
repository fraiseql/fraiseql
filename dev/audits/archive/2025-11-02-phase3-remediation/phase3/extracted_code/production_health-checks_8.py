# Extracted from: docs/production/health-checks.md
# Block number: 8
health = HealthCheck()
health.add_check("pool", check_pool_stats)
