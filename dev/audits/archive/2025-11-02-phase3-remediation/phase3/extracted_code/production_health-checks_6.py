# Extracted from: docs/production/health-checks.md
# Block number: 6
health = HealthCheck()
health.add_check("database", check_database)
