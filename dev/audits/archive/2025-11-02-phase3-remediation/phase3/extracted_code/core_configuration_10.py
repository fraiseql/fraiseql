# Extracted from: docs/core/configuration.md
# Block number: 10
# Strict rate limiting
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    rate_limit_enabled=True,
    rate_limit_requests_per_minute=30,
    rate_limit_requests_per_hour=500,
    rate_limit_burst_size=5,
    rate_limit_whitelist=["10.0.0.1", "10.0.0.2"],
)
