# Extracted from: docs/core/configuration.md
# Block number: 2
# Standard PostgreSQL URL
config = FraiseQLConfig(database_url="postgresql://user:pass@localhost:5432/mydb")

# Unix socket connection
config = FraiseQLConfig(database_url="postgresql://user@/var/run/postgresql:5432/mydb")

# With connection pool tuning
config = FraiseQLConfig(
    database_url="postgresql://localhost/mydb",
    database_pool_size=50,
    database_max_overflow=20,
    database_pool_timeout=60,
)
